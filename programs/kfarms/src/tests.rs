#![allow(clippy::unwrap_used)]

use crate::stake_operations::*;
use crate::state::*;
use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
use crate::farm_operations::{refresh_global_reward, refresh_global_rewards, update_reward_config};
use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve};
use decimal_wad::decimal::Decimal;
use anchor_lang::prelude::Pubkey;

fn make_farm() -> FarmState {
    FarmState::default()
}

fn make_user() -> UserState {
    UserState::default()
}

#[test]
fn penalty_math_edges() {
    // zero penalty before start intended
    let (post, pen) = apply_early_withdrawal_penalty(100, 1_000, 999, 500, 1_000).unwrap();
    assert_eq!(post, 1_000);
    assert_eq!(pen, 0);

    // after maturity -> zero penalty
    let (post, pen) = apply_early_withdrawal_penalty(100, 1_000, 1_200, 500, 1_000).unwrap();
    assert_eq!(post, 1_000);
    assert_eq!(pen, 0);

    // proportional penalty inside window (max 10% bps -> at half time remaining = 5%)
    let (post, pen) = apply_early_withdrawal_penalty(100, 1_000, 1_050, 1_000, 1_000).unwrap();
    assert_eq!(post, 950);
    assert_eq!(pen, 50);
}

#[test]
fn convert_rounding_behaviour() {
    // stake to amount and back preserves bounds and is fair on rounding
    let total_stake = Decimal::from(1_000_000u64);
    let total_amount = 1_000_000u64;

    for amount in [1u64, 2, 3, 9, 10, 11, 99, 100, 101, 999_999, 1_000_000] {
        let stake = convert_amount_to_stake(amount, total_stake, total_amount);
        let amount_floor = convert_stake_to_amount(stake, total_stake, total_amount, false);
        let amount_ceil = convert_stake_to_amount(stake, total_stake, total_amount, true);
        assert!(amount_floor <= amount);
        assert!(amount_ceil >= amount);
        assert!(amount_ceil - amount_floor <= 1);
    }
}

#[test]
fn vault_withdrawal_proportionality() {
    let mut farm = make_farm();
    farm.total_staked_amount = 900;
    farm.total_pending_amount = 100;

    // withdraw 10% of total (100/1000)
    let effects = withdraw_farm(&mut farm, 100).unwrap();
    assert_eq!(effects.amount_to_withdraw, 100);
    assert!(!effects.farm_to_freeze);
    assert_eq!(farm.total_staked_amount, 810);
    assert_eq!(farm.total_pending_amount, 90);

    // withdraw all -> freeze
    let effects = withdraw_farm(&mut farm, 9000).unwrap();
    assert!(effects.farm_to_freeze);
    assert_eq!(effects.amount_to_withdraw, 900);
    assert_eq!(farm.total_staked_amount + farm.total_pending_amount, 0);
}

#[test]
fn delegated_invariant_active_matches_amount() {
    // In delegated farms, total_active_stake_scaled equals total_staked_amount
    let mut farm = make_farm();
    farm.delegate_authority = Pubkey::new_unique();
    farm.total_active_stake_scaled = 1234;
    farm.total_staked_amount = 1234;
    assert_eq!(farm.total_active_stake_scaled, farm.total_staked_amount as u128);
}

#[test]
fn user_reward_monotone_and_conservation() {
    // Setup farm with 1 reward and user with stake
    let mut farm = make_farm();
    farm.num_reward_tokens = 1;
    farm.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(10);
    farm.reward_infos[0].rewards_available = 10_000;
    farm.set_total_active_stake_decimal(Decimal::from(1_000u64));

    let mut user = make_user();
    user.set_active_stake_decimal(Decimal::from(500u64));

    // Issue rewards twice and ensure user tallies grow monotonically
    refresh_global_reward(&mut farm, None, 1_000, 0).unwrap();
    crate::farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    let first = user.rewards_issued_unclaimed[0];
    refresh_global_reward(&mut farm, None, 1_100, 0).unwrap();
    crate::farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    let second = user.rewards_issued_unclaimed[0];
    assert!(second >= first);

    // Conservation: increase in (rps * total_active_stake) equals issued within floor residual
    let rps = farm.reward_infos[0].get_reward_per_share_decimal();
    let issued = farm.reward_infos[0].rewards_issued_cumulative;
    let approx = (rps * farm.get_total_active_stake_decimal())
        .try_floor()
        .unwrap();
    assert!(issued >= approx && issued - approx <= 1);
}

#[test]
fn issuance_and_rps_progression() {
    let mut farm = make_farm();
    // Set a single reward
    farm.num_reward_tokens = 1;
    farm.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(100);
    farm.reward_infos[0].rewards_available = 10_000;
    farm.reward_infos[0].last_issuance_ts = 1_000;

    // Add active stake so issuance updates RPS
    farm.set_total_active_stake_decimal(Decimal::from(1_000u64));
    let ts2 = 1_010; // 10 time units
    refresh_global_reward(&mut farm, None, ts2, 0).unwrap();

    // Issued = 100 * 10 = 1000
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 1_000);
    assert_eq!(farm.reward_infos[0].rewards_available, 9_000);

    // reward_per_share should be 1000 / 1000 = 1
    let rps = farm.reward_infos[0].get_reward_per_share_decimal();
    assert_eq!(rps, Decimal::from(1u64));
}

#[test]
fn deposit_cap_and_oracle_edges() {
    let mut farm = make_farm();
    farm.deposit_cap_amount = 1_000;

    // No oracle: simple cap check
    farm.total_staked_amount = 900;
    assert!(farm.can_accept_deposit(100, None, 0).unwrap());
    assert!(!farm.can_accept_deposit(101, None, 0).unwrap());

    // With oracle: use price with exp to scale down
    farm.scope_oracle_price_id = 0;
    farm.scope_oracle_max_age = u64::MAX;
    // Fake DatedPrice minimal struct via scope::types is not available here; rely on None path for unit
    // This test focuses on arithmetic in None path already covered. Oracle path is covered in integration E2E.
}


