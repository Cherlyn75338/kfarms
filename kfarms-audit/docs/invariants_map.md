### Invariants to Instructions Mapping (Draft)

- deposit/extend_lock/withdraw
  - Pre: Settle pool RPT_i, then settle user
  - Post: Σ_u p_{u,i} == P_i; RPT_i monotonic; no under/overflow in u128
  - Dust: unchanged except via pool settle; not attributable to new points
  - Lock changes affect only future accrual (paidRPT_u := RPT_i after mutation)

- claim
  - Pre: Settle pool and user
  - Post: user.accrued reset; conservation holds against vault/mint

- update_pool_weight / set_global_emission
  - Pre: Settle all affected pools first
  - Post: New weights in effect for subsequent Δslot windows; no retroactive redistribution

- external points setter
  - Only via whitelisted CPI with sysvar::instructions proof
  - Proof-of-deposit required; unique position ID; revocation on external close
  - Snapshot consistency for governance

- governance snapshot/tally
  - Vote weight = snapshot points at configured slot
  - Immutability during vote window; bounded arithmetic for tallies

