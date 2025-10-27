use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::token_2022::{self as token22};
use anchor_spl::token_interface::{Mint as MintInterface, TokenAccount as TokenAccountInterface, TokenInterface};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use farms::{
    state::*,
    utils::*,
};
use decimal_wad::decimal::Decimal;
use std::str::FromStr;

pub struct TestEnvironment {
    pub context: ProgramTestContext,
    pub global_config: Keypair,
    pub farm: Keypair,
    pub stake_mint: Keypair,
    pub reward_mints: Vec<Keypair>,
    pub admin: Keypair,
    pub treasury_admin: Keypair,
    pub users: Vec<TestUser>,
}

pub struct TestUser {
    pub keypair: Keypair,
    pub user_state: Pubkey,
    pub stake_token_account: Pubkey,
    pub reward_token_accounts: Vec<Pubkey>,
}

impl TestEnvironment {
    pub async fn new() -> Self {
        let mut program_test = ProgramTest::new(
            "farms",
            farms::id(),
            processor!(farms::entry),
        );
        
        // Add SPL Token programs
        program_test.add_program("spl_token", spl_token::id(), None);
        program_test.add_program("spl_token_2022", spl_token_2022::id(), None);
        
        // Add Scope oracle program for testing
        program_test.add_program("scope", scope::id(), None);
        
        let mut context = program_test.start_with_context().await;
        
        let global_config = Keypair::new();
        let farm = Keypair::new();
        let stake_mint = Keypair::new();
        let admin = Keypair::new();
        let treasury_admin = Keypair::new();
        
        // Fund admin accounts
        Self::airdrop(&mut context, &admin.pubkey(), 10_000_000_000).await;
        Self::airdrop(&mut context, &treasury_admin.pubkey(), 10_000_000_000).await;
        
        TestEnvironment {
            context,
            global_config,
            farm,
            stake_mint,
            reward_mints: vec![],
            admin,
            treasury_admin,
            users: vec![],
        }
    }
    
    pub async fn airdrop(context: &mut ProgramTestContext, pubkey: &Pubkey, lamports: u64) {
        let ix = system_instruction::transfer(
            &context.payer.pubkey(),
            pubkey,
            lamports,
        );
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&context.payer.pubkey()),
            &[&context.payer],
            context.last_blockhash,
        );
        context.banks_client.process_transaction(tx).await.unwrap();
    }
    
    pub async fn create_mint(
        &mut self,
        decimals: u8,
        authority: &Pubkey,
        is_token_2022: bool,
    ) -> Keypair {
        let mint = Keypair::new();
        let rent = self.context.banks_client.get_rent().await.unwrap();
        
        let space = if is_token_2022 {
            spl_token_2022::state::Mint::LEN
        } else {
            spl_token::state::Mint::LEN
        };
        
        let lamports = rent.minimum_balance(space);
        
        let program_id = if is_token_2022 {
            spl_token_2022::id()
        } else {
            spl_token::id()
        };
        
        let create_account_ix = system_instruction::create_account(
            &self.context.payer.pubkey(),
            &mint.pubkey(),
            lamports,
            space as u64,
            &program_id,
        );
        
        let init_mint_ix = if is_token_2022 {
            spl_token_2022::instruction::initialize_mint2(
                &program_id,
                &mint.pubkey(),
                authority,
                None,
                decimals,
            ).unwrap()
        } else {
            spl_token::instruction::initialize_mint2(
                &program_id,
                &mint.pubkey(),
                authority,
                None,
                decimals,
            ).unwrap()
        };
        
        let tx = Transaction::new_signed_with_payer(
            &[create_account_ix, init_mint_ix],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer, &mint],
            self.context.last_blockhash,
        );
        
        self.context.banks_client.process_transaction(tx).await.unwrap();
        
        mint
    }
    
    pub async fn create_token_account(
        &mut self,
        mint: &Pubkey,
        owner: &Pubkey,
        is_token_2022: bool,
    ) -> Pubkey {
        let program_id = if is_token_2022 {
            spl_token_2022::id()
        } else {
            spl_token::id()
        };
        
        let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
            owner,
            mint,
            &program_id,
        );
        
        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &self.context.payer.pubkey(),
            owner,
            mint,
            &program_id,
        );
        
        let tx = Transaction::new_signed_with_payer(
            &[create_ata_ix],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer],
            self.context.last_blockhash,
        );
        
        self.context.banks_client.process_transaction(tx).await.unwrap();
        
        ata
    }
    
    pub async fn mint_tokens(
        &mut self,
        mint: &Pubkey,
        to: &Pubkey,
        authority: &Keypair,
        amount: u64,
        is_token_2022: bool,
    ) {
        let program_id = if is_token_2022 {
            spl_token_2022::id()
        } else {
            spl_token::id()
        };
        
        let mint_to_ix = if is_token_2022 {
            spl_token_2022::instruction::mint_to(
                &program_id,
                mint,
                to,
                &authority.pubkey(),
                &[],
                amount,
            ).unwrap()
        } else {
            spl_token::instruction::mint_to(
                &program_id,
                mint,
                to,
                &authority.pubkey(),
                &[],
                amount,
            ).unwrap()
        };
        
        let tx = Transaction::new_signed_with_payer(
            &[mint_to_ix],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer, authority],
            self.context.last_blockhash,
        );
        
        self.context.banks_client.process_transaction(tx).await.unwrap();
    }
    
    pub async fn add_user(&mut self, initial_stake_amount: u64) -> TestUser {
        let user = Keypair::new();
        Self::airdrop(&mut self.context, &user.pubkey(), 10_000_000_000).await;
        
        // Create stake token account and mint tokens
        let stake_token_account = self.create_token_account(
            &self.stake_mint.pubkey(),
            &user.pubkey(),
            false,
        ).await;
        
        if initial_stake_amount > 0 {
            self.mint_tokens(
                &self.stake_mint.pubkey(),
                &stake_token_account,
                &self.admin,
                initial_stake_amount,
                false,
            ).await;
        }
        
        // Create reward token accounts
        let mut reward_token_accounts = vec![];
        for reward_mint in &self.reward_mints {
            let reward_account = self.create_token_account(
                &reward_mint.pubkey(),
                &user.pubkey(),
                true,
            ).await;
            reward_token_accounts.push(reward_account);
        }
        
        // Derive user state PDA
        let (user_state, _) = Pubkey::find_program_address(
            &[
                BASE_SEED_USER_STATE,
                self.farm.pubkey().as_ref(),
                user.pubkey().as_ref(),
            ],
            &farms::id(),
        );
        
        TestUser {
            keypair: user,
            user_state,
            stake_token_account,
            reward_token_accounts,
        }
    }
    
    pub fn get_farm_vault_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                BASE_SEED_FARM_VAULT,
                self.farm.pubkey().as_ref(),
                self.stake_mint.pubkey().as_ref(),
            ],
            &farms::id(),
        )
    }
    
    pub fn get_farm_vault_authority_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                BASE_SEED_FARM_VAULTS_AUTHORITY,
                self.farm.pubkey().as_ref(),
            ],
            &farms::id(),
        )
    }
    
    pub fn get_reward_vault_pda(&self, reward_mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                BASE_SEED_REWARD_VAULT,
                self.farm.pubkey().as_ref(),
                reward_mint.as_ref(),
            ],
            &farms::id(),
        )
    }
    
    pub fn get_treasury_vault_pda(&self, reward_mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                BASE_SEED_REWARD_TREASURY_VAULT,
                self.global_config.pubkey().as_ref(),
                reward_mint.as_ref(),
            ],
            &farms::id(),
        )
    }
    
    pub fn get_treasury_vault_authority_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                BASE_SEED_TREASURY_VAULTS_AUTHORITY,
                self.global_config.pubkey().as_ref(),
            ],
            &farms::id(),
        )
    }
    
    pub async fn advance_time(&mut self, slots: u64) {
        let current_slot = self.context.banks_client.get_root_slot().await.unwrap();
        self.context.warp_to_slot(current_slot + slots).unwrap();
    }
    
    pub async fn advance_time_seconds(&mut self, seconds: i64) {
        let clock = self.context.banks_client.get_sysvar::<Clock>().await.unwrap();
        let new_unix_timestamp = clock.unix_timestamp + seconds;
        self.context.warp_to_timestamp(new_unix_timestamp).unwrap();
    }
}

// Helper functions for creating test configurations
pub fn create_test_reward_info(
    reward_mint: Pubkey,
    reward_type: RewardType,
    rewards_per_second_decimals: u64,
) -> RewardInfo {
    RewardInfo {
        reward_mint,
        reward_vault: Pubkey::default(),
        reward_type,
        reward_schedule_curve: RewardScheduleCurve::default(),
        last_issuance_ts: 0,
        reward_per_share_scaled: Decimal::zero(),
        rewards_issued_unclaimed: 0,
        rewards_issued_cumulative: 0,
        rewards_available: 0,
        rewards_per_second_decimals,
        decimals: 6,
        reward_treasury_fee_bps: 0,
        external_reward_wallet_pubkey: Pubkey::default(),
        min_claim_duration_seconds: 0,
        padding0: [0; 6],
        padding1: [0; 32],
        padding2: [0; 256],
    }
}

pub fn create_test_lock_config(
    deposit_cap_amount: u64,
    withdrawal_penalty_bps: u64,
    lock_duration: u64,
    cooldown_duration: u64,
) -> FarmConfigOption {
    FarmConfigOption {
        deposit_cap_amount,
        deposit_warmup_period: 0,
        withdrawal_cooldown_period: cooldown_duration,
        withdrawal_penalty_bps,
        locking_mode: LockingMode::None,
        locking_start_timestamp: 0,
        locking_duration: lock_duration,
        locking_early_withdrawal_penalty_bps: 0,
        slashing_penalty_bps: 0,
        slashing_penalty_recipient: Pubkey::default(),
        reserved0: [0; 1887],
    }
}

pub fn create_test_reward_schedule_curve(points: Vec<(u64, u64)>) -> RewardScheduleCurve {
    let mut curve = RewardScheduleCurve::default();
    for (i, (ts_start, rewards_per_second)) in points.iter().enumerate() {
        if i < curve.points.len() {
            curve.points[i] = RewardScheduleCurvePoint {
                ts_start: *ts_start,
                rewards_per_second: *rewards_per_second,
            };
        }
    }
    curve
}