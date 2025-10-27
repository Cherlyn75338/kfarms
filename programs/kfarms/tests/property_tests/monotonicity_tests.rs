use proptest::prelude::*;
use decimal_wad::decimal::Decimal;
use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    #[test]
    fn test_reward_per_share_monotonicity(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(3);
        
        // Add initial rewards
        for i in 0..3 {
            farm.add_rewards(i, 10_000_000_000).unwrap();
        }
        
        let mut previous_rps = vec![Decimal::zero(); 3];
        
        for action in actions {
            // Record RPS before action
            for (i, reward) in farm.rewards.iter().enumerate() {
                previous_rps[i] = reward.reward_per_share_scaled;
            }
            
            let _ = farm.apply_action(&action);
            
            // Check RPS never decreased
            for (i, reward) in farm.rewards.iter().enumerate() {
                prop_assert!(
                    reward.reward_per_share_scaled >= previous_rps[i],
                    "RPS decreased for reward {}: {:?} < {:?}",
                    i, reward.reward_per_share_scaled, previous_rps[i]
                );
            }
        }
        
        prop_assert!(farm.check_monotonicity_invariant());
    }
    
    #[test]
    fn test_cumulative_rewards_monotonicity(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(2);
        
        // Setup different reward types
        farm.rewards[0].reward_type = RewardType::Proportional;
        farm.rewards[1].reward_type = RewardType::Constant;
        
        for i in 0..2 {
            farm.add_rewards(i, 10_000_000_000).unwrap();
        }
        
        let mut prev_cumulative = vec![0u64; 2];
        
        for action in actions {
            // Record cumulative before
            for (i, reward) in farm.rewards.iter().enumerate() {
                prev_cumulative[i] = reward.rewards_issued_cumulative;
            }
            
            let _ = farm.apply_action(&action);
            
            // Check cumulative never decreased
            for (i, reward) in farm.rewards.iter().enumerate() {
                prop_assert!(
                    reward.rewards_issued_cumulative >= prev_cumulative[i],
                    "Cumulative rewards decreased for reward {}: {} < {}",
                    i, reward.rewards_issued_cumulative, prev_cumulative[i]
                );
            }
        }
    }
    
    #[test]
    fn test_user_earned_monotonicity_until_harvest(
        user_actions in prop::collection::vec(
            (0..1000u64, prop::bool::ANY),
            1..50
        )
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        let user_id = 0;
        let mut prev_unclaimed = 0u64;
        
        // Initial stake
        let stake_action = FarmAction {
            user_id,
            action_type: ActionType::Stake,
            amount: 1_000_000_000,
            time_delta: 0,
        };
        farm.apply_action(&stake_action).unwrap();
        
        for (time_delta, should_harvest) in user_actions {
            // Advance time and refresh
            farm.current_time += time_delta;
            farm.refresh_farm().unwrap();
            farm.refresh_user(user_id).unwrap();
            
            let user = farm.users.get(&user_id).unwrap();
            let current_unclaimed = user.rewards_unclaimed[0];
            
            if should_harvest {
                // Harvest resets unclaimed
                farm.harvest(user_id, 0).unwrap();
                prev_unclaimed = 0;
            } else {
                // Without harvest, unclaimed should not decrease
                prop_assert!(
                    current_unclaimed >= prev_unclaimed,
                    "User unclaimed rewards decreased without harvest: {} < {}",
                    current_unclaimed, prev_unclaimed
                );
                prev_unclaimed = current_unclaimed;
            }
        }
    }
    
    #[test]
    fn test_last_issuance_timestamp_monotonicity(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        let mut prev_ts = vec![0u64; farm.rewards.len()];
        
        for action in actions {
            // Record timestamps before
            for (i, reward) in farm.rewards.iter().enumerate() {
                prev_ts[i] = reward.last_issuance_ts;
            }
            
            let _ = farm.apply_action(&action);
            
            // Check timestamps never go backwards
            for (i, reward) in farm.rewards.iter().enumerate() {
                prop_assert!(
                    reward.last_issuance_ts >= prev_ts[i],
                    "Last issuance timestamp went backwards for reward {}: {} < {}",
                    i, reward.last_issuance_ts, prev_ts[i]
                );
            }
            
            // Also check against current time
            for reward in &farm.rewards {
                prop_assert!(
                    reward.last_issuance_ts <= farm.current_time,
                    "Last issuance timestamp in future: {} > {}",
                    reward.last_issuance_ts, farm.current_time
                );
            }
        }
    }
    
    #[test]
    fn test_available_rewards_decrease_monotonicity(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(2);
        
        // Add initial rewards
        let initial_amounts = vec![5_000_000_000, 3_000_000_000];
        for (i, &amount) in initial_amounts.iter().enumerate() {
            farm.add_rewards(i, amount).unwrap();
        }
        
        let mut prev_available = initial_amounts.clone();
        let mut rewards_added = vec![0u64; 2];
        
        for action in actions {
            // Track if we're adding rewards
            if let ActionType::AddRewards(idx, amount) = action.action_type {
                if idx < 2 {
                    rewards_added[idx] += amount;
                }
            }
            
            let _ = farm.apply_action(&action);
            
            // Check available rewards
            for (i, reward) in farm.rewards.iter().enumerate() {
                // Available can only increase via AddRewards
                let expected_max = prev_available[i] + rewards_added[i];
                prop_assert!(
                    reward.rewards_available <= expected_max,
                    "Available rewards increased unexpectedly for reward {}: {} > {}",
                    i, reward.rewards_available, expected_max
                );
                
                prev_available[i] = reward.rewards_available;
            }
            
            // Reset tracking after check
            rewards_added = vec![0u64; 2];
        }
    }
    
    #[test]
    fn test_total_stake_bounds(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        let mut max_total_stake = Decimal::zero();
        
        for action in actions {
            let _ = farm.apply_action(&action);
            
            // Track maximum
            if farm.total_active_stake > max_total_stake {
                max_total_stake = farm.total_active_stake;
            }
            
            // Total active stake should never exceed total staked
            prop_assert!(
                farm.total_active_stake <= farm.total_staked,
                "Active stake exceeds total stake: {:?} > {:?}",
                farm.total_active_stake, farm.total_staked
            );
            
            // Should never be negative (Decimal should prevent this)
            prop_assert!(
                farm.total_active_stake >= Decimal::zero(),
                "Active stake is negative: {:?}",
                farm.total_active_stake
            );
        }
    }
}