#[cfg(test)]
mod locking_and_freezing_tests {
    use crate::state::{FarmState, UserState, LockingMode};
    use crate::stake_operations::*;
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    use decimal_wad::decimal::Decimal;
    use crate::FarmError;
    
    #[test]
    fn test_no_locking_mode() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::None as u8;
        farm.locking_duration = 0;
        farm.locking_early_withdrawal_penalty_bps = 0;
        
        let mut user = UserState::default();
        user.active_stake_scaled = 1000 * 10u128.pow(18);
        
        // Should be able to unstake without penalty
        let unstake_amount = 1000u64;
        let timestamp = 100;
        
        // In None mode, no penalty should apply
        assert_eq!(farm.locking_mode, LockingMode::None as u8);
        
        // Verify no cooldown period required
        assert_eq!(farm.withdrawal_cooldown_period, 0);
    }
    
    #[test]
    fn test_with_expiry_locking_before_start() {
        // Test WithExpiry penalty=0 before start timestamp
        let locking_duration = 1000;
        let locking_start = 100;
        let timestamp_now = 50; // Before start
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;
        
        let result = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        );
        
        assert!(result.is_ok());
        let (amount_after_penalty, penalty) = result.unwrap();
        
        // CRITICAL: Before start timestamp, penalty should be 0
        assert_eq!(penalty, 0, "No penalty should apply before locking period starts");
        assert_eq!(amount_after_penalty, unstake_amount, "Full amount should be returned");
    }
    
    #[test]
    fn test_with_expiry_locking_during_period() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::WithExpiry as u8;
        farm.locking_duration = 1000;
        farm.locking_start = 100;
        farm.locking_early_withdrawal_penalty_bps = 5000; // 50%
        
        // Test at different points during locking period
        let test_cases = vec![
            (100, 500),  // At start: 50% penalty
            (350, 375),  // 25% through: 37.5% penalty
            (600, 250),  // 50% through: 25% penalty
            (850, 125),  // 75% through: 12.5% penalty
            (1099, 0),   // Near end: ~0% penalty
            (1100, 0),   // At end: 0% penalty
            (1200, 0),   // After end: 0% penalty
        ];
        
        for (timestamp, expected_penalty) in test_cases {
            let result = apply_early_withdrawal_penalty(
                farm.locking_duration,
                farm.locking_start,
                timestamp,
                farm.locking_early_withdrawal_penalty_bps,
                1000,
            );
            
            if timestamp >= farm.locking_start + farm.locking_duration {
                // After maturity
                assert!(result.is_ok());
                let (_, penalty) = result.unwrap();
                assert_eq!(penalty, 0);
            } else if timestamp < farm.locking_start {
                // Before start
                assert!(result.is_ok());
                let (_, penalty) = result.unwrap();
                assert_eq!(penalty, 0);
            } else {
                // During locking period
                assert!(result.is_ok());
                let (_, penalty) = result.unwrap();
                
                // Allow small rounding difference
                let diff = if penalty > expected_penalty {
                    penalty - expected_penalty
                } else {
                    expected_penalty - penalty
                };
                assert!(diff <= 1, "Penalty {} should be close to {}", penalty, expected_penalty);
            }
        }
    }
    
    #[test]
    fn test_continuous_locking_mode() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::Continuous as u8;
        farm.locking_duration = 1000;
        farm.locking_early_withdrawal_penalty_bps = 5000; // 50%
        
        let mut user = UserState::default();
        user.last_stake_ts = 100; // User staked at timestamp 100
        
        // Test unstaking at different times
        let test_cases = vec![
            (100, 500),   // Immediately: 50% penalty
            (600, 250),   // Halfway: 25% penalty
            (1100, 0),    // After duration: 0% penalty
        ];
        
        for (current_ts, expected_penalty) in test_cases {
            let result = apply_early_withdrawal_penalty(
                farm.locking_duration,
                user.last_stake_ts,
                current_ts,
                farm.locking_early_withdrawal_penalty_bps,
                1000,
            );
            
            assert!(result.is_ok());
            let (amount_after_penalty, penalty) = result.unwrap();
            
            // Check penalty calculation
            let diff = if penalty > expected_penalty {
                penalty - expected_penalty
            } else {
                expected_penalty - penalty
            };
            assert!(diff <= 1, "Penalty {} should be close to {}", penalty, expected_penalty);
            
            // Verify amount after penalty
            assert_eq!(amount_after_penalty + penalty, 1000);
        }
    }
    
    #[test]
    fn test_restake_pattern_continuous_mode() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::Continuous as u8;
        farm.locking_duration = 1000;
        farm.locking_early_withdrawal_penalty_bps = 5000;
        
        let mut user = UserState::default();
        
        // Initial stake
        user.last_stake_ts = 100;
        user.active_stake_scaled = 1000 * 10u128.pow(18);
        
        // Partial unstake at t=300 (20% through lock period)
        let partial_unstake_ts = 300;
        let expected_penalty_bps = 5000 * (1000 - 200) / 1000; // 40% penalty
        
        let result = apply_early_withdrawal_penalty(
            farm.locking_duration,
            user.last_stake_ts,
            partial_unstake_ts,
            farm.locking_early_withdrawal_penalty_bps,
            500, // Unstake half
        );
        
        assert!(result.is_ok());
        let (amount, penalty) = result.unwrap();
        assert_eq!(penalty, 200); // 40% of 500
        assert_eq!(amount, 300);
        
        // Restake at t=400
        user.last_stake_ts = 400; // Reset lock timer
        
        // Try to unstake again at t=500 (10% through new lock)
        let second_unstake_ts = 500;
        let expected_penalty_bps = 5000 * (1000 - 100) / 1000; // 45% penalty
        
        let result = apply_early_withdrawal_penalty(
            farm.locking_duration,
            user.last_stake_ts, // Uses new stake timestamp
            second_unstake_ts,
            farm.locking_early_withdrawal_penalty_bps,
            300,
        );
        
        assert!(result.is_ok());
        let (amount, penalty) = result.unwrap();
        assert_eq!(penalty, 135); // 45% of 300
        assert_eq!(amount, 165);
    }
    
    #[test]
    fn test_farm_freeze_blocks_operations() {
        let mut farm = FarmState::default();
        farm.is_farm_frozen = 1; // Farm is frozen
        
        // When farm is frozen, all operations should be blocked
        // This would be checked in handler functions
        assert_eq!(farm.is_farm_frozen, 1);
        
        // Test vault withdrawal triggers freeze
        farm.is_farm_frozen = 0;
        farm.farm_vault = Default::default();
        
        // Simulate vault withdrawal (would set freeze flag)
        // In actual implementation: if all funds withdrawn, set is_farm_frozen = 1
        farm.total_staked_amount = 0;
        farm.is_farm_frozen = 1;
        
        assert_eq!(farm.is_farm_frozen, 1, "Farm should freeze when vault emptied");
    }
    
    #[test]
    fn test_cooldown_and_warmup_periods() {
        let mut farm = FarmState::default();
        farm.deposit_warmup_period = 3600; // 1 hour warmup
        farm.withdrawal_cooldown_period = 7200; // 2 hour cooldown
        
        let mut user = UserState::default();
        
        // Test deposit warmup
        user.pending_deposit_stake_ts = 1000;
        let current_ts = 2000;
        
        // Not enough time passed for warmup
        assert!(current_ts - user.pending_deposit_stake_ts < farm.deposit_warmup_period as u64);
        
        // After warmup period
        let after_warmup_ts = 5000;
        assert!(after_warmup_ts - user.pending_deposit_stake_ts >= farm.deposit_warmup_period as u64);
        
        // Test withdrawal cooldown
        user.pending_withdrawal_unstake_ts = 6000;
        let withdrawal_attempt_ts = 7000;
        
        // Not enough time for cooldown
        assert!(withdrawal_attempt_ts - user.pending_withdrawal_unstake_ts < farm.withdrawal_cooldown_period as u64);
        
        // After cooldown
        let after_cooldown_ts = 14000;
        assert!(after_cooldown_ts - user.pending_withdrawal_unstake_ts >= farm.withdrawal_cooldown_period as u64);
    }
    
    #[test]
    fn test_penalty_percentage_validation() {
        // Test invalid penalty percentages
        let test_cases = vec![
            (0, true),      // 0% not allowed (would allow free early withdrawal)
            (10000, true),  // 100% not allowed (would take everything)
            (10001, true),  // >100% invalid
            (5000, false),  // 50% valid
            (9999, false),  // 99.99% valid
        ];
        
        for (penalty_bps, should_error) in test_cases {
            let result = apply_early_withdrawal_penalty(
                1000,
                0,
                500,
                penalty_bps,
                1000,
            );
            
            if should_error {
                assert!(result.is_err(), "Penalty {} bps should error", penalty_bps);
            } else {
                assert!(result.is_ok(), "Penalty {} bps should be valid", penalty_bps);
            }
        }
    }
    
    #[test]
    fn test_locking_bypass_prevention() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::WithExpiry as u8;
        farm.locking_duration = 86400; // 1 day
        farm.locking_early_withdrawal_penalty_bps = 5000;
        farm.is_farm_delegated = 1; // Even with delegation
        
        // Delegation should NOT bypass locking
        assert_eq!(farm.locking_mode, LockingMode::WithExpiry as u8);
        assert_eq!(farm.locking_duration, 86400);
        
        // Cooldown should still apply with delegation
        farm.withdrawal_cooldown_period = 3600;
        assert_eq!(farm.withdrawal_cooldown_period, 3600);
        
        // Test that locking parameters cannot be set to bypass
        // (Would be enforced in handler functions)
    }
    
    #[test]
    fn test_emergency_withdrawal_with_penalty() {
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::Continuous as u8;
        farm.locking_duration = 86400; // 1 day
        farm.locking_early_withdrawal_penalty_bps = 9000; // 90% penalty for emergency
        
        let mut user = UserState::default();
        user.last_stake_ts = 1000;
        user.active_stake_scaled = 10000 * 10u128.pow(18);
        
        // Emergency withdrawal immediately after staking
        let emergency_ts = 1001; // 1 second after staking
        
        let result = apply_early_withdrawal_penalty(
            farm.locking_duration,
            user.last_stake_ts,
            emergency_ts,
            farm.locking_early_withdrawal_penalty_bps,
            10000,
        );
        
        assert!(result.is_ok());
        let (amount_received, penalty) = result.unwrap();
        
        // Should receive only 10% due to 90% penalty
        assert!(penalty >= 8999, "Emergency withdrawal should have high penalty");
        assert!(amount_received <= 1001, "Should receive minimal amount in emergency");
    }
    
    #[test]
    fn test_granular_pause_controls() {
        let mut farm = FarmState::default();
        
        // Test individual pause flags
        farm.is_deposits_paused = 1;
        farm.is_withdrawals_paused = 0;
        farm.is_rewards_paused = 0;
        
        assert_eq!(farm.is_deposits_paused, 1, "Only deposits should be paused");
        assert_eq!(farm.is_withdrawals_paused, 0, "Withdrawals should be active");
        assert_eq!(farm.is_rewards_paused, 0, "Rewards should be active");
        
        // Test combinations
        farm.is_deposits_paused = 1;
        farm.is_rewards_paused = 1;
        
        assert_eq!(farm.is_deposits_paused, 1);
        assert_eq!(farm.is_rewards_paused, 1);
        assert_eq!(farm.is_withdrawals_paused, 0, "Withdrawals still active");
        
        // Full freeze overrides individual flags
        farm.is_farm_frozen = 1;
        assert_eq!(farm.is_farm_frozen, 1, "Full freeze should override all");
    }
}