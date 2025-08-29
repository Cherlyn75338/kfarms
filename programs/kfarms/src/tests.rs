#![allow(clippy::unwrap_used)]
use super::*;
use decimal_wad::decimal::Decimal;
use proptest::prelude::*;
use crate::farm_operations;

fn farm_state_with_defaults() -> FarmState {
    FarmState::default()
}

fn user_state_with_defaults() -> UserState {
    UserState::default()
}

// A. Unit tests: Proportional math conversions and withdraw_farm pro-rata splits
#[test]
fn test_convert_amount_to_stake_and_back_basic() {
    let total_amount = 1_000_000_u64;
    let total_stake = Decimal::from(1_000_000_u64);
    // Add 1 wei -> proportional stake should be 1
    let stake_added = stake_operations::convert_amount_to_stake(1, total_stake, total_amount);
    assert_eq!(stake_added, Decimal::from(1u64));

    // Convert back with round down
    let amount = stake_operations::convert_stake_to_amount(
        stake_added,
        total_stake + stake_added,
        total_amount + 1,
        false,
    );
    assert_eq!(amount, 1);
}

#[test]
fn test_convert_stake_to_amount_rounding() {
    // 3/10 of 1 does not fit in u64 integer -> check floor vs ceil
    let stake = Decimal::from(3u64);
    let total_stake = Decimal::from(10u64);
    let total_amount = 1u64;
    let floor_amount = stake_operations::convert_stake_to_amount(stake, total_stake, total_amount, false);
    let ceil_amount = stake_operations::convert_stake_to_amount(stake, total_stake, total_amount, true);
    assert_eq!(floor_amount, 0);
    assert_eq!(ceil_amount, 1);
}

#[test]
fn test_withdraw_farm_pro_rata_splits() {
    let mut farm = farm_state_with_defaults();
    farm.total_staked_amount = 700;
    farm.total_pending_amount = 300;

    // Withdraw 50% of total (1000 total)
    let res = stake_operations::withdraw_farm(&mut farm, 500).unwrap();
    assert_eq!(res.amount_to_withdraw, 500);
    assert!(!res.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 350);
    assert_eq!(farm.total_pending_amount, 150);

    // Withdraw more than remaining -> freeze
    let res2 = stake_operations::withdraw_farm(&mut farm, 1_000).unwrap();
    assert_eq!(res2.amount_to_withdraw, 500);
    assert!(res2.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(farm.total_pending_amount, 0);
}

// B. Precision & rounding: mul_div monotonicity and exactness where exact division possible
proptest! {
    #[test]
    fn prop_u64_mul_div_monotone(
        a in 1u64..=1_000_000_000,
        b in 0u64..=1_000_000_000,
        c in 1u64..=1_000_000_000,
    ) {
        let x = utils::math::u64_mul_div(a, b, c);
        // Increasing b should not decrease result for fixed a,c
        let b2 = b.saturating_add(1);
        let x2 = utils::math::u64_mul_div(a, b2, c);
        prop_assert!(x2 >= x || b == 1_000_000_000);
    }
}

// Exactness when divisible
#[test]
fn test_u64_mul_div_exact_when_divisible() {
    let a = 12345678u64;
    let c = 42u64;
    let b = a * c; // exact
    let res = utils::math::u64_mul_div(a, c, 1);
    assert_eq!(res, a * c);
    let res2 = utils::math::u64_mul_div(b, 1, c);
    assert_eq!(res2, a);
}

// full_decimal_mul_div fuzz for monotonicity in b (u64) holding a,c > 0
proptest! {
    #[test]
    fn prop_full_decimal_mul_div_monotone(
        a_raw in 1u64..=u32::MAX as u64,
        b in 0u64..=u64::MAX,
        c_raw in 1u64..=u32::MAX as u64
    ) {
        let a = Decimal::from(a_raw);
        let c = Decimal::from(c_raw);
        let x = utils::math::full_decimal_mul_div(a, b, c);
        let y = utils::math::full_decimal_mul_div(a, b.saturating_add(1), c);
        prop_assert!(y >= x || b == u64::MAX);
    }
}

// C. Invariants (unit-level):
#[test]
fn test_farm_total_active_scaled_matches_amount_in_non_delegated() {
    let mut farm = farm_state_with_defaults();
    farm.total_staked_amount = 1_000_000;
    // Set stake scaled to match amount
    farm.total_active_stake_scaled = Decimal::from(farm.total_staked_amount).to_scaled_val().unwrap();
    assert_eq!(farm.get_total_active_stake_decimal(), Decimal::from(1_000_000u64));
}

#[test]
fn test_rewards_issued_unclaimed_monotone() {
    let mut farm = farm_state_with_defaults();
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1;
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.reward_infos[0].rewards_available = 10_000;
    farm.reward_infos[0].reward_schedule_curve.set_constant(10);
    farm.reward_infos[0].rewards_per_second_decimals = 0;
    farm.reward_infos[0].last_issuance_ts = 0;

    farm_operations::refresh_global_reward(&mut farm, None, 1, 0).unwrap();
    let a = farm.reward_infos[0].rewards_issued_unclaimed;
    farm_operations::refresh_global_reward(&mut farm, None, 2, 0).unwrap();
    let b = farm.reward_infos[0].rewards_issued_unclaimed;
    assert!(b >= a);
}

// D. Locking/penalty
#[test]
fn test_apply_early_withdrawal_penalty_with_expiry_before_start_zero_penalty() {
    // WithExpiry path uses start and now < start => zero penalty
    let locking_duration = 1_000u64;
    let start = 10_000u64;
    let now = start - 1;
    let bps = 2_000u64; // 20%
    let amount = 1_000u64;
    let (post, pen) = utils::withdrawal_penalty::apply_early_withdrawal_penalty(locking_duration, start, now, bps, amount).unwrap();
    assert_eq!(pen, 0);
    assert_eq!(post, amount);
}

#[test]
fn test_apply_early_withdrawal_penalty_continuous_midway() {
    // midway => 50% of bps
    let locking_duration = 100u64;
    let start = 1000u64;
    let now = 1050u64; // midway
    let bps = 1_000u64; // 10%
    let amount = 1_000u64;
    let (post, pen) = utils::withdrawal_penalty::apply_early_withdrawal_penalty(locking_duration, start, now, bps, amount).unwrap();
    // Half of 10% = 5%
    assert_eq!(pen, 50);
    assert_eq!(post, 950);
}

#[test]
fn test_unstake_flow_with_continuous_penalty_and_cooldown() {
    let mut farm = farm_state_with_defaults();
    farm.total_staked_amount = 1_000;
    farm.set_total_active_stake_decimal(Decimal::from(1_000u64));
    farm.withdrawal_cooldown_period = 10; // seconds
    farm.locking_mode = LockingMode::Continuous as u64;
    farm.locking_duration = 100;
    farm.locking_early_withdrawal_penalty_bps = 1_000; // 10%

    let mut user = user_state_with_defaults();
    user.active_stake_scaled = Decimal::from(1_000u64).to_scaled_val().unwrap();
    user.last_stake_ts = 1_000;

    // Unstake 500 stake shares at t=1_010 (10s after last stake) => 90% remaining of duration => 9% penalty
    let (net_unstaked, pending_stake_added, penalty) = stake_operations::unstake(
        &mut user,
        &mut farm,
        Decimal::from(500u64),
        1_010,
    ).unwrap();
    assert!(net_unstaked > 0);
    assert!(pending_stake_added > Decimal::zero());
    assert!(penalty > 0);
    // cooldown timestamp set on handler path; here we only test math layer
}

// G. Oracle price handling: unit-level check using can_accept_deposit math branch
#[test]
fn test_oracle_paths_require_price_when_enabled() {
    let mut farm = farm_state_with_defaults();
    farm.scope_oracle_price_id = 0; // enable oracle usage
    // can_accept_deposit requires Some(price)
    let err1 = farm.can_accept_deposit(0, None, 0).unwrap_err();
    let _ = err1;

    // refresh_global_reward requires Some(price)
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1;
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.reward_infos[0].rewards_available = 1_000;
    farm.reward_infos[0].reward_schedule_curve.set_constant(1);
    farm.reward_infos[0].rewards_per_second_decimals = 0;
    farm.reward_infos[0].last_issuance_ts = 0;
    let r = farm_operations::refresh_global_reward(&mut farm, None, 1, 0);
    assert!(r.is_err());
}

// Freeze controls: withdrawing entire vault freezes farm
#[test]
fn test_withdraw_farm_freeze_flag() {
    let mut farm = farm_state_with_defaults();
    farm.total_staked_amount = 10;
    farm.total_pending_amount = 5;
    let total = farm.total_staked_amount + farm.total_pending_amount;
    let r = stake_operations::withdraw_farm(&mut farm, total).unwrap();
    assert!(r.farm_to_freeze);
    assert_eq!(r.amount_to_withdraw, total);
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(farm.total_pending_amount, 0);
}

// B/C combination: reward issuance and user refresh rounding behavior
#[test]
fn test_reward_issuance_and_user_refresh_flooring_bias_simple() {
    // Setup farm with 1 reward, some stake, and RPS constant via curve
    let mut farm = farm_state_with_defaults();
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1_000; // non-delegated path
    farm.set_total_active_stake_decimal(Decimal::from(1_000u64));
    farm.reward_infos[0].rewards_available = 1_000_000;
    farm.reward_infos[0].reward_schedule_curve.set_constant(100); // 100 units/sec
    farm.reward_infos[0].rewards_per_second_decimals = 0; // no extra decimals
    farm.reward_infos[0].last_issuance_ts = 0;

    let mut user = user_state_with_defaults();
    user.active_stake_scaled = Decimal::from(1_000u64).to_scaled_val().unwrap();
    user.last_claim_ts[0] = 0;

    // After 1 second, 100 rewards issued; user gets all
    farm_operations::refresh_global_reward(&mut farm, None, 1, 0).unwrap();
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0], 100);

    // Harvest with 0% fee
    let mut global = GlobalConfig::default();
    global.treasury_fee_bps = 0;
    let res = farm_operations::harvest(&mut farm, &mut user, &global, None, 0, 2).unwrap();
    // Harvest includes rewards issued up to ts=2 (another 100), so total 200
    assert_eq!(res.reward_user, 200);
    assert_eq!(res.reward_treasury, 0);
    assert_eq!(user.rewards_issued_unclaimed[0], 0);
}

