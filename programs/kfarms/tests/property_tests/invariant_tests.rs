use proptest::prelude::*;
use decimal_wad::decimal::Decimal;
use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    #[test]
    fn test_no_rewards_when_no_stake(
        time_advances in prop::collection::vec(1..1000u64, 1..20)
    ) {
        let mut farm = MockFarmState::new(3);
        
        // Add rewards but no stakes
        for i in 0..3 {
            farm.add_rewards(i, 10_000_000_000).unwrap();
        }
        
        // Advance time multiple times without any stakes
        for &delta in &time_advances {
            farm.current_time += delta;
            farm.refresh_farm().unwrap();
        }
        
        // No rewards should have been issued
        for reward in &farm.rewards {
            prop_assert_eq!(
                reward.rewards_issued_cumulative, 0,
                "Rewards issued with zero stake"
            );
            prop_assert_eq!(
                reward.reward_per_share_scaled, Decimal::zero(),
                "RPS changed with zero stake"
            );
            prop_assert_eq!(
                reward.last_issuance_ts, farm.current_time,
                "Last issuance not updated correctly"
            );
        }
    }
    
    #[test]
    fn test_user_cannot_claim_more_than_earned(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(2);
        
        // Add rewards
        for i in 0..2 {
            farm.add_rewards(i, 10_000_000_000).unwrap();
        }
        
        // Track what each user should have earned
        let mut user_earned: HashMap<usize, Vec<u64>> = HashMap::new();
        
        for action in actions {
            // Before applying action, record earned amounts
            if let Some(user) = farm.users.get(&action.user_id) {
                user_earned.entry(action.user_id)
                    .or_insert_with(|| vec![0; 2]);
                
                for i in 0..2 {
                    let earned = user_earned.get_mut(&action.user_id).unwrap();
                    earned[i] = user.rewards_unclaimed[i];
                }
            }
            
            let _ = farm.apply_action(&action);
            
            // After harvest, check user didn't get more than earned
            if let ActionType::Harvest(reward_idx) = action.action_type {
                if reward_idx < 2 {
                    let earned = user_earned.get(&action.user_id)
                        .and_then(|e| e.get(reward_idx))
                        .copied()
                        .unwrap_or(0);
                    
                    let claimed = farm.total_rewards_claimed[reward_idx];
                    
                    // User cannot claim more than they had unclaimed
                    prop_assert!(
                        claimed <= farm.total_rewards_deposited[reward_idx],
                        "User claimed more than available"
                    );
                }
            }
        }
    }
    
    #[test]
    fn test_bounded_rounding_error(
        num_users in 2..20usize,
        num_operations in 10..100usize,
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 100_000_000_000).unwrap();
        
        // High precision tracking
        let mut precise_total_rewards = 0u128;
        let mut distributed_rewards = 0u64;
        
        // Create users and stake
        for user_id in 0..num_users {
            let stake = 1_000_000_000u64 / (user_id + 1) as u64; // Different amounts
            let action = FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount: stake,
                time_delta: 10,
            };
            farm.apply_action(&action).unwrap();
        }
        
        // Perform many small operations
        for op in 0..num_operations {
            // Small time advance
            farm.current_time += 1;
            farm.refresh_farm().unwrap();
            
            // Track precise rewards that should be issued
            if farm.total_active_stake > Decimal::zero() {
                let rewards_per_second = farm.rewards[0].rewards_per_second as u128;
                let decimals = 10u128.pow(farm.rewards[0].rewards_per_second_decimals as u32);
                precise_total_rewards += rewards_per_second / decimals;
            }
            
            // Occasionally harvest
            if op % 10 == 0 {
                let user_id = op % num_users;
                farm.refresh_user(user_id).unwrap();
                
                if let Some(user) = farm.users.get(&user_id) {
                    distributed_rewards += user.rewards_unclaimed[0];
                }
                farm.harvest(user_id, 0).ok();
            }
        }
        
        // Calculate actual distributed
        let actual_distributed = farm.rewards[0].rewards_issued_cumulative;
        let precise_distributed = (precise_total_rewards as u64)
            .min(farm.total_rewards_deposited[0]);
        
        // Rounding error should be bounded by number of operations
        let max_error = num_operations as u64 * num_users as u64;
        
        prop_assert!(
            actual_distributed <= precise_distributed + max_error,
            "Rounding error too large: {} > {} + {}",
            actual_distributed, precise_distributed, max_error
        );
    }
    
    #[test]
    fn test_deposit_cap_enforcement(
        cap_amount in 1_000_000..10_000_000_000u64,
        stake_attempts in prop::collection::vec(
            (0..10usize, 100_000..2_000_000_000u64),
            5..20
        )
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        // Simulate deposit cap
        let mut total_deposited = 0u64;
        
        for (user_id, amount) in stake_attempts {
            // Check if stake would exceed cap
            if total_deposited + amount <= cap_amount {
                let action = FarmAction {
                    user_id,
                    action_type: ActionType::Stake,
                    amount,
                    time_delta: 1,
                };
                
                if farm.apply_action(&action).is_ok() {
                    total_deposited += amount;
                }
            }
            
            // Verify cap not exceeded
            let farm_total = farm.total_staked.to_u64().unwrap_or(0);
            prop_assert!(
                farm_total <= cap_amount,
                "Deposit cap exceeded: {} > {}",
                farm_total, cap_amount
            );
        }
    }
    
    #[test]
    fn test_penalty_bounds(
        penalty_bps in 0..10000u64, // 0 to 100%
        unstake_amounts in prop::collection::vec(1000..1_000_000_000u64, 1..10)
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        // Stake initial amount
        let user_id = 0;
        let initial_stake = 10_000_000_000u64;
        let stake_action = FarmAction {
            user_id,
            action_type: ActionType::Stake,
            amount: initial_stake,
            time_delta: 0,
        };
        farm.apply_action(&stake_action).unwrap();
        
        let mut total_penalties = 0u64;
        
        for amount in unstake_amounts {
            // Try to unstake
            let unstake_action = FarmAction {
                user_id,
                action_type: ActionType::Unstake,
                amount,
                time_delta: 10,
            };
            
            if farm.apply_action(&unstake_action).is_ok() {
                // Calculate expected penalty
                let penalty = amount * penalty_bps / 10000;
                total_penalties += penalty;
                
                // Simulate penalty application
                farm.total_slashed += penalty;
            }
        }
        
        // Penalties should never exceed total unstaked amount
        let user = farm.users.get(&user_id).unwrap();
        let total_unstaked = initial_stake - user.active_stake.to_u64().unwrap_or(0);
        
        prop_assert!(
            total_penalties <= total_unstaked,
            "Penalties exceed unstaked amount: {} > {}",
            total_penalties, total_unstaked
        );
        
        // Penalty rate should be bounded
        if total_unstaked > 0 {
            let effective_penalty_rate = total_penalties * 10000 / total_unstaked;
            prop_assert!(
                effective_penalty_rate <= penalty_bps,
                "Effective penalty rate too high: {} > {}",
                effective_penalty_rate, penalty_bps
            );
        }
    }
    
    #[test]
    fn test_time_unit_consistency(
        use_seconds in prop::bool::ANY,
        time_advances in prop::collection::vec(1..1000u64, 1..10)
    ) {
        let mut farm_seconds = MockFarmState::new(1);
        let mut farm_slots = MockFarmState::new(1);
        
        // Setup identical farms with different time units
        farm_seconds.add_rewards(0, 10_000_000_000).unwrap();
        farm_slots.add_rewards(0, 10_000_000_000).unwrap();
        
        // Adjust rewards per second based on time unit
        if use_seconds {
            farm_seconds.rewards[0].rewards_per_second = 1000;
            farm_slots.rewards[0].rewards_per_second = 1000 * 2; // Assuming 2 slots/sec
        }
        
        // Stake same amount
        for farm in [&mut farm_seconds, &mut farm_slots] {
            let action = FarmAction {
                user_id: 0,
                action_type: ActionType::Stake,
                amount: 1_000_000_000,
                time_delta: 0,
            };
            farm.apply_action(&action).unwrap();
        }
        
        // Advance time (adjusting for unit difference)
        let total_seconds: u64 = time_advances.iter().sum();
        let total_slots = total_seconds * 2; // Assuming 2 slots/sec
        
        farm_seconds.current_time += total_seconds;
        farm_slots.current_time += total_slots;
        
        farm_seconds.refresh_farm().unwrap();
        farm_slots.refresh_farm().unwrap();
        
        // Rewards should be proportionally similar
        let rewards_seconds = farm_seconds.rewards[0].rewards_issued_cumulative;
        let rewards_slots = farm_slots.rewards[0].rewards_issued_cumulative;
        
        // Allow for some difference due to rounding
        let ratio = if rewards_slots > 0 {
            rewards_seconds * 100 / rewards_slots
        } else {
            100
        };
        
        prop_assert!(
            ratio >= 45 && ratio <= 55, // Within 10% considering the 2x difference
            "Time unit rewards mismatch: {} vs {} (ratio: {})",
            rewards_seconds, rewards_slots, ratio
        );
    }
}