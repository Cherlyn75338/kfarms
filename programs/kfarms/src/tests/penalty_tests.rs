#[cfg(test)]
mod tests {
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;

    #[test]
    fn test_penalty_zero_before_start() {
        let (after, pen) = apply_early_withdrawal_penalty(100, 1_000, 900, 5000, 1_000).unwrap();
        assert_eq!(after, 1_000);
        assert_eq!(pen, 0);
    }

    #[test]
    fn test_penalty_full_after_maturity() {
        let (after, pen) = apply_early_withdrawal_penalty(100, 1_000, 1_100, 5000, 1_000).unwrap();
        assert_eq!(after, 1_000);
        assert_eq!(pen, 0);
    }

    #[test]
    fn test_penalty_midway() {
        // duration 100, now 105 -> 50% remaining; penalty_bps 5000 => 2500 bps effective
        let (after, pen) = apply_early_withdrawal_penalty(100, 1_000, 1_050, 5000, 1_000).unwrap();
        assert_eq!(after, 750);
        assert_eq!(pen, 250);
    }

    #[test]
    fn test_penalty_invalid_duration_zero() {
        let err = apply_early_withdrawal_penalty(0, 1_000, 1_000, 5000, 1_000).err().unwrap();
        // Invalid timestamps
        assert_eq!(err, crate::FarmError::InvalidLockingTimestamps.into());
    }
}

