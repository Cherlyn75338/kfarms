#![cfg(test)]

use crate::stake_operations::{convert_amount_to_stake, convert_stake_to_amount};
use crate::utils::math::{full_decimal_mul_div, u128_mul_div, u64_mul_div};
use decimal_wad::decimal::Decimal;

#[test]
fn test_u64_mul_div_edges() {
    assert_eq!(u64_mul_div(0, u64::MAX, 1), 0);
    assert_eq!(u64_mul_div(u64::MAX, 0, 1), 0);
    assert_eq!(u64_mul_div(u64::MAX, 1, u64::MAX), 1);
}

#[test]
fn test_u128_mul_div_edges() {
    assert_eq!(u128_mul_div(0, u128::MAX, 1).unwrap(), 0);
    assert_eq!(u128_mul_div(u128::MAX, 0, 1).unwrap(), 0);
    assert_eq!(u128_mul_div(u128::MAX, 1, u128::MAX).unwrap(), 1);
}

#[test]
fn test_full_decimal_mul_div_precision() {
    let a = Decimal::from_scaled_val(1_000_000_000_000_000_000u128); // 1.0
    let c = Decimal::from_scaled_val(2_000_000_000_000_000_000u128); // 2.0
    let res = full_decimal_mul_div(a, 3, c); // 1 * 3 / 2 = 1.5
    let as_scaled: u128 = res.to_scaled_val().unwrap();
    assert_eq!(as_scaled, 1_500_000_000_000_000_000u128);
}

#[test]
fn test_convert_roundtrip_conservation() {
    // Start with some totals
    let total_amount: u64 = 1_000_000_000;
    let total_stake: Decimal = Decimal::from_scaled_val(5_000_000_000_000_000_000u128); // 5e18

    // Take a series of amounts and convert to stake and back
    for amount in [0u64, 1, 10, 1234, 1_000_000_000u64] {
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        let back_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let back_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        assert!(back_floor <= amount && amount <= back_ceil);
    }
}

#[test]
fn test_penalty_math_zero_duration_zero_penalty() {
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    // Zero duration acts as matured immediately => zero penalty
    let (amt, pen) = apply_early_withdrawal_penalty(0, 100, 100, 1_000, 10_000).unwrap();
    assert_eq!(amt, 10_000);
    assert_eq!(pen, 0);
}

#[test]
fn test_penalty_math_before_and_after() {
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    // Before start => zero penalty
    let (amt, pen) = apply_early_withdrawal_penalty(1000, 1_000, 900, 1_000, 10_000).unwrap();
    assert_eq!(amt, 10_000);
    assert_eq!(pen, 0);
    // After maturity => zero penalty
    let (amt2, pen2) = apply_early_withdrawal_penalty(1000, 1_000, 2_001, 1_000, 10_000).unwrap();
    assert_eq!(amt2, 10_000);
    assert_eq!(pen2, 0);
}

#[test]
fn test_deposit_cap_no_oracle() {
    use crate::state::FarmState;
    let mut farm = FarmState::default();
    farm.deposit_cap_amount = 1_000;
    farm.total_staked_amount = 900;
    farm.scope_oracle_price_id = u64::MAX; // disable oracle
    // Accept within cap
    assert!(farm.can_accept_deposit(100, None, 0).unwrap());
    // Reject over cap
    assert!(!farm.can_accept_deposit(200, None, 0).unwrap());
}

#[test]
fn test_reward_schedule_cumulative_no_overflow() {
    use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve};
    // Simple constant RPS
    let mut curve = RewardScheduleCurve::from_constant(10);
    // Extend with a later point to test segmentation
    curve.set_point(1, RewardPerTimeUnitPoint::new(1_000, 20));
    // From ts 0 to 2_000
    let amt = curve
        .get_cumulative_amount_issued_since_last_ts(0, 2_000)
        .unwrap();
    // First 1000 * 10 + next 1000 * 20 = 10_000 + 20_000 = 30_000
    assert_eq!(amt, 30_000);
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn stake_amount_roundtrip_random(total_amount in 1u64..=1_000_000u64, amount in 0u64..=1_000_000u64,
        total_stake_scaled in 1u128..=1_000_000_000_000_000_000u128) {
        let total_stake = Decimal::from_scaled_val(total_stake_scaled);
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        let back_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let back_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        prop_assert!(back_floor <= amount && amount <= back_ceil);
    }
}

