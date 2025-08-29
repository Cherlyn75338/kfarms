# Critical Security Findings - KFarms Audit

## 🚨 CRITICAL-001: Arbitrary Reward Theft via Delegated Stakes

### Summary
Delegated farms allow external authorities to set user stakes without any proof of custody, enabling complete theft of all rewards in the pool.

### Severity: CRITICAL
- **Likelihood**: High (trivial to exploit)
- **Impact**: Critical (total loss of funds)

### Technical Details
The `set_stake_delegated` instruction accepts arbitrary stake values without validation:

```rust
// handler_set_stake_delegated.rs:29-34
farm_operations::set_stake(
    farm_state,
    user_state,
    new_stake,  // Any value accepted!
    TimeUnit::now_from_clock(time_unit, &Clock::get()?),
)?;
```

### Attack Scenario
1. Attacker obtains control of delegate_authority
2. Creates controlled user account
3. Sets stake to u64::MAX (or any large value)
4. Calls refresh to trigger reward distribution
5. Harvests all rewards
6. Reduces stake to 0

### Proof of Concept
```rust
// Set massive stake without any tokens
set_stake_delegated(attacker_user, u64::MAX);
refresh_farm();
harvest_rewards(attacker_user); // Drains pool
set_stake_delegated(attacker_user, 0);
```

### Recommendation
Require cryptographic proof of custody:
```rust
pub struct CustodyProof {
    pub locked_amount: u64,
    pub lock_program: Pubkey,
    pub lock_account: Pubkey,
    pub signature: [u8; 64],
}

// Verify via CPI to lock program
invoke_signed(
    &verify_lock_instruction,
    &[lock_program, lock_account],
    &[authority_seeds],
)?;
```

---

## 🚨 CRITICAL-002: Division by Zero in Reward Distribution

### Summary
The reward distribution calculation divides by `total_active_stake_scaled` without checking for zero, causing program panic.

### Severity: CRITICAL
- **Likelihood**: Medium
- **Impact**: High (DoS)

### Technical Details
```rust
// farm_operations.rs:865-869
let added_reward_per_share = if farm_state.is_delegated() {
    Decimal::from(rewards) / farm_state.total_active_stake_scaled // PANIC if 0!
} else {
    Decimal::from(rewards) / farm_state.get_total_active_stake_decimal()
};
```

### Attack Scenario
1. Create delegated farm
2. Ensure total_active_stake_scaled = 0
3. Add rewards
4. Call refresh → Program panics

### Recommendation
```rust
if farm_state.total_active_stake_scaled == 0 {
    farm_state.reward_infos[reward_index].last_issuance_ts = ts;
    return Ok(());
}
// Proceed with distribution only if stake > 0
```

---

## 🚨 HIGH-001: Integer Overflow in Stake Accumulation

### Summary
The `set_stake` function uses unchecked arithmetic when updating total stakes, risking overflow.

### Severity: HIGH
- **Likelihood**: Low
- **Impact**: Critical

### Technical Details
```rust
// farm_operations.rs:481-491
let (diff, op_u64, op_u128): (u64, &OpAssignU64, &OpAssignU128) = ...
op_u128(&mut farm_state.total_active_stake_scaled, u128::from(diff));
// No overflow check!
```

### Recommendation
```rust
farm_state.total_active_stake_scaled = farm_state
    .total_active_stake_scaled
    .checked_add(u128::from(diff))
    .ok_or(FarmError::IntegerOverflow)?;
```

---

## 🚨 HIGH-002: Decimal Conversion Panics

### Summary
Multiple locations use `.unwrap()` on decimal conversions that can panic.

### Severity: HIGH
- **Likelihood**: Medium  
- **Impact**: High (DoS)

### Locations
- `state.rs:141`: `value.to_scaled_val().unwrap()`
- `state.rs:145`: `value.to_scaled_val().unwrap()`
- `state.rs:466`: `value.to_scaled_val().unwrap()`
- `state.rs:539`: `value.to_scaled_val().unwrap()`

### Recommendation
Replace all `.unwrap()` with proper error handling:
```rust
value.to_scaled_val()
    .ok_or(FarmError::DecimalOverflow)?
```

---

## Summary of Required Actions

### Immediate (Before Next Deployment)
1. **DISABLE** all delegated farms until custody proof is implemented
2. **ADD** zero-stake checks before all division operations
3. **REPLACE** all `.unwrap()` calls with error handling
4. **ADD** overflow protection to all arithmetic

### Short-term (Next Sprint)
1. Implement custody proof mechanism
2. Add rate limiting for stake changes
3. Implement EMA smoothing for external points
4. Add comprehensive test coverage

### Long-term
1. Formal verification of reward mathematics
2. External security audit
3. Bug bounty program