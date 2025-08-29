// Remediation Patches for Critical Vulnerabilities

// =============================================================================
// PATCH 1: Fix Delegated Stake Vulnerability
// =============================================================================

use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

/// New struct for custody proof
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CustodyProof {
    /// Amount of tokens locked
    pub locked_amount: u64,
    /// Program that holds the lock
    pub lock_program: Pubkey,
    /// Account containing locked tokens
    pub lock_account: Pubkey,
    /// Slot when lock was created
    pub lock_slot: u64,
    /// Signature from lock program
    pub signature: [u8; 64],
}

/// Enhanced set_stake_delegated with custody proof
pub fn set_stake_delegated_secure(
    ctx: Context<SetStakeDelegatedSecure>,
    new_stake: u64,
    custody_proof: CustodyProof,
) -> Result<()> {
    let farm_state = &mut ctx.accounts.farm_state.load_mut()?;
    let user_state = &mut ctx.accounts.user_state.load_mut()?;
    
    // 1. Verify delegate authority
    require!(
        farm_state.delegate_authority == ctx.accounts.delegate_authority.key()
            || farm_state.second_delegated_authority == ctx.accounts.delegate_authority.key(),
        FarmError::AuthorityFarmDelegateMissmatch
    );
    
    // 2. Verify custody proof
    verify_custody_proof(
        &custody_proof,
        &ctx.accounts.lock_program,
        &ctx.accounts.lock_account,
        new_stake,
    )?;
    
    // 3. Apply rate limiting
    let current_slot = Clock::get()?.slot;
    let last_update_slot = user_state.last_stake_ts; // Reuse as slot tracker
    
    const MIN_SLOTS_BETWEEN_UPDATES: u64 = 10;
    const MAX_CHANGE_PER_UPDATE: u64 = 100_000_000; // 100M tokens max change
    
    require!(
        current_slot >= last_update_slot + MIN_SLOTS_BETWEEN_UPDATES,
        FarmError::UpdateTooFrequent
    );
    
    let current_stake = user_state.active_stake_scaled as u64;
    let change = new_stake.abs_diff(current_stake);
    
    require!(
        change <= MAX_CHANGE_PER_UPDATE,
        FarmError::ExceedsRateLimit
    );
    
    // 4. Apply EMA smoothing
    let smoothed_stake = apply_ema_smoothing(current_stake, new_stake);
    
    // 5. Update state with overflow protection
    update_stake_safe(farm_state, user_state, smoothed_stake, current_slot)?;
    
    msg!(
        "Secure stake update: {} -> {} (smoothed: {})",
        current_stake,
        new_stake,
        smoothed_stake
    );
    
    Ok(())
}

/// Verify custody proof via CPI
fn verify_custody_proof(
    proof: &CustodyProof,
    lock_program: &AccountInfo,
    lock_account: &AccountInfo,
    expected_amount: u64,
) -> Result<()> {
    // Verify proof freshness
    let current_slot = Clock::get()?.slot;
    const MAX_PROOF_AGE: u64 = 100; // ~1 minute
    
    require!(
        current_slot <= proof.lock_slot + MAX_PROOF_AGE,
        FarmError::StaleProof
    );
    
    // Verify amount matches
    require!(
        proof.locked_amount >= expected_amount,
        FarmError::InsufficientCustody
    );
    
    // CPI to lock program to verify custody
    let verify_ix = solana_program::instruction::Instruction {
        program_id: *lock_program.key,
        accounts: vec![
            AccountMeta::new_readonly(*lock_account.key, false),
        ],
        data: proof.signature.to_vec(),
    };
    
    invoke(&verify_ix, &[lock_account.clone()])?;
    
    Ok(())
}

/// Apply exponential moving average smoothing
fn apply_ema_smoothing(old_stake: u64, new_stake: u64) -> u64 {
    const ALPHA: u64 = 200; // 20% weight to new value
    const SCALE: u64 = 1000;
    
    let weighted_new = (new_stake as u128) * (ALPHA as u128);
    let weighted_old = (old_stake as u128) * ((SCALE - ALPHA) as u128);
    
    ((weighted_new + weighted_old) / (SCALE as u128)) as u64
}

// =============================================================================
// PATCH 2: Fix Division by Zero
// =============================================================================

pub fn refresh_global_reward_safe(
    farm_state: &mut FarmState,
    scope_price: Option<DatedPrice>,
    ts: u64,
    reward_index: usize,
) -> Result<()> {
    let reward_info = farm_state.reward_infos[reward_index];
    
    if ts == reward_info.last_issuance_ts {
        return Ok(());
    }
    
    // CRITICAL FIX: Check for zero stake before division
    if farm_state.total_active_stake_scaled == 0 {
        // Update timestamp but skip distribution
        farm_state.reward_infos[reward_index].last_issuance_ts = ts;
        msg!("Skipping reward distribution: zero total stake");
        return Ok(());
    }
    
    // Safe to proceed with division
    let amount = calculate_reward_amount(farm_state, &reward_info, scope_price, ts)?;
    
    if amount == 0 {
        farm_state.reward_infos[reward_index].last_issuance_ts = ts;
        return Ok(());
    }
    
    let rewards = std::cmp::min(amount, reward_info.rewards_available);
    
    // Update state with overflow protection
    farm_state.reward_infos[reward_index].last_issuance_ts = ts;
    
    farm_state.reward_infos[reward_index].rewards_issued_unclaimed = 
        farm_state.reward_infos[reward_index]
            .rewards_issued_unclaimed
            .checked_add(rewards)
            .ok_or(FarmError::IntegerOverflow)?;
    
    farm_state.reward_infos[reward_index].rewards_issued_cumulative = 
        farm_state.reward_infos[reward_index]
            .rewards_issued_cumulative
            .checked_add(rewards)
            .ok_or(FarmError::IntegerOverflow)?;
    
    farm_state.reward_infos[reward_index].rewards_available = 
        farm_state.reward_infos[reward_index]
            .rewards_available
            .checked_sub(rewards)
            .ok_or(FarmError::IntegerOverflow)?;
    
    // Safe division with proper decimal handling
    update_reward_per_share_safe(farm_state, reward_index, rewards)?;
    
    Ok(())
}

fn update_reward_per_share_safe(
    farm_state: &mut FarmState,
    reward_index: usize,
    rewards: u64,
) -> Result<()> {
    use decimal_wad::decimal::Decimal;
    use uint::construct_uint;
    
    construct_uint! { struct U256(4); }
    
    // Get current reward per share
    let mut reward_per_share = farm_state.reward_infos[reward_index]
        .get_reward_per_share_decimal();
    
    // Calculate added reward per share using U256 to prevent overflow
    let rewards_scaled = U256::from(rewards) * U256::from(10u128.pow(18));
    let total_stake = U256::from(farm_state.total_active_stake_scaled);
    
    // Safe division
    let added_rps_u256 = rewards_scaled / total_stake;
    
    // Convert back to u128 with overflow check
    let added_rps_u128: u128 = added_rps_u256
        .try_into()
        .map_err(|_| FarmError::IntegerOverflow)?;
    
    let added_reward_per_share = Decimal::from_scaled_val(added_rps_u128);
    
    // Update with overflow protection
    reward_per_share = reward_per_share
        .checked_add(added_reward_per_share)
        .ok_or(FarmError::DecimalOverflow)?;
    
    // Safe conversion back
    farm_state.reward_infos[reward_index]
        .set_reward_per_share_decimal_safe(reward_per_share)?;
    
    Ok(())
}

// =============================================================================
// PATCH 3: Safe Decimal Operations
// =============================================================================

pub trait SafeDecimalOps {
    fn set_reward_per_share_decimal_safe(&mut self, value: Decimal) -> Result<()>;
    fn set_active_stake_decimal_safe(&mut self, value: Decimal) -> Result<()>;
}

impl SafeDecimalOps for RewardInfo {
    fn set_reward_per_share_decimal_safe(&mut self, value: Decimal) -> Result<()> {
        self.reward_per_share_scaled = value
            .to_scaled_val()
            .ok_or(FarmError::DecimalOverflow)?;
        Ok(())
    }
}

impl SafeDecimalOps for UserState {
    fn set_active_stake_decimal_safe(&mut self, value: Decimal) -> Result<()> {
        self.active_stake_scaled = value
            .to_scaled_val()
            .ok_or(FarmError::DecimalOverflow)?;
        Ok(())
    }
}

// =============================================================================
// PATCH 4: Safe Arithmetic Operations
// =============================================================================

pub fn update_stake_safe(
    farm_state: &mut FarmState,
    user_state: &mut UserState,
    new_stake: u64,
    current_slot: u64,
) -> Result<()> {
    let current_stake = user_state.active_stake_scaled as u64;
    
    // Calculate difference with direction
    let (diff, is_increase) = if new_stake > current_stake {
        (new_stake - current_stake, true)
    } else {
        (current_stake - new_stake, false)
    };
    
    // Update farm total with overflow protection
    if is_increase {
        farm_state.total_active_stake_scaled = farm_state
            .total_active_stake_scaled
            .checked_add(diff as u128)
            .ok_or(FarmError::IntegerOverflow)?;
            
        farm_state.total_staked_amount = farm_state
            .total_staked_amount
            .checked_add(diff)
            .ok_or(FarmError::IntegerOverflow)?;
    } else {
        farm_state.total_active_stake_scaled = farm_state
            .total_active_stake_scaled
            .checked_sub(diff as u128)
            .ok_or(FarmError::IntegerOverflow)?;
            
        farm_state.total_staked_amount = farm_state
            .total_staked_amount
            .checked_sub(diff)
            .ok_or(FarmError::IntegerOverflow)?;
    }
    
    // Update user state
    user_state.active_stake_scaled = new_stake as u128;
    user_state.last_stake_ts = current_slot;
    
    Ok(())
}

// =============================================================================
// PATCH 5: Time Safety
// =============================================================================

pub fn get_safe_timestamp(time_unit: u8) -> Result<u64> {
    let clock = Clock::get()?;
    
    // Prefer slot-based timing for security
    match time_unit {
        0 => Ok(clock.slot), // SLOT-based (preferred)
        1 => {
            // Unix timestamp - add sanity checks
            let ts = clock.unix_timestamp as u64;
            
            // Sanity check: timestamp should be reasonable
            const MIN_TIMESTAMP: u64 = 1_600_000_000; // Sep 2020
            const MAX_TIMESTAMP: u64 = 2_000_000_000; // May 2033
            
            require!(
                ts >= MIN_TIMESTAMP && ts <= MAX_TIMESTAMP,
                FarmError::InvalidTimestamp
            );
            
            Ok(ts)
        }
        _ => Err(FarmError::InvalidTimeUnit.into()),
    }
}

// =============================================================================
// New Error Codes
// =============================================================================

#[error_code]
pub enum FarmError {
    // ... existing errors ...
    
    #[msg("Update too frequent, please wait")]
    UpdateTooFrequent,
    
    #[msg("Exceeds rate limit for stake changes")]
    ExceedsRateLimit,
    
    #[msg("Custody proof is stale")]
    StaleProof,
    
    #[msg("Insufficient custody for requested stake")]
    InsufficientCustody,
    
    #[msg("Decimal overflow")]
    DecimalOverflow,
    
    #[msg("Invalid time unit")]
    InvalidTimeUnit,
}

// =============================================================================
// Enhanced Context with Additional Accounts
// =============================================================================

#[derive(Accounts)]
pub struct SetStakeDelegatedSecure<'info> {
    pub delegate_authority: Signer<'info>,
    
    #[account(mut,
        has_one = farm_state,
    )]
    pub user_state: AccountLoader<'info, UserState>,
    
    #[account(mut)]
    pub farm_state: AccountLoader<'info, FarmState>,
    
    /// Lock program that holds custody
    pub lock_program: AccountInfo<'info>,
    
    /// Account containing locked tokens
    pub lock_account: AccountInfo<'info>,
}