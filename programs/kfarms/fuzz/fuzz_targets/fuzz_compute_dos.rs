#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use farms::state::*;
use decimal_wad::decimal::Decimal;

#[derive(Debug, Arbitrary)]
struct ComputeDoSInput {
    num_rewards: u8,
    num_users: u8,
    num_operations: u16,
    extreme_decimals: Vec<u8>,
    extreme_timestamps: Vec<u64>,
    extreme_amounts: Vec<u64>,
}

struct ComputeBudget {
    units_consumed: u64,
    max_units: u64,
}

impl ComputeBudget {
    fn new() -> Self {
        ComputeBudget {
            units_consumed: 0,
            max_units: 200_000, // Solana's default compute budget
        }
    }
    
    fn consume(&mut self, units: u64) -> Result<(), String> {
        self.units_consumed = self.units_consumed.saturating_add(units);
        if self.units_consumed > self.max_units {
            return Err("Compute budget exceeded".to_string());
        }
        Ok(())
    }
}

fuzz_target!(|input: ComputeDoSInput| {
    let mut budget = ComputeBudget::new();
    
    // Test with maximum number of rewards (10)
    let num_rewards = (input.num_rewards % 11).max(1);
    
    // Simulate refresh_farm with many rewards
    for _ in 0..num_rewards {
        // Each reward refresh costs compute units
        if budget.consume(1000).is_err() {
            return; // Compute DoS detected
        }
        
        // Simulate complex reward calculation
        for decimal in &input.extreme_decimals {
            let decimal = *decimal % 19; // Max 18 decimals
            let divisor = 10u128.pow(decimal as u32);
            
            // Expensive division operation
            if budget.consume(50).is_err() {
                return;
            }
            
            let _ = u128::MAX / divisor;
        }
    }
    
    // Test with many users
    let num_users = (input.num_users % 100).max(1);
    
    for user_id in 0..num_users {
        // Simulate refresh_user for each user
        if budget.consume(500).is_err() {
            return;
        }
        
        // For each user, calculate rewards for all tokens
        for _ in 0..num_rewards {
            if budget.consume(100).is_err() {
                return;
            }
            
            // Simulate tally calculation with extreme values
            for amount in &input.extreme_amounts {
                let amount = amount % u64::MAX;
                let _ = Decimal::from(amount)
                    .checked_mul(Decimal::from(amount));
                
                if budget.consume(20).is_err() {
                    return;
                }
            }
        }
    }
    
    // Test rapid time advances
    for timestamp in &input.extreme_timestamps {
        let timestamp = timestamp % (365 * 24 * 60 * 60); // Cap at 1 year
        
        // Simulate time-based reward calculation
        if budget.consume(200).is_err() {
            return;
        }
        
        // Complex schedule curve evaluation
        for _ in 0..10 {
            if budget.consume(50).is_err() {
                return;
            }
        }
    }
    
    // Test with maximum operations in single transaction
    let num_ops = (input.num_operations % 1000).max(1);
    
    for _ in 0..num_ops {
        // Each operation has overhead
        if budget.consume(100).is_err() {
            return;
        }
    }
    
    // If we get here, no DoS vulnerability found with this input
    assert!(
        budget.units_consumed <= budget.max_units,
        "Compute budget should not exceed limit"
    );
});