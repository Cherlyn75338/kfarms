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
};
use crate::test_utils::*;

#[tokio::test]
async fn test_initialize_global_config() {
    let mut env = TestEnvironment::new().await;
    
    // Create initialize global config instruction
    let accounts = farms::accounts::InitializeGlobalConfig {
        payer: env.context.payer.pubkey(),
        global_config: env.global_config.pubkey(),
        global_admin: env.admin.pubkey(),
        treasury_fee_vault_authority: env.treasury_admin.pubkey(),
        system_program: solana_sdk::system_program::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeGlobalConfig {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.global_config],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify global config was created
    let global_config_account = env.context.banks_client
        .get_account(env.global_config.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(global_config_account.owner, farms::id());
    
    // Deserialize and verify initial state
    let global_config_data = global_config_account.data.as_slice();
    let global_config = GlobalConfig::try_deserialize(&mut &global_config_data[8..]).unwrap();
    
    assert_eq!(global_config.global_admin, env.admin.pubkey());
    assert_eq!(global_config.treasury_fee_vault_authority, env.treasury_admin.pubkey());
    assert_eq!(global_config.pending_global_admin, Pubkey::default());
}

#[tokio::test]
async fn test_initialize_farm() {
    let mut env = TestEnvironment::new().await;
    
    // First initialize global config
    initialize_global_config(&mut env).await;
    
    // Create stake mint
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    
    // Get PDAs
    let (farm_vault, _) = env.get_farm_vault_pda();
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    // Initialize farm
    let accounts = farms::accounts::InitializeFarm {
        global_config: env.global_config.pubkey(),
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        farm_vault,
        farm_vaults_authority: farm_vault_authority,
        token: env.stake_mint.pubkey(),
        token_program: spl_token::id(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeFarm {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.farm, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify farm was created
    let farm_account = env.context.banks_client
        .get_account(env.farm.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(farm_account.owner, farms::id());
    
    // Verify farm vault was created
    let farm_vault_account = env.context.banks_client
        .get_account(farm_vault)
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(farm_vault_account.owner, spl_token::id());
}

#[tokio::test]
async fn test_initialize_farm_delegated() {
    let mut env = TestEnvironment::new().await;
    
    // Initialize global config
    initialize_global_config(&mut env).await;
    
    // Create stake mint
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    
    // Create delegate authorities
    let delegate_authority = Keypair::new();
    let second_delegate = Keypair::new();
    
    // Get PDAs
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    // Initialize delegated farm
    let accounts = farms::accounts::InitializeFarmDelegated {
        global_config: env.global_config.pubkey(),
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        delegate_authority: delegate_authority.pubkey(),
        second_delegate_authority: second_delegate.pubkey(),
        farm_vaults_authority: farm_vault_authority,
        token: env.stake_mint.pubkey(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeFarmDelegated {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.farm, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify farm was created with delegation
    let farm_account = env.context.banks_client
        .get_account(env.farm.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    let farm_data = farm_account.data.as_slice();
    let farm_state = FarmState::try_deserialize(&mut &farm_data[8..]).unwrap();
    
    assert_eq!(farm_state.delegated_stake_farm_state.is_some(), true);
    if let Some(delegated) = farm_state.delegated_stake_farm_state {
        assert_eq!(delegated.delegate_authority, delegate_authority.pubkey());
        assert_eq!(delegated.second_delegate_authority, second_delegate.pubkey());
    }
}

#[tokio::test]
async fn test_initialize_reward() {
    let mut env = TestEnvironment::new().await;
    
    // Setup: Initialize global config and farm
    initialize_global_config(&mut env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(&mut env).await;
    
    // Create reward mint (Token-2022)
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    env.reward_mints.push(reward_mint);
    
    // Get PDAs
    let (reward_vault, _) = env.get_reward_vault_pda(&reward_mint.pubkey());
    let (treasury_vault, _) = env.get_treasury_vault_pda(&reward_mint.pubkey());
    let (treasury_vault_authority, _) = env.get_treasury_vault_authority_pda();
    
    // Initialize reward
    let accounts = farms::accounts::InitializeReward {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        reward_mint: reward_mint.pubkey(),
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
            reward_type: RewardType::Proportional,
            reward_schedule_curve: create_test_reward_schedule_curve(vec![
                (0, 100),
                (1000, 200),
                (2000, 50),
            ]),
            min_claim_duration_seconds: 60,
            rewards_per_second_decimals: 6,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify reward vault was created
    let reward_vault_account = env.context.banks_client
        .get_account(reward_vault)
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(reward_vault_account.owner, spl_token_2022::id());
    
    // Verify treasury vault was created
    let treasury_vault_account = env.context.banks_client
        .get_account(treasury_vault)
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(treasury_vault_account.owner, spl_token_2022::id());
    
    // Verify farm state was updated with reward info
    let farm_account = env.context.banks_client
        .get_account(env.farm.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    let farm_data = farm_account.data.as_slice();
    let farm_state = FarmState::try_deserialize(&mut &farm_data[8..]).unwrap();
    
    assert_eq!(farm_state.num_reward_tokens, 1);
    assert_eq!(farm_state.reward_infos[0].reward_mint, reward_mint.pubkey());
    assert_eq!(farm_state.reward_infos[0].reward_type, RewardType::Proportional);
    assert_eq!(farm_state.reward_infos[0].min_claim_duration_seconds, 60);
}

#[tokio::test]
async fn test_initialize_multiple_rewards() {
    let mut env = TestEnvironment::new().await;
    
    // Setup
    initialize_global_config(&mut env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(&mut env).await;
    
    // Initialize multiple rewards (up to 10)
    for i in 0..10 {
        let reward_mint = env.create_mint(6 + i, &env.admin.pubkey(), true).await;
        
        let (reward_vault, _) = env.get_reward_vault_pda(&reward_mint.pubkey());
        let (treasury_vault, _) = env.get_treasury_vault_pda(&reward_mint.pubkey());
        let (treasury_vault_authority, _) = env.get_treasury_vault_authority_pda();
        
        let accounts = farms::accounts::InitializeReward {
            farm: env.farm.pubkey(),
            admin: env.admin.pubkey(),
            payer: env.context.payer.pubkey(),
            reward_mint: reward_mint.pubkey(),
            reward_vault,
            reward_treasury_vault: treasury_vault,
            global_config: env.global_config.pubkey(),
            treasury_vaults_authority: treasury_vault_authority,
            token_program: spl_token_2022::id(),
            system_program: solana_sdk::system_program::id(),
            rent: solana_sdk::sysvar::rent::id(),
        };
        
        let reward_type = if i % 2 == 0 {
            RewardType::Proportional
        } else {
            RewardType::Constant
        };
        
        let ix = Instruction {
            program_id: farms::id(),
            accounts: accounts.to_account_metas(None),
            data: farms::instruction::InitializeReward {
                reward_type,
                reward_schedule_curve: create_test_reward_schedule_curve(vec![
                    (0, 100 * (i as u64 + 1)),
                ]),
                min_claim_duration_seconds: 30 * (i as u64 + 1),
                rewards_per_second_decimals: 6 + i as u64,
            }.data(),
        };
        
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.context.payer.pubkey()),
            &[&env.context.payer, &env.admin],
            env.context.last_blockhash,
        );
        
        env.context.banks_client.process_transaction(tx).await.unwrap();
        env.reward_mints.push(reward_mint);
    }
    
    // Verify all rewards were initialized
    let farm_account = env.context.banks_client
        .get_account(env.farm.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    let farm_data = farm_account.data.as_slice();
    let farm_state = FarmState::try_deserialize(&mut &farm_data[8..]).unwrap();
    
    assert_eq!(farm_state.num_reward_tokens, 10);
    
    // Try to add 11th reward (should fail)
    let extra_reward_mint = env.create_mint(16, &env.admin.pubkey(), true).await;
    let (reward_vault, _) = env.get_reward_vault_pda(&extra_reward_mint.pubkey());
    let (treasury_vault, _) = env.get_treasury_vault_pda(&extra_reward_mint.pubkey());
    let (treasury_vault_authority, _) = env.get_treasury_vault_authority_pda();
    
    let accounts = farms::accounts::InitializeReward {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        reward_mint: extra_reward_mint.pubkey(),
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
            reward_type: RewardType::Proportional,
            reward_schedule_curve: RewardScheduleCurve::default(),
            min_claim_duration_seconds: 0,
            rewards_per_second_decimals: 6,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    // This should fail
    let result = env.context.banks_client.process_transaction(tx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_initialize_user() {
    let mut env = TestEnvironment::new().await;
    
    // Setup
    initialize_global_config(&mut env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(&mut env).await;
    
    // Add a user
    let user = env.add_user(1_000_000).await;
    
    // Initialize user state
    let accounts = farms::accounts::InitializeUser {
        farm: env.farm.pubkey(),
        user_state: user.user_state,
        owner: user.keypair.pubkey(),
        delegatee: user.keypair.pubkey(),
        payer: env.context.payer.pubkey(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeUser {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &user.keypair],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify user state was created
    let user_state_account = env.context.banks_client
        .get_account(user.user_state)
        .await
        .unwrap()
        .unwrap();
    
    assert_eq!(user_state_account.owner, farms::id());
    
    let user_state_data = user_state_account.data.as_slice();
    let user_state = UserState::try_deserialize(&mut &user_state_data[8..]).unwrap();
    
    assert_eq!(user_state.user_key, user.keypair.pubkey());
    assert_eq!(user_state.farm_state, env.farm.pubkey());
    assert_eq!(user_state.is_initialized, 1);
    assert_eq!(user_state.active_stake_scaled, Decimal::zero());
    assert_eq!(user_state.pending_deposit_stake_scaled, Decimal::zero());
}

// Helper functions
async fn initialize_global_config(env: &mut TestEnvironment) {
    let accounts = farms::accounts::InitializeGlobalConfig {
        payer: env.context.payer.pubkey(),
        global_config: env.global_config.pubkey(),
        global_admin: env.admin.pubkey(),
        treasury_fee_vault_authority: env.treasury_admin.pubkey(),
        system_program: solana_sdk::system_program::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeGlobalConfig {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.global_config],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
}

async fn initialize_farm(env: &mut TestEnvironment) {
    let (farm_vault, _) = env.get_farm_vault_pda();
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    let accounts = farms::accounts::InitializeFarm {
        global_config: env.global_config.pubkey(),
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        payer: env.context.payer.pubkey(),
        farm_vault,
        farm_vaults_authority: farm_vault_authority,
        token: env.stake_mint.pubkey(),
        token_program: spl_token::id(),
        system_program: solana_sdk::system_program::id(),
        rent: solana_sdk::sysvar::rent::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::InitializeFarm {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.farm, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
}