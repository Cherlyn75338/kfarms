# Phase 3: External Points Setter Threat Model

## Critical Finding: No Custody Proof Required ⚠️

The current implementation allows delegate authorities to set arbitrary stake amounts without proving custody of underlying tokens.

```rust
// handler_set_stake_delegated.rs
pub fn process(ctx: Context<SetStakeDelegated>, new_stake: u64) -> Result<()> {
    // Only checks if caller is delegate_authority
    require!(
        farm_state.delegate_authority == ctx.accounts.delegate_authority.key()
            || farm_state.second_delegated_authority == ctx.accounts.delegate_authority.key(),
        FarmError::AuthorityFarmDelegateMissmatch
    );
    
    // Direct stake assignment without proof!
    farm_operations::set_stake(
        farm_state,
        user_state,
        new_stake,  // Arbitrary value accepted
        ts,
    )?;
}
```

## Attack Vectors

### AV1: Flash Points Attack
**Scenario**: 
1. Attacker controls delegate_authority
2. Sets massive stake for controlled user
3. Triggers reward refresh → inflated share
4. Harvests rewards immediately
5. Reduces stake to zero

**Impact**: Complete drainage of reward pools

### AV2: Sandwich Attack on Rewards
**Scenario**:
1. Monitor mempool for `add_rewards` transaction
2. Front-run: Set high stakes for multiple users
3. Let `add_rewards` execute
4. Back-run: Harvest and reduce stakes

**Impact**: Capture majority of newly added rewards

### AV3: Desync Attack
**Scenario**:
1. External protocol has different view of user stakes
2. Exploiter manipulates external state
3. Calls `set_stake_delegated` with inflated values
4. External protocol later corrects, but rewards already claimed

**Impact**: Permanent loss of rewards

### AV4: Stake Oscillation Attack
**Scenario**:
1. Rapidly oscillate stake values
2. Exploit rounding errors in reward calculation
3. Accumulate dust rewards

**Impact**: Slow drain via precision loss

## Required Security Controls

### Control 1: Custody Proof Mechanism
```rust
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CustodyProof {
    pub locked_amount: u64,
    pub lock_expiry: i64,
    pub proof_type: ProofType,
    pub signature: [u8; 64],
}

pub enum ProofType {
    CPI { program_id: Pubkey, account: Pubkey },
    MerkleProof { root: [u8; 32], path: Vec<[u8; 32]> },
    SPLSnapshot { slot: u64, hash: [u8; 32] },
}
```

### Control 2: Rate Limiting
```rust
pub struct RateLimits {
    pub max_change_per_slot: u64,      // e.g., 1000 tokens
    pub max_change_per_epoch: u64,     // e.g., 100000 tokens
    pub min_blocks_between_updates: u32, // e.g., 10 blocks
    pub last_update_slot: u64,
}

fn enforce_rate_limits(
    user_state: &UserState,
    new_stake: u64,
    limits: &RateLimits,
) -> Result<u64> {
    let current_slot = Clock::get()?.slot;
    let delta = new_stake.abs_diff(user_state.active_stake_scaled as u64);
    
    require!(
        current_slot >= user_state.last_update_slot + limits.min_blocks_between_updates,
        Error::UpdateTooFrequent
    );
    
    require!(
        delta <= limits.max_change_per_slot,
        Error::ExceedsSlotLimit
    );
    
    Ok(new_stake)
}
```

### Control 3: Smoothing Function (EMA/TWAP)
```rust
const ALPHA: u64 = 100; // 10% weight to new value
const ALPHA_SCALE: u64 = 1000;

fn apply_ema_smoothing(
    old_stake: u64,
    new_stake_raw: u64,
) -> u64 {
    // EMA: S_t = α * X_t + (1 - α) * S_{t-1}
    let weighted_new = new_stake_raw * ALPHA;
    let weighted_old = old_stake * (ALPHA_SCALE - ALPHA);
    (weighted_new + weighted_old) / ALPHA_SCALE
}
```

### Control 4: Whitelist & Verification
```rust
#[account]
pub struct DelegateWhitelist {
    pub authorized_programs: Vec<Pubkey>,
    pub required_signer_threshold: u8,
    pub required_signers: Vec<Pubkey>,
}

fn verify_delegate_authority(
    authority: &Pubkey,
    whitelist: &DelegateWhitelist,
    signers: &[AccountInfo],
) -> Result<()> {
    require!(
        whitelist.authorized_programs.contains(authority),
        Error::UnauthorizedDelegate
    );
    
    let valid_signers = signers.iter()
        .filter(|s| s.is_signer && whitelist.required_signers.contains(s.key))
        .count();
    
    require!(
        valid_signers >= whitelist.required_signer_threshold as usize,
        Error::InsufficientSigners
    );
    
    Ok(())
}
```

### Control 5: Idempotent Ordering
```rust
fn set_stake_with_ordering(
    farm_state: &mut FarmState,
    user_state: &mut UserState,
    new_stake: u64,
    proof_slot: u64,
) -> Result<()> {
    // Always refresh rewards first
    refresh_global_rewards(farm_state, Clock::get()?.slot)?;
    user_refresh_all_rewards(farm_state, user_state)?;
    
    // Verify proof is recent
    require!(
        proof_slot >= Clock::get()?.slot - MAX_PROOF_AGE,
        Error::StaleProof
    );
    
    // Apply change
    update_stake_with_proof(user_state, new_stake, proof_slot)?;
    
    Ok(())
}
```

## Severity Assessment

| Risk | Likelihood | Impact | Overall |
|------|------------|--------|---------|
| Flash Points | High | Critical | **CRITICAL** |
| Sandwich Attack | High | High | **CRITICAL** |
| Desync | Medium | High | **HIGH** |
| Oscillation | Low | Medium | **MEDIUM** |

## Immediate Recommendations

1. **URGENT**: Disable delegated farms until custody proof is implemented
2. **HIGH**: Add rate limiting on all stake changes
3. **HIGH**: Implement EMA smoothing for external points
4. **MEDIUM**: Add monitoring for abnormal stake patterns
5. **MEDIUM**: Require multi-sig for delegate authority changes