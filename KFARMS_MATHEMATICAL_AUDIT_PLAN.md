# KFarms Mathematical Audit Plan & Attack Surface Analysis

## Executive Summary

The KFarms protocol is a staking and farming system built on Solana that enables:
- Token staking with reward distribution
- Delegated staking where external protocols can manage user stakes
- Time-locked deposits with withdrawal penalties
- Oracle-based reward calculations using Scope price feeds
- Multiple reward token support (up to 10 tokens)

## 1. Architecture Overview

### Core Modules Identified

```
programs/kfarms/src/
├── state.rs (647 lines) - Core state structures and reward curves
├── farm_operations.rs (951 lines) - Main business logic
├── stake_operations.rs (432 lines) - Stake conversion and management
├── token_operations.rs (68 lines) - Token transfer operations
├── handlers/ (27 handler files) - Individual operation handlers
└── utils/
    ├── math.rs - Mathematical operations with overflow protection
    ├── withdrawal_penalty.rs - Early withdrawal penalty calculations
    └── scope.rs - Oracle price integration
```

### Key State Structures

1. **GlobalConfig**: Global admin and treasury configuration
2. **FarmState**: Main farm state with rewards, staking amounts, and oracle config
3. **UserState**: Individual user staking and reward tracking
4. **RewardInfo**: Per-token reward configuration and accounting

## 2. Mathematical Risk Analysis

### 2.1 Critical Mathematical Operations

#### A. Stake Conversion Functions (High Risk)
```rust
// stake_operations.rs:147-168
pub fn convert_stake_to_amount(
    stake: Decimal,
    total_stake: Decimal,
    total_amount: u64,
    round_up: bool,
) -> u64
```

**Vulnerabilities Identified:**
1. **Rounding manipulation**: The `round_up` parameter allows controlled rounding direction
2. **Division by zero protection**: Handled but edge cases with zero total_stake
3. **Precision loss**: Converting between Decimal and u64

#### B. Reward Distribution (Critical)
```rust
// farm_operations.rs:765-877
pub fn refresh_global_reward(
    farm_state: &mut FarmState,
    scope_price: Option<DatedPrice>,
    ts: u64,
    reward_index: usize,
) -> Result<()>
```

**Mathematical Concerns:**
1. **Oracle price manipulation**: Lines 796-814 apply oracle prices without TWAP
2. **Integer overflow risks**: Despite checked_add/sub usage, complex calculations at lines 783-826
3. **Reward per share calculation**: Line 865-869 uses different formulas for delegated vs non-delegated

#### C. Withdrawal Penalty Calculation
```rust
// utils/withdrawal_penalty.rs:5-49
fn get_withdrawal_penalty_bps(
    timestamp_beginning: u64,
    timestamp_now: u64,
    timestamp_maturity: u64,
    penalty_bps: u64,
) -> Result<u64, FarmError>
```

**Issues:**
1. **Linear penalty decay**: Line 46 `penalty = penalty_bps * time_remaining / total_duration`
2. **No minimum penalty enforcement**: Could approach zero near maturity
3. **Timestamp manipulation**: Relies on block timestamps

### 2.2 Precision & Rounding Analysis

#### Critical Precision Loss Points:

1. **full_decimal_mul_div** (math.rs:64-81)
   - Uses U256 for intermediate calculations
   - Potential overflow at line 73: `numerator = a_scaled_bigint * wad_big_int * b`
   - Panic on overflow rather than graceful handling

2. **u64_mul_div** (math.rs:83-90)
   - Basic mul-div pattern with U128 intermediate
   - Panics on overflow: `.expect("u64_mul_div overflow")`

3. **Decimal to u64 conversions**
   - Multiple locations use `try_floor()` and `try_ceil()`
   - Asymmetric rounding can be exploited for value extraction

## 3. Attack Surface Mapping

### 3.1 Governance Integrity
**Finding**: No governance module present in current codebase
- No voting mechanisms identified
- Admin functions use simple authority checks
- Transfer ownership exists but no multi-sig or timelock

### 3.2 Financial Math Safety

#### High-Risk Areas:
1. **Reward accumulation overflow** (farm_operations.rs:844-859)
   - Uses checked arithmetic but complex state updates
   - Potential for reward double-counting

2. **Stake scaling operations** (throughout stake_operations.rs)
   - Decimal precision (18 decimals) to u64 conversions
   - Rounding errors accumulate over time

3. **Oracle price application** (farm_operations.rs:796-814)
   - Direct multiplication without slippage protection
   - No sanity checks on price bounds

### 3.3 Lock/Unlock Logic

#### Vulnerabilities:
1. **Delegated stake authority** (handler_set_stake_delegated.rs)
   - Two authorities can modify user stakes
   - No rate limiting or maximum change constraints
   - Line 16: Either primary OR secondary authority can act

2. **Warmup/Cooldown periods** (state.rs:97-98)
   - u32 timestamps could overflow in ~136 years
   - No maximum period enforcement

3. **Pending stake mechanisms**
   - Complex three-state system (active, pending deposit, pending withdrawal)
   - State transition timing attacks possible

### 3.4 Oracle Reliance

#### Critical Issues:
1. **Single price point usage** (farm_operations.rs:799-807)
   - No TWAP/VWAP implementation
   - Simple staleness check (max_age)
   - Scope oracle manipulation = direct reward manipulation

2. **Price application formula** (line 811-813)
   ```rust
   let px = price.price.value as u128;
   let factor = ten_pow(price.price.exp as usize) as u128;
   decimal_adjusted_amt * px / factor
   ```
   - Direct multiplication without bounds checking
   - Exponent handling could overflow with malicious oracle data

### 3.5 Token/Point Custody Model

#### Delegated Staking Risks:
1. **Mismatched invariants**: External protocol controls stake amounts
2. **No slashing protection**: Delegated authority can zero stakes
3. **Reward distribution mismatch**: Rewards based on stake that protocol controls

## 4. Detailed Test Cases

### 4.1 Overflow/Underflow Tests

```rust
// Test Case 1: Maximum stake amount
test_max_stake() {
    stake_amount = u64::MAX;
    total_stake = Decimal::from(1);
    // Should handle without overflow
}

// Test Case 2: Minimum precision
test_minimum_amounts() {
    stake_amount = 1;
    total_stake = Decimal::from(u64::MAX);
    // Check for precision loss
}

// Test Case 3: Rapid stake/unstake cycles
test_stake_unstake_loop() {
    for i in 0..1000 {
        stake(1);
        unstake(1);
    }
    // Check for rounding accumulation
}
```

### 4.2 Oracle Manipulation Tests

```rust
// Test Case 4: Price spike attack
test_oracle_price_spike() {
    normal_price = 100;
    spike_price = u64::MAX;
    // Update oracle, claim rewards
    // Check for overflow in reward calculation
}

// Test Case 5: Stale price exploitation
test_stale_price() {
    set_max_age(3600); // 1 hour
    advance_time(3599); // Just under limit
    // Attempt reward claim with old price
}
```

### 4.3 Timing Attack Tests

```rust
// Test Case 6: Warmup period bypass
test_warmup_bypass() {
    stake(amount);
    // Attempt immediate activation
    // Try delegated stake override
}

// Test Case 7: Penalty calculation edge
test_penalty_edge() {
    lock_duration = 100;
    current_time = lock_start + 99;
    // Check penalty calculation at boundary
}
```

## 5. Exploitation Scenarios

### Scenario 1: Delegated Stake Manipulation
1. Malicious delegated authority sets user stake to maximum
2. Claims rewards based on inflated stake
3. Resets stake before user notices
4. **Impact**: Reward theft, accounting corruption

### Scenario 2: Oracle Front-Running
1. Monitor oracle update transactions
2. Submit reward claim transaction with higher priority
3. Claim rewards at old price before update
4. **Impact**: Arbitrage profit from price discrepancy

### Scenario 3: Rounding Extraction
1. Repeatedly stake amounts that round favorably
2. Unstake with opposite rounding
3. Extract dust amounts per cycle
4. **Impact**: Slow value extraction over time

## 6. Remediation Recommendations

### Priority 1 (Critical)
1. **Implement TWAP for oracle prices**: Use time-weighted average prices
2. **Add slippage protection**: Maximum price change per update
3. **Fix decimal conversions**: Use consistent rounding with dust collection

### Priority 2 (High)
1. **Rate limit delegated operations**: Maximum stake changes per period
2. **Add invariant checks**: Total stakes = sum of user stakes
3. **Implement circuit breakers**: Pause on anomalous activity

### Priority 3 (Medium)
1. **Add governance timelock**: Delay admin operations
2. **Implement multi-sig**: Require multiple signatures for critical ops
3. **Add event emission**: Better monitoring and alerting

## 7. Formal Verification Requirements

### Invariants to Verify
1. `sum(user_stakes) == total_farm_stake`
2. `rewards_issued <= rewards_available + rewards_added`
3. `user_rewards_claimed <= user_rewards_earned`
4. `pending_deposits + pending_withdrawals <= total_locked`

### Properties to Prove
1. No user can claim more rewards than earned
2. Stake conversions preserve total value
3. Oracle updates cannot cause overflow
4. Penalty calculations decrease monotonically

## 8. Testing Strategy

### Unit Tests Required
- [ ] All mathematical operations with edge values
- [ ] Decimal conversion accuracy
- [ ] Overflow protection in all arithmetic
- [ ] Oracle price bound validation

### Integration Tests Required
- [ ] Multi-user staking scenarios
- [ ] Reward distribution fairness
- [ ] Delegated authority limits
- [ ] Time-based attack scenarios

### Fuzzing Targets
1. `convert_stake_to_amount` with random inputs
2. `refresh_global_reward` with extreme prices
3. `RewardScheduleCurve` with malformed curves
4. Penalty calculations with random timestamps

## 9. Audit Team Roles

| Role | Responsibility | Focus Areas |
|------|---------------|-------------|
| Math Auditor | Verify all formulas and precision | stake_operations.rs, math.rs |
| Oracle Auditor | Test price manipulation vectors | scope.rs, farm_operations.rs L796-814 |
| Locking Auditor | Verify timelock and penalty logic | withdrawal_penalty.rs, state transitions |
| Delegation Auditor | Test delegated stake scenarios | handler_set_stake_delegated.rs |
| Integration Tester | End-to-end attack scenarios | All handlers |

## 10. Continuous Monitoring

### On-Chain Monitoring Required
1. Large stake changes (>10% of pool)
2. Oracle price deviations (>5% from CEX)
3. Unusual reward claim patterns
4. Delegated authority actions

### Alert Thresholds
- Reward rate changes >20%
- Total stake changes >30% in 24h
- Oracle staleness >10 minutes
- Failed transactions spike

## Conclusion

The KFarms protocol exhibits several critical mathematical vulnerabilities:

1. **Oracle manipulation** is the highest risk - direct price application without TWAP
2. **Delegated staking** lacks sufficient controls and rate limiting
3. **Precision loss** in multiple decimal/integer conversions
4. **Missing invariant enforcement** throughout the codebase

Immediate action required on oracle implementation and delegated authority controls before mainnet deployment.