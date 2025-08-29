#[cfg(test)]
mod proportional_math_tests {
    use crate::stake_operations::{convert_stake_to_amount, convert_amount_to_stake};
    use crate::utils::math::{u64_mul_div, full_decimal_mul_div};
    use decimal_wad::decimal::Decimal;
    
    #[test]
    fn test_convert_stake_to_amount_zero() {
        let stake = Decimal::zero();
        let total_stake = Decimal::from(100u64);
        let total_amount = 1000u64;
        
        let result = convert_stake_to_amount(stake, total_stake, total_amount, false);
        assert_eq!(result, 0, "Zero stake should return zero amount");
    }
    
    #[test]
    fn test_convert_stake_to_amount_proportional() {
        let stake = Decimal::from(25u64);
        let total_stake = Decimal::from(100u64);
        let total_amount = 1000u64;
        
        let result = convert_stake_to_amount(stake, total_stake, total_amount, false);
        assert_eq!(result, 250, "25% stake should return 250 from 1000");
    }
    
    #[test]
    fn test_convert_stake_to_amount_rounding() {
        let stake = Decimal::from(33u64);
        let total_stake = Decimal::from(100u64);
        let total_amount = 1000u64;
        
        let result_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let result_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        
        assert_eq!(result_floor, 330, "Floor rounding should give 330");
        assert_eq!(result_ceil, 330, "Ceil rounding should give 330");
        
        // Test with amount that causes rounding
        let total_amount = 1001u64;
        let result_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let result_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        
        assert_eq!(result_floor, 330, "Floor rounding should give 330");
        assert_eq!(result_ceil, 331, "Ceil rounding should give 331");
    }
    
    #[test]
    fn test_convert_amount_to_stake_zero() {
        let amount = 0u64;
        let total_stake = Decimal::from(100u64);
        let total_amount = 1000u64;
        
        let result = convert_amount_to_stake(amount, total_stake, total_amount);
        assert_eq!(result, Decimal::zero(), "Zero amount should return zero stake");
    }
    
    #[test]
    fn test_convert_amount_to_stake_proportional() {
        let amount = 250u64;
        let total_stake = Decimal::from(100u64);
        let total_amount = 1000u64;
        
        let result = convert_amount_to_stake(amount, total_stake, total_amount);
        assert_eq!(result, Decimal::from(25u64), "250 from 1000 should give 25% stake");
    }
    
    #[test]
    fn test_convert_amount_to_stake_empty_pool() {
        let amount = 1000u64;
        let total_stake = Decimal::zero();
        let total_amount = 0u64;
        
        let result = convert_amount_to_stake(amount, total_stake, total_amount);
        assert_eq!(result, Decimal::from(1000u64), "Empty pool should return amount as stake");
    }
    
    #[test]
    fn test_bidirectional_conversion_consistency() {
        let initial_stake = Decimal::from(50u64);
        let total_stake = Decimal::from(200u64);
        let total_amount = 10000u64;
        
        // Convert stake to amount
        let amount = convert_stake_to_amount(initial_stake, total_stake, total_amount, false);
        
        // Convert back to stake
        let recovered_stake = convert_amount_to_stake(amount, total_stake, total_amount);
        
        // Check if we get back approximately the same stake (accounting for rounding)
        let diff = if recovered_stake > initial_stake {
            recovered_stake - initial_stake
        } else {
            initial_stake - recovered_stake
        };
        
        assert!(diff < Decimal::from(1u64), "Bidirectional conversion should be consistent");
    }
    
    #[test]
    fn test_u64_mul_div_basic() {
        assert_eq!(u64_mul_div(10, 20, 5), 40);
        assert_eq!(u64_mul_div(100, 50, 25), 200);
        assert_eq!(u64_mul_div(1000, 1, 1000), 1);
    }
    
    #[test]
    fn test_u64_mul_div_edge_cases() {
        // Test with 1 wei
        assert_eq!(u64_mul_div(1, 1, 1), 1);
        
        // Test with large numbers close to u64::MAX
        let large = u64::MAX / 2;
        assert_eq!(u64_mul_div(large, 2, large), 2);
        
        // Test precision preservation
        assert_eq!(u64_mul_div(1_000_000, 1_000_000, 1_000_000), 1_000_000);
    }
    
    #[test]
    #[should_panic(expected = "u64_mul_div overflow")]
    fn test_u64_mul_div_overflow() {
        u64_mul_div(u64::MAX, u64::MAX, 1);
    }
    
    #[test]
    fn test_full_decimal_mul_div_precision() {
        let a = Decimal::from(1_000_000u64);
        let b = 2u64;
        let c = Decimal::from(3u64);
        
        let result = full_decimal_mul_div(a, b, c);
        let expected = Decimal::from(666_666u64); // Approximately
        
        let diff = if result > expected {
            result - expected
        } else {
            expected - result
        };
        
        assert!(diff < Decimal::from(1u64), "full_decimal_mul_div should maintain precision");
    }
}

#[cfg(test)]
mod reward_issuance_tests {
    use crate::farm_operations::{refresh_global_reward, user_refresh_reward, harvest};
    use crate::state::{FarmState, UserState, RewardInfo, TokenInfo, RewardType};
    use crate::utils::math::ten_pow;
    use decimal_wad::decimal::Decimal;
    use anchor_lang::prelude::*;
    use scope::DatedPrice;
    
    fn create_test_farm() -> FarmState {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_active_stake_scaled = 1000 * ten_pow(18) as u128;
        farm.total_staked_amount = 1000;
        farm.scope_oracle_price_id = u64::MAX; // No oracle
        
        // Setup reward info
        farm.reward_infos[0].rewards_available = 1_000_000;
        farm.reward_infos[0].last_issuance_ts = 0;
        farm.reward_infos[0].rewards_per_second_decimals = 0;
        
        farm
    }
    
    fn create_test_user() -> UserState {
        let mut user = UserState::default();
        user.active_stake_scaled = 100 * ten_pow(18) as u128; // 10% of farm
        user
    }
    
    #[test]
    fn test_refresh_global_reward_no_stake() {
        let mut farm = create_test_farm();
        farm.total_active_stake_scaled = 0; // No stake
        
        let result = refresh_global_reward(&mut farm, None, 100, 0);
        assert!(result.is_ok(), "Should handle zero stake gracefully");
        assert_eq!(farm.reward_infos[0].last_issuance_ts, 100);
        assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 0);
    }
    
    #[test]
    fn test_refresh_global_reward_with_stake() {
        let mut farm = create_test_farm();
        let initial_available = farm.reward_infos[0].rewards_available;
        
        // Set up reward schedule for testing
        farm.reward_infos[0].reward_schedule_curve.points[0].rewards_per_second = 10;
        farm.reward_infos[0].reward_schedule_curve.points[0].ts_start = 0;
        farm.reward_infos[0].reward_schedule_curve.points[0].ts_end = 1000;
        
        let result = refresh_global_reward(&mut farm, None, 100, 0);
        assert!(result.is_ok(), "Should refresh rewards successfully");
        
        assert_eq!(farm.reward_infos[0].last_issuance_ts, 100);
        assert!(farm.reward_infos[0].rewards_issued_unclaimed > 0);
        assert!(farm.reward_infos[0].rewards_available < initial_available);
    }
    
    #[test]
    fn test_user_refresh_reward() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Set initial reward per share
        farm.reward_infos[0].set_reward_per_share_decimal(Decimal::from(10u64));
        
        let result = user_refresh_reward(&mut farm, &mut user, 0);
        assert!(result.is_ok(), "Should refresh user rewards");
        
        // User should have earned rewards based on their stake
        assert!(user.reward_infos[0].rewards_earned_scaled > 0);
        assert_eq!(
            user.reward_infos[0].get_reward_tally_decimal(),
            farm.reward_infos[0].get_reward_per_share_decimal()
        );
    }
    
    #[test]
    fn test_harvest_zero_rewards() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // User has no rewards to harvest
        user.reward_infos[0].rewards_earned_scaled = 0;
        
        let result = harvest(&mut farm, &mut user, None, 100, 0);
        assert!(result.is_err(), "Should fail when no rewards to harvest");
    }
    
    #[test]
    fn test_harvest_with_rewards() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Give user some rewards
        let reward_amount = 1000u128;
        user.reward_infos[0].rewards_earned_scaled = reward_amount * ten_pow(18) as u128;
        farm.reward_infos[0].rewards_issued_unclaimed = reward_amount as u64;
        
        let result = harvest(&mut farm, &mut user, None, 100, 0);
        assert!(result.is_ok(), "Should harvest rewards successfully");
        
        if let Ok(harvest_effects) = result {
            assert_eq!(harvest_effects.reward_amount, reward_amount as u64);
            assert_eq!(user.reward_infos[0].rewards_earned_scaled, 0);
        }
    }
}

#[cfg(test)]
mod withdrawal_penalty_tests {
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    use crate::FarmError;
    
    #[test]
    fn test_no_penalty_after_maturity() {
        let locking_duration = 100;
        let locking_start = 0;
        let timestamp_now = 101; // After maturity
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        assert_eq!(amount_after_penalty, 1000);
        assert_eq!(penalty, 0);
    }
    
    #[test]
    fn test_penalty_before_maturity() {
        let locking_duration = 100;
        let locking_start = 0;
        let timestamp_now = 50; // Halfway through
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        assert_eq!(amount_after_penalty, 750); // 50% * 50% = 25% penalty
        assert_eq!(penalty, 250);
    }
    
    #[test]
    fn test_penalty_at_start() {
        let locking_duration = 100;
        let locking_start = 0;
        let timestamp_now = 0; // At start
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        assert_eq!(amount_after_penalty, 500); // Full 50% penalty
        assert_eq!(penalty, 500);
    }
    
    #[test]
    fn test_penalty_before_start() {
        let locking_duration = 100;
        let locking_start = 100; // Start in future
        let timestamp_now = 50; // Before start
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        assert_eq!(amount_after_penalty, 1000); // No penalty before start
        assert_eq!(penalty, 0);
    }
    
    #[test]
    fn test_invalid_penalty_percentage() {
        let locking_duration = 100;
        let locking_start = 0;
        let timestamp_now = 50;
        let penalty_bps = 10001; // > 100%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_zero_penalty_not_allowed() {
        let locking_duration = 100;
        let locking_start = 0;
        let timestamp_now = 50;
        let penalty_bps = 0; // 0%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_err());
    }
}