#![cfg(test)]

use farms::farm_operations::refresh_global_rewards;
use farms::state::{FarmState, RewardInfo, RewardScheduleCurve, RewardType};
use decimal_wad::decimal::Decimal;

fn farm_with(rps: u64, reward_type: RewardType, decimals: u8) -> FarmState {
    let mut farm = FarmState::default();
    let mut info = RewardInfo::default();
    info.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    info.rewards_per_second_decimals = decimals;
    info.reward_type = reward_type as u8;
    info.last_issuance_ts = 0;
    info.rewards_available = u64::MAX;
    farm.reward_infos[0] = info;
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1;
    farm.total_active_stake_scaled = Decimal::one().to_scaled_val().unwrap();
    farm
}

#[test]
#[should_panic]
fn dt_clamp_missing_allows_huge_issuance() {
    let mut farm = farm_with(1_000_000, RewardType::Proportional, 0);
    let ts = u64::MAX - 10;
    let before = farm.reward_infos[0].rewards_available;
    refresh_global_rewards(&mut farm, None, ts).unwrap();
    let after = farm.reward_infos[0].rewards_available;
    assert!(after < before);
}

#[test]
#[should_panic]
fn overflow_guard_missing_in_mul_paths() {
    // This test documents sensitivity; concrete overflow may panic depending on rust checks
    let mut farm = farm_with(10_000_000, RewardType::Constant, 0);
    farm.total_staked_amount = u64::MAX;
    farm.total_active_stake_scaled = Decimal::from(u64::MAX).to_scaled_val().unwrap();
    let ts = u64::MAX/2;
    let _ = refresh_global_rewards(&mut farm, None, ts);
    assert!(farm.reward_infos[0].last_issuance_ts == ts || farm.reward_infos[0].rewards_issued_unclaimed > 0);
}

