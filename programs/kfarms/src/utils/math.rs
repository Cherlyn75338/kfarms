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
use crate::FarmError;

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

/// Multiplies two u128 values and divides by a third, avoiding intermediate overflow by
/// promoting to U256. Returns an error on division-by-zero or if the result does not fit in u128.
pub fn u128_mul_div(a: u128, b: u128, c: u128) -> Result<u128, FarmError> {
    if c == 0 {
        return Err(FarmError::MathOverflow);
    }

    let a_256: U256 = a.into();
    let b_256: U256 = b.into();
    let c_256: U256 = c.into();

    let numerator: U256 = a_256.saturating_mul(b_256);
    let result_256: U256 = numerator / c_256;

    // Ensure the result fits in u128
    let max_u128: U256 = u128::MAX.into();
    if result_256 > max_u128 {
        return Err(FarmError::IntegerOverflow);
    }

    Ok(result_256.as_u128())
}
