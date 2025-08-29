#!/usr/bin/env python3
"""
KFarms Audit Framework - Vulnerability Detection Demo
This script simulates the audit process and demonstrates vulnerability detection
"""

import json
import sys
from datetime import datetime
from typing import Dict, List, Tuple

class Colors:
    RED = '\033[91m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    MAGENTA = '\033[95m'
    CYAN = '\033[96m'
    WHITE = '\033[97m'
    RESET = '\033[0m'
    BOLD = '\033[1m'

class MathAuditor:
    """Mathematical invariant checker"""
    
    def __init__(self):
        self.SCALE = 10**18
        self.MAX_POINTS = 2**64 * 1000
        self.MAX_MULTIPLIER_BPS = 30000
        self.BPS_DIVISOR = 10000
        
    def check_invariants(self, state: Dict) -> List[Dict]:
        """Check all mathematical invariants"""
        violations = []
        
        # Check 1: Non-negativity
        for pool_id, pool in state['pools'].items():
            if pool['total_points'] < 0:
                violations.append({
                    'severity': 'CRITICAL',
                    'type': 'Non-negativity Violation',
                    'description': f'Pool {pool_id} has negative total_points',
                    'impact': 'Can lead to underflow and reward theft'
                })
                
        # Check 2: Conservation of rewards
        total_distributed = state['global']['total_distributed_global']
        emission_integral = state['global']['total_emission_integral']
        
        if total_distributed > emission_integral:
            violations.append({
                'severity': 'CRITICAL',
                'type': 'Conservation Violation',
                'description': f'Distributed ({total_distributed}) > Emitted ({emission_integral})',
                'impact': 'Protocol is insolvent - more rewards distributed than minted',
                'exploitation': 'Attackers can drain reward vault'
            })
            
        # Check 3: Points consistency
        for pool_id, pool in state['pools'].items():
            calculated_points = sum(user['points'] for user in state['users'].values())
            stored_points = pool['total_points']
            
            tolerance = 10
            if abs(calculated_points - stored_points) > tolerance:
                violations.append({
                    'severity': 'HIGH',
                    'type': 'Points Mismatch',
                    'description': f'Pool {pool_id}: calculated {calculated_points} != stored {stored_points}',
                    'impact': 'Incorrect reward distribution',
                    'exploitation': 'Users can claim more rewards than entitled'
                })
                
        # Check 4: Cumulative RPP monotonicity
        for pool_id, pool in state['pools'].items():
            if pool['cumulative_rpp'] < 0:
                violations.append({
                    'severity': 'CRITICAL',
                    'type': 'Monotonicity Violation',
                    'description': f'Pool {pool_id} has negative cumulative_rpp',
                    'impact': 'Time-travel attack possible'
                })
                
        # Check 5: Overflow risks
        for user_id, user in state['users'].items():
            # Check potential overflow in points calculation
            potential_points = user['staked_amount'] * self.MAX_MULTIPLIER_BPS
            if potential_points > 2**64:
                violations.append({
                    'severity': 'HIGH',
                    'type': 'Overflow Risk',
                    'description': f'User {user_id} points calculation can overflow u64',
                    'impact': 'Points wrap around to small value, losing rewards',
                    'exploitation': 'User loses accumulated rewards'
                })
                
        return violations

class ExternalPointsAuditor:
    """External points integration threat analyzer"""
    
    def analyze_threats(self, tx_history: List[Dict]) -> List[Dict]:
        """Analyze transaction history for external points threats"""
        threats = []
        
        # Check 1: Flash points attack
        for i in range(len(tx_history) - 1):
            curr = tx_history[i]
            next_tx = tx_history[i + 1] if i + 1 < len(tx_history) else None
            
            if next_tx and curr['user'] == next_tx['user']:
                # Check for massive increase followed by decrease
                increase_ratio = curr['new_points'] / max(curr['old_points'], 1)
                if next_tx:
                    decrease_ratio = next_tx['new_points'] / max(curr['new_points'], 1)
                    
                    if increase_ratio > 10 and decrease_ratio < 0.2:
                        threats.append({
                            'severity': 'CRITICAL',
                            'type': 'Flash Points Attack',
                            'description': f"User {curr['user'][:8]}... inflated points from {curr['old_points']} to {curr['new_points']} then back to {next_tx['new_points']}",
                            'impact': 'Temporary capture of emission rewards',
                            'exploitation': 'Attacker claims outsized rewards during inflation window',
                            'mitigation': 'Implement TWAP/EMA smoothing with α=10%'
                        })
                        
        # Check 2: Missing custody proof
        for tx in tx_history:
            if tx['new_points'] > 1000 and tx['custody_proof'] is None:
                threats.append({
                    'severity': 'HIGH',
                    'type': 'Custody Mismatch',
                    'description': f"User {tx['user'][:8]}... has {tx['new_points']} points without custody proof",
                    'impact': 'Points awarded without locked tokens',
                    'exploitation': 'Free points generation leading to reward theft',
                    'mitigation': 'Require CPI-based custody verification'
                })
                
        # Check 3: Rapid points changes (sandwich attack pattern)
        user_changes = {}
        for tx in tx_history:
            user = tx['user']
            if user not in user_changes:
                user_changes[user] = []
            user_changes[user].append((tx['slot'], tx['new_points'] - tx['old_points']))
            
        for user, changes in user_changes.items():
            if len(changes) >= 3:
                # Check for increase -> action -> decrease pattern
                for i in range(len(changes) - 2):
                    if changes[i][1] > 1000 and changes[i+2][1] < -1000:
                        if changes[i+2][0] - changes[i][0] < 100:  # Within 100 slots
                            threats.append({
                                'severity': 'HIGH',
                                'type': 'Sandwich Attack Pattern',
                                'description': f"User {user[:8]}... shows sandwich pattern",
                                'impact': 'MEV extraction around governance/rewards',
                                'mitigation': 'Implement per-slot change limits'
                            })
                            break
                            
        return threats

class SolanaAuditor:
    """Solana/Anchor specific vulnerability checker"""
    
    def check_patterns(self) -> List[Dict]:
        """Check for common Solana vulnerabilities"""
        vulnerabilities = []
        
        # Simulated pattern checks (would normally scan actual code)
        patterns = [
            {
                'pattern': 'as u64',
                'found': True,
                'severity': 'HIGH',
                'description': 'Unchecked type casting found',
                'impact': 'Integer overflow leading to incorrect calculations',
                'example': 'let points = (amount as u64) * multiplier;  // Can overflow!',
                'fix': 'Use checked_mul: amount.checked_mul(multiplier)?'
            },
            {
                'pattern': 'create_program_address with client bump',
                'found': True,
                'severity': 'CRITICAL',
                'description': 'Client-provided bump seed detected',
                'impact': 'PDA manipulation allowing unauthorized access',
                'example': 'Pubkey::create_program_address(&[seed, &[bump]], program_id)',
                'fix': 'Use find_program_address to derive bump internally'
            },
            {
                'pattern': 'division without zero check',
                'found': True,
                'severity': 'CRITICAL',
                'description': 'Unguarded division operation',
                'impact': 'Panic/DoS when denominator is zero',
                'example': 'let reward_per_point = total_rewards / total_points;',
                'fix': 'Guard: if total_points > 0 { ... } else { 0 }'
            },
            {
                'pattern': 'missing mut for state change',
                'found': False,
                'severity': 'MEDIUM',
                'description': 'Account not marked mutable',
                'impact': 'Transaction fails at runtime'
            },
            {
                'pattern': 'unbounded vector iteration',
                'found': True,
                'severity': 'HIGH',
                'description': 'Loop over unbounded collection',
                'impact': 'Compute budget exhaustion (DoS)',
                'example': 'for user in pool.all_users.iter() { ... }',
                'fix': 'Use pagination or indexed access'
            }
        ]
        
        for pattern in patterns:
            if pattern['found']:
                vulnerabilities.append(pattern)
                
        return vulnerabilities

def print_header():
    print(f"\n{Colors.BLUE}{Colors.BOLD}{'='*80}{Colors.RESET}")
    print(f"{Colors.BLUE}{Colors.BOLD}     KFarms Protocol Security Audit - Vulnerability Analysis{Colors.RESET}")
    print(f"{Colors.BLUE}{Colors.BOLD}{'='*80}{Colors.RESET}\n")

def print_section(title: str):
    print(f"\n{Colors.GREEN}{Colors.BOLD}{'─'*80}{Colors.RESET}")
    print(f"{Colors.GREEN}{Colors.BOLD}  {title}{Colors.RESET}")
    print(f"{Colors.GREEN}{Colors.BOLD}{'─'*80}{Colors.RESET}\n")

def severity_color(severity: str) -> str:
    colors = {
        'CRITICAL': Colors.RED,
        'HIGH': Colors.YELLOW,
        'MEDIUM': Colors.CYAN,
        'LOW': Colors.WHITE
    }
    return colors.get(severity, Colors.WHITE)

def main():
    print_header()
    
    # Load test data
    with open('test_data/protocol_state.json', 'r') as f:
        protocol_state = json.load(f)
    
    with open('test_data/tx_history.json', 'r') as f:
        tx_history = json.load(f)
    
    # Initialize auditors
    math_auditor = MathAuditor()
    external_auditor = ExternalPointsAuditor()
    solana_auditor = SolanaAuditor()
    
    all_findings = []
    
    # Phase 1: Mathematical Invariant Checking
    print_section("PHASE 1: Mathematical Invariant Verification")
    
    math_violations = math_auditor.check_invariants(protocol_state)
    
    if math_violations:
        print(f"{Colors.RED}⚠️  Found {len(math_violations)} mathematical violations:{Colors.RESET}\n")
        for violation in math_violations:
            color = severity_color(violation['severity'])
            print(f"{color}{Colors.BOLD}[{violation['severity']}]{Colors.RESET} {violation['type']}")
            print(f"  {Colors.WHITE}Description: {violation['description']}{Colors.RESET}")
            if 'impact' in violation:
                print(f"  {Colors.YELLOW}Impact: {violation['impact']}{Colors.RESET}")
            if 'exploitation' in violation:
                print(f"  {Colors.RED}Exploitation: {violation['exploitation']}{Colors.RESET}")
            print()
        all_findings.extend(math_violations)
    else:
        print(f"{Colors.GREEN}✅ All mathematical invariants passed{Colors.RESET}")
    
    # Phase 2: External Points Threat Analysis
    print_section("PHASE 2: External Points Integration Analysis")
    
    external_threats = external_auditor.analyze_threats(tx_history)
    
    if external_threats:
        print(f"{Colors.RED}⚠️  Found {len(external_threats)} external points threats:{Colors.RESET}\n")
        for threat in external_threats:
            color = severity_color(threat['severity'])
            print(f"{color}{Colors.BOLD}[{threat['severity']}]{Colors.RESET} {threat['type']}")
            print(f"  {Colors.WHITE}Description: {threat['description']}{Colors.RESET}")
            print(f"  {Colors.YELLOW}Impact: {threat['impact']}{Colors.RESET}")
            print(f"  {Colors.RED}Exploitation: {threat['exploitation']}{Colors.RESET}")
            print(f"  {Colors.GREEN}Mitigation: {threat['mitigation']}{Colors.RESET}")
            print()
        all_findings.extend(external_threats)
    else:
        print(f"{Colors.GREEN}✅ No external points threats detected{Colors.RESET}")
    
    # Phase 3: Solana/Anchor Pattern Analysis
    print_section("PHASE 3: Solana/Anchor Security Patterns")
    
    solana_vulns = solana_auditor.check_patterns()
    
    if solana_vulns:
        print(f"{Colors.RED}⚠️  Found {len(solana_vulns)} Solana-specific vulnerabilities:{Colors.RESET}\n")
        for vuln in solana_vulns:
            if vuln['found']:
                color = severity_color(vuln['severity'])
                print(f"{color}{Colors.BOLD}[{vuln['severity']}]{Colors.RESET} {vuln['description']}")
                print(f"  {Colors.YELLOW}Impact: {vuln['impact']}{Colors.RESET}")
                if 'example' in vuln:
                    print(f"  {Colors.MAGENTA}Example: {vuln['example']}{Colors.RESET}")
                if 'fix' in vuln:
                    print(f"  {Colors.GREEN}Fix: {vuln['fix']}{Colors.RESET}")
                print()
        all_findings.extend([v for v in solana_vulns if v['found']])
    
    # Summary
    print_section("AUDIT SUMMARY")
    
    severity_counts = {'CRITICAL': 0, 'HIGH': 0, 'MEDIUM': 0, 'LOW': 0}
    for finding in all_findings:
        severity_counts[finding['severity']] = severity_counts.get(finding['severity'], 0) + 1
    
    print(f"{Colors.BOLD}Total Findings: {len(all_findings)}{Colors.RESET}\n")
    print(f"  {Colors.RED}● Critical: {severity_counts['CRITICAL']}{Colors.RESET}")
    print(f"  {Colors.YELLOW}● High:     {severity_counts['HIGH']}{Colors.RESET}")
    print(f"  {Colors.CYAN}● Medium:   {severity_counts['MEDIUM']}{Colors.RESET}")
    print(f"  {Colors.WHITE}● Low:      {severity_counts['LOW']}{Colors.RESET}")
    
    # Risk Assessment
    print_section("RISK ASSESSMENT")
    
    if severity_counts['CRITICAL'] > 0:
        print(f"{Colors.RED}{Colors.BOLD}⛔ CRITICAL RISK - DO NOT DEPLOY{Colors.RESET}")
        print(f"\nThe protocol has {severity_counts['CRITICAL']} critical vulnerabilities that could lead to:")
        print("  • Direct theft of user funds")
        print("  • Complete protocol insolvency")
        print("  • Permanent freezing of assets")
        print("  • Governance manipulation")
    elif severity_counts['HIGH'] > 0:
        print(f"{Colors.YELLOW}{Colors.BOLD}⚠️  HIGH RISK - REQUIRES IMMEDIATE FIXES{Colors.RESET}")
        print(f"\nThe protocol has {severity_counts['HIGH']} high-severity issues that could lead to:")
        print("  • Theft of unclaimed yield")
        print("  • Temporary freezing of funds")
        print("  • Unfair reward distribution")
    else:
        print(f"{Colors.GREEN}{Colors.BOLD}✅ ACCEPTABLE RISK LEVEL{Colors.RESET}")
        print("\nNo critical or high-severity issues found.")
    
    # Recommendations
    print_section("KEY RECOMMENDATIONS")
    
    recommendations = [
        "1. **Implement TWAP/EMA smoothing** for external points with α=10% to prevent flash attacks",
        "2. **Add custody verification** via CPI to ensure points match locked tokens",
        "3. **Use checked arithmetic** everywhere - replace all 'as u64' with checked_mul",
        "4. **Guard all divisions** - check denominators > 0 before dividing",
        "5. **Derive PDA bumps internally** - never accept client-provided bumps",
        "6. **Implement per-slot change limits** for points updates (max 10K/slot)",
        "7. **Add circuit breakers** for emergency pause functionality",
        "8. **Use wide math (u128)** for intermediate calculations to prevent overflow",
        "9. **Paginate unbounded operations** to avoid compute budget exhaustion",
        "10. **Add comprehensive logging** for all state changes for audit trail"
    ]
    
    for rec in recommendations:
        print(f"  {rec}")
    
    print(f"\n{Colors.BLUE}{Colors.BOLD}{'='*80}{Colors.RESET}")
    print(f"{Colors.BLUE}{Colors.BOLD}     Audit Complete - {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}{Colors.RESET}")
    print(f"{Colors.BLUE}{Colors.BOLD}{'='*80}{Colors.RESET}\n")
    
    return 1 if severity_counts['CRITICAL'] > 0 else 0

if __name__ == "__main__":
    sys.exit(main())