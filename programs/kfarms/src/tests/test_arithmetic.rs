use crate::utils::math::{full_decimal_mul_div, u64_mul_div, ten_pow};
use decimal_wad::decimal::Decimal;

#[cfg(test)]
mod arithmetic_safety_tests {
    use super::*;

    // A. Unit tests for checked arithmetic and overflow prevention
    
    #[test]
    fn test_u64_mul_div_basic() {
        // Basic functionality
        assert_eq!(u64_mul_div(10, 20, 5), 40);
        assert_eq!(u64_mul_div(100, 50, 25), 200);
        assert_eq!(u64_mul_div(1000, 1000, 1000), 1000);
    }

    #[test]
    fn test_u64_mul_div_zero_values() {
        // Zero values
        assert_eq!(u64_mul_div(0, 100, 50), 0);
        assert_eq!(u64_mul_div(100, 0, 50), 0);
    }

    #[test]
    #[should_panic]
    fn test_u64_mul_div_division_by_zero() {
        // Division by zero should panic
        u64_mul_div(100, 100, 0);
    }

    #[test]
    fn test_u64_mul_div_max_values() {
        // Test with u64 max values - should not overflow internally
        let max = u64::MAX;
        assert_eq!(u64_mul_div(max, 1, max), 1);
        assert_eq!(u64_mul_div(max / 2, 2, max), 1);
    }

    #[test]
    #[should_panic(expected = "u64_mul_div overflow")]
    fn test_u64_mul_div_overflow() {
        // This should overflow when converting back to u64
        u64_mul_div(u64::MAX, u64::MAX, 1);
    }

    #[test]
    fn test_u64_mul_div_precision_loss() {
        // Test precision loss in integer division
        assert_eq!(u64_mul_div(10, 3, 9), 3); // 10*3/9 = 3.333... -> 3
        assert_eq!(u64_mul_div(100, 7, 13), 53); // 100*7/13 = 53.846... -> 53
    }

    #[test]
    fn test_u64_mul_div_boundary_cases() {
        // Test boundary values
        assert_eq!(u64_mul_div(1, 1, 1), 1);
        assert_eq!(u64_mul_div(u64::MAX, 1, u64::MAX), 1);
        assert_eq!(u64_mul_div(u64::MAX - 1, 1, u64::MAX), 0);
        
        // Large numerator, small divisor
        assert_eq!(u64_mul_div(1_000_000_000_000, 1000, 10), 100_000_000_000_000);
        
        // Small numerator, large divisor
        assert_eq!(u64_mul_div(10, 10, 1_000_000_000_000), 0);
    }

    #[test]
    fn test_full_decimal_mul_div_basic() {
        let a = Decimal::from(100u64);
        let b = 50u64;
        let c = Decimal::from(25u64);
        
        let result = full_decimal_mul_div(a, b, c);
        assert_eq!(result.to_u64().unwrap(), 200);
    }

    #[test]
    fn test_full_decimal_mul_div_precision() {
        // Test with decimal precision
        let a = Decimal::from_scaled_val(1_500_000_000_000_000_000u128); // 1.5
        let b = 2u64;
        let c = Decimal::from_scaled_val(1_000_000_000_000_000_000u128); // 1.0
        
        let result = full_decimal_mul_div(a, b, c);
        assert_eq!(result.to_scaled_val::<u128>().unwrap(), 3_000_000_000_000_000_000u128); // 3.0
    }

    #[test]
    #[should_panic(expected = "full_decimal_mul_div overflow")]
    fn test_full_decimal_mul_div_overflow() {
        let max_decimal = Decimal::from_scaled_val(u128::MAX);
        let result = full_decimal_mul_div(max_decimal, u64::MAX, Decimal::from(1u64));
    }

    #[test]
    fn test_full_decimal_mul_div_edge_cases() {
        // Zero values
        let zero = Decimal::from(0u64);
        let one = Decimal::from(1u64);
        
        assert_eq!(full_decimal_mul_div(zero, 100, one).to_u64().unwrap(), 0);
        assert_eq!(full_decimal_mul_div(one, 0, one).to_u64().unwrap(), 0);
        
        // Very small decimals
        let small = Decimal::from_scaled_val(1u128); // Smallest possible non-zero
        let result = full_decimal_mul_div(small, 1, one);
        assert_eq!(result.to_scaled_val::<u128>().unwrap(), 1u128);
    }

    #[test]
    fn test_ten_pow() {
        assert_eq!(ten_pow(0), 1);
        assert_eq!(ten_pow(1), 10);
        assert_eq!(ten_pow(2), 100);
        assert_eq!(ten_pow(6), 1_000_000);
        assert_eq!(ten_pow(9), 1_000_000_000);
        assert_eq!(ten_pow(18), 1_000_000_000_000_000_000);
        assert_eq!(ten_pow(19), 10_000_000_000_000_000_000);
    }

    #[test]
    #[should_panic(expected = "The exponent must be between 0 and 19")]
    fn test_ten_pow_overflow() {
        ten_pow(20);
    }

    #[test]
    fn test_checked_arithmetic_patterns() {
        // Simulate patterns from the codebase
        
        // Test checked_add
        let a: u64 = u64::MAX - 10;
        assert_eq!(a.checked_add(10), Some(u64::MAX));
        assert_eq!(a.checked_add(11), None);
        
        // Test checked_sub
        let b: u64 = 10;
        assert_eq!(b.checked_sub(10), Some(0));
        assert_eq!(b.checked_sub(11), None);
        
        // Test checked_mul
        let c: u64 = u64::MAX / 2;
        assert_eq!(c.checked_mul(2), Some(u64::MAX - 1));
        assert_eq!(c.checked_mul(3), None);
        
        // Test saturating operations
        assert_eq!(u64::MAX.saturating_add(1), u64::MAX);
        assert_eq!(0u64.saturating_sub(1), 0);
        assert_eq!(u64::MAX.saturating_mul(2), u64::MAX);
    }

    #[test]
    fn test_high_rps_calculations() {
        // Test with very high rewards per share values
        let high_rps = Decimal::from_scaled_val(u128::MAX / 2);
        let stake = 1_000_000u64;
        let total = Decimal::from(10_000_000u64);
        
        // This should handle large values without overflow
        let result = full_decimal_mul_div(high_rps, stake, total);
        assert!(result.to_scaled_val::<u128>().is_ok());
    }

    #[test]
    fn test_long_duration_calculations() {
        // Test with very long durations (years)
        let seconds_per_year = 365 * 24 * 60 * 60;
        let duration = 10 * seconds_per_year; // 10 years
        let amount_per_second = 1_000_000u64;
        
        // Calculate total rewards over duration
        let total = amount_per_second.saturating_mul(duration as u64);
        assert!(total > 0);
        assert!(total < u64::MAX);
    }

    #[test]
    fn test_large_price_calculations() {
        // Test with large price values and exponents
        let price_value = u64::MAX / 1000; // Large but not max
        let price_exp = 18u64; // 18 decimals
        
        // Simulate price adjustment
        let adjustment_factor = ten_pow(price_exp as usize);
        let adjusted = u64_mul_div(1_000_000, price_value, adjustment_factor);
        assert!(adjusted > 0);
    }

    #[test]
    fn test_decimal_conversion_safety() {
        // Test safe decimal conversions
        let decimal = Decimal::from(u64::MAX);
        
        // to_u64 should work for values within range
        assert_eq!(decimal.to_u64().unwrap(), u64::MAX);
        
        // Test scaled value conversions
        let scaled = decimal.to_scaled_val::<u128>().unwrap();
        assert!(scaled > 0);
        
        // Test overflow in conversion
        let large_decimal = Decimal::from_scaled_val(u128::MAX);
        assert!(large_decimal.to_u64().is_err());
    }

    #[test]
    fn test_issuance_path_arithmetic() {
        // Simulate issuance calculation path
        let total_stake = 1_000_000_000u64;
        let reward_rate = 100_000u64; // per second
        let duration = 86400u64; // 1 day
        
        // Calculate total issuance
        let total_issuance = reward_rate.checked_mul(duration).unwrap();
        assert_eq!(total_issuance, 8_640_000_000);
        
        // Calculate per-share increase
        let per_share_increase = u64_mul_div(total_issuance, 1_000_000_000, total_stake);
        assert_eq!(per_share_increase, 8_640_000_000);
    }

    #[test]
    fn test_deposit_cap_calculations() {
        // Test deposit cap enforcement
        let current_deposits = u64::MAX - 1_000_000;
        let new_deposit = 999_999;
        
        // Should succeed
        assert!(current_deposits.checked_add(new_deposit).is_some());
        
        // Should fail
        let large_deposit = 1_000_001;
        assert!(current_deposits.checked_add(large_deposit).is_none());
    }

    #[test]
    fn test_price_path_arithmetic() {
        // Test price calculation paths
        let base_amount = 1_000_000_000u64;
        let price_numerator = 150_000_000u64; // 1.5 in 8 decimals
        let price_denominator = 100_000_000u64; // 1.0 in 8 decimals
        
        let adjusted_amount = u64_mul_div(base_amount, price_numerator, price_denominator);
        assert_eq!(adjusted_amount, 1_500_000_000);
    }
}