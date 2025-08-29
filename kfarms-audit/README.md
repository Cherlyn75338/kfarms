## KFarms Audit Workspace

This repository scaffolds an end-to-end audit workspace for a Solana Rust staking/rewards program ("KFarms"). It includes:

- Python reference model of rewards/points accounting
- Deterministic and property-based tests for invariants
- PoC stubs for severity-oriented exploit probes
- Documentation of math, invariants, caps, and review checklists
- Optional Rust `solana-program-test` harness skeleton

### Quickstart

1) Setup Python deps
```
make setup
```

2) Run the unit tests
```
make test
```

3) Explore PoC skeletons (skipped by default)
```
pytest -q pocs -k "not slow" -q
```

### Structure

- `model/`: Python reference model implementing pool and user accounting using SCALE=1e18 fixed-point accumulators and u128 headroom checks
- `tests/`: Deterministic and property tests asserting core invariants and correctness
- `pocs/`: Skipped PoC tests demonstrating exploit shapes to be filled against the target program
- `docs/`: Mechanism spec, invariants mapping, caps table, and review checklists
- `harness/rust/`: Skeleton for a Rust `solana-program-test` integration harness (not built by default)

### Roles and workflow

- Lead/Coach: prioritize findings, sign-off; integrate test coverage and reporting
- Math/Mechanism Lead: own `docs/mechanism.md`, `docs/caps_table.md`, and the `model/`
- State/Access Control Lead: review `docs/checklists.md` and harness scaffolding
- Token/SPL Lead: custody and mint checks in `docs/checklists.md`
- Tooling/Fuzz Lead: expand property tests in `tests/` and PoCs under `pocs/`
- Integration Lead: add external points-setter proofs and tests
- Reporting Lead: produce PoC writeups and final report

### Notes

- Python integers are unbounded; we enforce u128 caps explicitly in the model for realism
- Division uses truncation toward zero; dust is tracked per-pool in a reservoir and not attributable to fresh stakers
- Time base is `slot`; updates clamp `Δslot` to a configured maximum to mitigate time-warp overpay

