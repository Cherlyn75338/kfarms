#[cfg(test)]
mod tests {
    use crate::utils::math::{full_decimal_mul_div, u64_mul_div};
    use crate::stake_operations::*;
    use decimal_wad::decimal::Decimal;
    use proptest::prelude::*;

    // Property-based testing with proptest
    proptest! {
        #[test]
        fn test_u64_mul_div_properties(
            a in 0u64..=1_000_000_000u64,
            b in 0u64..=1_000_000_000u64,
            c in 1u64..=1_000_000_000u64, // Avoid division by zero
        ) {
            let result = u64_mul_div(a, b, c);
            
            // Property 1: Result should fit in u64
            assert!(result <= u64::MAX);
            
            // Property 2: Commutativity of multiplication
            let result2 = u64_mul_div(b, a, c);
            assert_eq!(result, result2);
            
            // Property 3: Identity
            if b == c && b != 0 {
                assert_eq!(u64_mul_div(a, b, c), a);
            }
            
            // Property 4: Zero property
            if a == 0 || b == 0 {
                assert_eq!(result, 0);
            }
            
            // Property 5: Monotonicity
            if a > 0 && b > 0 {
                let result_plus = u64_mul_div(a + 1, b, c);
                assert!(result_plus >= result);
            }
        }

        #[test]
        fn test_decimal_mul_div_properties(
            a_val in 0u64..=1_000_000u64,
            b in 0u64..=1_000_000u64,
            c_val in 1u64..=1_000_000u64,
        ) {
            let a = Decimal::from(a_val);
            let c = Decimal::from(c_val);
            
            let result = full_decimal_mul_div(a, b, c);
            
            // Property 1: Zero property
            if a_val == 0 || b == 0 {
                assert_eq!(result, Decimal::zero());
            }
            
            // Property 2: Identity
            if b == c_val && b != 0 {
                let diff = if result > a { result - a } else { a - result };
                assert!(diff < Decimal::from(1u64)); // Allow small rounding
            }
            
            // Property 3: Scale preservation
            assert!(result.to_scaled_val::<u128>().is_ok());
        }

        #[test]
        fn test_stake_conversion_roundtrip(
            amount in 1u64..=1_000_000_000u64,
            total_amount in 1u64..=1_000_000_000_000u64,
            total_stake_val in 1u64..=1_000_000_000u64,
        ) {
            let total_stake = Decimal::from(total_stake_val);
            
            // Convert amount to stake
            let stake = convert_amount_to_stake(amount, total_stake, total_amount);
            
            // Convert back to amount
            let recovered_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
            let recovered_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
            
            // Property: Recovered amount should be close to original
            // Allow for rounding error of 1
            assert!(recovered_floor <= amount);
            assert!(recovered_ceil >= amount || recovered_ceil == amount - 1);
            assert!(recovered_ceil - recovered_floor <= 1);
        }

        #[test]
        fn test_share_distribution_fairness(
            amounts in prop::collection::vec(1u64..=1_000_000u64, 2..10)
        ) {
            let mut farm = FarmStake::default();
            let mut users: Vec<UserStake> = vec![UserStake::default(); amounts.len()];
            let mut user_deposits = vec![];
            
            // Each user deposits
            for (i, &amount) in amounts.iter().enumerate() {
                let stake = add_pending_deposit_stake(&mut users[i], &mut farm, amount).unwrap();
                activate_pending_stake(&mut users[i], &mut farm).unwrap();
                user_deposits.push((amount, stake));
            }
            
            // Property: Total shares equals sum of user shares
            let total_user_shares: Decimal = users.iter()
                .map(|u| u.active_stake)
                .fold(Decimal::zero(), |acc, s| acc + s);
            
            assert_eq!(total_user_shares, farm.total_active_stake);
            
            // Property: Share proportions match deposit proportions (approximately)
            let total_deposited: u64 = amounts.iter().sum();
            for (i, &amount) in amounts.iter().enumerate() {
                let expected_share_ratio = amount as f64 / total_deposited as f64;
                let actual_share_ratio = users[i].active_stake.to_scaled_val::<u128>().unwrap() as f64 
                    / farm.total_active_stake.to_scaled_val::<u128>().unwrap() as f64;
                
                // Allow 1% deviation due to rounding
                assert!((expected_share_ratio - actual_share_ratio).abs() < 0.01);
            }
        }

        #[test]
        fn test_penalty_calculation_bounds(
            duration in 100u64..=10_000u64,
            start in 1000u64..=100_000u64,
            penalty_bps in 1u64..=9999u64,
            amount in 1u64..=1_000_000_000u64,
        ) {
            use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
            
            // Test at various points in time
            let test_points = vec![
                start - 100,  // Before start
                start,         // At start
                start + duration / 4,  // 25% through
                start + duration / 2,  // 50% through
                start + duration * 3 / 4,  // 75% through
                start + duration - 1,  // Almost done
                start + duration,      // At maturity
                start + duration + 100, // After maturity
            ];
            
            for timestamp in test_points {
                let result = apply_early_withdrawal_penalty(
                    duration,
                    start,
                    timestamp,
                    penalty_bps,
                    amount,
                );
                
                if let Ok((withdrawn, penalty)) = result {
                    // Properties:
                    // 1. Sum equals original amount
                    assert_eq!(withdrawn + penalty, amount);
                    
                    // 2. Penalty is bounded by penalty_bps
                    let max_penalty = amount * penalty_bps / 10_000;
                    assert!(penalty <= max_penalty);
                    
                    // 3. No penalty after maturity
                    if timestamp >= start + duration {
                        assert_eq!(penalty, 0);
                    }
                    
                    // 4. No penalty before start (vulnerability check)
                    if timestamp < start {
                        assert_eq!(penalty, 0);
                    }
                }
            }
        }

        #[test]
        fn test_farm_withdrawal_prorata(
            active_amount in 1u64..=1_000_000_000u64,
            pending_amount in 1u64..=1_000_000_000u64,
            withdraw_ratio in 0.0f64..=2.0f64, // Can request more than available
        ) {
            let mut farm = FarmStake {
                total_active_amount: active_amount,
                total_pending_amount: pending_amount,
                total_active_stake: Decimal::from(active_amount),
                total_pending_stake: Decimal::from(pending_amount),
                ..Default::default()
            };
            
            let total = active_amount + pending_amount;
            let withdraw_request = (total as f64 * withdraw_ratio) as u64;
            
            let initial_active = farm.total_active_amount;
            let initial_pending = farm.total_pending_amount;
            
            let effects = withdraw_farm(&mut farm, withdraw_request).unwrap();
            
            // Properties:
            // 1. Conservation of value
            let remaining = farm.total_active_amount + farm.total_pending_amount;
            assert_eq!(remaining + effects.amount_to_withdraw, total);
            
            // 2. Pro-rata distribution (if partial withdrawal)
            if withdraw_request < total {
                let active_ratio = initial_active as f64 / total as f64;
                let pending_ratio = initial_pending as f64 / total as f64;
                
                let expected_active_withdrawn = (effects.amount_to_withdraw as f64 * active_ratio) as u64;
                let expected_pending_withdrawn = (effects.amount_to_withdraw as f64 * pending_ratio) as u64;
                
                let actual_active_withdrawn = initial_active - farm.total_active_amount;
                let actual_pending_withdrawn = initial_pending - farm.total_pending_amount;
                
                // Allow rounding error of 1
                assert!((actual_active_withdrawn as i64 - expected_active_withdrawn as i64).abs() <= 1);
                assert!((actual_pending_withdrawn as i64 - expected_pending_withdrawn as i64).abs() <= 1);
            }
            
            // 3. Farm freezes on full withdrawal
            if withdraw_request >= total {
                assert!(effects.farm_to_freeze);
                assert_eq!(farm.total_active_amount, 0);
                assert_eq!(farm.total_pending_amount, 0);
            }
        }

        #[test]
        fn test_rounding_accumulation(
            iterations in 10usize..=100usize,
            base_amount in 1_000_000u64..=10_000_000u64,
        ) {
            let mut value = Decimal::from(base_amount);
            let multiplier = 10001u64;
            let divisor = Decimal::from(10000u64);
            
            // Repeatedly multiply and divide
            for _ in 0..iterations {
                value = full_decimal_mul_div(value, multiplier, divisor);
                value = full_decimal_mul_div(value, 10000, Decimal::from(multiplier));
            }
            
            // Property: Rounding errors should be bounded
            let initial = Decimal::from(base_amount);
            let diff = if value > initial {
                value - initial
            } else {
                initial - value
            };
            
            // Allow up to 0.01% deviation per iteration
            let max_deviation = Decimal::from(base_amount) * iterations as u64 / 10000;
            assert!(diff < max_deviation);
        }
    }

    // Additional fuzz tests for edge cases
    #[test]
    fn fuzz_test_extreme_values() {
        // Test with maximum values
        let result = std::panic::catch_unwind(|| {
            u64_mul_div(u64::MAX, u64::MAX, 1)
        });
        assert!(result.is_err()); // Should overflow
        
        // Test with minimum divisor
        let result = u64_mul_div(1000, 1000, 1);
        assert_eq!(result, 1_000_000);
        
        // Test with zero numerator
        let result = u64_mul_div(0, u64::MAX, 1);
        assert_eq!(result, 0);
    }

    #[test]
    fn fuzz_test_decimal_edge_cases() {
        // Test with maximum scaled value
        let max_decimal = Decimal::from_scaled_val(u128::MAX);
        let one = Decimal::one();
        
        // This should not panic
        let result = full_decimal_mul_div(max_decimal, 1, max_decimal);
        assert_eq!(result, one);
        
        // Test with very small values
        let tiny = Decimal::from(1u64) / u64::MAX;
        let result = full_decimal_mul_div(tiny, u64::MAX, one);
        
        // Should be close to 1
        let diff = if result > one { result - one } else { one - result };
        assert!(diff < Decimal::from(1u64));
    }

    #[test]
    fn fuzz_test_share_dilution_attack() {
        // Simulate share dilution attack with random values
        let mut rng = proptest::test_runner::TestRunner::default();
        
        for _ in 0..100 {
            let first_deposit = rng.gen_range(1u64..100u64);
            let donation = rng.gen_range(1_000_000u64..1_000_000_000u64);
            let second_deposit = rng.gen_range(1_000_000u64..1_000_000_000u64);
            
            let mut farm = FarmStake::default();
            let mut user1 = UserStake::default();
            let mut user2 = UserStake::default();
            
            // First user deposits small amount
            add_pending_deposit_stake(&mut user1, &mut farm, first_deposit).unwrap();
            activate_pending_stake(&mut user1, &mut farm).unwrap();
            
            // Simulate donation attack
            increase_total_amount(&mut farm, donation).unwrap();
            
            // Second user deposits normal amount
            add_pending_deposit_stake(&mut user2, &mut farm, second_deposit).unwrap();
            activate_pending_stake(&mut user2, &mut farm).unwrap();
            
            // Check if attack was successful
            if user1.active_stake > user2.active_stake {
                // Attack successful - user1 has more shares despite smaller deposit
                // This demonstrates the vulnerability
                
                // Calculate actual value per share
                let total_value = farm.total_active_amount;
                let user1_value = convert_stake_to_amount(
                    user1.active_stake,
                    farm.total_active_stake,
                    total_value,
                    false
                );
                let user2_value = convert_stake_to_amount(
                    user2.active_stake,
                    farm.total_active_stake,
                    total_value,
                    false
                );
                
                // User1 profits from the attack
                assert!(user1_value > first_deposit);
                // User2 loses value
                assert!(user2_value < second_deposit);
            }
        }
    }
}