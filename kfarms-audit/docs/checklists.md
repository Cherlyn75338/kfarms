### Static Review and Solana-Specific Checks

- Anchor constraints: `has_one`, seeds, bumps, rent, reinit guards
- Signers/authorities: only PDAs; verify `invoke_signed` with correct seeds
- Upgradeability: owner and freeze switch; if immutable, confirm
- Account lifecycle: cannot close with pending rewards/balances
- Reentrancy/CPI: avoid state reuse and guard assumptions
- Token extensions: reject transfer-fee/interest-bearing/confidential unless supported

### Governance Manipulation Defenses

- Slot-based snapshots; immutable after start
- Weight rounding deterministic; store numerator/denominator if needed
- Prevent flash-points: min lock or early snapshots

### External Points Integration

- Whitelisted program enforced via sysvar::instructions parsing
- On-chain proof-of-deposit: owner, mint, balance, and uniqueness
- Revocation when external position closes or withdraws

