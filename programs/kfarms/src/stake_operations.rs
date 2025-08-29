use std::ops::{Deref, DerefMut};

use decimal_wad::decimal::Decimal;

use crate::{
    state::{self, LockingMode},
    types::VaultWithdrawEffects,
    utils::{
        math::{full_decimal_mul_div, u64_mul_div},
        withdrawal_penalty::apply_early_withdrawal_penalty,
    },
    xmsg, FarmError,
};

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct UserStake {
    active_stake: Decimal,
    pending_deposit_stake: Decimal,
    pending_withdrawal_unstake: Decimal,
    last_stake_ts: u64,
}

pub trait UserStakeAccessor {
    fn get_accessor(&mut self) -> UserStakeAbstract<Self>
    where
        Self: Sized;
    fn update(&mut self, abstract_val: UserStake)
    where
        Self: Sized;
}

pub struct UserStakeAbstract<'a, T: UserStakeAccessor> {
    internal: UserStake,
    src_ref: &'a mut T,
}

impl<T: UserStakeAccessor> Deref for UserStakeAbstract<'_, T> {
    type Target = UserStake;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<T: UserStakeAccessor> DerefMut for UserStakeAbstract<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.internal
    }
}

impl<T: UserStakeAccessor> Drop for UserStakeAbstract<'_, T> {
    fn drop(&mut self) {
        self.src_ref.update(self.internal);
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct FarmStake {
    total_active_stake: Decimal,
    total_pending_stake: Decimal,
    total_active_amount: u64,
    total_pending_amount: u64,
    locking_mode: LockingMode,
    locking_start_timestamp: u64,
    locking_duration: u64,
    locking_early_withdrawal_penalty_bps: u64,
}

pub trait FarmStakeAccessor {
    fn get_accessor(&mut self) -> FarmStakeAbstract<Self>
    where
        Self: Sized;
    fn update(&mut self, abstract_val: FarmStake)
    where
        Self: Sized;
}
pub struct FarmStakeAbstract<'a, T: FarmStakeAccessor> {
    internal: FarmStake,
    src_ref: &'a mut T,
}

impl<T: FarmStakeAccessor> Deref for FarmStakeAbstract<'_, T> {
    type Target = FarmStake;

    fn deref(&self) -> &Self::Target {
        &self.internal
    }
}

impl<T: FarmStakeAccessor> DerefMut for FarmStakeAbstract<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.internal
    }
}

impl<T: FarmStakeAccessor> Drop for FarmStakeAbstract<'_, T> {
    fn drop(&mut self) {
        self.src_ref.update(self.internal);
    }
}

impl UserStakeAccessor for state::UserState {
    fn get_accessor(&mut self) -> UserStakeAbstract<'_, Self> {
        UserStakeAbstract {
            internal: UserStake {
                active_stake: self.get_active_stake_decimal(),
                pending_deposit_stake: self.get_pending_deposit_stake_decimal(),
                pending_withdrawal_unstake: self.get_pending_withdrawal_unstake_decimal(),
                last_stake_ts: self.last_stake_ts,
            },
            src_ref: self,
        }
    }

    fn update(&mut self, abstract_val: UserStake) {
        self.set_active_stake_decimal(abstract_val.active_stake);
        self.set_pending_deposit_stake_decimal(abstract_val.pending_deposit_stake);
        self.set_pending_withdrawal_unstake_decimal(abstract_val.pending_withdrawal_unstake);
    }
}

impl FarmStakeAccessor for state::FarmState {
    fn get_accessor(&mut self) -> FarmStakeAbstract<'_, Self> {
        FarmStakeAbstract {
            internal: FarmStake {
                total_active_stake: self.get_total_active_stake_decimal(),
                total_pending_stake: self.get_total_pending_stake_decimal(),
                total_active_amount: self.total_staked_amount,
                total_pending_amount: self.total_pending_amount,
                locking_duration: self.locking_duration,
                locking_early_withdrawal_penalty_bps: self.locking_early_withdrawal_penalty_bps,
                locking_mode: self.get_locking_mode(),
                locking_start_timestamp: self.locking_start_timestamp,
            },
            src_ref: self,
        }
    }

    fn update(&mut self, abstract_val: FarmStake) {
        self.set_total_active_stake_decimal(abstract_val.total_active_stake);
        self.set_total_pending_stake_decimal(abstract_val.total_pending_stake);
        self.total_staked_amount = abstract_val.total_active_amount;
        self.total_pending_amount = abstract_val.total_pending_amount;
    }
}

pub fn convert_stake_to_amount(
    stake: Decimal,
    total_stake: Decimal,
    total_amount: u64,
    round_up: bool,
) -> u64 {
    if stake == Decimal::zero() {
        return 0;
    }

    let amount_dec = if total_stake != Decimal::zero() {
        full_decimal_mul_div(stake, total_amount, total_stake)
    } else {
        total_amount.into()
    };

    if round_up {
        amount_dec.try_ceil().unwrap()
    } else {
        amount_dec.try_floor().unwrap()
    }
}

pub fn convert_amount_to_stake(amount: u64, total_stake: Decimal, total_amount: u64) -> Decimal {
    if amount == 0 {
        return Decimal::zero();
    }
    if total_stake == Decimal::zero() || total_amount == 0 {
        assert_eq!(
            total_stake,
            Decimal::zero(),
            "Total amount is zero but total stake is not"
        );
        Decimal::from(amount)
    } else {
        total_stake * amount / total_amount
    }
}

pub fn add_pending_deposit_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
    deposited_amount: u64,
) -> Result<Decimal, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    let user_gained_pending_stake = convert_amount_to_stake(
        deposited_amount,
        farm.total_pending_stake,
        farm.total_pending_amount,
    );

    user_stake.pending_deposit_stake = user_stake.pending_deposit_stake + user_gained_pending_stake;

    farm.total_pending_amount += deposited_amount;
    farm.total_pending_stake = farm.total_pending_stake + user_gained_pending_stake;

    Ok(user_gained_pending_stake)
}

pub fn remove_pending_deposit_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
) -> Result<u64, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    let pending_amount_removed: u64 = convert_stake_to_amount(
        user_stake.pending_deposit_stake,
        farm.total_pending_stake,
        farm.total_pending_amount,
        false,
    );

    farm.total_pending_amount -= pending_amount_removed;

    farm.total_pending_stake = farm.total_pending_stake - user_stake.pending_deposit_stake;

    user_stake.pending_deposit_stake = Decimal::zero();

    Ok(pending_amount_removed)
}

pub fn add_active_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
    staked_amount: u64,
) -> Result<Decimal, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    let user_gained_active_stake = convert_amount_to_stake(
        staked_amount,
        farm.total_active_stake,
        farm.total_active_amount,
    );

    user_stake.active_stake = user_stake.active_stake + user_gained_active_stake;

    farm.total_active_amount += staked_amount;
    farm.total_active_stake = farm.total_active_stake + user_gained_active_stake;

    Ok(user_gained_active_stake)
}

pub fn activate_pending_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
) -> Result<(u64, Decimal), FarmError> {
    let amount_to_stake = remove_pending_deposit_stake(user_stake, farm)?;
    let gained_active_stake = add_active_stake(user_stake, farm, amount_to_stake)?;
    Ok((amount_to_stake, gained_active_stake))
}

pub fn remove_active_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
    unstaked_shares: Decimal,
) -> Result<u64, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    assert!(
        unstaked_shares <= user_stake.active_stake,
        "Not enough active stake ({}) to perform this unstake ({} requested)",
        user_stake.active_stake,
        unstaked_shares
    );

    let unstaked_amount: u64 = convert_stake_to_amount(
        unstaked_shares,
        farm.total_active_stake,
        farm.total_active_amount,
        false,
    );

    user_stake.active_stake = user_stake.active_stake - unstaked_shares;

    farm.total_active_amount -= unstaked_amount;
    farm.total_active_stake = farm.total_active_stake - unstaked_shares;

    Ok(unstaked_amount)
}

pub fn add_pending_withdrawal_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
    unstaked_amount: u64,
) -> Result<Decimal, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    let user_gained_pending_stake = convert_amount_to_stake(
        unstaked_amount,
        farm.total_pending_stake,
        farm.total_pending_amount,
    );

    user_stake.pending_withdrawal_unstake =
        user_stake.pending_withdrawal_unstake + user_gained_pending_stake;

    farm.total_pending_amount += unstaked_amount;
    farm.total_pending_stake = farm.total_pending_stake + user_gained_pending_stake;

    Ok(user_gained_pending_stake)
}

pub fn unstake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
    stake_share_to_unstake: Decimal,
    ts: u64,
) -> Result<(u64, Decimal, u64), FarmError> {
    let amount_to_unstake = remove_active_stake(user_stake, farm, stake_share_to_unstake)?;

    let farm_accessor = farm.get_accessor();
    let user_accessor = user_stake.get_accessor();
    let (amount_to_unstake_post_penalty, unstake_penalty) = match farm_accessor.locking_mode {
        LockingMode::None => (amount_to_unstake, 0),
        LockingMode::WithExpiry => apply_early_withdrawal_penalty(
            farm_accessor.locking_duration,
            farm_accessor.locking_start_timestamp,
            ts,
            farm_accessor.locking_early_withdrawal_penalty_bps,
            amount_to_unstake,
        )?,
        LockingMode::Continuous => apply_early_withdrawal_penalty(
            farm_accessor.locking_duration,
            user_accessor.last_stake_ts,
            ts,
            farm_accessor.locking_early_withdrawal_penalty_bps,
            amount_to_unstake,
        )?,
    };

    if farm_accessor.locking_mode != LockingMode::None {
        xmsg!(
            "Unstaking {}, with mode {:?}, got {} and penalty {}",
            amount_to_unstake,
            farm_accessor.locking_mode,
            amount_to_unstake_post_penalty,
            unstake_penalty
        );
    }

    drop(farm_accessor);
    drop(user_accessor);

    let gained_pending_stake =
        add_pending_withdrawal_stake(user_stake, farm, amount_to_unstake_post_penalty)?;
    Ok((
        amount_to_unstake_post_penalty,
        gained_pending_stake,
        unstake_penalty,
    ))
}

pub fn remove_pending_withdrawal_stake(
    user_stake: &mut impl UserStakeAccessor,
    farm: &mut impl FarmStakeAccessor,
) -> Result<u64, FarmError> {
    let mut user_stake = user_stake.get_accessor();
    let mut farm = farm.get_accessor();

    let pending_amount_removed: u64 = convert_stake_to_amount(
        user_stake.pending_withdrawal_unstake,
        farm.total_pending_stake,
        farm.total_pending_amount,
        false,
    );

    farm.total_pending_amount -= pending_amount_removed;
    farm.total_pending_stake = farm.total_pending_stake - user_stake.pending_withdrawal_unstake;

    user_stake.pending_withdrawal_unstake = Decimal::zero();

    Ok(pending_amount_removed)
}

pub fn increase_total_amount(
    farm: &mut impl FarmStakeAccessor,
    amount: u64,
) -> Result<(), FarmError> {
    let mut farm = farm.get_accessor();

    farm.total_active_amount += amount;
    Ok(())
}

pub fn withdraw_farm(
    farm: &mut impl FarmStakeAccessor,
    req_withdraw_amount: u64,
) -> Result<VaultWithdrawEffects, FarmError> {
    let mut farm = farm.get_accessor();

    let vault_amount = farm.total_active_amount + farm.total_pending_amount;

    if req_withdraw_amount >= vault_amount {
        farm.total_active_amount = 0;
        farm.total_pending_amount = 0;
        xmsg!("Withdraw all farm vault (left frozen): {vault_amount}");
        return Ok(VaultWithdrawEffects {
            amount_to_withdraw: vault_amount,
            farm_to_freeze: true,
        });
    }

    let removed_active_amount: u64 =
        u64_mul_div(farm.total_active_amount, req_withdraw_amount, vault_amount);
    let removed_pending_amount: u64 =
        u64_mul_div(farm.total_pending_amount, req_withdraw_amount, vault_amount);

    farm.total_active_amount -= removed_active_amount;
    farm.total_pending_amount -= removed_pending_amount;

    let amount_to_withdraw = removed_active_amount + removed_pending_amount;

    xmsg!("Withdraw farm vault: {removed_active_amount} (active) + {removed_pending_amount} (pending) = {amount_to_withdraw} / {req_withdraw_amount}");

    Ok(VaultWithdrawEffects {
        amount_to_withdraw,
        farm_to_freeze: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FarmState, UserState};

    #[test]
    fn convert_amount_to_stake_edges() {
        // zero amount
        let stake = convert_amount_to_stake(0, Decimal::zero(), 0);
        assert_eq!(stake, Decimal::zero());

        // empty pool -> 1:1 stake minting
        let stake = convert_amount_to_stake(12345, Decimal::zero(), 0);
        assert_eq!(stake, Decimal::from(12345u64));

        // proportional in non-empty pool
        let total_stake = Decimal::from(1_000u64);
        let total_amount = 2_000u64;
        // depositing 100 should yield 50 stake
        let stake = convert_amount_to_stake(100, total_stake, total_amount);
        assert_eq!(stake, Decimal::from(50u64));
    }

    #[test]
    fn convert_stake_to_amount_rounding() {
        // total stake 3, total amount 10 -> each stake ~ 3.333 amount
        let total_stake = Decimal::from(3u64);
        let total_amount = 10u64;
        let one = Decimal::from(1u64);
        let down = convert_stake_to_amount(one, total_stake, total_amount, false);
        let up = convert_stake_to_amount(one, total_stake, total_amount, true);
        assert_eq!(down, 3);
        assert_eq!(up, 4);
    }

    #[test]
    fn withdraw_farm_pro_rata_and_freeze() {
        let mut farm = FarmState::default();
        farm.total_staked_amount = 60;
        farm.total_pending_amount = 40;

        // Partial withdraw 10 out of 100
        let eff = withdraw_farm(&mut farm, 10).unwrap();
        assert!(!eff.farm_to_freeze);
        assert_eq!(eff.amount_to_withdraw, 10);
        assert_eq!(farm.total_staked_amount, 54); // 60 - 6
        assert_eq!(farm.total_pending_amount, 36); // 40 - 4

        // Withdraw all
        let eff = withdraw_farm(&mut farm, 90).unwrap();
        assert!(eff.farm_to_freeze);
        assert_eq!(eff.amount_to_withdraw, 90);
        assert_eq!(farm.total_staked_amount, 0);
        assert_eq!(farm.total_pending_amount, 0);
    }

    #[test]
    fn unstake_penalty_with_expiry_before_start_zero_penalty() {
        // Ensure WithExpiry before start timestamp yields 0 penalty per current logic
        let mut farm = FarmState::default();
        farm.locking_mode = LockingMode::WithExpiry as u64;
        farm.locking_duration = 1_000;
        farm.locking_start_timestamp = 1_000_000; // starts in the future
        farm.locking_early_withdrawal_penalty_bps = 5_000; // 50%

        // Set user/farm with some active stake/amount
        farm.total_staked_amount = 1_000;
        farm.set_total_active_stake_decimal(Decimal::from(1_000u64));

        let mut user = UserState::default();
        user.set_active_stake_decimal(Decimal::from(1_000u64));

        // Unstake half the shares before start time
        let ts_now = 999_000;
        let (_, _gained_pending_stake, penalty) =
            unstake(&mut user, &mut farm, Decimal::from(500u64), ts_now).unwrap();

        assert_eq!(penalty, 0, "Penalty should be zero before start timestamp in WithExpiry");
    }
}
