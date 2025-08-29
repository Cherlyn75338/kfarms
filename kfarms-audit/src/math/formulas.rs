use num_bigint::{BigUint, ToBigUint};
use rust_decimal::prelude::*;
use std::cmp;

/// Scale factor for fixed-point arithmetic (10^18)
pub const SCALE: u128 = 1_000_000_000_000_000_000;
pub const POINTS_SCALE: u128 = SCALE;
pub const REWARD_SCALE: u128 = SCALE;
pub const BPS_DIVISOR: u128 = 10_000;

/// Maximum allowed values for safety
pub const MAX_POINTS: u128 = u64::MAX as u128 * 1000; // Allow some headroom
pub const MAX_MULTIPLIER_BPS: u128 = 30_000; // 3x max multiplier
pub const MAX_EMISSION_RATE: u128 = 1_000_000 * SCALE; // 1M tokens/sec max
pub const MAX_TIME_DELTA: u64 = 86400 * 30; // 30 days max delta

/// Core mathematical formulas for KFarms protocol
#[derive(Debug, Clone)]
pub struct RewardMath {
    pub scale: u128,
}

impl RewardMath {
    pub fn new() -> Self {
        Self { scale: SCALE }
    }

    /// Calculate lock multiplier M(L) based on lock duration
    /// M(L) = 1 + (L / L_max) * (M_max - 1)
    /// Returns multiplier in basis points (10000 = 1x)
    pub fn calculate_lock_multiplier(
        lock_duration: u64,
        max_duration: u64,
        max_multiplier_bps: u128,
    ) -> Result<u128, MathError> {
        if lock_duration > max_duration {
            return Err(MathError::InvalidLockDuration);
        }
        
        if max_multiplier_bps > MAX_MULTIPLIER_BPS {
            return Err(MathError::MultiplierOverflow);
        }

        // Base multiplier is 10000 (1x)
        let base_multiplier = BPS_DIVISOR;
        
        // Calculate bonus: (L / L_max) * (M_max - base)
        let bonus = (lock_duration as u128)
            .checked_mul(max_multiplier_bps.saturating_sub(base_multiplier))
            .ok_or(MathError::Overflow)?
            .checked_div(max_duration as u128)
            .ok_or(MathError::DivisionByZero)?;

        Ok(base_multiplier.checked_add(bonus).ok_or(MathError::Overflow)?)
    }

    /// Calculate user points: P_u = A_u * M(L) / 10000 + P_ext
    pub fn calculate_user_points(
        staked_amount: u128,
        lock_multiplier_bps: u128,
        external_points: u128,
    ) -> Result<u128, MathError> {
        // Use wide math to prevent overflow
        let staked_points = staked_amount
            .checked_mul(lock_multiplier_bps)
            .ok_or(MathError::Overflow)?
            .checked_div(BPS_DIVISOR)
            .ok_or(MathError::DivisionByZero)?;

        let total_points = staked_points
            .checked_add(external_points)
            .ok_or(MathError::Overflow)?;

        if total_points > MAX_POINTS {
            return Err(MathError::PointsOverflow);
        }

        Ok(total_points)
    }

    /// Update cumulative reward per point
    /// C += (R'(t) * Δt * SCALE) / P_tot
    pub fn update_cumulative_rpp(
        current_c: u128,
        emission_rate: u128,
        time_delta: u64,
        total_points: u128,
    ) -> Result<u128, MathError> {
        if total_points == 0 {
            // No points, no distribution
            return Ok(current_c);
        }

        if emission_rate > MAX_EMISSION_RATE {
            return Err(MathError::EmissionRateTooHigh);
        }

        if time_delta > MAX_TIME_DELTA {
            return Err(MathError::TimeDeltaTooLarge);
        }

        // Calculate increment: (R' * Δt * SCALE) / P_tot
        let numerator = emission_rate
            .checked_mul(time_delta as u128)
            .ok_or(MathError::Overflow)?
            .checked_mul(SCALE)
            .ok_or(MathError::Overflow)?;

        let increment = numerator
            .checked_div(total_points)
            .ok_or(MathError::DivisionByZero)?;

        current_c.checked_add(increment).ok_or(MathError::Overflow)
    }

    /// Calculate earned rewards for a user
    /// earned_u = floor((P_u * (C - C_paid)) / SCALE) + accrued
    pub fn calculate_earned_rewards(
        user_points: u128,
        current_c: u128,
        user_c_paid: u128,
        user_accrued: u128,
    ) -> Result<u128, MathError> {
        // Ensure C is monotonic
        if current_c < user_c_paid {
            return Err(MathError::NonMonotonicC);
        }

        let c_delta = current_c.saturating_sub(user_c_paid);
        
        // Wide math: P_u * ΔC
        let pending = user_points
            .checked_mul(c_delta)
            .ok_or(MathError::Overflow)?
            .checked_div(SCALE)
            .ok_or(MathError::DivisionByZero)?;

        pending.checked_add(user_accrued).ok_or(MathError::Overflow)
    }

    /// Apply penalty for early withdrawal
    /// Returns (amount_after_penalty, penalty_amount)
    pub fn apply_withdrawal_penalty(
        amount: u128,
        time_remaining: u64,
        max_lock_duration: u64,
        max_penalty_bps: u128,
    ) -> Result<(u128, u128), MathError> {
        if time_remaining > max_lock_duration {
            return Err(MathError::InvalidLockDuration);
        }

        // Linear penalty: penalty_bps = (time_remaining / max_duration) * max_penalty_bps
        let penalty_bps = (time_remaining as u128)
            .checked_mul(max_penalty_bps)
            .ok_or(MathError::Overflow)?
            .checked_div(max_lock_duration as u128)
            .ok_or(MathError::DivisionByZero)?;

        let penalty_amount = amount
            .checked_mul(penalty_bps)
            .ok_or(MathError::Overflow)?
            .checked_div(BPS_DIVISOR)
            .ok_or(MathError::DivisionByZero)?;

        let amount_after = amount.saturating_sub(penalty_amount);

        Ok((amount_after, penalty_amount))
    }

    /// Verify conservation invariant
    /// Total distributed <= Integral of emission over time
    pub fn verify_conservation(
        total_distributed: u128,
        emission_integral: u128,
        max_rounding_error: u128,
    ) -> Result<bool, MathError> {
        // Allow for bounded rounding error
        let max_allowed = emission_integral
            .checked_add(max_rounding_error)
            .ok_or(MathError::Overflow)?;

        Ok(total_distributed <= max_allowed)
    }

    /// Calculate TWAP for governance (Time-Weighted Average Points)
    pub fn calculate_twap(
        point_samples: &[(u128, u64)], // (points, timestamp)
        window_start: u64,
        window_end: u64,
    ) -> Result<u128, MathError> {
        if window_end <= window_start {
            return Err(MathError::InvalidTimeWindow);
        }

        if point_samples.is_empty() {
            return Ok(0);
        }

        let window_duration = window_end - window_start;
        let mut weighted_sum: u128 = 0;
        let mut prev_time = window_start;
        let mut prev_points = point_samples[0].0;

        for &(points, timestamp) in point_samples.iter() {
            if timestamp <= prev_time {
                continue;
            }

            let time_delta = cmp::min(timestamp, window_end) - prev_time;
            let weighted_points = prev_points
                .checked_mul(time_delta as u128)
                .ok_or(MathError::Overflow)?;

            weighted_sum = weighted_sum
                .checked_add(weighted_points)
                .ok_or(MathError::Overflow)?;

            prev_time = timestamp;
            prev_points = points;

            if timestamp >= window_end {
                break;
            }
        }

        // Handle remaining time to window_end
        if prev_time < window_end {
            let time_delta = window_end - prev_time;
            let weighted_points = prev_points
                .checked_mul(time_delta as u128)
                .ok_or(MathError::Overflow)?;

            weighted_sum = weighted_sum
                .checked_add(weighted_points)
                .ok_or(MathError::Overflow)?;
        }

        Ok(weighted_sum / (window_duration as u128))
    }

    /// Smooth external points changes using EMA
    /// new_smoothed = α * new_value + (1 - α) * old_smoothed
    /// where α is in basis points
    pub fn apply_ema_smoothing(
        new_value: u128,
        old_smoothed: u128,
        alpha_bps: u128,
    ) -> Result<u128, MathError> {
        if alpha_bps > BPS_DIVISOR {
            return Err(MathError::InvalidAlpha);
        }

        let weighted_new = new_value
            .checked_mul(alpha_bps)
            .ok_or(MathError::Overflow)?;

        let weighted_old = old_smoothed
            .checked_mul(BPS_DIVISOR - alpha_bps)
            .ok_or(MathError::Overflow)?;

        let sum = weighted_new
            .checked_add(weighted_old)
            .ok_or(MathError::Overflow)?;

        Ok(sum / BPS_DIVISOR)
    }

    /// Normalize token amounts across different decimals
    pub fn normalize_decimals(
        amount: u128,
        from_decimals: u8,
        to_decimals: u8,
    ) -> Result<u128, MathError> {
        if from_decimals > 18 || to_decimals > 18 {
            return Err(MathError::InvalidDecimals);
        }

        if from_decimals == to_decimals {
            return Ok(amount);
        }

        if from_decimals < to_decimals {
            let scale_factor = 10u128.pow((to_decimals - from_decimals) as u32);
            amount.checked_mul(scale_factor).ok_or(MathError::Overflow)
        } else {
            let scale_factor = 10u128.pow((from_decimals - to_decimals) as u32);
            Ok(amount / scale_factor)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MathError {
    #[error("Arithmetic overflow")]
    Overflow,
    
    #[error("Division by zero")]
    DivisionByZero,
    
    #[error("Invalid lock duration")]
    InvalidLockDuration,
    
    #[error("Multiplier overflow")]
    MultiplierOverflow,
    
    #[error("Points overflow")]
    PointsOverflow,
    
    #[error("Emission rate too high")]
    EmissionRateTooHigh,
    
    #[error("Time delta too large")]
    TimeDeltaTooLarge,
    
    #[error("Non-monotonic cumulative value")]
    NonMonotonicC,
    
    #[error("Invalid time window")]
    InvalidTimeWindow,
    
    #[error("Invalid alpha value")]
    InvalidAlpha,
    
    #[error("Invalid decimals")]
    InvalidDecimals,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_multiplier() {
        let math = RewardMath::new();
        
        // Test linear scaling
        let m = math.calculate_lock_multiplier(180, 360, 30000).unwrap();
        assert_eq!(m, 20000); // 2x for half max duration
        
        // Test max duration
        let m = math.calculate_lock_multiplier(360, 360, 30000).unwrap();
        assert_eq!(m, 30000); // 3x for max duration
        
        // Test zero duration
        let m = math.calculate_lock_multiplier(0, 360, 30000).unwrap();
        assert_eq!(m, 10000); // 1x for no lock
    }

    #[test]
    fn test_cumulative_rpp_update() {
        let math = RewardMath::new();
        
        // Test normal update
        let new_c = math.update_cumulative_rpp(
            0,
            1000 * SCALE, // 1000 tokens/sec
            60, // 60 seconds
            1000000 * SCALE, // 1M total points
        ).unwrap();
        
        assert_eq!(new_c, 60000); // Should be 60k scaled units
        
        // Test zero points (no distribution)
        let new_c = math.update_cumulative_rpp(100, 1000, 60, 0).unwrap();
        assert_eq!(new_c, 100); // Unchanged
    }

    #[test]
    fn test_earned_rewards() {
        let math = RewardMath::new();
        
        let earned = math.calculate_earned_rewards(
            1000 * SCALE, // 1000 points
            2 * SCALE, // C = 2
            1 * SCALE, // C_paid = 1
            500, // 500 accrued
        ).unwrap();
        
        assert_eq!(earned, 1000 + 500); // 1000 from delta + 500 accrued
    }

    #[test]
    fn test_twap_calculation() {
        let math = RewardMath::new();
        
        let samples = vec![
            (1000, 0),
            (2000, 50),
            (1500, 100),
        ];
        
        let twap = math.calculate_twap(&samples, 0, 100).unwrap();
        assert_eq!(twap, 1500); // Average over the window
    }
}