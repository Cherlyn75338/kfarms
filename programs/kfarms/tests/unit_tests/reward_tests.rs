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
    utils::*,
};
use decimal_wad::decimal::Decimal;
use crate::test_utils::*;

#[tokio::test]
async fn test_constant_rewards() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with constant rewards
    setup_farm_base(&mut env).await;
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &reward_mint.pubkey(), RewardType::Constant, 1000, 6).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards to farm
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await; // 10000 tokens
    
    // Create users and stake
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    
    stake_tokens(&mut env, &user1, 100_000_000).await;
    stake_tokens(&mut env, &user2, 200_000_000).await;
    
    // Advance time
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    
    // With constant rewards, both users should receive same amount
    refresh_user(&mut env, &user1).await;
    refresh_user(&mut env, &user2).await;
    
    let user1_state = get_user_state(&mut env, &user1).await;
    let user2_state = get_user_state(&mut env, &user2).await;
    
    // Constant rewards should be equal regardless of stake
    assert_eq!(
        user1_state.reward_infos[0].rewards_issued_unclaimed,
        user2_state.reward_infos[0].rewards_issued_unclaimed
    );
}

#[tokio::test]
async fn test_proportional_rewards() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with proportional rewards
    setup_farm_base(&mut env).await;
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    
    // Create users with different stakes
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    
    stake_tokens(&mut env, &user1, 100_000_000).await; // 100 tokens
    stake_tokens(&mut env, &user2, 300_000_000).await; // 300 tokens
    
    // Advance time
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user1).await;
    refresh_user(&mut env, &user2).await;
    
    // With proportional rewards, user2 should have 3x user1's rewards
    let user1_state = get_user_state(&mut env, &user1).await;
    let user2_state = get_user_state(&mut env, &user2).await;
    
    let user1_rewards = user1_state.reward_infos[0].rewards_issued_unclaimed;
    let user2_rewards = user2_state.reward_infos[0].rewards_issued_unclaimed;
    
    // User2 should have approximately 3x rewards (allowing for rounding)
    assert!(user2_rewards >= user1_rewards * 29 / 10);
    assert!(user2_rewards <= user1_rewards * 31 / 10);
}

#[tokio::test]
async fn test_rps_decimals_variations() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Test different RPS decimal configurations
    let test_cases = vec![
        (0, 1_000_000),      // No decimals
        (3, 1_000),          // 3 decimals
        (6, 1),              // 6 decimals
        (9, 1),              // 9 decimals (with adjustment)
        (12, 1),             // 12 decimals
    ];
    
    for (decimals, base_rate) in test_cases {
        let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
        initialize_reward(
            &mut env,
            &reward_mint.pubkey(),
            RewardType::Proportional,
            base_rate,
            decimals,
        ).await;
        env.reward_mints.push(reward_mint);
    }
    
    // Add rewards to all reward tokens
    for i in 0..env.reward_mints.len() {
        add_rewards_to_farm(&mut env, i, 10_000_000_000).await;
    }
    
    // Stake and accumulate rewards
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Verify all reward tokens accumulated something
    let user_state = get_user_state(&mut env, &user).await;
    for i in 0..env.reward_mints.len() {
        assert!(user_state.reward_infos[i].rewards_issued_unclaimed > 0,
                "Reward token {} with decimals should accumulate", i);
    }
}

#[tokio::test]
async fn test_reward_schedule_curve() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Create reward with schedule curve
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    let schedule = create_test_reward_schedule_curve(vec![
        (0, 1000),        // Start: 1000 per second
        (100, 2000),      // After 100s: 2000 per second
        (300, 500),       // After 300s: 500 per second
        (500, 0),         // After 500s: stop
    ]);
    
    initialize_reward_with_schedule(
        &mut env,
        &reward_mint.pubkey(),
        RewardType::Proportional,
        schedule,
        6,
    ).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards
    add_rewards_to_farm(&mut env, 0, 1_000_000_000).await;
    
    // Stake
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    // Test different phases of the schedule
    let mut previous_rewards = 0u64;
    
    // Phase 1: 0-100s at 1000/s
    env.advance_time_seconds(50).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let phase1_rewards = user_state.reward_infos[0].rewards_issued_unclaimed;
    assert!(phase1_rewards > 0);
    previous_rewards = phase1_rewards;
    
    // Phase 2: 100-300s at 2000/s (higher rate)
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let phase2_rewards = user_state.reward_infos[0].rewards_issued_unclaimed - previous_rewards;
    assert!(phase2_rewards > phase1_rewards * 3 / 2); // Should be roughly 2x for same time
    previous_rewards = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Phase 3: 300-500s at 500/s (lower rate)
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let phase3_rewards = user_state.reward_infos[0].rewards_issued_unclaimed - previous_rewards;
    assert!(phase3_rewards < phase2_rewards / 2); // Should be much less
    previous_rewards = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Phase 4: After 500s (stopped)
    env.advance_time_seconds(300).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let phase4_rewards = user_state.reward_infos[0].rewards_issued_unclaimed - previous_rewards;
    assert_eq!(phase4_rewards, 0); // No more rewards
}

#[tokio::test]
async fn test_harvest_with_min_claim_duration() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Create reward with min claim duration
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    let min_claim_duration = 120; // 2 minutes
    
    initialize_reward_with_min_claim(
        &mut env,
        &reward_mint.pubkey(),
        RewardType::Proportional,
        1000,
        6,
        min_claim_duration,
    ).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards and stake
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    // Accumulate rewards
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Try to harvest before min duration (should fail)
    let result = try_harvest_rewards(&mut env, &user, 0).await;
    assert!(result.is_err());
    
    // Advance past min claim duration
    env.advance_time_seconds(30).await; // Total 130s > 120s
    
    // Now harvest should succeed
    harvest_rewards(&mut env, &user, 0).await;
    
    // Verify rewards were claimed
    let user_balance = get_token_balance(&mut env, &user.reward_token_accounts[0]).await;
    assert!(user_balance > 0);
    
    // Try to harvest again immediately (should fail due to min duration)
    let result = try_harvest_rewards(&mut env, &user, 0).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_zero_rewards_available_edge_case() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Initialize reward but don't add any rewards
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
    
    // Stake without rewards available
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    // Advance time
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // No rewards should be accumulated
    let user_state = get_user_state(&mut env, &user).await;
    assert_eq!(user_state.reward_infos[0].rewards_issued_unclaimed, 0);
    
    // Now add rewards
    add_rewards_to_farm(&mut env, 0, 1_000_000_000).await;
    
    // Advance time again
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Now rewards should accumulate
    let user_state = get_user_state(&mut env, &user).await;
    assert!(user_state.reward_infos[0].rewards_issued_unclaimed > 0);
}

#[tokio::test]
async fn test_multiple_reward_tokens() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Initialize 3 different reward tokens
    for i in 0..3 {
        let reward_mint = env.create_mint(6 + i, &env.admin.pubkey(), true).await;
        let reward_type = if i % 2 == 0 {
            RewardType::Proportional
        } else {
            RewardType::Constant
        };
        initialize_reward(
            &mut env,
            &reward_mint.pubkey(),
            reward_type,
            1000 * (i as u64 + 1),
            6,
        ).await;
        add_rewards_to_farm(&mut env, i, 10_000_000_000).await;
        env.reward_mints.push(reward_mint);
    }
    
    // Create users and stake
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    
    stake_tokens(&mut env, &user1, 100_000_000).await;
    stake_tokens(&mut env, &user2, 200_000_000).await;
    
    // Advance time and refresh
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user1).await;
    refresh_user(&mut env, &user2).await;
    
    // Verify all rewards accumulated
    let user1_state = get_user_state(&mut env, &user1).await;
    let user2_state = get_user_state(&mut env, &user2).await;
    
    for i in 0..3 {
        assert!(user1_state.reward_infos[i].rewards_issued_unclaimed > 0);
        assert!(user2_state.reward_infos[i].rewards_issued_unclaimed > 0);
        
        if i % 2 == 0 {
            // Proportional rewards
            let ratio = user2_state.reward_infos[i].rewards_issued_unclaimed * 100 /
                       user1_state.reward_infos[i].rewards_issued_unclaimed;
            assert!(ratio >= 190 && ratio <= 210); // ~2x with rounding tolerance
        } else {
            // Constant rewards
            assert_eq!(
                user1_state.reward_infos[i].rewards_issued_unclaimed,
                user2_state.reward_infos[i].rewards_issued_unclaimed
            );
        }
    }
}

#[tokio::test]
async fn test_reward_rate_change_mid_epoch() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Create reward with changing rate
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    let schedule = create_test_reward_schedule_curve(vec![
        (0, 1000),
        (50, 2000),  // Rate doubles at 50s
    ]);
    
    initialize_reward_with_schedule(
        &mut env,
        &reward_mint.pubkey(),
        RewardType::Proportional,
        schedule,
        6,
    ).await;
    env.reward_mints.push(reward_mint);
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    
    // User stakes
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    // Accumulate at first rate
    env.advance_time_seconds(40).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let rewards_at_40s = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Cross the rate change boundary
    env.advance_time_seconds(20).await; // Now at 60s
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let rewards_at_60s = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Rewards from 40-60s should reflect rate change at 50s
    // 10s at 1000/s + 10s at 2000/s = 10000 + 20000 = 30000 (base)
    let rewards_40_to_60 = rewards_at_60s - rewards_at_40s;
    
    // Continue at new rate
    env.advance_time_seconds(20).await; // Now at 80s
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    let user_state = get_user_state(&mut env, &user).await;
    let rewards_at_80s = user_state.reward_infos[0].rewards_issued_unclaimed;
    
    // Rewards from 60-80s should be at 2000/s
    let rewards_60_to_80 = rewards_at_80s - rewards_at_60s;
    
    // Second period should have roughly same as mixed period's high-rate portion
    assert!(rewards_60_to_80 > rewards_40_to_60 * 3 / 4);
}

#[tokio::test]
async fn test_treasury_fee_collection() {
    let mut env = TestEnvironment::new().await;
    setup_farm_base(&mut env).await;
    
    // Create reward with treasury fee
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    let treasury_fee_bps = 1000; // 10%
    
    initialize_reward_with_treasury_fee(
        &mut env,
        &reward_mint.pubkey(),
        RewardType::Proportional,
        1000,
        6,
        treasury_fee_bps,
    ).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards and stake
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    // Accumulate rewards
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Get treasury vault before harvest
    let (treasury_vault, _) = env.get_treasury_vault_pda(&reward_mint.pubkey());
    let treasury_balance_before = get_token_balance(&mut env, &treasury_vault).await;
    
    // Harvest rewards
    harvest_rewards(&mut env, &user, 0).await;
    
    // Check user received 90% of rewards
    let user_balance = get_token_balance(&mut env, &user.reward_token_accounts[0]).await;
    
    // Check treasury received 10%
    let treasury_balance_after = get_token_balance(&mut env, &treasury_vault).await;
    let treasury_fee_collected = treasury_balance_after - treasury_balance_before;
    
    // Verify fee ratio
    let total_distributed = user_balance + treasury_fee_collected;
    let fee_ratio = treasury_fee_collected * 10000 / total_distributed;
    assert!(fee_ratio >= 900 && fee_ratio <= 1100); // ~10% with tolerance
}

// Helper functions specific to reward tests
async fn setup_farm_base(env: &mut TestEnvironment) {
    initialize_global_config(env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
}

async fn initialize_reward_with_schedule(
    env: &mut TestEnvironment,
    reward_mint: &Pubkey,
    reward_type: RewardType,
    schedule: RewardScheduleCurve,
    rps_decimals: u64,
) {
    let (reward_vault, _) = env.get_reward_vault_pda(reward_mint);
    let (treasury_vault, _) = env.get_treasury_vault_pda(reward_mint);
    let (treasury_vault_authority, _) = env.get_treasury_vault_authority_pda();
    
    let accounts = farms::accounts::InitializeReward {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        reward_mint: *reward_mint,
        reward_vault,
        reward_treasury_vault: treasury_vault,
        global_config: env.global_config.pubkey(),
        treasury_vaults_authority: treasury_vault_authority,
        token_program: spl_token_2022::id(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeReward {
            reward_type,
            reward_schedule_curve: schedule,
            min_claim_duration_seconds: 0,
            rewards_per_second_decimals: rps_decimals,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
}

async fn initialize_reward_with_min_claim(
    env: &mut TestEnvironment,
    reward_mint: &Pubkey,
    reward_type: RewardType,
    rate: u64,
    rps_decimals: u64,
    min_claim_duration: u64,
) {
    let (reward_vault, _) = env.get_reward_vault_pda(reward_mint);
    let (treasury_vault, _) = env.get_treasury_vault_pda(reward_mint);
    let (treasury_vault_authority, _) = env.get_treasury_vault_authority_pda();
    
    let accounts = farms::accounts::InitializeReward {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        reward_mint: *reward_mint,
        reward_vault,
        reward_treasury_vault: treasury_vault,
        global_config: env.global_config.pubkey(),
        treasury_vaults_authority: treasury_vault_authority,
        token_program: spl_token_2022::id(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let schedule = create_test_reward_schedule_curve(vec![(0, rate)]);
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeReward {
            reward_type,
            reward_schedule_curve: schedule,
            min_claim_duration_seconds: min_claim_duration,
            rewards_per_second_decimals: rps_decimals,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
}

async fn initialize_reward_with_treasury_fee(
    env: &mut TestEnvironment,
    reward_mint: &Pubkey,
    reward_type: RewardType,
    rate: u64,
    rps_decimals: u64,
    treasury_fee_bps: u64,
) {
    // First initialize the reward normally
    initialize_reward(env, reward_mint, reward_type, rate, rps_decimals).await;
    
    // Then update the treasury fee
    update_reward_treasury_fee(env, reward_mint, treasury_fee_bps).await;
}

// Additional helper functions would be implemented here...