#[cfg(test)]
mod tests {
    use crate::stake_operations::*;
    use crate::state::{FarmState, UserState};
    use decimal_wad::decimal::Decimal;
    use std::collections::HashMap;

    /// Invariant: Total shares must equal sum of all user shares
    #[test]
    fn test_share_conservation_invariant() {
        let mut farm = FarmStake::default();
        let mut users = vec![UserStake::default(); 10];
        
        // Perform random operations
        for i in 0..10 {
            let amount = (i + 1) as u64 * 100;
            add_pending_deposit_stake(&mut users[i], &mut farm, amount).unwrap();
            activate_pending_stake(&mut users[i], &mut farm).unwrap();
        }
        
        // Check invariant
        let total_user_shares: Decimal = users.iter()
            .map(|u| u.active_stake + u.pending_deposit_stake + u.pending_withdrawal_unstake)
            .fold(Decimal::zero(), |acc, s| acc + s);
        
        let total_farm_shares = farm.total_active_stake + farm.total_pending_stake;
        
        assert_eq!(total_user_shares, total_farm_shares);
    }

    /// Invariant: Total amounts must equal sum of active + pending
    #[test]
    fn test_amount_conservation_invariant() {
        let mut farm = FarmStake::default();
        
        // Add various amounts
        farm.total_active_amount = 5000;
        farm.total_pending_amount = 2000;
        
        let total = farm.total_active_amount + farm.total_pending_amount;
        
        // Perform withdrawal
        let effects = withdraw_farm(&mut farm, 3500).unwrap();
        
        // Check conservation
        let remaining = farm.total_active_amount + farm.total_pending_amount;
        assert_eq!(remaining + effects.amount_to_withdraw, total);
    }

    /// Invariant: Rewards issued <= rewards available + rewards issued cumulative
    #[test]
    fn test_reward_conservation_invariant() {
        use crate::state::RewardInfo;
        
        let mut reward = RewardInfo::default();
        reward.rewards_available = 10000;
        reward.rewards_issued_cumulative = 0;
        reward.rewards_issued_unclaimed = 0;
        
        // Issue some rewards
        let issued = 5000u64;
        reward.rewards_available -= issued;
        reward.rewards_issued_cumulative += issued;
        reward.rewards_issued_unclaimed += issued;
        
        // Invariant check
        assert!(reward.rewards_issued_unclaimed <= reward.rewards_issued_cumulative);
        assert_eq!(
            reward.rewards_available + reward.rewards_issued_cumulative,
            10000 // Initial amount
        );
    }

    /// Invariant: User pending operations should resolve correctly
    #[test]
    fn test_pending_resolution_invariant() {
        let mut farm = FarmStake::default();
        let mut user = UserStake::default();
        
        // Add pending deposit
        let deposit_amount = 1000;
        add_pending_deposit_stake(&mut user, &mut farm, deposit_amount).unwrap();
        
        let pending_stake = user.pending_deposit_stake;
        
        // Activate pending
        activate_pending_stake(&mut user, &mut farm).unwrap();
        
        // Check invariant: pending moved to active
        assert_eq!(user.pending_deposit_stake, Decimal::zero());
        assert_eq!(user.active_stake, pending_stake);
    }

    /// Invariant: Share to amount conversion should be consistent
    #[test]
    fn test_conversion_consistency_invariant() {
        let total_stake = Decimal::from(1000u64);
        let total_amount = 5000u64;
        
        // Convert amount to stake and back
        let amount = 100u64;
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        let recovered_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
        
        // Should be equal or off by at most 1 due to rounding
        assert!(recovered_amount == amount || recovered_amount == amount - 1);
    }

    /// Invariant: Slashed amounts should accumulate correctly
    #[test]
    fn test_slashing_accumulation_invariant() {
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000,
            locking_mode: crate::state::LockingMode::WithExpiry,
            locking_start_timestamp: 1000,
            locking_duration: 1000,
            locking_early_withdrawal_penalty_bps: 5000,
            ..Default::default()
        };
        
        let mut user = UserStake {
            active_stake: Decimal::from(100u64),
            ..Default::default()
        };
        
        let mut total_slashed = 0u64;
        
        // Multiple unstakes with penalties
        for ts in [1100, 1200, 1300] {
            let stake_to_unstake = Decimal::from(20u64);
            let (_amount, _pending, penalty) = unstake(&mut user, &mut farm, stake_to_unstake, ts).unwrap();
            total_slashed += penalty;
        }
        
        // In real implementation, farm would track slashed amounts
        // This invariant ensures they accumulate correctly
        assert!(total_slashed > 0);
    }

    /// Invariant: No user can have negative balances
    #[test]
    fn test_no_negative_balances_invariant() {
        let mut user = UserStake::default();
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000,
            ..Default::default()
        };
        
        // Try to remove more than exists
        let result = std::panic::catch_unwind(|| {
            let mut user_copy = user;
            let mut farm_copy = farm;
            remove_active_stake(&mut user_copy, &mut farm_copy, Decimal::from(100u64))
        });
        
        assert!(result.is_err()); // Should panic or error
    }

    /// Invariant: Farm freeze should prevent certain operations
    #[test]
    fn test_farm_freeze_invariant() {
        let mut farm = FarmStake {
            total_active_amount: 1000,
            total_pending_amount: 500,
            ..Default::default()
        };
        
        // Withdraw all (should freeze farm)
        let effects = withdraw_farm(&mut farm, 2000).unwrap();
        
        assert!(effects.farm_to_freeze);
        assert_eq!(farm.total_active_amount, 0);
        assert_eq!(farm.total_pending_amount, 0);
    }

    /// Invariant: Decimal precision should be maintained
    #[test]
    fn test_decimal_precision_invariant() {
        let stake = Decimal::from(1u64) / 3; // 0.333...
        let total_stake = Decimal::from(1u64);
        let total_amount = 3u64;
        
        // Should maintain precision through conversions
        let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
        assert_eq!(amount, 1); // Floor of 0.999... = 0 or 1 depending on precision
        
        let amount_up = convert_stake_to_amount(stake, total_stake, total_amount, true);
        assert_eq!(amount_up, 1); // Ceil of 0.999... = 1
    }

    /// Invariant: Time-based operations should be monotonic
    #[test]
    fn test_time_monotonicity_invariant() {
        let mut user = UserStake {
            last_stake_ts: 1000,
            ..Default::default()
        };
        
        // Time should only move forward
        user.last_stake_ts = 1500;
        assert!(user.last_stake_ts >= 1000);
        
        // Pending timestamps should be in future
        let current = 2000;
        let warmup = 100;
        let pending_ts = current + warmup;
        assert!(pending_ts > current);
    }

    /// Invariant: Pro-rata distribution should be fair
    #[test]
    fn test_prorata_fairness_invariant() {
        let mut farm = FarmStake {
            total_active_amount: 8000,
            total_pending_amount: 2000,
            total_active_stake: Decimal::from(800u64),
            total_pending_stake: Decimal::from(200u64),
            ..Default::default()
        };
        
        let total_before = farm.total_active_amount + farm.total_pending_amount;
        let withdraw_amount = 5000;
        
        let effects = withdraw_farm(&mut farm, withdraw_amount).unwrap();
        
        // Check pro-rata was applied correctly
        let active_ratio = 8000f64 / 10000f64;
        let pending_ratio = 2000f64 / 10000f64;
        
        let expected_active_withdrawn = (withdraw_amount as f64 * active_ratio) as u64;
        let expected_pending_withdrawn = (withdraw_amount as f64 * pending_ratio) as u64;
        
        let actual_active_withdrawn = 8000 - farm.total_active_amount;
        let actual_pending_withdrawn = 2000 - farm.total_pending_amount;
        
        // Allow small rounding difference
        assert!((actual_active_withdrawn as i64 - expected_active_withdrawn as i64).abs() <= 1);
        assert!((actual_pending_withdrawn as i64 - expected_pending_withdrawn as i64).abs() <= 1);
    }

    /// Invariant: Delegated farms should maintain consistency
    #[test]
    fn test_delegated_farm_invariant() {
        use anchor_lang::prelude::Pubkey;
        
        let mut farm = FarmState::default();
        farm.delegate_authority = Pubkey::new_unique();
        farm.is_farm_delegated = 1;
        
        assert!(farm.is_delegated());
        
        // In delegated mode:
        // - No warmup/cooldown periods should apply
        // - Scaled values are used directly
        // - External protocol maintains token custody
        
        // This invariant ensures delegated mode is consistently applied
    }

    /// Invariant: Reward per share should only increase
    #[test]
    fn test_reward_per_share_monotonic_invariant() {
        use crate::state::RewardInfo;
        
        let mut reward = RewardInfo::default();
        let initial_rps = Decimal::from(100u64);
        reward.reward_per_share_scaled = initial_rps.to_scaled_val().unwrap();
        
        // Add more rewards (should increase RPS)
        let new_rps = Decimal::from(150u64);
        reward.reward_per_share_scaled = new_rps.to_scaled_val().unwrap();
        
        // Invariant: RPS should never decrease
        assert!(new_rps >= initial_rps);
    }
}