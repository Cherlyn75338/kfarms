### Objective and Scope

- Goal: Uncover and remediate Critical/High risks, especially around reward/points math and function usage.
- Targets: governance manipulation, direct theft, permanent freezing, insolvency, external “points setter” integrations.

### Critical Instructions

- initialize_global_config, update_global_config, initialize_farm, initialize_farm_delegated
- initialize_reward, add_rewards, withdraw_reward
- initialize_user, stake, set_stake_delegated, harvest_reward, unstake, withdraw_unstaked_deposits
- refresh_farm, refresh_user_state
- update_farm_config (reward schedule, rps, locking, caps), update_farm_admin, update_global_config_admin, update_second_delegated_authority
- withdraw_treasury, deposit_to_farm_vault, withdraw_from_farm_vault, withdraw_slashed_amount, transfer_ownership

### Key Accounts and Token Flows

- Farm-level: `FarmState`, `GlobalConfig`, reward vaults (per reward), farm vault, vault authorities
- User-level: `UserState` with active/pending stake, reward tallies, last claim
- Token programs: SPL Token and Token-2022; extension validation enforced for rewards
- Emissions: `RewardInfo` with `reward_schedule_curve`, `reward_per_share_scaled`, `rewards_available`

### State Machine (simplified)

```mermaid
digraph G {
  rankdir=LR;
  subgraph cluster_pool {
    label="Pool Lifecycle";
    init_farm -> active;
    active -> frozen [label="withdraw_from_farm_vault drain -> freeze"];
    frozen -> active [label="admin remediate"];
  }
  subgraph cluster_user {
    label="User Lifecycle";
    none -> initialized [label="initialize_user"];
    initialized -> staking [label="stake / set_stake_delegated"];
    staking -> cooldown [label="unstake (sets pending)"];
    cooldown -> withdrawn [label="withdraw_unstaked_deposits"];
    staking -> staking [label="harvest_reward"];
  }
}
```

### Pre/Post Conditions (selected)

- stake: pool rewards refreshed; user rewards refreshed; O(1); transfer from user -> farm_vault; deposit caps enforced
- harvest_reward: refresh pool, user; enforce min claim duration; split user/treasury; Token-2022 checked transfer from rewards_vault
- unstake: refresh pool/user; apply locking penalties if configured; set pending withdrawal and tally decrement; no immediate token transfer
- withdraw_unstaked_deposits: require cooldown elapsed; transfer from farm_vault -> user
- update_farm_config: authority checks; apply pending accrual before changes; validate schedule points

### Emissions/Math Observations

- Global accrual uses `reward_per_share_scaled` with Decimal (u128 wide math)
- Schedule curve piecewise accumulation now uses u128 internal accumulation with overflow guard
- Delegated farms: `set_stake` updates tallies consistently; pool/user accrual refreshed before changes

### Open Questions / Items to Validate Next

- External points setter model (delegated stake is present; external custody and rate limits TBD off-chain)
- Governance: timelocks/multisig expectations; spike caps for emissions
- Decimals normalization across reward mints; cross-mint behavior in multi-reward pools