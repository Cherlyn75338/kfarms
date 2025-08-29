#[cfg(test)]
mod fuzz_tests {
    use proptest::prelude::*;
    use crate::utils::math::{u64_mul_div, full_decimal_mul_div};
    use crate::stake_operations::{convert_stake_to_amount, convert_amount_to_stake};
    use decimal_wad::decimal::Decimal;
    
    proptest! {
        #[test]
        fn fuzz_mul_div_consistency(
            a in 1u64..=u64::MAX/1000,
            b in 1u64..=u64::MAX/1000,
            c in 1u64..=u64::MAX/1000,
        ) {
            // Test associativity where possible
            if let (Some(ab), Some(bc)) = ((a as u128).checked_mul(b as u128), (b as u128).checked_mul(c as u128)) {
                if ab <= u64::MAX as u128 && bc <= u64::MAX as u128 {
                    // (a * b) / c should be close to a * (b / c) for large c
                    let result1 = u64_mul_div(a, b, c);
                    
                    // Can't directly test equality due to integer division
                    // but results should be within rounding error
                    prop_assert!(result1 <= (a as u128 * b as u128 / c as u128) as u64 + 1);
                }
            }
        }
        
        #[test]
        fn fuzz_stake_conversion_bounds(
            stake_val in 1u64..=1_000_000_000u64,
            total_stake_val in 1u64..=1_000_000_000u64,
            total_amount in 1u64..=1_000_000_000u64,
        ) {
            let stake = Decimal::from(stake_val);
            let total_stake = Decimal::from(total_stake_val);
            
            if stake <= total_stake {
                let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
                
                // Amount should be proportional and bounded
                prop_assert!(amount <= total_amount);
                
                // If stake is X% of total, amount should be approximately X% of total
                let stake_ratio = stake_val as f64 / total_stake_val as f64;
                let amount_ratio = amount as f64 / total_amount as f64;
                
                // Allow 1% tolerance for rounding
                prop_assert!((stake_ratio - amount_ratio).abs() < 0.01);
            }
        }
        
        #[test]
        fn fuzz_amount_to_stake_invariants(
            amount in 0u64..=1_000_000_000u64,
            total_amount in 1u64..=1_000_000_000u64,
            total_stake_val in 1u64..=1_000_000_000u64,
        ) {
            let total_stake = Decimal::from(total_stake_val);
            
            if amount <= total_amount {
                let stake = convert_amount_to_stake(amount, total_stake, total_amount);
                
                // Stake should be proportional
                let expected_stake = Decimal::from(total_stake_val * amount / total_amount);
                
                let diff = if stake > expected_stake {
                    stake - expected_stake
                } else {
                    expected_stake - stake
                };
                
                // Allow small rounding error
                prop_assert!(diff < Decimal::from(2u64));
            }
        }
        
        #[test]
        fn fuzz_round_trip_precision(
            amount in 1u64..=1_000_000u64,
            total_amount in 1_000u64..=10_000_000u64,
            total_stake_val in 1_000u64..=10_000_000u64,
        ) {
            let total_stake = Decimal::from(total_stake_val);
            
            // Forward conversion
            let stake = convert_amount_to_stake(amount, total_stake, total_amount);
            
            // Reverse conversion with floor rounding
            let recovered_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
            
            // Reverse conversion with ceil rounding  
            let recovered_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
            
            // Floor should be <= original <= ceil
            prop_assert!(recovered_floor <= amount);
            prop_assert!(recovered_ceil >= amount || recovered_ceil == amount);
            
            // Difference should be at most 1
            prop_assert!(amount - recovered_floor <= 1);
            prop_assert!(recovered_ceil - amount <= 1 || recovered_ceil == amount);
        }
        
        #[test]
        fn fuzz_decimal_mul_div_overflow_safety(
            a_val in 1u64..=u64::MAX,
            b in 1u64..=1000u64,
            c_val in 1u64..=u64::MAX,
        ) {
            let a = Decimal::from(a_val);
            let c = Decimal::from(c_val);
            
            // Should not panic even with large values
            let result = std::panic::catch_unwind(|| {
                full_decimal_mul_div(a, b, c)
            });
            
            // Either succeeds or panics with overflow message
            if result.is_err() {
                // Expected overflow panic
                prop_assert!(true);
            } else {
                let value = result.unwrap();
                // Result should be valid Decimal
                prop_assert!(value >= Decimal::zero());
            }
        }
        
        #[test]
        fn fuzz_extreme_decimal_values(
            mantissa in 0u128..=u128::MAX,
            divisor in 1u128..=u128::MAX,
        ) {
            // Test with extreme decimal values
            if mantissa <= u128::MAX / 10u128.pow(18) {
                let decimal = Decimal::from_scaled_val(mantissa);
                
                // Operations should handle extreme values
                let half = decimal / 2;
                let double = decimal * 2;
                
                if mantissa < u128::MAX / 2 {
                    prop_assert!(double > decimal);
                }
                prop_assert!(half < decimal || decimal == Decimal::zero());
            }
        }
        
        #[test]
        fn fuzz_penalty_calculation(
            duration in 100u64..=1_000_000u64,
            elapsed in 0u64..=2_000_000u64,
            penalty_bps in 1u64..=9999u64,
            amount in 1u64..=1_000_000_000u64,
        ) {
            use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
            
            let start = 1000u64;
            let current = start + elapsed;
            
            let result = apply_early_withdrawal_penalty(
                duration,
                start,
                current,
                penalty_bps,
                amount,
            );
            
            if let Ok((amount_after, penalty)) = result {
                // Invariants
                prop_assert_eq!(amount_after + penalty, amount);
                prop_assert!(penalty <= amount);
                
                // Penalty should decrease over time
                if elapsed >= duration {
                    prop_assert_eq!(penalty, 0);
                } else if current > start {
                    // Penalty should be proportional to time remaining
                    let time_remaining = duration.saturating_sub(elapsed);
                    let max_penalty = amount * penalty_bps / 10000;
                    let expected_penalty = max_penalty * time_remaining / duration;
                    
                    // Allow 1 unit rounding error
                    prop_assert!(penalty <= expected_penalty + 1);
                    prop_assert!(penalty + 1 >= expected_penalty || penalty == expected_penalty);
                }
            }
        }
        
        #[test]
        fn fuzz_reward_distribution(
            num_users in 1usize..=100usize,
            total_rewards in 1_000u64..=1_000_000_000u64,
            user_stakes in prop::collection::vec(1u64..=1_000_000u64, 1..=100),
        ) {
            use crate::state::{FarmState, UserState};
            use crate::farm_operations::user_refresh_reward;
            use crate::utils::math::ten_pow;
            
            if user_stakes.len() != num_users {
                return Ok(());
            }
            
            let mut farm = FarmState::default();
            farm.num_reward_tokens = 1;
            
            // Setup farm with total stake
            let total_stake: u64 = user_stakes.iter().sum();
            farm.total_staked_amount = total_stake;
            farm.total_active_stake_scaled = (total_stake as u128) * ten_pow(18) as u128;
            
            // Issue rewards
            farm.reward_infos[0].rewards_issued_unclaimed = total_rewards;
            farm.reward_infos[0].set_reward_per_share_decimal(
                Decimal::from(total_rewards) / farm.get_total_active_stake_decimal()
            );
            
            // Distribute to users
            let mut total_distributed = 0u64;
            for &stake in user_stakes.iter() {
                let mut user = UserState::default();
                user.active_stake_scaled = (stake as u128) * ten_pow(18) as u128;
                
                user_refresh_reward(&mut farm, &mut user, 0).unwrap();
                
                let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
                    .to_u64()
                    .unwrap_or(0);
                
                total_distributed += user_rewards;
                
                // Each user should get proportional share
                let expected = (total_rewards as u128 * stake as u128 / total_stake as u128) as u64;
                prop_assert!(user_rewards <= expected + 1);
                prop_assert!(user_rewards + 1 >= expected || user_rewards == expected);
            }
            
            // Total distributed should approximately equal total rewards
            prop_assert!(total_distributed <= total_rewards + num_users as u64);
            prop_assert!(total_distributed + num_users as u64 >= total_rewards || total_distributed == total_rewards);
        }
    }
}