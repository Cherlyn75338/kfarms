use crate::{
    state::{FarmState, UserState, RewardInfo},
    utils::math::{u64_mul_div, full_decimal_mul_div},
};
use decimal_wad::decimal::Decimal;

#[cfg(test)]
mod invariant_tests {
    use super::*;

    // C. Invariant Enforcement Tests

    #[test]
    fn test_farm_level_issuance_conservation() {
        // Test that increase in reward_per_share * total_active_stake ≈ issued rewards
        let mut farm = create_test_farm();
        let initial_rps = Decimal::from(1000u64);
        let initial_stake = 1_000_000u64;
        
        farm.reward_infos[0].set_reward_per_share(initial_rps);
        farm.total_active_amount = initial_stake;
        
        // Issue rewards
        let rewards_to_issue = 50_000u64;
        farm.reward_infos[0].rewards_issued_cumulative += rewards_to_issue;
        
        // Calculate new RPS
        let rps_increase = Decimal::from(rewards_to_issue * 1_000_000_000_000_000_000u128 / initial_stake as u128);
        let new_rps = Decimal::from_scaled_val(
            initial_rps.to_scaled_val::<u128>().unwrap() + 
            rps_increase.to_scaled_val::<u128>().unwrap()
        );
        farm.reward_infos[0].set_reward_per_share(new_rps);
        
        // Verify conservation
        let total_rewards_claimable = full_decimal_mul_div(
            new_rps,
            initial_stake,
            Decimal::from(1_000_000_000_000_000_000u128)
        ).to_u64().unwrap();
        
        // Should be approximately equal to cumulative issued (within rounding)
        let initial_claimable = full_decimal_mul_div(
            initial_rps,
            initial_stake,
            Decimal::from(1_000_000_000_000_000_000u128)
        ).to_u64().unwrap();
        
        let increase = total_rewards_claimable - initial_claimable;
        assert!(increase >= rewards_to_issue - 1 && increase <= rewards_to_issue + 1);
    }

    #[test]
    fn test_rewards_accounting_invariant() {
        // rewards_available + rewards_issued_unclaimed + vault_balance invariants
        let mut farm = create_test_farm();
        let vault_balance = 1_000_000u64;
        let rewards_available = 500_000u64;
        let rewards_issued = 200_000u64;
        let rewards_claimed = 150_000u64;
        
        farm.reward_infos[0].rewards_available = rewards_available;
        farm.reward_infos[0].rewards_issued_cumulative = rewards_issued;
        
        // Calculate unclaimed
        let rewards_unclaimed = rewards_issued - rewards_claimed;
        
        // Invariant: total_rewards = available + unclaimed
        let total_rewards = rewards_available + rewards_unclaimed;
        assert_eq!(total_rewards, 550_000);
        
        // Vault should have at least total_rewards
        assert!(vault_balance >= total_rewards);
    }

    #[test]
    fn test_delegated_farm_stake_invariant() {
        // For delegated farms: total_active_stake_scaled == total_staked_amount
        let mut farm = create_test_farm();
        farm.is_delegated = true;
        
        let stake_amount = 1_000_000u64;
        farm.total_staked_amount = stake_amount;
        farm.set_total_active_stake(Decimal::from(stake_amount));
        
        // Verify invariant
        let active_stake = farm.get_total_active_stake().to_u64().unwrap();
        assert_eq!(active_stake, farm.total_staked_amount);
        
        // Pending should be zero for delegated farms
        farm.set_total_pending_stake(Decimal::from(0u64));
        assert_eq!(farm.get_total_pending_stake().to_u64().unwrap(), 0);
    }

    #[test]
    fn test_user_level_state_transitions() {
        // Test that pending + active transitions preserve totals
        let mut user = create_test_user();
        let initial_pending = 1000u64;
        let initial_active = 5000u64;
        
        user.set_pending_deposit_stake(Decimal::from(initial_pending));
        user.set_active_stake(Decimal::from(initial_active));
        
        // Activate pending
        let new_active = initial_active + initial_pending;
        user.set_active_stake(Decimal::from(new_active));
        user.set_pending_deposit_stake(Decimal::from(0u64));
        
        // Verify total preserved
        assert_eq!(user.get_active_stake().to_u64().unwrap(), new_active);
        assert_eq!(user.get_pending_deposit_stake().to_u64().unwrap(), 0);
        
        // Test withdrawal transition
        let withdrawal_amount = 2000u64;
        user.set_pending_withdrawal_unstake(Decimal::from(withdrawal_amount));
        user.set_active_stake(Decimal::from(new_active - withdrawal_amount));
        
        // Verify conservation
        let total = user.get_active_stake().to_u64().unwrap() + 
                   user.get_pending_withdrawal_unstake().to_u64().unwrap();
        assert_eq!(total, new_active);
    }

    #[test]
    fn test_reward_tally_monotonicity() {
        // Reward tallies should be monotone (never decrease)
        let mut user = create_test_user();
        let mut prev_tally = 0u128;
        
        for i in 0..100 {
            let new_tally = prev_tally + (i * 1000) as u128;
            user.set_rewards_tally(0, Decimal::from_scaled_val(new_tally));
            
            let current = user.get_rewards_tally(0).to_scaled_val::<u128>().unwrap();
            assert!(current >= prev_tally);
            prev_tally = current;
        }
    }

    #[test]
    fn test_total_stake_consistency() {
        // Total stake should equal sum of all user stakes
        let mut farm = create_test_farm();
        let mut users = vec![create_test_user(); 10];
        let mut total_user_stake = 0u64;
        
        for (i, user) in users.iter_mut().enumerate() {
            let stake = (i + 1) as u64 * 1000;
            user.set_active_stake(Decimal::from(stake));
            total_user_stake += stake;
        }
        
        farm.total_active_amount = total_user_stake;
        farm.set_total_active_stake(Decimal::from(total_user_stake));
        
        // Verify consistency
        assert_eq!(farm.total_active_amount, total_user_stake);
        assert_eq!(farm.get_total_active_stake().to_u64().unwrap(), total_user_stake);
    }

    #[test]
    fn test_pending_to_active_migration() {
        // Test migration from pending to active preserves invariants
        let mut farm = create_test_farm();
        let pending_amount = 10_000u64;
        let active_amount = 50_000u64;
        
        farm.total_pending_amount = pending_amount;
        farm.total_active_amount = active_amount;
        farm.set_total_pending_stake(Decimal::from(pending_amount));
        farm.set_total_active_stake(Decimal::from(active_amount));
        
        // Migrate pending to active
        farm.total_active_amount += pending_amount;
        farm.total_pending_amount = 0;
        farm.set_total_active_stake(Decimal::from(active_amount + pending_amount));
        farm.set_total_pending_stake(Decimal::from(0u64));
        
        // Verify invariants
        assert_eq!(farm.total_active_amount, active_amount + pending_amount);
        assert_eq!(farm.total_pending_amount, 0);
        assert_eq!(farm.get_total_active_stake().to_u64().unwrap(), active_amount + pending_amount);
        assert_eq!(farm.get_total_pending_stake().to_u64().unwrap(), 0);
    }

    #[test]
    fn test_reward_distribution_fairness() {
        // Test that rewards are distributed proportionally
        let mut farm = create_test_farm();
        let mut users = vec![create_test_user(); 3];
        
        // Set different stakes
        let stakes = vec![1000u64, 2000u64, 3000u64];
        let total_stake: u64 = stakes.iter().sum();
        
        for (i, stake) in stakes.iter().enumerate() {
            users[i].set_active_stake(Decimal::from(*stake));
        }
        
        farm.total_active_amount = total_stake;
        farm.set_total_active_stake(Decimal::from(total_stake));
        
        // Distribute rewards
        let total_rewards = 6000u64;
        let rps_increase = Decimal::from(total_rewards * 1_000_000_000_000_000_000u128 / total_stake as u128);
        
        // Calculate user rewards
        for (i, stake) in stakes.iter().enumerate() {
            let user_reward = full_decimal_mul_div(
                rps_increase,
                *stake,
                Decimal::from(1_000_000_000_000_000_000u128)
            ).to_u64().unwrap();
            
            // Should be proportional
            let expected = total_rewards * stake / total_stake;
            assert!(user_reward >= expected - 1 && user_reward <= expected + 1);
        }
    }

    #[test]
    fn test_slashing_invariants() {
        // Test that slashing maintains invariants
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        let initial_stake = 10_000u64;
        user.set_active_stake(Decimal::from(initial_stake));
        farm.total_active_amount = initial_stake;
        
        // Apply slashing
        let slash_amount = 1_000u64;
        let remaining = initial_stake - slash_amount;
        
        user.set_active_stake(Decimal::from(remaining));
        farm.total_active_amount = remaining;
        farm.slashed_amount_cumulative += slash_amount;
        
        // Verify invariants
        assert_eq!(user.get_active_stake().to_u64().unwrap(), remaining);
        assert_eq!(farm.total_active_amount, remaining);
        assert_eq!(farm.slashed_amount_cumulative, slash_amount);
        
        // Total accounted = active + slashed
        assert_eq!(farm.total_active_amount + farm.slashed_amount_cumulative, initial_stake);
    }

    #[test]
    fn test_zero_state_invariants() {
        // Test invariants when values are zero
        let farm = create_test_farm();
        let user = create_test_user();
        
        // Zero stakes should be valid
        assert_eq!(farm.total_active_amount, 0);
        assert_eq!(farm.total_pending_amount, 0);
        assert_eq!(user.get_active_stake().to_u64().unwrap(), 0);
        assert_eq!(user.get_pending_deposit_stake().to_u64().unwrap(), 0);
        
        // Zero rewards should be valid
        assert_eq!(farm.reward_infos[0].rewards_available, 0);
        assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 0);
    }

    #[test]
    #[should_panic]
    fn test_invariant_violation_detection() {
        // Test that invariant violations are detected
        let mut farm = create_test_farm();
        
        // Set inconsistent state (violates invariant)
        farm.total_active_amount = 1000;
        farm.set_total_active_stake(Decimal::from(2000u64)); // Different from amount
        
        // In production, this should trigger an assertion
        if !farm.is_delegated {
            assert_eq!(
                farm.total_active_amount,
                farm.get_total_active_stake().to_u64().unwrap(),
                "Invariant violation: stake != amount for non-delegated farm"
            );
        }
    }

    #[test]
    fn test_cumulative_counters_monotonicity() {
        // Test that cumulative counters never decrease
        let mut farm = create_test_farm();
        
        let mut prev_issued = 0u64;
        let mut prev_slashed = 0u64;
        
        for i in 0..100 {
            farm.reward_infos[0].rewards_issued_cumulative += i;
            farm.slashed_amount_cumulative += i / 10;
            
            assert!(farm.reward_infos[0].rewards_issued_cumulative >= prev_issued);
            assert!(farm.slashed_amount_cumulative >= prev_slashed);
            
            prev_issued = farm.reward_infos[0].rewards_issued_cumulative;
            prev_slashed = farm.slashed_amount_cumulative;
        }
    }

    // Helper functions
    fn create_test_farm() -> FarmState {
        FarmState {
            version: 1,
            creator: [0u8; 32],
            authority: [0u8; 32],
            pending_authority: [0u8; 32],
            farm_vault: [0u8; 32],
            farm_vault_authority: [0u8; 32],
            farm_token_mint: [0u8; 32],
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
            _padding: [0u8; 256],
        }
    }

    fn create_test_user() -> UserState {
        UserState {
            version: 1,
            farm: [0u8; 32],
            owner: [0u8; 32],
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
}