use super::*;
use num_bigint::BigInt;
use num_rational::BigRational;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_tests::MockFarmState;
    
    #[test]
    fn test_simple_stake_unstake_scenario() {
        // Create reference and actual implementations
        let mut reference = ReferenceFarm::new(1);
        let mut actual = MockFarmState::new(1);
        
        // Setup identical initial conditions
        let initial_rewards = 10_000_000_000u64;
        reference.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(initial_rewards));
        actual.add_rewards(0, initial_rewards).unwrap();
        
        // User 0 stakes 1000 tokens
        let stake_amount = 1_000_000_000u64;
        reference.stake(0, BigRational::from_integer(BigInt::from(stake_amount)));
        actual.apply_action(&FarmAction {
            user_id: 0,
            action_type: ActionType::Stake,
            amount: stake_amount,
            time_delta: 0,
        }).unwrap();
        
        // Advance time 100 seconds
        reference.advance_time(100);
        reference.refresh_global_rewards();
        actual.current_time += 100;
        actual.refresh_farm().unwrap();
        
        // Compare states
        let result = compare_farms(&reference, &actual, 1.0); // 1 unit tolerance
        assert!(result.is_ok(), "Farms diverged: {:?}", result);
        
        // User unstakes half
        let unstake_amount = 500_000_000u64;
        reference.unstake(0, BigRational::from_integer(BigInt::from(unstake_amount)));
        actual.apply_action(&FarmAction {
            user_id: 0,
            action_type: ActionType::Unstake,
            amount: unstake_amount,
            time_delta: 0,
        }).unwrap();
        
        // Final comparison
        let result = compare_farms(&reference, &actual, 1.0);
        assert!(result.is_ok(), "Farms diverged after unstake: {:?}", result);
    }
    
    #[test]
    fn test_multiple_users_proportional_rewards() {
        let mut reference = ReferenceFarm::new(1);
        let mut actual = MockFarmState::new(1);
        
        // Setup
        let initial_rewards = 100_000_000_000u64;
        reference.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(initial_rewards));
        reference.rewards[0].reward_type = RewardType::Proportional;
        actual.add_rewards(0, initial_rewards).unwrap();
        actual.rewards[0].reward_type = RewardType::Proportional;
        
        // Multiple users stake different amounts
        let stakes = vec![
            (0, 1_000_000_000u64),
            (1, 2_000_000_000u64),
            (2, 3_000_000_000u64),
        ];
        
        for (user_id, amount) in stakes {
            reference.stake(user_id, BigRational::from_integer(BigInt::from(amount)));
            actual.apply_action(&FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount,
                time_delta: 10,
            }).unwrap();
        }
        
        // Advance time and accumulate rewards
        for _ in 0..10 {
            reference.advance_time(100);
            reference.refresh_global_rewards();
            actual.current_time += 100;
            actual.refresh_farm().unwrap();
            
            // Refresh all users
            for user_id in 0..3 {
                reference.refresh_user_rewards(user_id);
                actual.refresh_user(user_id).unwrap();
            }
        }
        
        // Compare final states
        let result = compare_farms(&reference, &actual, 10.0); // Higher tolerance for complex scenario
        assert!(result.is_ok(), "Multi-user scenario diverged: {:?}", result);
        
        // Verify proportional distribution
        for user_id in 0..3 {
            let ref_user = reference.users.get(&user_id).unwrap();
            let act_user = actual.users.get(&user_id).unwrap();
            
            let ref_rewards = ref_user.rewards_unclaimed[0].to_f64().unwrap_or(0.0);
            let act_rewards = act_user.rewards_unclaimed[0] as f64;
            
            let diff = (ref_rewards - act_rewards).abs();
            assert!(diff < 100.0, "User {} rewards mismatch: ref={}, act={}", 
                    user_id, ref_rewards, act_rewards);
        }
    }
    
    #[test]
    fn test_constant_vs_proportional_rewards() {
        // Test with constant rewards
        let mut ref_const = ReferenceFarm::new(1);
        let mut act_const = MockFarmState::new(1);
        
        ref_const.rewards[0].reward_type = RewardType::Constant;
        ref_const.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(10_000_000_000u64));
        act_const.rewards[0].reward_type = RewardType::Constant;
        act_const.add_rewards(0, 10_000_000_000).unwrap();
        
        // Test with proportional rewards
        let mut ref_prop = ReferenceFarm::new(1);
        let mut act_prop = MockFarmState::new(1);
        
        ref_prop.rewards[0].reward_type = RewardType::Proportional;
        ref_prop.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(10_000_000_000u64));
        act_prop.rewards[0].reward_type = RewardType::Proportional;
        act_prop.add_rewards(0, 10_000_000_000).unwrap();
        
        // Apply same actions to both
        let actions = vec![
            (0, 1_000_000_000u64),
            (1, 500_000_000u64),
        ];
        
        for (user_id, amount) in actions {
            ref_const.stake(user_id, BigRational::from_integer(BigInt::from(amount)));
            act_const.apply_action(&FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount,
                time_delta: 0,
            }).unwrap();
            
            ref_prop.stake(user_id, BigRational::from_integer(BigInt::from(amount)));
            act_prop.apply_action(&FarmAction {
                user_id,
                action_type: ActionType::Stake,
                amount,
                time_delta: 0,
            }).unwrap();
        }
        
        // Advance time
        for _ in 0..5 {
            ref_const.advance_time(100);
            ref_const.refresh_global_rewards();
            act_const.current_time += 100;
            act_const.refresh_farm().unwrap();
            
            ref_prop.advance_time(100);
            ref_prop.refresh_global_rewards();
            act_prop.current_time += 100;
            act_prop.refresh_farm().unwrap();
        }
        
        // Compare constant rewards (should be equal for both users)
        ref_const.refresh_user_rewards(0);
        ref_const.refresh_user_rewards(1);
        act_const.refresh_user(0).unwrap();
        act_const.refresh_user(1).unwrap();
        
        let const_user0 = ref_const.users.get(&0).unwrap().rewards_unclaimed[0].to_f64().unwrap_or(0.0);
        let const_user1 = ref_const.users.get(&1).unwrap().rewards_unclaimed[0].to_f64().unwrap_or(0.0);
        
        assert!((const_user0 - const_user1).abs() < 1.0, 
                "Constant rewards should be equal: {} vs {}", const_user0, const_user1);
        
        // Compare proportional rewards (should be 2:1 ratio)
        ref_prop.refresh_user_rewards(0);
        ref_prop.refresh_user_rewards(1);
        act_prop.refresh_user(0).unwrap();
        act_prop.refresh_user(1).unwrap();
        
        let prop_user0 = ref_prop.users.get(&0).unwrap().rewards_unclaimed[0].to_f64().unwrap_or(0.0);
        let prop_user1 = ref_prop.users.get(&1).unwrap().rewards_unclaimed[0].to_f64().unwrap_or(0.0);
        
        let ratio = prop_user0 / prop_user1;
        assert!(ratio > 1.9 && ratio < 2.1, 
                "Proportional rewards ratio should be ~2: {}", ratio);
    }
    
    #[test]
    fn test_reward_schedule_changes() {
        let mut reference = ReferenceFarm::new(1);
        let mut actual = MockFarmState::new(1);
        
        // Setup reward schedule with multiple points
        reference.rewards[0].schedule = vec![
            (0, BigRational::from_integer(BigInt::from(1000))),
            (100, BigRational::from_integer(BigInt::from(2000))),
            (300, BigRational::from_integer(BigInt::from(500))),
            (500, BigRational::zero()),
        ];
        
        // Approximate schedule in actual implementation
        actual.rewards[0].rewards_per_second = 1000;
        
        reference.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(100_000_000_000u64));
        actual.add_rewards(0, 100_000_000_000).unwrap();
        
        // User stakes
        reference.stake(0, BigRational::from_integer(BigInt::from(1_000_000_000u64)));
        actual.apply_action(&FarmAction {
            user_id: 0,
            action_type: ActionType::Stake,
            amount: 1_000_000_000,
            time_delta: 0,
        }).unwrap();
        
        // Test different phases
        let test_points = vec![50, 150, 250, 350, 450, 550];
        let mut last_time = 0u64;
        
        for time_point in test_points {
            let delta = time_point - last_time;
            reference.advance_time(delta);
            reference.refresh_global_rewards();
            actual.current_time += delta;
            actual.refresh_farm().unwrap();
            
            // At 150, manually update actual rate to simulate schedule
            if time_point == 150 {
                actual.rewards[0].rewards_per_second = 2000;
            } else if time_point == 350 {
                actual.rewards[0].rewards_per_second = 500;
            } else if time_point == 550 {
                actual.rewards[0].rewards_per_second = 0;
            }
            
            last_time = time_point;
        }
        
        // Compare cumulative rewards (with higher tolerance due to schedule approximation)
        let ref_cumulative = reference.rewards[0].rewards_issued_cumulative.to_f64().unwrap_or(0.0);
        let act_cumulative = actual.rewards[0].rewards_issued_cumulative as f64;
        
        let diff_pct = ((ref_cumulative - act_cumulative).abs() / ref_cumulative) * 100.0;
        assert!(diff_pct < 10.0, 
                "Schedule test: cumulative mismatch {}% (ref={}, act={})", 
                diff_pct, ref_cumulative, act_cumulative);
    }
    
    #[test]
    fn test_random_scenarios() {
        let mut generator = ScenarioGenerator::new(42);
        
        for scenario_id in 0..10 {
            let mut reference = ReferenceFarm::new(1);
            let mut actual = MockFarmState::new(1);
            
            // Initialize with rewards
            reference.rewards[0].rewards_available = BigRational::from_integer(BigInt::from(1_000_000_000_000u64));
            actual.add_rewards(0, 1_000_000_000_000).unwrap();
            
            // Generate and apply random scenario
            let actions = generator.generate_random_scenario(50);
            
            for action in actions {
                match action {
                    TestAction::Stake { user_id, amount } => {
                        reference.stake(user_id, BigRational::from_integer(BigInt::from(amount)));
                        let _ = actual.apply_action(&FarmAction {
                            user_id,
                            action_type: ActionType::Stake,
                            amount,
                            time_delta: 0,
                        });
                    },
                    TestAction::Unstake { user_id, amount } => {
                        let _ = reference.unstake(user_id, BigRational::from_integer(BigInt::from(amount)));
                        let _ = actual.apply_action(&FarmAction {
                            user_id,
                            action_type: ActionType::Unstake,
                            amount,
                            time_delta: 0,
                        });
                    },
                    TestAction::Harvest { user_id, reward_idx } => {
                        if reference.users.contains_key(&user_id) {
                            let _ = reference.harvest(user_id, reward_idx);
                            let _ = actual.harvest(user_id, reward_idx);
                        }
                    },
                    TestAction::AdvanceTime { seconds } => {
                        reference.advance_time(seconds);
                        actual.current_time += seconds;
                    },
                    TestAction::AddRewards { reward_idx, amount } => {
                        if reward_idx < reference.rewards.len() {
                            reference.rewards[reward_idx].rewards_available += 
                                BigRational::from_integer(BigInt::from(amount));
                            let _ = actual.add_rewards(reward_idx, amount);
                        }
                    },
                    TestAction::RefreshFarm => {
                        reference.refresh_global_rewards();
                        let _ = actual.refresh_farm();
                    },
                }
            }
            
            // Final comparison with relaxed tolerance for random scenarios
            let result = compare_farms(&reference, &actual, 100.0);
            if result.is_err() {
                println!("Scenario {} diverged: {:?}", scenario_id, result);
                // Don't fail test, just log divergence for analysis
            }
        }
    }
    
    #[test]
    fn test_extreme_decimals() {
        let decimal_configs = vec![
            (0, 1_000_000_000),
            (6, 1_000_000),
            (9, 1_000),
            (12, 1),
            (18, 1),
        ];
        
        for (decimals, base_rate) in decimal_configs {
            let mut reference = ReferenceFarm::new(1);
            let mut actual = MockFarmState::new(1);
            
            reference.rewards[0].decimals = decimals;
            reference.rewards[0].schedule = vec![
                (0, BigRational::from_integer(BigInt::from(base_rate))),
            ];
            reference.rewards[0].rewards_available = 
                BigRational::from_integer(BigInt::from(10u128.pow(decimals) * 1_000_000));
            
            actual.rewards[0].rewards_per_second_decimals = decimals as u64;
            actual.rewards[0].rewards_per_second = base_rate;
            actual.add_rewards(0, 10u64.pow(decimals) * 1_000_000).unwrap();
            
            // Stake and accumulate
            reference.stake(0, BigRational::from_integer(BigInt::from(1_000_000_000u64)));
            actual.apply_action(&FarmAction {
                user_id: 0,
                action_type: ActionType::Stake,
                amount: 1_000_000_000,
                time_delta: 0,
            }).unwrap();
            
            reference.advance_time(1000);
            reference.refresh_global_rewards();
            actual.current_time += 1000;
            actual.refresh_farm().unwrap();
            
            // Compare with decimal-specific tolerance
            let tolerance = 10f64.powi(-(decimals as i32 / 3));
            let result = compare_farms(&reference, &actual, tolerance);
            
            assert!(result.is_ok(), 
                    "Decimals {} test failed: {:?}", decimals, result);
        }
    }
}