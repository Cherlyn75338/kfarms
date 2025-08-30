#![cfg(test)]

use crate::farm_operations::refresh_global_reward;
use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve, RewardType};
use crate::{FarmState, RewardInfo};

fn build_farm_proportional(
    rewards_available: u64,
    last_issuance_ts: u64,
    rps: u64,
) -> FarmState {
    let mut farm_state = FarmState::default();
    // Ensure refresh path executes
    farm_state.total_active_stake_scaled = 1;
    farm_state.num_reward_tokens = 1;

    let mut reward_info = RewardInfo::default();
    reward_info.rewards_available = rewards_available;
    reward_info.last_issuance_ts = last_issuance_ts;
    reward_info.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    reward_info.reward_type = RewardType::Proportional as u8;
    reward_info.rewards_per_second_decimals = 0;

    farm_state.reward_infos[0] = reward_info;
    farm_state
}

#[test]
fn schedule_overissuance_drains_vault_quickly() {
    // Configure large RPS and long dt that do not overflow u64, but far exceed vault size.
    // rps = 1e12, dt = 1_000_000 => cumulative_amt = 1e18 (<= u64::MAX)
    let ts_now: u64 = 2_000_000;
    let rewards_available: u64 = 1_000_000; // small vault
    let rps: u64 = 1_000_000_000_000; // 1e12 per time unit

    let mut farm = build_farm_proportional(rewards_available, ts_now - 1_000_000, rps);
    // No oracle scaling
    farm.scope_oracle_price_id = u64::MAX;

    refresh_global_reward(&mut farm, None, ts_now, 0).unwrap();

    // The computed issuance (1e18) is clamped by rewards_available, draining the vault in one refresh.
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, rewards_available);
}

#[test]
#[should_panic]
fn schedule_accumulation_overflow_panics_on_u64_mul() {
    // period_amount = reward_per_time_unit * (end_ts - start_ts)
    // rps = u64::MAX and dt = 2 => multiplication overflows
    let curve = RewardScheduleCurve::from_points(&[
        RewardPerTimeUnitPoint { ts_start: 0, reward_per_time_unit: u64::MAX },
    ])
    .unwrap();

    let _ = curve
        .get_cumulative_amount_issued_since_last_ts(0, 2)
        .unwrap();
}

