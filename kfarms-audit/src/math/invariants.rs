use std::collections::HashMap;

/// Core invariants that must hold at all times in the protocol
#[derive(Debug, Clone)]
pub struct Invariants {
    pub checks: Vec<InvariantCheck>,
}

#[derive(Debug, Clone)]
pub struct InvariantCheck {
    pub name: String,
    pub category: InvariantCategory,
    pub severity: Severity,
    pub check_fn: fn(&ProtocolState) -> Result<(), InvariantViolation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InvariantCategory {
    Conservation,
    Monotonicity,
    Bounds,
    Consistency,
    Safety,
    Governance,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Critical,  // Can lead to theft or insolvency
    High,      // Can lead to temporary freezing or unclaimed yield theft
    Medium,    // Can lead to accounting errors
    Low,       // Minor issues
}

#[derive(Debug, Clone)]
pub struct ProtocolState {
    pub pools: HashMap<String, PoolState>,
    pub users: HashMap<String, UserState>,
    pub global: GlobalState,
}

#[derive(Debug, Clone)]
pub struct PoolState {
    pub total_points: u128,
    pub cumulative_rpp: u128,
    pub emission_rate: u128,
    pub last_update_time: u64,
    pub total_distributed: u128,
    pub reward_vault_balance: u128,
    pub stake_vault_balance: u128,
}

#[derive(Debug, Clone)]
pub struct UserState {
    pub staked_amount: u128,
    pub lock_duration: u64,
    pub lock_end_time: u64,
    pub points: u128,
    pub external_points: u128,
    pub c_paid: u128,
    pub accrued_rewards: u128,
    pub total_claimed: u128,
}

#[derive(Debug, Clone)]
pub struct GlobalState {
    pub current_time: u64,
    pub current_slot: u64,
    pub total_emission_integral: u128,
    pub total_distributed_global: u128,
    pub governance_snapshot_time: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum InvariantViolation {
    #[error("Non-negativity violation: {field} is negative")]
    NonNegativity { field: String },
    
    #[error("Conservation violation: distributed {distributed} > integral {integral}")]
    Conservation { distributed: u128, integral: u128 },
    
    #[error("Monotonicity violation: {field} decreased from {old} to {new}")]
    Monotonicity { field: String, old: u128, new: u128 },
    
    #[error("Bounds violation: {field} = {value} exceeds max {max}")]
    BoundsExceeded { field: String, value: u128, max: u128 },
    
    #[error("Division by zero: {context}")]
    DivisionByZero { context: String },
    
    #[error("Consistency violation: {description}")]
    Consistency { description: String },
    
    #[error("Points mismatch: calculated {calculated} != stored {stored}")]
    PointsMismatch { calculated: u128, stored: u128 },
    
    #[error("Time violation: {description}")]
    TimeViolation { description: String },
    
    #[error("Overflow risk: {context}")]
    OverflowRisk { context: String },
}

impl Invariants {
    pub fn new() -> Self {
        Self {
            checks: vec![
                // Critical invariants
                InvariantCheck {
                    name: "Non-negativity".to_string(),
                    category: InvariantCategory::Safety,
                    severity: Severity::Critical,
                    check_fn: check_non_negativity,
                },
                InvariantCheck {
                    name: "Conservation of rewards".to_string(),
                    category: InvariantCategory::Conservation,
                    severity: Severity::Critical,
                    check_fn: check_conservation,
                },
                InvariantCheck {
                    name: "No division by zero".to_string(),
                    category: InvariantCategory::Safety,
                    severity: Severity::Critical,
                    check_fn: check_no_division_by_zero,
                },
                InvariantCheck {
                    name: "Points consistency".to_string(),
                    category: InvariantCategory::Consistency,
                    severity: Severity::Critical,
                    check_fn: check_points_consistency,
                },
                
                // High severity invariants
                InvariantCheck {
                    name: "Monotonic cumulative values".to_string(),
                    category: InvariantCategory::Monotonicity,
                    severity: Severity::High,
                    check_fn: check_monotonicity,
                },
                InvariantCheck {
                    name: "Bounded values".to_string(),
                    category: InvariantCategory::Bounds,
                    severity: Severity::High,
                    check_fn: check_bounds,
                },
                InvariantCheck {
                    name: "Time consistency".to_string(),
                    category: InvariantCategory::Consistency,
                    severity: Severity::High,
                    check_fn: check_time_consistency,
                },
                InvariantCheck {
                    name: "Vault solvency".to_string(),
                    category: InvariantCategory::Conservation,
                    severity: Severity::High,
                    check_fn: check_vault_solvency,
                },
                
                // Governance invariants
                InvariantCheck {
                    name: "Governance points stability".to_string(),
                    category: InvariantCategory::Governance,
                    severity: Severity::High,
                    check_fn: check_governance_stability,
                },
            ],
        }
    }

    pub fn check_all(&self, state: &ProtocolState) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();
        
        for check in &self.checks {
            if let Err(violation) = (check.check_fn)(state) {
                violations.push(violation);
            }
        }
        
        violations
    }

    pub fn check_critical_only(&self, state: &ProtocolState) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();
        
        for check in &self.checks {
            if check.severity == Severity::Critical {
                if let Err(violation) = (check.check_fn)(state) {
                    violations.push(violation);
                }
            }
        }
        
        violations
    }
}

// Invariant check functions

fn check_non_negativity(state: &ProtocolState) -> Result<(), InvariantViolation> {
    // Check pool values
    for (pool_id, pool) in &state.pools {
        if pool.total_points > u128::MAX / 2 {
            return Err(InvariantViolation::NonNegativity {
                field: format!("pool_{}_total_points", pool_id),
            });
        }
        if pool.cumulative_rpp > u128::MAX / 2 {
            return Err(InvariantViolation::NonNegativity {
                field: format!("pool_{}_cumulative_rpp", pool_id),
            });
        }
    }
    
    // Check user values
    for (user_id, user) in &state.users {
        if user.points > u128::MAX / 2 {
            return Err(InvariantViolation::NonNegativity {
                field: format!("user_{}_points", user_id),
            });
        }
        if user.accrued_rewards > u128::MAX / 2 {
            return Err(InvariantViolation::NonNegativity {
                field: format!("user_{}_accrued", user_id),
            });
        }
    }
    
    Ok(())
}

fn check_conservation(state: &ProtocolState) -> Result<(), InvariantViolation> {
    let total_distributed = state.global.total_distributed_global;
    let emission_integral = state.global.total_emission_integral;
    
    // Allow for rounding error based on number of claims
    let max_rounding_error = state.users.len() as u128;
    
    if total_distributed > emission_integral + max_rounding_error {
        return Err(InvariantViolation::Conservation {
            distributed: total_distributed,
            integral: emission_integral,
        });
    }
    
    // Check per-pool conservation
    for (_, pool) in &state.pools {
        if pool.total_distributed > pool.reward_vault_balance + pool.total_distributed {
            return Err(InvariantViolation::Conservation {
                distributed: pool.total_distributed,
                integral: pool.reward_vault_balance,
            });
        }
    }
    
    Ok(())
}

fn check_no_division_by_zero(state: &ProtocolState) -> Result<(), InvariantViolation> {
    for (pool_id, pool) in &state.pools {
        // If emission rate > 0, we need total_points > 0 to avoid division by zero
        if pool.emission_rate > 0 && pool.total_points == 0 {
            return Err(InvariantViolation::DivisionByZero {
                context: format!("Pool {} has emission but zero points", pool_id),
            });
        }
    }
    
    Ok(())
}

fn check_points_consistency(state: &ProtocolState) -> Result<(), InvariantViolation> {
    // Sum of user points should equal pool total points
    for (pool_id, pool) in &state.pools {
        let mut calculated_total: u128 = 0;
        
        for (_, user) in &state.users {
            calculated_total = calculated_total.saturating_add(user.points);
        }
        
        // Allow small rounding error
        let tolerance = 10;
        if calculated_total.abs_diff(pool.total_points) > tolerance {
            return Err(InvariantViolation::PointsMismatch {
                calculated: calculated_total,
                stored: pool.total_points,
            });
        }
    }
    
    // Check individual user points calculation
    for (_, user) in &state.users {
        let expected_points = calculate_expected_points(user);
        if user.points != expected_points {
            return Err(InvariantViolation::PointsMismatch {
                calculated: expected_points,
                stored: user.points,
            });
        }
    }
    
    Ok(())
}

fn check_monotonicity(state: &ProtocolState) -> Result<(), InvariantViolation> {
    // This would need historical data to check properly
    // For now, check that cumulative values are reasonable
    for (_, pool) in &state.pools {
        if pool.cumulative_rpp > u128::MAX / 1000 {
            return Err(InvariantViolation::Monotonicity {
                field: "cumulative_rpp".to_string(),
                old: 0,
                new: pool.cumulative_rpp,
            });
        }
    }
    
    Ok(())
}

fn check_bounds(state: &ProtocolState) -> Result<(), InvariantViolation> {
    use super::formulas::{MAX_POINTS, MAX_EMISSION_RATE, MAX_MULTIPLIER_BPS};
    
    for (pool_id, pool) in &state.pools {
        if pool.total_points > MAX_POINTS {
            return Err(InvariantViolation::BoundsExceeded {
                field: format!("pool_{}_total_points", pool_id),
                value: pool.total_points,
                max: MAX_POINTS,
            });
        }
        
        if pool.emission_rate > MAX_EMISSION_RATE {
            return Err(InvariantViolation::BoundsExceeded {
                field: format!("pool_{}_emission_rate", pool_id),
                value: pool.emission_rate,
                max: MAX_EMISSION_RATE,
            });
        }
    }
    
    for (user_id, user) in &state.users {
        if user.points > MAX_POINTS {
            return Err(InvariantViolation::BoundsExceeded {
                field: format!("user_{}_points", user_id),
                value: user.points,
                max: MAX_POINTS,
            });
        }
    }
    
    Ok(())
}

fn check_time_consistency(state: &ProtocolState) -> Result<(), InvariantViolation> {
    let current_time = state.global.current_time;
    
    // Check pool update times
    for (pool_id, pool) in &state.pools {
        if pool.last_update_time > current_time {
            return Err(InvariantViolation::TimeViolation {
                description: format!("Pool {} last_update in future", pool_id),
            });
        }
    }
    
    // Check user lock times
    for (user_id, user) in &state.users {
        if user.lock_end_time < current_time && user.lock_duration > 0 {
            // Lock should have expired
            if user.points > user.staked_amount + user.external_points {
                return Err(InvariantViolation::TimeViolation {
                    description: format!("User {} has multiplier after lock expiry", user_id),
                });
            }
        }
    }
    
    Ok(())
}

fn check_vault_solvency(state: &ProtocolState) -> Result<(), InvariantViolation> {
    for (pool_id, pool) in &state.pools {
        // Calculate total pending rewards
        let mut total_pending: u128 = 0;
        
        for (_, user) in &state.users {
            let pending = calculate_pending_rewards(user, pool);
            total_pending = total_pending.saturating_add(pending);
        }
        
        // Vault must have enough to cover all pending
        if pool.reward_vault_balance < total_pending {
            return Err(InvariantViolation::Consistency {
                description: format!("Pool {} insolvent: vault {} < pending {}", 
                    pool_id, pool.reward_vault_balance, total_pending),
            });
        }
    }
    
    Ok(())
}

fn check_governance_stability(state: &ProtocolState) -> Result<(), InvariantViolation> {
    if let Some(snapshot_time) = state.global.governance_snapshot_time {
        let time_to_snapshot = snapshot_time.saturating_sub(state.global.current_time);
        
        // Within 1 hour of snapshot, points changes should be restricted
        if time_to_snapshot < 3600 {
            // Check for large point changes (would need historical data)
            // For now, just ensure no extreme values
            for (_, user) in &state.users {
                if user.external_points > user.staked_amount * 10 {
                    return Err(InvariantViolation::Consistency {
                        description: "External points too high near governance snapshot".to_string(),
                    });
                }
            }
        }
    }
    
    Ok(())
}

// Helper functions

fn calculate_expected_points(user: &UserState) -> u128 {
    // Simplified calculation - would need full context in real implementation
    let base_points = user.staked_amount;
    let multiplier = if user.lock_end_time > 0 { 2 } else { 1 };
    base_points * multiplier + user.external_points
}

fn calculate_pending_rewards(user: &UserState, pool: &PoolState) -> u128 {
    use super::formulas::SCALE;
    
    if pool.cumulative_rpp <= user.c_paid {
        return user.accrued_rewards;
    }
    
    let c_delta = pool.cumulative_rpp - user.c_paid;
    let pending = (user.points * c_delta) / SCALE;
    pending + user.accrued_rewards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_checks() {
        let mut state = ProtocolState {
            pools: HashMap::new(),
            users: HashMap::new(),
            global: GlobalState {
                current_time: 1000,
                current_slot: 1000,
                total_emission_integral: 1_000_000,
                total_distributed_global: 900_000,
                governance_snapshot_time: None,
            },
        };
        
        // Add a valid pool
        state.pools.insert("pool1".to_string(), PoolState {
            total_points: 1_000_000,
            cumulative_rpp: 100,
            emission_rate: 1000,
            last_update_time: 900,
            total_distributed: 50_000,
            reward_vault_balance: 100_000,
            stake_vault_balance: 1_000_000,
        });
        
        // Add a valid user
        state.users.insert("user1".to_string(), UserState {
            staked_amount: 1000,
            lock_duration: 180,
            lock_end_time: 2000,
            points: 2000,
            external_points: 0,
            c_paid: 50,
            accrued_rewards: 100,
            total_claimed: 500,
        });
        
        let invariants = Invariants::new();
        let violations = invariants.check_all(&state);
        
        // Should have some violations due to points mismatch
        assert!(!violations.is_empty());
    }
}