use crate::{xmsg, FarmError};

use super::{consts::BPS_DIV_FACTOR, math::u64_mul_div};

fn get_withdrawal_penalty_bps(
    timestamp_beginning: u64,
    timestamp_now: u64,
    timestamp_maturity: u64,
    penalty_bps: u64,
) -> Result<u64, FarmError> {
    if timestamp_maturity < timestamp_beginning {
        return Err(FarmError::InvalidLockingTimestamps);
    }

    if timestamp_now < timestamp_beginning {
        xmsg!(
            "timestamp_now < timestamp_beginning where the user withdraws before
            the official locking period starts in the case of a \"WithExpiry\" locking mode"
        );
        return Ok(0);
    }

    if timestamp_now >= timestamp_maturity {
        xmsg!(
            "Time has passed, can unstake as usual ts_now={:?} ts_maturity={:?}",
            timestamp_now,
            timestamp_maturity
        );
        return Ok(0);
    }

    if penalty_bps > 10000 {
        xmsg!("Penalty percentage is greater than 1000");
        return Err(FarmError::InvalidPenaltyPercentage);
    }

    if penalty_bps == 0 || penalty_bps == 10000 {
        xmsg!("Penalty percentage is 0 or 100, therefore early withdrawal is not allowed");
        return Err(FarmError::EarlyWithdrawalNotAllowed);
    }

    let time_remaining = timestamp_maturity - timestamp_now;

    let total_duration = timestamp_maturity - timestamp_beginning;

    let penalty = penalty_bps * time_remaining / total_duration;

    Ok(penalty)
}

pub fn apply_early_withdrawal_penalty(
    locking_duration: u64,
    locking_start: u64,
    timestamp_now: u64,
    penalty_bps: u64,
    unstake_amount: u64,
) -> Result<(u64, u64), FarmError> {
    let timestamp_maturity = locking_start + locking_duration;
    let timestamp_beginning = locking_start;

    let penalty_bps = get_withdrawal_penalty_bps(
        timestamp_beginning,
        timestamp_now,
        timestamp_maturity,
        penalty_bps,
    )?;

    let penalty_amount = u64_mul_div(unstake_amount, penalty_bps, BPS_DIV_FACTOR);

    Ok((unstake_amount - penalty_amount, penalty_amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_penalty_after_maturity() {
        let duration = 100;
        let start = 1_000;
        let now = start + duration;
        let (amt, pen) = apply_early_withdrawal_penalty(duration, start, now, 500, 1_000).unwrap();
        assert_eq!(pen, 0);
        assert_eq!(amt, 1_000);
    }

    #[test]
    fn full_linear_penalty_midway() {
        let duration = 1000;
        let start = 10_000;
        let now = start + 500; // 50% remaining => 50% of penalty_bps
        let (_amt, pen) = apply_early_withdrawal_penalty(duration, start, now, 1000, 10_000).unwrap();
        // penalty_bps_eff = 1000 * (500/1000) = 500 bps => 5%
        assert_eq!(pen, 500);
    }

    #[test]
    fn zero_penalty_before_start_with_expiry() {
        let duration = 1_000;
        let start = 10_000;
        let now = start - 1; // before start -> current behavior returns 0 penalty
        let (amt, pen) = apply_early_withdrawal_penalty(duration, start, now, 500, 10_000).unwrap();
        assert_eq!(pen, 0);
        assert_eq!(amt, 10_000);
    }

    #[test]
    fn invalid_percentages_rejected() {
        let duration = 100;
        let start = 1000;
        let now = start + 1;
        let err = apply_early_withdrawal_penalty(duration, start, now, 0, 100).unwrap_err();
        assert_eq!(err, FarmError::EarlyWithdrawalNotAllowed);

        let err = apply_early_withdrawal_penalty(duration, start, now, 10000, 100).unwrap_err();
        assert_eq!(err, FarmError::EarlyWithdrawalNotAllowed);

        let err = apply_early_withdrawal_penalty(duration, start, now, 10001, 100).unwrap_err();
        assert_eq!(err, FarmError::InvalidPenaltyPercentage);
    }
}
