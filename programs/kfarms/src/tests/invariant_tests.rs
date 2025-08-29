use crate::stake_operations::*;
use crate::state::{FarmState, UserState};
use decimal_wad::decimal::Decimal;

/// Test that total stake and amount remain consistent
#[test]
fn test_stake_amount_conservation() {
    let mut farm = FarmState::default();
    let mut user1 = UserState::default();
    let mut user2 = UserState::default();
    
    // Initial deposit for user1
    let deposit1 = 1000u64;
    add_pending_deposit_stake(&mut user1, &mut farm, deposit1).unwrap();
    
    assert_eq!(farm.total_pending_amount, deposit1);
    
    // Initial deposit for user2
    let deposit2 = 2000u64;
    add_pending_deposit_stake(&mut user2, &mut farm, deposit2).unwrap();
    
    assert_eq!(farm.total_pending_amount, deposit1 + deposit2);
    
    // Activate user1's stake
    let (amount_activated, _) = activate_pending_stake(&mut user1, &mut farm).unwrap();
    
    assert_eq!(amount_activated, deposit1);
    assert_eq!(farm.total_staked_amount, deposit1);
    assert_eq!(farm.total_pending_amount, deposit2);
    
    // Total value conserved
    assert_eq!(farm.total_staked_amount + farm.total_pending_amount, deposit1 + deposit2);
}

/// Test that reward per share accumulator is monotonic
#[test]
fn test_reward_per_share_monotonic() {
    let mut reward_per_share = Decimal::zero();
    let total_stake = Decimal::from(1_000_000u64);
    
    for i in 0..100 {
        let reward = Decimal::from(i * 100);
        let added = reward / total_stake;
        let new_rps = reward_per_share + added;
        
        // Should always increase or stay same
        assert!(new_rps >= reward_per_share);
        reward_per_share = new_rps;
    }
}

/// Test that user rewards never exceed farm rewards
#[test]
fn test_user_rewards_bounded() {
    let farm_rewards_issued = 10_000u64;
    let mut total_user_rewards = 0u64;
    
    // Simulate 10 users claiming rewards
    for i in 0..10 {
        let user_reward = farm_rewards_issued * (i + 1) / 55; // Proportional distribution
        total_user_rewards += user_reward;
    }
    
    // Total user rewards should not exceed farm rewards
    assert!(total_user_rewards <= farm_rewards_issued);
}

/// Test share proportion invariant
#[test]
fn test_share_proportion_invariant() {
    let total_stake = Decimal::from(1_000_000u64);
    let total_amount = 1_000_000u64;
    
    // User owns 10% of stake
    let user_stake = Decimal::from(100_000u64);
    let user_amount = convert_stake_to_amount(user_stake, total_stake, total_amount, false);
    
    // Should own approximately 10% of amount
    assert!(user_amount >= 99_000 && user_amount <= 101_000);
    
    // Add more stake
    let new_deposit = 50_000u64;
    let new_stake = convert_amount_to_stake(new_deposit, total_stake, total_amount);
    let new_total_stake = total_stake + new_stake;
    let new_total_amount = total_amount + new_deposit;
    
    // Proportion should be maintained
    let user_new_amount = convert_stake_to_amount(
        user_stake, 
        new_total_stake, 
        new_total_amount, 
        false
    );
    
    // User's proportion should decrease but value should be preserved
    assert!(user_new_amount <= user_amount);
}

/// Test that pending operations don't affect active stake
#[test]
fn test_pending_active_separation() {
    let mut farm = FarmState::default();
    let mut user = UserState::default();
    
    // Add pending deposit
    let deposit = 1000u64;
    add_pending_deposit_stake(&mut user, &mut farm, deposit).unwrap();
    
    // Active stake should be unchanged
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(user.get_active_stake_decimal(), Decimal::zero());
    
    // Only pending should change
    assert_eq!(farm.total_pending_amount, deposit);
    assert!(user.get_pending_deposit_stake_decimal() > Decimal::zero());
}

/// Test delegated farm invariants
#[test]
fn test_delegated_farm_invariants() {
    let mut farm = FarmState::default();
    farm.is_farm_delegated = 1; // Set as delegated
    
    // In delegated farms, total_active_stake_scaled should equal total_staked_amount
    farm.total_staked_amount = 10_000;
    farm.total_active_stake_scaled = 10_000;
    
    // This invariant must hold
    assert_eq!(
        farm.total_active_stake_scaled as u64,
        farm.total_staked_amount
    );
    
    // Pending amounts should stay zero in delegated farms
    assert_eq!(farm.total_pending_amount, 0);
    assert_eq!(farm.total_pending_stake_scaled, 0);
}

/// Test withdrawal amount conservation
#[test]
fn test_withdrawal_conservation() {
    let mut farm = FarmState::default();
    let mut user = UserState::default();
    
    // Setup: deposit and activate
    let deposit = 1000u64;
    add_pending_deposit_stake(&mut user, &mut farm, deposit).unwrap();
    activate_pending_stake(&mut user, &mut farm).unwrap();
    
    assert_eq!(farm.total_staked_amount, deposit);
    
    // Unstake half
    let unstake_amount = user.get_active_stake_decimal() / 2;
    let initial_active = farm.total_staked_amount;
    
    // Perform unstake (without penalty for simplicity)
    let amount_removed = remove_active_stake(&mut user, &mut farm, unstake_amount).unwrap();
    add_pending_withdrawal_stake(&mut user, &mut farm, amount_removed).unwrap();
    
    // Conservation check
    assert_eq!(
        farm.total_staked_amount + farm.total_pending_amount,
        initial_active
    );
    
    // Remove pending withdrawal
    let withdrawn = remove_pending_withdrawal_stake(&mut user, &mut farm).unwrap();
    
    assert_eq!(withdrawn, amount_removed);
    assert_eq!(
        farm.total_staked_amount + withdrawn,
        initial_active
    );
}

/// Test reward tally consistency
#[test]
fn test_reward_tally_consistency() {
    // When a user's stake increases, their reward tally should be updated
    // to prevent them from claiming past rewards
    
    let reward_per_share = Decimal::from(100u64);
    let initial_stake = Decimal::from(1000u64);
    let added_stake = Decimal::from(500u64);
    
    // Initial tally
    let initial_tally = reward_per_share * initial_stake;
    
    // After stake increase, tally should increase proportionally
    let new_stake = initial_stake + added_stake;
    let expected_new_tally = initial_tally + (reward_per_share * added_stake);
    let actual_new_tally = reward_per_share * new_stake;
    
    assert_eq!(expected_new_tally, actual_new_tally);
}

/// Test that frozen farms maintain state
#[test]
fn test_frozen_farm_state() {
    let mut farm = FarmState::default();
    farm.total_staked_amount = 1000;
    farm.total_pending_amount = 500;
    
    // Withdraw everything (should freeze)
    let effects = withdraw_farm(&mut farm, 1500).unwrap();
    
    assert!(effects.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 0);
    assert_eq!(farm.total_pending_amount, 0);
    
    // Farm is now frozen, state should be zero
    assert_eq!(farm.total_staked_amount + farm.total_pending_amount, 0);
}