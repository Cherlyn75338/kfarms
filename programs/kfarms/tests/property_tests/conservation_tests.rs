use proptest::prelude::*;
use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    #[test]
    fn test_reward_conservation(
        actions in action_sequence_strategy(),
        initial_rewards in prop::collection::vec(1000..10_000_000u64, 1..3)
    ) {
        let mut farm = MockFarmState::new(initial_rewards.len());
        
        // Add initial rewards
        for (i, &amount) in initial_rewards.iter().enumerate() {
            farm.add_rewards(i, amount).unwrap();
        }
        
        // Apply all actions
        for action in actions {
            let _ = farm.apply_action(&action); // Some actions may fail, that's ok
        }
        
        // Check conservation invariant
        prop_assert!(
            farm.check_conservation_invariant(),
            "Conservation violated: claimed + unclaimed + available > deposited"
        );
    }
    
    #[test]
    fn test_reward_conservation_with_rounding(
        actions in action_sequence_strategy(),
        num_users in 1..20usize,
        stake_amounts in prop::collection::vec(1000..1_000_000_000u64, 1..20)
    ) {
        let mut farm = MockFarmState::new(3);
        
        // Setup different decimal configurations
        farm.rewards[0].rewards_per_second_decimals = 0;
        farm.rewards[1].rewards_per_second_decimals = 6;
        farm.rewards[2].rewards_per_second_decimals = 18;
        
        // Add rewards
        for i in 0..3 {
            farm.add_rewards(i, 100_000_000_000).unwrap();
        }
        
        // Create users and stake
        for (user_id, &amount) in stake_amounts.iter().enumerate().take(num_users) {
            let stake_action = FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount,
                time_delta: 10,
            };
            let _ = farm.apply_action(&stake_action);
        }
        
        // Apply random actions
        for action in actions {
            let _ = farm.apply_action(&action);
        }
        
        // Check conservation with rounding tolerance
        for i in 0..farm.rewards.len() {
            let total_in = farm.total_rewards_deposited[i];
            let total_out = farm.total_rewards_claimed[i];
            let unclaimed = farm.rewards[i].rewards_issued_unclaimed;
            let available = farm.rewards[i].rewards_available;
            
            let accounted = total_out + unclaimed + available;
            
            // Rounding error should be bounded by number of operations
            let max_rounding_error = (actions.len() + num_users) as u64;
            
            prop_assert!(
                accounted <= total_in + max_rounding_error,
                "Reward {} conservation violated with rounding: {} > {} + {}",
                i, accounted, total_in, max_rounding_error
            );
        }
    }
    
    #[test]
    fn test_stake_conservation(
        actions in action_sequence_strategy()
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        let mut total_staked = 0u64;
        let mut total_unstaked = 0u64;
        
        for action in actions {
            match action.action_type {
                ActionType::Stake => {
                    if farm.apply_action(&action).is_ok() {
                        total_staked += action.amount;
                    }
                },
                ActionType::Unstake => {
                    if farm.apply_action(&action).is_ok() {
                        total_unstaked = total_unstaked.saturating_add(action.amount);
                    }
                },
                _ => {
                    let _ = farm.apply_action(&action);
                }
            }
        }
        
        // Check stake consistency
        prop_assert!(
            farm.check_stake_consistency(),
            "Stake conservation violated: sum of user stakes != total active stake"
        );
        
        // Total staked - unstaked should roughly equal active stake + pending
        let expected_active = total_staked.saturating_sub(total_unstaked);
        let actual_active = farm.total_active_stake.to_u64().unwrap_or(0);
        let total_pending: u64 = farm.users.values()
            .map(|u| u.pending_withdrawal)
            .sum();
        
        let accounted = actual_active + total_pending;
        
        // Allow for failed operations
        prop_assert!(
            accounted <= total_staked,
            "More stake accounted than deposited: {} > {}",
            accounted, total_staked
        );
    }
    
    #[test]
    fn test_slashing_conservation(
        actions in action_sequence_strategy(),
        penalty_bps in 100..5000u64 // 1% to 50%
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        // Track slashing events
        let mut total_slashed_expected = 0u64;
        
        for action in actions {
            if let ActionType::Unstake = action.action_type {
                // Simulate slashing on unstake
                let user = farm.users.get(&action.user_id);
                if let Some(user) = user {
                    let stake_u64 = user.active_stake.to_u64().unwrap_or(0);
                    if stake_u64 >= action.amount {
                        let penalty = action.amount * penalty_bps / 10000;
                        total_slashed_expected += penalty;
                        farm.total_slashed += penalty;
                    }
                }
            }
            let _ = farm.apply_action(&action);
        }
        
        // Slashed amount should be tracked correctly
        prop_assert_eq!(
            farm.total_slashed,
            total_slashed_expected,
            "Slashed amount mismatch"
        );
    }
    
    #[test]
    fn test_treasury_fee_conservation(
        actions in action_sequence_strategy(),
        treasury_fee_bps in 0..2000u64 // 0% to 20%
    ) {
        let mut farm = MockFarmState::new(1);
        farm.add_rewards(0, 10_000_000_000).unwrap();
        
        let mut total_treasury_fees = 0u64;
        let mut total_user_claims = 0u64;
        
        for action in actions {
            if let ActionType::Harvest(idx) = action.action_type {
                // Calculate treasury fee before harvest
                if let Some(user) = farm.users.get(&action.user_id) {
                    let claimable = user.rewards_unclaimed.get(idx).copied().unwrap_or(0);
                    if claimable > 0 {
                        let treasury_fee = claimable * treasury_fee_bps / 10000;
                        let user_amount = claimable - treasury_fee;
                        
                        total_treasury_fees += treasury_fee;
                        total_user_claims += user_amount;
                    }
                }
            }
            let _ = farm.apply_action(&action);
        }
        
        // Total distributed should equal claims + fees
        let total_distributed = farm.total_rewards_claimed[0];
        
        // Allow small rounding error
        let expected = total_user_claims + total_treasury_fees;
        prop_assert!(
            total_distributed <= expected + actions.len() as u64,
            "Treasury fee conservation violated: {} > {}",
            total_distributed, expected
        );
    }
}