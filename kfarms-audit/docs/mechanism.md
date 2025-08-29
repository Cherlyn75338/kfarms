### KFarms Mechanism and Math Spec (Draft)

Definitions (units):

- s_{u,i}: user stake in base units of token i
- f(lock): dimensionless lock multiplier ≥ 1, capped; scaled by MULTIPLIER_SCALE=1e6
- p_{u,i} = floor(s_{u,i} × f(lock) / MULTIPLIER_SCALE)
- P_i = Σ_u p_{u,i}
- W_i: pool weight; W = Σ_i W_i
- E: global emission per slot
- e_i = floor(E × W_i / W)
- RPT_i: reward-per-point accumulator scaled by SCALE=1e18
- ΔRPT_i = floor((e_i × Δslot × SCALE + dust_i) / P_i), when P_i>0; else 0
- dust_i: per-pool dust reservoir (remainder), carried forward, not claimable by fresh stakers
- earned_{u,i} = floor(p_{u,i} × (RPT_i - paidRPT_{u,i}) / SCALE) + accrued_{u,i}

Invariants:

- RPT_i monotonic non-decreasing
- Σ_u p_{u,i} == P_i (after each instruction)
- Bounds: all intermediate products fit u128; SCALE large enough to avoid precision loss
- Conservation: distributed ≤ funded+minted, dust tracked per-pool

Ordering per instruction:

1. Settle pool using current slot with clamp on Δslot ≤ MAX_SLOTS_PER_UPDATE
2. Settle user accrued
3. Mutate stake/points/weights
4. Align paidRPT to pool RPT

Rounding policy:

- Use truncation toward zero for division
- Maintain `dust_reservoir` as numerator remainder to preserve conservation
- Never allow new stakers to capture historical dust

Time base:

- Use slot; clamp Δslot per settle to mitigate time warp

