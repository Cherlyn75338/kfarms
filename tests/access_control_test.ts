import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Kfarms } from "../target/types/kfarms";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, createMint, createAccount, mintTo } from "@solana/spl-token";
import { assert } from "chai";

describe("Access Control Vulnerability Test", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Kfarms as Program<Kfarms>;

  // Test accounts
  let globalAdmin: Keypair;
  let farmAdmin: Keypair;
  let attacker: Keypair;
  let globalConfig: PublicKey;
  let farmState: PublicKey;
  let tokenMint: PublicKey;
  let rewardMint: PublicKey;

  before(async () => {
    // Setup test accounts
    globalAdmin = Keypair.generate();
    farmAdmin = Keypair.generate();
    attacker = Keypair.generate();

    // Airdrop SOL to test accounts
    await provider.connection.requestAirdrop(globalAdmin.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(farmAdmin.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(attacker.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    
    // Wait for airdrops to confirm
    await new Promise(resolve => setTimeout(resolve, 1000));
  });

  describe("Test 1: Verify Admin Access Control on update_farm_config", () => {
    it("Should NOT allow non-admin to update emission rates", async () => {
      // First, initialize global config as global admin
      const globalConfigKeypair = Keypair.generate();
      globalConfig = globalConfigKeypair.publicKey;

      const [treasuryVaultsAuthority] = await PublicKey.findProgramAddress(
        [Buffer.from("authority"), globalConfig.toBuffer()],
        program.programId
      );

      try {
        await program.methods
          .initializeGlobalConfig()
          .accounts({
            globalAdmin: globalAdmin.publicKey,
            globalConfig: globalConfig,
            treasuryVaultsAuthority: treasuryVaultsAuthority,
            systemProgram: SystemProgram.programId,
          })
          .signers([globalAdmin, globalConfigKeypair])
          .rpc();
      } catch (e) {
        console.log("Global config initialization error:", e);
      }

      // Initialize a farm as farm admin
      const farmStateKeypair = Keypair.generate();
      farmState = farmStateKeypair.publicKey;

      // Create token mint
      tokenMint = await createMint(
        provider.connection,
        farmAdmin,
        farmAdmin.publicKey,
        null,
        9
      );

      const [farmVaultsAuthority] = await PublicKey.findProgramAddress(
        [Buffer.from("authority"), farmState.toBuffer()],
        program.programId
      );

      const [farmVault] = await PublicKey.findProgramAddress(
        [Buffer.from("fvault"), farmState.toBuffer(), tokenMint.toBuffer()],
        program.programId
      );

      try {
        await program.methods
          .initializeFarm()
          .accounts({
            farmAdmin: farmAdmin.publicKey,
            farmState: farmState,
            globalConfig: globalConfig,
            farmVault: farmVault,
            farmVaultsAuthority: farmVaultsAuthority,
            tokenMint: tokenMint,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([farmAdmin, farmStateKeypair])
          .rpc();
        
        console.log("✅ Farm initialized successfully by admin");
      } catch (e) {
        console.log("Farm initialization error:", e);
      }

      // Now try to update farm config as an attacker (non-admin)
      console.log("\n🔴 Testing: Attacker attempting to update emission rates...");
      
      let attackSucceeded = false;
      try {
        // Attempt to update reward RPS (emissions rate) as attacker
        const mode = 0; // UpdateRewardRps
        const rewardIndex = 0;
        const newEmissionRate = BigInt("1000000000000000000000"); // 10^20 - extremely high rate
        
        // Serialize the data
        const data = Buffer.concat([
          Buffer.from(new Uint8Array(new BigUint64Array([BigInt(rewardIndex)]).buffer)),
          Buffer.from(new Uint8Array(new BigUint64Array([newEmissionRate]).buffer))
        ]);

        await program.methods
          .updateFarmConfig(mode, data)
          .accounts({
            signer: attacker.publicKey,
            farmState: farmState,
            scopePrices: null,
          })
          .signers([attacker])
          .rpc();
        
        attackSucceeded = true;
        console.log("❌ VULNERABILITY CONFIRMED: Attacker successfully updated emission rate!");
      } catch (error) {
        console.log("✅ Access control working: Attacker's attempt failed with error:", error.message);
        assert(error.message.includes("InvalidFarmConfigUpdateAuthority") || 
               error.message.includes("has_one") ||
               error.message.includes("ConstraintHasOne"),
               "Should fail with proper access control error");
      }

      assert(!attackSucceeded, "Attack should not succeed - access control vulnerability exists!");
    });

    it("Should allow only farm_admin to update emission rates", async () => {
      console.log("\n🟢 Testing: Farm admin updating emission rates...");
      
      try {
        // First initialize a reward token
        rewardMint = await createMint(
          provider.connection,
          farmAdmin,
          farmAdmin.publicKey,
          null,
          9
        );

        const [rewardVault] = await PublicKey.findProgramAddress(
          [Buffer.from("rvault"), farmState.toBuffer(), rewardMint.toBuffer()],
          program.programId
        );

        const [treasuryVault] = await PublicKey.findProgramAddress(
          [Buffer.from("tvault"), globalConfig.toBuffer(), rewardMint.toBuffer()],
          program.programId
        );

        const [farmVaultsAuthority] = await PublicKey.findProgramAddress(
          [Buffer.from("authority"), farmState.toBuffer()],
          program.programId
        );

        const [treasuryVaultsAuthority] = await PublicKey.findProgramAddress(
          [Buffer.from("authority"), globalConfig.toBuffer()],
          program.programId
        );

        // Initialize reward
        await program.methods
          .initializeReward()
          .accounts({
            farmAdmin: farmAdmin.publicKey,
            farmState: farmState,
            globalConfig: globalConfig,
            rewardMint: rewardMint,
            rewardVault: rewardVault,
            rewardTreasuryVault: treasuryVault,
            farmVaultsAuthority: farmVaultsAuthority,
            treasuryVaultsAuthority: treasuryVaultsAuthority,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([farmAdmin])
          .rpc();

        // Now update emission rate as admin
        const mode = 0; // UpdateRewardRps  
        const rewardIndex = 0;
        const newEmissionRate = BigInt("1000"); // Normal rate
        
        const data = Buffer.concat([
          Buffer.from(new Uint8Array(new BigUint64Array([BigInt(rewardIndex)]).buffer)),
          Buffer.from(new Uint8Array(new BigUint64Array([newEmissionRate]).buffer))
        ]);

        await program.methods
          .updateFarmConfig(mode, data)
          .accounts({
            signer: farmAdmin.publicKey,
            farmState: farmState,
            scopePrices: null,
          })
          .signers([farmAdmin])
          .rpc();
        
        console.log("✅ Farm admin successfully updated emission rate");
      } catch (error) {
        console.log("Error:", error);
        throw error;
      }
    });
  });

  describe("Test 2: Verify Access Control on Global Config Updates", () => {
    it("Should NOT allow non-global-admin to update global config", async () => {
      console.log("\n🔴 Testing: Attacker attempting to update global config...");
      
      let attackSucceeded = false;
      try {
        // Try to set pending global admin as attacker
        const mode = 0; // SetPendingGlobalAdmin
        const attackerPubkeyBytes = attacker.publicKey.toBuffer();
        const value = new Uint8Array(32);
        value.set(attackerPubkeyBytes);

        await program.methods
          .updateGlobalConfig(mode, Array.from(value))
          .accounts({
            globalAdmin: attacker.publicKey,
            globalConfig: globalConfig,
          })
          .signers([attacker])
          .rpc();
        
        attackSucceeded = true;
        console.log("❌ VULNERABILITY CONFIRMED: Attacker successfully updated global config!");
      } catch (error) {
        console.log("✅ Access control working: Attacker's attempt failed");
        assert(error.message.includes("has_one") || 
               error.message.includes("ConstraintHasOne"),
               "Should fail with proper access control error");
      }

      assert(!attackSucceeded, "Attack should not succeed - access control vulnerability exists!");
    });
  });

  describe("Test 3: Verify Delegated Authority Restrictions", () => {
    it("Should NOT allow arbitrary users to act as delegate authority", async () => {
      console.log("\n🔴 Testing: Attacker attempting to use delegated authority...");
      
      // Create a delegated farm
      const delegatedFarmStateKeypair = Keypair.generate();
      const delegatedFarmState = delegatedFarmStateKeypair.publicKey;
      const legitimateDelegate = Keypair.generate();
      
      await provider.connection.requestAirdrop(legitimateDelegate.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
      await new Promise(resolve => setTimeout(resolve, 1000));

      const [farmVaultsAuthority] = await PublicKey.findProgramAddress(
        [Buffer.from("authority"), delegatedFarmState.toBuffer()],
        program.programId
      );

      try {
        // Initialize delegated farm with legitimate delegate
        await program.methods
          .initializeFarmDelegated()
          .accounts({
            farmAdmin: farmAdmin.publicKey,
            farmDelegate: legitimateDelegate.publicKey,
            farmState: delegatedFarmState,
            globalConfig: globalConfig,
            farmVaultsAuthority: farmVaultsAuthority,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([farmAdmin, legitimateDelegate, delegatedFarmStateKeypair])
          .rpc();
        
        console.log("✅ Delegated farm initialized");
      } catch (e) {
        console.log("Delegated farm initialization error:", e);
      }

      // Now try to use delegated authority as attacker
      let attackSucceeded = false;
      try {
        // Create a user state for testing
        const userStateKeypair = Keypair.generate();
        const userState = userStateKeypair.publicKey;

        // Try to set stake as attacker (not the legitimate delegate)
        await program.methods
          .setStakeDelegated(new anchor.BN(1000000))
          .accounts({
            delegateAuthority: attacker.publicKey,
            userState: userState,
            farmState: delegatedFarmState,
          })
          .signers([attacker])
          .rpc();
        
        attackSucceeded = true;
        console.log("❌ VULNERABILITY CONFIRMED: Attacker successfully used delegated authority!");
      } catch (error) {
        console.log("✅ Access control working: Attacker cannot act as delegate");
        assert(error.message.includes("AuthorityFarmDelegateMissmatch") ||
               error.message.includes("FarmNotDelegated"),
               "Should fail with delegate authority mismatch");
      }

      assert(!attackSucceeded, "Attack should not succeed - delegate authority is properly restricted!");
    });
  });
});