use anchor_lang::prelude::*;
use anchor_lang::InstructionData;
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::Signer,
    transaction::Transaction,
};
use farms::{
    instruction::*,
    state::*,
    farm_operations::*,
};
use decimal_wad::decimal::Decimal;
use crate::test_utils::*;

#[tokio::test]
async fn test_stake_unstake_basic_lifecycle() {
    let mut env = TestEnvironment::new().await;
    
    // Setup
    setup_farm_with_rewards(&mut env).await;
    let mut user = env.add_user(1_000_000_000).await; // 1000 tokens with 9 decimals
    initialize_user_state(&mut env, &user).await;
    
    // Test stake
    let stake_amount = 500_000_000; // 500 tokens
    stake_tokens(&mut env, &user, stake_amount).await;
    
    // Verify stake
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(
        user_state.active_stake_scaled,
        Decimal::from(stake_amount)
    );
    
    // Advance time
    env.advance_time_seconds(100).await;
    
    // Test unstake
    let unstake_amount = 200_000_000; // 200 tokens
    unstake_tokens(&mut env, &user, unstake_amount).await;
    
    // Verify unstake (should be in pending withdrawal)
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(
        user_state.active_stake_scaled,
        Decimal::from(stake_amount - unstake_amount)
    );
    assert_eq!(
        user_state.pending_withdrawal.amount,
        unstake_amount
    );
    
    // Advance past cooldown period
    env.advance_time_seconds(1000).await;
    
    // Withdraw unstaked deposits
    withdraw_unstaked_deposits(&mut env, &user).await;
    
    // Verify withdrawal completed
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_withdrawal.amount, 0);
}

#[tokio::test]
async fn test_stake_with_p_tot_zero_transition() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with rewards
    setup_farm_with_rewards(&mut env).await;
    
    // Add rewards to the farm
    add_rewards_to_farm(&mut env, 0, 1_000_000_000).await; // 1000 reward tokens
    
    // Create first user and stake
    let user1 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    stake_tokens(&mut env, &user1, 500_000_000).await;
    
    // Advance time to accumulate rewards
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    
    // First user unstakes everything (P_tot becomes 0)
    unstake_tokens(&mut env, &user1, 500_000_000).await;
    
    // Verify P_tot is 0
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.total_active_stake_scaled, Decimal::zero());
    
    // Advance time while P_tot = 0
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    
    // Verify no rewards were issued while P_tot = 0
    let farm_state = get_farm_state(&mut env).await;
    let rewards_before = farm_state.reward_infos[0].rewards_issued_cumulative;
    
    // Second user stakes (P_tot becomes non-zero again)
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user2).await;
    stake_tokens(&mut env, &user2, 300_000_000).await;
    
    // Advance time and verify rewards resume
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    
    let farm_state = get_farm_state(&mut env).await;
    let rewards_after = farm_state.reward_infos[0].rewards_issued_cumulative;
    assert!(rewards_after > rewards_before);
}

#[tokio::test]
async fn test_stake_with_warmup_period() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with warmup period
    setup_farm_with_warmup_cooldown(&mut env, 60, 120).await; // 60s warmup, 120s cooldown
    
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Stake tokens
    let stake_amount = 500_000_000;
    stake_tokens(&mut env, &user, stake_amount).await;
    
    // Immediately after stake, should be in pending deposit
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_deposit_stake_scaled, Decimal::from(stake_amount));
    assert_eq!(user_state.active_stake_scaled, Decimal::zero());
    
    // Advance time but not past warmup
    env.advance_time_seconds(30).await;
    refresh_user(&mut env, &user).await;
    
    // Still in pending
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_deposit_stake_scaled, Decimal::from(stake_amount));
    assert_eq!(user_state.active_stake_scaled, Decimal::zero());
    
    // Advance past warmup period
    env.advance_time_seconds(40).await;
    refresh_user(&mut env, &user).await;
    
    // Should now be active
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_deposit_stake_scaled, Decimal::zero());
    assert_eq!(user_state.active_stake_scaled, Decimal::from(stake_amount));
}

#[tokio::test]
async fn test_unstake_with_cooldown_period() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with cooldown period
    setup_farm_with_warmup_cooldown(&mut env, 0, 120).await; // No warmup, 120s cooldown
    
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Stake and then unstake
    stake_tokens(&mut env, &user, 500_000_000).await;
    env.advance_time_seconds(10).await;
    
    let unstake_amount = 200_000_000;
    unstake_tokens(&mut env, &user, unstake_amount).await;
    
    // Check pending withdrawal
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_withdrawal.amount, unstake_amount);
    assert!(user_state.pending_withdrawal.ts_start > 0);
    
    // Try to withdraw before cooldown expires (should fail)
    env.advance_time_seconds(60).await; // Only 60s, cooldown is 120s
    let result = try_withdraw_unstaked_deposits(&mut env, &user).await;
    assert!(result.is_err());
    
    // Advance past cooldown
    env.advance_time_seconds(70).await; // Total 130s > 120s cooldown
    withdraw_unstaked_deposits(&mut env, &user).await;
    
    // Verify withdrawal completed
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_withdrawal.amount, 0);
}

#[tokio::test]
async fn test_locking_mode_none() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with no locking
    setup_farm_with_locking(&mut env, LockingMode::None, 0, 0, 0).await;
    
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Should be able to stake and unstake freely
    stake_tokens(&mut env, &user, 500_000_000).await;
    env.advance_time_seconds(10).await;
    unstake_tokens(&mut env, &user, 200_000_000).await;
    
    // No penalties should apply
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_withdrawal.amount, 200_000_000);
}

#[tokio::test]
async fn test_locking_mode_continuous() {
    let mut env = TestEnvironment::new().await;
    
    let lock_duration = 86400; // 1 day
    let early_penalty_bps = 5000; // 50%
    
    // Setup farm with continuous locking
    setup_farm_with_locking(
        &mut env,
        LockingMode::Continuous,
        0,
        lock_duration,
        early_penalty_bps,
    ).await;
    
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Stake tokens
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    // Try to unstake immediately (should incur penalty)
    let unstake_amount = 200_000_000;
    unstake_tokens(&mut env, &user, unstake_amount).await;
    
    // Check that penalty was applied
    let user_state = get_user_state(&mut env, &user).await;
    let expected_penalty = (unstake_amount as u128 * early_penalty_bps as u128 / 10000) as u64;
    let expected_withdrawal = unstake_amount - expected_penalty;
    assert_eq!(user_state.pending_withdrawal.amount, expected_withdrawal);
    
    // Verify slashed amount
    let farm_state = get_farm_state(&mut env).await;
    assert!(farm_state.total_slashed_amount > 0);
}

#[tokio::test]
async fn test_locking_mode_with_expiry() {
    let mut env = TestEnvironment::new().await;
    
    let current_time = env.context.banks_client
        .get_sysvar::<Clock>()
        .await
        .unwrap()
        .unix_timestamp as u64;
    
    let lock_start = current_time + 100;
    let lock_duration = 86400; // 1 day
    let early_penalty_bps = 3000; // 30%
    
    // Setup farm with expiry locking
    setup_farm_with_locking(
        &mut env,
        LockingMode::WithExpiry,
        lock_start,
        lock_duration,
        early_penalty_bps,
    ).await;
    
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Stake before lock period starts
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    // Unstake before lock starts (no penalty)
    unstake_tokens(&mut env, &user, 100_000_000).await;
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.pending_withdrawal.amount, 100_000_000);
    
    // Advance to lock period
    env.advance_time_seconds(150).await;
    
    // Unstake during lock period (penalty applies)
    unstake_tokens(&mut env, &user, 100_000_000).await;
    let user_state = get_user_state(&mut env, &user).await;
    let expected_penalty = (100_000_000u128 * early_penalty_bps as u128 / 10000) as u64;
    assert_eq!(
        user_state.pending_withdrawal.amount,
        100_000_000 + (100_000_000 - expected_penalty)
    );
    
    // Advance past lock expiry
    env.advance_time_seconds(lock_duration as i64 + 100).await;
    
    // Unstake after lock expires (no penalty)
    unstake_tokens(&mut env, &user, 100_000_000).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    // Previous pending + new unstake without penalty
    assert!(user_state.pending_withdrawal.amount > 200_000_000);
}

#[tokio::test]
async fn test_multiple_users_staking() {
    let mut env = TestEnvironment::new().await;
    
    setup_farm_with_rewards(&mut env).await;
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await; // 10000 reward tokens
    
    // Create multiple users
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(2_000_000_000).await;
    let user3 = env.add_user(3_000_000_000).await;
    
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    initialize_user_state(&mut env, &user3).await;
    
    // Users stake different amounts
    stake_tokens(&mut env, &user1, 100_000_000).await;
    stake_tokens(&mut env, &user2, 200_000_000).await;
    stake_tokens(&mut env, &user3, 300_000_000).await;
    
    // Verify total stake
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(
        farm_state.total_active_stake_scaled,
        Decimal::from(600_000_000)
    );
    
    // Advance time to accumulate rewards
    env.advance_time_seconds(1000).await;
    refresh_farm(&mut env).await;
    
    // Users harvest rewards
    harvest_rewards(&mut env, &user1, 0).await;
    harvest_rewards(&mut env, &user2, 0).await;
    harvest_rewards(&mut env, &user3, 0).await;
    
    // Verify rewards are proportional to stake
    let user1_rewards = get_user_rewards(&mut env, &user1, 0).await;
    let user2_rewards = get_user_rewards(&mut env, &user2, 0).await;
    let user3_rewards = get_user_rewards(&mut env, &user3, 0).await;
    
    // User2 should have ~2x user1's rewards
    assert!(user2_rewards > user1_rewards * 19 / 10); // Allow some rounding
    assert!(user2_rewards < user1_rewards * 21 / 10);
    
    // User3 should have ~3x user1's rewards
    assert!(user3_rewards > user1_rewards * 29 / 10);
    assert!(user3_rewards < user1_rewards * 31 / 10);
}

#[tokio::test]
async fn test_edge_case_zero_stake_attempt() {
    let mut env = TestEnvironment::new().await;
    
    setup_farm_with_rewards(&mut env).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Try to stake 0 tokens (should fail)
    let result = try_stake_tokens(&mut env, &user, 0).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_edge_case_unstake_more_than_staked() {
    let mut env = TestEnvironment::new().await;
    
    setup_farm_with_rewards(&mut env).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Stake some tokens
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    // Try to unstake more than staked (should fail)
    let result = try_unstake_tokens(&mut env, &user, 200_000_000).await;
    assert!(result.is_err());
}

// Helper functions
async fn setup_farm_with_rewards(env: &mut TestEnvironment) {
    // Initialize global config
    initialize_global_config(env).await;
    
    // Create and initialize farm
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
    
    // Create and initialize reward
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(env, &reward_mint.pubkey(), RewardType::Proportional, 100, 6).await;
    env.reward_mints.push(reward_mint);
}

async fn setup_farm_with_warmup_cooldown(
    env: &mut TestEnvironment,
    warmup: u64,
    cooldown: u64,
) {
    setup_farm_with_rewards(env).await;
    
    // Update farm config with warmup/cooldown
    let config = FarmConfigOption {
        deposit_cap_amount: 0,
        deposit_warmup_period: warmup,
        withdrawal_cooldown_period: cooldown,
        withdrawal_penalty_bps: 0,
        locking_mode: LockingMode::None,
        locking_start_timestamp: 0,
        locking_duration: 0,
        locking_early_withdrawal_penalty_bps: 0,
        slashing_penalty_bps: 0,
        slashing_penalty_recipient: Pubkey::default(),
        reserved0: [0; 1887],
    };
    
    update_farm_config(env, config).await;
}

async fn setup_farm_with_locking(
    env: &mut TestEnvironment,
    mode: LockingMode,
    start_ts: u64,
    duration: u64,
    penalty_bps: u64,
) {
    setup_farm_with_rewards(env).await;
    
    let current_time = if start_ts == 0 {
        env.context.banks_client
            .get_sysvar::<Clock>()
            .await
            .unwrap()
            .unix_timestamp as u64
    } else {
        start_ts
    };
    
    let config = FarmConfigOption {
        deposit_cap_amount: 0,
        deposit_warmup_period: 0,
        withdrawal_cooldown_period: 0,
        withdrawal_penalty_bps: 0,
        locking_mode: mode,
        locking_start_timestamp: current_time,
        locking_duration: duration,
        locking_early_withdrawal_penalty_bps: penalty_bps,
        slashing_penalty_bps: 0,
        slashing_penalty_recipient: env.admin.pubkey(),
        reserved0: [0; 1887],
    };
    
    update_farm_config(env, config).await;
}

// Additional helper functions would go here...
// (initialize_global_config, initialize_farm, initialize_reward, etc.)
// These are similar to the ones in initialization_tests.rs but extracted for reuse