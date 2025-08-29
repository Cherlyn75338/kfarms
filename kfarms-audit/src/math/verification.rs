use num_bigint::BigUint;
use std::collections::HashMap;

/// Mathematical verification utilities
pub struct MathVerifier {
    pub tolerance: u128,
    pub scale: u128,
}

impl MathVerifier {
    pub fn new(scale: u128) -> Self {
        Self {
            tolerance: 10, // Allow 10 units of rounding error
            scale,
        }
    }

    /// Verify conservation of tokens across all operations
    pub fn verify_token_conservation(
        &self,
        initial_supply: u128,
        total_distributed: u128,
        total_burned: u128,
        current_supply: u128,
    ) -> Result<(), VerificationError> {
        let expected_supply = initial_supply
            .saturating_sub(total_distributed)
            .saturating_sub(total_burned);

        if current_supply.abs_diff(expected_supply) > self.tolerance {
            return Err(VerificationError::ConservationViolation {
                expected: expected_supply,
                actual: current_supply,
            });
        }

        Ok(())
    }

    /// Verify reward distribution fairness
    pub fn verify_distribution_fairness(
        &self,
        user_rewards: &HashMap<String, u128>,
        user_points: &HashMap<String, u128>,
        total_distributed: u128,
    ) -> Result<(), VerificationError> {
        let total_points: u128 = user_points.values().sum();
        
        if total_points == 0 {
            return Ok(());
        }

        for (user, &rewards) in user_rewards {
            let points = user_points.get(user).copied().unwrap_or(0);
            let expected_share = (total_distributed as u128)
                .saturating_mul(points)
                .saturating_div(total_points);

            if rewards.abs_diff(expected_share) > self.tolerance * 100 {
                return Err(VerificationError::UnfairDistribution {
                    user: user.clone(),
                    expected: expected_share,
                    actual: rewards,
                });
            }
        }

        Ok(())
    }

    /// Verify no value creation (anti-inflation)
    pub fn verify_no_value_creation(
        &self,
        operations: &[Operation],
    ) -> Result<(), VerificationError> {
        let mut net_value = 0i128;

        for op in operations {
            match op {
                Operation::Mint(amount) => net_value += *amount as i128,
                Operation::Burn(amount) => net_value -= *amount as i128,
                Operation::Transfer(from, to, amount) => {
                    // Transfers should be net-zero
                    if from == to {
                        return Err(VerificationError::SelfTransfer);
                    }
                }
            }
        }

        if net_value > self.tolerance as i128 {
            return Err(VerificationError::ValueCreation {
                amount: net_value as u128,
            });
        }

        Ok(())
    }

    /// Verify monotonic progression of cumulative values
    pub fn verify_monotonicity(
        &self,
        values: &[u128],
    ) -> Result<(), VerificationError> {
        for window in values.windows(2) {
            if window[1] < window[0] {
                return Err(VerificationError::MonotonicityViolation {
                    prev: window[0],
                    curr: window[1],
                });
            }
        }
        Ok(())
    }

    /// Verify precision in fixed-point arithmetic
    pub fn verify_precision(
        &self,
        result: u128,
        expected: u128,
        max_error_bps: u128,
    ) -> Result<(), VerificationError> {
        let error = result.abs_diff(expected);
        let max_error = (expected * max_error_bps) / 10_000;

        if error > max_error {
            return Err(VerificationError::PrecisionError {
                result,
                expected,
                error,
                max_error,
            });
        }

        Ok(())
    }

    /// Verify bounds on all values
    pub fn verify_bounds(
        &self,
        value: u128,
        min: u128,
        max: u128,
        name: &str,
    ) -> Result<(), VerificationError> {
        if value < min || value > max {
            return Err(VerificationError::BoundsViolation {
                name: name.to_string(),
                value,
                min,
                max,
            });
        }
        Ok(())
    }

    /// Cross-verify using high-precision arithmetic
    pub fn cross_verify_with_bigint(
        &self,
        operation: &str,
        inputs: &[u128],
        result: u128,
    ) -> Result<(), VerificationError> {
        let big_result = match operation {
            "mul_div" if inputs.len() == 3 => {
                let a = BigUint::from(inputs[0]);
                let b = BigUint::from(inputs[1]);
                let c = BigUint::from(inputs[2]);
                (a * b) / c
            }
            _ => return Ok(()),
        };

        if let Some(expected) = big_result.to_u128() {
            if result.abs_diff(expected) > 1 {
                return Err(VerificationError::CrossVerificationFailed {
                    operation: operation.to_string(),
                    chain_result: result,
                    reference_result: expected,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum Operation {
    Mint(u128),
    Burn(u128),
    Transfer(String, String, u128),
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Conservation violation: expected {expected}, got {actual}")]
    ConservationViolation { expected: u128, actual: u128 },

    #[error("Unfair distribution for {user}: expected {expected}, got {actual}")]
    UnfairDistribution {
        user: String,
        expected: u128,
        actual: u128,
    },

    #[error("Value creation detected: {amount}")]
    ValueCreation { amount: u128 },

    #[error("Self-transfer detected")]
    SelfTransfer,

    #[error("Monotonicity violation: {curr} < {prev}")]
    MonotonicityViolation { prev: u128, curr: u128 },

    #[error("Precision error: result {result}, expected {expected}, error {error} > max {max_error}")]
    PrecisionError {
        result: u128,
        expected: u128,
        error: u128,
        max_error: u128,
    },

    #[error("Bounds violation for {name}: {value} not in [{min}, {max}]")]
    BoundsViolation {
        name: String,
        value: u128,
        min: u128,
        max: u128,
    },

    #[error("Cross-verification failed for {operation}: chain {chain_result} != reference {reference_result}")]
    CrossVerificationFailed {
        operation: String,
        chain_result: u128,
        reference_result: u128,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservation_verification() {
        let verifier = MathVerifier::new(1_000_000_000_000_000_000);

        // Valid conservation
        assert!(verifier
            .verify_token_conservation(1000000, 100000, 50000, 850000)
            .is_ok());

        // Invalid conservation
        assert!(verifier
            .verify_token_conservation(1000000, 100000, 50000, 900000)
            .is_err());
    }

    #[test]
    fn test_monotonicity() {
        let verifier = MathVerifier::new(1_000_000_000_000_000_000);

        // Valid monotonic sequence
        let valid = vec![1, 2, 3, 5, 8, 13];
        assert!(verifier.verify_monotonicity(&valid).is_ok());

        // Invalid sequence
        let invalid = vec![1, 2, 3, 2, 5];
        assert!(verifier.verify_monotonicity(&invalid).is_err());
    }

    #[test]
    fn test_cross_verification() {
        let verifier = MathVerifier::new(1_000_000_000_000_000_000);

        // Test mul_div operation
        let inputs = vec![1000, 2000, 500];
        let result = (1000u128 * 2000) / 500; // 4000

        assert!(verifier
            .cross_verify_with_bigint("mul_div", &inputs, result)
            .is_ok());

        // Test with rounding error
        let bad_result = result + 10;
        assert!(verifier
            .cross_verify_with_bigint("mul_div", &inputs, bad_result)
            .is_err());
    }
}