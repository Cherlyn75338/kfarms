#![allow(dead_code)]

use decimal_wad::decimal::Decimal;

const BPS_DIV_FACTOR: u64 = 10_000;

fn u64_mul_div(a: u64, b: u64, c: u64) -> u64 {
    // faithful to on-chain u64_mul_div using 128-bit intermediate
    let a128 = u128::from(a);
    let b128 = u128::from(b);
    let result = a128.saturating_mul(b128) / u128::from(c);
    result as u64
}

fn ten_pow(x: usize) -> u64 {
    const POWERS_OF_TEN: [u64; 20] = [
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
    POWERS_OF_TEN[x]
}

fn apply_time_scaled_penalty_bps(
    timestamp_beginning: u64,
    timestamp_now: u64,
    timestamp_maturity: u64,
    penalty_bps: u64,
) -> Option<u64> {
    if timestamp_maturity < timestamp_beginning {
        return None;
    }
    if timestamp_now < timestamp_beginning || timestamp_now >= timestamp_maturity {
        return Some(0);
    }
    if penalty_bps > 10_000 || penalty_bps == 0 || penalty_bps == 10_000 {
        return None;
    }
    let time_remaining = timestamp_maturity - timestamp_now;
    let total_duration = timestamp_maturity - timestamp_beginning;
    Some(penalty_bps.saturating_mul(time_remaining) / total_duration)
}

fn apply_early_withdrawal_penalty(
    locking_duration: u64,
    locking_start: u64,
    timestamp_now: u64,
    penalty_bps: u64,
    unstake_amount: u64,
) -> Option<(u64, u64)> {
    let ts_maturity = locking_start + locking_duration;
    let eff_bps = apply_time_scaled_penalty_bps(locking_start, timestamp_now, ts_maturity, penalty_bps)?;
    let penalty_amount = u64_mul_div(unstake_amount, eff_bps, BPS_DIV_FACTOR);
    Some((unstake_amount - penalty_amount, penalty_amount))
}

fn convert_stake_to_amount(
    stake: Decimal,
    total_stake: Decimal,
    total_amount: u64,
    round_up: bool,
) -> u64 {
    if stake == Decimal::zero() {
        return 0;
    }
    let amount_dec = if total_stake != Decimal::zero() {
        // emulate full_decimal_mul_div: (stake * 1e18 * total_amount) / total_stake / 1e18
        let num = stake * Decimal::from(total_amount);
        num / total_stake
    } else {
        Decimal::from(total_amount)
    };
    if round_up {
        amount_dec.try_ceil().unwrap()
    } else {
        amount_dec.try_floor().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_treasury_fee_thresholds() {
        let fee_25 = 250u64;
        assert_eq!(u64_mul_div(39, fee_25, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(40, fee_25, BPS_DIV_FACTOR), 1);

        let fee_5 = 500u64;
        assert_eq!(u64_mul_div(19, fee_5, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(20, fee_5, BPS_DIV_FACTOR), 1);

        // WSOL 9 decimals
        let one_sol = ten_pow(9);
        let fee_one_sol = u64_mul_div(one_sol, fee_25, BPS_DIV_FACTOR);
        assert!(fee_one_sol > 0);
        let thirty_nine_sol = 39 * one_sol;
        assert!(u64_mul_div(thirty_nine_sol, fee_25, BPS_DIV_FACTOR) > 0);
        assert_eq!(u64_mul_div(39, fee_25, BPS_DIV_FACTOR), 0);
    }

    #[test]
    fn zero_penalty_threshold_and_splitting() {
        let penalty_bps = 500u64; // 5%
        let locking_start = 0u64;
        let locking_duration = 1000u64;
        let now = locking_start; // earliest, full penalty applies

        let (net_19, pen_19) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 19).unwrap();
        assert_eq!(pen_19, 0);
        assert_eq!(net_19, 19);

        let (net_20, pen_20) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 20).unwrap();
        assert_eq!(pen_20, 1);
        assert_eq!(net_20, 19);

        // Splitting avoids penalty
        let chunks = 1000u64;
        let chunk_amt = 19u64;
        let mut total_pen = 0u64;
        let mut total_net = 0u64;
        for _ in 0..chunks {
            let (n, p) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, chunk_amt).unwrap();
            total_pen += p;
            total_net += n;
        }
        assert_eq!(total_pen, 0);
        assert_eq!(total_net, chunks * chunk_amt);

        let total = chunks * chunk_amt; // 19_000
        let (_, pen_single) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, total).unwrap();
        assert_eq!(pen_single, u64_mul_div(total, penalty_bps, BPS_DIV_FACTOR));
        assert_eq!(pen_single, 950);
    }

    #[test]
    fn time_scaled_penalty_threshold() {
        // midway -> eff bps = floor(500 * 50 / 100) = 250
        let penalty_bps = 500u64;
        let locking_start = 0u64;
        let locking_duration = 100u64;
        let now = 50u64;

        let (net_39, pen_39) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 39).unwrap();
        assert_eq!(pen_39, 0);
        assert_eq!(net_39, 39);

        let (net_40, pen_40) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 40).unwrap();
        assert_eq!(pen_40, 1);
        assert_eq!(net_40, 39);
    }

    #[test]
    fn convert_stake_to_amount_flooring_behavior() {
        let total_stake = Decimal::from(1000u64);
        let user_stake = Decimal::from(500u64);
        let total_amount = 101u64;
        let floor_amt = convert_stake_to_amount(user_stake, total_stake, total_amount, false);
        assert_eq!(floor_amt, 50);
        let ceil_amt = convert_stake_to_amount(user_stake, total_stake, total_amount, true);
        assert_eq!(ceil_amt, 51);
    }

    #[test]
    fn large_amounts_pay_fees_and_penalties() {
        let fee_bps_25 = 250u64; // 2.5%
        let one_sol = ten_pow(9);
        assert!(u64_mul_div(one_sol, fee_bps_25, BPS_DIV_FACTOR) > 0);
        assert!(u64_mul_div(39 * one_sol, fee_bps_25, BPS_DIV_FACTOR) > 0);

        let penalty_bps = 500u64;
        let locking_start = 0u64;
        let locking_duration = 100u64;
        let now = 99u64; // 1% remaining -> eff ~ 5 bps
        let (_, pen) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, one_sol).unwrap();
        assert!(pen > 0);
    }
}

