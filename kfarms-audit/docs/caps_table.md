### Caps and u128 Headroom (Draft)

Parameters (example, adjust to deployment):

- Max stake per user per pool: S_user_max = 1e20 (base units)
- Max total stake per pool: S_pool_max = 1e22
- Max lock multiplier: f_max = 10x → MULT_MAX = 10e6
- SCALE = 1e18; MULTIPLIER_SCALE = 1e6
- Max global emission per slot: E_max = 1e18
- Max Δslot per instruction: 8192
- Max pools: 64; Max total weight: 1e9

Critical products:

- Points: S_pool_max × MULT_MAX ≈ 1e22 × 1e7 = 1e29 → after /1e6 = 1e23 (fits u128? 2^128≈3.4e38 → OK)
- RPT increment numerator: (E_max × Δslot × SCALE) ≈ 1e18 × 8e3 × 1e18 = 8e39 (may exceed u128 before division). Mitigation: compute as ((E×W_i/W) × Δslot) first, then multiply by SCALE, and rely on division by P_i ≥ 1 to keep drpt within u128; if P_i can be tiny, cap E_max or require minimum P_i before accrual.
- Accrued increment: points × delta_rpt / SCALE: worst-case points 1e23, delta_rpt up to ~1e18 → product 1e41; division by 1e18 → 1e23 (fits u128)

Guidelines:

- Use checked u128 for all state fields; avoid intermediate blow-up by ordering operations and dividing before multiplying by SCALE when safe.
- Enforce caps on E, Δslot, and minimum P_i to ensure numerator fits u256 in off-chain model but u128 after division.

