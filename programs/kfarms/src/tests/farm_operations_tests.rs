#[cfg(test)]
mod tests {
    use crate::farm_operations::*;
    use crate::state::{FarmState, UserState, RewardInfo, RewardScheduleCurve, TimeUnit};
    use crate::utils::math::ten_pow;
    use decimal_wad::decimal::Decimal;
    use anchor_lang::prelude::Pubkey;

    fn create_test_farm() -> FarmState {
        let mut farm = FarmState::default();
        farm.total_active_stake_scaled = 1000_000_000_000_000_000; // 1.0 in WAD
        farm.total_staked_amount = 1000;
        farm.num_reward_tokens = 1;
        farm.time_unit = TimeUnit::Seconds as u8;
        
        // Initialize reward info
        farm.reward_infos[0] = RewardInfo {
            rewards_available: 10_000,
            rewards_per_second_decimals: 0,
            reward_schedule_curve: RewardScheduleCurve::Constant { rate: 100 },
            last_issuance_ts: 1000,
            ..Default::default()
        };
        
        farm
    }

    fn create_test_user() -> UserState {
        let mut user = UserState::default();
        user.active_stake_scaled = 100_000_000_000_000_000; // 0.1 in WAD
        user
    }

    #[test]
    fn test_user_refresh_reward_basic() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Set reward per share
        farm.reward_infos[0].reward_per_share_scaled = 2_000_000_000_000_000_000; // 2.0 in WAD
        
        user_refresh_reward(&mut farm, &mut user, 0).unwrap();
        
        // User should receive: 0.1 * 2.0 = 0.2 rewards
        // But it floors to u64, and depends on previous tally
        assert!(user.rewards_issued_unclaimed[0] > 0);
    }

    #[test]
    fn test_user_refresh_reward_rounding() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Very small reward per share to test rounding
        farm.reward_infos[0].reward_per_share_scaled = 1; // Minimal amount
        
        user_refresh_reward(&mut farm, &mut user, 0).unwrap();
        
        // Should floor to 0 due to rounding
        assert_eq!(user.rewards_issued_unclaimed[0], 0);
    }

    #[test]
    fn test_refresh_global_reward_no_stakers() {
        let mut farm = create_test_farm();
        farm.total_active_stake_scaled = 0; // No stakers
        
        let initial_ts = farm.reward_infos[0].last_issuance_ts;
        refresh_global_reward(&mut farm, None, initial_ts + 100, 0).unwrap();
        
        // Timestamp should update but no rewards issued
        assert_eq!(farm.reward_infos[0].last_issuance_ts, initial_ts + 100);
        assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 0);
    }

    #[test]
    fn test_refresh_global_reward_with_oracle() {
        let mut farm = create_test_farm();
        farm.scope_oracle_price_id = 1; // Enable oracle
        farm.scope_oracle_max_age = 300;
        
        let price = DatedPrice {
            price: scope::Price { value: 200, exp: -2 }, // $2.00
            unix_timestamp: 1050,
            ..Default::default()
        };
        
        refresh_global_reward(&mut farm, Some(price), 1100, 0).unwrap();
        
        // Rewards should be adjusted by oracle price
        assert!(farm.reward_infos[0].rewards_issued_unclaimed > 0);
    }

    #[test]
    fn test_refresh_global_reward_oracle_too_old() {
        let mut farm = create_test_farm();
        farm.scope_oracle_price_id = 1;
        farm.scope_oracle_max_age = 300;
        
        let price = DatedPrice {
            price: scope::Price { value: 200, exp: -2 },
            unix_timestamp: 700, // Too old
            ..Default::default()
        };
        
        let result = refresh_global_reward(&mut farm, Some(price), 1100, 0);
        assert!(matches!(result, Err(e) if e.to_string().contains("ScopeOraclePriceTooOld")));
    }

    #[test]
    fn test_reward_issuance_cap() {
        let mut farm = create_test_farm();
        farm.reward_infos[0].rewards_available = 50; // Limited rewards
        farm.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::Constant { rate: 1000 };
        
        refresh_global_reward(&mut farm, None, 2000, 0).unwrap();
        
        // Should only issue up to available amount
        assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 50);
        assert_eq!(farm.reward_infos[0].rewards_available, 0);
    }

    #[test]
    fn test_harvest_with_treasury_fee() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        let global_config = GlobalConfig {
            treasury_fee_bps: 1000, // 10% fee
            ..Default::default()
        };
        
        // Give user some rewards
        user.rewards_issued_unclaimed[0] = 1000;
        user.last_claim_ts[0] = 0;
        farm.reward_infos[0].rewards_issued_unclaimed = 1000;
        
        let effects = harvest(&mut farm, &mut user, &global_config, None, 0, 1000).unwrap();
        
        assert_eq!(effects.reward_treasury, 100); // 10% fee
        assert_eq!(effects.reward_user, 900); // 90% to user
        assert_eq!(user.rewards_issued_unclaimed[0], 0); // Claimed
    }

    #[test]
    fn test_harvest_min_claim_duration() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        let global_config = GlobalConfig::default();
        
        user.rewards_issued_unclaimed[0] = 1000;
        user.last_claim_ts[0] = 900;
        farm.reward_infos[0].min_claim_duration_seconds = 200;
        farm.reward_infos[0].rewards_issued_unclaimed = 1000;
        
        // Too soon to claim
        let result = harvest(&mut farm, &mut user, &global_config, None, 0, 1000);
        assert!(matches!(result, Err(e) if e.to_string().contains("MinClaimDurationNotReached")));
        
        // Wait enough time
        let effects = harvest(&mut farm, &mut user, &global_config, None, 0, 1100).unwrap();
        assert!(effects.reward_user > 0);
    }

    #[test]
    fn test_stake_with_warmup_period() {
        let mut farm = create_test_farm();
        farm.deposit_warmup_period = 100;
        
        let mut user = UserState::default();
        
        let effects = stake(&mut farm, &mut user, None, 500, 1000).unwrap();
        
        // Should be pending, not active
        assert_eq!(user.pending_deposit_stake_scaled, effects.stake_gained_scaled);
        assert_eq!(user.active_stake_scaled, 0);
        assert_eq!(user.pending_deposit_stake_ts, 1100); // current + warmup
    }

    #[test]
    fn test_unstake_with_cooldown() {
        let mut farm = create_test_farm();
        farm.withdrawal_cooldown_period = 200;
        
        let mut user = create_test_user();
        user.active_stake_scaled = Decimal::from(100u64).to_scaled_val().unwrap();
        
        unstake(&mut farm, &mut user, None, Decimal::from(50u64), 1000).unwrap();
        
        // Should be pending withdrawal
        assert!(user.pending_withdrawal_unstake_scaled > 0);
        assert_eq!(user.pending_withdrawal_unstake_ts, 1200); // current + cooldown
    }

    #[test]
    fn test_deposit_cap_enforcement() {
        let mut farm = create_test_farm();
        farm.deposit_cap_amount = 2000;
        farm.scope_oracle_price_id = u64::MAX; // No oracle
        
        // Should accept within cap
        assert!(farm.can_accept_deposit(999, None, 1000).unwrap());
        
        // Should reject over cap
        assert!(!farm.can_accept_deposit(1001, None, 1000).unwrap());
        
        // Zero cap means no limit
        farm.deposit_cap_amount = 0;
        assert!(farm.can_accept_deposit(u64::MAX, None, 1000).unwrap());
    }

    #[test]
    fn test_reward_accumulation_over_time() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Simulate time passing and rewards accumulating
        for ts in (1100..1500).step_by(100) {
            refresh_global_reward(&mut farm, None, ts, 0).unwrap();
            user_refresh_reward(&mut farm, &mut user, 0).unwrap();
        }
        
        // User should have accumulated rewards
        assert!(user.rewards_issued_unclaimed[0] > 0);
        
        // Rewards should be proportional to stake
        let total_rewards = farm.reward_infos[0].rewards_issued_cumulative;
        let user_share = user.active_stake_scaled as f64 / farm.total_active_stake_scaled as f64;
        let expected_user_rewards = (total_rewards as f64 * user_share) as u64;
        
        // Allow for rounding differences
        let diff = if user.rewards_issued_unclaimed[0] > expected_user_rewards {
            user.rewards_issued_unclaimed[0] - expected_user_rewards
        } else {
            expected_user_rewards - user.rewards_issued_unclaimed[0]
        };
        assert!(diff <= 10); // Small rounding tolerance
    }

    #[test]
    fn test_delegated_farm_operations() {
        let mut farm = create_test_farm();
        farm.delegate_authority = Pubkey::new_unique();
        farm.is_farm_delegated = 1;
        
        let mut user = UserState::default();
        
        // In delegated mode, set_stake_delegated would be used instead
        // This test verifies the is_delegated checks work correctly
        assert!(farm.is_delegated());
        
        // Delegated farms use scaled values directly
        user.active_stake_scaled = 1000;
        user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    }

    #[test]
    fn test_slashed_amount_tracking() {
        let mut farm = create_test_farm();
        farm.locking_mode = 1; // WithExpiry
        farm.locking_duration = 1000;
        farm.locking_start_timestamp = 1000;
        farm.locking_early_withdrawal_penalty_bps = 5000;
        
        let mut user = create_test_user();
        user.active_stake_scaled = Decimal::from(100u64).to_scaled_val().unwrap();
        
        // Unstake with penalty
        unstake(&mut farm, &mut user, None, Decimal::from(100u64), 1500).unwrap();
        
        // Slashed amount should be tracked
        assert!(farm.slashed_amount_current > 0);
        assert_eq!(farm.slashed_amount_cumulative, farm.slashed_amount_current);
    }
}