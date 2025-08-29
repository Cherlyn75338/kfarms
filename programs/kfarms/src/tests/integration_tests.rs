#[cfg(test)]
mod integration_tests {
    use crate::state::{FarmState, UserState, RewardInfo, LockingMode};
    use crate::farm_operations::{refresh_global_rewards, user_refresh_reward, harvest, withdraw_unstaked_deposits};
    use crate::farm_operations::unstake as farm_unstake;
    use crate::stake_operations::{add_pending_deposit_stake, move_pending_deposit_to_active, remove_active_stake, convert_stake_to_amount, convert_amount_to_stake};
    use crate::utils::math::ten_pow;
    use decimal_wad::decimal::Decimal;
    use scope::DatedPrice;
    
    fn setup_farm_and_users(num_users: usize) -> (FarmState, Vec<UserState>) {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 2; // Multiple reward tokens
        farm.deposit_warmup_period = 10;
        farm.withdrawal_cooldown_period = 20;
        farm.locking_mode = LockingMode::Continuous as u8;
        farm.locking_duration = 100;
        farm.locking_early_withdrawal_penalty_bps = 5000; // 50%
        
        // Setup reward tokens
        for i in 0..2 {
            farm.reward_infos[i].rewards_available = 1_000_000;
            farm.reward_infos[i].reward_schedule_curve.points[0].rewards_per_second = 10;
            farm.reward_infos[i].reward_schedule_curve.points[0].ts_start = 0;
            farm.reward_infos[i].reward_schedule_curve.points[0].ts_end = 10000;
        }
        
        let mut users = Vec::new();
        for i in 0..num_users {
            let mut user = UserState::default();
            user.user_id = i as u64;
            users.push(user);
        }
        
        (farm, users)
    }
    
    #[test]
    fn test_full_lifecycle_single_user() {
        let (mut farm, mut users) = setup_farm_and_users(1);
        let mut user = &mut users[0];
        let mut timestamp = 0u64;
        
        // Step 1: Deposit
        let deposit_amount = 10000u64;
        add_pending_deposit_stake(user, &mut farm, deposit_amount).unwrap();
        assert_eq!(farm.total_pending_amount, deposit_amount);
        
        // Step 2: Wait for warmup and activate
        timestamp += farm.deposit_warmup_period as u64 + 1;
        user.pending_deposit_stake_ts = 0; // Simulate warmup passed
        move_pending_deposit_to_active(user, &mut farm).unwrap();
        assert_eq!(farm.total_staked_amount, deposit_amount);
        assert_eq!(farm.total_pending_amount, 0);
        
        // Step 3: Accrue rewards
        timestamp += 100;
        refresh_global_rewards(&mut farm, None, timestamp).unwrap();
        assert!(farm.reward_infos[0].rewards_issued_unclaimed > 0);
        assert!(farm.reward_infos[1].rewards_issued_unclaimed > 0);
        
        // Step 4: Refresh user rewards
        for i in 0..2 {
            user_refresh_reward(&mut farm, user, i).unwrap();
        }
        assert!(user.reward_infos[0].rewards_earned_scaled > 0);
        assert!(user.reward_infos[1].rewards_earned_scaled > 0);
        
        // Step 5: Harvest rewards
        let harvest_result_0 = harvest(&mut farm, user, None, timestamp, 0).unwrap();
        assert!(harvest_result_0.reward_amount > 0);
        assert_eq!(user.reward_infos[0].rewards_earned_scaled, 0);
        
        // Step 6: Partial unstake with penalty
        user.last_stake_ts = timestamp - 50; // Halfway through lock
        let unstake_amount = Decimal::from(5000u64);
        timestamp += 1;
        
        let unstake_result = farm_unstake(
            user,
            &mut farm,
            unstake_amount,
            timestamp
        ).unwrap();
        
        assert!(unstake_result.token_amount_penalty > 0); // Should have penalty
        assert!(user.pending_withdrawal_unstake_scaled > 0);
        
        // Step 7: Wait for cooldown and withdraw
        timestamp += farm.withdrawal_cooldown_period as u64 + 1;
        user.pending_withdrawal_unstake_ts = timestamp - farm.withdrawal_cooldown_period as u64 - 1;
        
        let withdraw_result = withdraw_unstaked_deposits(&mut farm, user, timestamp).unwrap();
        assert!(withdraw_result.amount_to_withdraw > 0);
        assert_eq!(user.pending_withdrawal_unstake_scaled, 0);
        
        // Step 8: Final state verification
        assert!(farm.total_staked_amount < deposit_amount); // Some withdrawn
        assert!(user.get_active_stake_decimal() > Decimal::zero()); // Some still staked
    }
    
    #[test]
    fn test_multi_user_reward_distribution() {
        let (mut farm, mut users) = setup_farm_and_users(10);
        let mut timestamp = 0u64;
        
        // All users deposit different amounts
        for (i, user) in users.iter_mut().enumerate() {
            let amount = (i + 1) as u64 * 1000; // 1000, 2000, ..., 10000
            add_pending_deposit_stake(user, &mut farm, amount).unwrap();
            move_pending_deposit_to_active(user, &mut farm).unwrap();
        }
        
        // Generate rewards
        timestamp += 1000;
        refresh_global_rewards(&mut farm, None, timestamp).unwrap();
        
        let total_rewards_issued = farm.reward_infos[0].rewards_issued_unclaimed;
        assert!(total_rewards_issued > 0);
        
        // Distribute rewards to all users
        let mut total_user_rewards = 0u64;
        for user in users.iter_mut() {
            user_refresh_reward(&mut farm, user, 0).unwrap();
            let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
                .to_u64()
                .unwrap_or(0);
            total_user_rewards += user_rewards;
        }
        
        // Verify fair distribution
        let distribution_error = if total_user_rewards > total_rewards_issued {
            total_user_rewards - total_rewards_issued
        } else {
            total_rewards_issued - total_user_rewards
        };
        
        assert!(
            distribution_error <= users.len() as u64,
            "Reward distribution error too large: {}",
            distribution_error
        );
        
        // Verify proportional distribution
        for (i, user) in users.iter().enumerate() {
            let user_stake = user.get_active_stake_decimal();
            let farm_stake = farm.get_total_active_stake_decimal();
            let user_share = user_stake / farm_stake;
            
            let user_rewards = user.reward_infos[0].get_rewards_earned_decimal();
            let expected_rewards = Decimal::from(total_rewards_issued) * user_share;
            
            let reward_diff = if user_rewards > expected_rewards {
                user_rewards - expected_rewards
            } else {
                expected_rewards - user_rewards
            };
            
            assert!(
                reward_diff < Decimal::from(10u64),
                "User {} reward distribution not proportional",
                i
            );
        }
    }
    
    #[test]
    fn test_concurrent_operations() {
        let (mut farm, mut users) = setup_farm_and_users(5);
        let mut timestamp = 0u64;
        
        // Simulate concurrent operations
        // User 0: Deposit
        add_pending_deposit_stake(&mut users[0], &mut farm, 5000).unwrap();
        
        // User 1: Already staked, now unstaking
        users[1].active_stake_scaled = 3000 * ten_pow(18) as u128;
        farm.total_staked_amount += 3000;
        farm.total_active_stake_scaled += 3000 * ten_pow(18) as u128;
        users[1].last_stake_ts = timestamp;
        timestamp += 50;
        
        farm_unstake(&mut users[1], &mut farm, Decimal::from(1500u64), timestamp).unwrap();
        
        // User 2: Harvesting rewards
        users[2].active_stake_scaled = 2000 * ten_pow(18) as u128;
        users[2].reward_infos[0].rewards_earned_scaled = 100 * ten_pow(18) as u128;
        farm.reward_infos[0].rewards_issued_unclaimed = 100;
        
        harvest(&mut farm, &mut users[2], None, timestamp, 0).unwrap();
        
        // User 3: Moving pending to active
        users[3].pending_deposit_stake_scaled = 1000 * ten_pow(18) as u128;
        farm.total_pending_amount = 1000;
        farm.total_pending_stake_scaled = 1000 * ten_pow(18) as u128;
        
        move_pending_deposit_to_active(&mut users[3], &mut farm).unwrap();
        
        // User 4: Withdrawing after cooldown
        users[4].pending_withdrawal_unstake_scaled = 500 * ten_pow(18) as u128;
        users[4].pending_withdrawal_unstake_ts = timestamp - 30;
        farm.total_pending_amount += 500;
        farm.total_pending_stake_scaled += 500 * ten_pow(18) as u128;
        
        withdraw_unstaked_deposits(&mut farm, &mut users[4], timestamp).unwrap();
        
        // Verify farm state consistency
        assert!(farm.total_staked_amount > 0);
        assert!(farm.total_active_stake_scaled > 0);
        
        // Verify no negative values
        for user in &users {
            assert!(user.active_stake_scaled < u128::MAX / 2);
            assert!(user.pending_deposit_stake_scaled < u128::MAX / 2);
            assert!(user.pending_withdrawal_unstake_scaled < u128::MAX / 2);
        }
    }
    
    #[test]
    fn test_oracle_integration() {
        let (mut farm, mut users) = setup_farm_and_users(1);
        farm.scope_oracle_price_id = 1; // Enable oracle
        farm.scope_oracle_max_age = 100;
        
        let mut timestamp = 1000u64;
        
        // Create price feed
        let price = scope::DatedPrice {
            price: scope::Price {
                value: 100_000_000, // $100 with 6 decimals
                exp: 6,
            },
            unix_timestamp: (timestamp - 10) as i64,
            ..Default::default()
        };
        
        // Deposit with oracle check
        let deposit_amount = 1000u64;
        let deposit_check = farm.can_accept_deposit(deposit_amount, Some(price), timestamp);
        assert!(deposit_check.is_ok());
        
        add_pending_deposit_stake(&mut users[0], &mut farm, deposit_amount).unwrap();
        move_pending_deposit_to_active(&mut users[0], &mut farm).unwrap();
        
        // Refresh rewards with oracle price
        timestamp += 100;
        let price_new = scope::DatedPrice {
            price: scope::Price {
                value: 110_000_000, // Price increased to $110
                exp: 6,
            },
            unix_timestamp: (timestamp - 5) as i64,
            ..Default::default()
        };
        
        refresh_global_rewards(&mut farm, Some(price_new), timestamp).unwrap();
        
        // Rewards should be adjusted by oracle price
        assert!(farm.reward_infos[0].rewards_issued_unclaimed > 0);
    }
    
    #[test]
    fn test_edge_case_integration() {
        let (mut farm, mut users) = setup_farm_and_users(3);
        
        // Test 1: Empty pool first depositor
        assert_eq!(farm.total_staked_amount, 0);
        add_pending_deposit_stake(&mut users[0], &mut farm, 1).unwrap(); // 1 wei
        move_pending_deposit_to_active(&mut users[0], &mut farm).unwrap();
        assert_eq!(users[0].get_active_stake_decimal(), Decimal::from(1u64));
        
        // Test 2: Large deposit after small
        add_pending_deposit_stake(&mut users[1], &mut farm, u64::MAX / 2).unwrap();
        move_pending_deposit_to_active(&mut users[1], &mut farm).unwrap();
        
        // Test 3: Rewards with extreme stake differences
        refresh_global_rewards(&mut farm, None, 1000).unwrap();
        
        for i in 0..2 {
            user_refresh_reward(&mut farm, &mut users[i], 0).unwrap();
        }
        
        // User with 1 wei should get almost no rewards
        let small_rewards = users[0].reward_infos[0].get_rewards_earned_decimal();
        let large_rewards = users[1].reward_infos[0].get_rewards_earned_decimal();
        
        assert!(large_rewards > small_rewards * 1000);
    }
    
    #[test]
    fn test_complex_penalty_scenarios() {
        let (mut farm, mut users) = setup_farm_and_users(1);
        farm.locking_mode = LockingMode::Continuous as u8;
        farm.locking_duration = 1000;
        farm.locking_early_withdrawal_penalty_bps = 9000; // 90% penalty
        
        let mut timestamp = 0u64;
        
        // Deposit and activate
        add_pending_deposit_stake(&mut users[0], &mut farm, 10000).unwrap();
        move_pending_deposit_to_active(&mut users[0], &mut farm).unwrap();
        users[0].last_stake_ts = timestamp;
        
        // Try immediate withdrawal (maximum penalty)
        timestamp += 1;
        let result = farm_unstake(&mut users[0], &mut farm, Decimal::from(5000u64), timestamp);
        assert!(result.is_ok());
        
        let unstake_effects = result.unwrap();
        assert!(unstake_effects.token_amount_penalty >= 4400); // ~90% penalty
        
        // Wait partial duration and try again
        timestamp += 500; // 50% through lock period
        users[0].last_stake_ts = timestamp - 500;
        
        let result = farm_unstake(&mut users[0], &mut farm, Decimal::from(2500u64), timestamp);
        assert!(result.is_ok());
        
        let unstake_effects = result.unwrap();
        assert!(unstake_effects.token_amount_penalty < 2250); // ~45% penalty
        assert!(unstake_effects.token_amount_penalty > 1000); // But still significant
    }
}