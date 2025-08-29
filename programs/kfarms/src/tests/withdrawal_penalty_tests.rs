use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
use crate::FarmError;

#[test]
fn test_no_penalty_after_maturity() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1100; // After maturity
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
fn test_no_penalty_before_start() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 900; // Before start
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
fn test_proportional_penalty() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1050; // Halfway through
    let penalty_bps = 1000; // 10%
    let unstake_amount = 1000;
    
    let (amount, penalty) = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    ).unwrap();
    
    // Penalty should be 10% * 50/100 = 5% = 50
    assert_eq!(penalty, 50);
    assert_eq!(amount, 950);
}

#[test]
fn test_full_penalty_at_start() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1000; // Right at start
    let penalty_bps = 2000; // 20%
    let unstake_amount = 1000;
    
    let (amount, penalty) = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    ).unwrap();
    
    // Full penalty of 20%
    assert_eq!(penalty, 200);
    assert_eq!(amount, 800);
}

#[test]
fn test_zero_penalty_not_allowed() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1050;
    let penalty_bps = 0; // 0% penalty
    let unstake_amount = 1000;
    
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    assert!(matches!(result, Err(FarmError::EarlyWithdrawalNotAllowed)));
}

#[test]
fn test_hundred_percent_penalty_not_allowed() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1050;
    let penalty_bps = 10000; // 100% penalty
    let unstake_amount = 1000;
    
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    assert!(matches!(result, Err(FarmError::EarlyWithdrawalNotAllowed)));
}

#[test]
fn test_invalid_penalty_percentage() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1050;
    let penalty_bps = 10001; // > 100%
    let unstake_amount = 1000;
    
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    assert!(matches!(result, Err(FarmError::InvalidPenaltyPercentage)));
}

#[test]
fn test_zero_duration_edge_case() {
    let locking_duration = 0; // Zero duration - potential division by zero
    let locking_start = 1000;
    let timestamp_now = 1000;
    let penalty_bps = 5000;
    let unstake_amount = 1000;
    
    // This should either return no penalty or handle gracefully
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    // With zero duration, we're always at maturity
    assert!(result.is_ok());
    let (amount, penalty) = result.unwrap();
    assert_eq!(penalty, 0);
    assert_eq!(amount, 1000);
}

#[test]
fn test_overflow_protection_large_values() {
    // Test with large values that could overflow in penalty_bps * time_remaining
    let locking_duration = u64::MAX / 2;
    let locking_start = 1000;
    let timestamp_now = 1000 + locking_duration / 4; // 1/4 through
    let penalty_bps = 9999; // Just under 100%
    let unstake_amount = u64::MAX / 10;
    
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    // Should handle without panic
    assert!(result.is_ok());
    let (amount, penalty) = result.unwrap();
    assert!(penalty > 0);
    assert!(amount < unstake_amount);
    assert_eq!(amount + penalty, unstake_amount);
}

#[test]
fn test_precision_small_amounts() {
    let locking_duration = 100;
    let locking_start = 1000;
    let timestamp_now = 1001; // 1% through
    let penalty_bps = 100; // 1% penalty
    let unstake_amount = 10; // Small amount
    
    let (amount, penalty) = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    ).unwrap();
    
    // With 1% penalty and 1% time passed, penalty should be minimal
    // 1% * 99/100 ≈ 0.99% of 10 ≈ 0
    assert!(penalty <= 1);
    assert!(amount >= 9);
}

#[test]
fn test_invalid_timestamps() {
    let locking_duration = 100;
    let locking_start = 2000; // Start is after maturity (invalid)
    let timestamp_now = 1500;
    let penalty_bps = 5000;
    let unstake_amount = 1000;
    
    // Start + duration would overflow or be invalid
    let result = apply_early_withdrawal_penalty(
        locking_duration,
        locking_start,
        timestamp_now,
        penalty_bps,
        unstake_amount,
    );
    
    // Should return 0 penalty since now < start
    assert!(result.is_ok());
    let (amount, penalty) = result.unwrap();
    assert_eq!(penalty, 0);
    assert_eq!(amount, 1000);
}

#[test]
fn test_boundary_values() {
    // Test at exact boundaries
    let test_cases = vec![
        (100, 1000, 1099, 5000, 1000), // 1 unit before maturity
        (100, 1000, 1001, 9900, 1000), // 1 unit after start
        (1, 1000, 1000, 5000, 1000),   // Duration of 1
        (u64::MAX - 1000, 1000, 1001, 100, 1000), // Large duration
    ];
    
    for (duration, start, now, bps, amount) in test_cases {
        let result = apply_early_withdrawal_penalty(
            duration,
            start,
            now,
            bps,
            amount,
        );
        
        assert!(result.is_ok(), 
            "Failed for duration={}, start={}, now={}, bps={}, amount={}",
            duration, start, now, bps, amount
        );
        
        let (withdrawn, penalty) = result.unwrap();
        assert_eq!(withdrawn + penalty, amount);
        assert!(penalty <= amount);
    }
}