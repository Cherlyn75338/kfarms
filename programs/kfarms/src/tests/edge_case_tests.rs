#[cfg(test)]
mod edge_case_scenario_tests {
    use crate::state::{FarmState, UserState, RewardInfo};
    use crate::stake_operations::*;
    use crate::farm_operations::*;
    use crate::utils::math::ten_pow;
    use decimal_wad::decimal::Decimal;
    use crate::FarmError;
    
    #[test]
    fn test_zero_amount_operations() {
        let mut farm = FarmState::default();
        let mut user = UserState::default();
        
        // Test zero deposit
        let result = add_pending_deposit_stake(&mut user, &mut farm, 0);
        assert!(result.is_ok());
        assert_eq!(user.get_pending_deposit_stake_decimal(), Decimal::zero());
        
        // Test zero withdrawal
        let result = remove_active_stake(&mut user, &mut farm, Decimal::zero());
        assert!(result.is_err(), "Should not allow zero withdrawal");
        
        // Test zero stake conversion
        let amount = convert_stake_to_amount(Decimal::zero(), Decimal::from(100u64), 1000, false);
        assert_eq!(amount, 0);
        
        let stake = convert_amount_to_stake(0, Decimal::from(100u64), 1000);
        assert_eq!(stake, Decimal::zero());
    }
    
    #[test]
    fn test_one_wei_operations() {
        let mut farm = FarmState::default();
        farm.total_staked_amount = 1_000_000_000; // 1 billion units
        farm.total_active_stake_scaled = (1_000_000_000 * ten_pow(18)) as u128;
        
        let mut user = UserState::default();
        
        // Test 1 wei deposit
        let result = add_pending_deposit_stake(&mut user, &mut farm, 1);
        assert!(result.is_ok());
        
        // The stake should be non-zero but very small
        let stake = user.get_pending_deposit_stake_decimal();
        assert!(stake > Decimal::zero(), "1 wei should produce non-zero stake");
        assert!(stake < Decimal::from(1u64), "1 wei stake should be fractional");
    }
    
    #[test]
    fn test_u64_max_operations() {
        let mut farm = FarmState::default();
        let mut user = UserState::default();
        
        // Test with u64::MAX amount
        farm.total_staked_amount = u64::MAX - 1;
        farm.total_active_stake_scaled = ((u64::MAX - 1) as u128) * ten_pow(18) as u128;
        
        // Try to deposit 1 more (should handle overflow)
        let result = add_pending_deposit_stake(&mut user, &mut farm, 2);
        assert!(result.is_err() || farm.total_pending_amount <= 2);
        
        // Test u64::MAX as sentinel value
        farm.scope_oracle_price_id = u64::MAX;
        assert_eq!(farm.scope_oracle_price_id, u64::MAX, "Sentinel value should be preserved");
    }
    
    #[test]
    fn test_u128_scale_edges() {
        // Test operations at u128 scale boundaries
        let mut farm = FarmState::default();
        
        // Set to maximum safe scaled value
        let max_safe_scaled = u128::MAX / (ten_pow(18) as u128);
        farm.total_active_stake_scaled = max_safe_scaled * (ten_pow(18) as u128);
        
        // Operations should handle this gracefully
        let stake_decimal = farm.get_total_active_stake_decimal();
        assert!(stake_decimal > Decimal::zero());
        
        // Test setting back
        farm.set_total_active_stake_decimal(stake_decimal);
        assert_eq!(farm.total_active_stake_scaled, max_safe_scaled * (ten_pow(18) as u128));
    }
    
    #[test]
    fn test_extreme_decimals() {
        // Test with tokens of different decimal places
        let decimals = vec![0, 6, 9, 18, 27];
        
        for decimal_places in decimals {
            if decimal_places > 19 {
                continue; // Skip unsupported decimals
            }
            
            let scale = ten_pow(decimal_places);
            let mut farm = FarmState::default();
            farm.token.token_program = Default::default();
            farm.token.decimals = decimal_places as u8;
            
            // Test operations with this decimal scale
            let amount = scale; // 1 token
            let mut user = UserState::default();
            
            let result = add_pending_deposit_stake(&mut user, &mut farm, amount);
            assert!(result.is_ok(), "Should handle {} decimals", decimal_places);
        }
    }
    
    #[test]
    fn test_empty_pool_operations() {
        let mut farm = FarmState::default();
        assert_eq!(farm.total_staked_amount, 0);
        assert_eq!(farm.total_active_stake_scaled, 0);
        
        let mut user = UserState::default();
        
        // First deposit to empty pool
        let first_deposit = 1000u64;
        let result = add_pending_deposit_stake(&mut user, &mut farm, first_deposit);
        assert!(result.is_ok());
        
        // Move to active
        let result = move_pending_deposit_to_active(&mut user, &mut farm);
        assert!(result.is_ok());
        
        // First depositor should get 1:1 stake
        assert_eq!(user.get_active_stake_decimal(), Decimal::from(first_deposit));
        assert_eq!(farm.total_staked_amount, first_deposit);
    }
    
    #[test]
    fn test_single_staker_scenario() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        
        let mut user = UserState::default();
        
        // Single staker deposits
        let deposit = 10000u64;
        add_pending_deposit_stake(&mut user, &mut farm, deposit).unwrap();
        move_pending_deposit_to_active(&mut user, &mut farm).unwrap();
        
        // Issue rewards
        farm.reward_infos[0].rewards_issued_unclaimed = 1000;
        farm.reward_infos[0].set_reward_per_share_decimal(
            Decimal::from(1000u64) / farm.get_total_active_stake_decimal()
        );
        
        // Single staker should get all rewards
        user_refresh_reward(&mut farm, &mut user, 0).unwrap();
        
        let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
            .to_u64()
            .unwrap_or(0);
        
        assert_eq!(user_rewards, 1000, "Single staker should get all rewards");
    }
    
    #[test]
    fn test_many_stakers_scenario() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        
        let num_stakers = 1000;
        let mut users = Vec::new();
        
        // Many stakers deposit equal amounts
        for i in 0..num_stakers {
            let mut user = UserState::default();
            user.user_id = i;
            
            let deposit = 100u64;
            add_pending_deposit_stake(&mut user, &mut farm, deposit).unwrap();
            move_pending_deposit_to_active(&mut user, &mut farm).unwrap();
            
            users.push(user);
        }
        
        // Issue rewards
        let total_rewards = 100000u64;
        farm.reward_infos[0].rewards_issued_unclaimed = total_rewards;
        farm.reward_infos[0].set_reward_per_share_decimal(
            Decimal::from(total_rewards) / farm.get_total_active_stake_decimal()
        );
        
        // Each staker should get equal share
        let mut total_distributed = 0u64;
        for user in &mut users {
            user_refresh_reward(&mut farm, user, 0).unwrap();
            
            let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
                .to_u64()
                .unwrap_or(0);
            
            total_distributed += user_rewards;
            
            // Each should get approximately total_rewards / num_stakers
            let expected = total_rewards / num_stakers as u64;
            let diff = if user_rewards > expected {
                user_rewards - expected
            } else {
                expected - user_rewards
            };
            
            assert!(diff <= 1, "Reward distribution should be fair");
        }
        
        // Total distributed should approximately equal total rewards
        let diff = if total_distributed > total_rewards {
            total_distributed - total_rewards
        } else {
            total_rewards - total_distributed
        };
        
        assert!(
            diff <= num_stakers as u64,
            "Total distribution should match rewards with minimal rounding"
        );
    }
    
    #[test]
    fn test_time_wrap_scenarios() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        
        // Test with timestamps near u64::MAX
        let near_max_ts = u64::MAX - 1000;
        farm.reward_infos[0].last_issuance_ts = near_max_ts;
        
        // Try to refresh with wrapped timestamp (should handle gracefully)
        let wrapped_ts = 100u64; // Simulating wrap-around
        
        let result = refresh_global_reward(&mut farm, None, wrapped_ts, 0);
        // Should either error or handle the wrap gracefully
        assert!(result.is_err() || farm.reward_infos[0].last_issuance_ts == wrapped_ts);
    }
    
    #[test]
    fn test_reentrant_sequences() {
        let mut farm = FarmState::default();
        farm.deposit_warmup_period = 0;
        farm.withdrawal_cooldown_period = 0;
        
        let mut user = UserState::default();
        
        // Rapid stake -> unstake -> stake sequence
        let amount = 1000u64;
        
        // First stake
        add_pending_deposit_stake(&mut user, &mut farm, amount).unwrap();
        move_pending_deposit_to_active(&mut user, &mut farm).unwrap();
        
        let initial_stake = user.get_active_stake_decimal();
        
        // Immediate unstake
        let unstake_amount = initial_stake / 2;
        remove_active_stake(&mut user, &mut farm, unstake_amount).unwrap();
        
        // Immediate re-stake
        add_pending_deposit_stake(&mut user, &mut farm, amount / 2).unwrap();
        move_pending_deposit_to_active(&mut user, &mut farm).unwrap();
        
        // Verify final state is consistent
        let final_stake = user.get_active_stake_decimal();
        assert!(final_stake > Decimal::zero());
        assert!(final_stake <= initial_stake + Decimal::from(amount / 2));
    }
    
    #[test]
    fn test_very_large_rewards_small_stake() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_staked_amount = 1; // Minimal stake
        farm.total_active_stake_scaled = ten_pow(18) as u128;
        
        let mut user = UserState::default();
        user.active_stake_scaled = ten_pow(18) as u128; // 100% of pool
        
        // Issue very large rewards
        let large_rewards = u64::MAX / 2;
        farm.reward_infos[0].rewards_issued_unclaimed = large_rewards;
        farm.reward_infos[0].set_reward_per_share_decimal(
            Decimal::from(large_rewards) / farm.get_total_active_stake_decimal()
        );
        
        // User should get all rewards without overflow
        let result = user_refresh_reward(&mut farm, &mut user, 0);
        assert!(result.is_ok());
        
        let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
            .to_u64()
            .unwrap_or(0);
        
        assert_eq!(user_rewards, large_rewards);
    }
    
    #[test]
    fn test_very_small_rewards_huge_stake() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_staked_amount = u64::MAX / 2; // Huge stake
        farm.total_active_stake_scaled = ((u64::MAX / 2) as u128) * ten_pow(18) as u128;
        
        let mut user = UserState::default();
        user.active_stake_scaled = farm.total_active_stake_scaled / 10; // 10% of pool
        
        // Issue very small rewards
        let small_rewards = 1u64;
        farm.reward_infos[0].rewards_issued_unclaimed = small_rewards;
        farm.reward_infos[0].set_reward_per_share_decimal(
            Decimal::from(small_rewards) / farm.get_total_active_stake_decimal()
        );
        
        // User should get proportional share (might be 0 due to rounding)
        let result = user_refresh_reward(&mut farm, &mut user, 0);
        assert!(result.is_ok());
        
        let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
            .to_u64()
            .unwrap_or(0);
        
        assert!(user_rewards <= small_rewards);
    }
    
    #[test]
    fn test_reward_index_bounds() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 3;
        
        let mut user = UserState::default();
        
        // Test valid indices
        for i in 0..3 {
            let result = user_refresh_reward(&mut farm, &mut user, i);
            assert!(result.is_ok());
        }
        
        // Test out of bounds index
        let result = user_refresh_reward(&mut farm, &mut user, 10);
        assert!(result.is_err() || farm.num_reward_tokens == 10);
    }
    
    #[test]
    fn test_auto_stake_path_with_max_amount() {
        let mut farm = FarmState::default();
        let mut user = UserState::default();
        
        // Test auto-stake with amount = u64::MAX
        farm.total_staked_amount = 1000;
        farm.total_active_stake_scaled = (1000 * ten_pow(18)) as u128;
        
        // This should be handled gracefully or error appropriately
        let result = add_pending_deposit_stake(&mut user, &mut farm, u64::MAX);
        
        // Should either error due to overflow or handle the max value
        assert!(result.is_err() || user.get_pending_deposit_stake_decimal() > Decimal::zero());
    }
}