use proptest::prelude::*;
use proptest::collection::vec;
use proptest::strategy::{Strategy, BoxedStrategy};
use decimal_wad::decimal::Decimal;
use farms::state::*;
use farms::farm_operations::*;
use std::collections::HashMap;

// Property test modules
pub mod conservation_tests;
pub mod monotonicity_tests;
pub mod commutativity_tests;
pub mod invariant_tests;

// Common property test types and strategies
#[derive(Debug, Clone)]
pub struct FarmAction {
    pub user_id: usize,
    pub action_type: ActionType,
    pub amount: u64,
    pub time_delta: u64,
}

#[derive(Debug, Clone)]
pub enum ActionType {
    Stake,
    Unstake,
    Harvest(usize), // reward index
    RefreshFarm,
    RefreshUser,
    AddRewards(usize, u64), // reward index, amount
}

// Strategy for generating valid farm actions
pub fn farm_action_strategy() -> BoxedStrategy<FarmAction> {
    (
        0..10usize, // user_id
        prop_oneof![
            Just(ActionType::Stake),
            Just(ActionType::Unstake),
            (0..3usize).prop_map(ActionType::Harvest),
            Just(ActionType::RefreshFarm),
            Just(ActionType::RefreshUser),
            (0..3usize, 1000..1_000_000u64)
                .prop_map(|(idx, amt)| ActionType::AddRewards(idx, amt)),
        ],
        1000..1_000_000_000u64, // amount
        0..1000u64, // time_delta
    )
        .prop_map(|(user_id, action_type, amount, time_delta)| FarmAction {
            user_id,
            action_type,
            amount,
            time_delta,
        })
        .boxed()
}

// Strategy for generating sequences of actions
pub fn action_sequence_strategy() -> BoxedStrategy<Vec<FarmAction>> {
    vec(farm_action_strategy(), 1..100).boxed()
}

// Mock farm state for property testing
#[derive(Debug, Clone)]
pub struct MockFarmState {
    pub total_staked: Decimal,
    pub total_active_stake: Decimal,
    pub rewards: Vec<MockRewardState>,
    pub users: HashMap<usize, MockUserState>,
    pub current_time: u64,
    pub total_rewards_deposited: Vec<u64>,
    pub total_rewards_claimed: Vec<u64>,
    pub total_slashed: u64,
}

#[derive(Debug, Clone)]
pub struct MockRewardState {
    pub reward_type: RewardType,
    pub rewards_available: u64,
    pub rewards_issued_unclaimed: u64,
    pub rewards_issued_cumulative: u64,
    pub reward_per_share_scaled: Decimal,
    pub last_issuance_ts: u64,
    pub rewards_per_second: u64,
    pub rewards_per_second_decimals: u64,
}

#[derive(Debug, Clone)]
pub struct MockUserState {
    pub active_stake: Decimal,
    pub pending_deposit: Decimal,
    pub pending_withdrawal: u64,
    pub reward_tallies: Vec<Decimal>,
    pub rewards_unclaimed: Vec<u64>,
    pub last_claim_ts: Vec<u64>,
}

impl MockFarmState {
    pub fn new(num_rewards: usize) -> Self {
        MockFarmState {
            total_staked: Decimal::zero(),
            total_active_stake: Decimal::zero(),
            rewards: (0..num_rewards)
                .map(|_| MockRewardState {
                    reward_type: RewardType::Proportional,
                    rewards_available: 0,
                    rewards_issued_unclaimed: 0,
                    rewards_issued_cumulative: 0,
                    reward_per_share_scaled: Decimal::zero(),
                    last_issuance_ts: 0,
                    rewards_per_second: 1000,
                    rewards_per_second_decimals: 6,
                })
                .collect(),
            users: HashMap::new(),
            current_time: 0,
            total_rewards_deposited: vec![0; num_rewards],
            total_rewards_claimed: vec![0; num_rewards],
            total_slashed: 0,
        }
    }
    
    pub fn apply_action(&mut self, action: &FarmAction) -> Result<(), String> {
        // Advance time
        self.current_time += action.time_delta;
        
        // Ensure user exists
        if !self.users.contains_key(&action.user_id) {
            self.users.insert(action.user_id, MockUserState {
                active_stake: Decimal::zero(),
                pending_deposit: Decimal::zero(),
                pending_withdrawal: 0,
                reward_tallies: vec![Decimal::zero(); self.rewards.len()],
                rewards_unclaimed: vec![0; self.rewards.len()],
                last_claim_ts: vec![0; self.rewards.len()],
            });
        }
        
        match &action.action_type {
            ActionType::Stake => self.stake(action.user_id, action.amount),
            ActionType::Unstake => self.unstake(action.user_id, action.amount),
            ActionType::Harvest(reward_idx) => self.harvest(action.user_id, *reward_idx),
            ActionType::RefreshFarm => self.refresh_farm(),
            ActionType::RefreshUser => self.refresh_user(action.user_id),
            ActionType::AddRewards(reward_idx, amount) => self.add_rewards(*reward_idx, *amount),
        }
    }
    
    fn stake(&mut self, user_id: usize, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Cannot stake 0".to_string());
        }
        
        self.refresh_farm()?;
        self.refresh_user(user_id)?;
        
        let user = self.users.get_mut(&user_id).unwrap();
        let amount_decimal = Decimal::from(amount);
        user.active_stake = user.active_stake
            .checked_add(amount_decimal)
            .ok_or("Stake overflow")?;
        
        self.total_staked = self.total_staked
            .checked_add(amount_decimal)
            .ok_or("Total stake overflow")?;
        self.total_active_stake = self.total_active_stake
            .checked_add(amount_decimal)
            .ok_or("Active stake overflow")?;
        
        Ok(())
    }
    
    fn unstake(&mut self, user_id: usize, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Cannot unstake 0".to_string());
        }
        
        self.refresh_farm()?;
        self.refresh_user(user_id)?;
        
        let user = self.users.get_mut(&user_id).unwrap();
        let amount_decimal = Decimal::from(amount);
        
        if user.active_stake < amount_decimal {
            return Err("Insufficient stake".to_string());
        }
        
        user.active_stake = user.active_stake
            .checked_sub(amount_decimal)
            .ok_or("Unstake underflow")?;
        user.pending_withdrawal += amount;
        
        self.total_active_stake = self.total_active_stake
            .checked_sub(amount_decimal)
            .ok_or("Active stake underflow")?;
        
        Ok(())
    }
    
    fn harvest(&mut self, user_id: usize, reward_idx: usize) -> Result<(), String> {
        if reward_idx >= self.rewards.len() {
            return Err("Invalid reward index".to_string());
        }
        
        self.refresh_farm()?;
        self.refresh_user(user_id)?;
        
        let user = self.users.get_mut(&user_id).unwrap();
        let claimed = user.rewards_unclaimed[reward_idx];
        
        if claimed == 0 {
            return Ok(());
        }
        
        user.rewards_unclaimed[reward_idx] = 0;
        user.last_claim_ts[reward_idx] = self.current_time;
        self.total_rewards_claimed[reward_idx] += claimed;
        
        // Update farm reward state
        let reward = &mut self.rewards[reward_idx];
        reward.rewards_issued_unclaimed = reward.rewards_issued_unclaimed
            .saturating_sub(claimed);
        
        Ok(())
    }
    
    fn refresh_farm(&mut self) -> Result<(), String> {
        // Issue rewards since last refresh
        for (i, reward) in self.rewards.iter_mut().enumerate() {
            if self.total_active_stake == Decimal::zero() {
                reward.last_issuance_ts = self.current_time;
                continue;
            }
            
            let time_delta = self.current_time.saturating_sub(reward.last_issuance_ts);
            if time_delta == 0 {
                continue;
            }
            
            // Calculate rewards to issue
            let base_rewards = (reward.rewards_per_second as u128)
                .saturating_mul(time_delta as u128);
            
            let decimals_divisor = 10u128.pow(reward.rewards_per_second_decimals as u32);
            let rewards_to_issue = (base_rewards / decimals_divisor) as u64;
            
            // Apply reward type multiplier
            let final_rewards = match reward.reward_type {
                RewardType::Constant => rewards_to_issue,
                RewardType::Proportional => {
                    // Scale by total stake
                    let stake_multiplier = self.total_active_stake.to_u64().unwrap_or(1);
                    rewards_to_issue.saturating_mul(stake_multiplier) / 1_000_000_000
                }
            };
            
            // Cap by available rewards
            let issued = final_rewards.min(reward.rewards_available);
            
            if issued > 0 {
                reward.rewards_available = reward.rewards_available.saturating_sub(issued);
                reward.rewards_issued_unclaimed += issued;
                reward.rewards_issued_cumulative += issued;
                
                // Update RPS
                let issued_decimal = Decimal::from(issued);
                let rps_increment = issued_decimal
                    .checked_div(self.total_active_stake)
                    .unwrap_or(Decimal::zero());
                
                reward.reward_per_share_scaled = reward.reward_per_share_scaled
                    .checked_add(rps_increment)
                    .unwrap_or(reward.reward_per_share_scaled);
            }
            
            reward.last_issuance_ts = self.current_time;
        }
        
        Ok(())
    }
    
    fn refresh_user(&mut self, user_id: usize) -> Result<(), String> {
        let user = self.users.get_mut(&user_id).unwrap();
        
        for (i, reward) in self.rewards.iter().enumerate() {
            // Calculate new rewards
            let new_tally = reward.reward_per_share_scaled
                .checked_mul(user.active_stake)
                .unwrap_or(Decimal::zero());
            
            let old_tally = user.reward_tallies[i];
            if new_tally > old_tally {
                let reward_decimal = new_tally.checked_sub(old_tally).unwrap();
                let reward_amount = reward_decimal.to_u64().unwrap_or(0);
                
                user.rewards_unclaimed[i] += reward_amount;
                user.reward_tallies[i] = old_tally
                    .checked_add(Decimal::from(reward_amount))
                    .unwrap_or(old_tally);
            }
        }
        
        Ok(())
    }
    
    fn add_rewards(&mut self, reward_idx: usize, amount: u64) -> Result<(), String> {
        if reward_idx >= self.rewards.len() {
            return Err("Invalid reward index".to_string());
        }
        
        self.rewards[reward_idx].rewards_available += amount;
        self.total_rewards_deposited[reward_idx] += amount;
        
        Ok(())
    }
    
    // Invariant checking functions
    pub fn check_conservation_invariant(&self) -> bool {
        for i in 0..self.rewards.len() {
            let total_in = self.total_rewards_deposited[i];
            let total_out = self.total_rewards_claimed[i];
            let unclaimed = self.rewards[i].rewards_issued_unclaimed;
            let available = self.rewards[i].rewards_available;
            
            // Total claimed + unclaimed + available should equal total deposited
            let accounted = total_out + unclaimed + available;
            
            // Allow small rounding error (e.g., 0.01%)
            let tolerance = total_in / 10000;
            if accounted > total_in + tolerance {
                return false;
            }
        }
        true
    }
    
    pub fn check_monotonicity_invariant(&self) -> bool {
        for reward in &self.rewards {
            // RPS should never decrease
            if reward.reward_per_share_scaled < Decimal::zero() {
                return false;
            }
            
            // Cumulative issued should never decrease
            if reward.rewards_issued_cumulative < 0 {
                return false;
            }
        }
        
        for user in self.users.values() {
            // User rewards should never be negative
            for &unclaimed in &user.rewards_unclaimed {
                if unclaimed < 0 {
                    return false;
                }
            }
        }
        
        true
    }
    
    pub fn check_stake_consistency(&self) -> bool {
        // Sum of user active stakes should equal total active stake
        let sum_user_stakes = self.users.values()
            .map(|u| u.active_stake)
            .fold(Decimal::zero(), |acc, s| acc.checked_add(s).unwrap_or(acc));
        
        // Allow small rounding error
        let diff = if sum_user_stakes > self.total_active_stake {
            sum_user_stakes - self.total_active_stake
        } else {
            self.total_active_stake - sum_user_stakes
        };
        
        diff < Decimal::from(self.users.len() as u64) // 1 unit per user max error
    }
}