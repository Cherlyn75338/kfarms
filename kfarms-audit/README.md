# KFarms Protocol Security Audit Framework

## 🎯 Objective

A rigorous, math-centric Solana/Rust audit strategy for KFarms to uncover bugs leading to:

- **Critical**: Governance manipulation, direct theft, permanent freezing, insolvency
- **High**: Theft/freezing of unclaimed yield or temporary freezing

Special focus on reward/points mathematics and formula implementation in functions.

## 🏗️ Architecture

```
kfarms-audit/
├── src/
│   ├── math/              # Mathematical specifications and invariants
│   │   ├── formulas.rs    # Core reward formulas
│   │   ├── invariants.rs  # Protocol invariants
│   │   └── bounds.rs      # Overflow/underflow bounds
│   ├── solana/            # Solana/Anchor validators
│   │   ├── account_validator.rs
│   │   ├── pda_checker.rs
│   │   └── token_flow.rs
│   ├── threats/           # Threat models
│   │   ├── external_points.rs
│   │   ├── governance.rs
│   │   └── flash_attacks.rs
│   └── tests/             # Test harnesses
│       ├── property_tests.rs
│       └── fuzz_tests.rs
├── scripts/               # Audit automation
├── docs/                  # Team playbooks
└── reports/              # Generated reports
```

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/kfarms-audit
cd kfarms-audit

# Install dependencies
cargo build --release

# Run full audit
./scripts/audit.sh ./path/to/kfarms/program
```

### Running Individual Checks

```bash
# Mathematical invariants
cargo run --release -- math --state protocol_state.json

# Solana analysis
cargo run --release -- solana --program ./programs --id <PROGRAM_ID>

# Threat detection
cargo run --release -- threats --history tx_history.json

# Property tests
cargo run --release -- proptest --cases 10000

# Generate report
cargo run --release -- report --format markdown
```

## 📊 Mathematical Specification

### Core Variables

- **A_u**: User staked amount
- **L_u**: Lock duration
- **M(L)**: Lock multiplier function
- **P_u = A_u · M(L) + P_ext**: Total user points
- **P_tot = Σ P_u**: Total pool points
- **R'(t)**: Emission rate (tokens/sec)
- **C(t)**: Cumulative reward-per-point

### Key Formulas

```rust
// Lock multiplier
M(L) = 1 + (L / L_max) * (M_max - 1)

// Cumulative reward per point update
C += (R'(t) * Δt * SCALE) / P_tot

// User earned rewards
earned_u = floor((P_u * (C - C_paid)) / SCALE) + accrued_u
```

### Critical Invariants

1. **Non-negativity**: All values ≥ 0
2. **Conservation**: Total distributed ≤ ∫ R'(t) dt
3. **Monotonicity**: C(t) non-decreasing
4. **Bounds**: Points ≤ MAX_POINTS
5. **No division by zero**: P_tot > 0 when R' > 0

## 🔍 Threat Model

### External Points Setter Risks

1. **Flash Points Attack**: Temporary inflation for reward capture
2. **Custody Mismatch**: Points without backing
3. **Sandwich Attacks**: Front/back-running reward updates
4. **TWAP Manipulation**: Governance weight gaming

### Mitigations

- Proof-of-custody binding via CPI
- TWAP/EMA smoothing (α = 10%)
- Per-slot/epoch change limits
- Minimum lock horizons
- Provider whitelisting

## 🧪 Testing Strategy

### Property-Based Testing

```rust
// Conservation property
prop_assert!(total_distributed <= emission_integral + rounding_error);

// Monotonicity property
prop_assert!(new_cumulative >= old_cumulative);

// Points consistency
prop_assert!(calculated_points == stored_points);
```

### Fuzz Testing Targets

- Instruction sequences
- Arithmetic operations
- Time-based transitions
- External points updates

### Differential Testing

Compare on-chain implementation against high-precision reference model.

## 👥 Team Roles

### Lead Auditor (You)
- Orchestrate scope and priorities
- Review critical findings
- Final sign-off

### Protocol Mathematician
- Derive/verify formulas
- Prove invariants
- Calculate error bounds

### Solana Engineer
- Anchor constraints
- PDA/seeds validation
- CPI safety

### Fuzz Analyst
- Property tests
- Fuzzing harnesses
- SMT verification

### Threat Modeler
- Game theory analysis
- MEV/timing attacks
- Governance manipulation

## 📈 Audit Phases

### Phase 1: Discovery (1-2 days)
- Collect artifacts
- Map critical paths
- Define state machine

### Phase 2: Math Specification (2-3 days)
- Formalize invariants
- Prove conservation
- Bound errors

### Phase 3: Code Analysis (3-4 days)
- Solana/Anchor review
- Pattern matching
- Static analysis

### Phase 4: Testing (2-3 days)
- Property tests
- Fuzz campaigns
- Integration tests

### Phase 5: Threat Analysis (2 days)
- External points
- Governance attacks
- Economic exploits

### Phase 6: Reporting (1 day)
- Findings summary
- PoC development
- Remediation plan

## 🛠️ Tools and Commands

### Grep Patterns for Quick Triage

```bash
# Math hotspots
rg "reward_per|points|multiplier|emission" -g "*.rs"

# Time handling
rg "Clock::get|unix_timestamp|slot" -g "*.rs"

# Type casting
rg "as u64|as u32|checked_|saturating_" -g "*.rs"

# External calls
rg "set_points|external|CPI" -g "*.rs"
```

### CI/CD Integration

```yaml
# Run in GitHub Actions
- name: KFarms Audit
  run: ./scripts/audit.sh ${{ github.workspace }}/programs
```

## 📋 Checklist

### Mathematical Safety
- [ ] Lock multiplier bounds verified
- [ ] Cumulative values monotonic
- [ ] Conservation law holds
- [ ] No division by zero
- [ ] Overflow protection

### Solana Security
- [ ] PDA derivation correct
- [ ] Account ownership validated
- [ ] Token decimals handled
- [ ] Compute budget safe
- [ ] No reentrancy

### External Points
- [ ] Custody proof required
- [ ] TWAP smoothing applied
- [ ] Change limits enforced
- [ ] Replay protection
- [ ] Provider whitelisted

### Governance
- [ ] Snapshot timing safe
- [ ] Vote weight stable
- [ ] No flash loans
- [ ] Timelock enforced

## 📊 Metrics

Track these metrics during audit:

- **Code Coverage**: > 90%
- **Property Tests**: > 10,000 cases
- **Fuzz Iterations**: > 1M
- **Invariant Violations**: 0 critical
- **Gas/Compute**: < limits

## 🚨 Emergency Response

If critical issue found:

1. **Immediate**: Notify team lead
2. **Document**: Create detailed PoC
3. **Verify**: Reproduce on testnet
4. **Mitigate**: Propose fix
5. **Test**: Verify remediation
6. **Deploy**: Coordinate upgrade

## 📚 References

- [Solana Security Best Practices](https://docs.solana.com/developing/programming-model/security)
- [Anchor Security](https://book.anchor-lang.com/anchor_in_depth/security.html)
- [Formal Verification in Rust](https://github.com/model-checking/kani)
- [Property-Based Testing](https://proptest-rs.github.io/proptest/)

## 📝 License

MIT

## 🤝 Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

## 📧 Contact

- Lead: security@kfarms.io
- Discord: [KFarms Security](https://discord.gg/kfarms)
- Bug Bounty: [Immunefi](https://immunefi.com/bounty/kfarms)