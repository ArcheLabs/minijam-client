import { web3Accounts, web3Enable, web3FromAddress } from "@polkadot/extension-dapp";
import { u8aToHex } from "@polkadot/util";
import { decodeAddress } from "@polkadot/util-crypto";
import { SR25519_ONLY_ERROR, type WalletAccount, type WalletAdapter } from "./adapter";

const extensionAddresses = new Map<string, string>();

export class ExtensionWalletAdapter implements WalletAdapter {
  async connect(): Promise<WalletAccount[]> {
    const extensions = await web3Enable("MiniJAM Stage 0 Playground");
    if (!extensions.length) throw new Error("No compatible Polkadot wallet extension was found.");
    const accounts = await web3Accounts();
    return accounts.map(({ address, meta, type }) => {
      const accountId = u8aToHex(decodeAddress(address));
      extensionAddresses.set(accountId, address);
      return { accountId, name: meta.name ?? "Wallet account", type: type ?? "unknown" };
    });
  }

  async sign(account: WalletAccount, payloadHex: string): Promise<string> {
    if (account.type !== "sr25519") throw new Error(SR25519_ONLY_ERROR);
    const address = extensionAddresses.get(account.accountId);
    if (!address) throw new Error("Reconnect the wallet before signing.");
    const injector = await web3FromAddress(address);
    if (!injector.signer.signRaw) throw new Error("The selected wallet cannot sign raw payloads.");
    const result = await injector.signer.signRaw({ address, data: payloadHex, type: "bytes" });
    return result.signature;
  }

  disconnect() {
    extensionAddresses.clear();
  }
}
