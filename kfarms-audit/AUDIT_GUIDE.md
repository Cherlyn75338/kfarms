# KFarms Audit Execution Guide

## 🎯 Audit Objective
Find and prove high/critical bugs in the KFarms Solana staking/rewards program with emphasis on mathematical correctness and reward accounting vulnerabilities.

## 🚀 Quick Start

### 1. Initial Setup
```bash
# Clone the KFarms repository (when available)
git clone <kfarms-repo-url> kfarms-source
cd kfarms-audit

# Install dependencies
pip3 install decimal
cargo install anchor-cli
```

### 2. Run Initial Analysis
```bash
# Run the reference model tests
python3 reference-model/reference_model.py

# Run quick fuzzing
python3 testing/fuzzer.py --quick

# Run comprehensive fuzzing (takes longer)
python3 testing/fuzzer.py --operations 10000 --seed 42
```

## 📋 Phase-by-Phase Execution

### Phase 1: Intake and Mapping (Day 1-2)

**Lead: State/Access Control Lead**

1. **Collect Program Information**
   ```bash
   # Get program address
   solana program show <PROGRAM_ID>
   
   # Download IDL if Anchor
   anchor idl fetch <PROGRAM_ID> -o idl.json
   
   # Get upgrade authority
   solana program show <PROGRAM_ID> | grep "ProgramData"
   ```

2. **Map Account Structure**
   - Use `docs/team-guides/state-access-lead-guide.md`
   - Document all PDAs in `analysis/account-model.md`
   - Create state diagram in `docs/diagrams/`

3. **Identify Critical Functions**
   - List all instructions
   - Map state transitions
   - Identify admin functions

### Phase 2: Mathematical Analysis (Day 2-4)

**Lead: Math/Mechanism Lead**

1. **Extract Formulas**
   - Follow `docs/team-guides/math-lead-guide.md`
   - Document in `analysis/formulas.md`
   - Use the mathematical spec in `docs/math-spec.md` as reference

2. **Build Reference Model**
   ```python
   # Extend reference_model.py with actual formulas
   cd reference-model
   python3 reference_model.py
   ```

3. **Verify Invariants**
   - Check each invariant from `docs/math-spec.md`
   - Run differential tests
   - Document violations

### Phase 3: Vulnerability Hunting (Day 4-7)

**All Team Members**

1. **Use the Audit Checklist**
   - Go through `docs/audit-checklist.md` systematically
   - Mark items as you verify them
   - Document findings immediately

2. **Check Known Patterns**
   - Review `docs/vulnerability-patterns.md`
   - Test each pattern against the codebase
   - Create PoCs for confirmed vulnerabilities

3. **Run Automated Tools**
   ```bash
   # Fuzzing with specific patterns
   python3 testing/fuzzer.py --operations 10000
   
   # Static analysis (if available)
   cargo audit
   soteria <program_path>
   ```

### Phase 4: Proof of Concept Development (Day 7-9)

**Lead: Tooling/Fuzz Lead**

1. **Create PoCs**
   - Use `pocs/poc_template.rs` as starting point
   - One PoC per vulnerability
   - Include clear exploitation steps

2. **Test PoCs**
   ```bash
   cd pocs
   cargo test --test <poc_name>
   ```

3. **Document Impact**
   - Calculate maximum extractable value
   - Identify affected users
   - Assess recovery difficulty

### Phase 5: Reporting (Day 9-10)

**Lead: Reporting Lead**

1. **Create Finding Reports**
   - Use `reports/finding_template.md`
   - One report per vulnerability
   - Include all sections

2. **Generate Executive Summary**
   - Total vulnerabilities found
   - Severity distribution
   - Key recommendations

3. **Prepare Presentation**
   - High-level findings
   - Critical issues requiring immediate attention
   - Remediation timeline

## 🔍 Key Areas to Focus On

### Mathematical Vulnerabilities (HIGHEST PRIORITY)
- [ ] Overflow in points calculation (`stake * multiplier`)
- [ ] Overflow in reward calculation (`points * emission * time`)
- [ ] Division by zero when `total_points = 0`
- [ ] Rounding exploitation in repeated operations
- [ ] Time-based calculation manipulation

### State Consistency
- [ ] Non-atomic state updates
- [ ] Incorrect operation ordering (settle → update → transfer)
- [ ] Missing settlement before state changes
- [ ] Reentrancy via CPI

### Access Control
- [ ] Missing signer checks
- [ ] Incorrect PDA validation
- [ ] Authority confusion
- [ ] Upgrade authority risks

### Integration Points
- [ ] External points setter validation
- [ ] Proof-of-deposit verification
- [ ] CPI origin validation
- [ ] Position uniqueness checks

## 🛠️ Testing Commands

### Reference Model Testing
```bash
# Run basic test
python3 reference-model/reference_model.py

# Run with custom scenario
python3 -c "
from reference_model import ReferenceModel, PoolConfig
model = ReferenceModel()
# Add your test scenario
"
```

### Fuzzing
```bash
# Quick test (100 operations)
python3 testing/fuzzer.py --quick

# Standard test (1000 operations)
python3 testing/fuzzer.py

# Comprehensive test (10000 operations)
python3 testing/fuzzer.py --operations 10000

# Reproducible test
python3 testing/fuzzer.py --seed 12345

# Stress test
python3 testing/fuzzer.py --users 100 --pools 20 --operations 50000
```

### Invariant Checking
```python
# In Python console
from reference_model import ReferenceModel
model = ReferenceModel()
# ... perform operations ...
violations = model.check_invariants()
print(violations)
```

## 📊 Severity Classification

| Severity | Impact | Example |
|----------|--------|---------|
| **CRITICAL** | Direct fund theft, complete protocol failure | Overflow leading to arbitrary minting |
| **HIGH** | Significant fund loss, major functionality broken | Incorrect reward calculation |
| **MEDIUM** | Limited fund loss, degraded functionality | Dust accumulation exploit |
| **LOW** | Minor issues, best practice violations | Missing event emission |

## 📝 Documentation Structure

```
kfarms-audit/
├── README.md                    # Overview and team structure
├── AUDIT_GUIDE.md              # This file
├── docs/
│   ├── math-spec.md            # Mathematical specification
│   ├── audit-checklist.md      # Comprehensive checklist
│   ├── vulnerability-patterns.md # Known attack patterns
│   └── team-guides/            # Role-specific guides
├── analysis/
│   ├── formulas.md             # Extracted formulas
│   ├── invariants.md           # Invariant analysis
│   ├── access-control.md       # Access control findings
│   └── findings.md             # All findings summary
├── testing/
│   ├── fuzzer.py               # Main fuzzing tool
│   └── test_results/           # Test outputs
├── pocs/
│   ├── poc_template.rs         # PoC template
│   └── [vulnerability_name].rs # Specific PoCs
├── reports/
│   ├── finding_template.md     # Finding report template
│   ├── fuzzing_report.json     # Automated fuzzing results
│   └── final_report.md         # Final audit report
└── reference-model/
    └── reference_model.py      # High-precision reference

```

## 🚨 Escalation Protocol

### Immediate Escalation Required
- Missing access control on admin functions
- Overflow vulnerabilities in core math
- Arbitrary minting capability
- Fund theft vectors

### High Priority
- Incorrect reward calculations
- State inconsistency issues
- Governance manipulation vectors

### Standard Priority
- Best practice violations
- Gas optimization opportunities
- Code quality issues

## 💡 Pro Tips

1. **Always Check Math First**: Most critical bugs are in mathematical calculations
2. **Test Edge Cases**: Zero amounts, maximum values, empty pools
3. **Follow the Money**: Trace every token transfer path
4. **Verify Invariants**: Check conservation laws after every operation
5. **Document Everything**: Even suspected issues that aren't confirmed

## 🔗 Resources

- [Solana Security Best Practices](https://docs.solana.com/developing/on-chain-programs/developing-rust#security-best-practices)
- [Anchor Security](https://book.anchor-lang.com/anchor_in_depth/security.html)
- [Neodyme's Solana Security Workshop](https://workshop.neodyme.io/)
- [Sec3's Soteria](https://github.com/sec3-product/soteria)

## 📞 Communication Channels

- **Daily Standup**: 10 AM UTC
- **Finding Discussion**: Slack #audit-findings
- **Escalation**: Direct message to Lead
- **Documentation**: GitHub PR to this repo

---

Remember: **Thoroughness over speed**. A single missed critical vulnerability can lead to total protocol failure. Take your time, verify everything twice, and document comprehensively.