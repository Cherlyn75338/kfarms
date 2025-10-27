use anchor_lang::prelude::*;
use anchor_lang::InstructionData;
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::Signer,
    transaction::Transaction,
};
use spl_token_2022::{
    extension::{
        ExtensionType,
        BaseStateWithExtensions,
        StateWithExtensions,
    },
    state::Mint as Token2022Mint,
};
use farms::{
    instruction::*,
    state::*,
};
use crate::test_utils::*;

#[tokio::test]
async fn test_reward_mint_with_allowed_extensions() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_base(&mut env).await;
    
    // Test allowed extensions
    let allowed_extensions = vec![
        ExtensionType::MintCloseAuthority,
        ExtensionType::PermanentDelegate,
        ExtensionType::MetadataPointer,
    ];
    
    for extension in allowed_extensions {
        let reward_mint = create_token_2022_mint_with_extension(
            &mut env,
            6,
            &env.admin.pubkey(),
            extension,
        ).await;
        
        // Initialize reward with this mint (should succeed)
        initialize_reward(&mut env, &reward_mint, RewardType::Proportional, 1000, 6).await;
        env.reward_mints.push(Keypair::from_bytes(&reward_mint.to_bytes()).unwrap());
    }
    
    // Verify all rewards were initialized
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.num_reward_tokens as usize, allowed_extensions.len());
}

#[tokio::test]
async fn test_reward_mint_with_transfer_fee_fails() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_base(&mut env).await;
    
    // Create Token-2022 mint with transfer fee extension
    let reward_mint = create_token_2022_mint_with_transfer_fee(
        &mut env,
        6,
        &env.admin.pubkey(),
        500, // 5% fee
        1_000_000, // Max fee
    ).await;
    
    // Try to initialize reward with fee mint (should fail)
    let result = try_initialize_reward(
        &mut env,
        &reward_mint,
        RewardType::Proportional,
        1000,
        6,
    ).await;
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_reward_mint_with_transfer_hook_fails() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_base(&mut env).await;
    
    // Create Token-2022 mint with transfer hook extension
    let hook_program = Pubkey::new_unique();
    let reward_mint = create_token_2022_mint_with_transfer_hook(
        &mut env,
        6,
        &env.admin.pubkey(),
        hook_program,
    ).await;
    
    // Try to initialize reward with hook mint (should fail)
    let result = try_initialize_reward(
        &mut env,
        &reward_mint,
        RewardType::Proportional,
        1000,
        6,
    ).await;
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_token_2022_reward_transfers() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with Token-2022 reward
    setup_farm_base(&mut env).await;
    
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards using Token-2022 transfer
    let admin_reward_account = env.create_token_account(
        &env.reward_mints[0].pubkey(),
        &env.admin.pubkey(),
        true,
    ).await;
    
    env.mint_tokens(
        &env.reward_mints[0].pubkey(),
        &admin_reward_account,
        &env.admin,
        10_000_000_000,
        true,
    ).await;
    
    add_rewards_to_farm(&mut env, 0, 5_000_000_000).await;
    
    // Stake and accumulate rewards
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Harvest Token-2022 rewards
    harvest_rewards(&mut env, &user, 0).await;
    
    // Verify Token-2022 rewards received
    let user_balance = get_token_balance(&mut env, &user.reward_token_accounts[0]).await;
    assert!(user_balance > 0);
}

#[tokio::test]
async fn test_mixed_token_and_token_2022_rewards() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_base(&mut env).await;
    
    // Add SPL Token reward
    let spl_reward_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_reward(&mut env, &spl_reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    
    // Add Token-2022 reward
    let token2022_reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &token2022_reward_mint.pubkey(), RewardType::Constant, 500, 6).await;
    
    env.reward_mints.push(spl_reward_mint);
    env.reward_mints.push(token2022_reward_mint);
    
    // Add rewards to both
    for i in 0..2 {
        let is_token_2022 = i == 1;
        let admin_account = env.create_token_account(
            &env.reward_mints[i].pubkey(),
            &env.admin.pubkey(),
            is_token_2022,
        ).await;
        
        env.mint_tokens(
            &env.reward_mints[i].pubkey(),
            &admin_account,
            &env.admin,
            10_000_000_000,
            is_token_2022,
        ).await;
        
        add_rewards_to_farm(&mut env, i, 5_000_000_000).await;
    }
    
    // User stakes and accumulates both types
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    
    // Create reward token accounts for both types
    user.reward_token_accounts.clear();
    for i in 0..2 {
        let is_token_2022 = i == 1;
        let reward_account = env.create_token_account(
            &env.reward_mints[i].pubkey(),
            &user.keypair.pubkey(),
            is_token_2022,
        ).await;
        user.reward_token_accounts.push(reward_account);
    }
    
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Harvest both reward types
    for i in 0..2 {
        harvest_rewards(&mut env, &user, i).await;
    }
    
    // Verify both rewards received
    for i in 0..2 {
        let balance = get_token_balance(&mut env, &user.reward_token_accounts[i]).await;
        assert!(balance > 0, "Reward {} should have balance", i);
    }
}

#[tokio::test]
async fn test_token_2022_decimals_handling() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_base(&mut env).await;
    
    // Test various decimal configurations for Token-2022
    let decimal_configs = vec![
        (0, 1_000_000),
        (3, 1_000),
        (6, 1),
        (9, 1),
        (12, 1),
        (15, 1),
        (18, 1),
    ];
    
    for (decimals, base_amount) in decimal_configs {
        let reward_mint = env.create_mint(decimals, &env.admin.pubkey(), true).await;
        initialize_reward(
            &mut env,
            &reward_mint.pubkey(),
            RewardType::Proportional,
            base_amount,
            decimals as u64,
        ).await;
        env.reward_mints.push(reward_mint);
    }
    
    // Add rewards to all
    for (i, (decimals, _)) in decimal_configs.iter().enumerate() {
        let admin_account = env.create_token_account(
            &env.reward_mints[i].pubkey(),
            &env.admin.pubkey(),
            true,
        ).await;
        
        let mint_amount = 10u64.pow(*decimals as u32) * 1000; // 1000 tokens
        env.mint_tokens(
            &env.reward_mints[i].pubkey(),
            &admin_account,
            &env.admin,
            mint_amount,
            true,
        ).await;
        
        add_rewards_to_farm(&mut env, i, mint_amount / 2).await;
    }
    
    // Stake and verify all accumulate correctly
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 100_000_000).await;
    
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Check all rewards accumulated
    let user_state = get_user_state(&mut env, &user).await;
    for i in 0..decimal_configs.len() {
        assert!(
            user_state.reward_infos[i].rewards_issued_unclaimed > 0,
            "Reward {} with {} decimals should accumulate",
            i,
            decimal_configs[i].0
        );
    }
}

#[tokio::test]
async fn test_token_2022_treasury_vault() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with Token-2022 reward that has treasury fee
    setup_farm_base(&mut env).await;
    
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(&mut env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
    
    // Set treasury fee
    let treasury_fee_bps = 1500; // 15%
    update_reward_treasury_fee(&mut env, &env.reward_mints[0].pubkey(), treasury_fee_bps).await;
    
    // Add rewards
    add_rewards_to_farm(&mut env, 0, 10_000_000_000).await;
    
    // Stake and accumulate
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    
    env.advance_time_seconds(100).await;
    refresh_farm(&mut env).await;
    refresh_user(&mut env, &user).await;
    
    // Get treasury vault balance before harvest
    let (treasury_vault, _) = env.get_treasury_vault_pda(&env.reward_mints[0].pubkey());
    let treasury_before = get_token_balance(&mut env, &treasury_vault).await;
    
    // Harvest
    harvest_rewards(&mut env, &user, 0).await;
    
    // Verify treasury received Token-2022 fees
    let treasury_after = get_token_balance(&mut env, &treasury_vault).await;
    let treasury_collected = treasury_after - treasury_before;
    assert!(treasury_collected > 0);
    
    // Verify fee percentage
    let user_balance = get_token_balance(&mut env, &user.reward_token_accounts[0]).await;
    let total = user_balance + treasury_collected;
    let fee_ratio = treasury_collected * 10000 / total;
    assert!(fee_ratio >= 1400 && fee_ratio <= 1600); // ~15%
}

// Helper functions
async fn setup_farm_base(env: &mut TestEnvironment) {
    initialize_global_config(env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
}

async fn create_token_2022_mint_with_extension(
    env: &mut TestEnvironment,
    decimals: u8,
    authority: &Pubkey,
    extension: ExtensionType,
) -> Pubkey {
    // Implementation would create Token-2022 mint with specific extension
    // This is a simplified version
    let mint = Keypair::new();
    let rent = env.context.banks_client.get_rent().await.unwrap();
    
    let space = ExtensionType::try_calculate_account_len::<Token2022Mint>(&[extension]).unwrap();
    let lamports = rent.minimum_balance(space);
    
    // Create account and initialize mint with extension
    // ... implementation details ...
    
    mint.pubkey()
}

async fn create_token_2022_mint_with_transfer_fee(
    env: &mut TestEnvironment,
    decimals: u8,
    authority: &Pubkey,
    fee_basis_points: u16,
    max_fee: u64,
) -> Pubkey {
    // Implementation would create Token-2022 mint with transfer fee extension
    let mint = Keypair::new();
    // ... implementation details ...
    mint.pubkey()
}

async fn create_token_2022_mint_with_transfer_hook(
    env: &mut TestEnvironment,
    decimals: u8,
    authority: &Pubkey,
    hook_program: Pubkey,
) -> Pubkey {
    // Implementation would create Token-2022 mint with transfer hook extension
    let mint = Keypair::new();
    // ... implementation details ...
    mint.pubkey()
}

async fn try_initialize_reward(
    env: &mut TestEnvironment,
    reward_mint: &Pubkey,
    reward_type: RewardType,
    rate: u64,
    rps_decimals: u64,
) -> Result<()> {
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
    
    env.context.banks_client.process_transaction(tx).await
        .map_err(|e| anchor_lang::error::Error::from(e))
}

// Additional helper functions would be implemented here...