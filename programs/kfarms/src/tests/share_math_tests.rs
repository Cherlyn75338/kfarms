#[cfg(test)]
mod tests {
    use decimal_wad::decimal::Decimal;
    use crate::stake_operations::{convert_amount_to_stake, convert_stake_to_amount};

    #[test]
    fn test_share_amount_rounding() {
        let total_stake = Decimal::from(1000u64);
        let total_amount = 1000u64;

        // Identity
        assert_eq!(convert_stake_to_amount(Decimal::from(10u64), total_stake, total_amount, false), 10);
        assert_eq!(convert_amount_to_stake(10, total_stake, total_amount), Decimal::from(10u64));

        // Round down
        let amt = convert_stake_to_amount(Decimal::from_scaled_val(1500u128), Decimal::from_scaled_val(10_000u128), 3, false);
        assert_eq!(amt, 0);

        // Round up
        let amt_up = convert_stake_to_amount(Decimal::from_scaled_val(1500u128), Decimal::from_scaled_val(10_000u128), 3, true);
        assert_eq!(amt_up, 1);
    }
}

