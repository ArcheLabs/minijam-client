import { u8aToHex } from "@polkadot/util";
import { cryptoWaitReady, sr25519PairFromSeed, sr25519Sign } from "@polkadot/util-crypto";
import type { WalletAccount, WalletAdapter } from "./adapter";

export class TestWalletAdapter implements WalletAdapter {
  private pair?: ReturnType<typeof sr25519PairFromSeed>;

  constructor(private readonly seed = new Uint8Array(32).fill(7)) {}

  async connect(): Promise<WalletAccount[]> {
    await cryptoWaitReady();
    this.pair = sr25519PairFromSeed(this.seed);
    return [{ accountId: u8aToHex(this.pair.publicKey), name: "Stage 0 test wallet", type: "sr25519" }];
  }

  async sign(_account: WalletAccount, payloadHex: string): Promise<string> {
    await cryptoWaitReady();
    this.pair ??= sr25519PairFromSeed(this.seed);
    const payload = Uint8Array.from(payloadHex.slice(2).match(/.{2}/g)!.map((byte) => Number.parseInt(byte, 16)));
    return u8aToHex(sr25519Sign(payload, this.pair));
  }

  disconnect() {
    this.pair = undefined;
  }
}
