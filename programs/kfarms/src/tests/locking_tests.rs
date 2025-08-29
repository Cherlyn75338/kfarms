#[cfg(test)]
mod tests {
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    use crate::utils::consts::BPS_DIV_FACTOR;
    use crate::FarmError;

    #[test]
    fn test_no_penalty_after_maturity() {
        let locking_duration = 1000;
        let locking_start = 1000;
        let timestamp_now = 2001; // After maturity
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;

        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        ).unwrap();

        assert_eq!(amount, 1000);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn test_penalty_before_lock_start() {
        // CRITICAL VULNERABILITY TEST
        let locking_duration = 1000;
        let locking_start = 2000; // Future start
        let timestamp_now = 1500; // Before start
        let penalty_bps = 5000; // 50%
        let unstake_amount = 1000;

        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            unstake_amount,
        ).unwrap();

        // Vulnerability: Returns 0 penalty when withdrawing before lock start
        assert_eq!(penalty, 0);
        assert_eq!(amount, 1000);
    }

    #[test]
    fn test_linear_penalty_calculation() {
        let locking_duration = 1000;
        let locking_start = 1000;
        let penalty_bps = 5000; // 50% max penalty
        let unstake_amount = 1000;

        // Test at different points in time
        let test_cases = vec![
            (1000, 500), // At start: 50% penalty
            (1250, 375), // 25% through: 37.5% penalty
            (1500, 250), // 50% through: 25% penalty
            (1750, 125), // 75% through: 12.5% penalty
            (1999, 0),   // Almost at end: ~0% penalty
        ];

        for (timestamp, expected_penalty) in test_cases {
            let (amount, penalty) = apply_early_withdrawal_penalty(
                locking_duration,
                locking_start,
                timestamp,
                penalty_bps,
                unstake_amount,
            ).unwrap();

            // Allow small rounding differences
            assert!(
                (penalty as i64 - expected_penalty as i64).abs() <= 1,
                "At timestamp {}: expected penalty ~{}, got {}",
                timestamp,
                expected_penalty,
                penalty
            );
            assert_eq!(amount + penalty, unstake_amount);
        }
    }

    #[test]
    fn test_invalid_penalty_percentage() {
        let result = apply_early_withdrawal_penalty(
            1000,
            1000,
            1500,
            10001, // > 100%
            1000,
        );

        assert!(matches!(result, Err(e) if e.to_string().contains("InvalidPenaltyPercentage")));
    }

    #[test]
    fn test_early_withdrawal_not_allowed() {
        // Test 0% penalty (no early withdrawal)
        let result = apply_early_withdrawal_penalty(
            1000,
            1000,
            1500,
            0,
            1000,
        );
        assert!(matches!(result, Err(e) if e.to_string().contains("EarlyWithdrawalNotAllowed")));

        // Test 100% penalty (no early withdrawal)
        let result = apply_early_withdrawal_penalty(
            1000,
            1000,
            1500,
            10000,
            1000,
        );
        assert!(matches!(result, Err(e) if e.to_string().contains("EarlyWithdrawalNotAllowed")));
    }

    #[test]
    fn test_invalid_timestamps() {
        // Maturity before start
        let result = apply_early_withdrawal_penalty(
            0, // Duration 0 means maturity = start
            2000,
            1500,
            5000,
            1000,
        );
        
        // This might not error but should handle gracefully
        match result {
            Ok((amount, penalty)) => {
                // If it succeeds, penalty should be 0 (before start)
                assert_eq!(penalty, 0);
                assert_eq!(amount, 1000);
            }
            Err(_) => {
                // Or it should error appropriately
            }
        }
    }

    #[test]
    fn test_penalty_precision() {
        // Test precision with small amounts
        let locking_duration = 1000;
        let locking_start = 1000;
        let timestamp_now = 1500; // Halfway
        let penalty_bps = 5000; // 50% max
        
        // Test with 1 wei
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            1,
        ).unwrap();
        
        assert_eq!(penalty, 0); // Should round down to 0
        assert_eq!(amount, 1);
        
        // Test with amount that should round
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            3, // 25% of 3 = 0.75, should round down to 0
        ).unwrap();
        
        assert_eq!(penalty, 0);
        assert_eq!(amount, 3);
        
        // Test with larger amount
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            timestamp_now,
            penalty_bps,
            10000, // 25% of 10000 = 2500
        ).unwrap();
        
        assert_eq!(penalty, 2500);
        assert_eq!(amount, 7500);
    }

    #[test]
    fn test_penalty_edge_timestamps() {
        let locking_duration = 1000;
        let locking_start = 1000;
        let penalty_bps = 5000;
        let unstake_amount = 1000;

        // Exactly at start
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            1000,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        assert_eq!(penalty, 500); // Full penalty
        assert_eq!(amount, 500);

        // Exactly at maturity
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            2000,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        assert_eq!(penalty, 0);
        assert_eq!(amount, 1000);

        // One second before maturity
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            1999,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        assert_eq!(penalty, 0); // Should be minimal
        assert_eq!(amount, 1000);
    }

    #[test]
    fn test_continuous_vs_with_expiry_modes() {
        // This test demonstrates the difference between Continuous and WithExpiry modes
        // In real usage:
        // - WithExpiry uses farm's locking_start_timestamp
        // - Continuous uses user's last_stake_ts
        
        let locking_duration = 1000;
        let penalty_bps = 5000;
        let unstake_amount = 1000;
        
        // Scenario 1: User stakes at time 500, farm lock starts at 1000
        let user_stake_time = 500;
        let farm_lock_start = 1000;
        let withdraw_time = 1250; // 25% into farm lock period
        
        // WithExpiry mode (uses farm start)
        let (expiry_amount, expiry_penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            farm_lock_start,
            withdraw_time,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        
        // Continuous mode (uses user stake time)
        let (continuous_amount, continuous_penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            user_stake_time,
            withdraw_time,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        
        // WithExpiry: 25% through lock = 37.5% penalty
        assert_eq!(expiry_penalty, 375);
        
        // Continuous: 75% through lock = 12.5% penalty
        assert_eq!(continuous_penalty, 125);
        
        // Continuous mode is more favorable here
        assert!(continuous_amount > expiry_amount);
    }

    #[test]
    fn test_penalty_attack_vector() {
        // Test potential attack: stake right before WithExpiry lock starts
        let locking_duration = 1000;
        let locking_start = 2000;
        let penalty_bps = 9000; // 90% penalty
        let unstake_amount = 1_000_000;
        
        // Attacker stakes at time 1999, withdraws at 1999 (before lock starts)
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            1999,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        
        // VULNERABILITY: No penalty applied
        assert_eq!(penalty, 0);
        assert_eq!(amount, 1_000_000);
        
        // If they wait 1 more second (lock starts)
        let (amount, penalty) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            2000,
            penalty_bps,
            unstake_amount,
        ).unwrap();
        
        // Now they face 90% penalty
        assert_eq!(penalty, 900_000);
        assert_eq!(amount, 100_000);
    }
}