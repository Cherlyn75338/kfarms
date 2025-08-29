#![allow(clippy::unwrap_used)]

use super::*;
use decimal_wad::decimal::Decimal;
use proptest::prelude::*;

fn setup_farm(ts: u64, rps: u64, reward_type: RewardType) -> (FarmState, UserState) {
    let mut farm = FarmState::default();
    farm.time_unit = TimeUnit::Seconds as u8;
    farm.total_staked_amount = 1;
    farm.set_total_active_stake_decimal(Decimal::from(1u64));
    farm.num_reward_tokens = 1;
    farm.reward_infos[0].reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    farm.reward_infos[0].last_issuance_ts = ts;
    farm.reward_infos[0].rewards_per_second_decimals = 0;
    farm.reward_infos[0].reward_type = reward_type as u8;
    farm.reward_infos[0].rewards_available = u64::MAX;

    let mut user = UserState::default();
    user.set_active_stake_decimal(Decimal::from(1u64));
    user.last_claim_ts[0] = ts;

    (farm, user)
}

proptest! {
    #[test]
    fn prop_rpt_monotone(dt in 1u64..1000, steps in 1usize..20) {
        let start = 1_000_000u64;
        let (mut farm, mut user) = setup_farm(start, 13, RewardType::Proportional);

        let mut t = start;
        let mut last_rpt = farm.reward_infos[0].get_reward_per_share_decimal();
        for _ in 0..steps {
            t = t.saturating_add(dt);
            farm_operations::refresh_global_rewards(&mut farm, None, t).unwrap();
            let rpt = farm.reward_infos[0].get_reward_per_share_decimal();
            prop_assert!(rpt >= last_rpt);
            last_rpt = rpt;

            // user reward is non-negative
            let before = user.rewards_issued_unclaimed[0];
            farm_operations::user_refresh_reward(&mut farm, &mut user, 0).unwrap();
            let delta = user.rewards_issued_unclaimed[0] - before;
            prop_assert!(delta >= 0);
        }
    }
}

#[test]
fn poc_time_warp_overpay_depletes_available() {
    // No clamp: very large dt issues a massive amount immediately
    let start = 2_000_000u64;
    let (mut farm, _user) = setup_farm(start, 1_000_000, RewardType::Proportional);
    farm.reward_infos[0].rewards_available = 10_000_000; // finite

    // Warp ahead a long time
    let warp = start + 10_000; // 10k seconds at 1e6 rps = 1e10 requested
    farm_operations::refresh_global_rewards(&mut farm, None, warp).unwrap();

    // Without clamp, issuance is capped only by rewards_available and drains it
    assert_eq!(farm.reward_infos[0].rewards_available, 0);
    assert_eq!(farm.reward_infos[0].rewards_issued_cumulative, 10_000_000);
}

#[test]
#[should_panic]
fn poc_overflow_amplification_constant_reward_panics() {
    // Construct parameters to overflow the final u64 cast
    let start = 100u64;
    let (mut farm, _user) = setup_farm(start, u64::MAX / 2, RewardType::Constant);
    farm.total_staked_amount = u64::MAX; // amplify
    farm.reward_infos[0].rewards_per_second_decimals = 0; // no scaling down

    // Large dt multiplies into u128 and then unwrap() to u64 should panic
    let warp = start + 3;
    let _ = farm_operations::refresh_global_rewards(&mut farm, None, warp);
}

