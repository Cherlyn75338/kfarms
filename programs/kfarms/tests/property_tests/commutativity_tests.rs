use proptest::prelude::*;
use super::*;
use std::collections::HashSet;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    
    #[test]
    fn test_stake_order_commutativity_same_slot(
        user1_amount in 1000..1_000_000_000u64,
        user2_amount in 1000..1_000_000_000u64,
        user3_amount in 1000..1_000_000_000u64,
    ) {
        // Test that order of stakes in same slot doesn't affect final state
        let mut farm1 = MockFarmState::new(1);
        let mut farm2 = MockFarmState::new(1);
        
        // Add same initial rewards
        farm1.add_rewards(0, 10_000_000_000).unwrap();
        farm2.add_rewards(0, 10_000_000_000).unwrap();
        
        // Farm 1: user1, user2, user3
        let actions1 = vec![
            FarmAction {
                user_id: 1,
                action_type: ActionType::Stake,
                amount: user1_amount,
                time_delta: 0, // Same slot
            },
            FarmAction {
                user_id: 2,
                action_type: ActionType::Stake,
                amount: user2_amount,
                time_delta: 0, // Same slot
            },
            FarmAction {
                user_id: 3,
                action_type: ActionType::Stake,
                amount: user3_amount,
                time_delta: 0, // Same slot
            },
        ];
        
        // Farm 2: user3, user1, user2 (different order)
        let actions2 = vec![
            FarmAction {
                user_id: 3,
                action_type: ActionType::Stake,
                amount: user3_amount,
                time_delta: 0,
            },
            FarmAction {
                user_id: 1,
                action_type: ActionType::Stake,
                amount: user1_amount,
                time_delta: 0,
            },
            FarmAction {
                user_id: 2,
                action_type: ActionType::Stake,
                amount: user2_amount,
                time_delta: 0,
            },
        ];
        
        // Apply actions
        for action in actions1 {
            farm1.apply_action(&action).unwrap();
        }
        for action in actions2 {
            farm2.apply_action(&action).unwrap();
        }
        
        // Advance time to accumulate rewards
        farm1.current_time += 1000;
        farm2.current_time += 1000;
        farm1.refresh_farm().unwrap();
        farm2.refresh_farm().unwrap();
        
        // Final states should be identical
        prop_assert_eq!(
            farm1.total_active_stake,
            farm2.total_active_stake,
            "Total active stake mismatch"
        );
        
        prop_assert_eq!(
            farm1.rewards[0].reward_per_share_scaled,
            farm2.rewards[0].reward_per_share_scaled,
            "RPS mismatch"
        );
        
        // Check each user's state
        for user_id in [1, 2, 3] {
            farm1.refresh_user(user_id).unwrap();
            farm2.refresh_user(user_id).unwrap();
            
            let user1 = farm1.users.get(&user_id).unwrap();
            let user2 = farm2.users.get(&user_id).unwrap();
            
            prop_assert_eq!(
                user1.active_stake,
                user2.active_stake,
                "User {} stake mismatch", user_id
            );
        }
    }
    
    #[test]
    fn test_harvest_order_commutativity_same_slot(
        stake_amounts in prop::collection::vec(1000..1_000_000_000u64, 3..5),
        harvest_order1 in prop::collection::vec(0..5usize, 3..5),
        harvest_order2 in prop::collection::vec(0..5usize, 3..5),
    ) {
        // Filter to valid user indices
        let num_users = stake_amounts.len();
        let harvest_order1: Vec<usize> = harvest_order1.into_iter()
            .filter(|&i| i < num_users)
            .collect();
        let harvest_order2: Vec<usize> = harvest_order2.into_iter()
            .filter(|&i| i < num_users)
            .collect();
        
        if harvest_order1.is_empty() || harvest_order2.is_empty() {
            return Ok(());
        }
        
        // Ensure both orders contain same users (just different order)
        let set1: HashSet<_> = harvest_order1.iter().collect();
        let set2: HashSet<_> = harvest_order2.iter().collect();
        if set1 != set2 {
            return Ok(()); // Skip if different users
        }
        
        let mut farm1 = MockFarmState::new(1);
        let mut farm2 = MockFarmState::new(1);
        
        // Setup identical initial state
        farm1.add_rewards(0, 10_000_000_000).unwrap();
        farm2.add_rewards(0, 10_000_000_000).unwrap();
        
        // All users stake
        for (user_id, &amount) in stake_amounts.iter().enumerate() {
            let action = FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount,
                time_delta: 0,
            };
            farm1.apply_action(&action).unwrap();
            farm2.apply_action(&action.clone()).unwrap();
        }
        
        // Accumulate rewards
        farm1.current_time += 1000;
        farm2.current_time += 1000;
        farm1.refresh_farm().unwrap();
        farm2.refresh_farm().unwrap();
        
        // Harvest in different orders
        for &user_id in &harvest_order1 {
            farm1.refresh_user(user_id).unwrap();
            farm1.harvest(user_id, 0).unwrap();
        }
        
        for &user_id in &harvest_order2 {
            farm2.refresh_user(user_id).unwrap();
            farm2.harvest(user_id, 0).unwrap();
        }
        
        // Total claimed should be the same
        prop_assert_eq!(
            farm1.total_rewards_claimed[0],
            farm2.total_rewards_claimed[0],
            "Total claimed mismatch"
        );
        
        // Remaining unclaimed should be the same
        prop_assert_eq!(
            farm1.rewards[0].rewards_issued_unclaimed,
            farm2.rewards[0].rewards_issued_unclaimed,
            "Unclaimed mismatch"
        );
    }
    
    #[test]
    fn test_mixed_operations_partial_commutativity(
        base_stake in 1_000_000..10_000_000_000u64,
        operations in prop::collection::vec(
            (0..3usize, 1000..1_000_000u64, prop::bool::ANY),
            5..10
        )
    ) {
        // Test that independent user operations commute
        let mut farm1 = MockFarmState::new(1);
        let mut farm2 = MockFarmState::new(1);
        
        farm1.add_rewards(0, 100_000_000_000).unwrap();
        farm2.add_rewards(0, 100_000_000_000).unwrap();
        
        // Initial stake for all users
        for user_id in 0..3 {
            let action = FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount: base_stake,
                time_delta: 0,
            };
            farm1.apply_action(&action).unwrap();
            farm2.apply_action(&action.clone()).unwrap();
        }
        
        // Collect operations by user
        let mut user_ops: Vec<Vec<(u64, bool)>> = vec![vec![], vec![], vec![]];
        for (user_id, amount, is_unstake) in operations {
            if user_id < 3 {
                user_ops[user_id].push((amount, is_unstake));
            }
        }
        
        // Farm1: apply in user order (0, 1, 2)
        for (user_id, ops) in user_ops.iter().enumerate() {
            for &(amount, is_unstake) in ops {
                let action = FarmAction {
                    user_id,
                    action_type: if is_unstake {
                        ActionType::Unstake
                    } else {
                        ActionType::Stake
                    },
                    amount,
                    time_delta: 0,
                };
                let _ = farm1.apply_action(&action);
            }
        }
        
        // Farm2: apply in reverse user order (2, 1, 0)
        for (user_id, ops) in user_ops.iter().enumerate().rev() {
            for &(amount, is_unstake) in ops {
                let action = FarmAction {
                    user_id,
                    action_type: if is_unstake {
                        ActionType::Unstake
                    } else {
                        ActionType::Stake
                    },
                    amount,
                    time_delta: 0,
                };
                let _ = farm2.apply_action(&action);
            }
        }
        
        // Final state should be the same
        prop_assert_eq!(
            farm1.total_active_stake,
            farm2.total_active_stake,
            "Total stake mismatch after reordering"
        );
        
        // Each user's final state should match
        for user_id in 0..3 {
            if let (Some(user1), Some(user2)) = (
                farm1.users.get(&user_id),
                farm2.users.get(&user_id)
            ) {
                prop_assert_eq!(
                    user1.active_stake,
                    user2.active_stake,
                    "User {} stake mismatch", user_id
                );
                
                prop_assert_eq!(
                    user1.pending_withdrawal,
                    user2.pending_withdrawal,
                    "User {} pending withdrawal mismatch", user_id
                );
            }
        }
    }
    
    #[test]
    fn test_refresh_operations_commutativity(
        num_refreshes in 1..10usize,
        time_deltas in prop::collection::vec(1..100u64, 1..10)
    ) {
        let mut farm1 = MockFarmState::new(1);
        let mut farm2 = MockFarmState::new(1);
        
        // Setup
        farm1.add_rewards(0, 10_000_000_000).unwrap();
        farm2.add_rewards(0, 10_000_000_000).unwrap();
        
        // Add some stakes
        for user_id in 0..3 {
            let action = FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount: 1_000_000_000,
                time_delta: 0,
            };
            farm1.apply_action(&action).unwrap();
            farm2.apply_action(&action.clone()).unwrap();
        }
        
        // Farm1: Multiple refreshes with time advances
        for &delta in &time_deltas {
            farm1.current_time += delta;
            for _ in 0..num_refreshes {
                farm1.refresh_farm().unwrap();
            }
        }
        
        // Farm2: Single refresh with total time advance
        let total_time: u64 = time_deltas.iter().sum();
        farm2.current_time += total_time;
        farm2.refresh_farm().unwrap();
        
        // Results should be the same (multiple refreshes = single refresh)
        prop_assert_eq!(
            farm1.rewards[0].rewards_issued_cumulative,
            farm2.rewards[0].rewards_issued_cumulative,
            "Cumulative rewards mismatch"
        );
        
        prop_assert_eq!(
            farm1.rewards[0].reward_per_share_scaled,
            farm2.rewards[0].reward_per_share_scaled,
            "RPS mismatch after refresh consolidation"
        );
    }
}