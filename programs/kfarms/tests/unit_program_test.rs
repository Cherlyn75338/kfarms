#![cfg(test)]
use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_lang::InstructionData;
use anchor_spl::token::{self, Token, TokenAccount, Mint};
use anchor_spl::associated_token::AssociatedToken;
use solana_program_test::*;
use solana_sdk::{instruction::Instruction, signature::Keypair, signer::Signer, transaction::Transaction};

use farms as program_lib;
use farms::state::*;
use farms::utils::consts::*;

fn program_test() -> ProgramTest {
    ProgramTest::new("farms", farms::ID, processor!(farms::entry))
}

#[tokio::test]
async fn initialize_global_config_and_farm_and_reward() {
    let mut pt = program_test();
    let (mut banks_client, payer, recent_blockhash) = pt.start().await;

    // Create GlobalConfig PDA account
    let global_config_key = Keypair::new();
    let global_admin = Keypair::new();

    // Allocate GlobalConfig
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(SIZE_GLOBAL_CONFIG);
    let create_gc_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &global_config_key.pubkey(),
        lamports,
        SIZE_GLOBAL_CONFIG as u64,
        &farms::ID,
    );

    // Derive treasury authority
    let (treasury_auth, _bump) = Pubkey::find_program_address(
        &[BASE_SEED_TREASURY_VAULTS_AUTHORITY, global_config_key.pubkey().as_ref()],
        &farms::ID,
    );

    // Initialize global config
    let init_gc_ix = Instruction {
        program_id: farms::ID,
        accounts: farms::accounts::InitializeGlobalConfig {
            global_admin: global_admin.pubkey(),
            global_config: global_config_key.pubkey(),
            treasury_vaults_authority: treasury_auth,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: farms::instruction::InitializeGlobalConfig {}.data(),
    };

    let mut tx = Transaction::new_with_payer(&[create_gc_ix, init_gc_ix], Some(&payer.pubkey()));
    tx.sign(&[&payer, &global_config_key, &global_admin], recent_blockhash);
    banks_client.process_transaction(tx).await.unwrap();
}

