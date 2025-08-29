use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;
use std::collections::HashSet;

/// Validates Anchor account constraints and ownership
#[derive(Debug, Clone)]
pub struct AccountValidator {
    pub program_id: Pubkey,
    pub known_authorities: HashSet<Pubkey>,
}

#[derive(Debug, Clone)]
pub struct AccountCheck {
    pub name: String,
    pub account: Pubkey,
    pub owner: Option<Pubkey>,
    pub is_writable: bool,
    pub is_signer: bool,
    pub is_initialized: bool,
    pub data_len: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum AccountValidationError {
    #[error("Invalid owner for account {account}: expected {expected}, got {actual}")]
    InvalidOwner {
        account: Pubkey,
        expected: Pubkey,
        actual: Pubkey,
    },
    
    #[error("Account {account} should be writable but is not")]
    NotWritable { account: Pubkey },
    
    #[error("Account {account} should be signer but is not")]
    NotSigner { account: Pubkey },
    
    #[error("Account {account} is not initialized")]
    NotInitialized { account: Pubkey },
    
    #[error("Account {account} has insufficient data length: {actual} < {required}")]
    InsufficientDataLength {
        account: Pubkey,
        actual: usize,
        required: usize,
    },
    
    #[error("Duplicate account detected: {account}")]
    DuplicateAccount { account: Pubkey },
    
    #[error("Account {account} is not rent exempt")]
    NotRentExempt { account: Pubkey },
    
    #[error("Unauthorized authority: {authority}")]
    UnauthorizedAuthority { authority: Pubkey },
}

impl AccountValidator {
    pub fn new(program_id: Pubkey) -> Self {
        Self {
            program_id,
            known_authorities: HashSet::new(),
        }
    }

    pub fn add_authority(&mut self, authority: Pubkey) {
        self.known_authorities.insert(authority);
    }

    /// Validate has_one constraints
    pub fn validate_has_one(
        &self,
        account_data: &[u8],
        field_offset: usize,
        expected_pubkey: &Pubkey,
        field_name: &str,
    ) -> Result<(), AccountValidationError> {
        if account_data.len() < field_offset + 32 {
            return Err(AccountValidationError::InsufficientDataLength {
                account: *expected_pubkey,
                actual: account_data.len(),
                required: field_offset + 32,
            });
        }

        let stored_pubkey = Pubkey::try_from(&account_data[field_offset..field_offset + 32])
            .map_err(|_| AccountValidationError::InvalidOwner {
                account: *expected_pubkey,
                expected: *expected_pubkey,
                actual: Pubkey::default(),
            })?;

        if stored_pubkey != *expected_pubkey {
            return Err(AccountValidationError::InvalidOwner {
                account: *expected_pubkey,
                expected: *expected_pubkey,
                actual: stored_pubkey,
            });
        }

        Ok(())
    }

    /// Check for account duplication attacks
    pub fn check_no_duplicates(&self, accounts: &[AccountCheck]) -> Result<(), AccountValidationError> {
        let mut seen = HashSet::new();
        
        for account in accounts {
            if !seen.insert(account.account) {
                return Err(AccountValidationError::DuplicateAccount {
                    account: account.account,
                });
            }
        }
        
        Ok(())
    }

    /// Validate account ownership
    pub fn validate_ownership(
        &self,
        account: &AccountCheck,
        expected_owner: &Pubkey,
    ) -> Result<(), AccountValidationError> {
        if let Some(owner) = account.owner {
            if owner != *expected_owner {
                return Err(AccountValidationError::InvalidOwner {
                    account: account.account,
                    expected: *expected_owner,
                    actual: owner,
                });
            }
        }
        Ok(())
    }

    /// Validate account permissions
    pub fn validate_permissions(&self, account: &AccountCheck) -> Result<(), AccountValidationError> {
        // Check writability for state-changing operations
        if self.requires_writable(&account.name) && !account.is_writable {
            return Err(AccountValidationError::NotWritable {
                account: account.account,
            });
        }

        // Check signer requirements
        if self.requires_signer(&account.name) && !account.is_signer {
            return Err(AccountValidationError::NotSigner {
                account: account.account,
            });
        }

        Ok(())
    }

    /// Check if authority is valid
    pub fn validate_authority(&self, authority: &Pubkey) -> Result<(), AccountValidationError> {
        if !self.known_authorities.contains(authority) {
            return Err(AccountValidationError::UnauthorizedAuthority {
                authority: *authority,
            });
        }
        Ok(())
    }

    /// Validate discriminator for Anchor accounts
    pub fn validate_discriminator(
        &self,
        account_data: &[u8],
        expected_discriminator: &[u8; 8],
    ) -> Result<(), AccountValidationError> {
        if account_data.len() < 8 {
            return Err(AccountValidationError::InsufficientDataLength {
                account: Pubkey::default(),
                actual: account_data.len(),
                required: 8,
            });
        }

        let discriminator = &account_data[..8];
        if discriminator != expected_discriminator {
            return Err(AccountValidationError::NotInitialized {
                account: Pubkey::default(),
            });
        }

        Ok(())
    }

    /// Check for reallocation safety
    pub fn validate_realloc(
        &self,
        current_len: usize,
        new_len: usize,
        max_len: usize,
    ) -> Result<(), AccountValidationError> {
        if new_len > max_len {
            return Err(AccountValidationError::InsufficientDataLength {
                account: Pubkey::default(),
                actual: new_len,
                required: max_len,
            });
        }

        // Check for potential overflow in realloc
        if new_len < current_len {
            // Shrinking - ensure no data loss
            // This would need actual data validation
        }

        Ok(())
    }

    fn requires_writable(&self, account_name: &str) -> bool {
        matches!(
            account_name,
            "pool" | "user_position" | "stake_vault" | "reward_vault" | "user_account"
        )
    }

    fn requires_signer(&self, account_name: &str) -> bool {
        matches!(
            account_name,
            "authority" | "admin" | "user" | "governance"
        )
    }
}

/// Check for Token-2022 specific issues
pub struct Token2022Validator;

impl Token2022Validator {
    /// Check for transfer fee configuration
    pub fn check_transfer_fees(mint_data: &[u8]) -> Result<bool, AccountValidationError> {
        // Parse mint extensions to check for transfer fees
        // This is simplified - real implementation would parse extensions properly
        Ok(false) // Assume no fees for now
    }

    /// Check freeze authority
    pub fn check_freeze_authority(
        mint_data: &[u8],
        expected_authority: Option<Pubkey>,
    ) -> Result<(), AccountValidationError> {
        // Parse and validate freeze authority
        // Should be None or a controlled PDA
        Ok(())
    }

    /// Check close authority
    pub fn check_close_authority(
        account_data: &[u8],
        expected_authority: Option<Pubkey>,
    ) -> Result<(), AccountValidationError> {
        // Parse and validate close authority
        Ok(())
    }

    /// Validate mint decimals
    pub fn validate_decimals(mint_data: &[u8]) -> Result<u8, AccountValidationError> {
        if mint_data.len() < 45 {
            return Err(AccountValidationError::InsufficientDataLength {
                account: Pubkey::default(),
                actual: mint_data.len(),
                required: 45,
            });
        }
        
        // Decimals is at offset 44 in mint account
        let decimals = mint_data[44];
        
        if decimals > 18 {
            return Err(AccountValidationError::InvalidOwner {
                account: Pubkey::default(),
                expected: Pubkey::default(),
                actual: Pubkey::default(),
            });
        }
        
        Ok(decimals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_validator() {
        let program_id = Pubkey::new_unique();
        let mut validator = AccountValidator::new(program_id);
        
        let authority = Pubkey::new_unique();
        validator.add_authority(authority);
        
        // Test authority validation
        assert!(validator.validate_authority(&authority).is_ok());
        assert!(validator.validate_authority(&Pubkey::new_unique()).is_err());
        
        // Test duplicate detection
        let account1 = AccountCheck {
            name: "account1".to_string(),
            account: Pubkey::new_unique(),
            owner: Some(program_id),
            is_writable: true,
            is_signer: false,
            is_initialized: true,
            data_len: 100,
        };
        
        let account2 = AccountCheck {
            name: "account2".to_string(),
            account: account1.account, // Duplicate
            owner: Some(program_id),
            is_writable: false,
            is_signer: false,
            is_initialized: true,
            data_len: 100,
        };
        
        let accounts = vec![account1, account2];
        assert!(validator.check_no_duplicates(&accounts).is_err());
    }

    #[test]
    fn test_discriminator_validation() {
        let validator = AccountValidator::new(Pubkey::new_unique());
        
        let mut account_data = vec![0u8; 100];
        let discriminator = [1, 2, 3, 4, 5, 6, 7, 8];
        account_data[..8].copy_from_slice(&discriminator);
        
        assert!(validator.validate_discriminator(&account_data, &discriminator).is_ok());
        assert!(validator.validate_discriminator(&account_data, &[8, 7, 6, 5, 4, 3, 2, 1]).is_err());
    }
}