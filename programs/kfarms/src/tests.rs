#![allow(clippy::float_cmp)]

use super::*;
use crate::farm_operations::{
    harvest as fo_harvest,
    initialize_reward as fo_initialize_reward,
    refresh_global_rewards as fo_refresh_global_rewards,
    unstake as fo_unstake,
    stake as fo_stake,
    user_refresh_reward as fo_user_refresh_reward,
    user_refresh_state as fo_user_refresh_state,
    withdraw_from_farm_vault as fo_withdraw_from_farm_vault,
    withdraw_reward as fo_withdraw_reward,
    withdraw_unstaked_deposits as fo_withdraw_unstaked_deposits,
};
use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;
use proptest::prelude::*;

fn default_farm(time_unit: TimeUnit) -> FarmState {
    let mut f = FarmState::default();
    f.time_unit = time_unit as u8;
    f
}

fn add_reward_basic(f: &mut FarmState, mint: Pubkey) {
    let now = 1000u64;
    fo_initialize_reward(
        f,
        Pubkey::new_unique(),
        mint,
        6,
        Pubkey::new_unique(),
        now,
    )
    .unwrap();
}

fn setup_constant_rps(f: &mut FarmState, rps: u64, reward_type: RewardType, rps_decimals: u8) {
    let idx = 0usize;
    let ri = &mut f.reward_infos[idx];
    ri.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    ri.reward_type = reward_type as u8;
    ri.rewards_per_second_decimals = rps_decimals;
}

#[test]
fn issuance_skips_when_p_tot_zero() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    setup_constant_rps(&mut f, 100, RewardType::Proportional, 0);
    f.reward_infos[0].rewards_available = 1_000_000;

    // total_active_stake_scaled == 0 => only last_issuance_ts updates
    let last = f.reward_infos[0].last_issuance_ts;
    fo_refresh_global_rewards(&mut f, None, last + 10).unwrap();
    assert_eq!(f.reward_infos[0].rewards_issued_unclaimed, 0);
    assert_eq!(f.reward_infos[0].reward_per_share_scaled, 0);
    assert_eq!(f.reward_infos[0].last_issuance_ts, last + 10);
}

#[test]
fn warmup_and_cooldown_paths() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    setup_constant_rps(&mut f, 0, RewardType::Proportional, 0);
    f.deposit_warmup_period = 10;
    f.withdrawal_cooldown_period = 20;

    let mut u = UserState::default();

    // stake with warmup -> goes to pending
    fo_stake(&mut f, &mut u, None, 1_000, 1_000).unwrap();
    assert!(u.pending_deposit_stake_scaled > 0);
    assert_eq!(u.pending_deposit_stake_ts, 1_010);

    // activating pending after warmup
    fo_user_refresh_state(&mut f, &mut u, None, 1_011).unwrap();
    assert_eq!(u.pending_deposit_stake_scaled, 0);
    assert!(u.active_stake_scaled > 0);

    // request unstake -> pending withdrawal enforced with cooldown
    let to_unstake = Decimal::from_scaled_val(u.active_stake_scaled);
    fo_unstake(&mut f, &mut u, None, to_unstake, 1_012).unwrap();
    assert!(u.pending_withdrawal_unstake_scaled > 0);
    assert_eq!(u.pending_withdrawal_unstake_ts, 1_032);

    // cannot withdraw before cooldown
    assert!(fo_withdraw_unstaked_deposits(&mut f, &mut u, 1_031).is_err());
    // can withdraw at or after
    assert!(fo_withdraw_unstaked_deposits(&mut f, &mut u, 1_032).is_ok());
}

#[test]
fn locking_modes_and_penalties() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    setup_constant_rps(&mut f, 0, RewardType::Proportional, 0);

    let mut u = UserState::default();
    fo_stake(&mut f, &mut u, None, 10_000, 1_000).unwrap();
    fo_user_refresh_state(&mut f, &mut u, None, 1_000).unwrap();

    // None: no penalty
    f.locking_mode = LockingMode::None as u64;
    f.withdrawal_cooldown_period = 0;
    let s_all = Decimal::from_scaled_val(u.active_stake_scaled);
    fo_unstake(&mut f, &mut u, None, s_all, 1_001).unwrap();
    // reset state
    u = UserState::default();
    fo_stake(&mut f, &mut u, None, 10_000, 2_000).unwrap();
    fo_user_refresh_state(&mut f, &mut u, None, 2_000).unwrap();

    // WithExpiry: early penalty enforced
    f.locking_mode = LockingMode::WithExpiry as u64;
    f.locking_start_timestamp = 2_000;
    f.locking_duration = 100;
    f.locking_early_withdrawal_penalty_bps = 1_000; // 10%
    let s = Decimal::from_scaled_val(u.active_stake_scaled);
    fo_unstake(&mut f, &mut u, None, s, 2_010).unwrap();
    assert!(f.slashed_amount_current > 0);
}

#[test]
fn reward_types_and_rps_decimals() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    f.total_staked_amount = 1_000;
    f.set_total_active_stake_decimal(Decimal::from(1_000u64));
    f.num_reward_tokens = 1;
    f.reward_infos[0].rewards_available = 1_000_000;

    // Proportional
    setup_constant_rps(&mut f, 100, RewardType::Proportional, 0);
    let last = f.reward_infos[0].last_issuance_ts;
    fo_refresh_global_rewards(&mut f, None, last + 10).unwrap();
    assert_eq!(f.reward_infos[0].rewards_issued_cumulative, 1000);

    // Constant with decimals scaling
    setup_constant_rps(&mut f, 100, RewardType::Constant, 2);
    let last = f.reward_infos[0].last_issuance_ts;
    let prev = f.reward_infos[0].rewards_issued_cumulative;
    fo_refresh_global_rewards(&mut f, None, last + 10).unwrap();
    // amount = rps*dt*total_staked / 10^dec = 100*10*1000/100 = 10_000
    let delta = f.reward_infos[0].rewards_issued_cumulative - prev;
    assert_eq!(delta, 10_000);
}

#[test]
fn reward_schedule_curve_multi_points_and_mid_epoch() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    f.total_staked_amount = 1;
    f.set_total_active_stake_decimal(Decimal::from(1u64));
    f.num_reward_tokens = 1;
    f.reward_infos[0].rewards_available = u64::MAX / 2;

    let base = f.reward_infos[0].last_issuance_ts;
    let pts = [
        RewardPerTimeUnitPoint::new(base, 1),
        RewardPerTimeUnitPoint::new(base + 10, 3),
        RewardPerTimeUnitPoint::new(base + 20, 0),
        RewardPerTimeUnitPoint::new(u64::MAX, 0),
    ];
    f.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_points(&pts).unwrap();

    let last = f.reward_infos[0].last_issuance_ts;
    fo_refresh_global_rewards(&mut f, None, last + 5).unwrap(); // 5*1 = 5
    fo_refresh_global_rewards(&mut f, None, last + 15).unwrap(); // + (10..15)*3 = 15
    assert_eq!(f.reward_infos[0].rewards_issued_cumulative, 25);
}

#[test]
fn harvest_min_claim_duration_and_zero_available() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    let mut u = UserState::default();
    f.num_reward_tokens = 1;
    f.reward_infos[0].min_claim_duration_seconds = 100;
    f.total_staked_amount = 1;
    f.set_total_active_stake_decimal(Decimal::from(1u64));
    f.reward_infos[0].rewards_available = 10_000;
    f.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(10);

    fo_stake(&mut f, &mut u, None, 1, 1_000).unwrap();
    fo_user_refresh_state(&mut f, &mut u, None, 1_010).unwrap();
    // accrue some and do an initial harvest to set last_claim_ts
    fo_refresh_global_rewards(&mut f, None, 1_050).unwrap();
    fo_user_refresh_reward(&mut f, &mut u, 0).unwrap();
    let g = GlobalConfig::default();
    let _ = fo_harvest(&mut f, &mut u, &g, None, 0, 1_050).unwrap();
    // too early for next harvest (min_claim_duration_seconds=100)
    assert_eq!(
        fo_harvest(&mut f, &mut u, &g, None, 0, 1_099).unwrap_err(),
        FarmError::MinClaimDurationNotReached.into()
    );
    // at duration
    let res = fo_harvest(&mut f, &mut u, &g, None, 0, 1_150).unwrap();
    assert!(res.reward_user > 0);
}

#[test]
fn admin_vault_withdraw_freezes_on_full() {
    let mut f = default_farm(TimeUnit::Seconds);
    f.total_staked_amount = 500;
    f.total_pending_amount = 500;
    let out = fo_withdraw_from_farm_vault(&mut f, 1_000).unwrap();
    assert_eq!(out, 1_000);
    assert_eq!(f.is_farm_frozen, 1);
}

#[test]
fn withdraw_reward_rules() {
    let mut f = default_farm(TimeUnit::Seconds);
    add_reward_basic(&mut f, Pubkey::new_unique());
    f.num_reward_tokens = 1;
    f.reward_infos[0].rewards_available = 1_000;
    // cannot withdraw when schedule set (default curve is zero, allowed), set non-default to allow then disallow
    // first allowed (default curve is all zero and equals default())
    let mint0 = f.reward_infos[0].token.mint;
    let w = fo_withdraw_reward(&mut f, None, &mint0, 0, 100, 1_000)
        .unwrap();
    assert_eq!(w.reward_amount, 100);
    // set schedule to non-default (still zero rps but non-default structure)
    f.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(1);
    let mint_again = f.reward_infos[0].token.mint;
    assert!(fo_withdraw_reward(&mut f, None, &mint_again, 0, 1, 1_010).is_err());
}

// Property: C is monotonic increasing when P_tot > 0 and issuance occurs
proptest! {
    #[test]
    fn prop_monotonic_c(rewards in 1u64..1000, stake in 1u64..1000, dt1 in 1u64..100, dt2 in 1u64..100) {
        let mut f = default_farm(TimeUnit::Seconds);
        add_reward_basic(&mut f, Pubkey::new_unique());
        f.num_reward_tokens = 1;
        f.total_staked_amount = stake;
        f.set_total_active_stake_decimal(Decimal::from(stake));
        f.reward_infos[0].rewards_available = u64::MAX/4;
        f.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(rewards);

        let last = f.reward_infos[0].last_issuance_ts;
        fo_refresh_global_rewards(&mut f, None, last + dt1).unwrap();
        let c1 = f.reward_infos[0].reward_per_share_scaled;
        fo_refresh_global_rewards(&mut f, None, last + dt1 + dt2).unwrap();
        let c2 = f.reward_infos[0].reward_per_share_scaled;
        prop_assert!(c2 >= c1);
    }
}

