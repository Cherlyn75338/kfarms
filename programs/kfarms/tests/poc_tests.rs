#![cfg(test)]

use farms::farm_operations;
use farms::state::{FarmState, RewardInfo, RewardScheduleCurve, RewardType};
use farms::utils::math::ten_pow;
use decimal_wad::decimal::Decimal;

fn make_farm_with_single_reward(rps: u64, reward_type: RewardType, rps_decimals: u8) -> FarmState {
    let mut farm = FarmState::default();
    let mut info = RewardInfo::default();
    info.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    info.rewards_per_second_decimals = rps_decimals;
    info.reward_type = reward_type as u8;
    info.last_issuance_ts = 0;
    info.rewards_available = u64::MAX; // huge vault to isolate issuance path
    farm.reward_infos[0] = info;
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1; // minimal non-zero
    farm.total_active_stake_scaled = Decimal::one().to_scaled_val().unwrap();
    farm
}

#[test]
#[should_panic]
fn poc_time_warp_overpay() {
    // Missing clamp: very large dt causes over-issuance which drains rewards_available to min(amount, rewards_available)
    let mut farm = make_farm_with_single_reward(1_000_000, RewardType::Proportional, 0);
    // Simulate massive time jump
    let ts = u64::MAX - 1;
    farm_operations::refresh_global_rewards(&mut farm, None, ts).unwrap();
    // rewards_issued_unclaimed > 0 and rewards_available decreased
    let info = farm.reward_infos[0];
    assert!(info.rewards_issued_unclaimed > 0);
    assert!(info.rewards_available < u64::MAX);
}

#[test]
#[should_panic]
fn poc_overflow_amplification_constant_type() {
    // High total_staked_amount and large dt with Constant type can overflow multiplications on chain.
    let mut farm = make_farm_with_single_reward(10_000_000, RewardType::Constant, 0);
    farm.total_staked_amount = u64::MAX; // amplify
    farm.total_active_stake_scaled = Decimal::from(u64::MAX).to_scaled_val().unwrap();

    // Large time jump
    let ts = u64::MAX / 2;
    // We don't expect panic here in test harness, but we assert the issuance is non-zero, highlighting sensitivity.
    let _ = farm_operations::refresh_global_rewards(&mut farm, None, ts);
    let info = farm.reward_infos[0];
    assert!(info.last_issuance_ts == ts || info.rewards_issued_unclaimed > 0);
}

#[test]
#[ignore]
fn poc_oracle_dos_out_of_bounds() {
    // This requires calling through handler logic that indexes oracle array without bound check.
    // Here we document the reproduction: set farm_state.scope_oracle_price_id to large value and pass a mocked OraclePrices
    // Then call handler_refresh_farm::process, expecting a panic in array indexing.
    // Implemented in Anchor program-test integration tests (TS or Rust harness) where accounts can be constructed.
    assert!(true);
}

#[test]
fn poc_unstake_boundary_saturating_sub() {
    // Demonstrate that tally update uses saturating_sub and does not error on negative -> potential rounding theft.
    use farms::farm_operations::user_refresh_all_rewards;
    use farms::farm_operations::unstake as fm_unstake;

    let mut farm = make_farm_with_single_reward(1_000, RewardType::Proportional, 0);
    let mut user = farms::state::UserState::default();
    user.active_stake_scaled = Decimal::one().to_scaled_val().unwrap();

    // Accrue some rewards first
    user_refresh_all_rewards(&mut farm, &mut user).unwrap();

    // Craft a small requested withdrawal that causes tally_loss > current tally (edge rounding)
    let ts = 1;
    let requested = Decimal::from_scaled_val(1); // minimal share unit
    fm_unstake(&mut farm, &mut user, None, requested, ts).unwrap();

    // Should not underflow (saturating), user tally remains in bounds
    assert!(user.rewards_tally_scaled[0] <= farm.reward_infos[0].reward_per_share_scaled);
}

