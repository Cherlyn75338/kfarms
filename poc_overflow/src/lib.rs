#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardType {
    Proportional,
    Constant,
}

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub ts_start: u64,
    pub reward_per_time_unit: u64,
}

// Vulnerable curve calculation with intentional wrapping behavior to emulate release build
pub fn get_cumulative_amount_issued_since_last_ts_wrapping(
    points: &[Point],
    last_issued_ts: u64,
    current_ts: u64,
) -> u64 {
    assert!(last_issued_ts <= current_ts);
    assert!(!points.is_empty());

    // Find most recent point index
    let mut start_index = 0usize;
    for (i, pt) in points.iter().enumerate() {
        if pt.ts_start > last_issued_ts {
            start_index = if i > 0 { i - 1 } else { 0 };
            break;
        }
        start_index = i;
    }

    let mut cumulative_amount: u64 = 0;
    for i in start_index..points.len() {
        let pt = points[i];
        if pt.ts_start >= current_ts {
            break;
        }

        let start_ts = if pt.ts_start > last_issued_ts {
            pt.ts_start
        } else {
            last_issued_ts
        };

        let end_ts = if i < points.len() - 1 && points[i + 1].ts_start < current_ts {
            points[i + 1].ts_start
        } else {
            current_ts
        };

        let dt = end_ts - start_ts;
        // Intentional wrapping mul/add like release build
        let period_amount = pt.reward_per_time_unit.wrapping_mul(dt);
        cumulative_amount = cumulative_amount.wrapping_add(period_amount);
    }

    cumulative_amount
}

fn ten_pow_u64(x: u32) -> u64 {
    const POWERS: [u64; 20] = [
        1,
        10,
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
        100_000_000,
        1_000_000_000,
        10_000_000_000,
        100_000_000_000,
        1_000_000_000_000,
        10_000_000_000_000,
        100_000_000_000_000,
        1_000_000_000_000_000,
        10_000_000_000_000_000,
        100_000_000_000_000_000,
        1_000_000_000_000_000_000,
        10_000_000_000_000_000_000,
    ];
    assert!((x as usize) < POWERS.len());
    POWERS[x as usize]
}

// Core reward calc POC with release-like wrapping on u128
pub fn calc_amount_wrapping(
    reward_type: RewardType,
    cumulative_amt: u64,
    total_staked_amount: u64,
    rps_decimals: u8,
    oracle_enabled: bool,
    px: u64,
    factor: u64,
) -> u64 {
    let cumulative_amt_u128 = cumulative_amt as u128;

    let reward_type_amt = match reward_type {
        RewardType::Proportional => cumulative_amt_u128,
        RewardType::Constant => {
            let stake_u128 = total_staked_amount as u128;
            cumulative_amt_u128.wrapping_mul(stake_u128)
        }
    };

    let decimal_adjusted_amt = reward_type_amt / (ten_pow_u64(rps_decimals as u32) as u128);

    let oracle_adjusted_amt = if !oracle_enabled {
        decimal_adjusted_amt
    } else {
        let px_u128 = px as u128;
        let factor_u128 = factor as u128;
        decimal_adjusted_amt
            .wrapping_mul(px_u128)
            .wrapping_div(factor_u128)
    };

    // Emulate try_into().unwrap(): panic if > u64::MAX
    if oracle_adjusted_amt > u64::MAX as u128 {
        panic!("u64 conversion overflow");
    }
    oracle_adjusted_amt as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    const MOD_2_64: u128 = 1u128 << 64;

    #[test]
    fn period_wraps_with_large_rps_and_dt() {
        let pts = [Point { ts_start: 0, reward_per_time_unit: u64::MAX }];
        let last = 0u64;
        let now = 2u64; // dt = 2
        let wrapped = get_cumulative_amount_issued_since_last_ts_wrapping(&pts, last, now);
        assert_eq!(wrapped, u64::MAX - 1);
    }

    #[test]
    #[should_panic]
    fn constant_rewards_try_into_panics_when_exceeds_u64() {
        // cumulative = u64::MAX, stake = 2 -> reward_type_amt = (2^64 - 1) * 2 = 2^65 - 2 > u64::MAX
        let _ = calc_amount_wrapping(
            RewardType::Constant,
            u64::MAX,
            2,
            0,
            false,
            0,
            1,
        );
    }

    #[test]
    fn oracle_scaling_overflow_before_division_wraps_silently() {
        // Use maximal pre-oracle amount and a factor that would scale back, but multiplication overflows first
        let cumulative = u64::MAX;
        let stake = u64::MAX; // reward_type_amt ~ (2^64-1)^2 ~ near u128::MAX
        let rps_decimals = 0u8; // no reduction
        let px = u64::MAX; // very large px causes overflow before division
        let factor = u64::MAX; // intended to cancel px

        let amt = calc_amount_wrapping(
            RewardType::Constant,
            cumulative,
            stake,
            rps_decimals,
            true,
            px,
            factor,
        );

        // If there were no overflow before division, px/factor == 1 and result should be u128::MAX-ish (> u64::MAX) and panic.
        // Instead, we observe a wrapped u64 amount due to pre-division overflow and final clamp.
        assert!(amt > 0);
    }

    #[test]
    fn over_and_under_issuance_examples_with_wrap() {
        // Under-issuance: mathematically huge issuance wraps to small value
        let pts = [Point { ts_start: 0, reward_per_time_unit: u64::MAX }];
        let cum = get_cumulative_amount_issued_since_last_ts_wrapping(&pts, 0, 3); // dt=3 -> wraps
        assert_ne!(cum, 3u64.saturating_mul(u64::MAX));

        // Over-issuance/drain: if wrapped amount is still very large relative to available, min(amount, available) drains vault
        let available = 1_000_000_000_000u64;
        let wrapped_amount = u64::MAX; // simulate a huge wrapped value
        let issued = core::cmp::min(wrapped_amount, available);
        assert_eq!(issued, available); // drains faster than intended
    }

    #[test]
    fn counters_overflow_dos_scenario() {
        let rewards_issued_cumulative = u64::MAX;
        let to_issue = 1u64;
        let overflow = rewards_issued_cumulative.checked_add(to_issue);
        assert!(overflow.is_none(), "overflow triggers IntegerOverflow -> DoS");
    }

    #[test]
    fn realistic_overflow_large_remainder_drains_vault() {
        // Realistic scale: rps ~ 3e13 (pre-decimal), dt ~ 1e6 seconds (~11.6 days)
        let rps: u64 = 30_000_000_000_000;
        let dt: u64 = 1_000_000;
        let pts = [Point { ts_start: 0, reward_per_time_unit: rps }];
        let last = 0u64;
        let now = dt;

        let wrapped = get_cumulative_amount_issued_since_last_ts_wrapping(&pts, last, now);

        // Expected wrapped remainder = (rps * dt) % 2^64
        let correct_u128 = (rps as u128) * (dt as u128);
        assert!(correct_u128 > (u64::MAX as u128));
        let expected_remainder = (correct_u128 % MOD_2_64) as u64;
        assert_eq!(wrapped, expected_remainder);

        // Clamp by rewards_available drains the vault if available < wrapped
        let rewards_available: u64 = 1_000_000_000_000; // 1e12
        let issued = core::cmp::min(wrapped, rewards_available);
        assert_eq!(issued, rewards_available);
    }

    #[test]
    fn realistic_overflow_small_remainder_under_issuance() {
        // Choose rps close to floor(2^64 / dt) to make product just over the boundary
        let dt: u64 = 1_000_000;
        let near_boundary_rps = (u64::MAX as u128 + 1u128) / (dt as u128); // floor(2^64 / dt)
        let rps: u64 = (near_boundary_rps as u64).saturating_add(1);
        let pts = [Point { ts_start: 0, reward_per_time_unit: rps }];

        let wrapped = get_cumulative_amount_issued_since_last_ts_wrapping(&pts, 0, dt);
        let correct_u128 = (rps as u128) * (dt as u128);
        assert!(correct_u128 > (u64::MAX as u128));
        let expected_remainder = (correct_u128 % MOD_2_64) as u64;
        assert_eq!(wrapped, expected_remainder);

        // If remainder is small, issuance is small despite massive true amount
        assert!(wrapped < 1_000_000u64);
    }

    #[test]
    fn multi_point_add_overflow_wraps_sum() {
        // Two adjacent periods each below 2^64, but their sum exceeds 2^64
        let rps: u64 = 9_500_000_000_000; // 9.5e12
        let dt: u64 = 1_000_000; // 1e6 seconds

        // Build two points at t=0 and t=dt, so both contribute dt each when last=0 and now=2*dt
        let pts = [
            Point { ts_start: 0, reward_per_time_unit: rps },
            Point { ts_start: dt, reward_per_time_unit: rps },
        ];
        let wrapped = get_cumulative_amount_issued_since_last_ts_wrapping(&pts, 0, 2 * dt);

        let a = (rps as u128) * (dt as u128);
        let b = a;
        assert!(a < MOD_2_64);
        assert!(b < MOD_2_64);
        let sum = a + b;
        assert!(sum > (u64::MAX as u128));
        let expected_remainder = (sum % MOD_2_64) as u64;
        assert_eq!(wrapped, expected_remainder);
        assert!(wrapped > 0);
    }
}

