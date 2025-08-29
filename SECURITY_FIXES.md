# Security Fixes Implementation Guide

## Critical Fix 1: Division by Zero Protection

### Current Vulnerable Code
```rust
// farm_operations.rs:865-869
let added_reward_per_share = if farm_state.is_delegated() {
    Decimal::from(rewards) / farm_state.total_active_stake_scaled
} else {
    Decimal::from(rewards) / farm_state.get_total_active_stake_decimal()
};
```

### Fixed Implementation
```rust
// farm_operations.rs:865-869
let added_reward_per_share = if farm_state.is_delegated() {
    // Check for zero stake before division
    if farm_state.total_active_stake_scaled == 0 {
        Decimal::zero()
    } else {
        Decimal::from(rewards) / farm_state.total_active_stake_scaled
    }
} else {
    // Already has zero check at line 777
    Decimal::from(rewards) / farm_state.get_total_active_stake_decimal()
};
```

---

## Critical Fix 2: Delegated Stake Rate Limiting

### Add to state.rs
```rust
// Add to UserState struct
pub struct UserState {
    // ... existing fields ...
    
    // Add these new fields:
    pub last_delegated_update_ts: u64,
    pub last_delegated_stake_amount: u64,
    pub delegated_update_count: u64,
    
    // Reduce padding accordingly
    pub _padding_1: [u64; 47], // Was 50
}
```

### Update handler_set_stake_delegated.rs
```rust
use crate::utils::consts::DELEGATED_STAKE_RATE_LIMIT;

pub fn process(ctx: Context<SetStakeDelegated>, new_stake: u64) -> Result<()> {
    check_remaining_accounts(&ctx)?;
    
    let farm_state = &mut ctx.accounts.farm_state.load_mut()?;
    let user_state = &mut ctx.accounts.user_state.load_mut()?;
    let time_unit = farm_state.time_unit;
    let current_ts = TimeUnit::now_from_clock(time_unit, &Clock::get()?);
    
    // Existing checks
    require!(farm_state.is_delegated(), FarmError::FarmNotDelegated);
    require!(
        farm_state.delegate_authority == ctx.accounts.delegate_authority.key()
            || farm_state.second_delegated_authority == ctx.accounts.delegate_authority.key(),
        FarmError::AuthorityFarmDelegateMissmatch
    );
    
    // NEW: Rate limiting
    const MIN_UPDATE_INTERVAL: u64 = 3600; // 1 hour
    require!(
        current_ts >= user_state.last_delegated_update_ts + MIN_UPDATE_INTERVAL,
        FarmError::DelegatedUpdateTooFrequent
    );
    
    // NEW: Bounds checking
    let current_stake = user_state.active_stake_scaled as u64;
    const MAX_CHANGE_BPS: u64 = 2000; // 20% max change
    
    if current_stake > 0 {
        let max_increase = current_stake + (current_stake * MAX_CHANGE_BPS / 10000);
        let min_decrease = current_stake.saturating_sub(current_stake * MAX_CHANGE_BPS / 10000);
        
        require!(
            new_stake <= max_increase && new_stake >= min_decrease,
            FarmError::DelegatedStakeChangeTooLarge
        );
    }
    
    // NEW: Update tracking fields
    user_state.last_delegated_update_ts = current_ts;
    user_state.last_delegated_stake_amount = new_stake;
    user_state.delegated_update_count += 1;
    
    // Existing stake update
    farm_operations::set_stake(
        farm_state,
        user_state,
        new_stake,
        current_ts,
    )?;
    
    // NEW: Emit event for monitoring
    emit!(DelegatedStakeUpdate {
        user: ctx.accounts.user_state.key(),
        farm: ctx.accounts.farm_state.key(),
        delegate: ctx.accounts.delegate_authority.key(),
        old_stake: current_stake,
        new_stake,
        timestamp: current_ts,
        update_count: user_state.delegated_update_count,
    });
    
    Ok(())
}

// Add event definition
#[event]
pub struct DelegatedStakeUpdate {
    pub user: Pubkey,
    pub farm: Pubkey,
    pub delegate: Pubkey,
    pub old_stake: u64,
    pub new_stake: u64,
    pub timestamp: u64,
    pub update_count: u64,
}
```

---

## Critical Fix 3: Oracle Price Validation

### Add to state.rs
```rust
// Add to FarmState
pub struct FarmState {
    // ... existing fields ...
    
    // Add these fields:
    pub last_oracle_price: u64,
    pub last_oracle_price_ts: u64,
    pub oracle_price_max_change_bps: u64,
    pub oracle_twap_window: u64,
    
    // Reduce padding
    pub _padding: [u64; 70], // Was 74
}
```

### Update farm_operations.rs
```rust
// In refresh_global_reward function, update oracle price validation:

let oracle_adjusted_amt = if farm_state.scope_oracle_price_id == u64::MAX {
    decimal_adjusted_amt
} else {
    let price = scope_price.ok_or(FarmError::MissingScopePrices)?;
    
    // NEW: Age validation (existing)
    if ts - price.unix_timestamp > farm_state.scope_oracle_max_age {
        return Err(FarmError::ScopeOraclePriceTooOld.into());
    }
    
    // NEW: Price sanity check
    let current_price = price.price.value as u64;
    
    if farm_state.last_oracle_price > 0 {
        // Check price change is within bounds
        let price_change_bps = if current_price > farm_state.last_oracle_price {
            ((current_price - farm_state.last_oracle_price) * 10000) / farm_state.last_oracle_price
        } else {
            ((farm_state.last_oracle_price - current_price) * 10000) / farm_state.last_oracle_price
        };
        
        const DEFAULT_MAX_CHANGE_BPS: u64 = 2000; // 20%
        let max_change = if farm_state.oracle_price_max_change_bps > 0 {
            farm_state.oracle_price_max_change_bps
        } else {
            DEFAULT_MAX_CHANGE_BPS
        };
        
        require!(
            price_change_bps <= max_change,
            FarmError::OraclePriceChangeToLarge
        );
    }
    
    // NEW: Update last known price
    farm_state.last_oracle_price = current_price;
    farm_state.last_oracle_price_ts = ts;
    
    // Calculate with validated price
    let px = current_price as u128;
    let factor = ten_pow(price.price.exp as usize) as u128;
    decimal_adjusted_amt * px / factor
};
```

---

## Critical Fix 4: Overflow Protection

### Update Cargo.toml
```toml
[profile.release]
overflow-checks = true
lto = "fat"
codegen-units = 1

[profile.release.package.farms]
overflow-checks = true
```

### Update all arithmetic operations
```rust
// Replace all unchecked operations with checked variants

// BEFORE:
let result = a + b;

// AFTER:
let result = a.checked_add(b)
    .ok_or_else(|| FarmError::IntegerOverflow)?;

// BEFORE:
let current_stake_amount: u64 = user_state
    .active_stake_scaled
    .try_into()
    .expect("Delegated farm: active stake don't fit on u64");

// AFTER:
let current_stake_amount: u64 = user_state
    .active_stake_scaled
    .try_into()
    .map_err(|_| FarmError::IntegerOverflow)?;
```

---

## Critical Fix 5: Reentrancy Protection

### Add to state.rs
```rust
// Add to FarmState
pub struct FarmState {
    // ... existing fields ...
    
    // Add reentrancy guard
    pub is_locked: u8,
    
    // Adjust padding
    pub _padding: [u64; 69],
}
```

### Create reentrancy guard macro
```rust
// In utils/macros.rs
#[macro_export]
macro_rules! with_reentrancy_guard {
    ($farm_state:expr, $body:block) => {{
        // Check if locked
        require!($farm_state.is_locked == 0, FarmError::Reentrancy);
        
        // Lock
        $farm_state.is_locked = 1;
        
        // Execute body
        let result = $body;
        
        // Unlock
        $farm_state.is_locked = 0;
        
        result
    }};
}
```

### Apply to all state-changing operations
```rust
// Example in handler_harvest_reward.rs
pub fn process(ctx: Context<HarvestReward>, reward_index: u64) -> Result<()> {
    let farm_state = &mut ctx.accounts.farm_state.load_mut()?;
    
    with_reentrancy_guard!(farm_state, {
        // Existing harvest logic
        let effects = farm_operations::harvest(
            farm_state,
            user_state,
            &global_config,
            scope_price,
            reward_index as usize,
            current_ts,
        )?;
        
        // Transfer tokens
        // ...
        
        Ok(())
    })
}
```

---

## Fix 6: Reward Conservation During Zero Stake Periods

### Update farm_operations.rs
```rust
// In refresh_global_reward function

if farm_state.total_active_stake_scaled == 0 {
    // NEW: Don't advance time, accumulate rewards for later
    // Instead of updating last_issuance_ts, keep it unchanged
    // This ensures rewards aren't lost
    
    // Optional: Track accumulated rewards during zero-stake period
    farm_state.reward_infos[reward_index].pending_distribution += 
        reward_info.reward_schedule_curve
            .get_cumulative_amount_issued_since_last_ts(
                reward_info.last_issuance_ts, 
                ts
            )?;
    
    return Ok(());
}

// When stake becomes non-zero, distribute accumulated rewards
if farm_state.reward_infos[reward_index].pending_distribution > 0 {
    let pending = farm_state.reward_infos[reward_index].pending_distribution;
    farm_state.reward_infos[reward_index].pending_distribution = 0;
    
    // Add pending to current rewards
    amount = amount.checked_add(pending)
        .ok_or_else(|| FarmError::IntegerOverflow)?;
}
```

---

## Fix 7: Dust Collection Mechanism

### Add dust collection instruction
```rust
// New handler: handler_collect_dust.rs
pub fn process(ctx: Context<CollectDust>, reward_index: u64) -> Result<()> {
    let farm_state = &mut ctx.accounts.farm_state.load_mut()?;
    
    // Only admin can collect dust
    require!(
        ctx.accounts.authority.key() == farm_state.farm_admin,
        FarmError::Unauthorized
    );
    
    // Calculate dust (rewards that can't be distributed due to rounding)
    let reward_info = &farm_state.reward_infos[reward_index as usize];
    let dust_threshold = 100; // Minimum dust to collect
    
    // If rewards are below threshold and no active stakes
    if reward_info.rewards_available < dust_threshold 
        && farm_state.total_active_stake_scaled == 0 {
        
        let dust_amount = reward_info.rewards_available;
        farm_state.reward_infos[reward_index as usize].rewards_available = 0;
        
        // Transfer dust to treasury
        // ...
        
        emit!(DustCollected {
            farm: ctx.accounts.farm_state.key(),
            reward_index,
            amount: dust_amount,
        });
    }
    
    Ok(())
}
```

---

## Fix 8: Emergency Pause Mechanism

### Add to state.rs
```rust
pub struct FarmState {
    // ... existing fields ...
    
    // Add pause flag
    pub is_paused: u8,
    pub pause_authority: Pubkey,
    
    // Adjust padding
    pub _padding: [u64; 68],
}
```

### Add pause check macro
```rust
// In utils/macros.rs
#[macro_export]
macro_rules! require_not_paused {
    ($farm_state:expr) => {
        require!($farm_state.is_paused == 0, FarmError::FarmPaused);
    };
}
```

### Apply to all user-facing operations
```rust
// Example in handler_stake.rs
pub fn process(ctx: Context<Stake>, amount: u64) -> Result<()> {
    let farm_state = &mut ctx.accounts.farm_state.load_mut()?;
    
    // Check pause status
    require_not_paused!(farm_state);
    
    // Continue with normal logic
    // ...
}
```

---

## Testing Implementation

### Add comprehensive test suite
```typescript
// tests/security.ts
import * as anchor from "@project-serum/anchor";
import { assert } from "chai";

describe("Security Tests", () => {
  describe("Division by Zero Protection", () => {
    it("should handle zero stake in delegated farm", async () => {
      // Test implementation
    });
  });
  
  describe("Rate Limiting", () => {
    it("should prevent rapid delegated updates", async () => {
      // Test implementation
    });
  });
  
  describe("Oracle Validation", () => {
    it("should reject extreme price changes", async () => {
      // Test implementation
    });
  });
  
  describe("Overflow Protection", () => {
    it("should handle maximum values safely", async () => {
      // Test implementation
    });
  });
  
  describe("Reentrancy", () => {
    it("should prevent reentrant calls", async () => {
      // Test implementation
    });
  });
});
```

---

## Deployment Checklist

### Pre-deployment
- [ ] All fixes implemented and tested
- [ ] Formal verification completed
- [ ] Fuzzing completed (minimum 1M iterations)
- [ ] Gas optimization verified
- [ ] Emergency procedures documented

### Deployment
- [ ] Deploy to devnet first
- [ ] Run integration tests on devnet
- [ ] Deploy to testnet with limits
- [ ] Monitor for 1 week minimum
- [ ] Deploy to mainnet with conservative limits

### Post-deployment
- [ ] Monitor all events
- [ ] Set up alerts for anomalies
- [ ] Regular security reviews
- [ ] Bug bounty program active

---

## Error Codes to Add

```rust
// Add to FarmError enum
pub enum FarmError {
    // ... existing errors ...
    
    #[msg("Reentrancy detected")]
    Reentrancy,
    
    #[msg("Delegated stake update too frequent")]
    DelegatedUpdateTooFrequent,
    
    #[msg("Delegated stake change too large")]
    DelegatedStakeChangeTooLarge,
    
    #[msg("Oracle price change too large")]
    OraclePriceChangeToLarge,
    
    #[msg("Farm is paused")]
    FarmPaused,
    
    #[msg("Unauthorized")]
    Unauthorized,
}
```

---

*End of Security Fixes Document*