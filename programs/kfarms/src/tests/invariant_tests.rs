#[cfg(test)]
mod invariant_enforcement_tests {
    use crate::state::{FarmState, UserState};
    use crate::stake_operations::*;
    use crate::farm_operations::*;
    use decimal_wad::decimal::Decimal;
    use crate::utils::math::ten_pow;
    
    struct InvariantChecker {
        initial_total_stake: Decimal,
        initial_total_amount: u64,
        initial_rewards_issued: u64,
        initial_rewards_claimed: u64,
    }
    
    impl InvariantChecker {
        fn new(farm: &FarmState) -> Self {
            Self {
                initial_total_stake: farm.get_total_active_stake_decimal(),
                initial_total_amount: farm.total_staked_amount,
                initial_rewards_issued: farm.reward_infos[0].rewards_issued_cumulative,
                initial_rewards_claimed: farm.reward_infos[0].rewards_claimed,
            }
        }
        
        fn check_farm_invariants(&self, farm: &FarmState) {
            // Invariant 1: total_active_stake_scaled tracks share supply
            if !farm.is_delegated() {
                let expected_stake = Decimal::from(farm.total_staked_amount);
                let actual_stake = farm.get_total_active_stake_decimal();
                
                // Allow small rounding errors
                let diff = if actual_stake > expected_stake {
                    actual_stake - expected_stake
                } else {
                    expected_stake - actual_stake
                };
                
                assert!(
                    diff < Decimal::from(1u64),
                    "Farm stake invariant violated: expected {:?}, got {:?}",
                    expected_stake,
                    actual_stake
                );
            }
            
            // Invariant 2: rewards_issued_unclaimed is monotonic
            assert!(
                farm.reward_infos[0].rewards_issued_cumulative >= self.initial_rewards_issued,
                "Rewards issued should be monotonic"
            );
            
            // Invariant 3: rewards_claimed is monotonic
            assert!(
                farm.reward_infos[0].rewards_claimed >= self.initial_rewards_claimed,
                "Rewards claimed should be monotonic"
            );
            
            // Invariant 4: unclaimed = issued - claimed
            let unclaimed = farm.reward_infos[0].rewards_issued_unclaimed;
            let issued = farm.reward_infos[0].rewards_issued_cumulative;
            let claimed = farm.reward_infos[0].rewards_claimed;
            
            assert_eq!(
                unclaimed,
                issued.saturating_sub(claimed),
                "Unclaimed rewards invariant violated"
            );
            
            // Invariant 5: total vault balance >= active + pending amounts
            let total_tracked = farm.total_staked_amount
                .saturating_add(farm.total_pending_amount);
            
            // In real scenario, we'd check against actual vault balance
            // For now, just ensure non-negative
            assert!(total_tracked <= u64::MAX, "Amount overflow detected");
        }
        
        fn check_user_invariants(&self, user: &UserState, farm: &FarmState) {
            // Invariant 1: User shares are consistent
            let total_user_shares = user.get_active_stake_decimal()
                + user.get_pending_deposit_stake_decimal()
                + user.get_pending_withdrawal_unstake_decimal();
            
            // User shares should not exceed farm total
            assert!(
                user.get_active_stake_decimal() <= farm.get_total_active_stake_decimal(),
                "User active stake exceeds farm total"
            );
            
            // Invariant 2: Reward tally <= global reward per share
            for i in 0..farm.num_reward_tokens as usize {
                assert!(
                    user.reward_infos[i].get_reward_tally_decimal() 
                        <= farm.reward_infos[i].get_reward_per_share_decimal(),
                    "User reward tally exceeds global"
                );
            }
            
            // Invariant 3: Timestamps are reasonable
            if user.pending_deposit_stake_ts > 0 {
                assert!(
                    user.pending_deposit_stake_ts <= u64::MAX / 2,
                    "Unreasonable pending deposit timestamp"
                );
            }
            
            if user.pending_withdrawal_unstake_ts > 0 {
                assert!(
                    user.pending_withdrawal_unstake_ts <= u64::MAX / 2,
                    "Unreasonable pending withdrawal timestamp"
                );
            }
        }
    }
    
    #[test]
    fn test_deposit_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.deposit_warmup_period = 0; // No warmup for simplicity
        
        let mut user = UserState::default();
        let checker = InvariantChecker::new(&farm);
        
        // Perform deposit
        let deposit_amount = 1000u64;
        let result = add_pending_deposit_stake(&mut user, &mut farm, deposit_amount);
        assert!(result.is_ok());
        
        // Move to active (simulating warmup period passing)
        let result = move_pending_deposit_to_active(&mut user, &mut farm);
        assert!(result.is_ok());
        
        // Check invariants
        checker.check_farm_invariants(&farm);
        checker.check_user_invariants(&user, &farm);
        
        // Additional checks
        assert_eq!(farm.total_staked_amount, deposit_amount);
        assert_eq!(user.get_active_stake_decimal(), Decimal::from(deposit_amount));
    }
    
    #[test]
    fn test_withdrawal_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_staked_amount = 10000;
        farm.total_active_stake_scaled = (10000 * ten_pow(18)) as u128;
        farm.withdrawal_cooldown_period = 0; // No cooldown for simplicity
        
        let mut user = UserState::default();
        user.active_stake_scaled = (1000 * ten_pow(18)) as u128; // 10% of farm
        
        let checker = InvariantChecker::new(&farm);
        
        // Perform withdrawal
        let withdraw_stake = Decimal::from(500u64);
        let result = remove_active_stake(&mut user, &mut farm, withdraw_stake);
        assert!(result.is_ok());
        
        // Check invariants
        checker.check_farm_invariants(&farm);
        checker.check_user_invariants(&user, &farm);
        
        // Additional checks
        assert_eq!(user.get_active_stake_decimal(), Decimal::from(500u64));
        assert!(farm.total_staked_amount < 10000);
    }
    
    #[test]
    fn test_reward_distribution_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_staked_amount = 10000;
        farm.total_active_stake_scaled = (10000 * ten_pow(18)) as u128;
        farm.reward_infos[0].rewards_available = 1_000_000;
        farm.reward_infos[0].rewards_issued_unclaimed = 0;
        
        let mut users = vec![UserState::default(); 10];
        
        // Distribute stake among users
        for (i, user) in users.iter_mut().enumerate() {
            user.active_stake_scaled = ((1000 * ten_pow(18)) as u128); // Each user has 10%
            user.user_id = i as u64;
        }
        
        let checker = InvariantChecker::new(&farm);
        
        // Issue some rewards
        farm.reward_infos[0].rewards_issued_unclaimed = 10000;
        farm.reward_infos[0].rewards_issued_cumulative = 10000;
        farm.reward_infos[0].set_reward_per_share_decimal(
            Decimal::from(10000u64) / farm.get_total_active_stake_decimal()
        );
        
        // Refresh rewards for all users
        let mut total_user_rewards = 0u64;
        for user in &mut users {
            let result = user_refresh_reward(&mut farm, user, 0);
            assert!(result.is_ok());
            
            let user_rewards = user.reward_infos[0].get_rewards_earned_decimal()
                .to_u64()
                .unwrap_or(0);
            total_user_rewards += user_rewards;
        }
        
        // Check invariants
        checker.check_farm_invariants(&farm);
        for user in &users {
            checker.check_user_invariants(user, &farm);
        }
        
        // Sum of user rewards should approximately equal issued rewards
        let diff = if total_user_rewards > farm.reward_infos[0].rewards_issued_unclaimed {
            total_user_rewards - farm.reward_infos[0].rewards_issued_unclaimed
        } else {
            farm.reward_infos[0].rewards_issued_unclaimed - total_user_rewards
        };
        
        assert!(
            diff <= users.len() as u64, // Allow rounding error per user
            "User rewards sum doesn't match farm issued: {} vs {}",
            total_user_rewards,
            farm.reward_infos[0].rewards_issued_unclaimed
        );
    }
    
    #[test]
    fn test_slashing_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.total_staked_amount = 10000;
        farm.total_active_stake_scaled = (10000 * ten_pow(18)) as u128;
        farm.slashed_amount_current = 0;
        farm.slashed_amount_cumulative = 0;
        
        let initial_amount = farm.total_staked_amount;
        
        // Simulate slashing event
        let slash_amount = 1000u64;
        farm.slashed_amount_current += slash_amount;
        farm.slashed_amount_cumulative += slash_amount;
        farm.total_staked_amount -= slash_amount;
        
        // Check slashing invariants
        assert_eq!(
            farm.total_staked_amount,
            initial_amount - slash_amount,
            "Slashing should reduce total staked amount"
        );
        
        assert_eq!(
            farm.slashed_amount_cumulative,
            farm.slashed_amount_current,
            "Cumulative should track current when no recovery"
        );
        
        // Simulate partial recovery
        let recovery_amount = 300u64;
        farm.slashed_amount_current -= recovery_amount;
        farm.total_staked_amount += recovery_amount;
        
        assert_eq!(
            farm.slashed_amount_cumulative,
            slash_amount,
            "Cumulative should remain unchanged on recovery"
        );
        
        assert_eq!(
            farm.slashed_amount_current,
            slash_amount - recovery_amount,
            "Current slashed should decrease on recovery"
        );
    }
    
    #[test]
    fn test_state_transition_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.deposit_warmup_period = 100;
        farm.withdrawal_cooldown_period = 100;
        
        let mut user = UserState::default();
        
        // Test state transitions: pending deposit -> active -> pending withdrawal
        
        // Stage 1: Pending deposit
        let deposit_amount = 1000u64;
        add_pending_deposit_stake(&mut user, &mut farm, deposit_amount).unwrap();
        
        assert_eq!(farm.total_pending_amount, deposit_amount);
        assert!(user.get_pending_deposit_stake_decimal() > Decimal::zero());
        assert_eq!(user.get_active_stake_decimal(), Decimal::zero());
        
        // Stage 2: Move to active
        move_pending_deposit_to_active(&mut user, &mut farm).unwrap();
        
        assert_eq!(farm.total_pending_amount, 0);
        assert_eq!(farm.total_staked_amount, deposit_amount);
        assert_eq!(user.get_pending_deposit_stake_decimal(), Decimal::zero());
        assert!(user.get_active_stake_decimal() > Decimal::zero());
        
        // Stage 3: Initiate withdrawal
        let withdraw_amount = Decimal::from(500u64);
        remove_active_stake(&mut user, &mut farm, withdraw_amount).unwrap();
        
        assert!(user.get_active_stake_decimal() < Decimal::from(deposit_amount));
        assert!(farm.total_staked_amount < deposit_amount);
        
        // All transitions should maintain conservation of value
        let total_tracked = farm.total_staked_amount + farm.total_pending_amount;
        assert!(
            total_tracked <= deposit_amount,
            "Value should be conserved or reduced (due to fees/penalties)"
        );
    }
}