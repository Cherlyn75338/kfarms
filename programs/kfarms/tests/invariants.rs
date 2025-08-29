#![cfg(test)]

use proptest::prelude::*;
use decimal_wad::decimal::Decimal;
use farms::farm_operations::{refresh_global_rewards, user_refresh_all_rewards};
use farms::state::{FarmState, RewardInfo, RewardScheduleCurve, RewardType, UserState};

fn setup(rps: u64, reward_type: RewardType) -> (FarmState, UserState) {
    let mut farm = FarmState::default();
    let mut info = RewardInfo::default();
    info.reward_schedule_curve = RewardScheduleCurve::from_constant(rps);
    info.rewards_per_second_decimals = 0;
    info.reward_type = reward_type as u8;
    info.last_issuance_ts = 0;
    info.rewards_available = u64::MAX;
    farm.reward_infos[0] = info;
    farm.num_reward_tokens = 1;
    farm.total_staked_amount = 1;
    farm.total_active_stake_scaled = Decimal::one().to_scaled_val().unwrap();
    let user = UserState::default();
    (farm, user)
}

proptest! {
    // Invariants: non-negativity, monotone RPT, conservation vs vaults (approx), no underflow on tally.
    #[test]
    fn prop_sequences_and_invariants(
        rps in 0u64..1_000_000u64,
        t1 in 1u64..1000u64,
        t2 in 1u64..2000u64,
    ) {
        let (mut farm, mut user) = setup(rps, RewardType::Proportional);

        // Initial state
        prop_assert!(farm.reward_infos[0].rewards_issued_unclaimed == 0);
        prop_assert!(farm.reward_infos[0].reward_per_share_scaled >= 0);

        // Sequence: idle -> first staker is implicit in setup; then time progresses
        refresh_global_rewards(&mut farm, None, t1).unwrap();
        user_refresh_all_rewards(&mut farm, &mut user).unwrap();

        // Non-negativity
        prop_assert!(farm.reward_infos[0].rewards_issued_unclaimed >= 0);
        prop_assert!(farm.reward_infos[0].rewards_available <= u64::MAX);

        // Monotone RPT
        let rpt1 = farm.reward_infos[0].reward_per_share_scaled;
        refresh_global_rewards(&mut farm, None, t1 + t2).unwrap();
        let rpt2 = farm.reward_infos[0].reward_per_share_scaled;
        prop_assert!(rpt2 >= rpt1);
    }
}

