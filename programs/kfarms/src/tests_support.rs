#![allow(dead_code)]
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::token_interface as spl_if;

use crate::utils::consts::*;
use crate::{FarmState, GlobalConfig, UserState};

pub const DEFAULT_STAKE_DECIMALS: u8 = 6;
pub const DEFAULT_REWARD_DECIMALS: u8 = 6;

pub fn pda_find_farm_vault(farm: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BASE_SEED_FARM_VAULT, farm.as_ref(), mint.as_ref()], &crate::ID)
}

pub fn pda_find_farm_vaults_authority(farm: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BASE_SEED_FARM_VAULTS_AUTHORITY, farm.as_ref()], &crate::ID)
}

pub fn pda_find_reward_vault(farm: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BASE_SEED_REWARD_VAULT, farm.as_ref(), mint.as_ref()], &crate::ID)
}

pub fn pda_find_reward_treasury_vault(global: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[BASE_SEED_REWARD_TREASURY_VAULT, global.as_ref(), mint.as_ref()],
        &crate::ID,
    )
}

pub fn pda_find_user_state(farm: &Pubkey, delegatee: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BASE_SEED_USER_STATE, farm.as_ref(), delegatee.as_ref()], &crate::ID)
}

