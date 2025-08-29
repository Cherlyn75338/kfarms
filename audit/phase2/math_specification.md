# Phase 2: Mathematical Specification and Invariants

## Core Variables

### User-level
- `A_u`: User's token amount staked
- `S_u`: User's active stake (scaled to u128)  
- `S_u_pending`: Pending stake awaiting warmup
- `P_u`: User's points = S_u (for delegated farms)
- `D_u[r]`: User's reward debt for reward r
- `E_u[r]`: User's earned unclaimed rewards

### Farm-level
- `S_total`: Total active stake (u128 scaled)
- `S_total_pending`: Total pending stake
- `A_total`: Total token amount staked
- `R_available[r]`: Available rewards for distribution
- `C[r]`: Cumulative reward per share (u128 scaled)
- `R'[r](t)`: Emission rate at time t

### Constants
- `SCALE = 10^18`: Decimal scaling factor
- `WAD = 10^18`: Fixed-point arithmetic base

## Canonical Reward Accounting Model

### 1. Reward Distribution Update
When refreshing at time `t`:
```
Δt = max(0, t - t_last_update[r])
if S_total > 0:
    amount = min(R'[r](t) * Δt, R_available[r])
    C[r] += (amount * SCALE) / S_total
    R_available[r] -= amount
    t_last_update[r] = t
```

### 2. User Reward Calculation
```
new_debt = S_u * C[r]
earned = (new_debt - D_u[r]) / SCALE
E_u[r] += earned
D_u[r] = new_debt
```

### 3. Delegated Farm Points Setting
For delegated farms:
```
S_u_new = external_points (directly set, no token backing)
Δpoints = |S_u_new - S_u_old|
S_total += Δpoints (if increase)
S_total -= Δpoints (if decrease)
```

## Critical Invariants

### I1: Non-Negativity
- ∀u,r: S_u ≥ 0, E_u[r] ≥ 0, D_u[r] ≥ 0
- ∀r: C[r] ≥ C_prev[r] (monotonic increase)
- S_total ≥ 0, R_available[r] ≥ 0

### I2: Conservation of Rewards
```
Total_distributed[r] = Σ_u E_u[r] + R_issued_unclaimed[r]
Total_distributed[r] ≤ Total_added[r] - R_available[r]
```

### I3: Stake Consistency
For non-delegated farms:
```
S_total = Σ_u S_u
A_total = Σ_u A_u
S_u = A_u (1:1 ratio)
```

For delegated farms:
```
S_total = Σ_u S_u (no token backing required)
A_total = 0 (no real tokens)
```

### I4: Division Safety
```
if S_total == 0:
    skip reward distribution
    update t_last only
```

### I5: Overflow Protection
All operations use checked arithmetic:
```
C[r] ∈ [0, 2^128)
S_u ∈ [0, 2^128)
Intermediate: use U256 for mul before div
```

### I6: Time Monotonicity
```
t_current ≥ t_last_update[r]
t_current ≥ t_last_stake
```

## Vulnerability Analysis

### V1: Delegated Stake Manipulation ⚠️ CRITICAL
**Issue**: No custody proof for external points
```solidity
// Current implementation in set_stake_delegated
user_state.active_stake_scaled = new_stake; // Direct assignment!
farm_state.total_active_stake += delta;
```
**Risk**: Attacker can claim arbitrary reward share without tokens

### V2: Reward Per Share Precision Loss
**Issue**: Integer division in reward distribution
```
added_reward_per_share = rewards / total_stake
```
**Risk**: Dust accumulation, especially with small rewards or large stakes

### V3: Decimal Conversion Overflow
**Issue**: Multiple precision conversions
```
Decimal::from_scaled_val() -> can panic
.to_scaled_val() -> can overflow
```
**Risk**: DoS or incorrect calculations

### V4: Race Condition in Delegated Updates
**Issue**: No atomicity between external state and farm state
**Risk**: Double-claim if external program updates during harvest

## Recommended Fixes

### Fix 1: Custody Proof for Delegated Stakes
```rust
pub fn set_stake_delegated_with_proof(
    ctx: Context<SetStakeDelegated>,
    new_stake: u64,
    proof: CustodyProof,
) -> Result<()> {
    // Verify proof via CPI or signature
    verify_custody_proof(&proof, new_stake)?;
    // Apply smoothing/rate limiting
    let smoothed_stake = apply_ema(old_stake, new_stake, ALPHA);
    // Update with bounds
    require!(smoothed_stake <= MAX_STAKE_DELTA);
    ...
}
```

### Fix 2: Improved Precision Handling
```rust
// Use u256 for intermediate calculations
let numerator = U256::from(rewards) * U256::from(SCALE);
let denominator = U256::from(total_stake);
let added_rps = (numerator / denominator).try_into()?;
```

### Fix 3: Safe Decimal Operations
```rust
impl SafeDecimal for Decimal {
    fn safe_from_scaled(val: u128) -> Result<Self> {
        if val > MAX_DECIMAL_VAL {
            return Err(DecimalOverflow);
        }
        Ok(Decimal::from_scaled_val(val))
    }
}
```