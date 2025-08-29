use proptest::prelude::*;
use crate::math::{RewardMath, SCALE, BPS_DIVISOR};

/// Property-based tests for mathematical invariants
#[cfg(test)]
mod property_tests {
    use super::*;

    proptest! {
        /// Test: Lock multiplier is always between 1x and max
        #[test]
        fn prop_lock_multiplier_bounds(
            lock_duration in 0u64..=365*86400,
            max_duration in 1u64..=365*86400,
            max_multiplier_bps in 10000u128..=30000
        ) {
            let math = RewardMath::new();
            
            if lock_duration <= max_duration {
                let result = math.calculate_lock_multiplier(
                    lock_duration,
                    max_duration,
                    max_multiplier_bps
                );
                
                if let Ok(multiplier) = result {
                    // Multiplier should be between 1x and max
                    prop_assert!(multiplier >= BPS_DIVISOR);
                    prop_assert!(multiplier <= max_multiplier_bps);
                    
                    // Monotonicity: longer lock = higher multiplier
                    if lock_duration > 0 {
                        let shorter = math.calculate_lock_multiplier(
                            lock_duration / 2,
                            max_duration,
                            max_multiplier_bps
                        ).unwrap();
                        prop_assert!(multiplier >= shorter);
                    }
                }
            }
        }

        /// Test: Conservation of rewards - distributed <= emitted
        #[test]
        fn prop_conservation_invariant(
            emission_rate in 0u128..=1_000_000 * SCALE,
            time_delta in 0u64..=86400,
            total_points in 1u128..=u64::MAX as u128,
            num_users in 1usize..=100
        ) {
            let math = RewardMath::new();
            
            // Calculate total emission
            let total_emission = emission_rate
                .saturating_mul(time_delta as u128);
            
            // Update cumulative reward per point
            let c = math.update_cumulative_rpp(
                0,
                emission_rate,
                time_delta,
                total_points
            ).unwrap_or(0);
            
            // Distribute to users proportionally
            let mut total_distributed = 0u128;
            let points_per_user = total_points / (num_users as u128);
            
            for _ in 0..num_users {
                let earned = math.calculate_earned_rewards(
                    points_per_user,
                    c,
                    0,
                    0
                ).unwrap_or(0);
                
                total_distributed = total_distributed.saturating_add(earned);
            }
            
            // Conservation: distributed <= emitted (with rounding tolerance)
            let rounding_tolerance = (num_users as u128) * 2;
            prop_assert!(total_distributed <= total_emission + rounding_tolerance);
        }

        /// Test: Monotonicity of cumulative values
        #[test]
        fn prop_cumulative_monotonicity(
            initial_c in 0u128..=u128::MAX/2,
            emission_rate in 0u128..=1_000_000 * SCALE,
            time_delta1 in 0u64..=3600,
            time_delta2 in 0u64..=3600,
            total_points in 1u128..=u64::MAX as u128
        ) {
            let math = RewardMath::new();
            
            // First update
            let c1 = math.update_cumulative_rpp(
                initial_c,
                emission_rate,
                time_delta1,
                total_points
            ).unwrap_or(initial_c);
            
            // C should be non-decreasing
            prop_assert!(c1 >= initial_c);
            
            // Second update
            let c2 = math.update_cumulative_rpp(
                c1,
                emission_rate,
                time_delta2,
                total_points
            ).unwrap_or(c1);
            
            // Still non-decreasing
            prop_assert!(c2 >= c1);
        }

        /// Test: Points calculation consistency
        #[test]
        fn prop_points_calculation_consistency(
            staked_amount in 0u128..=u64::MAX as u128,
            lock_multiplier_bps in 10000u128..=30000,
            external_points in 0u128..=u64::MAX as u128
        ) {
            let math = RewardMath::new();
            
            let result = math.calculate_user_points(
                staked_amount,
                lock_multiplier_bps,
                external_points
            );
            
            if let Ok(points) = result {
                // Points should be at least staked amount (1x multiplier minimum)
                prop_assert!(points >= staked_amount);
                
                // Points should include external points
                prop_assert!(points >= external_points);
                
                // Reversibility check
                let base_points = points - external_points;
                let expected_base = (staked_amount * lock_multiplier_bps) / BPS_DIVISOR;
                prop_assert_eq!(base_points, expected_base);
            }
        }

        /// Test: Penalty calculation bounds
        #[test]
        fn prop_penalty_bounds(
            amount in 1u128..=u64::MAX as u128,
            time_remaining in 0u64..=365*86400,
            max_lock_duration in 1u64..=365*86400,
            max_penalty_bps in 0u128..=5000 // Max 50% penalty
        ) {
            let math = RewardMath::new();
            
            if time_remaining <= max_lock_duration {
                let result = math.apply_withdrawal_penalty(
                    amount,
                    time_remaining,
                    max_lock_duration,
                    max_penalty_bps
                );
                
                if let Ok((amount_after, penalty)) = result {
                    // Penalty should not exceed max
                    let max_penalty = (amount * max_penalty_bps) / BPS_DIVISOR;
                    prop_assert!(penalty <= max_penalty);
                    
                    // Amount after + penalty = original amount
                    prop_assert_eq!(amount_after + penalty, amount);
                    
                    // No penalty if no time remaining
                    if time_remaining == 0 {
                        prop_assert_eq!(penalty, 0);
                        prop_assert_eq!(amount_after, amount);
                    }
                }
            }
        }

        /// Test: TWAP calculation properties
        #[test]
        fn prop_twap_bounds(
            samples in prop::collection::vec(
                (0u128..=1_000_000, 0u64..=10000),
                1..=100
            )
        ) {
            let math = RewardMath::new();
            
            if samples.len() >= 2 {
                let mut sorted_samples = samples.clone();
                sorted_samples.sort_by_key(|s| s.1);
                
                let window_start = sorted_samples[0].1;
                let window_end = sorted_samples[sorted_samples.len() - 1].1;
                
                if window_end > window_start {
                    let twap = math.calculate_twap(
                        &sorted_samples,
                        window_start,
                        window_end
                    ).unwrap_or(0);
                    
                    // TWAP should be between min and max values
                    let min_points = sorted_samples.iter().map(|s| s.0).min().unwrap();
                    let max_points = sorted_samples.iter().map(|s| s.0).max().unwrap();
                    
                    prop_assert!(twap >= min_points);
                    prop_assert!(twap <= max_points);
                }
            }
        }

        /// Test: Decimal normalization preserves value relationships
        #[test]
        fn prop_decimal_normalization(
            amount1 in 0u128..=u64::MAX as u128,
            amount2 in 0u128..=u64::MAX as u128,
            from_decimals in 0u8..=18,
            to_decimals in 0u8..=18
        ) {
            let math = RewardMath::new();
            
            let norm1 = math.normalize_decimals(amount1, from_decimals, to_decimals);
            let norm2 = math.normalize_decimals(amount2, from_decimals, to_decimals);
            
            if let (Ok(n1), Ok(n2)) = (norm1, norm2) {
                // Ordering should be preserved
                if amount1 < amount2 {
                    prop_assert!(n1 <= n2); // <= due to rounding
                } else if amount1 > amount2 {
                    prop_assert!(n1 >= n2);
                } else {
                    prop_assert_eq!(n1, n2);
                }
                
                // Round-trip for same decimals
                if from_decimals == to_decimals {
                    prop_assert_eq!(n1, amount1);
                }
            }
        }

        /// Test: EMA smoothing convergence
        #[test]
        fn prop_ema_convergence(
            target_value in 0u128..=1_000_000,
            initial_value in 0u128..=1_000_000,
            alpha_bps in 1u128..=5000, // 0.01% to 50%
            iterations in 10usize..=100
        ) {
            let math = RewardMath::new();
            
            let mut current = initial_value;
            
            for _ in 0..iterations {
                current = math.apply_ema_smoothing(
                    target_value,
                    current,
                    alpha_bps
                ).unwrap_or(current);
            }
            
            // After many iterations, should converge close to target
            let tolerance = target_value / 100; // 1% tolerance
            let diff = current.abs_diff(target_value);
            
            // Higher alpha = faster convergence
            if alpha_bps >= 2000 && iterations >= 50 {
                prop_assert!(diff <= tolerance);
            }
        }

        /// Test: No overflow in wide math operations
        #[test]
        fn prop_no_overflow_wide_math(
            a in 0u128..=u64::MAX as u128,
            b in 0u128..=u64::MAX as u128,
            c in 1u128..=u64::MAX as u128
        ) {
            // Test pattern: (a * b) / c
            let result = a.checked_mul(b).and_then(|prod| prod.checked_div(c));
            
            // If operation succeeds, verify correctness
            if let Some(res) = result {
                // Result should be less than max of inputs (roughly)
                prop_assert!(res <= a.max(b) * 2);
                
                // Verify with big integer math for correctness
                use num_bigint::BigUint;
                let big_a = BigUint::from(a);
                let big_b = BigUint::from(b);
                let big_c = BigUint::from(c);
                let big_result = (big_a * big_b) / big_c;
                
                // Should match within rounding
                if let Some(exact) = big_result.to_u128() {
                    prop_assert!(res.abs_diff(exact) <= 1);
                }
            }
        }
    }
}