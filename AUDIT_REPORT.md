# Kamino Farms Protocol Security Audit Report

## Executive Summary

**Protocol**: Kamino Farms (kfarms)  
**Program ID**: `FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr`  
**Audit Date**: December 2024  
**Auditor**: Security Analysis Team  
**Severity Classification**: Critical ⚠️ | High 🔴 | Medium 🟡 | Low 🟢 | Informational ℹ️

### Overall Assessment
The Kamino Farms protocol implements a staking and rewards distribution system with support for multiple reward tokens, time-locked staking, and delegated stake management. While the core mathematical implementation appears sound, several **critical security vulnerabilities** have been identified that require immediate attention.

---

## Phase 1: Discovery and Scoping ✅

### Protocol Architecture
- **Core Program**: Single Anchor program at `programs/kfarms/`
- **Key Components**:
  - Farm state management with multi-token rewards
  - User stake tracking with pending/active states
  - Delegated stake mechanism for external points integration
  - Time-based reward distribution with customizable curves
  - Withdrawal cooldown and deposit warmup periods
  - Oracle price integration via Scope

### Critical Instructions Identified
1. **Staking Operations**: `stake`, `unstake`, `set_stake_delegated`
2. **Reward Management**: `initialize_reward`, `add_rewards`, `harvest_reward`, `withdraw_reward`
3. **Farm Administration**: `initialize_farm`, `update_farm_config`, `refresh_farm`
4. **User Management**: `initialize_user`, `refresh_user_state`, `transfer_ownership`
5. **Vault Operations**: `deposit_to_farm_vault`, `withdraw_from_farm_vault`

### State Structures
- `GlobalConfig`: Protocol-wide configuration
- `FarmState`: Per-farm configuration and state
- `UserState`: Per-user stake and reward tracking
- `RewardInfo`: Per-reward token configuration

---

## Phase 2: Math-First Specification Analysis 🔴

### Core Mathematical Model

#### Variables
- **User Stake**: `A_u = active_stake_scaled` (u128)
- **Lock Duration**: Not explicitly implemented as per-user variable
- **Total Points**: `P_tot = total_active_stake_scaled` (u128)
- **Reward Rate**: `R'(t)` via `RewardScheduleCurve`
- **Cumulative Reward-per-Share**: `C = reward_per_share_scaled` (u128)
- **Scaling Factor**: `S = 10^18` (WAD from decimal_wad)

#### Critical Finding #1: Division-by-Zero Vulnerability ⚠️
**Location**: `farm_operations.rs:865-869`
```rust
let added_reward_per_share = if farm_state.is_delegated() {
    Decimal::from(rewards) / farm_state.total_active_stake_scaled  // VULNERABLE
} else {
    Decimal::from(rewards) / farm_state.get_total_active_stake_decimal()
};
```

**Issue**: When `is_delegated() == true`, the code divides by `total_active_stake_scaled` directly as u128, which could be 0.

**Impact**: Program panic/DoS when delegated farm has no active stake but attempts to distribute rewards.

**Recommendation**: Add zero-check before division:
```rust
if farm_state.total_active_stake_scaled == 0 {
    return Ok(());
}
```

#### Critical Finding #2: Reward Calculation Order Vulnerability 🔴
**Location**: Multiple locations in `farm_operations.rs`

The protocol correctly follows the pattern:
1. `refresh_global_rewards()` - Updates global reward state
2. `user_refresh_all_rewards()` - Updates user's earned rewards
3. Apply state changes

However, the delegated stake setter (`set_stake_delegated`) can bypass proper accrual in edge cases.

---

## Phase 3: External Points Setter Threat Model 🔴

### Critical Finding #3: Insufficient Access Control for Delegated Authority ⚠️
**Location**: `handler_set_stake_delegated.rs:14-18`

```rust
require!(
    farm_state.delegate_authority == ctx.accounts.delegate_authority.key()
        || farm_state.second_delegated_authority == ctx.accounts.delegate_authority.key(),
    FarmError::AuthorityFarmDelegateMissmatch
);
```

**Issues**:
1. No time-based rate limiting on stake updates
2. No bounds checking on stake amount changes
3. No verification of external points provider program
4. Missing custody proof for external points

**Attack Vectors**:
1. **Flash Points Attack**: Malicious delegate can set high stakes, trigger reward distribution, then reduce stakes
2. **Sandwich Attack**: Front-run legitimate transactions with stake manipulations
3. **Griefing**: Repeatedly change user stakes to manipulate reward distribution

**Recommendations**:
```rust
// Add rate limiting
require!(
    current_ts >= user_state.last_delegated_update_ts + MIN_UPDATE_INTERVAL,
    FarmError::DelegatedUpdateTooFrequent
);

// Add bounds checking
let max_change = user_state.active_stake_scaled * MAX_CHANGE_BPS / 10000;
require!(
    stake_change <= max_change,
    FarmError::DelegatedStakeChangeTooLarge
);

// Verify external program (via CPI)
require!(
    ctx.accounts.external_program.key() == expected_external_program,
    FarmError::InvalidExternalProgram
);
```

---

## Phase 4: Solana/Anchor Security Checklist 🟡

### Finding #4: Missing Rent-Exemption Validation 🟡
**Location**: Multiple account initializations

The protocol doesn't explicitly verify rent-exemption for newly created accounts, relying on Anchor's default behavior.

### Finding #5: Potential Reentrancy in Token Transfers 🟡
**Location**: `handler_stake.rs:45-51`, `handler_harvest_reward.rs`

Token transfers occur after state updates (good), but no explicit reentrancy guards are present.

**Recommendation**: Add explicit reentrancy protection:
```rust
require!(!ctx.accounts.farm_state.is_locked, FarmError::Reentrancy);
ctx.accounts.farm_state.is_locked = true;
// ... perform operations ...
ctx.accounts.farm_state.is_locked = false;
```

### Finding #6: Unchecked Arithmetic in Release Mode 🔴
**Location**: Throughout the codebase

While the code uses `checked_add` and `checked_sub` in many places, some operations rely on Rust's debug assertions:
- `farm_operations.rs:466-468` - Direct `try_into()` without overflow check
- Multiple `Decimal` operations that could overflow

**Recommendation**: Enable `overflow-checks` in release profile:
```toml
[profile.release]
overflow-checks = true
```

---

## Phase 5: Math-Heavy Bug Patterns 🔴

### Finding #7: Rounding Exploitation via Decimal Truncation 🔴
**Location**: `farm_operations.rs:582-584`

```rust
let reward: u64 = (new_reward_tally - rewards_tally)
    .try_floor()
    .map_err(|_| dbg_msg!(FarmError::IntegerOverflow))?;
```

**Issue**: Consistent floor operations favor the protocol over users, leading to dust accumulation.

**Impact**: Over time, significant rewards may be trapped in the protocol.

### Finding #8: Time-based Attack Vector 🟡
**Location**: `farm_operations.rs:773-779`

```rust
if ts == reward_info.last_issuance_ts {
    return Ok(());
}
if farm_state.total_active_stake_scaled == 0 {
    farm_state.reward_infos[reward_index].last_issuance_ts = ts;
    return Ok(());
}
```

**Issue**: When `total_active_stake_scaled == 0`, time advances without reward distribution, potentially losing rewards.

### Finding #9: Decimal Precision Loss 🟡
**Location**: `utils/math.rs:64-81`

The `full_decimal_mul_div` function uses 256-bit arithmetic but may lose precision in edge cases.

---

## Phase 6: Test and Verification Gaps 🔴

### Finding #10: Insufficient Test Coverage ⚠️
**Location**: `tests/kfarms.ts`

The test file contains only 17 lines with minimal coverage. Critical paths untested:
- Multi-user reward distribution
- Edge cases (zero stakes, maximum values)
- Delegated stake manipulation scenarios
- Time-based attacks
- Decimal overflow conditions

**Recommendation**: Implement comprehensive test suite:
```typescript
describe("Critical Security Tests", () => {
  it("should handle zero total stake gracefully", async () => {
    // Test reward distribution with P_tot = 0
  });
  
  it("should prevent delegated stake manipulation", async () => {
    // Test rapid stake changes by delegate
  });
  
  it("should maintain invariants under all conditions", async () => {
    // Property-based testing for mathematical invariants
  });
});
```

---

## Additional Critical Findings

### Finding #11: Oracle Price Manipulation Risk 🔴
**Location**: `farm_operations.rs:796-814`

The protocol uses Scope oracle prices without sufficient validation:
```rust
let px = price.price.value as u128;
let factor = ten_pow(price.price.exp as usize) as u128;
decimal_adjusted_amt * px / factor
```

**Issues**:
1. No sanity checks on price values
2. No comparison with TWAP
3. Single oracle dependency

### Finding #12: Withdrawal Penalty Bypass 🟡
**Location**: `stake_operations.rs` and locking mechanism

The locking mechanism can potentially be bypassed through delegated stake updates.

---

## Security Recommendations Summary

### Critical (Immediate Action Required)
1. ✅ Fix division-by-zero in delegated reward distribution
2. ✅ Implement rate limiting for delegated stake updates
3. ✅ Add comprehensive test coverage
4. ✅ Enable overflow checks in release builds
5. ✅ Implement oracle price sanity checks

### High Priority
1. ✅ Add bounds checking for delegated stake changes
2. ✅ Implement explicit reentrancy guards
3. ✅ Add TWAP validation for oracle prices
4. ✅ Fix time advancement with zero stake issue

### Medium Priority
1. ✅ Implement dust collection mechanism
2. ✅ Add event emission for critical operations
3. ✅ Implement emergency pause mechanism
4. ✅ Add slippage protection for users

### Low Priority
1. ✅ Optimize gas usage in hot paths
2. ✅ Improve error messages
3. ✅ Add more detailed logging

---

## Invariant Verification

### Global Invariants to Enforce
```rust
// Add these assertions in debug builds
debug_assert!(farm_state.total_active_stake_scaled <= MAX_TOTAL_STAKE);
debug_assert!(farm_state.reward_per_share_scaled >= last_reward_per_share);
debug_assert!(total_rewards_distributed <= total_rewards_added);
debug_assert!(user_rewards_claimed <= user_rewards_earned);
```

### Per-Instruction Invariants
1. **stake**: User balance decreases by exact amount staked
2. **unstake**: Total stake decreases by exact amount unstaked
3. **harvest_reward**: Reward balance conserved between vault and user
4. **set_stake_delegated**: Total stake sum remains consistent

---

## Attack Scenarios and Mitigations

### Scenario 1: Delegated Authority Manipulation
**Attack**: Malicious delegate rapidly changes stakes to capture unfair reward share
**Mitigation**: Implement time-weighted average stakes with minimum holding period

### Scenario 2: Oracle Price Manipulation
**Attack**: Manipulate Scope oracle to inflate reward values
**Mitigation**: Use multiple oracles, implement circuit breakers

### Scenario 3: Dust Attack
**Attack**: Create many small stakes to accumulate rounding errors
**Mitigation**: Minimum stake requirements, dust collection mechanism

---

## Conclusion

The Kamino Farms protocol demonstrates solid architectural design with sophisticated reward distribution mechanics. However, several critical vulnerabilities must be addressed before mainnet deployment:

1. **Division-by-zero vulnerability** in delegated farms (CRITICAL)
2. **Insufficient access control** for delegated stake updates (CRITICAL)
3. **Lack of comprehensive testing** (CRITICAL)
4. **Missing overflow protection** in release builds (HIGH)
5. **Oracle manipulation risks** (HIGH)

The protocol should undergo:
1. Immediate fixes for critical vulnerabilities
2. Comprehensive testing including fuzzing and formal verification
3. Additional audit after fixes are implemented
4. Gradual rollout with limits and monitoring

**Overall Risk Assessment**: HIGH - Not ready for mainnet deployment without addressing critical issues.

---

## Appendix A: Code Snippets for Fixes

### Fix 1: Division by Zero Protection
```rust
// In refresh_global_reward function
if farm_state.is_delegated() && farm_state.total_active_stake_scaled == 0 {
    // Skip reward distribution when no active stake
    return Ok(());
}
```

### Fix 2: Delegated Stake Rate Limiting
```rust
// Add to UserState
last_delegated_update_ts: u64,

// In set_stake_delegated
const MIN_UPDATE_INTERVAL: u64 = 3600; // 1 hour
require!(
    current_ts >= user_state.last_delegated_update_ts + MIN_UPDATE_INTERVAL,
    FarmError::UpdateTooFrequent
);
```

### Fix 3: Oracle Sanity Checks
```rust
// Add price validation
const MAX_PRICE_CHANGE_BPS: u64 = 2000; // 20%
if let Some(last_price) = farm_state.last_oracle_price {
    let change = price.abs_diff(last_price) * 10000 / last_price;
    require!(change <= MAX_PRICE_CHANGE_BPS, FarmError::PriceChangeToLarge);
}
```

---

## Appendix B: Testing Checklist

- [ ] Zero stake reward distribution
- [ ] Maximum stake overflow
- [ ] Delegated stake rapid changes
- [ ] Multi-user concurrent operations
- [ ] Time manipulation attacks
- [ ] Oracle price edge cases
- [ ] Decimal precision limits
- [ ] Reentrancy attempts
- [ ] Authority transfer edge cases
- [ ] Emergency withdrawal scenarios

---

*End of Audit Report*