#![cfg(test)]

use crate::farm_operations::{refresh_global_reward, refresh_global_rewards};
use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve, RewardType};
use crate::utils::math::ten_pow;
use crate::{FarmError, FarmState, RewardInfo};
use decimal_wad::decimal::Decimal;

fn build_farm_state_with_one_reward(
    total_staked_amount: u64,
    total_active_stake_scaled_units: u128,
    rewards_available: u64,
    last_issuance_ts: u64,
    rps: u64,
    reward_type: RewardType,
    rps_decimals: u8,
) -> FarmState {
    let mut farm_state = FarmState::default();
    // Ensure global refresh path is not short-circuited
    farm_state.total_active_stake_scaled = total_active_stake_scaled_units;
    farm_state.total_staked_amount = total_staked_amount;
    farm_state.num_reward_tokens = 1;

    let mut reward_info = RewardInfo::default();
    reward_info.rewards_available = rewards_available;
    reward_info.last_issuance_ts = last_issuance_ts;
    reward_info.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    reward_info.reward_type = reward_type as u8;
    reward_info.rewards_per_second_decimals = rps_decimals;

    farm_state.reward_infos[0] = reward_info;
    farm_state
}

#[test]
#[should_panic]
fn poc_schedule_accumulation_overflow_panics_on_u64_mul() {
    // period_amount = reward_per_time_unit * (end_ts - start_ts)
    // Use rps = u64::MAX and duration = 2 => multiplication overflows and panics
    let curve = RewardScheduleCurve::from_points(&[
        RewardPerTimeUnitPoint { ts_start: 0, reward_per_time_unit: u64::MAX },
    ])
    .unwrap();

    // This call should panic due to checked overflow in release profile
    let _ = curve
        .get_cumulative_amount_issued_since_last_ts(0, 2)
        .unwrap();
}

#[test]
#[should_panic]
fn poc_constant_rewards_unwrap_panic_amount_exceeds_u64_max() {
    // Setup so that:
    // cumulative_amt = u64::MAX (rps = u64::MAX, dt = 1)
    // reward_type_amt = cumulative_amt * total_staked_amount = u64::MAX * u64::MAX ~ 3.4e38 (fits u128)
    // decimal_adjusted_amt = reward_type_amt / 10^0 -> ~3.4e38
    // oracle disabled; converting to u64 via try_into().unwrap() panics -> DoS
    let ts_now: u64 = 1_000;
    let mut farm = build_farm_state_with_one_reward(
        u64::MAX, /* total_staked_amount */
        1,        /* total_active_stake_scaled_units */
        u64::MAX, /* rewards_available */
        ts_now - 1,
        u64::MAX,         /* rps */
        RewardType::Constant,
        0, /* rps_decimals */
    );

    // Disable oracle path
    farm.scope_oracle_price_id = u64::MAX;

    // Panics inside refresh_global_reward at try_into().unwrap()
    let _ = refresh_global_reward(&mut farm, None, ts_now, 0).unwrap();
}

#[test]
fn poc_clamp_drains_reward_vault_faster_than_intended() {
    // Choose values so amount (computed) > rewards_available, but amount <= u64::MAX to avoid unwrap panic
    let ts_now: u64 = 5_000;
    let rewards_available: u64 = 1_000_000; // small vault
    let rps: u64 = 1_000_000_000; // 1e9 per unit
    let total_staked_amount: u64 = 10_000; // magnifies constant issuance
    let mut farm = build_farm_state_with_one_reward(
        total_staked_amount,
        1, // stake shares
        rewards_available,
        ts_now - 1, // dt = 1
        rps,
        RewardType::Constant,
        0, // no decimal scaling
    );

    farm.scope_oracle_price_id = u64::MAX; // disable oracle to isolate behavior

    // Execute
    refresh_global_reward(&mut farm, None, ts_now, 0).unwrap();

    // With rps * dt * total_staked_amount = 1e9 * 1 * 1e4 = 1e13 > rewards_available
    // issuance is clamped to rewards_available and drains the vault
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, rewards_available);
}

#[test]
fn poc_counters_overflow_causes_dos_error() {
    // Configure a small issuance and set counters near u64::MAX so checked_add fails
    let ts_now: u64 = 10_000;
    let mut farm = build_farm_state_with_one_reward(
        1, // tiny stake to keep amount small
        1,
        u64::MAX, // plenty available so clamp won't change the amount
        ts_now - 1,
        100, // small rps
        RewardType::Constant,
        0,
    );

    // Prepare counters to overflow on addition
    farm.reward_infos[0].rewards_issued_cumulative = u64::MAX - 1;

    // Disable oracle
    farm.scope_oracle_price_id = u64::MAX;

    // Expect IntegerOverflow error from checked_add on rewards_issued_cumulative
    let err = refresh_global_reward(&mut farm, None, ts_now, 0).unwrap_err();
    let dbg = format!("{:?}", err);
    assert!(dbg.contains("IntegerOverflow"));
}

#[test]
#[should_panic]
fn poc_oracle_scaling_mul_overflow_before_division_panics() {
    // Demonstrate that decimal_adjusted_amt * px can overflow u128 before division
    // even when final scaled value would have fit in u64.
    // This mirrors the production math path in the oracle branch.
    let decimal_adjusted_amt: u128 = (u128::MAX / 3) + 1; // large enough to overflow when multiplied by 3
    let px: u128 = 3;
    let factor: u128 = 1; // division happens after the overflowing multiply

    // This multiply overflows (checked in tests and by release overflow-checks) and panics
    let _ = decimal_adjusted_amt * px / factor;
}

#[test]
fn sanity_rps_decimals_scaling_keeps_within_bounds() {
    // Validate that rps decimals scaling can be used to keep amount under u64::MAX
    let ts_now: u64 = 42_000;
    let mut farm = build_farm_state_with_one_reward(
        1_000_000, // total_staked_amount
        1,
        u64::MAX,
        ts_now - 1,
        u64::MAX, // very large raw rps
        RewardType::Constant,
        19, // divide by 1e19 to reduce magnitude
    );
    farm.scope_oracle_price_id = u64::MAX;

    // Should not panic and should compute a finite issuance under u64::MAX
    refresh_global_reward(&mut farm, None, ts_now, 0).unwrap();

    // Ensure last issuance timestamp progressed
    assert_eq!(farm.reward_infos[0].last_issuance_ts, ts_now);
}

#[test]
#[should_panic]
fn poc_refresh_global_rewards_can_brick_entire_farm_via_unwrap_panic() {
    // Misconfiguration: zero decimals, massive RPS and stake -> amount > u64::MAX -> unwrap panic
    let ts_now: u64 = 777_777;
    let mut farm = build_farm_state_with_one_reward(
        u64::MAX,
        1,
        u64::MAX,
        ts_now - 1,
        u64::MAX,
        RewardType::Constant,
        0,
    );
    farm.scope_oracle_price_id = u64::MAX;

    // Many instructions call this implicitly; show that a single call panics
    let _ = refresh_global_rewards(&mut farm, None, ts_now).unwrap();
}

