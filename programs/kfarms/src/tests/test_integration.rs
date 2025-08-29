use crate::{
    state::{FarmState, UserState, RewardInfo, GlobalConfig},
    utils::{
        math::{u64_mul_div, full_decimal_mul_div, ten_pow},
        withdrawal_penalty::apply_early_withdrawal_penalty,
        consts::BPS_DIV_FACTOR,
    },
    FarmError,
};
use decimal_wad::decimal::Decimal;
use solana_program::pubkey::Pubkey;

#[cfg(test)]
mod integration_tests {
    use super::*;

    // Comprehensive Integration Tests

    #[test]
    fn test_full_lifecycle_single_user() {
        // Test complete lifecycle: create → stake → wait → harvest → unstake
        let mut farm = create_initialized_farm();
        let mut user = create_user_state();
        let mut global_config = create_global_config();
        
        // Initial state
        assert_eq!(farm.total_staked_amount, 0);
        assert_eq!(user.get_active_stake().to_u64().unwrap(), 0);
        
        // 1. Stake
        let stake_amount = 1_000_000u64;
        let stake_result = stake_tokens(&mut farm, &mut user, stake_amount, 1000);
        assert!(stake_result.is_ok());
        
        // 2. Wait for warmup
        let current_ts = 1000 + farm.deposit_warmup_period as u64;
        let activate_result = activate_pending_stake(&mut farm, &mut user, current_ts);
        assert!(activate_result.is_ok());
        
        // Verify activation
        assert_eq!(user.get_active_stake().to_u64().unwrap(), stake_amount);
        assert_eq!(farm.total_active_amount, stake_amount);
        
        // 3. Accrue rewards
        let reward_duration = 86400u64; // 1 day
        let new_ts = current_ts + reward_duration;
        let rewards = calculate_and_issue_rewards(&mut farm, current_ts, new_ts);
        assert!(rewards > 0);
        
        // 4. Harvest rewards
        let harvest_result = harvest_rewards(&mut farm, &mut user, &mut global_config, new_ts);
        assert!(harvest_result.is_ok());
        let (user_rewards, treasury_fee) = harvest_result.unwrap();
        assert!(user_rewards > 0);
        
        // 5. Unstake
        let unstake_result = unstake_tokens(&mut farm, &mut user, stake_amount, new_ts);
        assert!(unstake_result.is_ok());
        
        // 6. Wait for cooldown
        let withdraw_ts = new_ts + farm.withdrawal_cooldown_period as u64;
        let withdraw_result = complete_withdrawal(&mut farm, &mut user, withdraw_ts);
        assert!(withdraw_result.is_ok());
        
        // Final verification
        assert_eq!(user.get_active_stake().to_u64().unwrap(), 0);
        assert_eq!(farm.total_active_amount, 0);
    }

    #[test]
    fn test_multi_user_proportional_rewards() {
        // Test that multiple users receive proportional rewards
        let mut farm = create_initialized_farm();
        let mut users = vec![create_user_state(); 3];
        let stakes = vec![1_000_000u64, 2_000_000u64, 3_000_000u64];
        let total_stake: u64 = stakes.iter().sum();
        
        // All users stake
        for (i, stake) in stakes.iter().enumerate() {
            stake_and_activate(&mut farm, &mut users[i], *stake, 1000 + i as u64);
        }
        
        // Verify total stake
        assert_eq!(farm.total_active_amount, total_stake);
        
        // Issue rewards
        let reward_amount = 60_000u64;
        issue_rewards_to_farm(&mut farm, reward_amount, 2000);
        
        // Each user harvests
        let mut total_harvested = 0u64;
        for (i, stake) in stakes.iter().enumerate() {
            let expected_reward = (reward_amount * stake) / total_stake;
            let actual_reward = calculate_user_reward(&farm, &users[i]);
            
            // Should be proportional (within rounding)
            assert!(actual_reward >= expected_reward - 1 && 
                   actual_reward <= expected_reward + 1);
            
            total_harvested += actual_reward;
        }
        
        // Total harvested should not exceed issued
        assert!(total_harvested <= reward_amount);
    }

    #[test]
    fn test_delegated_farm_operations() {
        // Test delegated farm with external RPS updates
        let mut farm = create_delegated_farm();
        let mut user = create_user_state();
        
        // Stake in delegated farm
        let stake_amount = 1_000_000u64;
        stake_and_activate(&mut farm, &mut user, stake_amount, 1000);
        
        // External RPS update (simulating delegated authority)
        let new_rps = Decimal::from(2_000_000u64);
        update_delegated_rps(&mut farm, new_rps, 2000);
        
        // Calculate rewards based on external RPS
        let rewards = calculate_user_reward(&farm, &user);
        assert!(rewards > 0);
        
        // Verify delegated invariants
        assert_eq!(farm.total_active_amount, farm.total_staked_amount);
        assert_eq!(farm.get_total_pending_stake().to_u64().unwrap(), 0);
    }

    #[test]
    fn test_locking_with_penalty_scenario() {
        // Test complete locking scenario with early withdrawal penalty
        let mut farm = create_locking_farm();
        let mut user = create_user_state();
        
        let stake_amount = 10_000_000u64;
        stake_and_activate(&mut farm, &mut user, stake_amount, farm.locking_start_timestamp);
        
        // Try early withdrawal (50% through locking period)
        let withdraw_ts = farm.locking_start_timestamp + farm.locking_duration / 2;
        
        let penalty_result = apply_early_withdrawal_penalty(
            farm.locking_duration,
            farm.locking_start_timestamp,
            withdraw_ts,
            farm.locking_early_withdrawal_penalty_bps,
            stake_amount,
        );
        
        assert!(penalty_result.is_ok());
        let (amount_after_penalty, penalty) = penalty_result.unwrap();
        
        // Penalty should be 25% (50% of max 50% penalty)
        let expected_penalty = stake_amount / 4;
        assert_eq!(penalty, expected_penalty);
        assert_eq!(amount_after_penalty, stake_amount - expected_penalty);
        
        // Update farm state
        farm.slashed_amount_cumulative += penalty;
    }

    #[test]
    fn test_high_frequency_stake_unstake() {
        // Test system behavior under high frequency operations
        let mut farm = create_initialized_farm();
        let mut users = vec![create_user_state(); 10];
        
        // Rapid stake/unstake cycles
        for cycle in 0..100 {
            let user_idx = cycle % 10;
            let amount = ((cycle + 1) * 1000) as u64;
            
            if cycle % 2 == 0 {
                // Stake
                stake_and_activate(&mut farm, &mut users[user_idx], amount, 1000 + cycle as u64);
            } else {
                // Unstake (if has balance)
                let current_stake = users[user_idx].get_active_stake().to_u64().unwrap();
                if current_stake >= amount {
                    unstake_tokens(&mut farm, &mut users[user_idx], amount, 1000 + cycle as u64).ok();
                }
            }
        }
        
        // Verify consistency
        let total_user_stakes: u64 = users.iter()
            .map(|u| u.get_active_stake().to_u64().unwrap())
            .sum();
        assert_eq!(total_user_stakes, farm.total_active_amount);
    }

    #[test]
    fn test_oracle_price_integration() {
        // Test farm with oracle price adjustments
        let mut farm = create_farm_with_oracle();
        let mut user = create_user_state();
        
        // Stake
        let stake_amount = 1_000_000u64;
        stake_and_activate(&mut farm, &mut user, stake_amount, 1000);
        
        // Issue rewards with price adjustment
        let base_rewards = 10_000u64;
        let price_multiplier = 1_500_000_000u64; // 1.5x price
        let adjusted_rewards = u64_mul_div(base_rewards, price_multiplier, 1_000_000_000);
        
        issue_rewards_with_price(&mut farm, base_rewards, price_multiplier, 2000);
        
        // Harvest and verify price-adjusted rewards
        let user_rewards = calculate_user_reward(&farm, &user);
        assert!(user_rewards > base_rewards); // Should be ~1.5x due to price
    }

    #[test]
    fn test_deposit_cap_enforcement() {
        // Test deposit cap limits
        let mut farm = create_capped_farm(5_000_000); // 5M cap
        let mut users = vec![create_user_state(); 3];
        
        // First user stakes 3M
        let result1 = stake_tokens(&mut farm, &mut users[0], 3_000_000, 1000);
        assert!(result1.is_ok());
        
        // Second user stakes 2M (at cap)
        let result2 = stake_tokens(&mut farm, &mut users[1], 2_000_000, 1001);
        assert!(result2.is_ok());
        
        // Third user tries to stake 1M (over cap)
        let result3 = stake_tokens(&mut farm, &mut users[2], 1_000_000, 1002);
        assert!(result3.is_err());
        assert_eq!(result3.unwrap_err(), FarmError::DepositCapExceeded);
    }

    #[test]
    fn test_warmup_cooldown_cycles() {
        // Test warmup and cooldown period enforcement
        let mut farm = create_initialized_farm();
        farm.deposit_warmup_period = 100;
        farm.withdrawal_cooldown_period = 200;
        
        let mut user = create_user_state();
        
        // Stake
        stake_tokens(&mut farm, &mut user, 1_000_000, 1000).unwrap();
        
        // Try to activate before warmup - should fail
        let early_activate = activate_pending_stake(&mut farm, &mut user, 1050);
        assert!(early_activate.is_err());
        
        // Activate after warmup - should succeed
        let proper_activate = activate_pending_stake(&mut farm, &mut user, 1100);
        assert!(proper_activate.is_ok());
        
        // Unstake
        unstake_tokens(&mut farm, &mut user, 500_000, 1200).unwrap();
        
        // Try to withdraw before cooldown - should fail
        let early_withdraw = complete_withdrawal(&mut farm, &mut user, 1300);
        assert!(early_withdraw.is_err());
        
        // Withdraw after cooldown - should succeed
        let proper_withdraw = complete_withdrawal(&mut farm, &mut user, 1400);
        assert!(proper_withdraw.is_ok());
    }

    #[test]
    fn test_reward_schedule_curve() {
        // Test different reward schedule curves
        let mut farm = create_initialized_farm();
        
        // Linear curve
        let linear_rewards = calculate_linear_rewards(1000, 2000, 100);
        assert_eq!(linear_rewards, 100_000); // 100 * 1000 seconds
        
        // Exponential decay curve
        let exp_rewards = calculate_exponential_rewards(1000, 2000, 100, 0.999);
        assert!(exp_rewards < linear_rewards); // Should decay
        
        // Step function curve
        let step_rewards = calculate_step_rewards(1000, 2000, vec![
            (1000, 1500, 100),
            (1500, 2000, 50),
        ]);
        assert_eq!(step_rewards, 75_000); // 500*100 + 500*50
    }

    #[test]
    fn test_complex_multi_reward_scenario() {
        // Test farm with multiple reward tokens
        let mut farm = create_multi_reward_farm();
        let mut user = create_user_state();
        
        // Stake
        stake_and_activate(&mut farm, &mut user, 1_000_000, 1000);
        
        // Issue different rewards
        for i in 0..3 {
            let reward_amount = (i + 1) as u64 * 10_000;
            farm.reward_infos[i].rewards_available += reward_amount;
            farm.reward_infos[i].rewards_issued_cumulative += reward_amount;
        }
        
        // Harvest all rewards
        let mut total_rewards = vec![0u64; 3];
        for i in 0..3 {
            total_rewards[i] = calculate_user_reward_for_index(&farm, &user, i);
            assert!(total_rewards[i] > 0);
        }
        
        // Verify each reward token independently
        assert_eq!(total_rewards[0], 10_000);
        assert_eq!(total_rewards[1], 20_000);
        assert_eq!(total_rewards[2], 30_000);
    }

    #[test]
    fn test_emergency_scenarios() {
        // Test emergency scenarios and recovery
        let mut farm = create_initialized_farm();
        let mut users = vec![create_user_state(); 5];
        
        // Users stake
        for i in 0..5 {
            stake_and_activate(&mut farm, &mut users[i], 1_000_000, 1000 + i as u64);
        }
        
        // Emergency: Freeze farm
        farm.is_frozen = true;
        
        // New stakes should fail
        let mut new_user = create_user_state();
        let stake_result = stake_tokens(&mut farm, &mut new_user, 1_000_000, 2000);
        assert!(stake_result.is_err());
        
        // Unfreeze
        farm.is_frozen = false;
        
        // Operations should resume
        let stake_result2 = stake_tokens(&mut farm, &mut new_user, 1_000_000, 2001);
        assert!(stake_result2.is_ok());
    }

    #[test]
    fn test_decimal_precision_in_practice() {
        // Test decimal precision in real scenarios
        let mut farm = create_initialized_farm();
        let mut user = create_user_state();
        
        // Very small stake
        stake_and_activate(&mut farm, &mut user, 1, 1000);
        
        // Large reward distribution
        let large_reward = 1_000_000_000_000u64;
        issue_rewards_to_farm(&mut farm, large_reward, 2000);
        
        // User should get proportional share despite small stake
        let user_reward = calculate_user_reward(&farm, &user);
        assert_eq!(user_reward, large_reward); // Only staker gets all
        
        // Add another user with large stake
        let mut user2 = create_user_state();
        stake_and_activate(&mut farm, &mut user2, 999_999_999, 2001);
        
        // Issue more rewards
        issue_rewards_to_farm(&mut farm, 1_000_000_000, 3000);
        
        // First user should get tiny fraction
        let user1_new_reward = calculate_user_reward(&farm, &user) - user_reward;
        assert!(user1_new_reward < 10); // Very small share
    }

    // Helper functions for integration tests
    fn create_initialized_farm() -> FarmState {
        let mut farm = FarmState {
            version: 1,
            creator: Pubkey::new_unique().to_bytes(),
            authority: Pubkey::new_unique().to_bytes(),
            pending_authority: [0u8; 32],
            farm_vault: Pubkey::new_unique().to_bytes(),
            farm_vault_authority: Pubkey::new_unique().to_bytes(),
            farm_token_mint: Pubkey::new_unique().to_bytes(),
            farm_token_decimals: 9,
            reward_infos: vec![RewardInfo::default()],
            total_staked_amount: 0,
            total_active_amount: 0,
            total_pending_amount: 0,
            total_active_stake_scaled: 0,
            total_pending_stake_scaled: 0,
            slashed_amount_cumulative: 0,
            deposit_warmup_period: 0,
            withdrawal_cooldown_period: 0,
            is_delegated: false,
            is_permissioned: false,
            locking_mode: 0,
            locking_start_timestamp: 0,
            locking_duration: 0,
            locking_early_withdrawal_penalty_bps: 0,
            deposit_cap_amount: u64::MAX,
            is_frozen: false,
            _padding: [0u8; 256],
        };
        
        // Initialize reward info
        farm.reward_infos[0].rewards_per_second_decimals = 12;
        farm.reward_infos[0].reward_mint_decimals = 9;
        
        farm
    }

    fn create_user_state() -> UserState {
        UserState {
            version: 1,
            farm: Pubkey::new_unique().to_bytes(),
            owner: Pubkey::new_unique().to_bytes(),
            active_stake_scaled: 0,
            pending_deposit_stake_scaled: 0,
            pending_deposit_stake_ts: 0,
            pending_withdrawal_unstake_scaled: 0,
            pending_withdrawal_unstake_ts: 0,
            last_claim_ts: [0u64; 10],
            rewards_tally_scaled: [0u128; 10],
            _padding: [0u8; 256],
        }
    }

    fn create_global_config() -> GlobalConfig {
        GlobalConfig {
            authority: Pubkey::new_unique().to_bytes(),
            treasury_fee_bps: 100, // 1% fee
            _padding: [0u8; 256],
        }
    }

    fn create_delegated_farm() -> FarmState {
        let mut farm = create_initialized_farm();
        farm.is_delegated = true;
        farm
    }

    fn create_locking_farm() -> FarmState {
        let mut farm = create_initialized_farm();
        farm.locking_mode = 1; // WithPenalty
        farm.locking_start_timestamp = 1000;
        farm.locking_duration = 365 * 24 * 60 * 60; // 1 year
        farm.locking_early_withdrawal_penalty_bps = 5000; // 50% max penalty
        farm
    }

    fn create_farm_with_oracle() -> FarmState {
        let mut farm = create_initialized_farm();
        farm.reward_infos[0].oracle_price_account = Some(Pubkey::new_unique().to_bytes());
        farm
    }

    fn create_capped_farm(cap: u64) -> FarmState {
        let mut farm = create_initialized_farm();
        farm.deposit_cap_amount = cap;
        farm
    }

    fn create_multi_reward_farm() -> FarmState {
        let mut farm = create_initialized_farm();
        farm.reward_infos = vec![RewardInfo::default(); 3];
        for info in &mut farm.reward_infos {
            info.rewards_per_second_decimals = 12;
            info.reward_mint_decimals = 9;
        }
        farm
    }

    fn stake_tokens(farm: &mut FarmState, user: &mut UserState, amount: u64, ts: u64) -> Result<(), FarmError> {
        if farm.is_frozen {
            return Err(FarmError::FarmFrozen);
        }
        
        if farm.total_staked_amount + amount > farm.deposit_cap_amount {
            return Err(FarmError::DepositCapExceeded);
        }
        
        user.set_pending_deposit_stake(Decimal::from(amount));
        user.pending_deposit_stake_ts = ts;
        farm.total_pending_amount += amount;
        farm.total_staked_amount += amount;
        
        Ok(())
    }

    fn activate_pending_stake(farm: &mut FarmState, user: &mut UserState, ts: u64) -> Result<(), FarmError> {
        if ts < user.pending_deposit_stake_ts + farm.deposit_warmup_period as u64 {
            return Err(FarmError::WarmupNotComplete);
        }
        
        let pending = user.get_pending_deposit_stake().to_u64().unwrap();
        user.set_pending_deposit_stake(Decimal::from(0));
        user.set_active_stake(Decimal::from(
            user.get_active_stake().to_u64().unwrap() + pending
        ));
        
        farm.total_pending_amount -= pending;
        farm.total_active_amount += pending;
        
        Ok(())
    }

    fn stake_and_activate(farm: &mut FarmState, user: &mut UserState, amount: u64, ts: u64) {
        stake_tokens(farm, user, amount, ts).unwrap();
        activate_pending_stake(farm, user, ts + farm.deposit_warmup_period as u64).unwrap();
    }

    fn unstake_tokens(farm: &mut FarmState, user: &mut UserState, amount: u64, ts: u64) -> Result<(), FarmError> {
        let active = user.get_active_stake().to_u64().unwrap();
        if amount > active {
            return Err(FarmError::InsufficientStake);
        }
        
        user.set_active_stake(Decimal::from(active - amount));
        user.set_pending_withdrawal_unstake(Decimal::from(amount));
        user.pending_withdrawal_unstake_ts = ts;
        
        farm.total_active_amount -= amount;
        
        Ok(())
    }

    fn complete_withdrawal(farm: &mut FarmState, user: &mut UserState, ts: u64) -> Result<(), FarmError> {
        if ts < user.pending_withdrawal_unstake_ts + farm.withdrawal_cooldown_period as u64 {
            return Err(FarmError::CooldownNotComplete);
        }
        
        let pending = user.get_pending_withdrawal_unstake().to_u64().unwrap();
        user.set_pending_withdrawal_unstake(Decimal::from(0));
        farm.total_staked_amount -= pending;
        
        Ok(())
    }

    fn calculate_and_issue_rewards(farm: &mut FarmState, from_ts: u64, to_ts: u64) -> u64 {
        let duration = to_ts - from_ts;
        let rewards = duration * 100; // 100 rewards per second
        
        farm.reward_infos[0].rewards_available += rewards;
        farm.reward_infos[0].rewards_issued_cumulative += rewards;
        farm.reward_infos[0].last_issuance_ts = to_ts;
        
        rewards
    }

    fn harvest_rewards(
        farm: &mut FarmState,
        user: &mut UserState,
        config: &mut GlobalConfig,
        ts: u64
    ) -> Result<(u64, u64), FarmError> {
        let rewards = calculate_user_reward(farm, user);
        let treasury_fee = u64_mul_div(rewards, config.treasury_fee_bps, BPS_DIV_FACTOR);
        let user_rewards = rewards - treasury_fee;
        
        user.last_claim_ts[0] = ts;
        farm.reward_infos[0].rewards_available -= rewards;
        
        Ok((user_rewards, treasury_fee))
    }

    fn calculate_user_reward(farm: &FarmState, user: &UserState) -> u64 {
        let stake = user.get_active_stake().to_u64().unwrap();
        if stake == 0 || farm.total_active_amount == 0 {
            return 0;
        }
        
        (farm.reward_infos[0].rewards_issued_cumulative * stake) / farm.total_active_amount
    }

    fn calculate_user_reward_for_index(farm: &FarmState, user: &UserState, index: usize) -> u64 {
        let stake = user.get_active_stake().to_u64().unwrap();
        if stake == 0 || farm.total_active_amount == 0 {
            return 0;
        }
        
        (farm.reward_infos[index].rewards_issued_cumulative * stake) / farm.total_active_amount
    }

    fn issue_rewards_to_farm(farm: &mut FarmState, amount: u64, ts: u64) {
        farm.reward_infos[0].rewards_available += amount;
        farm.reward_infos[0].rewards_issued_cumulative += amount;
        farm.reward_infos[0].last_issuance_ts = ts;
        
        // Update RPS
        if farm.total_active_amount > 0 {
            let rps_increase = Decimal::from(amount * 1_000_000_000_000_000_000u128 / farm.total_active_amount as u128);
            let current_rps = farm.reward_infos[0].get_reward_per_share();
            let new_rps = Decimal::from_scaled_val(
                current_rps.to_scaled_val::<u128>().unwrap() + 
                rps_increase.to_scaled_val::<u128>().unwrap()
            );
            farm.reward_infos[0].set_reward_per_share(new_rps);
        }
    }

    fn issue_rewards_with_price(farm: &mut FarmState, base_amount: u64, price: u64, ts: u64) {
        let adjusted_amount = u64_mul_div(base_amount, price, 1_000_000_000);
        issue_rewards_to_farm(farm, adjusted_amount, ts);
    }

    fn update_delegated_rps(farm: &mut FarmState, new_rps: Decimal, ts: u64) {
        farm.reward_infos[0].set_reward_per_share(new_rps);
        farm.reward_infos[0].last_issuance_ts = ts;
    }

    fn calculate_linear_rewards(from_ts: u64, to_ts: u64, rate: u64) -> u64 {
        (to_ts - from_ts) * rate
    }

    fn calculate_exponential_rewards(from_ts: u64, to_ts: u64, initial_rate: u64, decay: f64) -> u64 {
        let duration = (to_ts - from_ts) as f64;
        let avg_rate = initial_rate as f64 * (1.0 - decay.powf(duration)) / (1.0 - decay);
        (avg_rate * duration) as u64
    }

    fn calculate_step_rewards(from_ts: u64, to_ts: u64, steps: Vec<(u64, u64, u64)>) -> u64 {
        let mut total = 0u64;
        for (start, end, rate) in steps {
            let period_start = start.max(from_ts);
            let period_end = end.min(to_ts);
            if period_end > period_start {
                total += (period_end - period_start) * rate;
            }
        }
        total
    }

    struct GlobalConfig {
        authority: [u8; 32],
        treasury_fee_bps: u16,
        _padding: [u8; 256],
    }
}