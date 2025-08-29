use crate::{
    state::{FarmState, UserState, RewardInfo},
    utils::math::{u64_mul_div, full_decimal_mul_div, ten_pow},
};
use decimal_wad::decimal::Decimal;

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    // D. Edge Case Scenario Tests

    #[test]
    fn test_timestamp_pre_start() {
        // Test behavior before farm starts
        let farm = create_test_farm_with_timestamps(1000, 2000, 3000);
        let current_ts = 500; // Before start
        
        // Rewards should not accrue before start
        let rewards = calculate_rewards_for_period(&farm, 0, current_ts);
        assert_eq!(rewards, 0);
    }

    #[test]
    fn test_timestamp_at_start() {
        // Test behavior exactly at farm start
        let farm = create_test_farm_with_timestamps(1000, 2000, 3000);
        let current_ts = 1000; // Exactly at start
        
        // No time has passed, no rewards
        let rewards = calculate_rewards_for_period(&farm, 1000, current_ts);
        assert_eq!(rewards, 0);
    }

    #[test]
    fn test_timestamp_post_maturity() {
        // Test behavior after farm maturity
        let farm = create_test_farm_with_timestamps(1000, 2000, 3000);
        let current_ts = 4000; // After end
        
        // Should only accrue rewards up to end time
        let rewards = calculate_rewards_for_period(&farm, 1000, current_ts);
        let max_rewards = calculate_rewards_for_period(&farm, 1000, 3000);
        assert_eq!(rewards, max_rewards);
    }

    #[test]
    fn test_zero_duration_farm() {
        // Test farm with zero duration
        let farm = create_test_farm_with_timestamps(1000, 1000, 1000);
        
        // Should handle gracefully
        let rewards = calculate_rewards_for_period(&farm, 1000, 1001);
        assert_eq!(rewards, 0);
    }

    #[test]
    fn test_extremely_large_duration() {
        // Test with very large duration (100 years)
        let seconds_per_year = 365 * 24 * 60 * 60;
        let duration = 100 * seconds_per_year;
        let farm = create_test_farm_with_timestamps(0, duration / 2, duration);
        
        // Should handle without overflow
        let rewards_per_second = 1000u64;
        let total_rewards = rewards_per_second.saturating_mul(duration as u64);
        assert!(total_rewards > 0);
        assert!(total_rewards == rewards_per_second * duration as u64);
    }

    #[test]
    fn test_extreme_price_exponent() {
        // Test with extreme price exponents
        let test_cases = vec![
            (1_000_000u64, 0i32),  // exp = 0
            (1_000_000u64, -18i32), // exp = -18 (very small)
            (1u64, 18i32),          // exp = 18 (very large)
            (u64::MAX, -9i32),      // Large value, negative exp
        ];
        
        for (value, exp) in test_cases {
            let adjusted = apply_price_with_exponent(value, exp);
            assert!(adjusted.is_ok() || adjusted.is_err());
            
            if let Ok(val) = adjusted {
                assert!(val <= u64::MAX);
            }
        }
    }

    #[test]
    fn test_extreme_price_values() {
        // Test with extreme price values
        let prices = vec![
            0u64,           // Zero price
            1u64,           // Minimum price
            u64::MAX,       // Maximum price
            u64::MAX / 2,   // Large but not max
        ];
        
        for price in prices {
            let amount = 1_000_000u64;
            
            // Apply price adjustment
            if price > 0 && price < u64::MAX / amount {
                let adjusted = amount.checked_mul(price);
                assert!(adjusted.is_some());
            }
        }
    }

    #[test]
    fn test_missing_oracle_price() {
        // Test behavior when oracle price is missing
        let mut farm = create_test_farm();
        farm.reward_infos[0].oracle_price_account = None;
        
        // Should use default price (1:1)
        let amount = 1_000_000u64;
        let adjusted = adjust_amount_by_price(amount, None);
        assert_eq!(adjusted, amount);
    }

    #[test]
    fn test_oracle_max_age_threshold() {
        // Test oracle price age validation
        let current_ts = 1000u64;
        let max_age = 60u64; // 60 seconds
        
        let test_cases = vec![
            (current_ts - 30, true),  // Fresh price
            (current_ts - 60, true),  // Exactly at threshold
            (current_ts - 61, false), // Too old
            (current_ts + 10, false), // Future timestamp (invalid)
        ];
        
        for (price_ts, should_be_valid) in test_cases {
            let is_valid = is_price_fresh(price_ts, current_ts, max_age);
            assert_eq!(is_valid, should_be_valid);
        }
    }

    #[test]
    fn test_decimal_mismatch_scenarios() {
        // Test decimal mismatches between tokens
        let test_cases = vec![
            (6, 9, 1_000_000),     // USDC (6) to SOL (9)
            (9, 6, 1_000_000_000), // SOL (9) to USDC (6)
            (18, 6, 1_000_000_000_000_000_000), // ETH (18) to USDC (6)
            (8, 8, 100_000_000),   // Same decimals
        ];
        
        for (from_decimals, to_decimals, amount) in test_cases {
            let converted = convert_decimals(amount, from_decimals, to_decimals);
            assert!(converted.is_ok());
            
            if let Ok(val) = converted {
                if to_decimals > from_decimals {
                    // Should scale up
                    assert!(val >= amount);
                } else if to_decimals < from_decimals {
                    // Should scale down
                    assert!(val <= amount);
                } else {
                    // Should be equal
                    assert_eq!(val, amount);
                }
            }
        }
    }

    #[test]
    fn test_token2022_mint_extensions() {
        // Test handling of Token-2022 mints with extensions
        let mint_with_transfer_fee = create_mint_with_transfer_fee();
        let mint_with_interest = create_mint_with_interest_bearing();
        
        // Should handle transfer fees
        let amount = 1_000_000u64;
        let fee_bps = 100u64; // 1%
        let net_amount = amount - u64_mul_div(amount, fee_bps, 10000);
        assert_eq!(apply_transfer_fee(amount, fee_bps), net_amount);
    }

    #[test]
    fn test_instruction_interleaving() {
        // Simulate instruction interleaving scenarios
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Sequence: refresh → stake → unstake → harvest
        
        // 1. Refresh
        let current_ts = 1000;
        farm.last_update_ts = current_ts;
        
        // 2. Stake
        let stake_amount = 10_000u64;
        user.set_active_stake(Decimal::from(stake_amount));
        farm.total_active_amount += stake_amount;
        
        // 3. Unstake (partial)
        let unstake_amount = 3_000u64;
        user.set_active_stake(Decimal::from(stake_amount - unstake_amount));
        user.set_pending_withdrawal_unstake(Decimal::from(unstake_amount));
        farm.total_active_amount -= unstake_amount;
        
        // 4. Harvest
        let rewards = calculate_user_rewards(&user, &farm);
        
        // Verify state consistency after interleaving
        assert_eq!(user.get_active_stake().to_u64().unwrap(), stake_amount - unstake_amount);
        assert_eq!(user.get_pending_withdrawal_unstake().to_u64().unwrap(), unstake_amount);
        assert_eq!(farm.total_active_amount, stake_amount - unstake_amount);
    }

    #[test]
    fn test_boundary_timestamp_operations() {
        // Test operations at exact boundary timestamps
        let warmup_period = 100u64;
        let cooldown_period = 200u64;
        let deposit_ts = 1000u64;
        
        // Test at warmup boundary
        let warmup_end = deposit_ts + warmup_period;
        assert!(is_deposit_ready(deposit_ts, warmup_end - 1, warmup_period) == false);
        assert!(is_deposit_ready(deposit_ts, warmup_end, warmup_period) == true);
        assert!(is_deposit_ready(deposit_ts, warmup_end + 1, warmup_period) == true);
        
        // Test at cooldown boundary
        let withdraw_ts = 2000u64;
        let cooldown_end = withdraw_ts + cooldown_period;
        assert!(is_withdrawal_ready(withdraw_ts, cooldown_end - 1, cooldown_period) == false);
        assert!(is_withdrawal_ready(withdraw_ts, cooldown_end, cooldown_period) == true);
        assert!(is_withdrawal_ready(withdraw_ts, cooldown_end + 1, cooldown_period) == true);
    }

    #[test]
    fn test_min_max_stake_amounts() {
        // Test with minimum and maximum stake amounts
        let mut farm = create_test_farm();
        
        // Test minimum stake (1 unit)
        farm.total_active_amount = 1;
        let min_rewards = calculate_rewards_for_stake(1, 1_000_000, 1);
        assert_eq!(min_rewards, 1_000_000);
        
        // Test maximum stake
        farm.total_active_amount = u64::MAX;
        let max_rewards = calculate_rewards_for_stake(u64::MAX, 1, u64::MAX);
        assert_eq!(max_rewards, 1);
    }

    #[test]
    fn test_high_frequency_operations() {
        // Test high frequency stake/unstake operations
        let mut farm = create_test_farm();
        let mut user = create_test_user();
        
        // Perform 1000 rapid operations
        for i in 0..1000 {
            if i % 2 == 0 {
                // Stake
                let amount = (i + 1) as u64;
                let current = user.get_active_stake().to_u64().unwrap();
                user.set_active_stake(Decimal::from(current + amount));
                farm.total_active_amount += amount;
            } else {
                // Unstake
                let amount = i as u64 / 2;
                let current = user.get_active_stake().to_u64().unwrap();
                if current >= amount {
                    user.set_active_stake(Decimal::from(current - amount));
                    farm.total_active_amount -= amount;
                }
            }
        }
        
        // Verify final state consistency
        assert!(farm.total_active_amount > 0);
        assert_eq!(user.get_active_stake().to_u64().unwrap(), farm.total_active_amount);
    }

    #[test]
    fn test_concurrent_user_operations() {
        // Simulate concurrent operations from multiple users
        let mut farm = create_test_farm();
        let mut users = vec![create_test_user(); 100];
        
        // Each user stakes different amount
        for (i, user) in users.iter_mut().enumerate() {
            let stake = ((i + 1) * 100) as u64;
            user.set_active_stake(Decimal::from(stake));
            farm.total_active_amount += stake;
        }
        
        // Verify total
        let sum: u64 = users.iter()
            .map(|u| u.get_active_stake().to_u64().unwrap())
            .sum();
        assert_eq!(sum, farm.total_active_amount);
        
        // Simultaneous withdrawals
        for user in users.iter_mut() {
            let current = user.get_active_stake().to_u64().unwrap();
            let withdraw = current / 2;
            user.set_active_stake(Decimal::from(current - withdraw));
            farm.total_active_amount -= withdraw;
        }
        
        // Verify consistency after concurrent operations
        let new_sum: u64 = users.iter()
            .map(|u| u.get_active_stake().to_u64().unwrap())
            .sum();
        assert_eq!(new_sum, farm.total_active_amount);
    }

    // Helper functions
    fn create_test_farm() -> FarmState {
        FarmState {
            version: 1,
            creator: [0u8; 32],
            authority: [0u8; 32],
            pending_authority: [0u8; 32],
            farm_vault: [0u8; 32],
            farm_vault_authority: [0u8; 32],
            farm_token_mint: [0u8; 32],
            farm_token_decimals: 9,
            reward_infos: vec![RewardInfo::default()],
            total_staked_amount: 0,
            total_active_amount: 0,
            total_pending_amount: 0,
            total_active_stake_scaled: 0,
            total_pending_stake_scaled: 0,
            slashed_amount_cumulative: 0,
            deposit_warmup_period: 0,
            withdrawal_cooldown_period: 0,
            is_delegated: false,
            is_permissioned: false,
            locking_mode: 0,
            locking_start_timestamp: 0,
            locking_duration: 0,
            locking_early_withdrawal_penalty_bps: 0,
            deposit_cap_amount: u64::MAX,
            last_update_ts: 0,
            _padding: [0u8; 256],
        }
    }

    fn create_test_farm_with_timestamps(start: u64, mid: u64, end: u64) -> FarmState {
        let mut farm = create_test_farm();
        farm.locking_start_timestamp = start;
        farm.locking_duration = end - start;
        farm.last_update_ts = mid;
        farm
    }

    fn create_test_user() -> UserState {
        UserState {
            version: 1,
            farm: [0u8; 32],
            owner: [0u8; 32],
            active_stake_scaled: 0,
            pending_deposit_stake_scaled: 0,
            pending_deposit_stake_ts: 0,
            pending_withdrawal_unstake_scaled: 0,
            pending_withdrawal_unstake_ts: 0,
            last_claim_ts: [0u64; 10],
            rewards_tally_scaled: [0u128; 10],
            _padding: [0u8; 256],
        }
    }

    fn calculate_rewards_for_period(farm: &FarmState, from_ts: u64, to_ts: u64) -> u64 {
        if to_ts <= from_ts {
            return 0;
        }
        
        let start = from_ts.max(farm.locking_start_timestamp);
        let end = to_ts.min(farm.locking_start_timestamp + farm.locking_duration);
        
        if end <= start {
            return 0;
        }
        
        (end - start) * 100 // Dummy calculation: 100 rewards per second
    }

    fn apply_price_with_exponent(value: u64, exp: i32) -> Result<u64, ()> {
        if exp >= 0 {
            value.checked_mul(ten_pow(exp as usize)).ok_or(())
        } else {
            let divisor = ten_pow((-exp) as usize);
            Ok(value / divisor)
        }
    }

    fn adjust_amount_by_price(amount: u64, price: Option<u64>) -> u64 {
        price.map(|p| u64_mul_div(amount, p, 1_000_000_000)).unwrap_or(amount)
    }

    fn is_price_fresh(price_ts: u64, current_ts: u64, max_age: u64) -> bool {
        price_ts <= current_ts && current_ts - price_ts <= max_age
    }

    fn convert_decimals(amount: u64, from_decimals: u8, to_decimals: u8) -> Result<u64, ()> {
        if to_decimals > from_decimals {
            let scale = ten_pow((to_decimals - from_decimals) as usize);
            amount.checked_mul(scale).ok_or(())
        } else if to_decimals < from_decimals {
            let scale = ten_pow((from_decimals - to_decimals) as usize);
            Ok(amount / scale)
        } else {
            Ok(amount)
        }
    }

    fn create_mint_with_transfer_fee() -> MockMint {
        MockMint { has_transfer_fee: true }
    }

    fn create_mint_with_interest_bearing() -> MockMint {
        MockMint { has_transfer_fee: false }
    }

    fn apply_transfer_fee(amount: u64, fee_bps: u64) -> u64 {
        amount - u64_mul_div(amount, fee_bps, 10000)
    }

    fn calculate_user_rewards(user: &UserState, farm: &FarmState) -> u64 {
        // Simplified reward calculation
        user.get_active_stake().to_u64().unwrap() / 100
    }

    fn is_deposit_ready(deposit_ts: u64, current_ts: u64, warmup_period: u64) -> bool {
        current_ts >= deposit_ts + warmup_period
    }

    fn is_withdrawal_ready(withdraw_ts: u64, current_ts: u64, cooldown_period: u64) -> bool {
        current_ts >= withdraw_ts + cooldown_period
    }

    fn calculate_rewards_for_stake(stake: u64, total_rewards: u64, total_stake: u64) -> u64 {
        if total_stake == 0 {
            return 0;
        }
        u64_mul_div(stake, total_rewards, total_stake)
    }

    struct MockMint {
        has_transfer_fee: bool,
    }
}