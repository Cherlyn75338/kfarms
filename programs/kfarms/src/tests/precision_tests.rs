use crate::stake_operations::{convert_amount_to_stake, convert_stake_to_amount};
use crate::utils::math::{full_decimal_mul_div, u64_mul_div};
use decimal_wad::decimal::Decimal;

#[test]
fn test_precision_conservation_in_conversions() {
    // Test that repeated conversions don't lose precision significantly
    let initial_amount = 1_000_000u64;
    let total_stake = Decimal::from(10_000_000u64);
    let total_amount = 10_000_000u64;
    
    let mut current_amount = initial_amount;
    
    // Perform 100 round-trip conversions
    for _ in 0..100 {
        let stake = convert_amount_to_stake(current_amount, total_stake, total_amount);
        current_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
    }
    
    // Should not lose more than 1% due to rounding
    assert!(current_amount >= initial_amount * 99 / 100);
}

#[test]
fn test_decimal_precision_limits() {
    // Test the precision limits of Decimal type
    let very_small = Decimal::from(1u64) / Decimal::from(1_000_000_000_000_000_000u64);
    assert!(very_small > Decimal::zero());
    
    let very_large = Decimal::from(u64::MAX);
    let doubled = very_large + very_large;
    assert!(doubled > very_large);
}

#[test]
fn test_rounding_accumulation() {
    // Test that rounding errors don't accumulate dangerously
    let total_stake = Decimal::from(1_000_000u64);
    let total_amount = 1_000_001u64; // Slightly different to cause rounding
    
    let mut total_recovered = 0u64;
    let num_users = 1000;
    let amount_per_user = total_amount / num_users;
    
    for _ in 0..num_users {
        let stake = convert_amount_to_stake(amount_per_user, total_stake, total_amount);
        let recovered = convert_stake_to_amount(stake, total_stake, total_amount, false);
        total_recovered += recovered;
    }
    
    // Total drift should be minimal
    let drift = if total_recovered > total_amount {
        total_recovered - total_amount
    } else {
        total_amount - total_recovered
    };
    
    // Drift should be less than number of operations
    assert!(drift < num_users);
}

#[test]
fn test_division_precision() {
    // Test precision in division operations
    let test_cases = vec![
        (1u64, 3u64),           // 1/3 = 0.333...
        (2u64, 3u64),           // 2/3 = 0.666...
        (1u64, 7u64),           // 1/7 = 0.142857...
        (22u64, 7u64),          // π approximation
        (1u64, 1_000_000u64),   // Very small result
    ];
    
    for (num, den) in test_cases {
        let result = u64_mul_div(num, 1_000_000_000, den);
        let expected = (num as u128 * 1_000_000_000) / den as u128;
        assert_eq!(result, expected as u64);
    }
}

#[test]
fn test_reward_per_share_precision() {
    // Test precision in reward per share calculations
    let reward = 1u64; // Minimum reward
    let total_stake = Decimal::from(1_000_000_000_000u64); // Large stake
    
    let reward_per_share = Decimal::from(reward) / total_stake;
    assert!(reward_per_share > Decimal::zero());
    
    // User with small stake should still get rewards eventually
    let user_stake = Decimal::from(1000u64);
    let user_reward = reward_per_share * user_stake;
    
    // After enough accumulation, user should get rewards
    let accumulated = user_reward * Decimal::from(1_000_000u64);
    assert!(accumulated >= Decimal::from(1u64));
}

#[test]
fn test_decimal_mul_div_precision() {
    // Test full_decimal_mul_div maintains precision
    let a = Decimal::from(1u64) + (Decimal::from(1u64) / Decimal::from(3u64)); // 1.333...
    let b = 3u64;
    let c = Decimal::from(2u64);
    
    let result = full_decimal_mul_div(a, b, c);
    let expected = Decimal::from(2u64); // (4/3 * 3) / 2 = 2
    
    let diff = if result > expected {
        result - expected
    } else {
        expected - result
    };
    
    // Should be very close
    assert!(diff < Decimal::from(1u64) / Decimal::from(1_000_000u64));
}

#[test]
fn test_fractional_amounts() {
    // Test handling of amounts that result in fractions
    let test_cases = vec![
        (10u64, 3u64),  // 10/3 = 3.333...
        (100u64, 7u64), // 100/7 = 14.285...
        (1000u64, 13u64), // 1000/13 = 76.923...
    ];
    
    for (total, divisor) in test_cases {
        let mut sum_floor = 0u64;
        let mut sum_ceil = 0u64;
        
        for i in 0..divisor {
            let share = Decimal::from(total) / Decimal::from(divisor);
            let amount_floor = share.try_floor().unwrap();
            let amount_ceil = share.try_ceil().unwrap();
            
            sum_floor += amount_floor;
            sum_ceil += amount_ceil;
        }
        
        // Floor sum should be less than or equal to total
        assert!(sum_floor <= total);
        // Ceil sum should be greater than or equal to total
        assert!(sum_ceil >= total);
        // Difference should be at most the divisor
        assert!(sum_ceil - sum_floor <= divisor);
    }
}

#[test]
fn test_minimum_representable_values() {
    // Test the smallest values that can be represented
    let min_decimal = Decimal::from(1u64) / Decimal::from(u64::MAX);
    assert!(min_decimal > Decimal::zero());
    
    // Test conversion maintains minimum values
    let total_stake = Decimal::from(u64::MAX);
    let total_amount = u64::MAX;
    
    let min_stake = convert_amount_to_stake(1, total_stake, total_amount);
    assert!(min_stake > Decimal::zero());
    
    let recovered = convert_stake_to_amount(min_stake, total_stake, total_amount, false);
    assert_eq!(recovered, 1);
}

#[test]
fn test_precision_in_reward_distribution() {
    // Simulate reward distribution to many users
    let total_reward = 1_000_000u64;
    let num_users = 10_000;
    let total_stake = Decimal::from(1_000_000_000u64);
    
    // Each user has slightly different stake
    let mut distributed = 0u64;
    
    for i in 0..num_users {
        let user_stake = Decimal::from(100_000u64 + i);
        let user_share = user_stake / total_stake;
        let user_reward = (user_share * Decimal::from(total_reward)).try_floor().unwrap();
        distributed += user_reward;
    }
    
    // Total distributed should be close to total reward
    let loss = total_reward.saturating_sub(distributed);
    
    // Loss due to rounding should be minimal
    assert!(loss < num_users); // Less than 1 per user
}