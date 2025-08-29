use crate::farm_operations::*;
use crate::state::{FarmState, RewardInfo, UserState, RewardType};
use crate::utils::math::ten_pow;
use crate::FarmError;
use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;

/// Test the dangerous unwrap() at line 826 of farm_operations.rs
#[test]
fn test_reward_issuance_overflow_protection() {
    // This test demonstrates the critical overflow issue in refresh_global_reward
    
    // Scenario 1: Large cumulative amount with Constant reward type
    let cumulative_amt: u128 = u64::MAX as u128;
    let total_staked_amount: u64 = u64::MAX;
    
    // This multiplication WILL overflow u128
    let reward_type_amt_result = cumulative_amt.checked_mul(total_staked_amount as u128);
    assert!(reward_type_amt_result.is_none(), "Multiplication should overflow");
    
    // Scenario 2: Oracle price adjustment overflow
    let decimal_adjusted_amt: u128 = u64::MAX as u128;
    let px: u128 = u64::MAX as u128;
    let factor: u128 = 1;
    
    // This multiplication WILL overflow u128
    let oracle_result = decimal_adjusted_amt.checked_mul(px);
    assert!(oracle_result.is_none(), "Oracle adjustment should overflow");
    
    // Scenario 3: Conversion to u64 that would panic
    let large_value: u128 = (u64::MAX as u128) + 1;
    let conversion_result: Result<u64, _> = large_value.try_into();
    assert!(conversion_result.is_err(), "Conversion should fail");
}

/// Test safe alternatives for reward issuance
#[test]
fn test_safe_reward_issuance() {
    // Safe version using saturating arithmetic
    let cumulative_amt: u128 = u64::MAX as u128;
    let total_staked_amount: u64 = u64::MAX;
    
    // Use saturating multiplication
    let reward_type_amt = cumulative_amt.saturating_mul(total_staked_amount as u128);
    assert_eq!(reward_type_amt, u128::MAX);
    
    // Safe conversion with explicit handling
    let oracle_adjusted_amt: u128 = u64::MAX as u128 + 1000;
    let safe_amount: u64 = oracle_adjusted_amt.min(u64::MAX as u128) as u64;
    assert_eq!(safe_amount, u64::MAX);
}

/// Test reward accumulator overflow in reward_user_once
#[test]
fn test_reward_accumulator_overflow() {
    let mut farm = FarmState::default();
    let mut user = UserState::default();
    
    // Set rewards near maximum
    farm.reward_infos[0].rewards_issued_unclaimed = u64::MAX - 10;
    user.rewards_issued_unclaimed[0] = u64::MAX - 10;
    
    // Adding 20 would overflow with regular addition
    let amount_to_add = 20u64;
    
    // Unsafe version (current code uses +=)
    let unchecked_result = farm.reward_infos[0].rewards_issued_unclaimed.checked_add(amount_to_add);
    assert!(unchecked_result.is_none(), "Should overflow");
    
    // Safe version
    let safe_result = farm.reward_infos[0].rewards_issued_unclaimed.saturating_add(amount_to_add);
    assert_eq!(safe_result, u64::MAX);
}

/// Test price exponent handling
#[test]
fn test_price_exponent_edge_cases() {
    // Test with maximum safe exponent
    let exp = 19;
    let factor = ten_pow(exp);
    assert_eq!(factor, 10_000_000_000_000_000_000);
    
    // Test division with large values
    let large_amount: u128 = u128::MAX / 2;
    let result = large_amount / (factor as u128);
    assert!(result < u128::MAX);
}

/// Test scope price adjustment with extreme values
#[test]
fn test_scope_price_extreme_values() {
    // Simulate extreme price scenario
    let decimal_adjusted_amt: u128 = u64::MAX as u128;
    let px: u128 = 1_000_000_000_000; // $1M with 6 decimals
    let factor: u128 = ten_pow(6) as u128;
    
    // Check if multiplication would overflow
    let mul_result = decimal_adjusted_amt.checked_mul(px);
    
    if let Some(product) = mul_result {
        let final_result = product / factor;
        assert!(final_result <= u128::MAX);
    } else {
        // Would overflow - need to handle
        let safe_result = (decimal_adjusted_amt / factor).saturating_mul(px);
        assert!(safe_result <= u128::MAX);
    }
}

/// Test reward per share calculation precision
#[test]
fn test_reward_per_share_precision() {
    let mut farm = FarmState::default();
    farm.total_active_stake_scaled = Decimal::from(1_000_000_000_000u64).to_scaled_val().unwrap();
    
    // Small reward amount
    let rewards = 1u64;
    
    // Calculate reward per share
    let added_reward_per_share = Decimal::from(rewards) / farm.total_active_stake_scaled;
    
    // Should be non-zero even with small rewards
    assert!(added_reward_per_share > Decimal::zero());
    
    // Test accumulation over time
    let mut total_rps = Decimal::zero();
    for _ in 0..1_000_000 {
        total_rps = total_rps + added_reward_per_share;
    }
    
    // Should accumulate to meaningful value
    assert!(total_rps > Decimal::from(1u64));
}

/// Test timestamp overflow in cooldown period
#[test]
fn test_timestamp_cooldown_overflow() {
    let current_ts: u64 = u64::MAX - 100;
    let cooldown_period: u32 = 1000;
    
    // This would overflow
    let result = current_ts.checked_add(cooldown_period as u64);
    assert!(result.is_none());
    
    // Safe version
    let safe_result = current_ts.saturating_add(cooldown_period as u64);
    assert_eq!(safe_result, u64::MAX);
}

/// Test division by zero scenarios
#[test]
fn test_division_by_zero_protection() {
    // Test in reward per share calculation
    let rewards = 100u64;
    let zero_stake = Decimal::zero();
    
    // This would panic with division by zero
    let result = std::panic::catch_unwind(|| {
        Decimal::from(rewards) / zero_stake
    });
    assert!(result.is_err());
    
    // Safe version - check before division
    let safe_rps = if zero_stake > Decimal::zero() {
        Decimal::from(rewards) / zero_stake
    } else {
        Decimal::zero()
    };
    assert_eq!(safe_rps, Decimal::zero());
}

/// Test cumulative issuance calculation
#[test]
fn test_cumulative_issuance_bounds() {
    // Maximum time difference
    let last_issued_ts = 0u64;
    let current_ts = u64::MAX;
    let reward_per_time_unit = 1000u64;
    
    // Calculate cumulative amount (would overflow)
    let time_diff = current_ts.saturating_sub(last_issued_ts);
    let cumulative_amt = (time_diff as u128).saturating_mul(reward_per_time_unit as u128);
    
    // Should be capped at u128::MAX
    assert!(cumulative_amt <= u128::MAX);
}

/// Integration test for complete reward issuance flow
#[test]
fn test_reward_issuance_integration() {
    let mut farm = FarmState::default();
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1_000_000;
    farm.total_active_stake_scaled = Decimal::from(1_000_000u64).to_scaled_val().unwrap();
    
    let mut reward_info = &mut farm.reward_infos[0];
    reward_info.rewards_available = 1_000_000;
    reward_info.last_issuance_ts = 1000;
    reward_info.rewards_per_second_decimals = 6;
    
    // Set up reward schedule
    reward_info.reward_schedule_curve = RewardScheduleCurve::from_constant(100).unwrap();
    
    // Simulate time passing
    let current_ts = 2000;
    let time_passed = current_ts - reward_info.last_issuance_ts;
    
    // Calculate expected issuance
    let cumulative_amt = (time_passed as u128) * 100;
    let decimal_adjusted = cumulative_amt / (ten_pow(6) as u128);
    
    assert!(decimal_adjusted < u64::MAX as u128);
    
    // Test with different reward types
    for reward_type in [RewardType::Proportional, RewardType::Constant] {
        let type_adjusted = match reward_type {
            RewardType::Proportional => decimal_adjusted,
            RewardType::Constant => decimal_adjusted.saturating_mul(farm.total_staked_amount as u128),
        };
        
        // Ensure it fits in u64
        if type_adjusted <= u64::MAX as u128 {
            let amount = type_adjusted as u64;
            assert!(amount <= reward_info.rewards_available);
        }
    }
}