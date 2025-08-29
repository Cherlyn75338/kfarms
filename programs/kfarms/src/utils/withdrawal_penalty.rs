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
    use crate::stake_operations::convert_stake_to_amount;
    use crate::utils::math::{ten_pow, u64_mul_div};
    use decimal_wad::decimal::Decimal;

    #[test]
    fn test_zero_treasury_fee_thresholds() {
        // fee = floor(amount * fee_bps / 10000)
        // 2.5% (250 bps): zero for amount <= 39, 40 -> 1
        let fee_bps_25 = 250u64;
        assert_eq!(u64_mul_div(39, fee_bps_25, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(40, fee_bps_25, BPS_DIV_FACTOR), 1);

        // 5% (500 bps): zero for amount <= 19, 20 -> 1
        let fee_bps_5 = 500u64;
        assert_eq!(u64_mul_div(19, fee_bps_5, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(20, fee_bps_5, BPS_DIV_FACTOR), 1);

        // Token denomination clarification for WSOL (9 decimals):
        // 39 base units -> zero fee; 39 SOL (39e9 base units) -> definitely pays fee
        let wsol_decimals = 9usize;
        let one_sol = ten_pow(wsol_decimals);
        let thirty_nine_sol = 39u64.saturating_mul(one_sol);
        assert!(u64_mul_div(thirty_nine_sol, fee_bps_25, BPS_DIV_FACTOR) > 0);
        assert_eq!(u64_mul_div(39, fee_bps_25, BPS_DIV_FACTOR), 0);
    }

    #[test]
    fn test_zero_penalty_threshold_and_splitting() {
        // At the beginning of lock: effective penalty_bps = full configured penalty_bps
        let penalty_bps = 500u64; // 5%
        let locking_start = 0u64;
        let locking_duration = 1000u64;
        let now = locking_start; // very early, full penalty applies

        // Threshold: unstake_amount < 10000 / 500 = 20 base units for zero penalty
        let (net_19, pen_19) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            19,
        )
        .unwrap();
        assert_eq!(pen_19, 0);
        assert_eq!(net_19, 19);

        let (net_20, pen_20) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            20,
        )
        .unwrap();
        assert_eq!(pen_20, 1);
        assert_eq!(net_20, 19);

        // Splitting a large unstake into many sub-threshold chunks avoids penalties
        // Example: total 19_000 split into 1000x 19 -> total penalty 0 vs single-unstake penalty 950
        let chunks = 1000u64;
        let chunk_amount = 19u64;
        let mut total_penalty_split = 0u64;
        let mut total_net_split = 0u64;
        for _ in 0..chunks {
            let (net, pen) = apply_early_withdrawal_penalty(
                locking_duration,
                locking_start,
                now,
                penalty_bps,
                chunk_amount,
            )
            .unwrap();
            total_penalty_split += pen;
            total_net_split += net;
        }
        assert_eq!(total_penalty_split, 0);
        assert_eq!(total_net_split, chunks * chunk_amount);

        // Single large unstake
        let total_amount = chunks * chunk_amount; // 19_000
        let (_, pen_single) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            total_amount,
        )
        .unwrap();
        assert_eq!(pen_single, u64_mul_div(total_amount, penalty_bps, BPS_DIV_FACTOR));
        assert_eq!(pen_single, 950);
    }

    #[test]
    fn test_time_scaled_penalty_threshold() {
        // Midway through: penalty_bps_eff = floor(500 * 50 / 100) = 250 (2.5%)
        let penalty_bps = 500u64;
        let locking_start = 0u64;
        let locking_duration = 100u64;
        let now = 50u64;

        let (net_39, pen_39) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            39,
        )
        .unwrap();
        assert_eq!(pen_39, 0); // threshold for 250 bps is < 40
        assert_eq!(net_39, 39);

        let (net_40, pen_40) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            40,
        )
        .unwrap();
        assert_eq!(pen_40, 1);
        assert_eq!(net_40, 39);
    }

    #[test]
    fn test_convert_stake_to_amount_flooring_behavior() {
        // 50% of the stake over an odd total_amount floors down when round_up=false
        let total_stake = Decimal::from(1000u64);
        let user_stake = Decimal::from(500u64);
        let total_amount = 101u64;
        let amount_floor = convert_stake_to_amount(user_stake, total_stake, total_amount, false);
        assert_eq!(amount_floor, 50);

        // If we were to round up, it would become 51 (not used in actual unstake path)
        let amount_ceil = convert_stake_to_amount(user_stake, total_stake, total_amount, true);
        assert_eq!(amount_ceil, 51);
    }

    #[test]
    fn test_large_amounts_pay_fees_and_penalties() {
        // Treasury fee examples
        let fee_bps_25 = 250u64; // 2.5%
        let wsol_decimals = 9usize;
        let one_sol = ten_pow(wsol_decimals); // 1e9 base units
        let fee_one_sol = u64_mul_div(one_sol, fee_bps_25, BPS_DIV_FACTOR);
        assert!(fee_one_sol > 0);

        let thirty_nine_sol = 39u64.saturating_mul(one_sol);
        let fee_thirty_nine_sol = u64_mul_div(thirty_nine_sol, fee_bps_25, BPS_DIV_FACTOR);
        assert!(fee_thirty_nine_sol > 0);

        // Early withdrawal penalty example near maturity
        // Effective penalty_bps ~ floor(500 * 1 / 100) = 5 bps when 1% time remains
        let penalty_bps = 500u64;
        let locking_start = 0u64;
        let locking_duration = 100u64;
        let now = 99u64; // 1 tick remaining
        let (_, pen) = apply_early_withdrawal_penalty(
            locking_duration,
            locking_start,
            now,
            penalty_bps,
            one_sol,
        )
        .unwrap();
        assert!(pen > 0);
    }
}
