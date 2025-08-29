# Solana/Rust Engineer Playbook

## Role Overview

As the Solana/Rust Engineer, you are responsible for validating Anchor constraints, PDA security, SPL token flows, CPI safety, and Solana-specific attack vectors in the KFarms protocol.

## Key Responsibilities

### 1. Account Validation

#### Anchor Constraints Checklist
```rust
#[account(
    mut,                                    // ✓ Writable only when needed
    seeds = [b"pool", pool_id.as_ref()],  // ✓ Seeds internally derived
    bump,                                   // ✓ Bump from PDA derivation
    has_one = authority,                   // ✓ Relationship validated
    constraint = pool.active @ ErrorCode::PoolInactive, // ✓ Custom constraints
)]
pub pool: Account<'info, Pool>,
```

**Validation Steps:**
1. Check `mut` is used only for state changes
2. Verify `has_one` relationships
3. Validate custom constraints
4. Ensure discriminator checks
5. Verify owner is program

#### Common Vulnerabilities
- Missing `mut` for state changes
- Client-provided bumps
- Missing ownership checks
- Account substitution attacks
- Duplicate account exploitation

### 2. PDA Security

#### Secure PDA Derivation
```rust
// GOOD: Internal derivation
let (pda, bump) = Pubkey::find_program_address(
    &[b"vault", pool.key().as_ref()],
    program_id
);

// BAD: Using client-provided bump
let pda = Pubkey::create_program_address(
    &[b"vault", pool.key().as_ref(), &[bump]],
    program_id
)?;
```

#### PDA Seed Analysis
```rust
// Check for uniqueness
seeds = [
    b"user_position",  // Constant prefix
    pool.key().as_ref(),  // Pool identifier
    user.key().as_ref(),  // User identifier
    // No user-controlled variable data!
];
```

**Security Checks:**
- Seeds include program-controlled prefixes
- No user-controlled variable seeds
- Unique combination prevents collisions
- PDAs are not on curve

### 3. Token Flow Analysis

#### SPL Token Operations
```rust
// Deposit flow
1. User -> Escrow (transfer)
2. Mint points to user
3. Update pool state

// Withdrawal flow
1. Burn user points
2. Calculate penalties
3. Escrow -> User (transfer)
4. Update pool state
```

#### Token-2022 Considerations
```rust
// Check for transfer fees
if mint.get_extension::<TransferFeeConfig>().is_ok() {
    // Handle transfer fee logic
}

// Verify freeze authority
assert!(mint.freeze_authority.is_none() || 
        mint.freeze_authority == program_pda);

// Check mint authority
assert!(mint.mint_authority == Some(program_pda));
```

### 4. CPI Safety

#### Secure CPI Pattern
```rust
// GOOD: Explicit program check
if external_program.key() != &EXPECTED_PROGRAM_ID {
    return Err(ErrorCode::InvalidProgram);
}

// Invoke with signers
let cpi_accounts = Transfer {
    from: vault.to_account_info(),
    to: user.to_account_info(),
    authority: vault_authority.to_account_info(),
};

let cpi_ctx = CpiContext::new_with_signer(
    token_program.to_account_info(),
    cpi_accounts,
    &[&vault_seeds[..]],  // PDA signer seeds
);

transfer(cpi_ctx, amount)?;
```

#### CPI Attack Vectors
- Reentrancy through callbacks
- Program substitution
- Signer confusion
- Account substitution
- Missing program ID validation

### 5. Compute Budget Management

#### Optimization Techniques
```rust
// Avoid loops over unbounded data
// BAD
for user in &pool.all_users {
    // O(n) operation
}

// GOOD: Paginated or indexed access
let user_position = pool.user_positions.get(&user_key);
```

#### Compute Unit Monitoring
```rust
// Measure compute consumption
msg!("Compute units remaining: {}", 
     sol_get_compute_budget());

// Critical operations should have budget checks
if sol_get_compute_budget() < MINIMUM_COMPUTE {
    return Err(ErrorCode::InsufficientCompute);
}
```

### 6. Time Handling

#### Clock Safety
```rust
// GOOD: Slot-based time
let clock = Clock::get()?;
let current_slot = clock.slot;

// Validate time progression
require!(
    current_slot >= last_update_slot,
    ErrorCode::TimeRegression
);

// CAREFUL: Unix timestamp can drift
let timestamp = clock.unix_timestamp as u64;
// Add bounds checking
let delta = timestamp.saturating_sub(last_timestamp);
require!(delta <= MAX_TIME_DELTA, ErrorCode::TimeDeltaTooLarge);
```

### 7. Arithmetic Safety

#### Checked Math
```rust
// GOOD: Checked operations
let result = amount
    .checked_mul(multiplier)?
    .checked_div(SCALE)?;

// GOOD: Wide math for intermediate
let wide_result = (amount as u128)
    .checked_mul(multiplier as u128)?
    .checked_div(SCALE as u128)?;
let result = u64::try_from(wide_result)?;

// BAD: Unchecked operations
let result = (amount * multiplier) / SCALE;  // Can overflow!
```

### 8. Account Size Management

#### Realloc Safety
```rust
// Safe reallocation
let new_size = account.data_len()
    .checked_add(additional_space)?;

require!(
    new_size <= MAX_ACCOUNT_SIZE,
    ErrorCode::AccountTooLarge
);

account.realloc(new_size, false)?;

// Update lamports for rent
let rent = Rent::get()?;
let required_lamports = rent.minimum_balance(new_size);
// Transfer additional lamports if needed
```

### 9. Security Patterns

#### Checks-Effects-Interactions
```rust
// 1. CHECKS - Validate all inputs
require!(amount > 0, ErrorCode::InvalidAmount);
require!(user_position.amount >= amount, ErrorCode::InsufficientBalance);

// 2. EFFECTS - Update internal state
user_position.amount = user_position.amount
    .checked_sub(amount)?;
pool.total_staked = pool.total_staked
    .checked_sub(amount)?;

// 3. INTERACTIONS - External calls last
transfer(cpi_ctx, amount)?;
```

#### Access Control
```rust
// Role-based access
#[access_control(ctx.accounts.validate())]
pub fn admin_function(ctx: Context<AdminOnly>) -> Result<()> {
    // Admin only logic
}

impl<'info> AdminOnly<'info> {
    pub fn validate(&self) -> Result<()> {
        require!(
            self.signer.key() == self.config.admin,
            ErrorCode::Unauthorized
        );
        Ok(())
    }
}
```

### 10. Testing Strategies

#### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::*;
    use solana_program_test::*;

    #[tokio::test]
    async fn test_deposit() {
        let program_test = ProgramTest::new(
            "kfarms",
            id(),
            processor!(process_instruction),
        );
        
        let (mut banks_client, payer, recent_blockhash) = 
            program_test.start().await;
        
        // Test logic
    }
}
```

#### Integration Tests
```rust
// Test instruction sequences
let instructions = vec![
    create_pool_ix(),
    deposit_ix(1000),
    lock_ix(30_days),
    claim_rewards_ix(),
    withdraw_ix(500),
];

for ix in instructions {
    banks_client.process_transaction(
        Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        )
    ).await?;
}
```

### 11. Audit Checklist

#### Account Security
- [ ] All accounts have correct owners
- [ ] Writable permissions are minimal
- [ ] Signer requirements enforced
- [ ] No account duplication
- [ ] Discriminators validated

#### PDA Security  
- [ ] Seeds derived internally
- [ ] No client-provided bumps
- [ ] Seeds are unique
- [ ] PDAs not on curve

#### Token Security
- [ ] Mint/freeze authority controlled
- [ ] Decimals handled correctly
- [ ] Transfer fees considered
- [ ] Vault solvency maintained

#### CPI Security
- [ ] Program IDs validated
- [ ] No reentrancy vectors
- [ ] Correct signer seeds
- [ ] Account validation

#### Compute/Storage
- [ ] No unbounded loops
- [ ] Realloc bounds checked
- [ ] Rent exemption maintained
- [ ] Compute budget sufficient

### 12. Red Flags

1. **`as` casts**: Potential overflow
2. **Unchecked math**: No overflow protection
3. **Missing `mut`**: State changes fail
4. **Client bumps**: PDA manipulation
5. **No ownership check**: Account substitution
6. **Unbounded loops**: DoS vector
7. **Missing constraints**: Invalid state
8. **Time assumptions**: Clock manipulation
9. **Fixed addresses**: Hardcoded keys
10. **No access control**: Unauthorized access

### 13. Tooling

```bash
# Static analysis
cargo clippy -- -D warnings

# Security audit
cargo audit

# Test coverage
cargo tarpaulin

# Fuzzing
cargo +nightly fuzz run target_function

# Anchor verification
anchor verify

# Program inspection
solana program dump <PROGRAM_ID>
```

### 14. Reporting Template

```markdown
# Solana/Anchor Security Analysis

## Account Validation
- [ ] Ownership checks: [Status]
- [ ] Permission model: [Status]
- [ ] Constraint validation: [Status]

## PDA Security
- [ ] Derivation method: [Status]
- [ ] Seed uniqueness: [Status]
- [ ] Bump handling: [Status]

## Token Flows
- [ ] SPL compliance: [Status]
- [ ] Token-2022 support: [Status]
- [ ] Decimal handling: [Status]

## Findings
### Critical
- [Finding 1]

### High
- [Finding 1]

### Medium
- [Finding 1]

## Recommendations
1. [Recommendation 1]
2. [Recommendation 2]
```

Remember: Solana's runtime is unforgiving. A single missing check can compromise the entire protocol. Be paranoid, verify everything, trust nothing from the client.