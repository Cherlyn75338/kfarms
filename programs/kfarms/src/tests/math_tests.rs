#[cfg(test)]
mod tests {
    use crate::utils::math::{full_decimal_mul_div, ten_pow, u64_mul_div};
    use decimal_wad::decimal::Decimal;

    #[test]
    fn test_ten_pow_edge_cases() {
        // Test valid range
        assert_eq!(ten_pow(0), 1);
        assert_eq!(ten_pow(1), 10);
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
    fn test_u64_mul_div_basic() {
        // Basic multiplication and division
        assert_eq!(u64_mul_div(10, 20, 5), 40);
        assert_eq!(u64_mul_div(100, 50, 10), 500);
        
        // Edge case: zero numerator
        assert_eq!(u64_mul_div(0, 100, 10), 0);
        
        // Precision test
        assert_eq!(u64_mul_div(1000, 3, 7), 428); // Floor division
    }

    #[test]
    fn test_u64_mul_div_extreme_values() {
        // Test with large values near u64::MAX
        let large_val = u64::MAX / 2;
        assert_eq!(u64_mul_div(large_val, 2, 2), large_val);
        
        // Test precision with basis points
        let amount = 1_000_000_000; // 1 billion
        let bps = 150; // 1.5%
        let bps_divisor = 10_000;
        assert_eq!(u64_mul_div(amount, bps, bps_divisor), 15_000_000);
    }

    #[test]
    #[should_panic(expected = "u64_mul_div overflow")]
    fn test_u64_mul_div_overflow() {
        // This should overflow when trying to fit result back into u64
        u64_mul_div(u64::MAX, u64::MAX, 1);
    }

    #[test]
    #[should_panic]
    fn test_u64_mul_div_division_by_zero() {
        u64_mul_div(100, 100, 0);
    }

    #[test]
    fn test_full_decimal_mul_div_basic() {
        let a = Decimal::from(100u64);
        let b = 50u64;
        let c = Decimal::from(25u64);
        
        let result = full_decimal_mul_div(a, b, c);
        assert_eq!(result, Decimal::from(200u64));
    }

    #[test]
    fn test_full_decimal_mul_div_precision() {
        // Test with fractional decimals
        let a = Decimal::from(1000u64) / 3; // ~333.333...
        let b = 3u64;
        let c = Decimal::from(1u64);
        
        let result = full_decimal_mul_div(a, b, c);
        let expected = Decimal::from(1000u64); // Should be close to 1000
        
        // Allow small rounding difference
        let diff = if result > expected {
            result - expected
        } else {
            expected - result
        };
        assert!(diff < Decimal::from(1u64));
    }

    #[test]
    fn test_full_decimal_mul_div_edge_cases() {
        // Zero cases
        let zero = Decimal::zero();
        let one = Decimal::one();
        
        assert_eq!(full_decimal_mul_div(zero, 100, one), zero);
        assert_eq!(full_decimal_mul_div(one, 0, one), zero);
        
        // Identity operations
        assert_eq!(full_decimal_mul_div(one, 1, one), one);
        
        // Large numbers
        let large = Decimal::from(u64::MAX);
        assert_eq!(full_decimal_mul_div(large, 1, large), one);
    }

    #[test]
    #[should_panic]
    fn test_full_decimal_mul_div_division_by_zero() {
        let a = Decimal::from(100u64);
        let b = 50u64;
        let c = Decimal::zero();
        
        full_decimal_mul_div(a, b, c);
    }

    #[test]
    fn test_rounding_consistency() {
        // Test that repeated operations don't accumulate rounding errors unexpectedly
        let initial = Decimal::from(1_000_000u64);
        let mut value = initial;
        
        // Perform 100 operations that should theoretically cancel out
        for _ in 0..100 {
            value = full_decimal_mul_div(value, 10001, Decimal::from(10000u64));
            value = full_decimal_mul_div(value, 10000, Decimal::from(10001u64));
        }
        
        // Should be very close to initial value
        let diff = if value > initial {
            value - initial
        } else {
            initial - value
        };
        
        // Allow for accumulated rounding error but it should be small
        assert!(diff < Decimal::from(100u64));
    }

    #[test]
    fn test_mul_div_monotonicity() {
        // Ensure monotonicity: if a1 < a2, then mul_div(a1, b, c) <= mul_div(a2, b, c)
        let b = 1000u64;
        let c = 777u64;
        
        let mut prev_result = 0u64;
        for a in 1..=100 {
            let result = u64_mul_div(a, b, c);
            assert!(result >= prev_result, "Monotonicity violated at a={}", a);
            prev_result = result;
        }
    }

    #[test]
    fn test_decimal_scaling_edge_cases() {
        // Test with maximum scaled values
        let max_scaled = Decimal::from_scaled_val(u128::MAX);
        let one = Decimal::one();
        
        // This should not panic but might lose precision
        let result = full_decimal_mul_div(max_scaled, 1, max_scaled);
        assert_eq!(result, one);
    }

    #[test]
    fn test_cross_decimal_precision() {
        // Simulate different token decimals (6, 9, 18)
        let amount_6_decimals = 1_000_000u64; // 1 token with 6 decimals
        let amount_9_decimals = 1_000_000_000u64; // 1 token with 9 decimals
        let amount_18_decimals = 1_000_000_000_000_000_000u64; // 1 token with 18 decimals
        
        // Convert to same base (18 decimals)
        let normalized_6 = amount_6_decimals * ten_pow(12);
        let normalized_9 = amount_9_decimals * ten_pow(9);
        let normalized_18 = amount_18_decimals;
        
        // All should represent the same value
        assert_eq!(normalized_6, normalized_18);
        assert_eq!(normalized_9, normalized_18);
    }
}