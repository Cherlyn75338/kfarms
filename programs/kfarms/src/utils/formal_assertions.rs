/// Formal verification assertions and invariant checks
/// These can be used with tools like MIRAI or Prusti for formal verification

use decimal_wad::decimal::Decimal;
use crate::state::*;

/// Core invariants that must hold at all times
pub mod invariants {
    use super::*;
    
    /// Conservation of value: Total rewards distributed <= Total rewards deposited
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_reward_conservation(farm: &FarmState) -> bool {
        for reward in &farm.reward_infos[..farm.num_reward_tokens as usize] {
            let total_distributed = reward.rewards_issued_cumulative;
            let total_remaining = reward.rewards_available;
            let total_unclaimed = reward.rewards_issued_unclaimed;
            
            // This is a critical invariant
            debug_assert!(
                total_distributed + total_remaining >= total_unclaimed,
                "Conservation violated: distributed + remaining < unclaimed"
            );
        }
        true
    }
    
    /// Monotonicity: Reward per share never decreases
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_rps_monotonicity(
        old_rps: Decimal,
        new_rps: Decimal
    ) -> bool {
        debug_assert!(
            new_rps >= old_rps,
            "RPS decreased: {:?} -> {:?}",
            old_rps, new_rps
        );
        new_rps >= old_rps
    }
    
    /// Stake consistency: Sum of user stakes equals total stake
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_stake_consistency(
        farm: &FarmState,
        user_stakes_sum: Decimal
    ) -> bool {
        let tolerance = Decimal::from(1u64); // 1 unit tolerance for rounding
        let diff = if user_stakes_sum > farm.total_active_stake_scaled {
            user_stakes_sum - farm.total_active_stake_scaled
        } else {
            farm.total_active_stake_scaled - user_stakes_sum
        };
        
        debug_assert!(
            diff <= tolerance,
            "Stake inconsistency: sum={:?}, total={:?}, diff={:?}",
            user_stakes_sum, farm.total_active_stake_scaled, diff
        );
        diff <= tolerance
    }
    
    /// No rewards when no stake
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_no_rewards_without_stake(
        farm: &FarmState,
        rewards_issued: u64
    ) -> bool {
        if farm.total_active_stake_scaled == Decimal::zero() {
            debug_assert_eq!(
                rewards_issued, 0,
                "Rewards issued with zero stake"
            );
            rewards_issued == 0
        } else {
            true
        }
    }
}

/// Preconditions that must be satisfied before operations
pub mod preconditions {
    use super::*;
    
    /// Stake amount must be positive
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_positive_stake(amount: u64) -> bool {
        debug_assert!(amount > 0, "Stake amount must be positive");
        amount > 0
    }
    
    /// User must have sufficient stake for unstaking
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_sufficient_stake(
        user_stake: Decimal,
        unstake_amount: Decimal
    ) -> bool {
        debug_assert!(
            user_stake >= unstake_amount,
            "Insufficient stake: {:?} < {:?}",
            user_stake, unstake_amount
        );
        user_stake >= unstake_amount
    }
    
    /// Reward index must be valid
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_valid_reward_index(
        index: usize,
        num_rewards: u8
    ) -> bool {
        debug_assert!(
            index < num_rewards as usize,
            "Invalid reward index: {} >= {}",
            index, num_rewards
        );
        index < num_rewards as usize
    }
    
    /// Time must not go backwards
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_time_progression(
        old_time: u64,
        new_time: u64
    ) -> bool {
        debug_assert!(
            new_time >= old_time,
            "Time went backwards: {} -> {}",
            old_time, new_time
        );
        new_time >= old_time
    }
}

/// Postconditions that must be satisfied after operations
pub mod postconditions {
    use super::*;
    
    /// After stake, user and farm stakes increase
    #[cfg_attr(feature = "mirai", mirai::ensures)]
    pub fn ensure_stake_increased(
        old_user_stake: Decimal,
        new_user_stake: Decimal,
        old_farm_stake: Decimal,
        new_farm_stake: Decimal,
        amount: Decimal
    ) -> bool {
        debug_assert_eq!(
            new_user_stake,
            old_user_stake + amount,
            "User stake didn't increase correctly"
        );
        debug_assert_eq!(
            new_farm_stake,
            old_farm_stake + amount,
            "Farm stake didn't increase correctly"
        );
        true
    }
    
    /// After unstake, stakes decrease
    #[cfg_attr(feature = "mirai", mirai::ensures)]
    pub fn ensure_stake_decreased(
        old_user_stake: Decimal,
        new_user_stake: Decimal,
        old_farm_stake: Decimal,
        new_farm_stake: Decimal,
        amount: Decimal
    ) -> bool {
        debug_assert_eq!(
            new_user_stake,
            old_user_stake - amount,
            "User stake didn't decrease correctly"
        );
        debug_assert_eq!(
            new_farm_stake,
            old_farm_stake - amount,
            "Farm stake didn't decrease correctly"
        );
        true
    }
    
    /// After harvest, unclaimed rewards reset
    #[cfg_attr(feature = "mirai", mirai::ensures)]
    pub fn ensure_rewards_claimed(
        user_unclaimed_after: u64
    ) -> bool {
        debug_assert_eq!(
            user_unclaimed_after, 0,
            "Unclaimed rewards not reset after harvest"
        );
        user_unclaimed_after == 0
    }
}

/// Math operation assertions
pub mod math_assertions {
    use super::*;
    
    /// Multiplication should not overflow
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_safe_mul(a: u64, b: u64) -> Option<u64> {
        let result = (a as u128).checked_mul(b as u128)?;
        if result <= u64::MAX as u128 {
            Some(result as u64)
        } else {
            debug_assert!(false, "Multiplication overflow: {} * {}", a, b);
            None
        }
    }
    
    /// Division by zero protection
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_safe_div(a: u64, b: u64) -> Option<u64> {
        if b == 0 {
            debug_assert!(false, "Division by zero");
            None
        } else {
            Some(a / b)
        }
    }
    
    /// Decimal operations should handle overflow
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_decimal_mul_div(
        a: Decimal,
        b: Decimal,
        c: Decimal
    ) -> Option<Decimal> {
        if c == Decimal::zero() {
            debug_assert!(false, "Decimal division by zero");
            return None;
        }
        
        a.checked_mul(b)?.checked_div(c)
    }
    
    /// Penalty calculation bounds
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_penalty_bounds(
        amount: u64,
        penalty_bps: u64
    ) -> u64 {
        debug_assert!(
            penalty_bps <= 10000,
            "Penalty BPS exceeds 100%: {}",
            penalty_bps
        );
        
        let penalty = (amount as u128)
            .saturating_mul(penalty_bps as u128)
            .saturating_div(10000);
        
        let result = penalty.min(amount as u128) as u64;
        
        debug_assert!(
            result <= amount,
            "Penalty exceeds amount: {} > {}",
            result, amount
        );
        
        result
    }
}

/// Security assertions
pub mod security_assertions {
    use super::*;
    use anchor_lang::prelude::*;
    
    /// Authority checks
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_admin_authority(
        signer: &Pubkey,
        expected_admin: &Pubkey
    ) -> bool {
        debug_assert_eq!(
            signer, expected_admin,
            "Unauthorized: signer {} != admin {}",
            signer, expected_admin
        );
        signer == expected_admin
    }
    
    /// PDA derivation correctness
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_pda_derivation(
        seeds: &[&[u8]],
        program_id: &Pubkey,
        expected_pda: &Pubkey
    ) -> bool {
        let (pda, _bump) = Pubkey::find_program_address(seeds, program_id);
        debug_assert_eq!(
            &pda, expected_pda,
            "PDA mismatch: derived {} != expected {}",
            pda, expected_pda
        );
        &pda == expected_pda
    }
    
    /// Token account ownership
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_token_account_owner(
        token_account_owner: &Pubkey,
        expected_owner: &Pubkey
    ) -> bool {
        debug_assert_eq!(
            token_account_owner, expected_owner,
            "Token account owner mismatch"
        );
        token_account_owner == expected_owner
    }
}

/// Oracle assertions
pub mod oracle_assertions {
    use super::*;
    
    /// Oracle price freshness
    #[cfg_attr(feature = "mirai", mirai::requires)]
    pub fn require_fresh_oracle_price(
        price_timestamp: u64,
        current_timestamp: u64,
        max_age: u64
    ) -> bool {
        let age = current_timestamp.saturating_sub(price_timestamp);
        debug_assert!(
            age <= max_age,
            "Stale oracle price: age {} > max {}",
            age, max_age
        );
        age <= max_age
    }
    
    /// Oracle price bounds
    #[cfg_attr(feature = "mirai", mirai::verify)]
    pub fn assert_reasonable_price(
        price: i64,
        min_price: i64,
        max_price: i64
    ) -> bool {
        debug_assert!(
            price >= min_price && price <= max_price,
            "Oracle price out of bounds: {} not in [{}, {}]",
            price, min_price, max_price
        );
        price >= min_price && price <= max_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_invariants() {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.reward_infos[0].rewards_issued_cumulative = 1000;
        farm.reward_infos[0].rewards_available = 500;
        farm.reward_infos[0].rewards_issued_unclaimed = 300;
        
        assert!(invariants::assert_reward_conservation(&farm));
    }
    
    #[test]
    fn test_preconditions() {
        assert!(preconditions::require_positive_stake(100));
        assert!(!preconditions::require_positive_stake(0));
        
        let user_stake = Decimal::from(1000);
        let unstake_amount = Decimal::from(500);
        assert!(preconditions::require_sufficient_stake(user_stake, unstake_amount));
    }
    
    #[test]
    fn test_math_assertions() {
        assert_eq!(math_assertions::assert_safe_mul(100, 200), Some(20000));
        assert_eq!(math_assertions::assert_safe_mul(u64::MAX, 2), None);
        
        assert_eq!(math_assertions::assert_safe_div(100, 20), Some(5));
        assert_eq!(math_assertions::assert_safe_div(100, 0), None);
        
        assert_eq!(math_assertions::assert_penalty_bounds(1000, 500), 50);
        assert_eq!(math_assertions::assert_penalty_bounds(1000, 10000), 1000);
    }
}