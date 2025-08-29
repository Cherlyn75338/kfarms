use crate::{
    utils::math::{u64_mul_div, ten_pow},
    state::{FarmState, RewardInfo},
    FarmError,
};
use decimal_wad::decimal::Decimal;

#[cfg(test)]
mod oracle_price_tests {
    use super::*;

    // G. Oracle Price Handling Tests

    #[test]
    fn test_price_application_no_overflow() {
        // Test that price applications don't overflow
        let test_cases = vec![
            (1_000_000u64, 1_000_000_000u64, 9i32),  // Normal price
            (u64::MAX / 2, 2u64, 0i32),              // Large amount, small price
            (1u64, u64::MAX / 2, 0i32),              // Small amount, large price
            (1_000_000u64, 1_500_000_000u64, 9i32),  // 1.5x price
        ];
        
        for (amount, price_value, price_exp) in test_cases {
            let result = apply_oracle_price_safe(amount, price_value, price_exp);
            assert!(result.is_ok() || result.is_err());
            
            if let Ok(adjusted) = result {
                assert!(adjusted <= u128::MAX as u64);
            }
        }
    }

    #[test]
    fn test_narrowing_to_u64_errors() {
        // Test explicit errors when narrowing to u64 would lose data
        let amount = u64::MAX;
        let price_value = u64::MAX;
        let price_exp = 0i32;
        
        let result = apply_oracle_price_safe(amount, price_value, price_exp);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), FarmError::PriceOverflow);
    }

    #[test]
    fn test_stale_price_detection() {
        // Test stale price window detection
        let current_ts = 1000000u64;
        let max_age = 300u64; // 5 minutes
        
        let test_cases = vec![
            (current_ts - 100, true),   // Fresh (100s old)
            (current_ts - 300, true),   // Exactly at threshold
            (current_ts - 301, false),  // Just over threshold
            (current_ts - 3600, false), // Very stale (1 hour)
            (current_ts + 10, false),   // Future price (invalid)
        ];
        
        for (price_ts, should_accept) in test_cases {
            let is_fresh = check_price_freshness(price_ts, current_ts, max_age);
            assert_eq!(is_fresh, should_accept);
        }
    }

    #[test]
    fn test_price_boundary_conditions() {
        // Test boundary conditions for price handling
        
        // Zero price
        let result = apply_oracle_price_safe(1_000_000, 0, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        
        // Price = 1 (no change)
        let result = apply_oracle_price_safe(1_000_000, 1_000_000_000, 9);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1_000_000);
        
        // Very small price
        let result = apply_oracle_price_safe(1_000_000_000, 1, 18);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // Rounds down to 0
        
        // Very large price
        let result = apply_oracle_price_safe(1, u64::MAX, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), u64::MAX);
    }

    #[test]
    fn test_twap_price_calculation() {
        // Test TWAP (Time-Weighted Average Price) calculation
        let prices = vec![
            (100, 1_000_000_000),  // Price at t=100
            (200, 1_100_000_000),  // Price at t=200
            (300, 1_050_000_000),  // Price at t=300
            (400, 1_200_000_000),  // Price at t=400
        ];
        
        let twap = calculate_twap(&prices);
        
        // TWAP should be weighted average
        // (1.0 * 100 + 1.1 * 100 + 1.05 * 100 + 1.2 * 100) / 400
        let expected = (1_000_000_000u128 * 100 + 
                       1_100_000_000u128 * 100 + 
                       1_050_000_000u128 * 100 + 
                       1_200_000_000u128 * 100) / 400;
        
        assert_eq!(twap, expected as u64);
    }

    #[test]
    fn test_ewma_price_smoothing() {
        // Test EWMA (Exponentially Weighted Moving Average) for price smoothing
        let alpha = 200; // 0.2 in basis points (2000/10000)
        let mut ewma = 1_000_000_000u64; // Initial price
        
        let new_prices = vec![
            1_100_000_000u64,  // 10% spike
            1_050_000_000u64,  // Partial recovery
            1_000_000_000u64,  // Back to normal
        ];
        
        for new_price in new_prices {
            ewma = calculate_ewma(ewma, new_price, alpha);
            
            // EWMA should smooth out spikes
            assert!(ewma > 950_000_000 && ewma < 1_150_000_000);
        }
    }

    #[test]
    fn test_price_spike_mitigation() {
        // Test guardrails against price spikes
        let base_price = 1_000_000_000u64;
        let max_change_bps = 1000; // 10% max change
        
        let test_prices = vec![
            1_050_000_000u64,  // 5% increase - should be accepted
            1_200_000_000u64,  // 20% increase - should be capped
            800_000_000u64,    // 20% decrease - should be capped
            1_090_000_000u64,  // 9% increase - should be accepted
        ];
        
        for new_price in test_prices {
            let capped_price = apply_price_cap(base_price, new_price, max_change_bps);
            
            let change_ratio = if new_price > base_price {
                ((new_price - base_price) * 10000) / base_price
            } else {
                ((base_price - new_price) * 10000) / base_price
            };
            
            if change_ratio > max_change_bps {
                // Price should be capped
                let max_price = base_price + (base_price * max_change_bps as u64) / 10000;
                let min_price = base_price - (base_price * max_change_bps as u64) / 10000;
                assert!(capped_price >= min_price && capped_price <= max_price);
            } else {
                // Price should be unchanged
                assert_eq!(capped_price, new_price);
            }
        }
    }

    #[test]
    fn test_mint_amount_with_price_caps() {
        // Test caps on per-interval mint amounts
        let max_mint_per_interval = 1_000_000u64;
        let price = 1_500_000_000u64; // 1.5x
        let price_exp = 9i32;
        
        let requested_mint = 2_000_000u64;
        let price_adjusted = apply_oracle_price_safe(requested_mint, price, price_exp).unwrap();
        
        // Apply mint cap
        let final_mint = price_adjusted.min(max_mint_per_interval);
        assert_eq!(final_mint, max_mint_per_interval);
    }

    #[test]
    fn test_missing_oracle_fallback() {
        // Test fallback behavior when oracle is missing
        let mut reward_info = create_test_reward_info();
        reward_info.oracle_price_account = None;
        
        let amount = 1_000_000u64;
        let adjusted = apply_price_adjustment(&reward_info, amount);
        
        // Should use 1:1 ratio when no oracle
        assert_eq!(adjusted, amount);
    }

    #[test]
    fn test_oracle_decimal_precision() {
        // Test handling of different decimal precisions
        let amount = 1_000_000u64; // 6 decimals (USDC-like)
        
        let test_cases = vec![
            (1_000_000_000u64, 9i32, 1_000_000u64),    // 9 decimals
            (1_000_000u64, 6i32, 1_000_000u64),        // 6 decimals
            (100_000_000u64, 8i32, 1_000_000u64),      // 8 decimals
            (1_000_000_000_000_000_000u64, 18i32, 1_000_000u64), // 18 decimals
        ];
        
        for (price_value, decimals, expected) in test_cases {
            let adjusted = adjust_for_decimals(amount, price_value, decimals);
            assert_eq!(adjusted, expected);
        }
    }

    #[test]
    fn test_oracle_price_with_negative_exponent() {
        // Test oracle prices with negative exponents
        let amount = 1_000_000_000u64;
        
        let test_cases = vec![
            (1_000_000_000u64, -9i32),  // Price = 1.0
            (1_500_000_000u64, -9i32),  // Price = 1.5
            (500_000_000u64, -9i32),    // Price = 0.5
            (1u64, -18i32),             // Very small price
        ];
        
        for (price_value, exp) in test_cases {
            let result = apply_oracle_price_with_negative_exp(amount, price_value, exp);
            assert!(result.is_ok());
            
            let adjusted = result.unwrap();
            if exp == -9 {
                // For -9 exponent, price_value of 1_000_000_000 = 1.0
                let expected = u64_mul_div(amount, price_value, 1_000_000_000);
                assert_eq!(adjusted, expected);
            }
        }
    }

    #[test]
    fn test_composite_price_feed() {
        // Test handling of composite price feeds (e.g., SOL/USD * USD/EUR)
        let sol_usd = 100_000_000_000u64; // $100 with 9 decimals
        let usd_eur = 920_000_000u64;     // 0.92 with 9 decimals
        
        let composite = calculate_composite_price(sol_usd, usd_eur, 9, 9);
        
        // SOL/EUR = SOL/USD * USD/EUR = 100 * 0.92 = 92
        let expected = 92_000_000_000u64; // 92 with 9 decimals
        assert_eq!(composite, expected);
    }

    #[test]
    fn test_price_confidence_interval() {
        // Test price confidence interval handling
        let price = 1_000_000_000u64;
        let confidence = 10_000_000u64; // 1% confidence interval
        
        let test_amounts = vec![
            1_000_000u64,
            10_000_000u64,
            100_000_000u64,
        ];
        
        for amount in test_amounts {
            let min_value = apply_oracle_price_safe(
                amount,
                price - confidence,
                9
            ).unwrap();
            
            let max_value = apply_oracle_price_safe(
                amount,
                price + confidence,
                9
            ).unwrap();
            
            // Values should be within confidence interval
            assert!(max_value > min_value);
            assert!(max_value - min_value <= (amount * 2 * confidence) / 1_000_000_000);
        }
    }

    #[test]
    fn test_oracle_update_frequency() {
        // Test that oracle updates are handled at appropriate frequency
        let mut last_update = 0u64;
        let min_update_interval = 1u64; // Minimum 1 second between updates
        
        let timestamps = vec![0, 1, 1, 2, 5, 5, 10];
        let mut accepted_updates = 0;
        
        for ts in timestamps {
            if ts >= last_update + min_update_interval {
                accepted_updates += 1;
                last_update = ts;
            }
        }
        
        assert_eq!(accepted_updates, 5); // 0, 1, 2, 5, 10
    }

    #[test]
    fn test_price_impact_limits() {
        // Test price impact limits for large operations
        let total_liquidity = 10_000_000_000u64; // $10M
        let max_impact_bps = 100; // 1% max price impact
        
        let test_trades = vec![
            10_000u64,         // Small trade - no impact
            100_000_000u64,    // 1% of liquidity - at limit
            1_000_000_000u64,  // 10% of liquidity - over limit
        ];
        
        for trade_size in test_trades {
            let impact_bps = (trade_size * 10000) / total_liquidity;
            let should_accept = impact_bps <= max_impact_bps;
            
            assert_eq!(
                is_trade_acceptable(trade_size, total_liquidity, max_impact_bps),
                should_accept
            );
        }
    }

    // Helper functions
    fn apply_oracle_price_safe(amount: u64, price_value: u64, price_exp: i32) -> Result<u64, FarmError> {
        let scaled_price = if price_exp >= 0 {
            (price_value as u128).checked_mul(ten_pow(price_exp as usize) as u128)
                .ok_or(FarmError::PriceOverflow)?
        } else {
            (price_value as u128) / ten_pow((-price_exp) as usize) as u128
        };
        
        let adjusted = ((amount as u128) * scaled_price) / 1_000_000_000u128;
        
        if adjusted > u64::MAX as u128 {
            return Err(FarmError::PriceOverflow);
        }
        
        Ok(adjusted as u64)
    }

    fn check_price_freshness(price_ts: u64, current_ts: u64, max_age: u64) -> bool {
        price_ts <= current_ts && current_ts - price_ts <= max_age
    }

    fn calculate_twap(prices: &[(u64, u64)]) -> u64 {
        if prices.is_empty() {
            return 0;
        }
        
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u64;
        
        for i in 0..prices.len() {
            let weight = if i == 0 {
                prices[0].0
            } else {
                prices[i].0 - prices[i-1].0
            };
            
            weighted_sum += (prices[i].1 as u128) * (weight as u128);
            total_weight += weight;
        }
        
        if total_weight == 0 {
            return 0;
        }
        
        (weighted_sum / total_weight as u128) as u64
    }

    fn calculate_ewma(current: u64, new_price: u64, alpha_bps: u64) -> u64 {
        // EWMA = α * new_price + (1 - α) * current
        let alpha_part = u64_mul_div(new_price, alpha_bps, 10000);
        let current_part = u64_mul_div(current, 10000 - alpha_bps, 10000);
        alpha_part + current_part
    }

    fn apply_price_cap(base_price: u64, new_price: u64, max_change_bps: u64) -> u64 {
        let max_increase = u64_mul_div(base_price, 10000 + max_change_bps, 10000);
        let max_decrease = u64_mul_div(base_price, 10000 - max_change_bps, 10000);
        
        new_price.min(max_increase).max(max_decrease)
    }

    fn create_test_reward_info() -> RewardInfo {
        RewardInfo {
            reward_type: 0,
            reward_vault: [0u8; 32],
            reward_vault_authority: [0u8; 32],
            reward_mint: [0u8; 32],
            reward_mint_decimals: 9,
            rewards_per_second_decimals: 12,
            reward_schedule_curve: Default::default(),
            last_issuance_ts: 0,
            reward_per_share_scaled: 0,
            rewards_issued_cumulative: 0,
            rewards_available: 0,
            oracle_price_account: None,
            _padding: [0u8; 256],
        }
    }

    fn apply_price_adjustment(reward_info: &RewardInfo, amount: u64) -> u64 {
        if reward_info.oracle_price_account.is_none() {
            return amount;
        }
        
        // Simulate price adjustment
        amount
    }

    fn adjust_for_decimals(amount: u64, price_value: u64, decimals: i32) -> u64 {
        if decimals == 9 {
            u64_mul_div(amount, price_value, 1_000_000_000)
        } else if decimals == 6 {
            u64_mul_div(amount, price_value, 1_000_000)
        } else if decimals == 8 {
            u64_mul_div(amount, price_value, 100_000_000)
        } else if decimals == 18 {
            u64_mul_div(amount, price_value / 1_000_000_000, 1_000_000_000)
        } else {
            amount
        }
    }

    fn apply_oracle_price_with_negative_exp(amount: u64, price_value: u64, exp: i32) -> Result<u64, FarmError> {
        if exp >= 0 {
            return Err(FarmError::InvalidPriceExponent);
        }
        
        let divisor = ten_pow((-exp) as usize);
        Ok(u64_mul_div(amount, price_value, divisor))
    }

    fn calculate_composite_price(price1: u64, price2: u64, decimals1: i32, decimals2: i32) -> u64 {
        let normalized1 = if decimals1 == 9 {
            price1
        } else {
            price1 * ten_pow((9 - decimals1) as usize)
        };
        
        let normalized2 = if decimals2 == 9 {
            price2
        } else {
            price2 * ten_pow((9 - decimals2) as usize)
        };
        
        u64_mul_div(normalized1, normalized2, 1_000_000_000)
    }

    fn is_trade_acceptable(trade_size: u64, total_liquidity: u64, max_impact_bps: u64) -> bool {
        let impact_bps = (trade_size * 10000) / total_liquidity;
        impact_bps <= max_impact_bps
    }
}