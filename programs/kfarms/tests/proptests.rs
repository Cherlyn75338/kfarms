#![cfg(test)]

use proptest::prelude::*;

// Note: These are pure logic/property tests that exercise library functions without spinning up Anchor runtime.
// We mirror the Decimal math where feasible and call into internal functions via a lightweight harness.

use farms::state::{RewardPerTimeUnitPoint, RewardScheduleCurve};

proptest! {
    // Invariant: Reward per time unit curve cumulative issuance is monotone and non-negative.
    #[test]
    fn prop_curve_cumulative_monotone_nonnegative(
        start in 0u64..1_000_000u64,
        mid_offset in 0u64..1_000u64,
        end_offset in 0u64..1_000u64,
        rps1 in 0u64..1_000_000u64,
        rps2 in 0u64..1_000_000u64,
        step in 0u64..10_000u64,
    ) {
        let mid = start.saturating_add(mid_offset);
        let end = mid.saturating_add(end_offset).saturating_add(1);
        let pts = [
            RewardPerTimeUnitPoint::new(start, rps1),
            RewardPerTimeUnitPoint::new(mid, rps2),
        ];
        let curve = RewardScheduleCurve::from_points(&pts).unwrap();

        let mut last = start;
        let mut last_cum = 0u64;
        let mut t = start;
        while t <= end {
            let cum = curve.get_cumulative_amount_issued_since_last_ts(last, t).unwrap();
            // monotone: cumulative issuance since last is >= 0
            prop_assert!(cum >= 0);
            // non-decreasing wrt t: issuance over larger window is at least previous window if last fixed
            prop_assert!(cum >= last_cum);
            last_cum = cum;
            t = t.saturating_add(step.max(1));
        }
    }
}

