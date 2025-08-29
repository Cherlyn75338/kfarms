// PDA Bump Manipulation Vulnerability Analysis
// This demonstrates a critical vulnerability that can lead to complete protocol compromise

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};

entrypoint!(process_instruction);

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct VaultAccount {
    pub authority: Pubkey,
    pub pool_id: [u8; 32],
    pub total_deposits: u64,
    pub bump: u8,  // VULNERABILITY: Storing client-provided bump
    pub is_initialized: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum VulnerableInstruction {
    /// Initialize vault with client-provided bump - VULNERABLE
    InitializeVaultVulnerable {
        pool_id: [u8; 32],
        bump: u8,  // VULNERABILITY: Accepting bump from client
    },
    /// Deposit funds to vault
    Deposit {
        amount: u64,
    },
    /// Withdraw funds (admin only)
    Withdraw {
        amount: u64,
    },
    /// Emergency withdraw (demonstrates exploit)
    EmergencyWithdraw,
    
    // Secure versions for comparison
    InitializeVaultSecure {
        pool_id: [u8; 32],
    },
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = VulnerableInstruction::try_from_slice(instruction_data)?;
    
    match instruction {
        VulnerableInstruction::InitializeVaultVulnerable { pool_id, bump } => {
            msg!("=== VULNERABLE INITIALIZATION ===");
            process_initialize_vulnerable(program_id, accounts, pool_id, bump)
        }
        VulnerableInstruction::Deposit { amount } => {
            process_deposit(program_id, accounts, amount)
        }
        VulnerableInstruction::Withdraw { amount } => {
            process_withdraw(program_id, accounts, amount)
        }
        VulnerableInstruction::EmergencyWithdraw => {
            process_emergency_withdraw(program_id, accounts)
        }
        VulnerableInstruction::InitializeVaultSecure { pool_id } => {
            msg!("=== SECURE INITIALIZATION ===");
            process_initialize_secure(program_id, accounts, pool_id)
        }
    }
}

/// VULNERABLE: Accepts client-provided bump seed
fn process_initialize_vulnerable(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    pool_id: [u8; 32],
    client_bump: u8,  // VULNERABILITY: Client controls this value
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let vault_account = next_account_info(account_iter)?;
    let authority = next_account_info(account_iter)?;
    let system_program = next_account_info(account_iter)?;
    
    msg!("Client provided bump: {}", client_bump);
    
    // VULNERABILITY: Using client-provided bump without validation
    let seeds = &[
        b"vault",
        pool_id.as_ref(),
        &[client_bump],  // CRITICAL: Client controls PDA derivation
    ];
    
    // This creates a PDA with the client's bump
    let vault_pda = Pubkey::create_program_address(seeds, program_id)?;
    
    // VULNERABILITY CHECK: Is this the canonical address?
    let (canonical_pda, canonical_bump) = Pubkey::find_program_address(
        &[b"vault", pool_id.as_ref()],
        program_id
    );
    
    msg!("Vault PDA: {}", vault_pda);
    msg!("Canonical PDA: {}", canonical_pda);
    msg!("Canonical bump: {}", canonical_bump);
    
    if vault_pda != canonical_pda {
        msg!("⚠️ WARNING: Non-canonical PDA created!");
        msg!("This is a shadow vault at bump {}!", client_bump);
    }
    
    // Ensure the vault account matches expected PDA
    if vault_account.key != &vault_pda {
        msg!("ERROR: Account key mismatch");
        return Err(ProgramError::InvalidArgument);
    }
    
    // Create the vault account
    let rent = Rent::get()?;
    let vault_size = std::mem::size_of::<VaultAccount>();
    let lamports = rent.minimum_balance(vault_size);
    
    invoke_signed(
        &system_instruction::create_account(
            authority.key,
            vault_account.key,
            lamports,
            vault_size as u64,
            program_id,
        ),
        &[authority.clone(), vault_account.clone(), system_program.clone()],
        &[seeds],  // Using client-controlled seeds
    )?;
    
    // Initialize vault data
    let mut vault_data = VaultAccount {
        authority: *authority.key,
        pool_id,
        total_deposits: 0,
        bump: client_bump,  // VULNERABILITY: Storing attacker-controlled bump
        is_initialized: true,
    };
    
    vault_data.serialize(&mut &mut vault_account.data.borrow_mut()[..])?;
    
    msg!("Vault initialized at {} with bump {}", vault_account.key, client_bump);
    
    Ok(())
}

/// Process deposit - can be hijacked to shadow vault
fn process_deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let vault_account = next_account_info(account_iter)?;
    let depositor = next_account_info(account_iter)?;
    let _system_program = next_account_info(account_iter)?;
    
    // Load vault data
    let mut vault = VaultAccount::try_from_slice(&vault_account.data.borrow())?;
    
    if !vault.is_initialized {
        return Err(ProgramError::UninitializedAccount);
    }
    
    // VULNERABILITY: This accepts ANY valid vault, including shadow vaults
    msg!("Depositing {} to vault at {}", amount, vault_account.key);
    msg!("Vault bump: {}", vault.bump);
    
    // Check if this is the canonical vault
    let (canonical_pda, canonical_bump) = Pubkey::find_program_address(
        &[b"vault", vault.pool_id.as_ref()],
        program_id
    );
    
    if vault_account.key != &canonical_pda {
        msg!("⚠️ CRITICAL: Depositing to shadow vault!");
        msg!("Expected canonical vault: {}", canonical_pda);
        msg!("Actual vault: {}", vault_account.key);
        msg!("This deposit can be stolen!");
    }
    
    // Transfer lamports (simplified - in production use token program)
    **vault_account.lamports.borrow_mut() += amount;
    **depositor.lamports.borrow_mut() -= amount;
    
    vault.total_deposits += amount;
    vault.serialize(&mut &mut vault_account.data.borrow_mut()[..])?;
    
    msg!("Deposit complete. Total in vault: {}", vault.total_deposits);
    
    Ok(())
}

/// Withdraw - demonstrates authority bypass
fn process_withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let vault_account = next_account_info(account_iter)?;
    let authority = next_account_info(account_iter)?;
    let recipient = next_account_info(account_iter)?;
    
    let mut vault = VaultAccount::try_from_slice(&vault_account.data.borrow())?;
    
    // VULNERABILITY: Using stored bump for PDA verification
    let seeds = &[
        b"vault",
        vault.pool_id.as_ref(),
        &[vault.bump],  // Using potentially attacker-controlled bump
    ];
    
    let derived_vault = Pubkey::create_program_address(seeds, program_id)?;
    
    if vault_account.key != &derived_vault {
        msg!("ERROR: Invalid vault PDA");
        return Err(ProgramError::InvalidArgument);
    }
    
    // Authority check - can be bypassed with shadow vault
    if authority.key != &vault.authority {
        msg!("ERROR: Unauthorized withdrawal attempt");
        msg!("Expected authority: {}", vault.authority);
        msg!("Provided authority: {}", authority.key);
        
        // BUT: If attacker created shadow vault, they control the authority!
        return Err(ProgramError::InvalidArgument);
    }
    
    if amount > vault.total_deposits {
        return Err(ProgramError::InsufficientFunds);
    }
    
    // Transfer funds
    **vault_account.lamports.borrow_mut() -= amount;
    **recipient.lamports.borrow_mut() += amount;
    
    vault.total_deposits -= amount;
    vault.serialize(&mut &mut vault_account.data.borrow_mut()[..])?;
    
    msg!("Withdrew {} from vault. Remaining: {}", amount, vault.total_deposits);
    
    Ok(())
}

/// Emergency withdraw - demonstrates complete compromise
fn process_emergency_withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let vault_account = next_account_info(account_iter)?;
    let attacker = next_account_info(account_iter)?;
    
    let vault = VaultAccount::try_from_slice(&vault_account.data.borrow())?;
    
    msg!("=== EXPLOIT DEMONSTRATION ===");
    msg!("Vault at: {}", vault_account.key);
    msg!("Vault bump: {}", vault.bump);
    msg!("Total deposits: {}", vault.total_deposits);
    
    // Check if this is a shadow vault
    let (canonical_pda, canonical_bump) = Pubkey::find_program_address(
        &[b"vault", vault.pool_id.as_ref()],
        program_id
    );
    
    if vault.bump != canonical_bump {
        msg!("🚨 SHADOW VAULT DETECTED!");
        msg!("This vault uses bump {} instead of canonical {}", vault.bump, canonical_bump);
        msg!("Attacker controls this vault!");
        
        // If attacker is the authority of shadow vault, they can drain it
        if attacker.key == &vault.authority {
            msg!("✅ Attacker is authority - draining vault!");
            
            let drain_amount = vault_account.lamports();
            **vault_account.lamports.borrow_mut() = 0;
            **attacker.lamports.borrow_mut() += drain_amount;
            
            msg!("💰 Drained {} lamports to attacker!", drain_amount);
            return Ok(());
        }
    }
    
    msg!("This is the canonical vault - no exploit possible");
    Err(ProgramError::InvalidArgument)
}

/// SECURE: Derives bump internally
fn process_initialize_secure(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    pool_id: [u8; 32],
) -> ProgramResult {
    let account_iter = &mut accounts.iter();
    let vault_account = next_account_info(account_iter)?;
    let authority = next_account_info(account_iter)?;
    let system_program = next_account_info(account_iter)?;
    
    // SECURE: Always derive bump internally
    let (vault_pda, bump) = Pubkey::find_program_address(
        &[b"vault", pool_id.as_ref()],
        program_id
    );
    
    msg!("Derived canonical PDA: {}", vault_pda);
    msg!("Canonical bump: {}", bump);
    
    // Ensure account matches canonical PDA
    if vault_account.key != &vault_pda {
        msg!("ERROR: Account is not the canonical PDA");
        return Err(ProgramError::InvalidArgument);
    }
    
    // Create account with canonical bump
    let seeds = &[
        b"vault",
        pool_id.as_ref(),
        &[bump],  // SECURE: Using internally derived bump
    ];
    
    let rent = Rent::get()?;
    let vault_size = std::mem::size_of::<VaultAccount>();
    let lamports = rent.minimum_balance(vault_size);
    
    invoke_signed(
        &system_instruction::create_account(
            authority.key,
            vault_account.key,
            lamports,
            vault_size as u64,
            program_id,
        ),
        &[authority.clone(), vault_account.clone(), system_program.clone()],
        &[seeds],
    )?;
    
    // Initialize with canonical bump
    let mut vault_data = VaultAccount {
        authority: *authority.key,
        pool_id,
        total_deposits: 0,
        bump,  // SECURE: Storing canonical bump
        is_initialized: true,
    };
    
    vault_data.serialize(&mut &mut vault_account.data.borrow_mut()[..])?;
    
    msg!("✅ Secure vault initialized with canonical bump {}", bump);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::instruction::{AccountMeta, Instruction};
    
    #[test]
    fn test_bump_collision() {
        // Demonstrate that multiple bumps can create valid PDAs
        let program_id = Pubkey::new_unique();
        let pool_id = [1u8; 32];
        
        // Find canonical bump
        let (canonical_pda, canonical_bump) = Pubkey::find_program_address(
            &[b"vault", pool_id.as_ref()],
            &program_id
        );
        
        println!("Canonical PDA: {} (bump: {})", canonical_pda, canonical_bump);
        
        // Try other bumps
        let mut shadow_vaults = vec![];
        for bump in 0..=255u8 {
            if let Ok(pda) = Pubkey::create_program_address(
                &[b"vault", pool_id.as_ref(), &[bump]],
                &program_id
            ) {
                if bump != canonical_bump {
                    shadow_vaults.push((pda, bump));
                    println!("Shadow vault found: {} (bump: {})", pda, bump);
                }
            }
        }
        
        println!("\n🚨 Found {} potential shadow vaults!", shadow_vaults.len());
        println!("Each could be used to hijack deposits!");
    }
}