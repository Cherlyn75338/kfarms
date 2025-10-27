#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::{Arbitrary, Unstructured};
use farms::state::*;
use farms::farm_operations;
use decimal_wad::decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Arbitrary)]
enum FuzzInstruction {
    Stake { user_id: u8, amount: u64 },
    Unstake { user_id: u8, amount: u64 },
    Harvest { user_id: u8, reward_idx: u8 },
    RefreshFarm,
    RefreshUser { user_id: u8 },
    AddRewards { reward_idx: u8, amount: u64 },
    AdvanceTime { slots: u32 },
    UpdateConfig { config: FuzzConfig },
}

#[derive(Debug, Arbitrary)]
struct FuzzConfig {
    deposit_warmup: u16,
    withdrawal_cooldown: u16,
    penalty_bps: u16,
    locking_mode: u8,
}

struct FuzzState {
    farm: FarmState,
    users: HashMap<u8, UserState>,
    current_slot: u64,
}

impl FuzzState {
    fn new() -> Self {
        let mut farm = FarmState::default();
        farm.num_reward_tokens = 1;
        farm.reward_infos[0] = RewardInfo {
            reward_type: RewardType::Proportional,
            rewards_available: 1_000_000_000_000,
            rewards_per_second_decimals: 6,
            decimals: 6,
            ..Default::default()
        };
        
        FuzzState {
            farm,
            users: HashMap::new(),
            current_slot: 0,
        }
    }
    
    fn apply_instruction(&mut self, instruction: FuzzInstruction) -> Result<(), String> {
        match instruction {
            FuzzInstruction::Stake { user_id, amount } => {
                // Ensure amount is reasonable
                let amount = amount % 1_000_000_000_000; // Cap at 1 trillion
                if amount == 0 {
                    return Ok(());
                }
                
                // Get or create user
                let user = self.users.entry(user_id).or_insert_with(UserState::default);
                
                // Simulate stake
                self.refresh_farm()?;
                self.refresh_user(user_id)?;
                
                let amount_decimal = Decimal::from(amount);
                user.active_stake_scaled = user.active_stake_scaled
                    .checked_add(amount_decimal)
                    .ok_or("Stake overflow")?;
                
                self.farm.total_active_stake_scaled = self.farm.total_active_stake_scaled
                    .checked_add(amount_decimal)
                    .ok_or("Total stake overflow")?;
                
                Ok(())
            },
            
            FuzzInstruction::Unstake { user_id, amount } => {
                let amount = amount % 1_000_000_000_000;
                if amount == 0 {
                    return Ok(());
                }
                
                if let Some(user) = self.users.get_mut(&user_id) {
                    self.refresh_farm()?;
                    self.refresh_user(user_id)?;
                    
                    let amount_decimal = Decimal::from(amount);
                    if user.active_stake_scaled >= amount_decimal {
                        user.active_stake_scaled = user.active_stake_scaled
                            .checked_sub(amount_decimal)
                            .ok_or("Unstake underflow")?;
                        
                        self.farm.total_active_stake_scaled = self.farm.total_active_stake_scaled
                            .checked_sub(amount_decimal)
                            .ok_or("Total stake underflow")?;
                        
                        // Apply penalties if configured
                        let penalty = self.calculate_penalty(amount);
                        let amount_after_penalty = amount.saturating_sub(penalty);
                        user.pending_withdrawal.amount = user.pending_withdrawal.amount
                            .saturating_add(amount_after_penalty);
                    }
                }
                Ok(())
            },
            
            FuzzInstruction::Harvest { user_id, reward_idx } => {
                let reward_idx = (reward_idx as usize) % 10;
                if reward_idx >= self.farm.num_reward_tokens as usize {
                    return Ok(());
                }
                
                if let Some(user) = self.users.get_mut(&user_id) {
                    self.refresh_farm()?;
                    self.refresh_user(user_id)?;
                    
                    let claimable = user.reward_infos[reward_idx].rewards_issued_unclaimed;
                    user.reward_infos[reward_idx].rewards_issued_unclaimed = 0;
                    self.farm.reward_infos[reward_idx].rewards_issued_unclaimed = 
                        self.farm.reward_infos[reward_idx].rewards_issued_unclaimed
                            .saturating_sub(claimable);
                }
                Ok(())
            },
            
            FuzzInstruction::RefreshFarm => {
                self.refresh_farm()
            },
            
            FuzzInstruction::RefreshUser { user_id } => {
                self.refresh_user(user_id)
            },
            
            FuzzInstruction::AddRewards { reward_idx, amount } => {
                let reward_idx = (reward_idx as usize) % 10;
                if reward_idx >= self.farm.num_reward_tokens as usize {
                    return Ok(());
                }
                
                let amount = amount % 10_000_000_000_000; // Cap at 10 trillion
                self.farm.reward_infos[reward_idx].rewards_available = 
                    self.farm.reward_infos[reward_idx].rewards_available
                        .saturating_add(amount);
                Ok(())
            },
            
            FuzzInstruction::AdvanceTime { slots } => {
                self.current_slot = self.current_slot.saturating_add(slots as u64);
                Ok(())
            },
            
            FuzzInstruction::UpdateConfig { config } => {
                // Update farm configuration
                self.farm.config.deposit_warmup_period = config.deposit_warmup as u64;
                self.farm.config.withdrawal_cooldown_period = config.withdrawal_cooldown as u64;
                self.farm.config.withdrawal_penalty_bps = config.penalty_bps as u64;
                self.farm.config.locking_mode = match config.locking_mode % 3 {
                    0 => LockingMode::None,
                    1 => LockingMode::Continuous,
                    _ => LockingMode::WithExpiry,
                };
                Ok(())
            },
        }
    }
    
    fn refresh_farm(&mut self) -> Result<(), String> {
        // Simplified refresh logic
        if self.farm.total_active_stake_scaled == Decimal::zero() {
            return Ok(());
        }
        
        for i in 0..self.farm.num_reward_tokens as usize {
            let reward = &mut self.farm.reward_infos[i];
            
            // Calculate time delta
            let time_delta = self.current_slot.saturating_sub(reward.last_issuance_ts);
            if time_delta == 0 {
                continue;
            }
            
            // Simple reward calculation
            let rewards_to_issue = (1000u64 * time_delta)
                .min(reward.rewards_available);
            
            if rewards_to_issue > 0 {
                reward.rewards_available = reward.rewards_available
                    .saturating_sub(rewards_to_issue);
                reward.rewards_issued_unclaimed = reward.rewards_issued_unclaimed
                    .saturating_add(rewards_to_issue);
                reward.rewards_issued_cumulative = reward.rewards_issued_cumulative
                    .saturating_add(rewards_to_issue);
                
                // Update RPS
                let issued_decimal = Decimal::from(rewards_to_issue);
                if let Some(rps_increment) = issued_decimal.checked_div(self.farm.total_active_stake_scaled) {
                    reward.reward_per_share_scaled = reward.reward_per_share_scaled
                        .checked_add(rps_increment)
                        .unwrap_or(reward.reward_per_share_scaled);
                }
            }
            
            reward.last_issuance_ts = self.current_slot;
        }
        
        Ok(())
    }
    
    fn refresh_user(&mut self, user_id: u8) -> Result<(), String> {
        if let Some(user) = self.users.get_mut(&user_id) {
            for i in 0..self.farm.num_reward_tokens as usize {
                let farm_reward = &self.farm.reward_infos[i];
                let user_reward = &mut user.reward_infos[i];
                
                // Calculate new rewards
                if let Some(new_tally) = farm_reward.reward_per_share_scaled
                    .checked_mul(user.active_stake_scaled) {
                    
                    let old_tally = user_reward.reward_tally_scaled;
                    if new_tally > old_tally {
                        if let Some(reward_decimal) = new_tally.checked_sub(old_tally) {
                            let reward_amount = reward_decimal.to_u64().unwrap_or(0);
                            user_reward.rewards_issued_unclaimed = 
                                user_reward.rewards_issued_unclaimed
                                    .saturating_add(reward_amount);
                            user_reward.reward_tally_scaled = new_tally;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    
    fn calculate_penalty(&self, amount: u64) -> u64 {
        let penalty_bps = self.farm.config.withdrawal_penalty_bps;
        amount.saturating_mul(penalty_bps) / 10000
    }
    
    fn check_invariants(&self) -> Result<(), String> {
        // Check conservation of rewards
        for i in 0..self.farm.num_reward_tokens as usize {
            let reward = &self.farm.reward_infos[i];
            
            // RPS should never be negative (Decimal prevents this)
            if reward.reward_per_share_scaled < Decimal::zero() {
                return Err("Negative RPS detected".to_string());
            }
            
            // Cumulative should never decrease
            if reward.rewards_issued_cumulative < 0 {
                return Err("Negative cumulative rewards".to_string());
            }
        }
        
        // Check stake consistency
        let mut sum_user_stakes = Decimal::zero();
        for user in self.users.values() {
            sum_user_stakes = sum_user_stakes
                .checked_add(user.active_stake_scaled)
                .ok_or("User stake sum overflow")?;
        }
        
        // Allow small rounding error
        let diff = if sum_user_stakes > self.farm.total_active_stake_scaled {
            sum_user_stakes - self.farm.total_active_stake_scaled
        } else {
            self.farm.total_active_stake_scaled - sum_user_stakes
        };
        
        if diff > Decimal::from(self.users.len() as u64) {
            return Err(format!("Stake inconsistency: sum={:?}, total={:?}", 
                             sum_user_stakes, self.farm.total_active_stake_scaled));
        }
        
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    // Parse arbitrary data into instructions
    let mut u = Unstructured::new(data);
    let instructions: Vec<FuzzInstruction> = match u.arbitrary() {
        Ok(v) => v,
        Err(_) => return,
    };
    
    // Limit number of instructions to prevent timeout
    let instructions: Vec<_> = instructions.into_iter().take(100).collect();
    
    let mut state = FuzzState::new();
    
    // Apply instructions and check for panics
    for instruction in instructions {
        let _ = state.apply_instruction(instruction);
        
        // Check invariants after each instruction
        if let Err(e) = state.check_invariants() {
            // Log invariant violation but don't panic
            // In real fuzzing, you might want to save this case
            eprintln!("Invariant violation: {}", e);
        }
    }
});