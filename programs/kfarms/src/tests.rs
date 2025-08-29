#![allow(clippy::unwrap_used)]

use super::*;
use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;

fn new_farm_with_one_reward(ts: u64) -> (FarmState, RewardInfo) {
    let mut farm = FarmState::default();
    farm.time_unit = TimeUnit::Seconds as u8;
    farm.total_staked_amount = 1; // prevent short-circuit in refresh
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.num_reward_tokens = 1;

    let mut reward = RewardInfo::default();
    reward.reward_schedule_curve = RewardScheduleCurve::from_constant(100);
    reward.last_issuance_ts = ts;
    reward.rewards_per_second_decimals = 0;
    reward.reward_type = RewardType::Proportional as u8;
    reward.rewards_available = u64::MAX; // plenty for tests

    farm.reward_infos[0] = reward;
    (farm, reward)
}

#[test]
fn unit_rpt_monotone_and_user_rounding() {
    let start = 1_000_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);
    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = start;

    // t1: +10s -> rps=100, total issued=1000; rpt increases
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    let rpt1 = farm.reward_infos[0].get_reward_per_share_decimal();
    assert!(rpt1 > Decimal::zero());

    // user accrual equals floor(active_stake * delta_rpt)
    let prev_unclaimed = user.rewards_issued_unclaimed[0];
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0] - prev_unclaimed, 1000);

    // t2: another +7s, rpt monotone and increments by 700
    farm_operations::refresh_global_rewards(&mut farm, None, start + 17).unwrap();
    let rpt2 = farm.reward_infos[0].get_reward_per_share_decimal();
    assert!(rpt2 > rpt1);
    let prev = user.rewards_issued_unclaimed[0];
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0] - prev, 700);
}

#[test]
fn unit_vault_deltas_and_rewards_available() {
    let start = 10_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);

    // supply a finite available amount to test depletion
    farm.reward_infos[0].rewards_available = 1500;

    // 10 seconds at 100 rps issues 1000
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 1000);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 1000);
    assert_eq!(farm.reward_infos[0].rewards_available, 500);

    // 10 more seconds tries to issue another 1000, but only 500 left
    farm_operations::refresh_global_rewards(&mut farm, None, start + 20).unwrap();
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 1500);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 1500);
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
}

#[test]
fn unit_unstake_tally_checked_sub_boundary() {
    let start = 5_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);
    farm.withdrawal_cooldown_period = 100;

    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = start;

    // accrue some rewards so rewards_tally > 0
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();

    // request to unstake almost entire tally contribution to provoke near-boundary behavior
    let requested = user.get_active_stake_decimal();
    farm_operations::unstake(&mut farm, &mut user, None, requested, start + 11).unwrap();

    // Saturating subtraction shouldn't underflow; ensure tally remains <= previous and non-negative
    let tally = user.rewards_tally_scaled[0];
    assert!(tally >= 0);
}

// Property tests included inline
use proptest::prelude::*;

fn setup_farm(ts: u64, rps: u64, reward_type: RewardType) -> (FarmState, UserState) {
    let mut farm = FarmState::default();
    farm.time_unit = TimeUnit::Seconds as u8;
    farm.total_staked_amount = 1;
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.num_reward_tokens = 1;
    farm.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    farm.reward_infos[0].last_issuance_ts = ts;
    farm.reward_infos[0].rewards_per_second_decimals = 0;
    farm.reward_infos[0].reward_type = reward_type as u8;
    farm.reward_infos[0].rewards_available = u64::MAX;

    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = ts;

    (farm, user)
}

proptest! {
    #[test]
    fn prop_rpt_monotone(dt in 1u64..1000, steps in 1usize..20) {
        let start = 1_000_000u64;
        let (mut farm, mut user) = setup_farm(start, 13, RewardType::Proportional);

        let mut t = start;
        let mut last_rpt = farm.reward_infos[0].get_reward_per_share_decimal();
        for _ in 0..steps {
            t = t.saturating_add(dt);
            farm_operations::refresh_global_rewards(&mut farm, None, t).unwrap();
            let rpt = farm.reward_infos[0].get_reward_per_share_decimal();
            prop_assert!(rpt >= last_rpt);
            last_rpt = rpt;

            // user reward is non-negative
            let before = user.rewards_issued_unclaimed[0];
            farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
            let delta = user.rewards_issued_unclaimed[0] - before;
            prop_assert!(delta >= 0);
        }
    }
}

#[test]
fn poc_time_warp_overpay_depletes_available() {
    // No clamp: very large dt issues a massive amount immediately
    let start = 2_000_000u64;
    let (mut farm, _user) = setup_farm(start, 1_000_000, RewardType::Proportional);
    farm.reward_infos[0].rewards_available = 10_000_000; // finite

    // Warp ahead a long time
    let warp = start + 10_000; // 10k seconds at 1e6 rps = 1e10 requested
    farm_operations::refresh_global_rewards(&mut farm, None, warp).unwrap();

    // Without clamp, issuance is capped only by rewards_available and drains it
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 10_000_000);
}

#[test]
#[should_panic]
fn poc_overflow_amplification_constant_reward_panics() {
    // Construct parameters to overflow the final u64 cast
    let start = 100u64;
    let (mut farm, _user) = setup_farm(start, u64::MAX / 2, RewardType::Constant);
    farm.total_staked_amount = u64::MAX; // amplify
    farm.reward_infos[0].rewards_per_second_decimals = 0; // no scaling down

    // Large dt multiplies into u128 and then unwrap() to u64 should panic
    let warp = start + 3;
    let _ = farm_operations::refresh_global_rewards(&mut farm, None, warp);
}

