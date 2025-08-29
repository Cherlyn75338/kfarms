### External Points / Delegated Stake Threat Model

Risks
- Flash points capture; custody mismatch; desync ordering within one tx; replay/malleability

Controls
- Proof of custody binding:
  - CPI from provider to KFarms with accounts proving locked position and amounts; or
  - Snapshot signed by provider PDA (Merkle/SPL‑noop), with fresh slot, unique nonce; expiry and anti‑replay
- Smoothing / rate limits:
  - EMA/TWAP on delegated stake or external points; per-slot/epoch caps; one update per epoch
- Minimum horizon: increases only if remaining lock ≥ threshold
- Whitelist provider program IDs; verify PDA signers and seeds
- Idempotent order: always refresh pool/user accrual before applying new stake; slot monotonicity

Math condition
- For any user on [t0,t1]: allocated rewards ≤ ∫ R'(t)·P_u(t)/P_tot(t) dt + rounding bound; enforce instantaneous ΔP_u ≤ κ per epoch via smoothing

Implementation hooks in code
- Delegated farms use `set_stake` which refreshes global and user rewards before stake mutation.
- Add provider PDA checks and per-epoch limits around delegated setter at instruction level (future extension).