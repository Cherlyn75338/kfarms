#[cfg(test)]
mod precision_and_rounding_tests {
    use crate::stake_operations::{convert_stake_to_amount, convert_amount_to_stake};
    use crate::utils::math::{u64_mul_div, full_decimal_mul_div};
    use decimal_wad::decimal::Decimal;
    use proptest::prelude::*;
    
    #[test]
    fn test_mul_div_monotonicity() {
        // Test that mul_div is monotonic in each argument
        for a in 1..100u64 {
            for b in 1..100u64 {
                for c in 1..100u64 {
                    let result1 = u64_mul_div(a, b, c);
                    
                    // Monotonic in a
                    if a < u64::MAX {
                        let result2 = u64_mul_div(a + 1, b, c);
                        assert!(result2 >= result1, "Not monotonic in a");
                    }
                    
                    // Monotonic in b
                    if b < u64::MAX {
                        let result3 = u64_mul_div(a, b + 1, c);
                        assert!(result3 >= result1, "Not monotonic in b");
                    }
                    
                    // Inverse monotonic in c
                    if c < u64::MAX {
                        let result4 = u64_mul_div(a, b, c + 1);
                        assert!(result4 <= result1, "Not inverse monotonic in c");
                    }
                }
            }
        }
    }
    
    #[test]
    fn test_rounding_bias_small_deposits() {
        // Test for dust extraction through repeated small deposits/withdrawals
        let mut total_stake = Decimal::from(1_000_000u64);
        let mut total_amount = 1_000_000u64;
        let mut accumulated_dust = 0u64;
        
        // Simulate 1000 small deposits
        for _ in 0..1000 {
            let small_amount = 1u64; // 1 wei deposit
            
            // Calculate stake for small deposit
            let stake_gained = convert_amount_to_stake(small_amount, total_stake, total_amount);
            
            // Update totals
            total_stake = total_stake + stake_gained;
            total_amount += small_amount;
            
            // Try to withdraw immediately (testing rounding)
            let amount_withdrawn = convert_stake_to_amount(
                stake_gained,
                total_stake,
                total_amount,
                false, // floor rounding
            );
            
            // Check for dust
            if amount_withdrawn < small_amount {
                accumulated_dust += small_amount - amount_withdrawn;
            }
            
            // Update totals after withdrawal
            total_stake = total_stake - stake_gained;
            total_amount -= amount_withdrawn;
        }
        
        // Dust should be minimal (less than 0.1% of total operations)
        assert!(
            accumulated_dust < 10,
            "Excessive dust accumulation: {}",
            accumulated_dust
        );
    }
    
    #[test]
    fn test_cross_decimal_precision() {
        // Test with different decimal scales (6, 9, 18 decimals)
        let decimals = vec![6, 9, 18];
        
        for &decimal in &decimals {
            let scale = 10u64.pow(decimal);
            let amount = scale; // 1 token
            
            let total_stake = Decimal::from(scale);
            let total_amount = scale;
            
            // Test conversion maintains precision
            let stake = convert_amount_to_stake(amount, total_stake, total_amount);
            let recovered_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
            
            assert_eq!(
                recovered_amount, amount,
                "Precision loss at {} decimals", decimal
            );
        }
    }
    
    #[test]
    fn test_extreme_ratios() {
        // Test with extreme stake/amount ratios
        
        // Very large stake, small amount
        let large_stake = Decimal::from(u64::MAX);
        let small_amount = 1u64;
        let stake = convert_amount_to_stake(1, large_stake, small_amount);
        assert!(stake > Decimal::zero(), "Should handle large stake/small amount");
        
        // Very small stake, large amount
        let small_stake = Decimal::from(1u64);
        let large_amount = u64::MAX;
        let amount = convert_stake_to_amount(
            Decimal::from(1u64),
            small_stake,
            large_amount,
            false
        );
        assert_eq!(amount, large_amount, "Should handle small stake/large amount");
    }
    
    #[test]
    fn test_full_decimal_mul_div_exactness() {
        // Test exactness for simple cases
        let test_cases = vec![
            (Decimal::from(100u64), 2u64, Decimal::from(2u64), Decimal::from(100u64)),
            (Decimal::from(1000u64), 3u64, Decimal::from(3u64), Decimal::from(1000u64)),
            (Decimal::from(50u64), 4u64, Decimal::from(2u64), Decimal::from(100u64)),
        ];
        
        for (a, b, c, expected) in test_cases {
            let result = full_decimal_mul_div(a, b, c);
            assert_eq!(result, expected, "Exactness test failed for {:?} * {} / {:?}", a, b, c);
        }
    }
    
    #[test]
    fn test_rounding_consistency() {
        // Test that rounding is consistent across operations
        let stake = Decimal::from(333u64);
        let total_stake = Decimal::from(1000u64);
        let total_amount = 1000u64;
        
        // Test floor vs ceil rounding
        let amount_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let amount_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        
        assert!(
            amount_ceil >= amount_floor,
            "Ceil should always be >= floor"
        );
        assert!(
            amount_ceil - amount_floor <= 1,
            "Rounding difference should be at most 1"
        );
    }
    
    #[test]
    fn test_precision_loss_accumulation() {
        // Test for precision loss over many operations
        let mut total_stake = Decimal::from(1_000_000u64);
        let mut total_amount = 1_000_000u64;
        
        let initial_ratio = total_amount as f64 / 1_000_000f64;
        
        // Perform 10000 random operations
        for i in 0..10000 {
            let operation_amount = (i % 100 + 1) as u64;
            
            if i % 2 == 0 {
                // Deposit
                let stake_gained = convert_amount_to_stake(operation_amount, total_stake, total_amount);
                total_stake = total_stake + stake_gained;
                total_amount += operation_amount;
            } else {
                // Withdraw
                let stake_to_remove = Decimal::from(operation_amount);
                if stake_to_remove < total_stake {
                    let amount_removed = convert_stake_to_amount(
                        stake_to_remove,
                        total_stake,
                        total_amount,
                        false
                    );
                    total_stake = total_stake - stake_to_remove;
                    total_amount -= amount_removed;
                }
            }
        }
        
        let final_ratio = total_amount as f64 / (total_stake.to_u64().unwrap_or(1) as f64);
        let ratio_drift = (final_ratio - initial_ratio).abs() / initial_ratio;
        
        assert!(
            ratio_drift < 0.001, // Less than 0.1% drift
            "Excessive ratio drift: {}%",
            ratio_drift * 100.0
        );
    }
}

#[cfg(test)]
mod fuzz_tests {
    use crate::utils::math::{u64_mul_div, full_decimal_mul_div};
    use decimal_wad::decimal::Decimal;
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn fuzz_u64_mul_div_no_panic(
            a in 0u64..=u64::MAX,
            b in 0u64..=u64::MAX,
            c in 1u64..=u64::MAX,
        ) {
            // Should not panic for valid inputs
            if let Some(expected) = (a as u128).checked_mul(b as u128).and_then(|v| v.checked_div(c as u128)) {
                if expected <= u64::MAX as u128 {
                    let result = u64_mul_div(a, b, c);
                    assert_eq!(result, expected as u64);
                }
            }
        }
        
        #[test]
        fn fuzz_full_decimal_mul_div_consistency(
            a_val in 1u64..1_000_000u64,
            b in 1u64..1_000u64,
            c_val in 1u64..1_000_000u64,
        ) {
            let a = Decimal::from(a_val);
            let c = Decimal::from(c_val);
            
            let result = full_decimal_mul_div(a, b, c);
            
            // Verify the result is approximately correct
            let expected_approx = (a_val as f64 * b as f64 / c_val as f64);
            let result_val = result.to_u64().unwrap_or(0) as f64;
            
            let error = (result_val - expected_approx).abs();
            let relative_error = error / expected_approx.max(1.0);
            
            assert!(
                relative_error < 0.001, // Less than 0.1% error
                "Large relative error: {} for {} * {} / {}",
                relative_error, a_val, b, c_val
            );
        }
        
        #[test]
        fn fuzz_conversion_round_trip(
            amount in 1u64..1_000_000u64,
            total_amount in 1u64..10_000_000u64,
            total_stake_val in 1u64..10_000_000u64,
        ) {
            use crate::stake_operations::{convert_stake_to_amount, convert_amount_to_stake};
            
            let total_stake = Decimal::from(total_stake_val);
            
            // Convert amount to stake
            let stake = convert_amount_to_stake(amount, total_stake, total_amount);
            
            // Convert back to amount
            let recovered_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
            
            // Check that we don't lose more than 1 unit due to rounding
            let loss = if amount > recovered_amount {
                amount - recovered_amount
            } else {
                0
            };
            
            assert!(
                loss <= 1,
                "Excessive loss in round trip: {} -> {} (loss: {})",
                amount, recovered_amount, loss
            );
        }
    }
}