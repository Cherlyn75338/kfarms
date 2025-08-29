use crate::stake_operations::*;
use crate::state::{FarmState, UserState, LockingMode};
use decimal_wad::decimal::Decimal;

// Mock implementations for testing
impl UserStakeAccessor for UserState {
    fn get_accessor(&mut self) -> UserStakeAbstract<'_, Self> {
        UserStakeAbstract {
            internal: UserStake {
                active_stake: self.get_active_stake_decimal(),
                pending_deposit_stake: self.get_pending_deposit_stake_decimal(),
                pending_withdrawal_unstake: self.get_pending_withdrawal_unstake_decimal(),
                last_stake_ts: self.last_stake_ts,
            },
            src_ref: self,
        }
    }

    fn update(&mut self, abstract_val: UserStake) {
        self.set_active_stake_decimal(abstract_val.active_stake);
        self.set_pending_deposit_stake_decimal(abstract_val.pending_deposit_stake);
        self.set_pending_withdrawal_unstake_decimal(abstract_val.pending_withdrawal_unstake);
        self.last_stake_ts = abstract_val.last_stake_ts;
    }
}

#[test]
fn test_convert_stake_to_amount_basic() {
    let stake = Decimal::from(100u64);
    let total_stake = Decimal::from(1000u64);
    let total_amount = 5000u64;
    
    // 100/1000 * 5000 = 500
    let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
    assert_eq!(amount, 500);
    
    // Test rounding up
    let amount_up = convert_stake_to_amount(stake, total_stake, total_amount, true);
    assert_eq!(amount_up, 500);
}

#[test]
fn test_convert_stake_to_amount_zero_stake() {
    let stake = Decimal::zero();
    let total_stake = Decimal::from(1000u64);
    let total_amount = 5000u64;
    
    let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
    assert_eq!(amount, 0);
}

#[test]
fn test_convert_stake_to_amount_zero_total() {
    let stake = Decimal::from(100u64);
    let total_stake = Decimal::zero();
    let total_amount = 0u64;
    
    // When total is zero, should return total_amount (0)
    let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
    assert_eq!(amount, 0);
}

#[test]
fn test_convert_amount_to_stake_basic() {
    let amount = 500u64;
    let total_stake = Decimal::from(1000u64);
    let total_amount = 5000u64;
    
    // 500/5000 * 1000 = 100
    let stake = convert_amount_to_stake(amount, total_stake, total_amount);
    assert_eq!(stake, Decimal::from(100u64));
}

#[test]
fn test_convert_amount_to_stake_zero_amount() {
    let amount = 0u64;
    let total_stake = Decimal::from(1000u64);
    let total_amount = 5000u64;
    
    let stake = convert_amount_to_stake(amount, total_stake, total_amount);
    assert_eq!(stake, Decimal::zero());
}

#[test]
fn test_convert_amount_to_stake_zero_total() {
    let amount = 100u64;
    let total_stake = Decimal::zero();
    let total_amount = 0u64;
    
    // When totals are zero, return amount as Decimal
    let stake = convert_amount_to_stake(amount, total_stake, total_amount);
    assert_eq!(stake, Decimal::from(100u64));
}

#[test]
#[should_panic(expected = "Total amount is zero but total stake is not")]
fn test_convert_amount_to_stake_inconsistent_state() {
    let amount = 100u64;
    let total_stake = Decimal::from(1000u64); // Non-zero stake
    let total_amount = 0u64; // But zero amount
    
    convert_amount_to_stake(amount, total_stake, total_amount);
}

#[test]
fn test_stake_conversion_roundtrip() {
    let original_amount = 1234u64;
    let total_stake = Decimal::from(10000u64);
    let total_amount = 50000u64;
    
    // Convert amount to stake
    let stake = convert_amount_to_stake(original_amount, total_stake, total_amount);
    
    // Convert back to amount
    let recovered_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
    
    // Should be close to original (may have small rounding difference)
    assert!(recovered_amount <= original_amount);
    assert!(original_amount - recovered_amount <= 1);
}

#[test]
fn test_stake_conversion_precision() {
    // Test with very small amounts
    let amount = 1u64;
    let total_stake = Decimal::from(1_000_000_000u64);
    let total_amount = 1_000_000_000u64;
    
    let stake = convert_amount_to_stake(amount, total_stake, total_amount);
    assert!(stake > Decimal::zero());
    
    let recovered = convert_stake_to_amount(stake, total_stake, total_amount, false);
    assert_eq!(recovered, amount);
}

#[test]
fn test_stake_conversion_large_values() {
    // Test with maximum values
    let amount = u64::MAX / 2;
    let total_stake = Decimal::from(u64::MAX / 2);
    let total_amount = u64::MAX / 2;
    
    let stake = convert_amount_to_stake(amount, total_stake, total_amount);
    assert_eq!(stake, Decimal::from(amount));
    
    let recovered = convert_stake_to_amount(stake, total_stake, total_amount, false);
    assert_eq!(recovered, amount);
}

#[test]
fn test_rounding_behavior() {
    let total_stake = Decimal::from(1000u64);
    let total_amount = 3000u64;
    
    // Test case that would result in fractional amount
    let stake = Decimal::from(333u64); // Would be 999 in amount
    
    let amount_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
    let amount_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
    
    assert!(amount_ceil >= amount_floor);
    assert!(amount_ceil - amount_floor <= 1);
}

#[test]
fn test_withdraw_farm_proportional() {
    let mut farm = FarmState::default();
    farm.total_staked_amount = 1000;
    farm.total_pending_amount = 500;
    
    // Withdraw 300 from total of 1500
    let effects = withdraw_farm(&mut farm, 300).unwrap();
    
    assert_eq!(effects.amount_to_withdraw, 300);
    assert!(!effects.farm_to_freeze);
    
    // Should maintain proportions
    // Active: 1000 * (1 - 300/1500) = 800
    // Pending: 500 * (1 - 300/1500) = 400
    assert_eq!(farm.total_staked_amount, 800);
    assert_eq!(farm.total_pending_amount, 400);
}

#[test]
fn test_withdraw_farm_all() {
    let mut farm = FarmState::default();
    farm.total_staked_amount = 1000;
    farm.total_pending_amount = 500;
    
    // Withdraw everything
    let effects = withdraw_farm(&mut farm, 1500).unwrap();
    
    assert_eq!(effects.amount_to_withdraw, 1500);
    assert!(effects.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(farm.total_pending_amount, 0);
}

#[test]
fn test_withdraw_farm_more_than_available() {
    let mut farm = FarmState::default();
    farm.total_staked_amount = 1000;
    farm.total_pending_amount = 500;
    
    // Try to withdraw more than available
    let effects = withdraw_farm(&mut farm, 2000).unwrap();
    
    // Should withdraw all available
    assert_eq!(effects.amount_to_withdraw, 1500);
    assert!(effects.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(farm.total_pending_amount, 0);
}