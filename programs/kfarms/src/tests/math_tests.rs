use crate::utils::math::{full_decimal_mul_div, ten_pow, u64_mul_div};
use decimal_wad::decimal::Decimal;

#[test]
fn test_ten_pow_valid_range() {
    assert_eq!(ten_pow(0), 1);
    assert_eq!(ten_pow(1), 10);
    assert_eq!(ten_pow(6), 1_000_000);
    assert_eq!(ten_pow(18), 1_000_000_000_000_000_000);
    assert_eq!(ten_pow(19), 10_000_000_000_000_000_000);
}

#[test]
#[should_panic(expected = "The exponent must be between 0 and 19")]
fn test_ten_pow_out_of_range() {
    ten_pow(20);
}

#[test]
fn test_u64_mul_div_basic() {
    assert_eq!(u64_mul_div(10, 20, 5), 40);
    assert_eq!(u64_mul_div(100, 50, 25), 200);
    assert_eq!(u64_mul_div(u64::MAX, 1, 2), u64::MAX / 2);
}

#[test]
fn test_u64_mul_div_precision() {
    // Test that we maintain precision for large numbers
    let a = 1_000_000_000_000u64;
    let b = 1_000_000_000u64;
    let c = 1_000_000u64;
    assert_eq!(u64_mul_div(a, b, c), 1_000_000_000_000_000u64);
}

#[test]
#[should_panic(expected = "u64_mul_div overflow")]
fn test_u64_mul_div_overflow() {
    // This should overflow when converting back to u64
    u64_mul_div(u64::MAX, u64::MAX, 1);
}

#[test]
#[should_panic]
fn test_u64_mul_div_divide_by_zero() {
    u64_mul_div(10, 20, 0);
}

#[test]
fn test_u64_mul_div_edge_cases() {
    // Test with zero numerator
    assert_eq!(u64_mul_div(0, 100, 50), 0);
    assert_eq!(u64_mul_div(100, 0, 50), 0);
    
    // Test with denominator of 1
    assert_eq!(u64_mul_div(100, 200, 1), 20_000);
    
    // Test maximum safe multiplication
    let max_safe = (u64::MAX as u128).sqrt() as u64;
    assert_eq!(u64_mul_div(max_safe, max_safe, max_safe), max_safe);
}

#[test]
fn test_full_decimal_mul_div_basic() {
    let a = Decimal::from(100u64);
    let b = 50u64;
    let c = Decimal::from(25u64);
    
    let result = full_decimal_mul_div(a, b, c);
    assert_eq!(result, Decimal::from(200u64));
}

#[test]
fn test_full_decimal_mul_div_precision() {
    // Test with decimal values
    let a = Decimal::from(1_000_000u64) / 1000; // 1000.0
    let b = 500u64;
    let c = Decimal::from(250u64);
    
    let result = full_decimal_mul_div(a, b, c);
    let expected = Decimal::from(2_000u64); // (1000 * 500) / 250 = 2000
    
    // Allow for small rounding differences
    let diff = if result > expected {
        result - expected
    } else {
        expected - result
    };
    assert!(diff < Decimal::from(1u64));
}

#[test]
#[should_panic(expected = "full_decimal_mul_div overflow")]
fn test_full_decimal_mul_div_overflow() {
    let max_decimal = Decimal::from(u64::MAX) * Decimal::from(u64::MAX);
    let b = u64::MAX;
    let c = Decimal::from(1u64);
    
    full_decimal_mul_div(max_decimal, b, c);
}

#[test]
fn test_full_decimal_mul_div_zero_handling() {
    let a = Decimal::zero();
    let b = 100u64;
    let c = Decimal::from(50u64);
    
    assert_eq!(full_decimal_mul_div(a, b, c), Decimal::zero());
    
    let a = Decimal::from(100u64);
    let b = 0u64;
    let c = Decimal::from(50u64);
    
    assert_eq!(full_decimal_mul_div(a, b, c), Decimal::zero());
}

#[test]
fn test_math_consistency() {
    // Verify that u64_mul_div and full_decimal_mul_div produce consistent results
    let test_cases = vec![
        (1000u64, 500u64, 250u64),
        (u64::MAX / 2, 2, 4),
        (1_000_000, 1_000, 100),
    ];
    
    for (a, b, c) in test_cases {
        let u64_result = u64_mul_div(a, b, c);
        let decimal_result = full_decimal_mul_div(
            Decimal::from(a),
            b,
            Decimal::from(c)
        );
        
        assert_eq!(
            u64_result,
            decimal_result.try_floor().unwrap(),
            "Results should match for a={}, b={}, c={}",
            a, b, c
        );
    }
}