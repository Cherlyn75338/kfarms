use anchor_lang::prelude::*;
use std::collections::HashMap;

/// External points setter threat model and controls
#[derive(Debug, Clone)]
pub struct ExternalPointsThreatModel {
    pub max_points_change_per_slot: u128,
    pub max_points_change_per_epoch: u128,
    pub min_lock_horizon: u64,
    pub twap_window: u64,
    pub ema_alpha_bps: u128,
    pub whitelisted_providers: Vec<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct PointsUpdate {
    pub user: Pubkey,
    pub provider: Pubkey,
    pub new_points: u128,
    pub old_points: u128,
    pub slot: u64,
    pub timestamp: u64,
    pub custody_proof: Option<CustodyProof>,
}

#[derive(Debug, Clone)]
pub struct CustodyProof {
    pub provider_pda: Pubkey,
    pub locked_amount: u128,
    pub lock_end_time: u64,
    pub merkle_root: Option<[u8; 32]>,
    pub signature: Option<[u8; 64]>,
    pub nonce: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExternalPointsThreatError {
    #[error("Flash points attack detected: {description}")]
    FlashPointsAttack { description: String },
    
    #[error("Custody mismatch: points {points} but custody {custody}")]
    CustodyMismatch { points: u128, custody: u128 },
    
    #[error("Provider not whitelisted: {provider}")]
    UnauthorizedProvider { provider: Pubkey },
    
    #[error("Points change too large: {change} > {max}")]
    ExcessivePointsChange { change: u128, max: u128 },
    
    #[error("Insufficient lock horizon: {remaining} < {required}")]
    InsufficientLockHorizon { remaining: u64, required: u64 },
    
    #[error("Replay attack detected: nonce {nonce} already used")]
    ReplayAttack { nonce: u64 },
    
    #[error("Stale proof: slot {proof_slot} too old (current: {current_slot})")]
    StaleProof { proof_slot: u64, current_slot: u64 },
    
    #[error("Sandwich attack pattern detected")]
    SandwichAttack,
    
    #[error("TWAP manipulation detected: instant change {instant} vs TWAP {twap}")]
    TWAPManipulation { instant: u128, twap: u128 },
}

impl ExternalPointsThreatModel {
    pub fn new() -> Self {
        Self {
            max_points_change_per_slot: 10_000,
            max_points_change_per_epoch: 100_000,
            min_lock_horizon: 86400 * 7, // 7 days
            twap_window: 3600, // 1 hour
            ema_alpha_bps: 1000, // 10% weight to new values
            whitelisted_providers: Vec::new(),
        }
    }

    /// Validate a points update against threat model
    pub fn validate_update(&self, update: &PointsUpdate) -> Result<(), ExternalPointsThreatError> {
        // Check provider whitelist
        if !self.whitelisted_providers.contains(&update.provider) {
            return Err(ExternalPointsThreatError::UnauthorizedProvider {
                provider: update.provider,
            });
        }

        // Check points change magnitude
        let change = update.new_points.abs_diff(update.old_points);
        if change > self.max_points_change_per_slot {
            return Err(ExternalPointsThreatError::ExcessivePointsChange {
                change,
                max: self.max_points_change_per_slot,
            });
        }

        // Validate custody proof if provided
        if let Some(ref proof) = update.custody_proof {
            self.validate_custody_proof(proof, update.new_points)?;
        }

        Ok(())
    }

    /// Validate custody proof
    fn validate_custody_proof(
        &self,
        proof: &CustodyProof,
        claimed_points: u128,
    ) -> Result<(), ExternalPointsThreatError> {
        // Check custody matches claimed points
        if claimed_points > proof.locked_amount * 3 {
            // Max 3x multiplier assumption
            return Err(ExternalPointsThreatError::CustodyMismatch {
                points: claimed_points,
                custody: proof.locked_amount,
            });
        }

        // Check lock horizon
        let current_time = Clock::get().unwrap().unix_timestamp as u64;
        let remaining_lock = proof.lock_end_time.saturating_sub(current_time);
        
        if remaining_lock < self.min_lock_horizon {
            return Err(ExternalPointsThreatError::InsufficientLockHorizon {
                remaining: remaining_lock,
                required: self.min_lock_horizon,
            });
        }

        Ok(())
    }

    /// Detect flash points attack patterns
    pub fn detect_flash_attack(
        &self,
        recent_updates: &[PointsUpdate],
        window_slots: u64,
    ) -> Result<(), ExternalPointsThreatError> {
        if recent_updates.len() < 2 {
            return Ok(());
        }

        let current_slot = recent_updates.last().unwrap().slot;
        let window_start = current_slot.saturating_sub(window_slots);

        // Group updates by user
        let mut user_updates: HashMap<Pubkey, Vec<&PointsUpdate>> = HashMap::new();
        for update in recent_updates {
            if update.slot >= window_start {
                user_updates.entry(update.user).or_default().push(update);
            }
        }

        // Check for flash patterns
        for (user, updates) in user_updates {
            if updates.len() >= 2 {
                // Check for large increase followed by decrease
                for i in 1..updates.len() {
                    let prev = updates[i - 1];
                    let curr = updates[i];
                    
                    // Flash pattern: huge increase then decrease
                    if prev.new_points > prev.old_points * 10 &&
                       curr.new_points < prev.new_points / 10 {
                        return Err(ExternalPointsThreatError::FlashPointsAttack {
                            description: format!("User {} flash points: {} -> {} -> {}", 
                                user, prev.old_points, prev.new_points, curr.new_points),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Detect sandwich attack patterns
    pub fn detect_sandwich_attack(
        &self,
        updates_before: &[PointsUpdate],
        target_action: &str,
        updates_after: &[PointsUpdate],
    ) -> Result<(), ExternalPointsThreatError> {
        // Check if same user increased points before and decreased after
        for before in updates_before {
            for after in updates_after {
                if before.user == after.user {
                    // Points increased before target action
                    let increased_before = before.new_points > before.old_points * 2;
                    // Points decreased after target action
                    let decreased_after = after.new_points < after.old_points / 2;
                    
                    if increased_before && decreased_after {
                        return Err(ExternalPointsThreatError::SandwichAttack);
                    }
                }
            }
        }

        Ok(())
    }

    /// Calculate TWAP to prevent manipulation
    pub fn calculate_points_twap(
        &self,
        historical_points: &[(u128, u64)], // (points, timestamp)
        window_start: u64,
        window_end: u64,
    ) -> u128 {
        if historical_points.is_empty() {
            return 0;
        }

        let mut weighted_sum: u128 = 0;
        let mut total_weight: u64 = 0;

        for i in 0..historical_points.len() {
            let (points, timestamp) = historical_points[i];
            
            if timestamp < window_start {
                continue;
            }
            if timestamp > window_end {
                break;
            }

            // Calculate weight (time until next update or window end)
            let next_time = if i + 1 < historical_points.len() {
                historical_points[i + 1].1.min(window_end)
            } else {
                window_end
            };

            let weight = next_time.saturating_sub(timestamp);
            weighted_sum = weighted_sum.saturating_add(points.saturating_mul(weight as u128));
            total_weight = total_weight.saturating_add(weight);
        }

        if total_weight > 0 {
            weighted_sum / (total_weight as u128)
        } else {
            0
        }
    }

    /// Apply EMA smoothing to points changes
    pub fn apply_ema_smoothing(
        &self,
        new_points: u128,
        current_smoothed: u128,
    ) -> u128 {
        let alpha = self.ema_alpha_bps;
        let weighted_new = new_points.saturating_mul(alpha);
        let weighted_old = current_smoothed.saturating_mul(10_000 - alpha);
        
        (weighted_new + weighted_old) / 10_000
    }

    /// Validate against TWAP manipulation
    pub fn validate_twap_bounds(
        &self,
        instant_points: u128,
        twap_points: u128,
        max_deviation_bps: u128,
    ) -> Result<(), ExternalPointsThreatError> {
        let deviation = instant_points.abs_diff(twap_points);
        let max_allowed = (twap_points * max_deviation_bps) / 10_000;
        
        if deviation > max_allowed {
            return Err(ExternalPointsThreatError::TWAPManipulation {
                instant: instant_points,
                twap: twap_points,
            });
        }

        Ok(())
    }

    /// Check for replay attacks
    pub fn check_replay_protection(
        &self,
        used_nonces: &HashMap<u64, u64>, // nonce -> slot used
        new_nonce: u64,
        current_slot: u64,
        max_age_slots: u64,
    ) -> Result<(), ExternalPointsThreatError> {
        // Check if nonce was already used
        if let Some(&used_slot) = used_nonces.get(&new_nonce) {
            // Allow reuse only if very old (expired)
            if current_slot - used_slot < max_age_slots {
                return Err(ExternalPointsThreatError::ReplayAttack {
                    nonce: new_nonce,
                });
            }
        }

        Ok(())
    }

    /// Validate proof freshness
    pub fn validate_proof_freshness(
        &self,
        proof_slot: u64,
        current_slot: u64,
        max_age: u64,
    ) -> Result<(), ExternalPointsThreatError> {
        if current_slot > proof_slot + max_age {
            return Err(ExternalPointsThreatError::StaleProof {
                proof_slot,
                current_slot,
            });
        }

        Ok(())
    }
}

/// CPI safety validator for external points
pub struct ExternalCPIValidator;

impl ExternalCPIValidator {
    /// Validate CPI call from external provider
    pub fn validate_cpi(
        provider_program: &Pubkey,
        instruction_data: &[u8],
        accounts: &[AccountInfo],
    ) -> Result<(), ExternalPointsThreatError> {
        // Verify provider program is the actual signer
        let provider_account = accounts.iter()
            .find(|a| a.key == provider_program)
            .ok_or(ExternalPointsThreatError::UnauthorizedProvider {
                provider: *provider_program,
            })?;

        if !provider_account.is_signer {
            return Err(ExternalPointsThreatError::UnauthorizedProvider {
                provider: *provider_program,
            });
        }

        // Additional CPI validation...
        Ok(())
    }

    /// Verify custody accounts in CPI
    pub fn verify_custody_accounts(
        user: &Pubkey,
        provider_program: &Pubkey,
        custody_accounts: &[AccountInfo],
    ) -> Result<u128, ExternalPointsThreatError> {
        // This would verify actual token accounts and balances
        // Simplified for example
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flash_attack_detection() {
        let model = ExternalPointsThreatModel::new();
        let user = Pubkey::new_unique();
        let provider = Pubkey::new_unique();

        let updates = vec![
            PointsUpdate {
                user,
                provider,
                new_points: 1000,
                old_points: 100,
                slot: 100,
                timestamp: 1000,
                custody_proof: None,
            },
            PointsUpdate {
                user,
                provider,
                new_points: 100000, // Flash increase
                old_points: 1000,
                slot: 101,
                timestamp: 1001,
                custody_proof: None,
            },
            PointsUpdate {
                user,
                provider,
                new_points: 100, // Flash decrease
                old_points: 100000,
                slot: 102,
                timestamp: 1002,
                custody_proof: None,
            },
        ];

        let result = model.detect_flash_attack(&updates, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_twap_calculation() {
        let model = ExternalPointsThreatModel::new();

        let historical = vec![
            (1000, 0),
            (2000, 100),
            (1500, 200),
            (3000, 300),
        ];

        let twap = model.calculate_points_twap(&historical, 0, 400);
        
        // TWAP should be weighted average over time
        assert!(twap > 1000 && twap < 3000);
    }

    #[test]
    fn test_ema_smoothing() {
        let model = ExternalPointsThreatModel::new();

        let smoothed = model.apply_ema_smoothing(1000, 500);
        
        // With 10% alpha, should be 0.1 * 1000 + 0.9 * 500 = 550
        assert_eq!(smoothed, 550);
    }

    #[test]
    fn test_replay_protection() {
        let model = ExternalPointsThreatModel::new();
        let mut used_nonces = HashMap::new();
        
        // First use should be OK
        assert!(model.check_replay_protection(&used_nonces, 12345, 100, 1000).is_ok());
        
        // Mark as used
        used_nonces.insert(12345, 100);
        
        // Immediate reuse should fail
        assert!(model.check_replay_protection(&used_nonces, 12345, 101, 1000).is_err());
        
        // Very old reuse should be OK
        assert!(model.check_replay_protection(&used_nonces, 12345, 1200, 1000).is_ok());
    }
}