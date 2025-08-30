#[cfg(test)]
mod tests {
    use crate::stake_operations::{convert_amount_to_stake, convert_stake_to_amount};
    use crate::utils::consts::BPS_DIV_FACTOR;
    use crate::utils::math::u64_mul_div;
    use crate::utils::withdrawal_penalty::apply_early_withdrawal_penalty;
    use decimal_wad::decimal::{Decimal, U192};

    fn zero_fee_threshold(bps: u64) -> u64 {
        // Max amount yielding zero fee: floor((10000-1)/bps)
        (BPS_DIV_FACTOR - 1) / bps
    }

    #[test]
    fn treasury_fee_zero_thresholds() {
        // 2.5% (250 bps): amounts <= 39 base units pay zero, 40 pays 1
        let fee_bps_25 = 250u64;
        let thr_25 = zero_fee_threshold(fee_bps_25);
        assert_eq!(thr_25, 39);
        assert_eq!(u64_mul_div(thr_25, fee_bps_25, BPS_DIV_FACTOR), 0);
        assert!(u64_mul_div(thr_25 + 1, fee_bps_25, BPS_DIV_FACTOR) >= 1);

        // 5% (500 bps): amounts <= 19 base units pay zero, 20 pays 1
        let fee_bps_5 = 500u64;
        let thr_5 = zero_fee_threshold(fee_bps_5);
        assert_eq!(thr_5, 19);
        assert_eq!(u64_mul_div(thr_5, fee_bps_5, BPS_DIV_FACTOR), 0);
        assert!(u64_mul_div(thr_5 + 1, fee_bps_5, BPS_DIV_FACTOR) >= 1);

        // Token denomination clarification: all math is in base units
        // WSOL (9 decimals): 39 base units => zero at 2.5%
        assert_eq!(u64_mul_div(39, fee_bps_25, BPS_DIV_FACTOR), 0);
        // 1 SOL = 1_000_000_000 base units => positive fee
        let one_sol: u64 = 1_000_000_000;
        let fee_one_sol_25 = u64_mul_div(one_sol, fee_bps_25, BPS_DIV_FACTOR);
        assert!(fee_one_sol_25 > 0);
        assert_eq!(fee_one_sol_25, (one_sol * fee_bps_25) / BPS_DIV_FACTOR);

        // 39 SOL => very large fee; check exact deterministic integer result
        let thirty_nine_sol: u64 = 39_000_000_000;
        let fee_39_sol_25 = u64_mul_div(thirty_nine_sol, fee_bps_25, BPS_DIV_FACTOR);
        assert_eq!(fee_39_sol_25, 975_000_000);

        // USDC (6 decimals): 0.000019 USDC = 19 base units => zero at 5%
        let usdc_19_base: u64 = 19; // 0.000019 USDC
        assert_eq!(u64_mul_div(usdc_19_base, fee_bps_5, BPS_DIV_FACTOR), 0);
        // 1 USDC = 1_000_000 base units => positive fee at 5%
        let one_usdc: u64 = 1_000_000;
        let fee_one_usdc_5 = u64_mul_div(one_usdc, fee_bps_5, BPS_DIV_FACTOR);
        assert_eq!(fee_one_usdc_5, 50_000);
    }

    #[test]
    fn penalty_zero_thresholds_and_time_scaling() {
        let duration = 100u64;
        let start = 0u64;
        let penalty_bps = 500u64; // 5%

        // At start (now = 0): effective bps = 500
        let now = 0u64;
        // <=19 => zero; 20 => 1
        let (_, p19) = apply_early_withdrawal_penalty(duration, start, now, penalty_bps, 19).unwrap();
        let (_, p20) = apply_early_withdrawal_penalty(duration, start, now, penalty_bps, 20).unwrap();
        assert_eq!(p19, 0);
        assert_eq!(p20, 1);

        // Halfway (now = 50): effective bps = floor(500*50/100) = 250
        let now_half = 50u64;
        let (_, p39) = apply_early_withdrawal_penalty(duration, start, now_half, penalty_bps, 39).unwrap();
        let (_, p40) = apply_early_withdrawal_penalty(duration, start, now_half, penalty_bps, 40).unwrap();
        assert_eq!(p39, 0); // <=39 base units zero at 2.5%
        assert_eq!(p40, 1);

        // Near maturity (now = 99): effective bps = floor(500*1/100) = 5
        let now_near = 99u64;
        let (_, p1999) = apply_early_withdrawal_penalty(duration, start, now_near, penalty_bps, 1_999).unwrap();
        let (_, p2000) = apply_early_withdrawal_penalty(duration, start, now_near, penalty_bps, 2_000).unwrap();
        assert_eq!(p1999, 0); // <=1999 base units zero at 5 bps
        assert_eq!(p2000, 1);
    }

    #[test]
    fn penalty_split_avoidance_saves_entire_penalty() {
        // Show that splitting into sub-threshold chunks results in near-zero aggregate penalty
        let duration = 100u64;
        let start = 0u64;
        let now = 0u64; // effective bps = 500
        let penalty_bps = 500u64; // 5%

        // Large amount in 9-decimals token, e.g., 1 SOL lamports
        let one_sol: u64 = 1_000_000_000;
        let (_, penalty_single) =
            apply_early_withdrawal_penalty(duration, start, now, penalty_bps, one_sol).unwrap();
        assert_eq!(penalty_single, 50_000_000); // 5% of 1e9

        // Any chunk <= 19 base units yields zero penalty at 5%
        let (_, penalty_chunk_19) =
            apply_early_withdrawal_penalty(duration, start, now, penalty_bps, 19).unwrap();
        assert_eq!(penalty_chunk_19, 0);

        // Therefore, splitting 1 SOL into many 19-base-unit chunks would incur ~0 aggregate penalty,
        // making the "missed" penalty up to the full single-operation penalty.
        // We assert the upper bound equals the single penalty.
        let max_missed_penalty_if_split = penalty_single;
        assert_eq!(max_missed_penalty_if_split, 50_000_000);

        // Similarly for 39 SOL at 2.5% (effective bps 250): chunk size 39 base units => zero
        let penalty_bps_25 = 250u64;
        let thirty_nine_sol: u64 = 39_000_000_000;
        let (_, penalty_single_39_sol) = apply_early_withdrawal_penalty(
            duration,
            start,
            now,
            penalty_bps_25,
            thirty_nine_sol,
        )
        .unwrap();
        assert_eq!(penalty_single_39_sol, 975_000_000); // 2.5% of 39e9

        let (_, penalty_chunk_39) =
            apply_early_withdrawal_penalty(duration, start, now, penalty_bps_25, 39).unwrap();
        assert_eq!(penalty_chunk_39, 0);
    }

    #[test]
    fn convert_shares_can_target_micro_amounts() {
        // Non-trivial pool ratio
        let total_stake = Decimal::from(123_456_789u64);
        let total_amount: u64 = 987_654_321u64;

        // Compute exact shares needed for 19 base units, then convert back with floor
        let mut shares_for_19 = convert_amount_to_stake(19, total_stake, total_amount);
        let mut amount_from_shares_19 =
            convert_stake_to_amount(shares_for_19, total_stake, total_amount, false);
        if amount_from_shares_19 < 19 {
            // Nudge shares upward by the minimum Decimal epsilon until the floored amount hits 19
            let eps = Decimal::from_scaled_val(U192([1, 0, 0]));
            for _ in 0..1_000_000 {
                shares_for_19 = shares_for_19 + eps;
                amount_from_shares_19 =
                    convert_stake_to_amount(shares_for_19, total_stake, total_amount, false);
                if amount_from_shares_19 >= 19 {
                    break;
                }
            }
        }
        assert_eq!(amount_from_shares_19, 19);

        // And for 39 base units
        let mut shares_for_39 = convert_amount_to_stake(39, total_stake, total_amount);
        let mut amount_from_shares_39 =
            convert_stake_to_amount(shares_for_39, total_stake, total_amount, false);
        if amount_from_shares_39 < 39 {
            let eps = Decimal::from_scaled_val(U192([1, 0, 0]));
            for _ in 0..1_000_000 {
                shares_for_39 = shares_for_39 + eps;
                amount_from_shares_39 =
                    convert_stake_to_amount(shares_for_39, total_stake, total_amount, false);
                if amount_from_shares_39 >= 39 {
                    break;
                }
            }
        }
        assert_eq!(amount_from_shares_39, 39);
    }
}

