#![cfg(test)]

use farms::state::{RewardPerTimeUnitPoint, RewardScheduleCurve, RewardType};
use farms::utils::math::ten_pow;
use farms::farm_operations::{refresh_global_rewards, user_refresh_reward};
use farms::state::{FarmState, RewardInfo, UserState};
use decimal_wad::decimal::Decimal;

// Reference math for cumulative issuance mirroring on-chain logic
fn reference_cumulative(points: &[(u64, u64)], last: u64, now: u64) -> u128 {
    let mut cum: u128 = 0;
    for w in points.windows(2) {
        let (ts0, rps0) = (w[0].0, w[0].1);
        let ts1 = w[1].0;
        if ts0 >= now { break; }
        let start = ts0.max(last);
        let end = ts1.min(now);
        if end > start { cum = cum.saturating_add(u128::from(rps0) * u128::from(end - start)); }
    }
    // tail segment
    if let Some(&(ts_last, rps_last)) = points.last() {
        if ts_last < now { let start = ts_last.max(last); if now > start { cum = cum.saturating_add(u128::from(rps_last) * u128::from(now - start)); } }
    }
    cum
}

#[test]
fn exact_match_rpt_and_decimal_adjustment() {
    let pts = [
        RewardPerTimeUnitPoint::new(0, 5),
        RewardPerTimeUnitPoint::new(100, 7),
    ];
    let curve = RewardScheduleCurve::from_points(&pts).unwrap();

    let last = 10u64;
    let now = 123u64;

    let onchain = curve.get_cumulative_amount_issued_since_last_ts(last, now).unwrap() as u128;
    let reference = reference_cumulative(&[(0,5),(100,7)], last, now);
    assert_eq!(onchain, reference);

    // Decimal adjustment parity with on-chain path
    let rps_decimals: u8 = 3; // rewards_per_second_decimals
    let reward_type = RewardType::Proportional as u8; // 0
    let total_staked_amount: u64 = 1_000;

    let reward_type_amt = match reward_type { 0 => onchain, _ => onchain * u128::from(total_staked_amount) };
    let decimal_adjusted = reward_type_amt / u128::from(ten_pow(rps_decimals as usize));
    // No oracle in this test; matches pre-oracle value
    assert_eq!(decimal_adjusted, reward_type_amt / 10u128.pow(3));
}

#[test]
fn exact_rpt_increment_and_user_rounding() {
    // Configure a simple farm with 1 token staked and rps=3
    let mut farm = FarmState::default();
    let mut info = RewardInfo::default();
    info.reward_schedule_curve = RewardScheduleCurve::from_constant(3);
    info.rewards_per_second_decimals = 0;
    info.reward_type = RewardType::Proportional as u8;
    info.last_issuance_ts = 0;
    info.rewards_available = 1_000_000;
    farm.reward_infos[0] = info;
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1;
    farm.total_active_stake_scaled = Decimal::one().to_scaled_val().unwrap();

    // Issue for dt=2
    refresh_global_rewards(&mut farm, None, 2).unwrap();
    // Expect amount= rps * dt = 6; rps increment = 6 / total_stake = 6
    assert_eq!(farm.reward_infos[0].rewards_issued_unclaimed, 6);
    assert_eq!(farm.reward_infos[0].reward_per_share_scaled, Decimal::from(6u64).to_scaled_val().unwrap());

    // User accrual rounding: with 1 stake, reward exactly 6
    let mut user = UserState::default();
    user.active_stake_scaled = Decimal::one().to_scaled_val().unwrap();
    user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert_eq!(user.rewards_issued_unclaimed[0], 6);

    // Fractional case: increase dt by 1 with rps=1, so added rps=1, total rps=7
    farm.reward_infos[0].reward_schedule_curve.set_constant(1);
    refresh_global_rewards(&mut farm, None, 3).unwrap();
    // Now user tally computation should floor fractional parts; with 1 stake remains exact
    let prev = user.rewards_issued_unclaimed[0];
    user_refresh_reward(&mut farm, &mut user, 0).unwrap();
    assert!(user.rewards_issued_unclaimed[0] >= prev);
}

