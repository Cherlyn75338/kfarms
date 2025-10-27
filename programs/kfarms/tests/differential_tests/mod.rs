use num_bigint::{BigUint, BigInt};
use num_rational::BigRational;
use num_traits::{Zero, One, ToPrimitive};
use rand::prelude::*;
use std::collections::HashMap;
use farms::state::*;
use decimal_wad::decimal::Decimal;

pub mod reference_implementation;
pub mod differential_scenarios;
pub mod comparison_tests;

// High-precision reference implementation
#[derive(Debug, Clone)]
pub struct ReferenceFarm {
    pub total_staked: BigRational,
    pub total_active_stake: BigRational,
    pub rewards: Vec<ReferenceReward>,
    pub users: HashMap<usize, ReferenceUser>,
    pub current_time: u64,
    pub config: ReferenceFarmConfig,
}

#[derive(Debug, Clone)]
pub struct ReferenceReward {
    pub reward_type: RewardType,
    pub rewards_available: BigRational,
    pub rewards_issued_unclaimed: BigRational,
    pub rewards_issued_cumulative: BigRational,
    pub reward_per_share: BigRational, // No scaling needed in reference
    pub last_issuance_ts: u64,
    pub schedule: Vec<(u64, BigRational)>, // (timestamp, rate)
    pub decimals: u32,
    pub treasury_fee_bps: u64,
}

#[derive(Debug, Clone)]
pub struct ReferenceUser {
    pub active_stake: BigRational,
    pub pending_deposit: BigRational,
    pub pending_withdrawal: BigRational,
    pub pending_withdrawal_ts: u64,
    pub reward_tallies: Vec<BigRational>,
    pub rewards_unclaimed: Vec<BigRational>,
    pub last_claim_ts: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct ReferenceFarmConfig {
    pub deposit_warmup_period: u64,
    pub withdrawal_cooldown_period: u64,
    pub withdrawal_penalty_bps: u64,
    pub locking_mode: LockingMode,
    pub locking_start_timestamp: u64,
    pub locking_duration: u64,
    pub locking_early_withdrawal_penalty_bps: u64,
}

impl ReferenceFarm {
    pub fn new(num_rewards: usize) -> Self {
        ReferenceFarm {
            total_staked: BigRational::zero(),
            total_active_stake: BigRational::zero(),
            rewards: (0..num_rewards)
                .map(|_| ReferenceReward {
                    reward_type: RewardType::Proportional,
                    rewards_available: BigRational::zero(),
                    rewards_issued_unclaimed: BigRational::zero(),
                    rewards_issued_cumulative: BigRational::zero(),
                    reward_per_share: BigRational::zero(),
                    last_issuance_ts: 0,
                    schedule: vec![(0, BigRational::from_integer(BigInt::from(1000)))],
                    decimals: 6,
                    treasury_fee_bps: 0,
                })
                .collect(),
            users: HashMap::new(),
            current_time: 0,
            config: ReferenceFarmConfig {
                deposit_warmup_period: 0,
                withdrawal_cooldown_period: 0,
                withdrawal_penalty_bps: 0,
                locking_mode: LockingMode::None,
                locking_start_timestamp: 0,
                locking_duration: 0,
                locking_early_withdrawal_penalty_bps: 0,
            },
        }
    }
    
    pub fn stake(&mut self, user_id: usize, amount: BigRational) {
        self.refresh_global_rewards();
        self.refresh_user_rewards(user_id);
        
        let user = self.users.entry(user_id).or_insert_with(|| ReferenceUser {
            active_stake: BigRational::zero(),
            pending_deposit: BigRational::zero(),
            pending_withdrawal: BigRational::zero(),
            pending_withdrawal_ts: 0,
            reward_tallies: vec![BigRational::zero(); self.rewards.len()],
            rewards_unclaimed: vec![BigRational::zero(); self.rewards.len()],
            last_claim_ts: vec![0; self.rewards.len()],
        });
        
        if self.config.deposit_warmup_period > 0 {
            user.pending_deposit += &amount;
        } else {
            user.active_stake += &amount;
            self.total_active_stake += &amount;
        }
        
        self.total_staked += amount;
    }
    
    pub fn unstake(&mut self, user_id: usize, amount: BigRational) -> BigRational {
        self.refresh_global_rewards();
        self.refresh_user_rewards(user_id);
        
        let user = self.users.get_mut(&user_id).unwrap();
        
        // Calculate penalty if applicable
        let mut penalty = BigRational::zero();
        
        if self.config.withdrawal_penalty_bps > 0 {
            penalty = &amount * BigRational::from_integer(BigInt::from(self.config.withdrawal_penalty_bps))
                / BigRational::from_integer(BigInt::from(10000));
        }
        
        // Apply locking penalties
        match self.config.locking_mode {
            LockingMode::Continuous => {
                let lock_penalty = &amount * 
                    BigRational::from_integer(BigInt::from(self.config.locking_early_withdrawal_penalty_bps))
                    / BigRational::from_integer(BigInt::from(10000));
                penalty = penalty.max(lock_penalty);
            },
            LockingMode::WithExpiry => {
                let now = self.current_time;
                let lock_end = self.config.locking_start_timestamp + self.config.locking_duration;
                if now >= self.config.locking_start_timestamp && now < lock_end {
                    let lock_penalty = &amount * 
                        BigRational::from_integer(BigInt::from(self.config.locking_early_withdrawal_penalty_bps))
                        / BigRational::from_integer(BigInt::from(10000));
                    penalty = penalty.max(lock_penalty);
                }
            },
            _ => {}
        }
        
        let amount_after_penalty = &amount - &penalty;
        
        user.active_stake -= &amount;
        self.total_active_stake -= &amount;
        
        if self.config.withdrawal_cooldown_period > 0 {
            user.pending_withdrawal += &amount_after_penalty;
            user.pending_withdrawal_ts = self.current_time;
        }
        
        penalty
    }
    
    pub fn harvest(&mut self, user_id: usize, reward_idx: usize) -> BigRational {
        self.refresh_global_rewards();
        self.refresh_user_rewards(user_id);
        
        let user = self.users.get_mut(&user_id).unwrap();
        let reward = &mut self.rewards[reward_idx];
        
        let claimable = user.rewards_unclaimed[reward_idx].clone();
        
        // Apply treasury fee
        let treasury_fee = if reward.treasury_fee_bps > 0 {
            &claimable * BigRational::from_integer(BigInt::from(reward.treasury_fee_bps))
                / BigRational::from_integer(BigInt::from(10000))
        } else {
            BigRational::zero()
        };
        
        let user_amount = &claimable - &treasury_fee;
        
        user.rewards_unclaimed[reward_idx] = BigRational::zero();
        user.last_claim_ts[reward_idx] = self.current_time;
        reward.rewards_issued_unclaimed -= claimable;
        
        user_amount
    }
    
    pub fn refresh_global_rewards(&mut self) {
        for (i, reward) in self.rewards.iter_mut().enumerate() {
            if self.total_active_stake.is_zero() {
                reward.last_issuance_ts = self.current_time;
                continue;
            }
            
            let time_delta = self.current_time - reward.last_issuance_ts;
            if time_delta == 0 {
                continue;
            }
            
            // Get rate from schedule
            let rate = self.get_reward_rate(i, self.current_time);
            
            // Calculate rewards to issue
            let base_rewards = rate * BigRational::from_integer(BigInt::from(time_delta));
            
            let rewards_to_issue = match reward.reward_type {
                RewardType::Constant => base_rewards,
                RewardType::Proportional => base_rewards * &self.total_active_stake,
            };
            
            // Cap by available
            let issued = rewards_to_issue.min(reward.rewards_available.clone());
            
            if !issued.is_zero() {
                reward.rewards_available -= &issued;
                reward.rewards_issued_unclaimed += &issued;
                reward.rewards_issued_cumulative += &issued;
                
                // Update RPS (high precision, no scaling needed)
                reward.reward_per_share += &issued / &self.total_active_stake;
            }
            
            reward.last_issuance_ts = self.current_time;
        }
    }
    
    pub fn refresh_user_rewards(&mut self, user_id: usize) {
        let rewards_snapshot: Vec<_> = self.rewards.iter()
            .map(|r| r.reward_per_share.clone())
            .collect();
        
        let user = self.users.entry(user_id).or_insert_with(|| ReferenceUser {
            active_stake: BigRational::zero(),
            pending_deposit: BigRational::zero(),
            pending_withdrawal: BigRational::zero(),
            pending_withdrawal_ts: 0,
            reward_tallies: vec![BigRational::zero(); self.rewards.len()],
            rewards_unclaimed: vec![BigRational::zero(); self.rewards.len()],
            last_claim_ts: vec![0; self.rewards.len()],
        });
        
        // Process pending deposits
        if !user.pending_deposit.is_zero() && 
           self.current_time >= self.config.deposit_warmup_period {
            user.active_stake += &user.pending_deposit;
            self.total_active_stake += &user.pending_deposit;
            user.pending_deposit = BigRational::zero();
        }
        
        // Calculate new rewards
        for (i, rps) in rewards_snapshot.iter().enumerate() {
            let new_tally = rps * &user.active_stake;
            let old_tally = &user.reward_tallies[i];
            
            if new_tally > *old_tally {
                let reward = &new_tally - old_tally;
                user.rewards_unclaimed[i] += &reward;
                user.reward_tallies[i] = new_tally;
            }
        }
    }
    
    fn get_reward_rate(&self, reward_idx: usize, timestamp: u64) -> BigRational {
        let reward = &self.rewards[reward_idx];
        
        // Find applicable rate from schedule
        let mut rate = BigRational::zero();
        for &(ts, ref r) in reward.schedule.iter().rev() {
            if timestamp >= ts {
                rate = r.clone();
                break;
            }
        }
        
        // Apply decimals
        let divisor = BigRational::from_integer(
            BigInt::from(10).pow(reward.decimals)
        );
        rate / divisor
    }
    
    pub fn advance_time(&mut self, seconds: u64) {
        self.current_time += seconds;
    }
}

// Comparison utilities
pub fn compare_farms(
    reference: &ReferenceFarm,
    actual: &MockFarmState,
    tolerance: f64,
) -> Result<(), String> {
    // Compare total stakes
    let ref_stake = reference.total_active_stake.to_f64().unwrap_or(0.0);
    let act_stake = actual.total_active_stake.to_f64().unwrap_or(0.0);
    
    if (ref_stake - act_stake).abs() > tolerance {
        return Err(format!(
            "Total stake mismatch: reference={}, actual={}, diff={}",
            ref_stake, act_stake, (ref_stake - act_stake).abs()
        ));
    }
    
    // Compare rewards
    for (i, (ref_reward, act_reward)) in reference.rewards.iter()
        .zip(actual.rewards.iter())
        .enumerate() 
    {
        let ref_rps = ref_reward.reward_per_share.to_f64().unwrap_or(0.0);
        let act_rps = act_reward.reward_per_share_scaled.to_f64().unwrap_or(0.0);
        
        // Account for scaling difference (actual uses WAD scaling)
        let act_rps_normalized = act_rps / 1e18;
        
        if (ref_rps - act_rps_normalized).abs() > tolerance {
            return Err(format!(
                "RPS mismatch for reward {}: reference={}, actual={}, diff={}",
                i, ref_rps, act_rps_normalized, (ref_rps - act_rps_normalized).abs()
            ));
        }
        
        let ref_cumulative = ref_reward.rewards_issued_cumulative.to_f64().unwrap_or(0.0);
        let act_cumulative = act_reward.rewards_issued_cumulative as f64;
        
        if (ref_cumulative - act_cumulative).abs() > tolerance {
            return Err(format!(
                "Cumulative rewards mismatch for reward {}: reference={}, actual={}",
                i, ref_cumulative, act_cumulative
            ));
        }
    }
    
    Ok(())
}

// Test scenario generator
pub struct ScenarioGenerator {
    rng: StdRng,
}

impl ScenarioGenerator {
    pub fn new(seed: u64) -> Self {
        ScenarioGenerator {
            rng: StdRng::seed_from_u64(seed),
        }
    }
    
    pub fn generate_random_scenario(&mut self, num_actions: usize) -> Vec<TestAction> {
        let mut actions = Vec::new();
        let num_users = self.rng.gen_range(2..10);
        
        for _ in 0..num_actions {
            let action = match self.rng.gen_range(0..6) {
                0 => TestAction::Stake {
                    user_id: self.rng.gen_range(0..num_users),
                    amount: self.rng.gen_range(1000..10_000_000_000),
                },
                1 => TestAction::Unstake {
                    user_id: self.rng.gen_range(0..num_users),
                    amount: self.rng.gen_range(1000..1_000_000_000),
                },
                2 => TestAction::Harvest {
                    user_id: self.rng.gen_range(0..num_users),
                    reward_idx: 0,
                },
                3 => TestAction::AdvanceTime {
                    seconds: self.rng.gen_range(1..1000),
                },
                4 => TestAction::AddRewards {
                    reward_idx: 0,
                    amount: self.rng.gen_range(1_000_000..10_000_000_000),
                },
                _ => TestAction::RefreshFarm,
            };
            actions.push(action);
        }
        
        actions
    }
}

#[derive(Debug, Clone)]
pub enum TestAction {
    Stake { user_id: usize, amount: u64 },
    Unstake { user_id: usize, amount: u64 },
    Harvest { user_id: usize, reward_idx: usize },
    AdvanceTime { seconds: u64 },
    AddRewards { reward_idx: usize, amount: u64 },
    RefreshFarm,
}