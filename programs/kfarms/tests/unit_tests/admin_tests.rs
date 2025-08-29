use anchor_lang::prelude::*;
use anchor_lang::InstructionData;
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use farms::{
    instruction::*,
    state::*,
};
use decimal_wad::decimal::Decimal;
use crate::test_utils::*;

#[tokio::test]
async fn test_deposit_to_farm_vault() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_with_single_reward(&mut env).await;
    
    // Create admin token account and mint tokens
    let admin_stake_account = env.create_token_account(
        &env.stake_mint.pubkey(),
        &env.admin.pubkey(),
        false,
    ).await;
    
    env.mint_tokens(
        &env.stake_mint.pubkey(),
        &admin_stake_account,
        &env.admin,
        10_000_000_000,
        false,
    ).await;
    
    // Get farm vault
    let (farm_vault, _) = env.get_farm_vault_pda();
    
    // Deposit to farm vault
    let deposit_amount = 5_000_000_000;
    let accounts = farms::accounts::DepositToFarmVault {
        farm: env.farm.pubkey(),
        farm_vault,
        admin: env.admin.pubkey(),
        admin_token_account: admin_stake_account,
        token_program: spl_token::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::DepositToFarmVault {
            amount: deposit_amount,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify deposit
    let vault_balance = get_token_balance(&mut env, &farm_vault).await;
    assert_eq!(vault_balance, deposit_amount);
    
    // Verify farm state updated
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.total_staked_amount, Decimal::from(deposit_amount));
}

#[tokio::test]
async fn test_withdraw_from_farm_vault() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with withdraw authority
    setup_farm_with_single_reward(&mut env).await;
    
    // Set withdraw authority
    let withdraw_authority = Keypair::new();
    update_farm_withdraw_authority(&mut env, Some(withdraw_authority.pubkey())).await;
    
    // Deposit some tokens first
    let admin_stake_account = env.create_token_account(
        &env.stake_mint.pubkey(),
        &env.admin.pubkey(),
        false,
    ).await;
    
    env.mint_tokens(
        &env.stake_mint.pubkey(),
        &admin_stake_account,
        &env.admin,
        10_000_000_000,
        false,
    ).await;
    
    deposit_to_farm_vault(&mut env, 5_000_000_000).await;
    
    // Create withdraw authority token account
    let withdraw_token_account = env.create_token_account(
        &env.stake_mint.pubkey(),
        &withdraw_authority.pubkey(),
        false,
    ).await;
    
    // Withdraw from farm vault
    let withdraw_amount = 2_000_000_000;
    let (farm_vault, _) = env.get_farm_vault_pda();
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    let accounts = farms::accounts::WithdrawFromFarmVault {
        farm: env.farm.pubkey(),
        farm_vault,
        farm_vaults_authority: farm_vault_authority,
        withdraw_authority: withdraw_authority.pubkey(),
        destination: withdraw_token_account,
        token_program: spl_token::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::WithdrawFromFarmVault {
            amount: withdraw_amount,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &withdraw_authority],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify withdrawal
    let vault_balance = get_token_balance(&mut env, &farm_vault).await;
    assert_eq!(vault_balance, 3_000_000_000);
    
    let withdraw_balance = get_token_balance(&mut env, &withdraw_token_account).await;
    assert_eq!(withdraw_balance, withdraw_amount);
    
    // Verify farm state updated
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(
        farm_state.total_staked_amount,
        Decimal::from(3_000_000_000)
    );
}

#[tokio::test]
async fn test_withdraw_from_farm_vault_freezes_farm() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with users staked
    setup_farm_with_single_reward(&mut env).await;
    
    // Set withdraw authority
    let withdraw_authority = Keypair::new();
    update_farm_withdraw_authority(&mut env, Some(withdraw_authority.pubkey())).await;
    
    // Users stake
    let user1 = env.add_user(1_000_000_000).await;
    let user2 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user1).await;
    initialize_user_state(&mut env, &user2).await;
    stake_tokens(&mut env, &user1, 500_000_000).await;
    stake_tokens(&mut env, &user2, 300_000_000).await;
    
    // Admin withdraws proportionally (should freeze farm)
    let (farm_vault, _) = env.get_farm_vault_pda();
    let vault_balance = get_token_balance(&mut env, &farm_vault).await;
    
    let withdraw_token_account = env.create_token_account(
        &env.stake_mint.pubkey(),
        &withdraw_authority.pubkey(),
        false,
    ).await;
    
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    let accounts = farms::accounts::WithdrawFromFarmVault {
        farm: env.farm.pubkey(),
        farm_vault,
        farm_vaults_authority: farm_vault_authority,
        withdraw_authority: withdraw_authority.pubkey(),
        destination: withdraw_token_account,
        token_program: spl_token::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::WithdrawFromFarmVault {
            amount: vault_balance,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &withdraw_authority],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify farm is frozen
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.is_farm_frozen(), true);
    
    // Try to stake (should fail)
    let user3 = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user3).await;
    let result = try_stake_tokens(&mut env, &user3, 100_000_000).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_add_rewards_flow() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with reward
    setup_farm_with_single_reward(&mut env).await;
    
    // Create admin reward token account and mint tokens
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
    
    // Add rewards
    let (reward_vault, _) = env.get_reward_vault_pda(&env.reward_mints[0].pubkey());
    
    let add_amount = 5_000_000_000;
    let accounts = farms::accounts::AddRewards {
        farm: env.farm.pubkey(),
        reward_mint: env.reward_mints[0].pubkey(),
        reward_vault,
        funder: env.admin.pubkey(),
        funder_reward_token_account: admin_reward_account,
        token_program: spl_token_2022::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::AddRewards {
            amount: add_amount,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify rewards added
    let farm_state = get_farm_state(&mut env).await;
    let reward_info = &farm_state.reward_infos[0];
    assert_eq!(reward_info.rewards_available, add_amount);
    
    let vault_balance = get_token_balance(&mut env, &reward_vault).await;
    assert_eq!(vault_balance, add_amount);
}

#[tokio::test]
async fn test_withdraw_reward_only_when_schedule_not_set() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with reward but no schedule
    setup_farm_base(&mut env).await;
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    
    // Initialize reward with empty schedule
    let empty_schedule = RewardScheduleCurve::default();
    initialize_reward_with_schedule(
        &mut env,
        &reward_mint.pubkey(),
        RewardType::Proportional,
        empty_schedule,
        6,
    ).await;
    env.reward_mints.push(reward_mint);
    
    // Add rewards
    add_rewards_to_farm(&mut env, 0, 5_000_000_000).await;
    
    // Admin should be able to withdraw since schedule not set
    let admin_reward_account = env.create_token_account(
        &reward_mint.pubkey(),
        &env.admin.pubkey(),
        true,
    ).await;
    
    let (reward_vault, _) = env.get_reward_vault_pda(&reward_mint.pubkey());
    
    let withdraw_amount = 2_000_000_000;
    let accounts = farms::accounts::WithdrawReward {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        reward_mint: reward_mint.pubkey(),
        reward_vault,
        destination: admin_reward_account,
        token_program: spl_token_2022::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::WithdrawReward {
            amount: withdraw_amount,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify withdrawal
    let admin_balance = get_token_balance(&mut env, &admin_reward_account).await;
    assert_eq!(admin_balance, withdraw_amount);
    
    // Now set a schedule
    let schedule = create_test_reward_schedule_curve(vec![(0, 1000)]);
    update_reward_schedule(&mut env, &reward_mint.pubkey(), schedule).await;
    
    // Try to withdraw again (should fail)
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::WithdrawReward {
            amount: 1_000_000_000,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    let result = env.context.banks_client.process_transaction(tx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_transfer_ownership() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_with_single_reward(&mut env).await;
    
    // Create new admin
    let new_admin = Keypair::new();
    
    // Initiate ownership transfer
    let accounts = farms::accounts::TransferOwnership {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
        new_admin: new_admin.pubkey(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::TransferOwnership {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify pending admin set
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.pending_admin, new_admin.pubkey());
    assert_eq!(farm_state.admin, env.admin.pubkey()); // Still old admin
    
    // New admin accepts ownership
    let accounts = farms::accounts::TransferOwnership {
        farm: env.farm.pubkey(),
        admin: new_admin.pubkey(),
        new_admin: new_admin.pubkey(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::TransferOwnership {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &new_admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify ownership transferred
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.admin, new_admin.pubkey());
    assert_eq!(farm_state.pending_admin, Pubkey::default());
}

#[tokio::test]
async fn test_update_farm_config() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_with_single_reward(&mut env).await;
    
    // Update various config options
    let new_config = FarmConfigOption {
        deposit_cap_amount: 1_000_000_000_000,
        deposit_warmup_period: 60,
        withdrawal_cooldown_period: 120,
        withdrawal_penalty_bps: 500, // 5%
        locking_mode: LockingMode::Continuous,
        locking_start_timestamp: 0,
        locking_duration: 86400,
        locking_early_withdrawal_penalty_bps: 2000, // 20%
        slashing_penalty_bps: 100, // 1%
        slashing_penalty_recipient: env.treasury_admin.pubkey(),
        reserved0: [0; 1887],
    };
    
    let accounts = farms::accounts::UpdateFarmConfig {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::UpdateFarmConfig {
            config_option: new_config.clone(),
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify config updated
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.config.deposit_cap_amount, new_config.deposit_cap_amount);
    assert_eq!(farm_state.config.deposit_warmup_period, new_config.deposit_warmup_period);
    assert_eq!(farm_state.config.withdrawal_cooldown_period, new_config.withdrawal_cooldown_period);
    assert_eq!(farm_state.config.withdrawal_penalty_bps, new_config.withdrawal_penalty_bps);
    assert_eq!(farm_state.config.locking_mode, new_config.locking_mode);
    assert_eq!(farm_state.config.locking_duration, new_config.locking_duration);
    assert_eq!(farm_state.config.locking_early_withdrawal_penalty_bps, new_config.locking_early_withdrawal_penalty_bps);
    assert_eq!(farm_state.config.slashing_penalty_bps, new_config.slashing_penalty_bps);
    assert_eq!(farm_state.config.slashing_penalty_recipient, new_config.slashing_penalty_recipient);
}

#[tokio::test]
async fn test_update_farm_admin() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm
    setup_farm_with_single_reward(&mut env).await;
    
    // Update farm admin settings
    let new_withdraw_authority = Keypair::new();
    
    let accounts = farms::accounts::UpdateFarmAdmin {
        farm: env.farm.pubkey(),
        admin: env.admin.pubkey(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::UpdateFarmAdmin {
            params: UpdateFarmAdminParams {
                withdraw_authority: Some(new_withdraw_authority.pubkey()),
                time_unit: Some(TimeUnit::Slots),
                farm_admin: None,
            },
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify updates
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.withdraw_authority, new_withdraw_authority.pubkey());
    assert_eq!(farm_state.time_unit, TimeUnit::Slots);
}

#[tokio::test]
async fn test_update_global_config() {
    let mut env = TestEnvironment::new().await;
    
    // Initialize global config
    initialize_global_config(&mut env).await;
    
    // Update various global config settings
    let new_treasury_admin = Keypair::new();
    
    // Update treasury admin
    let accounts = farms::accounts::UpdateGlobalConfig {
        global_config: env.global_config.pubkey(),
        global_admin: env.admin.pubkey(),
        new_authority: new_treasury_admin.pubkey(),
    };
    
    let mode = GlobalConfigOption::TreasuryFeeVaultAuthority as u8;
    let mut value = [0u8; 32];
    value[..32].copy_from_slice(&new_treasury_admin.pubkey().to_bytes());
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::UpdateGlobalConfig {
            mode,
            value,
        }.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &env.admin],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify update
    let global_config_account = env.context.banks_client
        .get_account(env.global_config.pubkey())
        .await
        .unwrap()
        .unwrap();
    
    let global_config_data = global_config_account.data.as_slice();
    let global_config = GlobalConfig::try_deserialize(&mut &global_config_data[8..]).unwrap();
    
    assert_eq!(global_config.treasury_fee_vault_authority, new_treasury_admin.pubkey());
}

#[tokio::test]
async fn test_withdraw_slashed_amount() {
    let mut env = TestEnvironment::new().await;
    
    // Setup farm with slashing
    setup_farm_with_single_reward(&mut env).await;
    
    // Set slashing configuration
    let slashing_recipient = Keypair::new();
    let config = FarmConfigOption {
        deposit_cap_amount: 0,
        deposit_warmup_period: 0,
        withdrawal_cooldown_period: 0,
        withdrawal_penalty_bps: 0,
        locking_mode: LockingMode::Continuous,
        locking_start_timestamp: 0,
        locking_duration: 86400,
        locking_early_withdrawal_penalty_bps: 5000, // 50%
        slashing_penalty_bps: 0,
        slashing_penalty_recipient: slashing_recipient.pubkey(),
        reserved0: [0; 1887],
    };
    
    update_farm_config(&mut env, config).await;
    
    // User stakes and unstakes early (incurring penalty)
    let user = env.add_user(1_000_000_000).await;
    initialize_user_state(&mut env, &user).await;
    stake_tokens(&mut env, &user, 500_000_000).await;
    unstake_tokens(&mut env, &user, 500_000_000).await; // Will be slashed
    
    // Verify slashed amount exists
    let farm_state = get_farm_state(&mut env).await;
    assert!(farm_state.total_slashed_amount > 0);
    let slashed_amount = farm_state.total_slashed_amount;
    
    // Create recipient token account
    let recipient_account = env.create_token_account(
        &env.stake_mint.pubkey(),
        &slashing_recipient.pubkey(),
        false,
    ).await;
    
    // Withdraw slashed amount
    let (farm_vault, _) = env.get_farm_vault_pda();
    let (farm_vault_authority, _) = env.get_farm_vault_authority_pda();
    
    let accounts = farms::accounts::WithdrawSlashedAmount {
        farm: env.farm.pubkey(),
        farm_vault,
        farm_vaults_authority: farm_vault_authority,
        slashing_penalty_recipient: slashing_recipient.pubkey(),
        destination: recipient_account,
        token_program: spl_token::id(),
    };
    
    let ix = Instruction {
        program_id: farms::id(),
        accounts: accounts.to_account_metas(None),
        data: farms::instruction::WithdrawSlashedAmount {}.data(),
    };
    
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.context.payer.pubkey()),
        &[&env.context.payer, &slashing_recipient],
        env.context.last_blockhash,
    );
    
    env.context.banks_client.process_transaction(tx).await.unwrap();
    
    // Verify withdrawal
    let recipient_balance = get_token_balance(&mut env, &recipient_account).await;
    assert_eq!(recipient_balance, slashed_amount);
    
    // Verify farm state updated
    let farm_state = get_farm_state(&mut env).await;
    assert_eq!(farm_state.total_slashed_amount, 0);
}

// Helper functions
async fn setup_farm_with_single_reward(env: &mut TestEnvironment) {
    initialize_global_config(env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
    
    let reward_mint = env.create_mint(6, &env.admin.pubkey(), true).await;
    initialize_reward(env, &reward_mint.pubkey(), RewardType::Proportional, 1000, 6).await;
    env.reward_mints.push(reward_mint);
}

async fn setup_farm_base(env: &mut TestEnvironment) {
    initialize_global_config(env).await;
    env.stake_mint = env.create_mint(9, &env.admin.pubkey(), false).await;
    initialize_farm(env).await;
}

// Additional helper functions would be implemented here...