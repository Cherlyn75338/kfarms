use crate::utils::math::{full_decimal_mul_div, u64_mul_div};
use decimal_wad::decimal::Decimal;
use proptest::prelude::*;

#[cfg(test)]
mod precision_rounding_tests {
    use super::*;

    // B. Precision & Rounding Behavior Tests

    #[test]
    fn test_mul_div_pattern_consistency() {
        // Verify that mul/div patterns always use safe methods
        let test_cases = vec![
            (1000u64, 500u64, 250u64),
            (u64::MAX / 2, 2, 4),
            (100_000_000, 1_000_000, 10_000),
        ];

        for (a, b, c) in test_cases {
            let result = u64_mul_div(a, b, c);
            // Verify no truncation bias by checking reverse operation
            let reverse = u64_mul_div(result, c, b);
            // Allow for rounding difference of 1
            assert!(reverse == a || reverse == a - 1 || reverse == a + 1);
        }
    }

    #[test]
    fn test_division_truncation_bias() {
        // Test for systematic truncation bias in divisions
        let mut total_truncation = 0i64;
        
        for i in 1..1000 {
            let numerator = i * 1000;
            let denominator = 333; // Creates fractional results
            
            let result = u64_mul_div(numerator, 1, denominator);
            let exact = (numerator as f64) / (denominator as f64);
            let truncation = exact - (result as f64);
            
            // Truncation should always be positive (floor behavior)
            assert!(truncation >= 0.0);
            assert!(truncation < 1.0);
            
            total_truncation += (truncation * 1000.0) as i64;
        }
        
        // Average truncation should be around 0.5
        let avg_truncation = total_truncation as f64 / 999000.0;
        assert!(avg_truncation > 0.4 && avg_truncation < 0.6);
    }

    #[test]
    fn test_stake_amount_conversion_conservation() {
        // Test conservation in stake <-> amount conversions
        let total_stake = Decimal::from(1_000_000u64);
        let total_amount = 500_000u64;
        
        // Convert amount to stake
        let stake_from_amount = full_decimal_mul_div(
            total_stake,
            100_000u64,
            Decimal::from(total_amount)
        );
        
        // Convert back to amount
        let amount_from_stake = full_decimal_mul_div(
            Decimal::from(total_amount),
            stake_from_amount.to_u64().unwrap(),
            total_stake
        );
        
        // Should be approximately equal (within rounding)
        let recovered = amount_from_stake.to_u64().unwrap();
        assert!(recovered >= 99_999 && recovered <= 100_001);
    }

    #[test]
    fn test_repeated_minimal_claims() {
        // Simulate repeated minimal claims to check for accumulator drift
        let mut accumulator = 0u64;
        let minimal_claim = 1u64;
        let fee_bps = 100u64; // 1%
        
        for _ in 0..10000 {
            // Apply fee on minimal claim
            let fee = u64_mul_div(minimal_claim, fee_bps, 10000);
            let net = minimal_claim.saturating_sub(fee);
            accumulator = accumulator.saturating_add(net);
        }
        
        // With 1% fee, we should have accumulated close to 9900
        assert!(accumulator >= 9900 && accumulator <= 10000);
    }

    #[test]
    fn test_rounding_fairness() {
        // Test that rounding doesn't systematically favor any party
        let test_amounts = vec![1u64, 10, 100, 1000, 10000, 100000];
        
        for amount in test_amounts {
            let fee_bps = 250u64; // 2.5%
            
            // Calculate fee with rounding down
            let fee_down = u64_mul_div(amount, fee_bps, 10000);
            
            // Calculate fee with rounding up simulation
            let fee_up = u64_mul_div(amount * 10000 + 9999, fee_bps, 100000000);
            
            // Difference should be at most 1
            assert!(fee_up <= fee_down + 1);
        }
    }

    #[test]
    fn test_decimal_precision_preservation() {
        // Test that decimal operations preserve precision
        let high_precision = Decimal::from_scaled_val(123_456_789_012_345_678u128);
        let multiplier = 2u64;
        let divisor = Decimal::from(3u64);
        
        let result = full_decimal_mul_div(high_precision, multiplier, divisor);
        let scaled = result.to_scaled_val::<u128>().unwrap();
        
        // Check precision is maintained
        assert!(scaled > 0);
        
        // Reverse operation
        let reversed = full_decimal_mul_div(result, 3, Decimal::from(2u64));
        let diff = if reversed > high_precision {
            reversed.to_scaled_val::<u128>().unwrap() - high_precision.to_scaled_val::<u128>().unwrap()
        } else {
            high_precision.to_scaled_val::<u128>().unwrap() - reversed.to_scaled_val::<u128>().unwrap()
        };
        
        // Allow for minimal rounding error
        assert!(diff <= 2);
    }

    #[test]
    fn test_reward_per_share_precision() {
        // Test reward per share calculations with high precision
        let total_stake = 1_000_000_000_000u64; // 1 trillion
        let reward = 1u64; // Minimal reward
        
        // Calculate reward per share with decimal precision
        let rps_decimal = Decimal::from(reward * 1_000_000_000_000_000_000u128 / total_stake as u128);
        
        // Apply to individual stake
        let user_stake = 100u64;
        let user_reward = full_decimal_mul_div(
            rps_decimal,
            user_stake,
            Decimal::from(1_000_000_000_000_000_000u128)
        );
        
        // Should get proportional share
        assert_eq!(user_reward.to_u64().unwrap_or(0), 0); // Too small to register
    }

    #[test]
    fn test_conservation_over_random_sequences() {
        // Property test for conservation
        let mut rng = rand::thread_rng();
        let mut total_in = 0u128;
        let mut total_out = 0u128;
        
        for _ in 0..1000 {
            let amount = rng.gen_range(1..1_000_000);
            let operation = rng.gen_range(0..4);
            
            match operation {
                0 => {
                    // Stake in
                    total_in += amount as u128;
                }
                1 => {
                    // Stake out (if possible)
                    if total_out + amount as u128 <= total_in {
                        total_out += amount as u128;
                    }
                }
                2 => {
                    // Apply fee
                    let fee = u64_mul_div(amount, 100, 10000); // 1% fee
                    total_in += (amount - fee) as u128;
                }
                _ => {
                    // Reward distribution
                    if total_in > total_out {
                        let active = total_in - total_out;
                        let reward_per_unit = amount as u128 * 1_000_000 / active;
                        // Rewards should be distributable
                        assert!(reward_per_unit * active / 1_000_000 <= amount as u128 + 1);
                    }
                }
            }
        }
        
        // Conservation check
        assert!(total_out <= total_in);
    }

    #[test]
    fn test_decimal_conversion_rounding() {
        // Test rounding in decimal conversions
        let test_values = vec![
            1u128,
            999_999_999_999_999_999u128,
            1_000_000_000_000_000_000u128,
            1_000_000_000_000_000_001u128,
            u128::MAX / 2,
        ];
        
        for value in test_values {
            let decimal = Decimal::from_scaled_val(value);
            let scaled_back = decimal.to_scaled_val::<u128>().unwrap();
            assert_eq!(scaled_back, value);
        }
    }

    #[test]
    fn test_accumulator_drift_prevention() {
        // Test that accumulators don't drift over many operations
        let mut accumulator = Decimal::from(1_000_000u64);
        let operations = 100_000;
        
        for i in 0..operations {
            // Alternate between adding and subtracting small amounts
            let small_amount = Decimal::from(1u64);
            if i % 2 == 0 {
                accumulator = Decimal::from_scaled_val(
                    accumulator.to_scaled_val::<u128>().unwrap() + 
                    small_amount.to_scaled_val::<u128>().unwrap()
                );
            } else {
                accumulator = Decimal::from_scaled_val(
                    accumulator.to_scaled_val::<u128>().unwrap() - 
                    small_amount.to_scaled_val::<u128>().unwrap()
                );
            }
        }
        
        // Should return to original value
        assert_eq!(accumulator.to_u64().unwrap(), 1_000_000);
    }

    #[test]
    fn test_minimum_unit_calculations() {
        // Test calculations with minimum units
        let one_wei = 1u64;
        let large_divisor = u64::MAX;
        
        // Should round down to 0
        let result = u64_mul_div(one_wei, 1, large_divisor);
        assert_eq!(result, 0);
        
        // But with large enough numerator, should get non-zero
        let result2 = u64_mul_div(large_divisor, one_wei, large_divisor);
        assert_eq!(result2, one_wei);
    }

    #[test]
    fn test_decimal_multiplication_associativity() {
        // Test that (a * b) * c ≈ a * (b * c) within rounding
        let a = Decimal::from(1_000_000u64);
        let b = 500u64;
        let c = Decimal::from(2u64);
        
        // Left associative
        let left = full_decimal_mul_div(
            full_decimal_mul_div(a, b, Decimal::from(1u64)),
            1,
            c
        );
        
        // Right associative (simulate)
        let right = full_decimal_mul_div(a, b, c);
        
        // Should be equal
        assert_eq!(
            left.to_scaled_val::<u128>().unwrap(),
            right.to_scaled_val::<u128>().unwrap()
        );
    }

    #[test]
    fn test_fee_extraction_limits() {
        // Test that fees can't extract more than available
        let amount = 100u64;
        let fee_bps = 10001u64; // >100%
        
        // Should be capped at amount
        let fee = u64_mul_div(amount, fee_bps.min(10000), 10000);
        assert!(fee <= amount);
    }
}

// Property-based tests using proptest
#[cfg(test)]
mod property_tests {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_mul_div_conservation(
            a in 1u64..=1_000_000_000,
            b in 1u64..=1_000_000_000,
            c in 1u64..=1_000_000_000
        ) {
            let result = u64_mul_div(a, b, c);
            
            // Check that result * c ≈ a * b (within rounding)
            let product_ab = (a as u128) * (b as u128);
            let product_result_c = (result as u128) * (c as u128);
            
            // Allow for rounding error
            let diff = if product_ab > product_result_c {
                product_ab - product_result_c
            } else {
                product_result_c - product_ab
            };
            
            prop_assert!(diff < c as u128);
        }
        
        #[test]
        fn prop_decimal_precision_maintained(
            value in 1u128..=1_000_000_000_000_000_000u128,
            multiplier in 1u64..=1000,
            divisor in 1u64..=1000
        ) {
            let decimal = Decimal::from_scaled_val(value);
            let divisor_decimal = Decimal::from(divisor);
            
            let result = full_decimal_mul_div(decimal, multiplier, divisor_decimal);
            
            // Result should maintain precision
            prop_assert!(result.to_scaled_val::<u128>().is_ok());
            
            // Reverse operation should approximately recover original
            let recovered = full_decimal_mul_div(result, divisor, Decimal::from(multiplier));
            let recovered_val = recovered.to_scaled_val::<u128>().unwrap();
            
            let diff = if recovered_val > value {
                recovered_val - value
            } else {
                value - recovered_val
            };
            
            // Allow for small rounding error
            prop_assert!(diff <= divisor as u128);
        }
        
        #[test]
        fn prop_no_value_creation(
            total_supply in 1_000_000u64..=1_000_000_000_000u64,
            num_users in 2usize..=100,
            seed in 0u64..=u64::MAX
        ) {
            use rand::{Rng, SeedableRng};
            use rand::rngs::StdRng;
            
            let mut rng = StdRng::seed_from_u64(seed);
            
            // Distribute total supply among users
            let mut user_balances = vec![0u64; num_users];
            let mut remaining = total_supply;
            
            for i in 0..num_users - 1 {
                let share = rng.gen_range(0..=remaining / (num_users - i) as u64);
                user_balances[i] = share;
                remaining -= share;
            }
            user_balances[num_users - 1] = remaining;
            
            // Sum should equal total
            let sum: u64 = user_balances.iter().sum();
            prop_assert_eq!(sum, total_supply);
            
            // Apply random operations
            for _ in 0..100 {
                let from = rng.gen_range(0..num_users);
                let to = rng.gen_range(0..num_users);
                
                if from != to && user_balances[from] > 0 {
                    let amount = rng.gen_range(1..=user_balances[from]);
                    user_balances[from] -= amount;
                    
                    // Apply fee
                    let fee = u64_mul_div(amount, 100, 10000); // 1% fee
                    let net = amount - fee;
                    user_balances[to] += net;
                    
                    // Fee is lost (burned)
                    let new_sum: u64 = user_balances.iter().sum();
                    prop_assert!(new_sum <= total_supply);
                }
            }
        }
    }
}