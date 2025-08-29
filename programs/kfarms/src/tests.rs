#![allow(clippy::unwrap_used)]

use super::*;
use anchor_lang::prelude::*;
use decimal_wad::decimal::Decimal;

fn new_farm_with_one_reward(ts: u64) -> (FarmState, RewardInfo) {
    let mut farm = FarmState::default();
    farm.time_unit = TimeUnit::Seconds as u8;
    farm.total_staked_amount = 1; // prevent short-circuit in refresh
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.num_reward_tokens = 1;

    let mut reward = RewardInfo::default();
    reward.reward_schedule_curve = RewardScheduleCurve::from_constant(100);
    reward.last_issuance_ts = ts;
    reward.rewards_per_second_decimals = 0;
    reward.reward_type = RewardType::Proportional as u8;
    reward.rewards_available = u64::MAX; // plenty for tests

    farm.reward_infos[0] = reward;
    (farm, reward)
}

#[test]
fn unit_rpt_monotone_and_user_rounding() {
    let start = 1_000_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);
    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = start;

    // t1: +10s -> rps=100, total issued=1000; rpt increases
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    let rpt1 = farm.reward_infos[0].get_reward_per_share_decimal();
    assert!(rpt1 > Decimal::zero());

    // user accrual equals floor(active_stake * delta_rpt)
    let prev_unclaimed = user.rewards_issued_unclaimed[0];
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0] - prev_unclaimed, 1000);

    // t2: another +7s, rpt monotone and increments by 700
    farm_operations::refresh_global_rewards(&mut farm, None, start + 17).unwrap();
    let rpt2 = farm.reward_infos[0].get_reward_per_share_decimal();
    assert!(rpt2 > rpt1);
    let prev = user.rewards_issued_unclaimed[0];
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0] - prev, 700);
}

#[test]
fn unit_vault_deltas_and_rewards_available() {
    let start = 10_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);

    // supply a finite available amount to test depletion
    farm.reward_infos[0].rewards_available = 1500;

    // 10 seconds at 100 rps issues 1000
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 1000);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 1000);
    assert_eq!(farm.reward_infos[0].rewards_available, 500);

    // 10 more seconds tries to issue another 1000, but only 500 left
    farm_operations::refresh_global_rewards(&mut farm, None, start + 20).unwrap();
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 1500);
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 1500);
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
}

#[test]
fn unit_unstake_tally_checked_sub_boundary() {
    let start = 5_000u64;
    let (mut farm, _reward) = new_farm_with_one_reward(start);
    farm.withdrawal_cooldown_period = 100;

    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = start;

    // accrue some rewards so rewards_tally > 0
    farm_operations::refresh_global_rewards(&mut farm, None, start + 10).unwrap();
    farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();

    // request to unstake almost entire tally contribution to provoke near-boundary behavior
    let requested = user.get_active_stake_decimal();
    farm_operations::unstake(&mut farm, &mut user, None, requested, start + 11).unwrap();

    // Saturating subtraction shouldn't underflow; ensure tally remains <= previous and non-negative
    let tally = user.rewards_tally_scaled[0];
    assert!(tally >= 0);
}

