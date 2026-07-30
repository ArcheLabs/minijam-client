export const SR25519_ONLY_ERROR =
  "This Stage 0 Playground currently supports sr25519 accounts only.";

export interface WalletAccount {
  accountId: string;
  address?: string;
  name: string;
  type: string;
}

export interface WalletAdapter {
  connect(): Promise<WalletAccount[]>;
  sign(account: WalletAccount, payloadHex: string): Promise<string>;
  disconnect(): void;
}
