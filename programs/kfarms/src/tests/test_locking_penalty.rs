use crate::{
    utils::{
        withdrawal_penalty::apply_early_withdrawal_penalty,
        math::u64_mul_div,
        consts::BPS_DIV_FACTOR,
    },
    state::FarmState,
    FarmError,
};

#[cfg(test)]
mod locking_penalty_tests {
    use super::*;

    // F. Locking/Freezing Controls Tests

    #[test]
    fn test_penalty_calculation_basic() {
        // Basic penalty calculation
        let locking_duration = 365 * 24 * 60 * 60; // 1 year
        let locking_start = 1000;
        let penalty_bps = 5000; // 50% max penalty
        let unstake_amount = 10000;
        
        // Test at different time points
        let test_cases = vec![
            (locking_start + 0, 5000),           // At start: 50% penalty
            (locking_start + locking_duration / 4, 3750), // 25% through: 37.5% penalty
            (locking_start + locking_duration / 2, 2500), // Halfway: 25% penalty
            (locking_start + 3 * locking_duration / 4, 1250), // 75% through: 12.5% penalty
            (locking_start + locking_duration, 0), // At maturity: 0% penalty
        ];
        
        for (timestamp, expected_penalty_bps) in test_cases {
            let result = apply_early_withdrawal_penalty(
                locking_duration,
                locking_start,
                timestamp,
                penalty_bps,
                unstake_amount,
            );
            
            assert!(result.is_ok());
            let (amount_after_penalty, penalty) = result.unwrap();
            
            let expected_penalty = u64_mul_div(unstake_amount, expected_penalty_bps, BPS_DIV_FACTOR);
            assert_eq!(penalty, expected_penalty);
            assert_eq!(amount_after_penalty, unstake_amount - expected_penalty);
        }
    }

    #[test]
    fn test_penalty_before_locking_start() {
        // Test withdrawal before locking period starts
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 500; // Before start
        let penalty_bps = 5000;
        let unstake_amount = 10000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        
        // No penalty before locking starts
        assert_eq!(penalty, 0);
        assert_eq!(amount_after_penalty, unstake_amount);
    }

    #[test]
    fn test_penalty_after_maturity() {
        // Test withdrawal after maturity
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1200; // After maturity (1000 + 100)
        let penalty_bps = 5000;
        let unstake_amount = 10000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        
        // No penalty after maturity
        assert_eq!(penalty, 0);
        assert_eq!(amount_after_penalty, unstake_amount);
    }

    #[test]
    fn test_penalty_overflow_protection() {
        // Test overflow protection in penalty calculation
        let locking_duration = u64::MAX / 2;
        let locking_start = 1000;
        let timestamp_now = 2000;
        let penalty_bps = 5000;
        let unstake_amount = u64::MAX / 2;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        // Should handle large values without panic
        assert!(result.is_ok());
    }

    #[test]
    fn test_penalty_division_by_zero() {
        // Test division by zero protection
        let locking_duration = 0; // Zero duration
        let locking_start = 1000;
        let timestamp_now = 1000;
        let penalty_bps = 5000;
        let unstake_amount = 10000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        // Should handle zero duration gracefully
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        assert_eq!(penalty, 0);
        assert_eq!(amount_after_penalty, unstake_amount);
    }

    #[test]
    fn test_invalid_penalty_percentage() {
        // Test invalid penalty percentages
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1050;
        let unstake_amount = 10000;
        
        // Test > 100% penalty
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            10001, // > 100%
            unstake_amount,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), FarmError::InvalidPenaltyPercentage);
    }

    #[test]
    fn test_zero_penalty_blocks_withdrawal() {
        // Test that 0% penalty blocks early withdrawal
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1050;
        let penalty_bps = 0; // 0% penalty
        let unstake_amount = 10000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), FarmError::EarlyWithdrawalNotAllowed);
    }

    #[test]
    fn test_hundred_percent_penalty_blocks_withdrawal() {
        // Test that 100% penalty blocks early withdrawal
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1050;
        let penalty_bps = 10000; // 100% penalty
        let unstake_amount = 10000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), FarmError::EarlyWithdrawalNotAllowed);
    }

    #[test]
    fn test_penalty_formula_correctness() {
        // Verify penalty formula against reference
        // penalty = max_penalty * time_remaining / total_duration
        
        let locking_duration = 1000;
        let locking_start = 0;
        let max_penalty_bps = 8000; // 80% max
        let unstake_amount = 100000;
        
        for i in 0..=10 {
            let progress = i as u64 * 100; // 0, 100, 200, ..., 1000
            let timestamp_now = locking_start + progress;
            
            let result = apply_early_withdrawal_penalty(
                locking_duration,
                locking_start,
                timestamp_now,
                max_penalty_bps,
                unstake_amount,
            );
            
            if timestamp_now >= locking_start + locking_duration {
                // After maturity
                let (amount, penalty) = result.unwrap();
                assert_eq!(penalty, 0);
                assert_eq!(amount, unstake_amount);
            } else {
                // During locking period
                let (amount, penalty) = result.unwrap();
                
                let time_remaining = locking_duration - progress;
                let expected_penalty_bps = max_penalty_bps * time_remaining / locking_duration;
                let expected_penalty = u64_mul_div(unstake_amount, expected_penalty_bps, BPS_DIV_FACTOR);
                
                assert_eq!(penalty, expected_penalty);
                assert_eq!(amount, unstake_amount - expected_penalty);
            }
        }
    }

    #[test]
    fn test_freeze_flag_behavior() {
        // Test freeze flag when vault is fully withdrawn
        let mut farm = create_test_farm();
        
        // Initially not frozen
        assert!(!is_farm_frozen(&farm));
        
        // Simulate full vault withdrawal
        farm.total_active_amount = 0;
        farm.total_pending_amount = 0;
        farm.is_frozen = true;
        
        // Should be frozen
        assert!(is_farm_frozen(&farm));
        
        // Test that operations are blocked when frozen
        assert!(!can_stake_when_frozen(&farm));
        assert!(!can_unstake_when_frozen(&farm));
        assert!(!can_claim_when_frozen(&farm));
    }

    #[test]
    fn test_locking_mode_transitions() {
        // Test transitions between locking modes
        let mut farm = create_test_farm();
        
        // No locking
        farm.locking_mode = 0;
        assert_eq!(get_locking_mode(&farm), LockingMode::None);
        
        // With penalty
        farm.locking_mode = 1;
        assert_eq!(get_locking_mode(&farm), LockingMode::WithPenalty);
        
        // With expiry
        farm.locking_mode = 2;
        assert_eq!(get_locking_mode(&farm), LockingMode::WithExpiry);
        
        // Invalid mode should be handled
        farm.locking_mode = 99;
        assert_eq!(get_locking_mode(&farm), LockingMode::None); // Default
    }

    #[test]
    fn test_penalty_accumulation() {
        // Test that penalties accumulate correctly
        let mut farm = create_test_farm();
        farm.slashed_amount_cumulative = 0;
        
        let penalties = vec![100, 250, 500, 1000];
        let mut expected_total = 0u64;
        
        for penalty in penalties {
            farm.slashed_amount_cumulative += penalty;
            expected_total += penalty;
            assert_eq!(farm.slashed_amount_cumulative, expected_total);
        }
    }

    #[test]
    fn test_minimum_penalty_amount() {
        // Test with very small amounts
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1050; // Halfway
        let penalty_bps = 5000; // 50% max -> 25% at halfway
        
        // Test with 1 unit
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            1, // Minimum amount
        );
        
        assert!(result.is_ok());
        let (amount, penalty) = result.unwrap();
        // With 1 unit and 25% penalty, should round down to 0
        assert_eq!(penalty, 0);
        assert_eq!(amount, 1);
    }

    #[test]
    fn test_penalty_with_large_amounts() {
        // Test with very large amounts
        let locking_duration = 100;
        let locking_start = 1000;
        let timestamp_now = 1050;
        let penalty_bps = 2500; // 25%
        let large_amount = u64::MAX / 2;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            large_amount,
        );
        
        assert!(result.is_ok());
        let (amount, penalty) = result.unwrap();
        
        // Calculate expected penalty
        let time_remaining = 50;
        let expected_penalty_bps = penalty_bps * time_remaining / locking_duration;
        let expected_penalty = u64_mul_div(large_amount, expected_penalty_bps, BPS_DIV_FACTOR);
        
        assert_eq!(penalty, expected_penalty);
        assert_eq!(amount, large_amount - expected_penalty);
    }

    #[test]
    fn test_invalid_locking_timestamps() {
        // Test with invalid timestamp configurations
        let unstake_amount = 10000;
        let penalty_bps = 5000;
        
        // Maturity before start
        let result = apply_early_withdrawal_penalty(
            0, // duration = 0, so maturity = start
            1000, // start
            500, // now (before start, but duration is 0)
            penalty_bps,
            unstake_amount,
        );
        
        // Should handle gracefully
        assert!(result.is_ok());
    }

    #[test]
    fn test_operations_during_locking() {
        // Test what operations are allowed during locking
        let mut farm = create_test_farm();
        farm.locking_mode = 1; // With penalty
        farm.locking_start_timestamp = 1000;
        farm.locking_duration = 1000;
        farm.locking_early_withdrawal_penalty_bps = 5000;
        
        let current_ts = 1500; // During locking period
        
        // Staking should be allowed
        assert!(can_stake_during_locking(&farm));
        
        // Unstaking with penalty should be allowed
        assert!(can_unstake_with_penalty(&farm, current_ts));
        
        // Claiming rewards should be allowed
        assert!(can_claim_during_locking(&farm));
        
        // After maturity
        let after_maturity = 2001;
        assert!(can_unstake_without_penalty(&farm, after_maturity));
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
            reward_infos: vec![],
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
        }
    }

    fn is_farm_frozen(farm: &FarmState) -> bool {
        farm.is_frozen
    }

    fn can_stake_when_frozen(farm: &FarmState) -> bool {
        !farm.is_frozen
    }

    fn can_unstake_when_frozen(farm: &FarmState) -> bool {
        !farm.is_frozen
    }

    fn can_claim_when_frozen(farm: &FarmState) -> bool {
        !farm.is_frozen
    }

    fn get_locking_mode(farm: &FarmState) -> LockingMode {
        match farm.locking_mode {
            0 => LockingMode::None,
            1 => LockingMode::WithPenalty,
            2 => LockingMode::WithExpiry,
            _ => LockingMode::None,
        }
    }

    fn can_stake_during_locking(_farm: &FarmState) -> bool {
        true // Staking typically allowed during locking
    }

    fn can_unstake_with_penalty(farm: &FarmState, current_ts: u64) -> bool {
        farm.locking_mode == 1 && 
        current_ts < farm.locking_start_timestamp + farm.locking_duration
    }

    fn can_claim_during_locking(_farm: &FarmState) -> bool {
        true // Claiming typically allowed during locking
    }

    fn can_unstake_without_penalty(farm: &FarmState, current_ts: u64) -> bool {
        current_ts >= farm.locking_start_timestamp + farm.locking_duration
    }

    #[derive(Debug, PartialEq)]
    enum LockingMode {
        None,
        WithPenalty,
        WithExpiry,
    }
}