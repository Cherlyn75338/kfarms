describe("kfarms integration smoke", () => {
  it("runs a basic environment check", async () => {
    // This does not require Anchor localnet and ensures the test runner works
    const envUrl = process.env.ANCHOR_PROVIDER_URL || "<unset>";
    const wallet = process.env.ANCHOR_WALLET || "<unset>";
    console.log("ANCHOR_PROVIDER_URL=", envUrl);
    console.log("ANCHOR_WALLET=", wallet);
  });
});
