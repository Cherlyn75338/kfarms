use anchor_lang::prelude::*;

/// CPI (Cross-Program Invocation) safety validator
pub struct CPISafetyValidator;

impl CPISafetyValidator {
    /// Validate CPI target program
    pub fn validate_target_program(
        program_id: &Pubkey,
        allowed_programs: &[Pubkey],
    ) -> Result<(), CPIError> {
        if !allowed_programs.contains(program_id) {
            return Err(CPIError::UnauthorizedProgram(*program_id));
        }
        Ok(())
    }

    /// Check for reentrancy risks
    pub fn check_reentrancy(
        calling_program: &Pubkey,
        target_program: &Pubkey,
    ) -> Result<(), CPIError> {
        if calling_program == target_program {
            return Err(CPIError::PotentialReentrancy);
        }
        Ok(())
    }

    /// Validate signer seeds for PDA
    pub fn validate_signer_seeds(
        seeds: &[&[u8]],
        expected_pda: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<(), CPIError> {
        let (derived_pda, _) = Pubkey::find_program_address(seeds, program_id);
        
        if derived_pda != *expected_pda {
            return Err(CPIError::InvalidSignerSeeds);
        }
        
        Ok(())
    }

    /// Analyze CPI depth
    pub fn check_cpi_depth(current_depth: u8, max_depth: u8) -> Result<(), CPIError> {
        if current_depth >= max_depth {
            return Err(CPIError::MaxDepthExceeded);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CPIError {
    #[error("Unauthorized program: {0}")]
    UnauthorizedProgram(Pubkey),
    
    #[error("Potential reentrancy detected")]
    PotentialReentrancy,
    
    #[error("Invalid signer seeds")]
    InvalidSignerSeeds,
    
    #[error("Max CPI depth exceeded")]
    MaxDepthExceeded,
}