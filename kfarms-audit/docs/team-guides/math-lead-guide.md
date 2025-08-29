# Math/Mechanism Lead Guide

## Your Primary Responsibilities

1. **Formula Extraction**: Document every mathematical formula in the codebase
2. **Invariant Definition**: Define and verify all mathematical invariants
3. **Reference Model**: Build off-chain reference implementation
4. **Overflow Analysis**: Prove all operations are safe from overflow
5. **Precision Analysis**: Verify sufficient precision and correct rounding

## Critical Focus Areas

### 1. Core Formulas to Extract

```rust
// Document each formula with:
// - Variable definitions and units
// - Value ranges and bounds
// - Overflow risk assessment
// - Precision requirements

// Example extraction:
// Formula: rewards = (user_points * delta_rpt) / SCALE
// Variables:
//   user_points: u128, range [0, MAX_STAKE * MAX_MULTIPLIER]
//   delta_rpt: u128, range [0, MAX_EMISSION * MAX_TIME * SCALE / MIN_POINTS]
//   SCALE: 10^18
// Overflow risk: user_points * delta_rpt must < 2^128
// Precision: Integer division loses < 1 unit per operation
```

### 2. Invariants to Verify

#### Conservation Invariants
```rust
// INV-1: Total distributed <= Total funded
assert!(sum_all_pools(distributed) <= total_funded);

// INV-2: User rewards + dust = Total distributed
assert!(sum_all_users(earned) + dust == distributed);
```

#### Monotonicity Invariants
```rust
// INV-3: RPT never decreases
assert!(new_rpt >= old_rpt);

// INV-4: Earned rewards never decrease (before claim)
assert!(new_earned >= old_earned || claimed_between);
```

#### Bounds Invariants
```rust
// INV-5: All intermediate calculations < 2^128
assert!(intermediate_value < u128::MAX);

// INV-6: Time deltas bounded
assert!(time_delta <= MAX_SLOTS_PER_UPDATE);
```

### 3. Reference Model Structure

```python
# reference_model.py
from decimal import Decimal, getcontext
from dataclasses import dataclass
from typing import Dict, List

# Set precision for exact calculations
getcontext().prec = 78  # Support 256-bit equivalent

@dataclass
class PoolState:
    total_points: Decimal
    rpt: Decimal  # Reward per token
    last_update: int
    emission_rate: Decimal
    weight: Decimal

@dataclass
class UserState:
    staked: Decimal
    lock_multiplier: Decimal
    points: Decimal
    paid_rpt: Decimal
    accrued: Decimal

class ReferenceModel:
    def __init__(self):
        self.pools: Dict[str, PoolState] = {}
        self.users: Dict[str, Dict[str, UserState]] = {}
        self.current_slot = 0
        self.SCALE = Decimal(10**18)
    
    def update_pool_rpt(self, pool_id: str):
        """Update pool RPT with exact decimal arithmetic"""
        pool = self.pools[pool_id]
        time_delta = self.current_slot - pool.last_update
        
        if pool.total_points > 0:
            rewards = pool.emission_rate * Decimal(time_delta)
            delta_rpt = (rewards * self.SCALE) / pool.total_points
            pool.rpt += delta_rpt
        
        pool.last_update = self.current_slot
    
    def calculate_user_rewards(self, user_id: str, pool_id: str) -> Decimal:
        """Calculate exact user rewards"""
        user = self.users[user_id][pool_id]
        pool = self.pools[pool_id]
        
        delta_rpt = pool.rpt - user.paid_rpt
        new_rewards = (user.points * delta_rpt) / self.SCALE
        
        return new_rewards + user.accrued
```

### 4. Overflow Prevention Analysis

```rust
// Maximum value analysis
const MAX_STAKE: u128 = 10_000_000_000 * 10_u128.pow(9);  // 10B tokens
const MAX_MULTIPLIER: u128 = 4;
const MAX_POINTS: u128 = MAX_STAKE * MAX_MULTIPLIER;
const MAX_EMISSION_RATE: u128 = 1_000_000 * 10_u128.pow(9);  // 1M tokens/slot
const MAX_TIME_DELTA: u128 = 432_000;  // 1 epoch
const SCALE: u128 = 10_u128.pow(18);

// Worst case multiplication
// points * emission * time * SCALE / total_points
// = MAX_POINTS * MAX_EMISSION_RATE * MAX_TIME_DELTA * SCALE / 1
// = 4*10^19 * 10^15 * 432000 * 10^18
// = ~1.7 * 10^58

// This exceeds u128 max (~3.4 * 10^38)
// SOLUTION: Must divide by total_points before multiplying by SCALE
// OR: Use intermediate division points
```

### 5. Precision and Rounding Analysis

```rust
// Rounding direction analysis
fn analyze_rounding_impact() {
    // Case 1: Reward calculation
    // earned = (points * delta_rpt) / SCALE
    // Rounding: DOWN (favors protocol)
    // Max loss per operation: 1 unit
    
    // Case 2: Emission distribution
    // pool_emission = (total_emission * weight) / total_weight  
    // Rounding: DOWN (conserves emissions)
    // Max loss: 1 unit per pool per update
    
    // Case 3: Points calculation
    // points = stake * multiplier
    // Rounding: Not applicable (exact)
    
    // Dust accumulation rate:
    // Worst case: N pools * M updates * 1 unit = total dust
    // Must track and redistribute
}
```

## Testing Requirements

### 1. Differential Testing Setup

```rust
#[test]
fn test_differential_against_reference() {
    let reference = ReferenceModel::new();
    let on_chain = OnChainSimulator::new();
    
    // Run identical operations
    for op in random_operations(1000) {
        reference.execute(op.clone());
        on_chain.execute(op);
        
        // Compare states (allowing for rounding)
        assert_states_equivalent(reference.state(), on_chain.state(), 1);
    }
}
```

### 2. Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_conservation_property(
        deposits in vec((0u64..MAX_STAKE, 0u64..MAX_LOCK), 1..100),
        updates in vec(0u64..MAX_EMISSION, 1..50),
        claims in vec(any::<bool>(), 1..100),
    ) {
        let result = simulate_protocol(deposits, updates, claims);
        
        // Conservation must hold
        prop_assert!(result.total_distributed <= result.total_minted);
        prop_assert!(result.user_sum + result.dust == result.total_distributed);
    }
}
```

### 3. Overflow Testing

```rust
#[test]
fn test_max_values_no_overflow() {
    // Test maximum possible values
    let max_test_cases = vec![
        (MAX_STAKE, MAX_MULTIPLIER, MAX_EMISSION_RATE, MAX_TIME_DELTA),
        (u128::MAX / 2, 2, MAX_EMISSION_RATE, 1),
        // Add more edge cases
    ];
    
    for (stake, mult, emission, time) in max_test_cases {
        let result = safe_reward_calculation(stake, mult, emission, time);
        assert!(result.is_ok(), "Overflow at {:?}", (stake, mult, emission, time));
    }
}
```

## Deliverables Checklist

- [ ] Complete formula documentation with units and ranges
- [ ] Invariant list with formal proofs
- [ ] Reference model implementation (Python/Rust)
- [ ] Overflow safety proof for all operations
- [ ] Precision analysis with maximum error bounds
- [ ] Differential test suite
- [ ] Property-based test suite
- [ ] Edge case test battery
- [ ] Mathematical security assessment report

## Red Flags to Watch For

1. **Unchecked arithmetic**: Any use of regular operators instead of checked_*
2. **Mixed precision**: Inconsistent scaling factors
3. **Division before multiplication**: Loss of precision
4. **Time source**: Using unix_timestamp instead of slot
5. **Unbounded loops**: Iterations over user-supplied data
6. **Missing zero checks**: Division by zero possibilities
7. **Retroactive updates**: Applying new rates to past periods
8. **Dust accumulation**: Untracked rounding losses

## Tools and Resources

- **256-bit math library**: `wide` or `uint` crate for overflow checking
- **Decimal library**: `rust_decimal` for reference implementation  
- **Fuzzing**: `cargo-fuzz` with custom harnesses
- **Property testing**: `proptest` or `quickcheck`
- **Symbolic execution**: `SMACK` or `Kani` for formal verification

## Communication Protocol

1. **Daily sync**: Report invariant violations immediately
2. **Documentation**: Update formula docs in `analysis/formulas.md`
3. **Testing**: Push test cases to `testing/math-tests/`
4. **Findings**: Log in `analysis/math-findings.md` with severity

Remember: Your work is the foundation for identifying critical vulnerabilities. A single overflow or precision error can lead to total protocol failure. Be thorough, be precise, and verify everything twice.