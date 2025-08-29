use anchor_lang::prelude::*;

/// Governance attack threat model
pub struct GovernanceThreatModel;

impl GovernanceThreatModel {
    /// Detect vote weight manipulation
    pub fn detect_vote_manipulation(
        snapshots: &[VoteSnapshot],
        max_change_rate: f64,
    ) -> Vec<ManipulationRisk> {
        let mut risks = Vec::new();
        
        for window in snapshots.windows(2) {
            let prev = &window[0];
            let curr = &window[1];
            
            let change_rate = (curr.total_weight as f64 - prev.total_weight as f64).abs() 
                / prev.total_weight as f64;
            
            if change_rate > max_change_rate {
                risks.push(ManipulationRisk::SuddenWeightChange {
                    from: prev.total_weight,
                    to: curr.total_weight,
                    slot: curr.slot,
                });
            }
        }
        
        risks
    }

    /// Check for flash loan governance attacks
    pub fn check_flash_loan_risk(
        borrow_events: &[BorrowEvent],
        vote_events: &[VoteEvent],
    ) -> bool {
        for borrow in borrow_events {
            for vote in vote_events {
                // Check if vote happened during borrow window
                if vote.slot >= borrow.start_slot && vote.slot <= borrow.end_slot {
                    return true;
                }
            }
        }
        false
    }

    /// Validate proposal timing
    pub fn validate_proposal_timing(
        proposal: &Proposal,
        current_slot: u64,
    ) -> Result<(), GovernanceError> {
        // Check minimum delay
        if proposal.execution_slot < current_slot + proposal.min_delay {
            return Err(GovernanceError::InsufficientDelay);
        }
        
        // Check not expired
        if current_slot > proposal.expiration_slot {
            return Err(GovernanceError::ProposalExpired);
        }
        
        Ok(())
    }
}

#[derive(Debug)]
pub struct VoteSnapshot {
    pub slot: u64,
    pub total_weight: u128,
    pub voter_count: u32,
}

#[derive(Debug)]
pub struct BorrowEvent {
    pub start_slot: u64,
    pub end_slot: u64,
    pub amount: u128,
}

#[derive(Debug)]
pub struct VoteEvent {
    pub slot: u64,
    pub voter: Pubkey,
    pub weight: u128,
}

#[derive(Debug)]
pub struct Proposal {
    pub execution_slot: u64,
    pub expiration_slot: u64,
    pub min_delay: u64,
}

#[derive(Debug)]
pub enum ManipulationRisk {
    SuddenWeightChange { from: u128, to: u128, slot: u64 },
    FlashLoanVoting,
    TimingManipulation,
}

#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("Insufficient delay before execution")]
    InsufficientDelay,
    
    #[error("Proposal has expired")]
    ProposalExpired,
    
    #[error("Vote weight manipulation detected")]
    ManipulationDetected,
}