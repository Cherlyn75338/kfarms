# State/Access Control Lead Guide

## Your Primary Responsibilities

1. **Account Model Analysis**: Map all PDAs, accounts, and their relationships
2. **Access Control Verification**: Validate all signer checks and authorities
3. **State Consistency**: Ensure atomic state transitions and consistency
4. **CPI Security**: Verify cross-program invocation boundaries
5. **Upgrade Security**: Assess upgradeability risks and controls

## Critical Focus Areas

### 1. Account Structure Mapping

```rust
// Document every account type with:
// - Seeds and derivation
// - Owner program
// - Authority structure
// - Mutability requirements
// - Size and rent requirements

// Example PDA mapping:
// Pool Account
// Seeds: [b"pool", pool_id.to_le_bytes()]
// Authority: pool.authority (Pubkey)
// Size: 256 bytes
// Mutable in: deposit, withdraw, update_pool, claim

// User Stake Account  
// Seeds: [b"stake", user.key().as_ref(), pool.key().as_ref()]
// Authority: user (must sign)
// Size: 128 bytes
// Mutable in: deposit, withdraw, claim, extend_lock
```

### 2. Access Control Matrix

| Instruction | Signer Required | Authority Check | PDA Validation | CPI Allowed |
|------------|-----------------|-----------------|----------------|-------------|
| initialize | Admin | Config authority | Pool PDA | No |
| deposit | User | User must sign | Stake PDA | Yes (token) |
| withdraw | User | User must sign | Stake PDA | Yes (token) |
| claim | User | User must sign | Stake PDA | Yes (token) |
| update_pool | Admin | Pool authority | Pool PDA | No |
| set_emissions | Admin | Config authority | Config PDA | No |
| emergency_pause | Admin | Emergency auth | Config PDA | No |

### 3. PDA Security Checklist

```rust
// For each PDA, verify:

// 1. Seeds are deterministic and collision-free
#[account(
    seeds = [b"pool", pool_id.to_le_bytes()],
    bump = pool.bump,  // Store and verify bump
)]

// 2. Correct program ownership
#[account(
    owner = program_id,  // Not system program or token program
)]

// 3. Has_one constraints enforced
#[account(
    has_one = authority,  // Verifies pool.authority == authority.key()
    has_one = vault,      // Verifies pool.vault == vault.key()
)]

// 4. Initialization protection
require!(!pool.initialized, Error::AlreadyInitialized);

// 5. Close account protection
require!(pool.total_staked == 0, Error::CannotCloseNonEmpty);
```

### 4. Authority Patterns to Verify

```rust
// Single Authority Pattern
pub struct Config {
    pub authority: Pubkey,  // Single point of failure?
    pub emergency_authority: Pubkey,  // Separate emergency power
}

// Multi-sig Pattern
pub struct Config {
    pub signers: Vec<Pubkey>,
    pub threshold: u8,  // M of N
    pub pending_action: Option<Action>,
}

// Timelock Pattern
pub struct Config {
    pub authority: Pubkey,
    pub pending_authority: Option<Pubkey>,
    pub authority_change_slot: u64,  // Timelock
}

// Role-based Pattern
pub struct Config {
    pub super_admin: Pubkey,
    pub pool_admin: Pubkey,
    pub pause_authority: Pubkey,
    pub fee_collector: Pubkey,
}
```

### 5. State Consistency Vulnerabilities

```rust
// VULNERABLE: Non-atomic updates
fn vulnerable_transfer() {
    // State 1: Deduct from sender
    sender.balance -= amount;
    
    // CPI could fail here!
    token::transfer(ctx, amount)?;
    
    // State 2: Credit to receiver  
    receiver.balance += amount;  // Never reached if CPI fails
}

// SECURE: Atomic state transitions
fn secure_transfer() {
    // Validate everything first
    require!(sender.balance >= amount);
    
    // Update internal state atomically
    sender.balance = sender.balance.checked_sub(amount)?;
    receiver.balance = receiver.balance.checked_add(amount)?;
    
    // External call last
    token::transfer(ctx, amount)?;
}
```

### 6. CPI Security Analysis

```rust
// CPI Origin Verification
fn verify_cpi_caller(ctx: &Context) -> Result<()> {
    let instruction_sysvar = &ctx.accounts.instruction_sysvar;
    
    // Parse instruction sysvar
    let current_instruction = sysvar::instructions::get_instruction_relative(
        0, 
        instruction_sysvar
    )?;
    
    // Verify calling program
    require!(
        current_instruction.program_id == WHITELISTED_PROGRAM,
        Error::UnauthorizedCaller
    );
    
    Ok(())
}

// CPI Reentrancy Protection
static mut REENTRANCY_GUARD: bool = false;

fn protected_function() -> Result<()> {
    unsafe {
        require!(!REENTRANCY_GUARD, Error::Reentrancy);
        REENTRANCY_GUARD = true;
    }
    
    // Do work...
    
    unsafe {
        REENTRANCY_GUARD = false;
    }
    
    Ok(())
}
```

## Attack Patterns to Test

### 1. Account Substitution Attack
```rust
// Test: Can attacker pass wrong accounts?
#[test]
fn test_account_substitution() {
    // Create two pools
    let pool_a = create_pool("A");
    let pool_b = create_pool("B");
    
    // Deposit to pool A
    deposit(user, pool_a, 1000);
    
    // Try to withdraw from pool B using pool A's stake
    let result = withdraw(user, pool_b, 1000);
    assert!(result.is_err());
}
```

### 2. Authority Escalation Attack
```rust
// Test: Can attacker become authority?
#[test]
fn test_authority_escalation() {
    let attacker = Keypair::new();
    
    // Try direct assignment
    let result = set_authority(attacker, attacker.pubkey());
    assert!(result.is_err());
    
    // Try through upgrade
    let result = upgrade_program(attacker);
    assert!(result.is_err());
}
```

### 3. PDA Seed Collision Attack
```rust
// Test: Can seeds collide?
#[test]
fn test_pda_collision() {
    // Try to create collision with different inputs
    let seeds1 = [b"pool", &[1, 0, 0, 0]];
    let seeds2 = [b"po", b"ol", &[1, 0, 0, 0]];
    
    let (pda1, _) = Pubkey::find_program_address(&seeds1, &program_id);
    let (pda2, _) = Pubkey::find_program_address(&seeds2, &program_id);
    
    assert_ne!(pda1, pda2);
}
```

### 4. Initialization Race Attack
```rust
// Test: Can double initialization occur?
#[test]
fn test_double_init() {
    let pool = create_uninitialized_pool();
    
    // First initialization
    initialize(pool, authority1);
    
    // Try second initialization
    let result = initialize(pool, authority2);
    assert!(result.is_err());
}
```

## Upgrade Security Checklist

- [ ] Program upgrade authority identified
- [ ] Upgrade authority is multisig or DAO
- [ ] Timelock on upgrades implemented
- [ ] Data migration paths considered
- [ ] Freeze authority implemented if needed
- [ ] Upgrade can be disabled permanently
- [ ] Version tracking implemented
- [ ] Rollback mechanism exists

## Deliverables Checklist

- [ ] Complete account model diagram
- [ ] Access control matrix for all instructions
- [ ] PDA derivation documentation
- [ ] Authority structure analysis
- [ ] CPI boundary security assessment
- [ ] State consistency verification
- [ ] Upgrade security assessment
- [ ] Attack vector test suite

## Red Flags to Watch For

1. **Missing signer checks**: Functions that don't verify signers
2. **Incorrect PDA seeds**: Non-deterministic or collision-prone seeds
3. **Wrong program owner**: Accounts owned by system program
4. **Missing has_one**: Relationships not enforced
5. **Unprotected initialization**: Can be called multiple times
6. **Authority confusion**: Multiple authority types mixed
7. **CPI without verification**: Accepting calls from any program
8. **State inconsistency**: Non-atomic multi-step operations

## Testing Tools and Techniques

```bash
# Anchor testing
anchor test

# Solana program test
cargo test-bpf

# Security scanner
cargo audit

# Account inspection
solana account <address> --output json

# Transaction simulation
solana simulate transaction <base64>
```

## Communication Protocol

1. **Immediate escalation**: Report missing access controls immediately
2. **Documentation**: Update findings in `analysis/access-control.md`
3. **Testing**: Add test cases to `testing/access-tests/`
4. **Diagrams**: Maintain account relationship diagrams in `docs/diagrams/`