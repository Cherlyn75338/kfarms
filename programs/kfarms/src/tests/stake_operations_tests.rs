#[cfg(test)]
mod tests {
    use crate::stake_operations::*;
    use crate::state::{FarmState, LockingMode, UserState};
    use decimal_wad::decimal::Decimal;

    // Mock implementations for testing
    impl UserStakeAccessor for UserStake {
        fn get_accessor(&mut self) -> UserStakeAbstract<'_, Self> {
            UserStakeAbstract {
                internal: *self,
                src_ref: self,
            }
        }

        fn update(&mut self, abstract_val: UserStake) {
            *self = abstract_val;
        }
    }

    impl FarmStakeAccessor for FarmStake {
        fn get_accessor(&mut self) -> FarmStakeAbstract<'_, Self> {
            FarmStakeAbstract {
                internal: *self,
                src_ref: self,
            }
        }

        fn update(&mut self, abstract_val: FarmStake) {
            *self = abstract_val;
        }
    }

    #[test]
    fn test_convert_stake_to_amount_basic() {
        // Test basic conversion
        let stake = Decimal::from(100u64);
        let total_stake = Decimal::from(1000u64);
        let total_amount = 5000u64;
        
        let amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
        assert_eq!(amount, 500); // 100/1000 * 5000 = 500
        
        // Test with round up
        let amount_up = convert_stake_to_amount(stake, total_stake, total_amount, true);
        assert_eq!(amount_up, 500);
    }

    #[test]
    fn test_convert_stake_to_amount_edge_cases() {
        // Zero stake
        let amount = convert_stake_to_amount(
            Decimal::zero(),
            Decimal::from(1000u64),
            5000u64,
            false
        );
        assert_eq!(amount, 0);
        
        // Zero total stake (first depositor case)
        let amount = convert_stake_to_amount(
            Decimal::from(100u64),
            Decimal::zero(),
            0u64,
            false
        );
        assert_eq!(amount, 0);
        
        // Total stake is zero but amount is non-zero (should return total_amount)
        let amount = convert_stake_to_amount(
            Decimal::from(100u64),
            Decimal::zero(),
            5000u64,
            false
        );
        assert_eq!(amount, 5000);
    }

    #[test]
    fn test_convert_stake_to_amount_rounding() {
        // Test rounding behavior
        let stake = Decimal::from(1u64);
        let total_stake = Decimal::from(3u64);
        let total_amount = 10u64;
        
        // 1/3 * 10 = 3.333... -> floor = 3
        let amount_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        assert_eq!(amount_floor, 3);
        
        // 1/3 * 10 = 3.333... -> ceil = 4
        let amount_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        assert_eq!(amount_ceil, 4);
    }

    #[test]
    fn test_convert_amount_to_stake_basic() {
        let amount = 500u64;
        let total_stake = Decimal::from(1000u64);
        let total_amount = 5000u64;
        
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        assert_eq!(stake, Decimal::from(100u64)); // 500/5000 * 1000 = 100
    }

    #[test]
    fn test_convert_amount_to_stake_edge_cases() {
        // Zero amount
        let stake = convert_amount_to_stake(0, Decimal::from(1000u64), 5000u64);
        assert_eq!(stake, Decimal::zero());
        
        // First depositor (zero total stake and amount)
        let stake = convert_amount_to_stake(100, Decimal::zero(), 0);
        assert_eq!(stake, Decimal::from(100u64));
    }

    #[test]
    #[should_panic(expected = "Total amount is zero but total stake is not")]
    fn test_convert_amount_to_stake_inconsistent_state() {
        // This should panic due to inconsistent state
        convert_amount_to_stake(100, Decimal::from(1000u64), 0);
    }

    #[test]
    fn test_stake_share_conservation() {
        // Test that converting back and forth preserves value (within rounding)
        let initial_amount = 1000u64;
        let total_stake = Decimal::from(10000u64);
        let total_amount = 50000u64;
        
        let stake = convert_amount_to_stake(initial_amount, total_stake, total_amount);
        let recovered_amount = convert_stake_to_amount(stake, total_stake, total_amount, false);
        
        // Should be equal or off by at most 1 due to rounding
        assert!(recovered_amount == initial_amount || recovered_amount == initial_amount - 1);
    }

    #[test]
    fn test_add_pending_deposit_stake() {
        let mut user = UserStake::default();
        let mut farm = FarmStake {
            total_pending_stake: Decimal::from(1000u64),
            total_pending_amount: 5000u64,
            ..Default::default()
        };
        
        let deposited = 500u64;
        let gained_stake = add_pending_deposit_stake(&mut user, &mut farm, deposited).unwrap();
        
        // 500/5000 * 1000 = 100
        assert_eq!(gained_stake, Decimal::from(100u64));
        assert_eq!(user.pending_deposit_stake, Decimal::from(100u64));
        assert_eq!(farm.total_pending_amount, 5500u64);
        assert_eq!(farm.total_pending_stake, Decimal::from(1100u64));
    }

    #[test]
    fn test_remove_pending_deposit_stake() {
        let mut user = UserStake {
            pending_deposit_stake: Decimal::from(100u64),
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_pending_stake: Decimal::from(1100u64),
            total_pending_amount: 5500u64,
            ..Default::default()
        };
        
        let removed_amount = remove_pending_deposit_stake(&mut user, &mut farm).unwrap();
        
        // 100/1100 * 5500 = 500
        assert_eq!(removed_amount, 500);
        assert_eq!(user.pending_deposit_stake, Decimal::zero());
        assert_eq!(farm.total_pending_amount, 5000u64);
        assert_eq!(farm.total_pending_stake, Decimal::from(1000u64));
    }

    #[test]
    fn test_activate_pending_stake() {
        let mut user = UserStake {
            pending_deposit_stake: Decimal::from(100u64),
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_pending_stake: Decimal::from(100u64),
            total_pending_amount: 500u64,
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000u64,
            ..Default::default()
        };
        
        let (amount_staked, active_stake_gained) = 
            activate_pending_stake(&mut user, &mut farm).unwrap();
        
        assert_eq!(amount_staked, 500);
        assert_eq!(active_stake_gained, Decimal::from(100u64));
        assert_eq!(user.pending_deposit_stake, Decimal::zero());
        assert_eq!(user.active_stake, Decimal::from(100u64));
        assert_eq!(farm.total_pending_amount, 0);
        assert_eq!(farm.total_active_amount, 5500);
    }

    #[test]
    fn test_unstake_with_no_locking() {
        let mut user = UserStake {
            active_stake: Decimal::from(100u64),
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000u64,
            locking_mode: LockingMode::None,
            ..Default::default()
        };
        
        let stake_to_unstake = Decimal::from(50u64);
        let (amount_unstaked, pending_stake, penalty) = 
            unstake(&mut user, &mut farm, stake_to_unstake, 1000).unwrap();
        
        assert_eq!(amount_unstaked, 250); // 50/1000 * 5000
        assert_eq!(penalty, 0);
        assert_eq!(user.active_stake, Decimal::from(50u64));
        assert!(pending_stake > Decimal::zero());
    }

    #[test]
    fn test_unstake_with_expiry_before_start() {
        // Critical test: WithExpiry mode before locking_start_timestamp
        let mut user = UserStake {
            active_stake: Decimal::from(100u64),
            last_stake_ts: 500,
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000u64,
            locking_mode: LockingMode::WithExpiry,
            locking_start_timestamp: 2000, // Future timestamp
            locking_duration: 1000,
            locking_early_withdrawal_penalty_bps: 5000, // 50%
            ..Default::default()
        };
        
        let current_ts = 1500; // Before locking_start_timestamp
        let stake_to_unstake = Decimal::from(50u64);
        let (amount_unstaked, _pending_stake, penalty) = 
            unstake(&mut user, &mut farm, stake_to_unstake, current_ts).unwrap();
        
        // VULNERABILITY: Penalty should be 0 when withdrawing before lock start
        assert_eq!(penalty, 0);
        assert_eq!(amount_unstaked, 250); // Full amount, no penalty
    }

    #[test]
    fn test_unstake_with_continuous_locking() {
        let mut user = UserStake {
            active_stake: Decimal::from(100u64),
            last_stake_ts: 1000,
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000u64,
            locking_mode: LockingMode::Continuous,
            locking_duration: 1000,
            locking_early_withdrawal_penalty_bps: 5000, // 50%
            ..Default::default()
        };
        
        let current_ts = 1500; // Halfway through lock period
        let stake_to_unstake = Decimal::from(50u64);
        let (amount_unstaked, _pending_stake, penalty) = 
            unstake(&mut user, &mut farm, stake_to_unstake, current_ts).unwrap();
        
        let expected_amount = 250; // 50/1000 * 5000
        let expected_penalty = 62; // 25% penalty (halfway through)
        
        assert_eq!(penalty, expected_penalty);
        assert_eq!(amount_unstaked, expected_amount - expected_penalty);
    }

    #[test]
    #[should_panic(expected = "Not enough active stake")]
    fn test_unstake_insufficient_stake() {
        let mut user = UserStake {
            active_stake: Decimal::from(50u64),
            ..Default::default()
        };
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 5000u64,
            ..Default::default()
        };
        
        let stake_to_unstake = Decimal::from(100u64); // More than user has
        unstake(&mut user, &mut farm, stake_to_unstake, 1000).unwrap();
    }

    #[test]
    fn test_withdraw_farm_partial() {
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 4000u64,
            total_pending_stake: Decimal::from(200u64),
            total_pending_amount: 1000u64,
            ..Default::default()
        };
        
        let effects = withdraw_farm(&mut farm, 2500).unwrap();
        
        // Pro-rata: 4000/5000 * 2500 = 2000 from active
        //          1000/5000 * 2500 = 500 from pending
        assert_eq!(effects.amount_to_withdraw, 2500);
        assert!(!effects.farm_to_freeze);
        assert_eq!(farm.total_active_amount, 2000);
        assert_eq!(farm.total_pending_amount, 500);
    }

    #[test]
    fn test_withdraw_farm_all() {
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 4000u64,
            total_pending_stake: Decimal::from(200u64),
            total_pending_amount: 1000u64,
            ..Default::default()
        };
        
        let effects = withdraw_farm(&mut farm, 5000).unwrap();
        
        assert_eq!(effects.amount_to_withdraw, 5000);
        assert!(effects.farm_to_freeze); // Farm should freeze
        assert_eq!(farm.total_active_amount, 0);
        assert_eq!(farm.total_pending_amount, 0);
    }

    #[test]
    fn test_withdraw_farm_more_than_available() {
        let mut farm = FarmStake {
            total_active_stake: Decimal::from(1000u64),
            total_active_amount: 4000u64,
            total_pending_stake: Decimal::from(200u64),
            total_pending_amount: 1000u64,
            ..Default::default()
        };
        
        let effects = withdraw_farm(&mut farm, 10000).unwrap();
        
        // Should withdraw all available
        assert_eq!(effects.amount_to_withdraw, 5000);
        assert!(effects.farm_to_freeze);
        assert_eq!(farm.total_active_amount, 0);
        assert_eq!(farm.total_pending_amount, 0);
    }

    #[test]
    fn test_share_dilution_attack() {
        // Test potential share dilution attack
        let mut farm = FarmStake::default();
        
        // First user deposits 1 wei
        let mut user1 = UserStake::default();
        add_pending_deposit_stake(&mut user1, &mut farm, 1).unwrap();
        activate_pending_stake(&mut user1, &mut farm).unwrap();
        
        // Attacker directly increases total_amount (simulating donation)
        increase_total_amount(&mut farm, 1_000_000_000).unwrap();
        
        // Second user deposits normal amount
        let mut user2 = UserStake::default();
        let user2_deposit = 1_000_000_000;
        add_pending_deposit_stake(&mut user2, &mut farm, user2_deposit).unwrap();
        activate_pending_stake(&mut user2, &mut farm).unwrap();
        
        // Check share distribution
        // User1 should have almost all shares despite tiny deposit
        assert!(user1.active_stake > user2.active_stake);
        
        // This demonstrates the share dilution vulnerability
        // Mitigation: Minimum initial deposit or virtual shares
    }
}