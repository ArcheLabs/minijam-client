import { playgroundApi, type Authorization } from "../api/playground";
import { SR25519_ONLY_ERROR, type WalletAccount, type WalletAdapter } from "../wallet/adapter";
import { paramsHash } from "./hash";

export const ACTION_DOMAIN = "minijam/playground-action/v1";
export type PlaygroundAction = "create_service" | "upgrade_service" | "work";

export async function signAction(
  wallet: WalletAdapter,
  account: WalletAccount,
  action: PlaygroundAction,
  params: Record<string, unknown>,
  expectedGenesis = import.meta.env.VITE_MINIJAM_GENESIS_HASH
): Promise<Authorization> {
  if (account.type !== "sr25519") throw new Error(SR25519_ONLY_ERROR);
  if (!expectedGenesis) throw new Error("The Playground genesis hash is not configured.");
  const hash = paramsHash(params);
  const expiry = Math.floor(Date.now() / 1000) + 120;
  const prepared = await playgroundApi.prepareAction({
    account: account.accountId,
    action,
    paramsHash: hash,
    expiry
  });
  const valid =
    prepared.account.toLowerCase() === account.accountId.toLowerCase() &&
    prepared.action === action &&
    prepared.paramsHash.toLowerCase() === hash.toLowerCase() &&
    prepared.domain === ACTION_DOMAIN &&
    prepared.genesis.toLowerCase() === expectedGenesis.toLowerCase() &&
    prepared.expiry === expiry &&
    /^0x[0-9a-f]{64}$/i.test(prepared.signingPayload);
  if (!valid) throw new Error("The prepared action does not match the operation you confirmed.");
  const signature = await wallet.sign(account, prepared.signingPayload);
  return { actionId: prepared.actionId, signature };
}
