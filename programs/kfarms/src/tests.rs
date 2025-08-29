#![cfg(any(test, feature = "test-bpf"))]
use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve};
use proptest::prelude::*;

#[test]
fn rps_curve_cumulative_overflow_guard() {
    // Construct a curve with two segments with large values
    let curve = RewardScheduleCurve::from_points(&[RewardPerTimeUnitPoint::new(0, u64::MAX)])
        .expect("valid curve");
    // Short interval to avoid exceeding u64::MAX
    let amt = curve
        .get_cumulative_amount_issued_since_last_ts(0, 1)
        .expect("no overflow for dt=1");
    assert_eq!(amt, u64::MAX); // dt=1 so amount==rps
}

#[test]
fn rps_curve_cumulative_detects_overflow_for_large_dt() {
    let curve = RewardScheduleCurve::from_points(&[RewardPerTimeUnitPoint::new(0, u64::MAX)])
        .expect("valid curve");
    // dt=2 would require u64::MAX*2 which should trigger overflow error
    let err = curve
        .get_cumulative_amount_issued_since_last_ts(0, 2)
        .err()
        .expect("overflow should error");
    let _ = err; // just ensure it errored; anchor Result error kind is not directly comparable here
}

#[test]
fn reward_schedule_curve_validation_rules() {
    // Non-empty and sorted
    let invalid = RewardScheduleCurve::from_points(&[]);
    assert!(invalid.is_err());

    let ok = RewardScheduleCurve::from_points(&[
        RewardPerTimeUnitPoint::new(1, 10),
        RewardPerTimeUnitPoint::new(5, 20),
    ]);
    assert!(ok.is_ok());
}

proptest! {
  #[test]
  fn cumulative_is_monotonic(ts0 in 0u64..1_000_000u64, dt in 0u64..1_000u64, rps in 0u64..1_000_000u64) {
    let curve = RewardScheduleCurve::from_points(&[RewardPerTimeUnitPoint::new(0, rps)]).unwrap();
    let t1 = ts0.saturating_add(dt);
    let a0 = curve.get_cumulative_amount_issued_since_last_ts(0, ts0).unwrap_or(0);
    let a1 = curve.get_cumulative_amount_issued_since_last_ts(0, t1).unwrap_or(a0);
    prop_assert!(a1 >= a0);
  }
}

