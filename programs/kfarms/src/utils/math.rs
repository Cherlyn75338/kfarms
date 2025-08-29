use decimal_wad::{
    common::WAD,
    decimal::{Decimal, U192},
    rate::U128,
};

#[allow(clippy::assign_op_pattern)]
mod big_ints {
    use uint::construct_uint;
    construct_uint! {pub struct U256(4);}
}

use big_ints::U256;

pub fn ten_pow(x: usize) -> u64 {
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

    if x > 19 {
        panic!("The exponent must be between 0 and 19.");
    }

    POWERS_OF_TEN[x]
}

impl From<U192> for U256 {
    fn from(val: U192) -> Self {
        U256([val.0[0], val.0[1], val.0[2], 0])
    }
}

impl TryFrom<U256> for U192 {
    type Error = ();

    fn try_from(val: U256) -> Result<Self, Self::Error> {
        if val.0[3] > 0 {
            Err(())
        } else {
            Ok(U192([val.0[0], val.0[1], val.0[2]]))
        }
    }
}

pub fn full_decimal_mul_div(a: Decimal, b: u64, c: Decimal) -> Decimal {
    let a_scaled: U192 = a.0;
    let c_scaled: U192 = c.0;

    let a_scaled_bigint: U256 = a_scaled.into();
    let c_scaled_bigint: U256 = c_scaled.into();

    let wad_big_int: U256 = WAD.into();

    let numerator = a_scaled_bigint * wad_big_int * b;
    let result_scaled_bigint = numerator / c_scaled_bigint;

    let result_scaled: U192 = result_scaled_bigint
        .try_into()
        .expect("full_decimal_mul_div overflow");

    Decimal::from_scaled_val(result_scaled)
}

pub fn u64_mul_div(a: u64, b: u64, c: u64) -> u64 {
    let a: U128 = a.into();
    let b: U128 = b.into();

    let numerator = a * b;
    let result = numerator / c;
    result.try_into().expect("u64_mul_div overflow")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn ten_pow_within_bounds() {
        assert_eq!(ten_pow(0), 1);
        assert_eq!(ten_pow(1), 10);
        assert_eq!(ten_pow(9), 1_000_000_000);
        assert_eq!(ten_pow(19), 10_000_000_000_000_000_000);
    }

    #[test]
    #[should_panic]
    fn ten_pow_out_of_bounds_panics() {
        let _ = ten_pow(20);
    }

    #[test]
    fn u64_mul_div_basic_cases() {
        assert_eq!(u64_mul_div(100, 50, 100), 50);
        assert_eq!(u64_mul_div(100, 1, 3), 33); // floor division
        assert_eq!(u64_mul_div(u64::MAX, 1, u64::MAX), 1);
        assert_eq!(u64_mul_div(0, 123, 7), 0);
    }

    #[test]
    fn full_decimal_mul_div_proportion() {
        // 50/200 of 1000 = 250
        let stake = Decimal::from(50u64);
        let total_amount = 1000u64;
        let total_stake = Decimal::from(200u64);

        let out = full_decimal_mul_div(stake, total_amount, total_stake);
        assert_eq!(out.try_floor::<u64>().unwrap(), 250u64);
        assert_eq!(out.try_ceil::<u64>().unwrap(), 250u64);
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    // Fuzz monotonicity and bounds for u64_mul_div
    proptest! {
        #[test]
        fn u64_mul_div_monotone_in_a(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX, c in 1u64..=u64::MAX) {
            // Avoid cases where a*b/c would overflow u64
            let lhs = (a as u128) * (b as u128);
            let rhs = (c as u128) * (u64::MAX as u128);
            prop_assume!(lhs <= rhs);
            let r1 = u64_mul_div(a, b, c);
            let r2 = u64_mul_div(a.saturating_add(1), b, c);
            prop_assert!(r2 >= r1);
        }

        #[test]
        fn u64_mul_div_monotone_in_b(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX, c in 1u64..=u64::MAX) {
            let lhs = (a as u128) * (b as u128);
            let rhs = (c as u128) * (u64::MAX as u128);
            prop_assume!(lhs <= rhs);
            let r1 = u64_mul_div(a, b, c);
            let r2 = u64_mul_div(a, b.saturating_add(1), c);
            prop_assert!(r2 >= r1);
        }

        #[test]
        fn u64_mul_div_upper_bound(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX, c in 1u64..=u64::MAX) {
            let lhs = (a as u128) * (b as u128);
            let rhs = (c as u128) * (u64::MAX as u128);
            prop_assume!(lhs <= rhs);
            let r = u64_mul_div(a, b, c);
            // floor(a*b/c) <= a*b/c <= max(a,b) * (min(a,b)/c) + ...
            // But easy bound: r <= max(a,b) when c >= min(a,b). Not always true; use safe bound:
            // r*c <= a*b
            let lhs = (r as u128) * (c as u128);
            let rhs = (a as u128) * (b as u128);
            prop_assert!(lhs <= rhs);
        }
    }
}
