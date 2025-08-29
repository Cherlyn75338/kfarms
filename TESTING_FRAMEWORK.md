# Comprehensive Testing Framework for Kamino Farms

## 1. Unit Testing Suite

### Core Mathematical Invariants Tests
```rust
// tests/unit/math_invariants.rs
use farms::farm_operations;
use proptest::prelude::*;

#[cfg(test)]
mod math_invariant_tests {
    use super::*;
    
    #[test]
    fn test_reward_per_share_monotonicity() {
        let mut farm = create_test_farm();
        let initial_rps = farm.reward_infos[0].reward_per_share_scaled;
        
        // Add rewards and refresh
        add_rewards(&mut farm, 1000);
        refresh_global_rewards(&mut farm, current_time());
        
        let new_rps = farm.reward_infos[0].reward_per_share_scaled;
        assert!(new_rps >= initial_rps, "RPS must be monotonically increasing");
    }
    
    #[test]
    fn test_reward_conservation() {
        let mut farm = create_test_farm();
        let mut users = create_test_users(10);
        
        // Track all rewards
        let total_added = 10000u64;
        add_rewards(&mut farm, total_added);
        
        // Distribute to users
        let mut total_claimed = 0u64;
        for user in &mut users {
            stake(&mut farm, user, 100);
        }
        
        advance_time(3600);
        refresh_global_rewards(&mut farm, current_time());
        
        for user in &mut users {
            let claimed = harvest_reward(&mut farm, user, 0);
            total_claimed += claimed;
        }
        
        // Conservation check
        assert!(
            total_claimed <= total_added,
            "Cannot distribute more than added"
        );
        
        // Check remaining
        let remaining = farm.reward_infos[0].rewards_available;
        assert_eq!(
            total_claimed + remaining,
            total_added,
            "Rewards must be conserved"
        );
    }
    
    #[test]
    fn test_no_negative_rewards() {
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Stake and immediately unstake
        stake(&mut farm, &mut user, 1000);
        unstake(&mut farm, &mut user, 1000);
        
        // Try to harvest
        let reward = harvest_reward(&mut farm, &mut user, 0);
        assert!(reward >= 0, "Rewards cannot be negative");
    }
}
```

### Delegated Stake Security Tests
```rust
// tests/unit/delegated_security.rs

#[test]
fn test_delegated_rate_limiting() {
    let mut farm = create_delegated_farm();
    let mut user = create_test_user();
    
    // First update should succeed
    set_stake_delegated(&mut farm, &mut user, 100, current_time());
    
    // Immediate second update should fail
    let result = set_stake_delegated(&mut farm, &mut user, 200, current_time());
    assert_eq!(
        result.unwrap_err(),
        FarmError::DelegatedUpdateTooFrequent
    );
    
    // After time passes, should succeed
    advance_time(3601);
    set_stake_delegated(&mut farm, &mut user, 200, current_time() + 3601);
}

#[test]
fn test_delegated_bounds_checking() {
    let mut farm = create_delegated_farm();
    let mut user = create_test_user();
    
    // Set initial stake
    set_stake_delegated(&mut farm, &mut user, 1000, current_time());
    advance_time(3601);
    
    // Try to increase by more than 20%
    let result = set_stake_delegated(&mut farm, &mut user, 1300, current_time());
    assert_eq!(
        result.unwrap_err(),
        FarmError::DelegatedStakeChangeTooLarge
    );
    
    // 20% increase should work
    set_stake_delegated(&mut farm, &mut user, 1200, current_time());
}

#[test]
fn test_zero_stake_division() {
    let mut farm = create_delegated_farm();
    
    // Ensure no stakes
    assert_eq!(farm.total_active_stake_scaled, 0);
    
    // Add rewards
    add_rewards(&mut farm, 1000);
    
    // This should not panic
    let result = refresh_global_rewards(&mut farm, current_time());
    assert!(result.is_ok(), "Should handle zero stake gracefully");
}
```

---

## 2. Integration Testing Suite

### Multi-User Scenario Tests
```typescript
// tests/integration/multi_user.ts
import { Program } from "@project-serum/anchor";
import { Keypair, PublicKey } from "@solana/web3.js";
import { assert } from "chai";

describe("Multi-User Integration Tests", () => {
  let program: Program;
  let farm: PublicKey;
  let users: Keypair[] = [];
  
  before(async () => {
    // Setup
    program = await initializeProgram();
    farm = await createFarm();
    
    // Create 100 users
    for (let i = 0; i < 100; i++) {
      users.push(Keypair.generate());
      await initializeUser(users[i]);
    }
  });
  
  it("handles concurrent operations correctly", async () => {
    // All users stake simultaneously
    const stakePromises = users.map(user => 
      stake(user, 100 + Math.random() * 900)
    );
    await Promise.all(stakePromises);
    
    // Verify total stake
    const farmState = await program.account.farmState.fetch(farm);
    const expectedTotal = users.reduce((sum, _, i) => sum + (100 + i * 9), 0);
    assert.approximately(
      farmState.totalStakedAmount.toNumber(),
      expectedTotal,
      100
    );
    
    // Add rewards
    await addRewards(1000000);
    
    // All users harvest simultaneously
    const harvestPromises = users.map(user => 
      harvestReward(user, 0)
    );
    const rewards = await Promise.all(harvestPromises);
    
    // Verify conservation
    const totalHarvested = rewards.reduce((sum, r) => sum + r, 0);
    assert.isAtMost(totalHarvested, 1000000);
  });
  
  it("maintains consistency during rapid stake changes", async () => {
    // Rapid stake/unstake cycles
    for (let cycle = 0; cycle < 10; cycle++) {
      // Random users stake
      const stakers = users.slice(0, 50);
      await Promise.all(stakers.map(u => stake(u, 100)));
      
      // Random users unstake
      const unstakers = users.slice(25, 75);
      await Promise.all(unstakers.map(u => unstake(u, 50)));
      
      // Verify consistency
      await verifyFarmConsistency(farm);
    }
  });
});
```

### Oracle Manipulation Tests
```typescript
// tests/integration/oracle_security.ts

describe("Oracle Security Tests", () => {
  it("rejects extreme price changes", async () => {
    const farm = await createFarmWithOracle();
    
    // Set initial price
    await updateOraclePrice(100);
    await refreshFarm(farm);
    
    // Try extreme price change (>20%)
    await updateOraclePrice(150);
    
    try {
      await refreshFarm(farm);
      assert.fail("Should reject large price change");
    } catch (err) {
      assert.include(err.toString(), "OraclePriceChangeToLarge");
    }
    
    // Gradual change should work
    await updateOraclePrice(115); // 15% change
    await refreshFarm(farm);
  });
  
  it("handles stale oracle prices", async () => {
    const farm = await createFarmWithOracle();
    
    // Set old price
    await updateOraclePrice(100, Date.now() - 7200000); // 2 hours old
    
    try {
      await refreshFarm(farm);
      assert.fail("Should reject stale price");
    } catch (err) {
      assert.include(err.toString(), "ScopeOraclePriceTooOld");
    }
  });
});
```

---

## 3. Fuzzing Framework

### Rust Fuzzing with cargo-fuzz
```rust
// fuzz/fuzz_targets/stake_operations.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use farms::farm_operations;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    
    // Parse fuzz input
    let amount = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let user_id = u64::from_le_bytes(data[8..16].try_into().unwrap());
    
    // Create test environment
    let mut farm = create_test_farm();
    let mut user = create_test_user_with_id(user_id);
    
    // Try operations with fuzzed values
    let _ = stake(&mut farm, &mut user, amount);
    let _ = refresh_global_rewards(&mut farm, current_time());
    let _ = harvest_reward(&mut farm, &mut user, 0);
    let _ = unstake(&mut farm, &mut user, amount / 2);
    
    // Verify invariants still hold
    assert!(farm.total_active_stake_scaled >= 0);
    assert!(farm.rewards_issued_cumulative >= farm.rewards_issued_unclaimed);
});
```

### Property-Based Testing with proptest
```rust
// tests/property/reward_distribution.rs
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_reward_distribution_properties(
        num_users in 1usize..100,
        stakes in prop::collection::vec(1u64..1000000, 1..100),
        rewards in 1u64..1000000000,
        time_delta in 1u64..86400
    ) {
        let mut farm = create_test_farm();
        let mut users = create_test_users(num_users);
        
        // Setup stakes
        for (user, stake) in users.iter_mut().zip(stakes.iter()) {
            stake_user(&mut farm, user, *stake);
        }
        
        // Add rewards
        add_rewards(&mut farm, rewards);
        
        // Advance time
        advance_time(time_delta);
        refresh_global_rewards(&mut farm, current_time());
        
        // Harvest all
        let mut total_harvested = 0u64;
        for user in &mut users {
            let harvested = harvest_reward(&mut farm, user, 0);
            total_harvested += harvested;
            
            // Property: No user gets negative rewards
            prop_assert!(harvested >= 0);
        }
        
        // Property: Conservation of rewards
        prop_assert!(total_harvested <= rewards);
        
        // Property: Fair distribution (proportional to stake)
        for (i, user) in users.iter().enumerate() {
            let user_stake = stakes[i];
            let total_stake: u64 = stakes.iter().sum();
            let expected_share = (rewards as u128 * user_stake as u128 / total_stake as u128) as u64;
            let actual = user.rewards_claimed;
            
            // Allow 1% deviation for rounding
            let tolerance = expected_share / 100;
            prop_assert!(
                actual >= expected_share.saturating_sub(tolerance) &&
                actual <= expected_share + tolerance,
                "User {} expected {} got {}", i, expected_share, actual
            );
        }
    }
}
```

---

## 4. Simulation Testing

### Economic Attack Simulations
```python
# simulations/economic_attacks.py
import numpy as np
import matplotlib.pyplot as plt

class FarmSimulator:
    def __init__(self):
        self.total_stake = 0
        self.reward_per_share = 0
        self.users = {}
        
    def simulate_sandwich_attack(self):
        """Simulate delegated stake sandwich attack"""
        
        # Setup: Victim has stake
        self.add_user("victim", stake=1000)
        self.add_user("attacker", stake=0)
        
        # Add rewards
        self.add_rewards(10000)
        
        results = []
        
        # Attack sequence
        for t in range(100):
            if t == 50:  # Attack moment
                # Front-run: Zero victim stake
                self.set_delegated_stake("victim", 0)
                # Set attacker stake high
                self.set_delegated_stake("attacker", 10000)
                
            if t == 51:  # Harvest
                victim_reward = self.harvest("victim")
                attacker_reward = self.harvest("attacker")
                results.append({
                    "victim": victim_reward,
                    "attacker": attacker_reward
                })
                
            if t == 52:  # Reset
                self.set_delegated_stake("victim", 1000)
                self.set_delegated_stake("attacker", 0)
                
            self.tick()
            
        return results
    
    def simulate_whale_manipulation(self):
        """Simulate large stake holder manipulating rewards"""
        
        # Small users
        for i in range(100):
            self.add_user(f"small_{i}", stake=10)
            
        # Whale
        self.add_user("whale", stake=0)
        
        rewards_captured = []
        
        for epoch in range(10):
            # Add rewards
            self.add_rewards(1000)
            
            # Whale enters just before distribution
            self.set_stake("whale", 10000)
            self.distribute_rewards()
            
            # Whale harvests and exits
            whale_rewards = self.harvest("whale")
            rewards_captured.append(whale_rewards)
            self.set_stake("whale", 0)
            
        return rewards_captured

# Run simulations
simulator = FarmSimulator()
sandwich_results = simulator.simulate_sandwich_attack()
whale_results = simulator.simulate_whale_manipulation()

print(f"Sandwich attack captured: {sandwich_results}")
print(f"Whale manipulation over 10 epochs: {sum(whale_results)}")
```

---

## 5. Stress Testing

### Load Testing Script
```typescript
// tests/stress/load_test.ts
import { Connection, Keypair } from "@solana/web3.js";
import { performance } from "perf_hooks";

async function stressTest() {
  const connection = new Connection("http://localhost:8899");
  const farm = await createFarm();
  
  // Test parameters
  const NUM_USERS = 1000;
  const OPERATIONS_PER_USER = 100;
  const CONCURRENT_OPERATIONS = 50;
  
  const users: Keypair[] = [];
  for (let i = 0; i < NUM_USERS; i++) {
    users.push(Keypair.generate());
    await initializeUser(users[i]);
  }
  
  console.log(`Starting stress test with ${NUM_USERS} users`);
  
  // Measure transaction throughput
  const startTime = performance.now();
  let successfulTxs = 0;
  let failedTxs = 0;
  
  // Execute operations in batches
  for (let batch = 0; batch < OPERATIONS_PER_USER; batch++) {
    const promises = [];
    
    for (let i = 0; i < CONCURRENT_OPERATIONS; i++) {
      const user = users[Math.floor(Math.random() * NUM_USERS)];
      const operation = Math.random();
      
      if (operation < 0.4) {
        promises.push(stake(user, 100).catch(() => failedTxs++));
      } else if (operation < 0.7) {
        promises.push(unstake(user, 50).catch(() => failedTxs++));
      } else {
        promises.push(harvestReward(user, 0).catch(() => failedTxs++));
      }
    }
    
    const results = await Promise.allSettled(promises);
    successfulTxs += results.filter(r => r.status === "fulfilled").length;
    
    // Add some rewards periodically
    if (batch % 10 === 0) {
      await addRewards(10000);
    }
  }
  
  const endTime = performance.now();
  const duration = (endTime - startTime) / 1000;
  const tps = successfulTxs / duration;
  
  console.log(`
    Stress Test Results:
    - Duration: ${duration.toFixed(2)}s
    - Successful Transactions: ${successfulTxs}
    - Failed Transactions: ${failedTxs}
    - TPS: ${tps.toFixed(2)}
    - Success Rate: ${(successfulTxs / (successfulTxs + failedTxs) * 100).toFixed(2)}%
  `);
  
  // Verify final state consistency
  await verifyFarmConsistency(farm);
}
```

---

## 6. Formal Verification

### TLA+ Specification
```tla
---------------------------- MODULE KaminoFarms ----------------------------
EXTENDS Integers, Sequences, TLC

CONSTANTS 
    Users,          \* Set of user identifiers
    MaxStake,       \* Maximum stake amount
    MaxRewards      \* Maximum rewards

VARIABLES
    stakes,         \* Function mapping users to stake amounts
    rewards,        \* Available rewards in pool
    distributed,    \* Rewards distributed to users
    totalStake,     \* Sum of all stakes
    rewardPerShare  \* Cumulative reward per share

Init ==
    /\ stakes = [u \in Users |-> 0]
    /\ rewards = 0
    /\ distributed = [u \in Users |-> 0]
    /\ totalStake = 0
    /\ rewardPerShare = 0

Stake(user, amount) ==
    /\ amount > 0
    /\ amount <= MaxStake
    /\ stakes' = [stakes EXCEPT ![user] = stakes[user] + amount]
    /\ totalStake' = totalStake + amount
    /\ UNCHANGED <<rewards, distributed, rewardPerShare>>

AddRewards(amount) ==
    /\ amount > 0
    /\ amount <= MaxRewards
    /\ rewards' = rewards + amount
    /\ IF totalStake > 0
       THEN rewardPerShare' = rewardPerShare + (amount \div totalStake)
       ELSE UNCHANGED rewardPerShare
    /\ UNCHANGED <<stakes, distributed, totalStake>>

Harvest(user) ==
    /\ stakes[user] > 0
    /\ LET earned == stakes[user] * rewardPerShare - distributed[user]
       IN /\ earned > 0
          /\ rewards >= earned
          /\ rewards' = rewards - earned
          /\ distributed' = [distributed EXCEPT ![user] = distributed[user] + earned]
    /\ UNCHANGED <<stakes, totalStake, rewardPerShare>>

\* Invariants
TypeInvariant ==
    /\ stakes \in [Users -> 0..MaxStake]
    /\ rewards \in 0..MaxRewards
    /\ totalStake = Sum(stakes)

ConservationInvariant ==
    \* Total distributed never exceeds total added
    LET totalDistributed == Sum(distributed)
        totalAdded == rewards + totalDistributed
    IN totalDistributed <= totalAdded

NoNegativeRewards ==
    \A u \in Users: distributed[u] >= 0

FairDistribution ==
    \* Each user's share is proportional to their stake
    \A u1, u2 \in Users:
        stakes[u1] > 0 /\ stakes[u2] > 0 =>
            distributed[u1] * stakes[u2] ~= distributed[u2] * stakes[u1]
```

---

## 7. CI/CD Pipeline Configuration

### GitHub Actions Workflow
```yaml
# .github/workflows/security-tests.yml
name: Security Testing Pipeline

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true
          
      - name: Run unit tests
        run: cargo test --all-features
        
      - name: Run property tests
        run: cargo test --features proptest
        
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Setup Solana
        run: |
          sh -c "$(curl -sSfL https://release.solana.com/v1.14.0/install)"
          export PATH="/home/runner/.local/share/solana/install/active_release/bin:$PATH"
          solana-test-validator &
          
      - name: Run integration tests
        run: |
          npm install
          npm run test:integration
          
  fuzzing:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz
        
      - name: Run fuzzing
        run: |
          cd fuzz
          cargo fuzz run stake_operations -- -max_total_time=300
          cargo fuzz run reward_distribution -- -max_total_time=300
          
  security-audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Run cargo audit
        run: |
          cargo install cargo-audit
          cargo audit
          
      - name: Run clippy
        run: cargo clippy -- -D warnings
        
      - name: Check for overflow
        run: |
          # Verify overflow-checks are enabled
          grep -q "overflow-checks = true" Cargo.toml
          
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
        
      - name: Generate coverage
        run: cargo tarpaulin --out Xml
        
      - name: Upload coverage
        uses: codecov/codecov-action@v2
        with:
          files: ./cobertura.xml
          fail_ci_if_error: true
          
      - name: Check coverage threshold
        run: |
          # Fail if coverage < 80%
          coverage=$(cargo tarpaulin --print-summary | grep "Coverage" | awk '{print $2}' | sed 's/%//')
          if (( $(echo "$coverage < 80" | bc -l) )); then
            echo "Coverage $coverage% is below 80% threshold"
            exit 1
          fi
```

---

## 8. Monitoring and Alerting

### Runtime Monitoring Script
```typescript
// monitoring/runtime_monitor.ts
import { Connection, PublicKey } from "@solana/web3.js";
import { WebhookClient } from "discord.js";

class FarmMonitor {
  private connection: Connection;
  private webhook: WebhookClient;
  private farms: Map<string, FarmState>;
  
  async monitorFarms() {
    setInterval(async () => {
      for (const [farmId, prevState] of this.farms) {
        const currentState = await this.getFarmState(farmId);
        
        // Check for anomalies
        this.checkDivisionByZero(farmId, currentState);
        this.checkRapidStakeChanges(farmId, prevState, currentState);
        this.checkRewardConservation(farmId, currentState);
        this.checkOraclePriceSpikes(farmId, prevState, currentState);
        
        this.farms.set(farmId, currentState);
      }
    }, 60000); // Check every minute
  }
  
  private checkDivisionByZero(farmId: string, state: FarmState) {
    if (state.isDelegated && state.totalActiveStake === 0 && state.rewardsAvailable > 0) {
      this.alert("CRITICAL", `Farm ${farmId} at risk of division by zero!`);
    }
  }
  
  private checkRapidStakeChanges(farmId: string, prev: FarmState, current: FarmState) {
    const changePercent = Math.abs(current.totalStake - prev.totalStake) / prev.totalStake * 100;
    
    if (changePercent > 50) {
      this.alert("WARNING", `Farm ${farmId} stake changed by ${changePercent.toFixed(2)}%`);
    }
  }
  
  private checkRewardConservation(farmId: string, state: FarmState) {
    const distributed = state.rewardsIssuedCumulative;
    const available = state.rewardsAvailable;
    const total = distributed + available;
    
    if (total > state.totalRewardsAdded) {
      this.alert("CRITICAL", `Farm ${farmId} reward conservation violated!`);
    }
  }
  
  private async alert(level: string, message: string) {
    console.error(`[${level}] ${message}`);
    
    if (this.webhook) {
      await this.webhook.send({
        embeds: [{
          title: `🚨 ${level} Alert`,
          description: message,
          color: level === "CRITICAL" ? 0xFF0000 : 0xFFFF00,
          timestamp: new Date().toISOString()
        }]
      });
    }
  }
}
```

---

## Testing Checklist

### Pre-Deployment
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Fuzzing completed (minimum 1M iterations)
- [ ] Property tests pass
- [ ] Formal verification complete
- [ ] Code coverage > 80%
- [ ] No critical vulnerabilities in audit
- [ ] Stress test successful (>100 TPS)

### Post-Deployment
- [ ] Monitoring active
- [ ] Alerts configured
- [ ] Incident response plan ready
- [ ] Bug bounty program live
- [ ] Regular security reviews scheduled

---

*End of Testing Framework Document*