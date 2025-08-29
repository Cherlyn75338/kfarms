#!/usr/bin/env python3
"""
KFarms Protocol Fuzzer - Property-based and sequence testing
"""

import random
import sys
import os
sys.path.append(os.path.join(os.path.dirname(__file__), '..', 'reference-model'))

from reference_model import ReferenceModel, PoolConfig, GlobalConfig, LockDuration
from decimal import Decimal
from typing import List, Tuple, Dict, Any
from dataclasses import dataclass
from enum import Enum
import json

class Operation(Enum):
    """Protocol operations to fuzz"""
    DEPOSIT = "deposit"
    WITHDRAW = "withdraw" 
    CLAIM = "claim"
    EXTEND_LOCK = "extend_lock"
    UPDATE_EMISSION = "update_emission"
    UPDATE_WEIGHT = "update_weight"
    ADVANCE_TIME = "advance_time"
    CREATE_POOL = "create_pool"

@dataclass
class FuzzConfig:
    """Fuzzing configuration"""
    num_users: int = 10
    num_pools: int = 5
    num_operations: int = 1000
    max_amount: Decimal = Decimal(10**6) * Decimal(10**9)  # 1M tokens
    max_time_jump: int = 10000  # slots
    seed: int = None
    
    # Attack patterns to test
    test_sandwich: bool = True
    test_dust_siphon: bool = True
    test_overflow: bool = True
    test_zero_staker: bool = True
    test_time_warp: bool = True

class ProtocolFuzzer:
    """Fuzzer for finding protocol vulnerabilities"""
    
    def __init__(self, config: FuzzConfig = None):
        self.config = config or FuzzConfig()
        if self.config.seed:
            random.seed(self.config.seed)
            
        self.model = ReferenceModel()
        self.users = [f"user_{i}" for i in range(self.config.num_users)]
        self.pools = []
        self.operation_log = []
        self.violations_found = []
        
    def setup(self):
        """Initialize pools and initial state"""
        # Create pools with varying weights
        for i in range(self.config.num_pools):
            pool_id = f"pool_{i}"
            self.pools.append(pool_id)
            self.model.create_pool(PoolConfig(
                pool_id=pool_id,
                token_mint=f"token_{i}",
                weight=Decimal(random.randint(10, 1000))
            ))
            
    def generate_random_operation(self) -> Tuple[Operation, Dict[str, Any]]:
        """Generate a random valid operation"""
        op = random.choice(list(Operation))
        params = {}
        
        if op == Operation.DEPOSIT:
            params = {
                "user_id": random.choice(self.users),
                "pool_id": random.choice(self.pools),
                "amount": Decimal(random.randint(1, int(self.config.max_amount))),
                "lock_duration_slots": random.choice([
                    0,
                    int(LockDuration.WEEK.value),
                    int(LockDuration.MONTH.value),
                    int(LockDuration.QUARTER.value),
                    int(LockDuration.YEAR.value)
                ])
            }
            
        elif op == Operation.WITHDRAW:
            # Pick a user with stake
            user_id = random.choice(self.users)
            pool_id = random.choice(self.pools)
            
            # Try to get current stake
            try:
                info = self.model.get_user_info(user_id, pool_id)
                if "staked" in info and Decimal(info["staked"]) > 0:
                    max_withdraw = Decimal(info["staked"])
                    params = {
                        "user_id": user_id,
                        "pool_id": pool_id,
                        "amount": Decimal(random.randint(1, int(max_withdraw)))
                    }
                else:
                    # No stake, generate deposit instead
                    return self.generate_random_operation()
            except:
                return self.generate_random_operation()
                
        elif op == Operation.CLAIM:
            params = {
                "user_id": random.choice(self.users),
                "pool_id": random.choice(self.pools)
            }
            
        elif op == Operation.EXTEND_LOCK:
            params = {
                "user_id": random.choice(self.users),
                "pool_id": random.choice(self.pools),
                "additional_duration_slots": random.randint(1000, 1000000)
            }
            
        elif op == Operation.UPDATE_EMISSION:
            params = {
                "new_rate": Decimal(random.randint(100, 10000)) * Decimal(10**9)
            }
            
        elif op == Operation.UPDATE_WEIGHT:
            params = {
                "pool_id": random.choice(self.pools),
                "new_weight": Decimal(random.randint(1, 1000))
            }
            
        elif op == Operation.ADVANCE_TIME:
            params = {
                "slots": random.randint(1, self.config.max_time_jump)
            }
            
        elif op == Operation.CREATE_POOL:
            pool_id = f"pool_new_{len(self.pools)}"
            self.pools.append(pool_id)
            params = {
                "config": PoolConfig(
                    pool_id=pool_id,
                    token_mint=f"token_new_{len(self.pools)}",
                    weight=Decimal(random.randint(1, 1000))
                )
            }
            
        return op, params
    
    def execute_operation(self, op: Operation, params: Dict[str, Any]) -> Tuple[bool, Any]:
        """Execute an operation and return success status"""
        try:
            result = None
            
            if op == Operation.DEPOSIT:
                result = self.model.deposit(**params)
            elif op == Operation.WITHDRAW:
                result = self.model.withdraw(**params)
            elif op == Operation.CLAIM:
                result = self.model.claim_rewards(**params)
            elif op == Operation.EXTEND_LOCK:
                result = self.model.extend_lock(**params)
            elif op == Operation.UPDATE_EMISSION:
                self.model.update_emission_rate(**params)
            elif op == Operation.UPDATE_WEIGHT:
                self.model.update_pool_weight(**params)
            elif op == Operation.ADVANCE_TIME:
                self.model.advance_time(**params)
            elif op == Operation.CREATE_POOL:
                result = self.model.create_pool(**params)
                
            return True, result
        except Exception as e:
            return False, str(e)
    
    def test_sandwich_attack(self):
        """Test sandwich attack around pool updates"""
        print("\n[*] Testing sandwich attack pattern...")
        
        # Setup: Create pool and victim deposit
        victim = "victim"
        attacker = "attacker"
        pool = self.pools[0]
        
        # Victim deposits
        self.model.deposit(victim, pool, Decimal(1000) * Decimal(10**9), 0)
        self.model.advance_time(1000)
        
        # Attacker front-runs emission update
        self.model.deposit(attacker, pool, Decimal(10000) * Decimal(10**9), 
                          int(LockDuration.YEAR.value))
        
        # Emission rate increases
        old_rate = self.model.config.total_emission_rate
        self.model.update_emission_rate(old_rate * 10)
        
        # Let some rewards accumulate
        self.model.advance_time(100)
        
        # Attacker claims
        attacker_rewards = self.model.claim_rewards(attacker, pool)
        victim_rewards = self.model.claim_rewards(victim, pool)
        
        # Check if attacker got disproportionate rewards
        attacker_info = self.model.get_user_info(attacker, pool)
        victim_info = self.model.get_user_info(victim, pool)
        
        if attacker_rewards > victim_rewards * 20:  # More than 20x despite similar timing
            self.violations_found.append({
                "type": "sandwich_attack",
                "severity": "HIGH",
                "details": f"Attacker gained {attacker_rewards} vs victim {victim_rewards}"
            })
    
    def test_dust_siphon(self):
        """Test dust siphoning through repeated operations"""
        print("\n[*] Testing dust siphon pattern...")
        
        attacker = "dust_attacker"
        pool = self.pools[0]
        initial_dust = Decimal(0)
        
        # Perform many small deposits and withdrawals
        for _ in range(100):
            # Deposit 1 wei
            self.model.deposit(attacker, pool, Decimal(1), 0)
            self.model.advance_time(1)
            
            # Try to claim
            rewards = self.model.claim_rewards(attacker, pool)
            initial_dust += rewards
            
            # Withdraw
            try:
                self.model.withdraw(attacker, pool, Decimal(1))
            except:
                pass  # May fail due to lock
                
        if initial_dust > Decimal(100):  # Accumulated significant dust
            self.violations_found.append({
                "type": "dust_siphon",
                "severity": "MEDIUM",
                "details": f"Accumulated {initial_dust} dust through micro-operations"
            })
    
    def test_overflow_scenarios(self):
        """Test overflow conditions with maximum values"""
        print("\n[*] Testing overflow scenarios...")
        
        # Test maximum stake with maximum multiplier
        user = "overflow_tester"
        pool = self.pools[0]
        
        # Try maximum values
        max_stake = Decimal(10**10) * Decimal(10**9)  # 10B tokens
        
        try:
            # Deposit max stake with max lock
            self.model.deposit(user, pool, max_stake, int(LockDuration.YEAR.value) * 4)
            
            # Advance time significantly
            self.model.advance_time(1000000)
            
            # Try to claim
            rewards = self.model.claim_rewards(user, pool)
            
            # Check for unrealistic rewards
            if rewards > max_stake * 100:  # More than 100x stake
                self.violations_found.append({
                    "type": "overflow",
                    "severity": "CRITICAL",
                    "details": f"Possible overflow: rewards {rewards} from stake {max_stake}"
                })
                
        except Exception as e:
            # Overflow might cause exception
            if "overflow" in str(e).lower():
                self.violations_found.append({
                    "type": "overflow",
                    "severity": "CRITICAL", 
                    "details": f"Overflow exception: {e}"
                })
    
    def test_zero_staker_attack(self):
        """Test zero-staker pool exploitation"""
        print("\n[*] Testing zero-staker attack...")
        
        # Create new pool
        pool_id = "zero_pool"
        self.model.create_pool(PoolConfig(
            pool_id=pool_id,
            token_mint="ZERO",
            weight=Decimal(100)
        ))
        
        # Let rewards accumulate with no stakers
        self.model.advance_time(100000)
        
        # Check accumulated dust
        pool_info = self.model.get_pool_info(pool_id)
        dust = Decimal(pool_info["dust_accumulated"])
        
        if dust > 0:
            # Now deposit 1 wei
            attacker = "zero_attacker"
            self.model.deposit(attacker, pool_id, Decimal(1), 0)
            self.model.advance_time(1)
            
            # Try to capture dust
            rewards = self.model.claim_rewards(attacker, pool_id)
            
            if rewards > dust * Decimal("0.1"):  # Got significant portion of dust
                self.violations_found.append({
                    "type": "zero_staker",
                    "severity": "HIGH",
                    "details": f"Captured {rewards} from {dust} accumulated dust"
                })
    
    def test_time_warp(self):
        """Test time manipulation vulnerabilities"""
        print("\n[*] Testing time warp scenarios...")
        
        user = "time_warper"
        pool = self.pools[0]
        
        # Normal deposit
        self.model.deposit(user, pool, Decimal(1000) * Decimal(10**9), 0)
        
        # Massive time jump
        self.model.advance_time(10000000)  # Way beyond max_slots_per_update
        
        # Check rewards
        rewards = self.model.claim_rewards(user, pool)
        
        # Calculate expected max rewards
        max_expected = (
            self.model.config.total_emission_rate * 
            self.model.config.max_slots_per_update * 
            10  # Some reasonable multiplier
        )
        
        if rewards > max_expected:
            self.violations_found.append({
                "type": "time_warp",
                "severity": "CRITICAL",
                "details": f"Time warp allowed excessive rewards: {rewards}"
            })
    
    def run_random_sequence(self):
        """Run random operation sequence"""
        print(f"\n[*] Running {self.config.num_operations} random operations...")
        
        for i in range(self.config.num_operations):
            op, params = self.generate_random_operation()
            success, result = self.execute_operation(op, params)
            
            self.operation_log.append({
                "index": i,
                "operation": op.value,
                "params": str(params),
                "success": success,
                "result": str(result) if success else result
            })
            
            # Check invariants periodically
            if i % 100 == 0:
                violations = self.model.check_invariants()
                if violations:
                    self.violations_found.append({
                        "type": "invariant_violation",
                        "severity": "CRITICAL",
                        "at_operation": i,
                        "violations": violations
                    })
            
            # Progress indicator
            if i % 100 == 0:
                print(f"  Completed {i}/{self.config.num_operations} operations")
    
    def run_all_tests(self):
        """Run all test patterns"""
        print("=" * 60)
        print("KFarms Protocol Fuzzer")
        print("=" * 60)
        
        self.setup()
        
        # Run specific attack patterns
        if self.config.test_sandwich:
            self.test_sandwich_attack()
        if self.config.test_dust_siphon:
            self.test_dust_siphon()
        if self.config.test_overflow:
            self.test_overflow_scenarios()
        if self.config.test_zero_staker:
            self.test_zero_staker_attack()
        if self.config.test_time_warp:
            self.test_time_warp()
            
        # Run random fuzzing
        self.run_random_sequence()
        
        # Final invariant check
        print("\n[*] Final invariant check...")
        final_violations = self.model.check_invariants()
        if final_violations:
            self.violations_found.append({
                "type": "final_invariant_violation",
                "severity": "CRITICAL",
                "violations": final_violations
            })
        
        # Report results
        self.report_results()
    
    def report_results(self):
        """Generate fuzzing report"""
        print("\n" + "=" * 60)
        print("FUZZING RESULTS")
        print("=" * 60)
        
        if not self.violations_found:
            print("\n✅ No violations found!")
        else:
            print(f"\n⚠️  Found {len(self.violations_found)} violations:\n")
            
            # Group by severity
            critical = [v for v in self.violations_found if v["severity"] == "CRITICAL"]
            high = [v for v in self.violations_found if v["severity"] == "HIGH"]
            medium = [v for v in self.violations_found if v["severity"] == "MEDIUM"]
            
            if critical:
                print(f"🔴 CRITICAL ({len(critical)}):")
                for v in critical[:5]:  # Show first 5
                    print(f"  - {v['type']}: {v.get('details', v.get('violations', ''))}")
                    
            if high:
                print(f"\n🟠 HIGH ({len(high)}):")
                for v in high[:5]:
                    print(f"  - {v['type']}: {v.get('details', v.get('violations', ''))}")
                    
            if medium:
                print(f"\n🟡 MEDIUM ({len(medium)}):")
                for v in medium[:5]:
                    print(f"  - {v['type']}: {v.get('details', v.get('violations', ''))}")
        
        # Save detailed report
        report = {
            "config": {
                "num_users": self.config.num_users,
                "num_pools": self.config.num_pools,
                "num_operations": self.config.num_operations,
                "seed": self.config.seed
            },
            "violations": self.violations_found,
            "summary": {
                "total_violations": len(self.violations_found),
                "critical": len([v for v in self.violations_found if v["severity"] == "CRITICAL"]),
                "high": len([v for v in self.violations_found if v["severity"] == "HIGH"]),
                "medium": len([v for v in self.violations_found if v["severity"] == "MEDIUM"])
            }
        }
        
        with open("/workspace/kfarms-audit/reports/fuzzing_report.json", "w") as f:
            json.dump(report, f, indent=2, default=str)
            
        print(f"\n📄 Detailed report saved to: reports/fuzzing_report.json")
        print(f"📊 Operation log entries: {len(self.operation_log)}")

def main():
    """Main fuzzing entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description="KFarms Protocol Fuzzer")
    parser.add_argument("--users", type=int, default=10, help="Number of users")
    parser.add_argument("--pools", type=int, default=5, help="Number of pools")
    parser.add_argument("--operations", type=int, default=1000, help="Number of operations")
    parser.add_argument("--seed", type=int, help="Random seed for reproducibility")
    parser.add_argument("--quick", action="store_true", help="Quick test with fewer operations")
    
    args = parser.parse_args()
    
    config = FuzzConfig(
        num_users=args.users,
        num_pools=args.pools,
        num_operations=100 if args.quick else args.operations,
        seed=args.seed
    )
    
    fuzzer = ProtocolFuzzer(config)
    fuzzer.run_all_tests()

if __name__ == "__main__":
    main()