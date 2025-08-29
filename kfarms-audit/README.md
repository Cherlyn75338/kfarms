# KFarms Solana Staking/Rewards Program Audit

## Audit Objective
Comprehensive security audit of the KFarms staking/rewards program with emphasis on mathematical correctness, reward accounting, and critical vulnerability identification.

## Primary Impact Targets
- Fund theft
- Insolvency 
- Permanent/temporary freezing
- Vote/points manipulation
- Unclaimed yield theft/freezing

## Team Structure

| Role | Responsibility | Primary Focus |
|------|---------------|---------------|
| **Lead/Coach** | Orchestration, risk prioritization, escalation criteria, final sign-off | Overall coordination |
| **Math/Mechanism Lead** | Extract/validate formulas, define invariants, build reference model | Mathematical correctness |
| **State/Access Control Lead** | Account model, PDAs, signers, authorities, upgradeability, CPI boundaries | Authorization security |
| **Token/SPL Lead** | Custody, mint/decimals, transfer-fee/frozen extensions, vault correctness | Token handling |
| **Tooling/Fuzz Lead** | Invariant/property fuzzing, sequence fuzzing, dynamic testing harness | Automated testing |
| **Integration Lead** | External "points setter" protocol assumptions and proof-of-deposit checks | External interactions |
| **Reporting Lead** | PoCs, trace diagrams, exploit economics, severity/cap fixes | Documentation |

## Audit Phases

### Phase 1: Intake and Mapping
- [ ] Collect program addresses and deployment configs
- [ ] Asset inventory (mints, vaults, PDAs, authorities)
- [ ] Diagram all flows

### Phase 2: Threat Model and Scope
- [ ] Define actors and trust boundaries
- [ ] Identify critical assets
- [ ] Map attack surfaces

### Phase 3: Math-Spec Extraction (Deep Focus)
- [ ] Extract all formulas
- [ ] Define invariants
- [ ] Prove bounds and overflow safety

### Phase 4: Function-Level Analysis
- [ ] Map math to each instruction
- [ ] Identify misuse patterns
- [ ] Edge case analysis

### Phase 5: Attack Pattern Testing
- [ ] Implement reference model
- [ ] Invariant testing
- [ ] Sequence fuzzing
- [ ] PoC development

### Phase 6: Reporting
- [ ] Document findings
- [ ] Provide fixes
- [ ] Create regression tests

## Directory Structure
```
kfarms-audit/
├── docs/           # Specifications and documentation
├── analysis/       # Code analysis and findings
├── testing/        # Test suites and harnesses
├── pocs/          # Proof of concept exploits
├── reports/       # Final audit reports
├── tools/         # Audit tools and scripts
└── reference-model/ # Off-chain reference implementation
```

## Quick Start
1. Review the [Mathematical Specification](docs/math-spec.md)
2. Check the [Audit Checklist](docs/audit-checklist.md)
3. See role-specific guides in `docs/team-guides/`
4. Run tests with scripts in `testing/`