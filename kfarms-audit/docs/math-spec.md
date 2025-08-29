# KFarms Mathematical Specification and Invariants

## Core Mathematical Model

### 1. Symbol Definitions

| Symbol | Description | Type | Units |
|--------|-------------|------|-------|
| `s_{u,i}` | User staked amount in pool i | u128 | Base units of token i |
| `f(lock)` | Lock duration multiplier | u128 | Dimensionless scalar ≥ 1 |
| `p_{u,i}` | User points in pool i | u128 | Points (s × f) |
| `P_i` | Total points in pool i | u128 | Points |
| `W_i` | Pool weight | u128 | Weight units |
| `W` | Total weight across all pools | u128 | Weight units |
| `E` | Global emission rate | u128 | Rewards per slot |
| `e_i` | Pool emission rate | u128 | Rewards per slot |
| `RPT_i` | Reward-per-point accumulator | u128 | Scaled fixed-point |
| `ΔRPT_i` | RPT increment per slot | u128 | Scaled fixed-point |
| `SCALE` | Fixed-point scaling factor | const | 10^18 |

### 2. Core Formulas

#### 2.1 Points Calculation
```
p_{u,i} = s_{u,i} × f(lock)
```
- **Overflow Protection**: Use u128 for multiplication
- **Bounds**: f(lock) ∈ [1, MAX_MULTIPLIER]

#### 2.2 Lock Multiplier Function
```
f(lock) = min(1 + (lock_duration / BASE_DURATION) × MULTIPLIER_RATE, MAX_MULTIPLIER)
```
- **MAX_MULTIPLIER**: Typically 2.5x to 4x
- **BASE_DURATION**: Reference period (e.g., 365 days)

#### 2.3 Pool Emission Allocation
```
e_i = E × W_i / W    (when W > 0)
e_i = 0              (when W = 0)
```

#### 2.4 Reward-Per-Point Update
```
ΔRPT_i = (e_i × Δslots × SCALE) / P_i    (when P_i > 0)
ΔRPT_i = 0                                (when P_i = 0)
RPT_i(t) = RPT_i(t-1) + ΔRPT_i
```

#### 2.5 User Earned Rewards
```
earned_{u,i} = (p_{u,i} × (RPT_i - paidRPT_{u,i})) / SCALE + accrued_{u,i}
```

### 3. Critical Invariants

#### 3.1 Conservation Invariants

**INV-1: Global Conservation**
```
∀t: Σ_i distributed_rewards_i(t) ≤ Σ reward_vault_funded_tokens(t)
```

**INV-2: Pool Conservation**
```
∀i,t: Σ_u earned_{u,i}(t) + dust_i(t) = total_distributed_i(t)
```

#### 3.2 Accounting Invariants

**INV-3: Points Consistency**
```
∀i,t: Σ_u p_{u,i}(t) = P_i(t)
```

**INV-4: Stake Consistency**
```
∀i,t: Σ_u s_{u,i}(t) ≤ vault_balance_i(t)
```

#### 3.3 Monotonicity Invariants

**INV-5: RPT Non-Decreasing**
```
∀i,t: RPT_i(t) ≥ RPT_i(t-1)
```

**INV-6: Earned Non-Decreasing (before claim)**
```
∀u,i,t: earned_{u,i}(t) ≥ earned_{u,i}(t-1) (if no claim between t-1 and t)
```

#### 3.4 Bounds Invariants

**INV-7: Overflow Prevention**
```
∀ operations: intermediate_value < 2^128
```

**INV-8: Time Bounds**
```
∀ update: Δslots ≤ MAX_SLOTS_PER_UPDATE (e.g., 432,000 = 1 epoch)
```

### 4. State Transition Rules

#### 4.1 Deposit Operation
```rust
fn deposit(user, pool, amount, lock_duration) {
    // 1. Settle pool
    update_pool_rpt(pool, current_slot);
    
    // 2. Settle user
    settle_user_rewards(user, pool);
    
    // 3. Update state
    user.staked[pool] += amount;
    user.lock_multiplier = compute_f(lock_duration);
    user.points[pool] = user.staked[pool] * user.lock_multiplier;
    pool.total_points += user.points[pool];
    
    // 4. Set checkpoint
    user.paid_rpt[pool] = pool.rpt;
}
```

#### 4.2 Withdraw Operation
```rust
fn withdraw(user, pool, amount) {
    // 1. Check lock expiry
    require!(current_slot >= user.lock_end_slot);
    
    // 2. Settle pool
    update_pool_rpt(pool, current_slot);
    
    // 3. Settle user
    settle_user_rewards(user, pool);
    
    // 4. Update state
    old_points = user.points[pool];
    user.staked[pool] -= amount;
    user.points[pool] = user.staked[pool] * user.lock_multiplier;
    pool.total_points -= (old_points - user.points[pool]);
    
    // 5. Transfer tokens
    transfer_from_vault(amount);
}
```

### 5. Overflow Analysis and Caps

#### 5.1 Maximum Values Table

| Parameter | Maximum Value | Justification |
|-----------|--------------|---------------|
| Token Supply | 10^10 × 10^9 | 10B tokens × 9 decimals |
| Max Stake/User | 10^9 × 10^9 | 1B tokens × 9 decimals |
| Max Multiplier | 4 | 4x max boost |
| Max Points | 4 × 10^18 | Max stake × multiplier |
| Emission Rate | 10^6 × 10^9 / slot | 1M tokens/slot |
| SCALE | 10^18 | Fixed-point precision |

#### 5.2 Worst-Case Multiplication
```
Worst case: points × RPT × emission × time
= (4 × 10^18) × (10^18) × (10^15) × (432,000)
= ~1.7 × 10^57
< 2^128 = ~3.4 × 10^38 ❌ OVERFLOW RISK

Solution: Use intermediate divisions or 256-bit math for this path
```

### 6. Rounding Policy

**Principle**: Always round against the protocol (in favor of users) for claims, round against users for deposits.

```rust
// Reward calculation - round down (against protocol)
earned = (points * delta_rpt) / SCALE;  // integer division rounds down

// Emission distribution - round down (conserves emissions)
pool_emission = (total_emission * pool_weight) / total_weight;

// Points calculation - round down (against user on deposit)
points = (staked * multiplier) / MULTIPLIER_SCALE;
```

### 7. Time Handling

```rust
const MAX_SLOTS_PER_UPDATE: u64 = 432_000; // 1 epoch

fn safe_time_delta(last_update: Slot, current: Slot) -> u64 {
    min(current.saturating_sub(last_update), MAX_SLOTS_PER_UPDATE)
}
```

### 8. Dust Management

```rust
struct PoolDust {
    accumulated: u128,
    threshold: u128,  // e.g., 1000 units
}

fn distribute_dust(pool: &mut Pool) {
    if pool.dust.accumulated >= pool.dust.threshold && pool.total_points > 0 {
        let dust_per_point = pool.dust.accumulated * SCALE / pool.total_points;
        pool.rpt += dust_per_point;
        pool.dust.accumulated = 0;
    }
}
```

### 9. External Points Integration

```rust
fn set_external_points(user, external_program, position_account, points) {
    // 1. Verify CPI origin
    require!(is_from_whitelisted_program(external_program));
    
    // 2. Verify position ownership and uniqueness
    let position = load_and_verify_position(position_account);
    require!(position.owner == user);
    require!(!is_position_already_counted(position.id));
    
    // 3. Verify custody proof
    require!(position.deposited_amount > 0);
    require!(position.status == Active);
    
    // 4. Update points with proof
    user.external_points[position.id] = points;
    mark_position_counted(position.id);
}
```

### 10. Governance Snapshot

```rust
struct Snapshot {
    slot: Slot,
    points: HashMap<Pubkey, u128>,
    finalized: bool,
}

fn create_proposal_snapshot(proposal_id: u32, snapshot_slot: Slot) {
    require!(snapshot_slot >= current_slot + MIN_SNAPSHOT_DELAY);
    
    let snapshot = Snapshot {
        slot: snapshot_slot,
        points: HashMap::new(),
        finalized: false,
    };
    
    proposals[proposal_id].snapshot = snapshot;
}

fn finalize_snapshot(proposal_id: u32) {
    require!(current_slot >= proposals[proposal_id].snapshot.slot);
    
    for user in all_users {
        proposals[proposal_id].snapshot.points[user] = user.total_points;
    }
    
    proposals[proposal_id].snapshot.finalized = true;
}
```