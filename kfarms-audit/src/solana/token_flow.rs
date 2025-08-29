use anchor_lang::prelude::*;
use spl_token::state::{Account as TokenAccount, Mint};

/// Token flow analyzer for SPL tokens
pub struct TokenFlowAnalyzer;

impl TokenFlowAnalyzer {
    /// Analyze token flow for deposit operation
    pub fn analyze_deposit_flow(
        user_token: &TokenAccount,
        vault_token: &TokenAccount,
        amount: u64,
    ) -> Result<FlowAnalysis, TokenFlowError> {
        // Check user has sufficient balance
        if user_token.amount < amount {
            return Err(TokenFlowError::InsufficientBalance);
        }

        // Check vault can receive
        if vault_token.amount.checked_add(amount).is_none() {
            return Err(TokenFlowError::VaultOverflow);
        }

        Ok(FlowAnalysis {
            source: user_token.owner,
            destination: vault_token.owner,
            amount,
            flow_type: FlowType::Deposit,
        })
    }

    /// Verify token mint consistency
    pub fn verify_mint_consistency(
        accounts: &[TokenAccount],
        expected_mint: &Pubkey,
    ) -> Result<(), TokenFlowError> {
        for account in accounts {
            if account.mint != *expected_mint {
                return Err(TokenFlowError::MintMismatch {
                    expected: *expected_mint,
                    actual: account.mint,
                });
            }
        }
        Ok(())
    }

    /// Check for token account delegation risks
    pub fn check_delegation_risk(account: &TokenAccount) -> Option<DelegationRisk> {
        if account.delegate.is_some() {
            Some(DelegationRisk {
                delegate: account.delegate,
                delegated_amount: account.delegated_amount,
            })
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct FlowAnalysis {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub flow_type: FlowType,
}

#[derive(Debug)]
pub enum FlowType {
    Deposit,
    Withdrawal,
    Claim,
    Transfer,
}

#[derive(Debug)]
pub struct DelegationRisk {
    pub delegate: Option<Pubkey>,
    pub delegated_amount: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenFlowError {
    #[error("Insufficient balance")]
    InsufficientBalance,
    
    #[error("Vault overflow")]
    VaultOverflow,
    
    #[error("Mint mismatch: expected {expected}, got {actual}")]
    MintMismatch { expected: Pubkey, actual: Pubkey },
    
    #[error("Unexpected delegation")]
    UnexpectedDelegation,
}