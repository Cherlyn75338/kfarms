use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use std::collections::HashMap;

/// PDA (Program Derived Address) security checker
#[derive(Debug, Clone)]
pub struct PDAChecker {
    pub program_id: Pubkey,
    pub known_seeds: HashMap<String, Vec<Vec<u8>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PDAError {
    #[error("Invalid PDA derivation for {name}: expected {expected}, got {actual}")]
    InvalidDerivation {
        name: String,
        expected: Pubkey,
        actual: Pubkey,
    },
    
    #[error("Bump seed mismatch for {name}: expected {expected}, got {actual}")]
    BumpMismatch {
        name: String,
        expected: u8,
        actual: u8,
    },
    
    #[error("Seeds collision detected for PDAs: {pda1} and {pda2}")]
    SeedsCollision { pda1: String, pda2: String },
    
    #[error("Missing required seed: {seed}")]
    MissingSeed { seed: String },
    
    #[error("Client-provided bump ignored - must derive internally")]
    ClientProvidedBump,
    
    #[error("PDA not on curve - invalid derivation")]
    NotOnCurve,
}

impl PDAChecker {
    pub fn new(program_id: Pubkey) -> Self {
        Self {
            program_id,
            known_seeds: HashMap::new(),
        }
    }

    /// Register expected seeds for a PDA
    pub fn register_seeds(&mut self, name: String, seeds: Vec<Vec<u8>>) {
        self.known_seeds.insert(name, seeds);
    }

    /// Derive and validate a PDA
    pub fn derive_and_validate(
        &self,
        seeds: &[&[u8]],
        expected_address: Option<Pubkey>,
    ) -> Result<(Pubkey, u8), PDAError> {
        let (pda, bump) = Pubkey::find_program_address(seeds, &self.program_id);
        
        // Verify PDA is not on curve (critical security check)
        if pda.is_on_curve() {
            return Err(PDAError::NotOnCurve);
        }
        
        if let Some(expected) = expected_address {
            if pda != expected {
                return Err(PDAError::InvalidDerivation {
                    name: "unknown".to_string(),
                    expected,
                    actual: pda,
                });
            }
        }
        
        Ok((pda, bump))
    }

    /// Validate pool PDA derivation
    pub fn validate_pool_pda(
        &self,
        pool_id: &[u8],
        mint: &Pubkey,
        expected_pda: &Pubkey,
    ) -> Result<u8, PDAError> {
        let seeds = &[
            b"pool",
            pool_id,
            mint.as_ref(),
        ];
        
        let (pda, bump) = self.derive_and_validate(seeds, Some(*expected_pda))?;
        
        if pda != *expected_pda {
            return Err(PDAError::InvalidDerivation {
                name: "pool".to_string(),
                expected: *expected_pda,
                actual: pda,
            });
        }
        
        Ok(bump)
    }

    /// Validate user position PDA
    pub fn validate_user_position_pda(
        &self,
        pool: &Pubkey,
        user: &Pubkey,
        expected_pda: &Pubkey,
    ) -> Result<u8, PDAError> {
        let seeds = &[
            b"user_position",
            pool.as_ref(),
            user.as_ref(),
        ];
        
        let (pda, bump) = self.derive_and_validate(seeds, Some(*expected_pda))?;
        
        if pda != *expected_pda {
            return Err(PDAError::InvalidDerivation {
                name: "user_position".to_string(),
                expected: *expected_pda,
                actual: pda,
            });
        }
        
        Ok(bump)
    }

    /// Validate vault authority PDA
    pub fn validate_vault_authority(
        &self,
        vault_type: &str,
        pool: &Pubkey,
        expected_pda: &Pubkey,
    ) -> Result<u8, PDAError> {
        let seeds = &[
            vault_type.as_bytes(),
            b"authority",
            pool.as_ref(),
        ];
        
        let (pda, bump) = self.derive_and_validate(seeds, Some(*expected_pda))?;
        
        if pda != *expected_pda {
            return Err(PDAError::InvalidDerivation {
                name: format!("{}_authority", vault_type),
                expected: *expected_pda,
                actual: pda,
            });
        }
        
        Ok(bump)
    }

    /// Check for potential seed collisions
    pub fn check_seed_collisions(&self) -> Result<(), PDAError> {
        let mut derived_pdas: HashMap<Pubkey, String> = HashMap::new();
        
        for (name, seeds) in &self.known_seeds {
            let seed_refs: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();
            let (pda, _) = Pubkey::find_program_address(&seed_refs, &self.program_id);
            
            if let Some(existing_name) = derived_pdas.get(&pda) {
                return Err(PDAError::SeedsCollision {
                    pda1: existing_name.clone(),
                    pda2: name.clone(),
                });
            }
            
            derived_pdas.insert(pda, name.clone());
        }
        
        Ok(())
    }

    /// Validate that bumps are derived internally, not from client
    pub fn validate_bump_derivation(
        &self,
        instruction_data: &[u8],
    ) -> Result<(), PDAError> {
        // Check if instruction data contains bump seeds
        // This is simplified - real implementation would parse instruction properly
        
        // Look for patterns that suggest client-provided bumps
        for window in instruction_data.windows(2) {
            // If we see a single byte that could be a bump (250-255 are common)
            if window[0] >= 250 && window[1] == 0 {
                // Potential client-provided bump
                return Err(PDAError::ClientProvidedBump);
            }
        }
        
        Ok(())
    }

    /// Generate unique seeds for a new PDA
    pub fn generate_unique_seeds(
        &self,
        base_seed: &[u8],
        discriminator: &[u8],
    ) -> Vec<Vec<u8>> {
        vec![
            base_seed.to_vec(),
            discriminator.to_vec(),
            self.program_id.to_bytes().to_vec(),
        ]
    }

    /// Validate external points setter PDA
    pub fn validate_external_points_pda(
        &self,
        external_program: &Pubkey,
        user: &Pubkey,
        pool: &Pubkey,
        expected_pda: &Pubkey,
    ) -> Result<u8, PDAError> {
        let seeds = &[
            b"external_points",
            external_program.as_ref(),
            user.as_ref(),
            pool.as_ref(),
        ];
        
        let (pda, bump) = self.derive_and_validate(seeds, Some(*expected_pda))?;
        
        if pda != *expected_pda {
            return Err(PDAError::InvalidDerivation {
                name: "external_points".to_string(),
                expected: *expected_pda,
                actual: pda,
            });
        }
        
        Ok(bump)
    }
}

/// Seeds pattern analyzer for security review
pub struct SeedsAnalyzer;

impl SeedsAnalyzer {
    /// Check if seeds contain user-controlled data
    pub fn has_user_controlled_seeds(seeds: &[&[u8]]) -> bool {
        // Check for patterns that suggest user control
        for seed in seeds {
            // Short seeds (< 8 bytes) might be user-provided IDs
            if seed.len() < 8 && seed.len() > 0 {
                return true;
            }
        }
        false
    }

    /// Check for predictable seeds
    pub fn has_predictable_seeds(seeds: &[&[u8]]) -> bool {
        for seed in seeds {
            // Check for sequential patterns
            if seed.len() >= 2 {
                let mut is_sequential = true;
                for i in 1..seed.len() {
                    if seed[i] != seed[i-1] + 1 {
                        is_sequential = false;
                        break;
                    }
                }
                if is_sequential {
                    return true;
                }
            }
            
            // Check for all zeros or all same byte
            if seed.iter().all(|&b| b == seed[0]) {
                return true;
            }
        }
        false
    }

    /// Analyze seed entropy
    pub fn calculate_seed_entropy(seeds: &[&[u8]]) -> f64 {
        let mut total_entropy = 0.0;
        
        for seed in seeds {
            if seed.is_empty() {
                continue;
            }
            
            // Simple entropy calculation based on byte distribution
            let mut byte_counts = [0u32; 256];
            for &byte in seed.iter() {
                byte_counts[byte as usize] += 1;
            }
            
            let len = seed.len() as f64;
            let mut entropy = 0.0;
            
            for count in byte_counts.iter() {
                if *count > 0 {
                    let probability = (*count as f64) / len;
                    entropy -= probability * probability.log2();
                }
            }
            
            total_entropy += entropy;
        }
        
        total_entropy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pda_derivation() {
        let program_id = Pubkey::new_unique();
        let checker = PDAChecker::new(program_id);
        
        let seeds = &[b"test", b"seeds"];
        let (pda, bump) = checker.derive_and_validate(seeds, None).unwrap();
        
        // Verify PDA is deterministic
        let (pda2, bump2) = checker.derive_and_validate(seeds, None).unwrap();
        assert_eq!(pda, pda2);
        assert_eq!(bump, bump2);
        
        // Verify PDA is not on curve
        assert!(!pda.is_on_curve());
    }

    #[test]
    fn test_seed_collision_detection() {
        let program_id = Pubkey::new_unique();
        let mut checker = PDAChecker::new(program_id);
        
        // Register different seeds
        checker.register_seeds("pda1".to_string(), vec![b"seed1".to_vec()]);
        checker.register_seeds("pda2".to_string(), vec![b"seed2".to_vec()]);
        
        // Should not have collisions with different seeds
        assert!(checker.check_seed_collisions().is_ok());
    }

    #[test]
    fn test_seed_entropy() {
        let high_entropy_seeds: Vec<&[u8]> = vec![
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            &[255, 128, 64, 32, 16, 8, 4, 2, 1],
        ];
        
        let low_entropy_seeds: Vec<&[u8]> = vec![
            &[0, 0, 0, 0, 0],
            &[1, 1, 1, 1, 1],
        ];
        
        let high_entropy = SeedsAnalyzer::calculate_seed_entropy(&high_entropy_seeds);
        let low_entropy = SeedsAnalyzer::calculate_seed_entropy(&low_entropy_seeds);
        
        assert!(high_entropy > low_entropy);
    }

    #[test]
    fn test_predictable_seeds_detection() {
        let predictable: Vec<&[u8]> = vec![
            &[1, 2, 3, 4, 5], // Sequential
            &[0, 0, 0, 0],    // All zeros
        ];
        
        assert!(SeedsAnalyzer::has_predictable_seeds(&predictable));
        
        let unpredictable: Vec<&[u8]> = vec![
            &[45, 123, 89, 234, 12],
        ];
        
        assert!(!SeedsAnalyzer::has_predictable_seeds(&unpredictable));
    }
}