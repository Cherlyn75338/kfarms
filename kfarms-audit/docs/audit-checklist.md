# KFarms Audit Checklist

## 1. Mathematical Correctness Checks

### Overflow/Underflow Prevention
- [ ] All multiplication uses u128 intermediate values
- [ ] Division by zero checks in place for P_i, W, denominators
- [ ] Checked math operations throughout (checked_add, checked_mul, etc.)
- [ ] Maximum value caps defined and enforced
- [ ] SCALE factor prevents precision loss without overflow

### Rounding and Precision
- [ ] Consistent rounding direction (truncation) documented
- [ ] Dust accumulation tracked and distributed fairly
- [ ] No rounding-based value extraction attacks
- [ ] Fixed-point arithmetic maintains sufficient precision
- [ ] Integer division ordering optimized (multiply before divide)

### Time-based Calculations
- [ ] Slot-based time (not unix timestamp) for determinism
- [ ] Maximum time delta per update enforced
- [ ] Monotonic time progression guaranteed
- [ ] No negative time deltas possible
- [ ] Retroactive changes prevented

## 2. State Transition Vulnerabilities

### Deposit/Stake Flow
- [ ] Pool RPT updated before user state changes
- [ ] User rewards settled before points modification
- [ ] Points calculation cannot overflow
- [ ] Lock multiplier bounded and validated
- [ ] Minimum deposit enforced if applicable
- [ ] Token transfer happens after state update
- [ ] Reentrancy protection in place

### Withdraw/Unstake Flow
- [ ] Lock expiry checked before withdrawal
- [ ] Rewards settled before reducing points
- [ ] Sufficient balance check
- [ ] Points and total points updated atomically
- [ ] Early withdrawal penalties applied correctly
- [ ] Emergency withdrawal path secure
- [ ] Cannot withdraw more than deposited

### Claim Rewards Flow
- [ ] Current RPT used for calculation
- [ ] User's paid_RPT updated after claim
- [ ] Vault balance sufficient for claim
- [ ] Transfer uses transfer_checked with decimals
- [ ] Cannot claim same rewards twice
- [ ] Accrued rewards reset after claim
- [ ] No reentrancy via CPI during claim

### Lock Extension/Modification
- [ ] Rewards settled before lock change
- [ ] New multiplier calculated correctly
- [ ] Points recalculated with new multiplier
- [ ] Total points updated with delta
- [ ] Cannot reduce lock duration
- [ ] Maximum lock duration enforced
- [ ] Lock end slot calculated correctly

## 3. Pool and Emission Management

### Pool Updates
- [ ] RPT settled with old parameters first
- [ ] Weight changes don't affect past rewards
- [ ] Pool addition doesn't dilute existing rewards
- [ ] Pool removal handles remaining stakers
- [ ] Zero weight pools don't accumulate rewards
- [ ] Pool parameters validated on update

### Emission Rate Changes
- [ ] Current period settled before rate change
- [ ] No retroactive reward changes
- [ ] Rate bounds enforced (min/max)
- [ ] Smooth transition without gaps
- [ ] Total emission cap tracked

### Weight Distribution
- [ ] Sum of weights calculated correctly
- [ ] Division by zero when W=0 handled
- [ ] Weight changes atomic
- [ ] No weight manipulation attacks
- [ ] Weight overflow prevented

## 4. Access Control and Authorization

### Account Validation
- [ ] PDA seeds verified for all accounts
- [ ] Correct program ownership checks
- [ ] Account discriminators validated (Anchor)
- [ ] Signer requirements enforced
- [ ] Authority checks for admin functions

### Upgrade Authority
- [ ] Program upgrade authority documented
- [ ] Upgrade process has timelock/multisig
- [ ] Data migration paths considered
- [ ] Freeze authority implemented if needed

### Role-based Permissions
- [ ] Admin roles clearly defined
- [ ] Privilege escalation prevented
- [ ] Emergency pause mechanism secure
- [ ] Role transfer process secure
- [ ] No unauthorized mint/burn capability

## 5. Token and SPL Integration

### Token Custody
- [ ] Vault PDA controls token accounts
- [ ] No external transfer authority
- [ ] Rent-exempt accounts
- [ ] Token account ownership validated
- [ ] Associated token accounts used correctly

### Transfer Safety
- [ ] transfer_checked used with decimals
- [ ] Amount validation before transfer
- [ ] Slippage protection if applicable
- [ ] Fee-on-transfer tokens handled/rejected
- [ ] Frozen token accounts handled

### Mint Authority
- [ ] Mint authority is PDA only
- [ ] No external mint capability
- [ ] Supply caps enforced if applicable
- [ ] Decimal places handled correctly
- [ ] Token extensions compatibility checked

## 6. External Integration Points

### CPI Security
- [ ] Whitelisted programs only
- [ ] Sysvar::instructions parsing for verification
- [ ] No arbitrary CPI calls
- [ ] Return data validated
- [ ] Reentrancy guards in place

### Points Setter Protocol
- [ ] Proof-of-deposit required
- [ ] Position uniqueness enforced
- [ ] Position ownership verified
- [ ] Revocation mechanism implemented
- [ ] Double-counting prevented
- [ ] Lifetime bounds on external points

### Oracle/Price Feed Integration
- [ ] Oracle staleness checks
- [ ] Price manipulation resistance
- [ ] Fallback mechanisms
- [ ] Multiple oracle aggregation if used
- [ ] Decimal conversion handled

## 7. Governance and Voting

### Snapshot Mechanism
- [ ] Immutable snapshots at proposal creation
- [ ] Snapshot slot in future (no same-block)
- [ ] Points frozen for voting period
- [ ] No post-snapshot manipulation
- [ ] Deterministic snapshot timing

### Vote Tallying
- [ ] Overflow prevention in tallies
- [ ] Quorum calculations correct
- [ ] Tie-breaking deterministic
- [ ] Vote weight = snapshot points only
- [ ] No double voting

### Proposal Execution
- [ ] Timelock before execution
- [ ] Execution authorization checked
- [ ] State changes atomic
- [ ] Proposal expiry implemented
- [ ] Cancellation mechanism secure

## 8. Attack Patterns to Test

### Economic Attacks
- [ ] Sandwich attacks around updates
- [ ] Front-running pool updates
- [ ] Back-running emissions
- [ ] Dust siphoning via rounding
- [ ] Flash loan attacks (if applicable)
- [ ] MEV extraction opportunities

### State Manipulation
- [ ] Points amplification overflow
- [ ] Retroactive multiplier abuse
- [ ] Zero-staker pool capture
- [ ] Time manipulation exploits
- [ ] Snapshot timing attacks
- [ ] Governance weight manipulation

### Denial of Service
- [ ] Vault drainage causing freezing
- [ ] Computational DOS via loops
- [ ] State bloat attacks
- [ ] Griefing via dust deposits
- [ ] Lock mechanism abuse

### Integration Exploits
- [ ] Fake proof-of-deposit
- [ ] Double-counting positions
- [ ] CPI origin spoofing
- [ ] External program compromise impact
- [ ] Cross-program reentrancy

## 9. Testing Requirements

### Unit Tests
- [ ] Each instruction tested independently
- [ ] Edge cases covered
- [ ] Error conditions tested
- [ ] Boundary values tested

### Integration Tests
- [ ] Multi-instruction sequences
- [ ] State consistency across operations
- [ ] Time-based scenarios
- [ ] Multi-user interactions

### Invariant Tests
- [ ] Conservation laws hold
- [ ] Monotonicity maintained
- [ ] Bounds respected
- [ ] Accounting consistency

### Fuzzing
- [ ] Random instruction sequences
- [ ] Property-based testing
- [ ] Differential testing vs reference
- [ ] Adversarial input generation

### Performance Tests
- [ ] Gas optimization verified
- [ ] Compute unit usage measured
- [ ] Account size limits respected
- [ ] Transaction size limits checked

## 10. Documentation and Reporting

### Code Documentation
- [ ] All functions documented
- [ ] Invariants documented in code
- [ ] Security assumptions explicit
- [ ] Error codes meaningful

### Deployment Documentation
- [ ] Deployment parameters documented
- [ ] Authority keys documented
- [ ] Upgrade process documented
- [ ] Emergency procedures defined

### Finding Classification
- [ ] Severity levels defined (Critical/High/Medium/Low)
- [ ] Impact assessment completed
- [ ] Likelihood evaluation done
- [ ] Remediation provided
- [ ] Regression tests created