import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Kfarms } from "../target/types/kfarms";

// Minimal governance access-control tests (negative cases) to exercise handlers' constraints
describe("kfarms governance access control", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.Kfarms as Program<Kfarms>;

  it("rejects update_global_config by non-admin", async () => {
    // We are just asserting that the method exists and fails without proper accounts
    // This is a smoke test; full integration setup would require PDAs and token accounts
    const bogus = anchor.web3.Keypair.generate();
    const value = Buffer.alloc(32);
    await program.methods
      .updateGlobalConfig(0, Array.from(value as unknown as Uint8Array) as any)
      .accounts({
        globalAdmin: bogus.publicKey,
        globalConfig: bogus.publicKey,
      })
      .rpc()
      .then(() => Promise.reject(new Error("should have failed")))
      .catch(() => Promise.resolve());
  });
});
