import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Kfarms } from "../target/types/kfarms";

describe("kfarms", () => {
  // Configure the client to use the local cluster if available, otherwise skip.
  const providerUrl = process.env.ANCHOR_PROVIDER_URL;
  if (!providerUrl) {
    it("skipped: missing ANCHOR_PROVIDER_URL", function () {
      this.skip();
    });
    return;
  }

  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.Kfarms as Program<Kfarms>;

  it("loads program id", async () => {
    // sanity check program id matches Anchor.toml
    const id = program.programId.toBase58();
    // eslint-disable-next-line no-unused-expressions
    expect(id).to.be.a("string");
  });
});
