# Protocol Mathematician Playbook

## Role Overview

As the Protocol Mathematician, you are responsible for the mathematical correctness and formal verification of all reward calculations, point systems, and economic invariants in the KFarms protocol.

## Key Responsibilities

### 1. Formula Derivation and Verification

#### Reward Distribution Formula
```
C(t+Δt) = C(t) + (R'(t) × Δt × SCALE) / P_tot(t)
earned_u = ⌊(P_u × (C - C_paid)) / SCALE⌋ + accrued_u
```

**Verification Steps:**
1. Prove conservation: Σ earned_u ≤ ∫ R'(t) dt
2. Bound rounding error: ε ≤ n × SCALE^(-1)
3. Verify monotonicity: C(t+Δt) ≥ C(t)

#### Lock Multiplier Function
```
M(L) = 1 + (L / L_max) × (M_max - 1)
```

**Properties to Verify:**
- M(0) = 1 (no lock = 1x multiplier)
- M(L_max) = M_max
- M is monotonically increasing
- M is continuous

### 2. Invariant Specification

#### Global Invariants
```rust
// Conservation
assert!(total_distributed <= total_emitted + rounding_tolerance);

// Non-negativity
assert!(all_values >= 0);

// Bounded growth
assert!(cumulative_rpp <= MAX_CUMULATIVE);
```

#### Per-Operation Invariants

**Deposit:**
- Pre: P_tot_old ≥ 0
- Post: P_tot_new = P_tot_old + P_u_new
- Post: P_u_new = A_u × M(L) + P_ext

**Withdraw:**
- Pre: P_u ≥ amount_to_withdraw
- Post: P_tot_new = P_tot_old - P_u_removed
- Penalty: amount_out = amount × (1 - penalty_rate)

**Claim:**
- Pre: earned_u ≥ 0
- Post: C_paid_new = C_current
- Post: accrued_new = 0

### 3. Error Bound Analysis

#### Rounding Error Accumulation
```
Per-claim error: ε_claim ≤ 1 token
Total error: ε_total ≤ n_claims × ε_claim
Relative error: ε_rel = ε_total / total_distributed
```

**Acceptable Bounds:**
- ε_rel < 0.01% for normal operations
- ε_rel < 0.1% under adversarial conditions

#### Overflow Analysis
```
Max values:
- P_tot_max = 2^64 × 1000 (allowing headroom)
- C_max = 2^128 / SCALE
- Time_max = 2^63 (unix timestamp)
```

### 4. TWAP/Governance Math

#### Time-Weighted Average Points
```
TWAP = (Σ P_i × Δt_i) / T_window
```

**Properties:**
- Resistant to flash manipulation
- Smooth transitions
- Bounded rate of change

#### EMA Smoothing
```
P_smooth(t) = α × P_instant(t) + (1-α) × P_smooth(t-1)
```

**Optimal α Selection:**
- α = 0.1 for normal operations
- α = 0.01 near governance events

### 5. Verification Techniques

#### Formal Methods
1. **SMT Solving**: Use Z3 to verify invariants
2. **Model Checking**: TLA+ specifications
3. **Theorem Proving**: Coq/Lean proofs for critical properties

#### Numerical Analysis
1. **Fixed-Point Arithmetic**: Verify precision
2. **Floating-Point**: Avoid in critical paths
3. **Wide Math**: Use u128 for intermediate calculations

### 6. Test Case Generation

#### Edge Cases
```python
test_cases = [
    # Boundary values
    (0, 0, 0),  # Empty pool
    (MAX_UINT64, 1, MAX_UINT128),  # Max values
    
    # Precision tests
    (1, SCALE, 1),  # Minimum unit
    (SCALE-1, SCALE, 0),  # Rounding down
    
    # Time boundaries
    (amount, 0, base_points),  # No lock
    (amount, MAX_DURATION, max_points),  # Max lock
]
```

#### Property Tests
```rust
proptest! {
    #[test]
    fn conservation_holds(
        emissions in 0..MAX_EMISSION,
        users in 1..1000,
        time in 0..MAX_TIME
    ) {
        let distributed = simulate_distribution(emissions, users, time);
        prop_assert!(distributed <= emissions * time);
    }
}
```

### 7. Documentation Requirements

#### Formula Documentation Template
```markdown
## Formula: [Name]

### Mathematical Definition
[LaTeX formula]

### Implementation
```rust
// Rust implementation
```

### Properties
- Property 1: [Description]
- Property 2: [Description]

### Bounds
- Input: [min, max]
- Output: [min, max]

### Error Analysis
- Rounding: ±[value]
- Overflow: Safe up to [value]
```

### 8. Review Checklist

- [ ] All formulas have formal definitions
- [ ] Invariants specified and proven
- [ ] Error bounds calculated
- [ ] Overflow conditions analyzed
- [ ] TWAP/governance math verified
- [ ] Test cases cover edge conditions
- [ ] Documentation complete
- [ ] Peer review conducted

### 9. Tools and Resources

#### Software
- **Z3**: SMT solver for invariant checking
- **SageMath**: Symbolic computation
- **Python/NumPy**: Numerical verification
- **TLA+**: Formal specification
- **Alloy**: Relational logic

#### References
- [Fixed-Point Arithmetic](https://en.wikipedia.org/wiki/Fixed-point_arithmetic)
- [Numerical Analysis](https://www.numerical-recipes.com/)
- [Formal Methods](https://www.microsoft.com/en-us/research/project/z3-3/)

### 10. Red Flags to Watch

1. **Division by Zero**: Check all denominators
2. **Integer Overflow**: Verify wide math usage
3. **Rounding Exploitation**: Accumulation attacks
4. **Time Manipulation**: Clock drift/regression
5. **Precision Loss**: Decimal conversions
6. **Non-Monotonic Updates**: C decreasing
7. **Conservation Violation**: Creating tokens
8. **Negative Values**: Underflow to large numbers

### 11. Reporting Template

```markdown
# Mathematical Analysis Report

## Executive Summary
[High-level findings]

## Formulas Reviewed
- Formula 1: [Status]
- Formula 2: [Status]

## Invariants Verified
- Invariant 1: ✅ Proven
- Invariant 2: ⚠️ Conditional

## Error Bounds
- Rounding: [Analysis]
- Overflow: [Analysis]

## Recommendations
1. [Recommendation 1]
2. [Recommendation 2]

## Appendix
[Detailed proofs and calculations]
```

### 12. Emergency Procedures

If you discover a critical mathematical flaw:

1. **Calculate Impact**: Determine maximum loss/exploit value
2. **Document Precisely**: Create mathematical proof of issue
3. **Notify Lead**: Immediate escalation with severity assessment
4. **Propose Fix**: Suggest corrected formula/implementation
5. **Verify Fix**: Prove the fix maintains all invariants
6. **Test Thoroughly**: Generate comprehensive test cases

Remember: Mathematics is the foundation of protocol security. A single formula error can compromise the entire system. Be thorough, be precise, be skeptical.