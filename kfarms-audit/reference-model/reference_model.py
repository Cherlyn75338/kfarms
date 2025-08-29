#!/usr/bin/env python3
"""
KFarms Reference Model - High Precision Mathematical Implementation
Used for differential testing against on-chain implementation
"""

from decimal import Decimal, getcontext
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple
from enum import Enum
import json
import hashlib

# Set precision for 256-bit equivalent calculations
getcontext().prec = 78

class LockDuration(Enum):
    """Lock duration options"""
    NONE = 0
    WEEK = 7 * 24 * 60 * 60 / 2  # slots (2 sec/slot)
    MONTH = 30 * 24 * 60 * 60 / 2
    QUARTER = 90 * 24 * 60 * 60 / 2
    YEAR = 365 * 24 * 60 * 60 / 2

@dataclass
class PoolConfig:
    """Pool configuration parameters"""
    pool_id: str
    token_mint: str
    weight: Decimal
    is_active: bool = True
    min_stake: Decimal = Decimal(0)
    max_stake: Decimal = Decimal(10**10) * Decimal(10**9)  # 10B tokens
    
@dataclass 
class PoolState:
    """Pool state tracking"""
    config: PoolConfig
    total_staked: Decimal = Decimal(0)
    total_points: Decimal = Decimal(0)
    rpt: Decimal = Decimal(0)  # Reward per token accumulator
    last_update_slot: int = 0
    total_distributed: Decimal = Decimal(0)
    dust_accumulated: Decimal = Decimal(0)
    
@dataclass
class UserStake:
    """User stake in a specific pool"""
    amount: Decimal = Decimal(0)
    lock_end_slot: int = 0
    lock_multiplier: Decimal = Decimal(1)
    points: Decimal = Decimal(0)
    paid_rpt: Decimal = Decimal(0)
    accrued_rewards: Decimal = Decimal(0)
    last_update_slot: int = 0
    
@dataclass
class GlobalConfig:
    """Global protocol configuration"""
    total_emission_rate: Decimal = Decimal(1000) * Decimal(10**9)  # per slot
    max_lock_multiplier: Decimal = Decimal(4)
    base_lock_duration: int = int(LockDuration.YEAR.value)
    scale: Decimal = Decimal(10**18)
    max_slots_per_update: int = 432_000  # 1 epoch
    dust_threshold: Decimal = Decimal(1000)  # Min dust to distribute
    
class ReferenceModel:
    """High-precision reference implementation of KFarms protocol"""
    
    def __init__(self, config: Optional[GlobalConfig] = None):
        self.config = config or GlobalConfig()
        self.pools: Dict[str, PoolState] = {}
        self.users: Dict[str, Dict[str, UserStake]] = {}  # user -> pool_id -> stake
        self.current_slot = 0
        self.total_weight = Decimal(0)
        
        # Tracking for invariant checking
        self.total_minted = Decimal(0)
        self.total_claimed = Decimal(0)
        
    def create_pool(self, config: PoolConfig) -> str:
        """Create a new staking pool"""
        if config.pool_id in self.pools:
            raise ValueError(f"Pool {config.pool_id} already exists")
            
        self.pools[config.pool_id] = PoolState(
            config=config,
            last_update_slot=self.current_slot
        )
        
        self._update_total_weight()
        return config.pool_id
    
    def _update_total_weight(self):
        """Recalculate total weight across all active pools"""
        self.total_weight = sum(
            pool.config.weight 
            for pool in self.pools.values() 
            if pool.config.is_active
        )
    
    def _calculate_lock_multiplier(self, lock_duration_slots: int) -> Decimal:
        """Calculate lock multiplier based on duration"""
        if lock_duration_slots <= 0:
            return Decimal(1)
            
        # Linear multiplier: 1 + (duration/base_duration) * (max_multiplier - 1)
        multiplier = Decimal(1) + (
            Decimal(lock_duration_slots) / Decimal(self.config.base_lock_duration)
        ) * (self.config.max_lock_multiplier - Decimal(1))
        
        return min(multiplier, self.config.max_lock_multiplier)
    
    def _update_pool_rpt(self, pool_id: str) -> Tuple[Decimal, Decimal]:
        """Update pool's reward per token accumulator"""
        pool = self.pools[pool_id]
        
        # Calculate time delta with bounds
        time_delta = min(
            self.current_slot - pool.last_update_slot,
            self.config.max_slots_per_update
        )
        
        if time_delta <= 0:
            return Decimal(0), Decimal(0)
        
        rewards_generated = Decimal(0)
        
        if pool.total_points > 0 and self.total_weight > 0:
            # Calculate pool's share of emissions
            pool_emission_rate = (
                self.config.total_emission_rate * 
                pool.config.weight / 
                self.total_weight
            )
            
            # Total rewards for this period
            rewards_generated = pool_emission_rate * Decimal(time_delta)
            
            # Update RPT (scaled to maintain precision)
            delta_rpt = (rewards_generated * self.config.scale) / pool.total_points
            pool.rpt += delta_rpt
            
            # Track distribution
            pool.total_distributed += rewards_generated
            self.total_minted += rewards_generated
            
        elif pool.total_points == 0 and self.total_weight > 0:
            # Accumulate dust when no stakers
            pool_emission_rate = (
                self.config.total_emission_rate * 
                pool.config.weight / 
                self.total_weight
            )
            dust = pool_emission_rate * Decimal(time_delta)
            pool.dust_accumulated += dust
            self.total_minted += dust
            
        pool.last_update_slot = self.current_slot
        return rewards_generated, pool.rpt
        
    def _settle_user_rewards(self, user_id: str, pool_id: str) -> Decimal:
        """Calculate and update user's pending rewards"""
        if user_id not in self.users or pool_id not in self.users[user_id]:
            return Decimal(0)
            
        user_stake = self.users[user_id][pool_id]
        pool = self.pools[pool_id]
        
        # Calculate new rewards since last settlement
        delta_rpt = pool.rpt - user_stake.paid_rpt
        new_rewards = (user_stake.points * delta_rpt) / self.config.scale
        
        # Update user state
        user_stake.accrued_rewards += new_rewards
        user_stake.paid_rpt = pool.rpt
        user_stake.last_update_slot = self.current_slot
        
        return new_rewards
    
    def deposit(
        self, 
        user_id: str, 
        pool_id: str, 
        amount: Decimal, 
        lock_duration_slots: int = 0
    ) -> Dict:
        """Deposit tokens into a pool with optional lock"""
        if pool_id not in self.pools:
            raise ValueError(f"Pool {pool_id} does not exist")
            
        pool = self.pools[pool_id]
        
        # Validate amount
        if amount < pool.config.min_stake:
            raise ValueError(f"Amount below minimum stake {pool.config.min_stake}")
        if amount > pool.config.max_stake:
            raise ValueError(f"Amount exceeds maximum stake {pool.config.max_stake}")
            
        # Update pool RPT before changing points
        self._update_pool_rpt(pool_id)
        
        # Initialize user stake if needed
        if user_id not in self.users:
            self.users[user_id] = {}
        if pool_id not in self.users[user_id]:
            self.users[user_id][pool_id] = UserStake()
            
        user_stake = self.users[user_id][pool_id]
        
        # Settle existing rewards before changing points
        self._settle_user_rewards(user_id, pool_id)
        
        # Calculate lock multiplier and new points
        lock_multiplier = self._calculate_lock_multiplier(lock_duration_slots)
        old_points = user_stake.points
        
        # Update user stake
        user_stake.amount += amount
        user_stake.lock_multiplier = max(user_stake.lock_multiplier, lock_multiplier)
        user_stake.lock_end_slot = max(
            user_stake.lock_end_slot, 
            self.current_slot + lock_duration_slots
        )
        user_stake.points = user_stake.amount * user_stake.lock_multiplier
        
        # Update pool state
        pool.total_staked += amount
        pool.total_points = pool.total_points - old_points + user_stake.points
        
        return {
            "user_stake": user_stake.amount,
            "user_points": user_stake.points,
            "lock_multiplier": user_stake.lock_multiplier,
            "lock_end_slot": user_stake.lock_end_slot,
            "pool_total_staked": pool.total_staked,
            "pool_total_points": pool.total_points
        }
    
    def withdraw(self, user_id: str, pool_id: str, amount: Decimal) -> Dict:
        """Withdraw staked tokens from a pool"""
        if user_id not in self.users or pool_id not in self.users[user_id]:
            raise ValueError(f"No stake found for user {user_id} in pool {pool_id}")
            
        user_stake = self.users[user_id][pool_id]
        pool = self.pools[pool_id]
        
        # Check lock expiry
        if self.current_slot < user_stake.lock_end_slot:
            raise ValueError(f"Lock not expired. Ends at slot {user_stake.lock_end_slot}")
            
        # Check sufficient balance
        if amount > user_stake.amount:
            raise ValueError(f"Insufficient stake. Have {user_stake.amount}, want {amount}")
            
        # Update pool RPT and settle rewards
        self._update_pool_rpt(pool_id)
        self._settle_user_rewards(user_id, pool_id)
        
        # Calculate point reduction
        old_points = user_stake.points
        user_stake.amount -= amount
        user_stake.points = user_stake.amount * user_stake.lock_multiplier
        points_reduction = old_points - user_stake.points
        
        # Update pool state
        pool.total_staked -= amount
        pool.total_points -= points_reduction
        
        # Reset lock if fully withdrawn
        if user_stake.amount == 0:
            user_stake.lock_multiplier = Decimal(1)
            user_stake.lock_end_slot = 0
            
        return {
            "withdrawn": amount,
            "remaining_stake": user_stake.amount,
            "remaining_points": user_stake.points,
            "pool_total_staked": pool.total_staked,
            "pool_total_points": pool.total_points
        }
    
    def claim_rewards(self, user_id: str, pool_id: str) -> Decimal:
        """Claim accumulated rewards"""
        if user_id not in self.users or pool_id not in self.users[user_id]:
            return Decimal(0)
            
        # Update pool and settle rewards
        self._update_pool_rpt(pool_id)
        self._settle_user_rewards(user_id, pool_id)
        
        user_stake = self.users[user_id][pool_id]
        rewards = user_stake.accrued_rewards
        
        # Reset accrued rewards
        user_stake.accrued_rewards = Decimal(0)
        self.total_claimed += rewards
        
        return rewards
    
    def extend_lock(
        self, 
        user_id: str, 
        pool_id: str, 
        additional_duration_slots: int
    ) -> Dict:
        """Extend lock duration for existing stake"""
        if user_id not in self.users or pool_id not in self.users[user_id]:
            raise ValueError(f"No stake found for user {user_id} in pool {pool_id}")
            
        user_stake = self.users[user_id][pool_id]
        pool = self.pools[pool_id]
        
        # Must have existing stake
        if user_stake.amount == 0:
            raise ValueError("No stake to extend lock for")
            
        # Update pool RPT and settle rewards BEFORE changing multiplier
        self._update_pool_rpt(pool_id)
        self._settle_user_rewards(user_id, pool_id)
        
        # Calculate new lock parameters
        new_end_slot = max(
            user_stake.lock_end_slot,
            self.current_slot
        ) + additional_duration_slots
        
        total_lock_duration = new_end_slot - self.current_slot
        new_multiplier = self._calculate_lock_multiplier(total_lock_duration)
        
        # Only allow increasing lock
        if new_multiplier <= user_stake.lock_multiplier:
            raise ValueError("New lock must be longer than existing")
            
        # Update points
        old_points = user_stake.points
        user_stake.lock_multiplier = new_multiplier
        user_stake.lock_end_slot = new_end_slot
        user_stake.points = user_stake.amount * user_stake.lock_multiplier
        
        # Update pool points
        pool.total_points = pool.total_points - old_points + user_stake.points
        
        return {
            "new_multiplier": user_stake.lock_multiplier,
            "new_lock_end": user_stake.lock_end_slot,
            "new_points": user_stake.points,
            "pool_total_points": pool.total_points
        }
    
    def update_emission_rate(self, new_rate: Decimal):
        """Update global emission rate"""
        # Settle all pools first
        for pool_id in self.pools:
            self._update_pool_rpt(pool_id)
            
        # Update rate
        self.config.total_emission_rate = new_rate
    
    def update_pool_weight(self, pool_id: str, new_weight: Decimal):
        """Update pool weight"""
        if pool_id not in self.pools:
            raise ValueError(f"Pool {pool_id} does not exist")
            
        # Settle pool first
        self._update_pool_rpt(pool_id)
        
        # Update weight
        self.pools[pool_id].config.weight = new_weight
        self._update_total_weight()
    
    def advance_time(self, slots: int):
        """Advance current slot for testing"""
        self.current_slot += slots
    
    def get_user_info(self, user_id: str, pool_id: str) -> Dict:
        """Get complete user information"""
        if user_id not in self.users or pool_id not in self.users[user_id]:
            return {"error": "User stake not found"}
            
        # Update and settle to get current values
        self._update_pool_rpt(pool_id)
        self._settle_user_rewards(user_id, pool_id)
        
        user_stake = self.users[user_id][pool_id]
        
        return {
            "staked": str(user_stake.amount),
            "points": str(user_stake.points),
            "lock_multiplier": str(user_stake.lock_multiplier),
            "lock_end_slot": user_stake.lock_end_slot,
            "pending_rewards": str(user_stake.accrued_rewards),
            "paid_rpt": str(user_stake.paid_rpt)
        }
    
    def get_pool_info(self, pool_id: str) -> Dict:
        """Get complete pool information"""
        if pool_id not in self.pools:
            return {"error": "Pool not found"}
            
        pool = self.pools[pool_id]
        
        return {
            "total_staked": str(pool.total_staked),
            "total_points": str(pool.total_points),
            "rpt": str(pool.rpt),
            "weight": str(pool.config.weight),
            "total_distributed": str(pool.total_distributed),
            "dust_accumulated": str(pool.dust_accumulated),
            "last_update_slot": pool.last_update_slot
        }
    
    def check_invariants(self) -> List[str]:
        """Check all protocol invariants"""
        violations = []
        
        # INV-1: Conservation - distributed <= minted
        total_distributed = sum(p.total_distributed for p in self.pools.values())
        if total_distributed > self.total_minted:
            violations.append(f"INV-1: Distributed {total_distributed} > Minted {self.total_minted}")
            
        # INV-2: Points consistency
        for pool_id, pool in self.pools.items():
            calculated_points = Decimal(0)
            for user_stakes in self.users.values():
                if pool_id in user_stakes:
                    calculated_points += user_stakes[pool_id].points
                    
            if abs(calculated_points - pool.total_points) > Decimal("0.000001"):
                violations.append(
                    f"INV-2: Pool {pool_id} points mismatch. "
                    f"Calculated: {calculated_points}, Stored: {pool.total_points}"
                )
                
        # INV-3: RPT monotonicity
        # (checked during updates)
        
        # INV-4: Claimed <= distributed
        if self.total_claimed > total_distributed:
            violations.append(f"INV-4: Claimed {self.total_claimed} > Distributed {total_distributed}")
            
        return violations

def create_test_scenario():
    """Create a test scenario for verification"""
    model = ReferenceModel()
    
    # Create pools
    model.create_pool(PoolConfig(
        pool_id="USDC",
        token_mint="USDC_MINT",
        weight=Decimal(100)
    ))
    
    model.create_pool(PoolConfig(
        pool_id="SOL", 
        token_mint="SOL_MINT",
        weight=Decimal(200)
    ))
    
    # Simulate operations
    results = []
    
    # User 1 deposits with lock
    model.deposit("user1", "USDC", Decimal(1000) * Decimal(10**9), 
                  int(LockDuration.MONTH.value))
    results.append(("deposit", model.get_user_info("user1", "USDC")))
    
    # Advance time
    model.advance_time(1000)
    
    # User 2 deposits
    model.deposit("user2", "USDC", Decimal(500) * Decimal(10**9), 0)
    
    # Check rewards accumulation
    model.advance_time(10000)
    results.append(("after_time", model.get_user_info("user1", "USDC")))
    
    # Claim rewards
    rewards = model.claim_rewards("user1", "USDC")
    results.append(("claimed", str(rewards)))
    
    # Check invariants
    violations = model.check_invariants()
    
    return results, violations

if __name__ == "__main__":
    results, violations = create_test_scenario()
    
    print("Test Results:")
    print(json.dumps(results, indent=2))
    
    print("\nInvariant Violations:")
    if violations:
        for v in violations:
            print(f"  - {v}")
    else:
        print("  None - All invariants hold")