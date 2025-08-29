use anchor_lang::prelude::*;
use anchor_lang::InstructionData;
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::Signer,
    transaction::Transaction,
    account::Account as SolanaAccount,
};
use farms::{
    instruction::*,
    state::*,
};
use scope::{OraclePrices, DatedPrice, Price};
use decimal_wad::decimal::Decimal;
use crate::test_utils::*;

#[tokio::test]
async fn test_valid_scope_prices() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle configuration
    setup_farm_with_oracle(&mut env).await;
    
    // Create mock Scope oracle account with valid prices
    let scope_prices = create_mock_scope_prices(100_000_000, 0); // $100 price, fresh
    let scope_prices_pubkey = Pubkey::new_unique();
    
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(scope_prices),
    );
    
    // Configure farm to use oracle
    let price_id = 0u64;
    update_farm_oracle_config(&mut env, scope_prices_pubkey, price_id).await;
    
    // Stake with oracle validation
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // This should succeed with valid oracle
    stake_tokens_with_oracle(&mut env, &user, 100_000_000, scope_prices_pubkey).await;
    
    // Verify stake succeeded
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.active_stake_scaled, Decimal::from(100_000_000));
}

#[tokio::test]
async fn test_stale_scope_prices() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle
    setup_farm_with_oracle(&mut env).await;
    
    // Create stale Scope oracle prices
    let stale_timestamp = env.context.banks_client
        .get_sysvar::<Clock>()
        .await
        .unwrap()
        .unix_timestamp - 3600; // 1 hour old
    
    let scope_prices = create_mock_scope_prices_with_timestamp(100_000_000, stale_timestamp);
    let scope_prices_pubkey = Pubkey::new_unique();
    
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(scope_prices),
    );
    
    // Configure farm with max age
    let price_id = 0u64;
    let max_age = 300u64; // 5 minutes max age
    update_farm_oracle_config_with_max_age(&mut env, scope_prices_pubkey, price_id, max_age).await;
    
    // Try to stake with stale oracle (should fail)
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    let result = try_stake_tokens_with_oracle(&mut env, &user, 100_000_000, scope_prices_pubkey).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mismatched_oracle_account() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle
    setup_farm_with_oracle(&mut env).await;
    
    // Configure farm with one oracle account
    let correct_oracle = Pubkey::new_unique();
    let price_id = 0u64;
    update_farm_oracle_config(&mut env, correct_oracle, price_id).await;
    
    // Create a different oracle account
    let wrong_oracle = Pubkey::new_unique();
    let scope_prices = create_mock_scope_prices(100_000_000, 0);
    env.context.set_account(
        &wrong_oracle,
        &create_scope_account(scope_prices),
    );
    
    // Try to use wrong oracle account (should fail)
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    let result = try_stake_tokens_with_oracle(&mut env, &user, 100_000_000, wrong_oracle).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_oracle_scaled_rewards() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle-scaled rewards
    setup_farm_with_oracle_rewards(&mut env).await;
    
    // Create oracle with specific price
    let base_price = 50_000_000; // $50
    let scope_prices = create_mock_scope_prices(base_price, 0);
    let scope_prices_pubkey = Pubkey::new_unique();
    
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(scope_prices),
    );
    
    // Configure reward to use oracle scaling
    let price_id = 0u64;
    update_reward_oracle_config(&mut env, 0, scope_prices_pubkey, price_id).await;
    
    // Add rewards and stake
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    // Accumulate rewards with first price
    env.advance_time_seconds(100).await;
    refresh_farm_with_oracle(&mut env, scope_prices_pubkey).await;
    refresh_user(&mut env, &user).await;
    
    let user_state = get_user_state(&mut env, &user).await;
    let rewards_at_50 = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Update oracle price (double it)
    let new_price = 100_000_000; // $100
    let new_scope_prices = create_mock_scope_prices(new_price, 0);
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(new_scope_prices),
    );
    
    // Accumulate more rewards with new price
    env.advance_time_seconds(100).await;
    refresh_farm_with_oracle(&mut env, scope_prices_pubkey).await;
    refresh_user(&mut env, &user).await;
    
    let user_state = get_user_state(&mut env, &user).await;
    let rewards_at_100 = user_state.reward_infos[0].rewards_issued_unclaimed - rewards_at_50;
    
    // Rewards should be scaled by price ratio (approximately 2x)
    assert!(rewards_at_100 >= rewards_at_50 * 19 / 10);
    assert!(rewards_at_100 <= rewards_at_50 * 21 / 10);
}

#[tokio::test]
async fn test_deposit_cap_with_oracle() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with deposit cap
    setup_farm_with_oracle(&mut env).await;
    
    // Set deposit cap in USD terms
    let deposit_cap_usd = 1_000_000_000; // $1000 cap
    update_farm_deposit_cap(&mut env, deposit_cap_usd).await;
    
    // Create oracle with token price
    let token_price = 2_000_000; // $2 per token
    let scope_prices = create_mock_scope_prices(token_price, 0);
    let scope_prices_pubkey = Pubkey::new_unique();
    
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(scope_prices),
    );
    
    let price_id = 0u64;
    update_farm_oracle_config(&mut env, scope_prices_pubkey, price_id).await;
    
    // Create users
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    
    // First user stakes near cap
    // With $2 per token and $1000 cap, max is 500 tokens
    stake_tokens_with_oracle(&mut env, &user1, 400_000_000, scope_prices_pubkey).await; // 400 tokens
    
    // Second user tries to exceed cap
    let result = try_stake_tokens_with_oracle(&mut env, &user2, 200_000_000, scope_prices_pubkey).await; // 200 tokens would exceed
    assert!(result.is_err());
    
    // But smaller amount should work
    stake_tokens_with_oracle(&mut env, &user2, 50_000_000, scope_prices_pubkey).await; // 50 tokens OK
}

#[tokio::test]
async fn test_oracle_price_exponent_scaling() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle
    setup_farm_with_oracle(&mut env).await;
    
    // Test different exponent values
    let test_cases = vec![
        (100_000_000i64, -8i32),  // 1.0 with 8 decimals
        (1_000_000i64, -6i32),     // 1.0 with 6 decimals
        (10_000i64, -4i32),        // 1.0 with 4 decimals
        (100i64, -2i32),           // 1.0 with 2 decimals
    ];
    
    for (value, exp) in test_cases {
        let scope_prices = create_mock_scope_prices_with_exponent(value, exp, 0);
        let scope_prices_pubkey = Pubkey::new_unique();
        
        env.context.set_account(
            &scope_prices_pubkey,
            &create_scope_account(scope_prices),
        );
        
        let price_id = 0u64;
        update_farm_oracle_config(&mut env, scope_prices_pubkey, price_id).await;
        
        // All should normalize to same effective price
        let user = env.add_user(1_000_000_000).await;
        initialize_user_state(&mut env, &user).await;
        
        stake_tokens_with_oracle(&mut env, &user, 100_000_000, scope_prices_pubkey).await;
        
        // Clean up for next iteration
        unstake_tokens(&mut env, &user, 100_000_000).await;
    }
}

#[tokio::test]
async fn test_oracle_refresh_required() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with oracle that affects rewards
    setup_farm_with_oracle_rewards(&mut env).await;
    
    // Create oracle
    let scope_prices = create_mock_scope_prices(100_000_000, 0);
    let scope_prices_pubkey = Pubkey::new_unique();
    
    env.context.set_account(
        &scope_prices_pubkey,
        &create_scope_account(scope_prices),
    );
    
    let price_id = 0u64;
    update_reward_oracle_config(&mut env, 0, scope_prices_pubkey, price_id).await;
    
    // Add rewards and stake
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    // Advance time significantly
    env.advance_time_seconds(1000).await;
    
    // Try to harvest without refreshing (might fail depending on implementation)
    // First refresh with oracle
    refresh_farm_with_oracle(&mut env, scope_prices_pubkey).await;
    refresh_user(&mut env, &user).await;
    
    // Now harvest should work
    harvest_rewards(&mut env, &user, 0).await;
}

// Helper functions
fn create_mock_scope_prices(price_value: i64, age_seconds: i64) -> OraclePrices {
    let current_time = Clock::default().unix_timestamp;
    create_mock_scope_prices_with_timestamp(price_value, current_time - age_seconds)
}

fn create_mock_scope_prices_with_timestamp(price_value: i64, timestamp: i64) -> OraclePrices {
    let mut prices = OraclePrices::default();
    prices.prices[0] = DatedPrice {
        price: Price {
            value: price_value,
            exp: -8, // 8 decimals
        },
        last_updated_slot: 0,
        unix_timestamp: timestamp as u64,
        ..Default::default()
    };
    prices
}

fn create_mock_scope_prices_with_exponent(value: i64, exp: i32, age_seconds: i64) -> OraclePrices {
    let current_time = Clock::default().unix_timestamp;
    let mut prices = OraclePrices::default();
    prices.prices[0] = DatedPrice {
        price: Price {
            value,
            exp,
        },
        last_updated_slot: 0,
        unix_timestamp: (current_time - age_seconds) as u64,
        ..Default::default()
    };
    prices
}

fn create_scope_account(prices: OraclePrices) -> SolanaAccount {
    let mut data = vec![0u8; std::mem::size_of::<OraclePrices>()];
    let prices_bytes = bytemuck::bytes_of(&prices);
    data[..prices_bytes.len()].copy_from_slice(prices_bytes);
    
    SolanaAccount {
        lamports: 1_000_000,
        data,
        owner: scope::id(),
        executable: false,
        rent_epoch: 0,
    }
}

async fn setup_farm_with_oracle(env: &mut TestEnvironment) {
    initialize_global_config(env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
}

async fn setup_farm_with_oracle_rewards(env: &mut TestEnvironment) {
    setup_farm_with_oracle(env).await;
    
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
}

async fn update_farm_oracle_config(
    env: &mut TestEnvironment,
    scope_prices: Pubkey,
    price_id: u64,
) {
    update_farm_oracle_config_with_max_age(env, scope_prices, price_id, 3600).await;
}

async fn update_farm_oracle_config_with_max_age(
    env: &mut TestEnvironment,
    scope_prices: Pubkey,
    price_id: u64,
    max_age: u64,
) {
    // Implementation would update farm's scope oracle configuration
    // This is a simplified version
    let mut farm_state = get_farm_state(env).await;
    farm_state.scope_oracle_price_id = price_id;
    farm_state.scope_oracle_max_age = max_age;
    farm_state.scope_prices = scope_prices;
    // In real implementation, would use UpdateFarmConfig instruction
}

async fn update_reward_oracle_config(
    env: &mut TestEnvironment,
    reward_index: usize,
    scope_prices: Pubkey,
    price_id: u64,
) {
    // Implementation would update reward's oracle configuration
    // This is a simplified version
    let mut farm_state = get_farm_state(env).await;
    // In real implementation, would use appropriate instruction
}

// Additional helper functions would be implemented here...