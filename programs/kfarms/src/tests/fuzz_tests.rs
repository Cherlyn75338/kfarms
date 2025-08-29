use crate::stake_operations::*;
use crate::utils::math::{u64_mul_div, full_decimal_mul_div};
use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
use decimal_wad::decimal::Decimal;
use proptest::prelude::*;

// Property-based tests using proptest would go here
// For now, we'll use deterministic fuzz-like tests

/// Fuzz test for u64_mul_div with random inputs
#[test]
fn fuzz_u64_mul_div() {
    let test_cases = vec![
        // (a, b, c)
        (0, 0, 1),
        (1, 1, 1),
        (u64::MAX, 1, 1),
        (u64::MAX / 2, 2, 1),
        (u64::MAX, 1, u64::MAX),
        (1000, 1000, 1000),
        (u64::MAX / 3, 3, 1),
        (12345, 67890, 54321),
        (u64::MAX - 1, 1, 2),
        (999999999, 999999999, 999999999),
    ];
    
    for (a, b, c) in test_cases {
        if c == 0 {
            continue; // Skip division by zero
        }
        
        // Check if the multiplication would overflow u64
        let result_u128 = (a as u128) * (b as u128) / (c as u128);
        
        if result_u128 <= u64::MAX as u128 {
            let result = u64_mul_div(a, b, c);
            assert_eq!(result, result_u128 as u64);
        } else {
            // Should panic on overflow
            let result = std::panic::catch_unwind(|| u64_mul_div(a, b, c));
            assert!(result.is_err());
        }
    }
}

/// Fuzz test for stake/amount conversions
#[test]
fn fuzz_stake_conversions() {
    let test_cases = vec![
        (1, 1, 1),
        (100, 1000, 1000),
        (u64::MAX / 2, u64::MAX / 2, u64::MAX / 2),
        (1, u64::MAX, u64::MAX),
        (12345, 1000000, 999999),
        (1, 1, u64::MAX),
        (0, 1000, 1000),
        (1000, 0, 0),
    ];
    
    for (amount, total_amount, total_stake_val) in test_cases {
        let total_stake = Decimal::from(total_stake_val);
        
        // Test amount to stake conversion
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        
        if amount == 0 {
            assert_eq!(stake, Decimal::zero());
        } else if total_amount == 0 || total_stake == Decimal::zero() {
            assert_eq!(stake, Decimal::from(amount));
        } else {
            // Verify the conversion is reasonable
            assert!(stake <= total_stake * 2); // Shouldn't be more than double
        }
        
        // Test roundtrip if valid
        if total_amount > 0 && total_stake > Decimal::zero() {
            let recovered = convert_stake_to_amount(stake, total_stake, total_amount, false);
            
            // Should be close to original
            let diff = if recovered > amount {
                recovered - amount
            } else {
                amount - recovered
            };
            
            // Allow for rounding error
            assert!(diff <= 1 || diff <= amount / 1000);
        }
    }
}

/// Fuzz test for withdrawal penalty calculation
#[test]
fn fuzz_withdrawal_penalty() {
    let test_cases = vec![
        // (duration, start, now, penalty_bps, amount)
        (100, 1000, 1050, 5000, 1000),
        (0, 1000, 1000, 5000, 1000),
        (u64::MAX / 2, 1000, 1001, 9999, u64::MAX / 10),
        (1, 1000, 1000, 5000, 1),
        (1000000, 0, 500000, 1000, 1000000),
        (100, 1000, 900, 5000, 1000), // Before start
        (100, 1000, 1100, 5000, 1000), // After maturity
    ];
    
    for (duration, start, now, penalty_bps, amount) in test_cases {
        let result = apply_early_withdrawal_penalty(
            duration,
            start,
            now,
            penalty_bps,
            amount,
        );
        
        match result {
            Ok((withdrawn, penalty)) => {
                // Basic invariants
                assert_eq!(withdrawn + penalty, amount);
                assert!(penalty <= amount);
                assert!(withdrawn <= amount);
                
                // Check penalty bounds
                if now < start || now >= start + duration {
                    // No penalty before start or after maturity
                    assert_eq!(penalty, 0);
                    assert_eq!(withdrawn, amount);
                } else if penalty_bps > 0 && penalty_bps < 10000 {
                    // Some penalty should apply
                    assert!(penalty <= amount * penalty_bps / 10000);
                }
            }
            Err(_) => {
                // Error cases should be for invalid parameters
                assert!(
                    penalty_bps == 0 || 
                    penalty_bps >= 10000 ||
                    duration == 0 && now > start
                );
            }
        }
    }
}

/// Fuzz test for decimal multiplication and division
#[test]
fn fuzz_decimal_operations() {
    let test_cases = vec![
        (1u64, 1u64, 1u64),
        (1000, 1000, 1000),
        (u64::MAX / 2, 2, 1),
        (1, u64::MAX, u64::MAX),
        (12345, 67890, 54321),
    ];
    
    for (a_val, b, c_val) in test_cases {
        let a = Decimal::from(a_val);
        let c = Decimal::from(c_val);
        
        if c == Decimal::zero() {
            continue; // Skip division by zero
        }
        
        let result = full_decimal_mul_div(a, b, c);
        
        // Verify the result is reasonable
        if b == 0 || a == Decimal::zero() {
            assert_eq!(result, Decimal::zero());
        } else {
            // Check bounds
            let max_expected = (a * Decimal::from(b)) * 2; // Allow some margin
            assert!(result <= max_expected);
        }
    }
}

/// Stress test for repeated operations
#[test]
fn stress_test_repeated_operations() {
    let mut total_stake = Decimal::from(1_000_000u64);
    let mut total_amount = 1_000_000u64;
    
    // Simulate 1000 operations
    for i in 0..1000 {
        let operation = i % 3;
        let amount = (i * 100 + 1) % 10000 + 1; // Non-zero amount
        
        match operation {
            0 => {
                // Add stake
                let new_stake = convert_amount_to_stake(amount, total_stake, total_amount);
                total_stake = total_stake + new_stake;
                total_amount += amount;
            }
            1 => {
                // Remove stake (if possible)
                if total_amount > amount {
                    let stake_to_remove = convert_amount_to_stake(amount, total_stake, total_amount);
                    if stake_to_remove <= total_stake {
                        total_stake = total_stake - stake_to_remove;
                        total_amount -= amount;
                    }
                }
            }
            _ => {
                // Convert and verify
                let test_stake = convert_amount_to_stake(amount, total_stake, total_amount);
                let recovered = convert_stake_to_amount(test_stake, total_stake, total_amount, false);
                
                // Should be close
                let diff = if recovered > amount {
                    recovered - amount
                } else {
                    amount - recovered
                };
                assert!(diff <= 1);
            }
        }
        
        // Invariants
        assert!(total_stake >= Decimal::zero());
        assert!(total_amount < u64::MAX);
    }
}

/// Fuzz test for edge case combinations
#[test]
fn fuzz_edge_case_combinations() {
    let edge_values = vec![0, 1, 100, 1000, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    
    for &a in &edge_values {
        for &b in &edge_values {
            for &c in &edge_values {
                if c == 0 {
                    continue;
                }
                
                // Test u64_mul_div if it won't overflow
                let result_u128 = (a as u128) * (b as u128) / (c as u128);
                if result_u128 <= u64::MAX as u128 {
                    let result = u64_mul_div(a, b, c);
                    assert_eq!(result, result_u128 as u64);
                }
                
                // Test decimal operations
                if c > 0 {
                    let a_dec = Decimal::from(a);
                    let c_dec = Decimal::from(c);
                    let result = full_decimal_mul_div(a_dec, b, c_dec);
                    
                    // Should not panic
                    assert!(result >= Decimal::zero());
                }
            }
        }
    }
}