#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use farms::utils::*;
use decimal_wad::decimal::Decimal;
use decimal_wad::rate::Rate;

#[derive(Debug, Arbitrary)]
struct MathInput {
    a: u128,
    b: u128,
    c: u128,
    decimals: u8,
    bps: u16,
    time_delta: u64,
}

fuzz_target!(|input: MathInput| {
    // Test full_decimal_mul_div
    let a = Decimal::from(input.a.min(u64::MAX as u128) as u64);
    let b = Decimal::from(input.b.min(u64::MAX as u128) as u64);
    let c = Decimal::from(input.c.max(1).min(u64::MAX as u128) as u64);
    
    // This should not panic
    let _ = full_decimal_mul_div(a, b, c);
    
    // Test decimal operations that should not overflow
    let _ = a.checked_add(b);
    let _ = a.checked_sub(b);
    let _ = a.checked_mul(b);
    let _ = a.checked_div(c);
    
    // Test penalty calculations
    let amount = input.a.min(u64::MAX as u128) as u64;
    let penalty_bps = input.bps.min(10000); // Cap at 100%
    
    let penalty = calculate_penalty(amount, penalty_bps as u64);
    assert!(penalty <= amount, "Penalty exceeds amount");
    
    // Test time-based calculations
    let rate = Rate::from_scaled_val(input.a.min(u128::MAX / 2) as u128);
    let time = input.time_delta;
    
    if let Ok(rate) = rate {
        let _ = calculate_rewards_for_period(rate, time);
    }
    
    // Test decimal conversions
    let decimals = input.decimals.min(18);
    let base_amount = input.a.min(10u128.pow(decimals as u32)) as u64;
    
    let scaled = scale_amount_by_decimals(base_amount, decimals);
    let unscaled = unscale_amount_by_decimals(scaled, decimals);
    
    // Verify round-trip (with potential precision loss)
    let max_precision_loss = 10u64.pow((18 - decimals as u32).min(9));
    assert!(
        unscaled <= base_amount + max_precision_loss,
        "Unexpected precision gain in round-trip"
    );
});

fn full_decimal_mul_div(a: Decimal, b: Decimal, c: Decimal) -> Option<Decimal> {
    a.checked_mul(b)?.checked_div(c)
}

fn calculate_penalty(amount: u64, penalty_bps: u64) -> u64 {
    (amount as u128)
        .saturating_mul(penalty_bps as u128)
        .saturating_div(10000)
        .min(amount as u128) as u64
}

fn calculate_rewards_for_period(rate: Rate, seconds: u64) -> u64 {
    let rate_u128 = rate.to_scaled_val().unwrap_or(0);
    let rewards = rate_u128
        .saturating_mul(seconds as u128)
        .saturating_div(10u128.pow(18)); // WAD scaling
    
    rewards.min(u64::MAX as u128) as u64
}

fn scale_amount_by_decimals(amount: u64, decimals: u8) -> u64 {
    let multiplier = 10u64.pow((18 - decimals as u32).min(18));
    amount.saturating_mul(multiplier)
}

fn unscale_amount_by_decimals(amount: u64, decimals: u8) -> u64 {
    let divisor = 10u64.pow((18 - decimals as u32).min(18));
    amount.saturating_div(divisor)
}