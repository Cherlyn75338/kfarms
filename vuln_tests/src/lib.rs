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
        println!("2.5% fee: fee(39)={}, fee(40)={}", u64_mul_div(39, fee_25, BPS_DIV_FACTOR), u64_mul_div(40, fee_25, BPS_DIV_FACTOR));
        assert_eq!(u64_mul_div(39, fee_25, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(40, fee_25, BPS_DIV_FACTOR), 1);

        let fee_5 = 500u64;
        println!("5% fee: fee(19)={}, fee(20)={}", u64_mul_div(19, fee_5, BPS_DIV_FACTOR), u64_mul_div(20, fee_5, BPS_DIV_FACTOR));
        assert_eq!(u64_mul_div(19, fee_5, BPS_DIV_FACTOR), 0);
        assert_eq!(u64_mul_div(20, fee_5, BPS_DIV_FACTOR), 1);

        // WSOL 9 decimals
        let one_sol = ten_pow(9);
        let fee_one_sol = u64_mul_div(one_sol, fee_25, BPS_DIV_FACTOR);
        println!("2.5% fee: fee(1 SOL base units={})={}", one_sol, fee_one_sol);
        assert!(fee_one_sol > 0);
        let thirty_nine_sol = 39 * one_sol;
        let fee_39_sol = u64_mul_div(thirty_nine_sol, fee_25, BPS_DIV_FACTOR);
        println!("2.5% fee: fee(39 SOL base units={})={}", thirty_nine_sol, fee_39_sol);
        assert!(fee_39_sol > 0);
        assert_eq!(u64_mul_div(39, fee_25, BPS_DIV_FACTOR), 0);
    }

    #[test]
    fn zero_penalty_threshold_and_splitting() {
        let penalty_bps = 500u64; // 5%
        let locking_start = 0u64;
        let locking_duration = 1000u64;
        let now = locking_start; // earliest, full penalty applies

        let (net_19, pen_19) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 19).unwrap();
        println!("5% early penalty at start: amount=19 -> net={}, penalty={}", net_19, pen_19);
        assert_eq!(pen_19, 0);
        assert_eq!(net_19, 19);

        let (net_20, pen_20) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 20).unwrap();
        println!("5% early penalty at start: amount=20 -> net={}, penalty={}", net_20, pen_20);
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
        println!("Splitting: {} chunks x {} -> total_net={}, total_penalty={}", chunks, chunk_amt, total_net, total_pen);
        assert_eq!(total_pen, 0);
        assert_eq!(total_net, chunks * chunk_amt);

        let total = chunks * chunk_amt; // 19_000
        let (_, pen_single) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, total).unwrap();
        println!("Single unstake: amount={} -> penalty={}", total, pen_single);
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
        println!("Midway (eff ~2.5%): amount=39 -> net={}, penalty={}", net_39, pen_39);
        assert_eq!(pen_39, 0);
        assert_eq!(net_39, 39);

        let (net_40, pen_40) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, 40).unwrap();
        println!("Midway (eff ~2.5%): amount=40 -> net={}, penalty={}", net_40, pen_40);
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
        let fee_1_sol = u64_mul_div(one_sol, fee_bps_25, BPS_DIV_FACTOR);
        let fee_39_sol = u64_mul_div(39 * one_sol, fee_bps_25, BPS_DIV_FACTOR);
        println!("2.5% fee: 1 SOL fee={}, 39 SOL fee={}", fee_1_sol, fee_39_sol);
        assert!(fee_1_sol > 0);
        assert!(fee_39_sol > 0);

        let penalty_bps = 500u64;
        let locking_start = 0u64;
        let locking_duration = 100u64;
        let now = 99u64; // 1% remaining -> eff ~ 5 bps
        let (_, pen) = apply_early_withdrawal_penalty(locking_duration, locking_start, now, penalty_bps, one_sol).unwrap();
        println!("Near maturity (~5 bps eff): 1 SOL penalty={}", pen);
        assert!(pen > 0);
    }

    #[test]
    fn profitability_analysis() {
        // Parameters
        let fee_bps_cases = [250u64, 500u64]; // 2.5%, 5%
        let tokens = [
            ("USDC", 6usize, 1.0_f64),
            ("WSOL", 9usize, 150.0_f64),
            ("ZERO_DEC_EXAMPLE", 0usize, 1.0_f64),
        ];
        let tx_fee_usd_cases = [0.00001_f64, 0.0001_f64, 0.001_f64];

        println!("=== Profitability per micro-op (USD saved per chunk vs tx fee) ===");
        for &bps in &fee_bps_cases {
            let threshold = ((BPS_DIV_FACTOR - 1) / bps) as f64; // e.g., 19 for 5%, 39 for 2.5%
            for &(sym, dec, price) in &tokens {
                let saved_per_chunk_usd = threshold * (bps as f64 / 10000.0) * price / 10f64.powi(dec as i32);
                for &tx_fee_usd in &tx_fee_usd_cases {
                    let profitable = if saved_per_chunk_usd > tx_fee_usd { "YES" } else { "NO" };
                    println!(
                        "bps={} token={} dec={} price=${} threshold={} saved_per_chunk=${:.10} tx_fee=${} PROFITABLE={}",
                        bps, sym, dec, price, threshold as u64, saved_per_chunk_usd, tx_fee_usd, profitable
                    );
                }
            }
        }

        // Example: total position size and required chunk count
        let examples = [
            ("USDC", 6usize, 1.0_f64, 1_000.0_f64),   // $1k USDC
            ("USDC", 6usize, 1.0_f64, 100_000.0_f64), // $100k USDC
            ("WSOL", 9usize, 150.0_f64, 100.0_f64),   // 100 SOL @ $150
        ];
        let bps = 500u64; // 5% penalty effective
        let threshold = ((BPS_DIV_FACTOR - 1) / bps) as u64; // 19
        println!("=== Chunk counts and total saved vs tx fees (bps=5%) ===");
        for &(sym, dec, price, position_units) in &examples {
            // Convert position to base units
            let base_units = if dec == 0 {
                position_units as u64
            } else {
                (position_units * 10f64.powi(dec as i32)) as u64
            };
            let n_chunks = (base_units + threshold - 1) / threshold;
            let saved_total_usd = (base_units as f64) * (bps as f64 / 10000.0) * price / 10f64.powi(dec as i32);
            let tx_fee_usd = 0.0001_f64; // assume $0.0001 per tx
            let total_tx_cost_usd = (n_chunks as f64) * tx_fee_usd;
            println!(
                "token={} dec={} price=${} position_units={} base_units={} chunks={} saved_total=${:.6} tx_cost=${:.6} NET=${:.6}",
                sym, dec, price, position_units, base_units, n_chunks, saved_total_usd, total_tx_cost_usd, saved_total_usd - total_tx_cost_usd
            );
        }
    }
}

