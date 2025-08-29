# Phase 4: Solana/Anchor Safety Checklist

## PDA and Seeds Analysis

### ✅ Correct PDA Derivation
```rust
// Properly derived PDAs with consistent seeds
seeds = [BASE_SEED_FARM_VAULT, farm_state.key().as_ref(), mint.as_ref()]
seeds = [BASE_SEED_REWARD_VAULT, farm_state.key().as_ref(), mint.as_ref()]
seeds = [BASE_SEED_FARM_VAULTS_AUTHORITY, farm_state.key().as_ref()]
```

### ⚠️ Issue: Client-Provided Bumps Not Validated
Some handlers don't validate bump seeds properly:
```rust
// Missing bump validation in some contexts
pub farm_vaults_authority_bump: u64, // Stored but not always checked
```

**Recommendation**: Always derive and validate bumps internally.

## Account Constraints

### ✅ Proper has_one Constraints
```rust
#[account(mut,
    has_one = owner,
    has_one = farm_state,
)]
pub user_state: AccountLoader<'info, UserState>,
```

### ⚠️ Issue: Missing Rent Exemption Checks
No explicit rent exemption validation for newly created accounts.

**Recommendation**: Add rent exemption checks for all account creations.

## Token Safety

### ✅ SPL Token Validation
```rust
constraint = rewards_vault.delegate.is_none() @ FarmError::RewardsVaultHasDelegate,
constraint = rewards_vault.close_authority.is_none() @ FarmError::RewardsVaultHasCloseAuthority,
```

### ⚠️ Issue: Token-2022 Extension Handling
```rust
// Limited validation for Token-2022 extensions
validate_reward_token_extensions(&ctx.accounts.reward_mint.to_account_info())?;
```
Only checks for unsupported extensions, doesn't handle fee-bearing tokens properly.

**Recommendation**: Add comprehensive Token-2022 support or explicitly reject.

## Time Source Issues

### ⚠️ CRITICAL: Mixed Time Sources
```rust
// Sometimes uses slot
TimeUnit::now_from_clock(time_unit, &Clock::get()?)

// Sometimes uses unix_timestamp  
if ts - price.unix_timestamp > self.scope_oracle_max_age
```

**Risk**: Time manipulation, especially with unix_timestamp which can be influenced by validators.

**Recommendation**: Use slot-based timing exclusively for critical operations.

## Arithmetic Safety

### ✅ Checked Operations Used
```rust
.checked_add(rewards)
.ok_or_else(|| dbg_msg!(FarmError::IntegerOverflow))?;

.checked_sub(rewards)
.ok_or_else(|| dbg_msg!(FarmError::IntegerOverflow))?;
```

### ⚠️ Issue: Unchecked Casts
```rust
// Dangerous cast without validation
.try_into()
.expect("Delegated farm: active stake don't fit on u64");

// Unsafe decimal conversions
value.to_scaled_val().unwrap(); // Can panic
```

**Recommendation**: Replace all `unwrap()` and `expect()` with proper error handling.

## Compute Budget

### ✅ O(1) Operations
Most operations are constant time with respect to number of users.

### ⚠️ Issue: Multiple Reward Tokens
```rust
for reward_index in 0..farm_state.num_reward_tokens as usize {
    refresh_global_reward(farm_state, scope_price, ts, reward_index)?;
}
```
Linear in number of rewards (max 10), could consume significant compute.

**Recommendation**: Add compute budget management for farms with many rewards.

## CPI and Reentrancy

### ✅ State Updates Before CPI
```rust
// Correct pattern: update state before external calls
farm_operations::harvest(...)?; // Updates state
token_operations::transfer_2022_from_vault(...)?; // Then transfers
```

### ⚠️ Issue: No CPI Program Validation
Missing validation of CPI target programs in delegated operations.

**Recommendation**: Whitelist allowed CPI programs.

## Critical Vulnerabilities Found

### 1. Integer Overflow in Delegated Stake
```rust
// In set_stake function
farm_state.total_active_stake_scaled += diff; // No overflow check!
```

### 2. Division by Zero Risk
```rust
// In refresh_global_reward
let added_reward_per_share = if farm_state.is_delegated() {
    Decimal::from(rewards) / farm_state.total_active_stake_scaled // Can be 0!
}
```

### 3. Time Manipulation
```rust
// Using unix_timestamp for critical logic
if ts - price.unix_timestamp > farm_state.scope_oracle_max_age
```

### 4. Missing Account Duplication Check
No validation to prevent the same account being passed multiple times in remaining_accounts.

## Security Recommendations

### High Priority
1. Fix division by zero in reward calculations
2. Add overflow protection for all arithmetic
3. Use slot-based timing exclusively
4. Validate all PDA bumps

### Medium Priority
1. Add compute budget management
2. Implement proper Token-2022 support
3. Add CPI program whitelisting
4. Check for account duplication

### Low Priority
1. Add rent exemption validation
2. Improve error messages
3. Add more comprehensive logging