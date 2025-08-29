#[cfg(test)]
mod tests {
    use crate::state::{RewardPerTimeUnitPoint, RewardScheduleCurve};

    #[test]
    fn test_cumulative_amount_simple() {
        let curve = RewardScheduleCurve::from_points(&[
            RewardPerTimeUnitPoint { ts_start: 0, reward_per_time_unit: 2 },
            RewardPerTimeUnitPoint { ts_start: 10, reward_per_time_unit: 1 },
        ]).unwrap();

        // From 0 to 5 -> 5*2 = 10
        let amt = curve.get_cumulative_amount_issued_since_last_ts(0, 5).unwrap();
        assert_eq!(amt, 10);

        // From 5 to 15 -> 5*2 + 5*1 = 15
        let amt = curve.get_cumulative_amount_issued_since_last_ts(5, 15).unwrap();
        assert_eq!(amt, 15);
    }

    #[test]
    fn test_cumulative_amount_overflow_guard() {
        // Large rps and duration should not overflow u64, will error instead
        let curve = RewardScheduleCurve::from_points(&[
            RewardPerTimeUnitPoint { ts_start: 0, reward_per_time_unit: u64::MAX },
        ]).unwrap();

        let err = curve.get_cumulative_amount_issued_since_last_ts(0, 2).err().unwrap();
        assert_eq!(err, crate::FarmError::IntegerOverflow.into());
    }
}

