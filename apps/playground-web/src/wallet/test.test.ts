import { u8aToHex, u8aWrapBytes } from "@polkadot/util";
import { cryptoWaitReady, sr25519Verify } from "@polkadot/util-crypto";
import { describe, expect, it } from "vitest";
import { TestWalletAdapter } from "./test";

describe("TestWalletAdapter", () => {
  it("signs the Polkadot bytes-wrapped message", async () => {
    await cryptoWaitReady();
    const wallet = new TestWalletAdapter(new Uint8Array(32).fill(7));
    const [account] = await wallet.connect();
    const signingHash = new Uint8Array(32).fill(0x5a);
    const signature = await wallet.sign(account, u8aToHex(signingHash));

    expect(sr25519Verify(u8aWrapBytes(signingHash), signature, account.accountId)).toBe(true);
    expect(sr25519Verify(signingHash, signature, account.accountId)).toBe(false);
  });
});
