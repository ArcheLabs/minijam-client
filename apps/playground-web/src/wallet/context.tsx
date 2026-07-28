import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import type { WalletAccount, WalletAdapter } from "./adapter";
import { ExtensionWalletAdapter } from "./extension";
import { TestWalletAdapter } from "./test";

interface WalletState {
  adapter: WalletAdapter;
  account?: WalletAccount;
  accounts: WalletAccount[];
  connecting: boolean;
  connect(): Promise<void>;
  select(accountId: string): void;
  disconnect(): void;
}

const WalletContext = createContext<WalletState | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo(
    () => import.meta.env.VITE_TEST_WALLET === "true"
      ? new TestWalletAdapter()
      : new ExtensionWalletAdapter(),
    []
  );
  const [accounts, setAccounts] = useState<WalletAccount[]>([]);
  const [account, setAccount] = useState<WalletAccount>();
  const [connecting, setConnecting] = useState(false);

  async function connect() {
    setConnecting(true);
    try {
      const next = await adapter.connect();
      setAccounts(next);
      setAccount(next.find((item) => item.type === "sr25519") ?? next[0]);
    } finally {
      setConnecting(false);
    }
  }

  function disconnect() {
    adapter.disconnect();
    setAccounts([]);
    setAccount(undefined);
  }

  return (
    <WalletContext.Provider value={{
      adapter,
      account,
      accounts,
      connecting,
      connect,
      select: (accountId) => setAccount(accounts.find((item) => item.accountId === accountId)),
      disconnect
    }}>
      {children}
    </WalletContext.Provider>
  );
}

export function useWallet() {
  const value = useContext(WalletContext);
  if (!value) throw new Error("WalletProvider is missing.");
  return value;
}
