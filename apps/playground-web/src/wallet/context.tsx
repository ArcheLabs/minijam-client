import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
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
const SESSION_KEY = "minijam.playground.wallet.v1";

interface WalletSession {
  connected: boolean;
  selectedAccountId?: string;
}

function readSession(): WalletSession | undefined {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return undefined;
    const session = parsed as Partial<WalletSession>;
    if (session.connected !== true) return undefined;
    return {
      connected: true,
      selectedAccountId: typeof session.selectedAccountId === "string"
        ? session.selectedAccountId
        : undefined
    };
  } catch {
    return undefined;
  }
}

function writeSession(account: WalletAccount) {
  try {
    localStorage.setItem(SESSION_KEY, JSON.stringify({
      connected: true,
      selectedAccountId: account.accountId
    } satisfies WalletSession));
  } catch {
    // Wallet operation should still work when storage is unavailable.
  }
}

function clearSession() {
  try {
    localStorage.removeItem(SESSION_KEY);
  } catch {
    // Ignore unavailable browser storage during disconnect/recovery.
  }
}

function chooseAccount(accounts: WalletAccount[], preferredAccountId?: string) {
  return accounts.find((item) =>
    item.accountId.toLowerCase() === preferredAccountId?.toLowerCase()
  ) ?? accounts.find((item) => item.type === "sr25519") ?? accounts[0];
}

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

  async function loadAccounts(preferredAccountId?: string) {
    const next = await adapter.connect();
    const selected = chooseAccount(next, preferredAccountId);
    setAccounts(next);
    setAccount(selected);
    if (selected) writeSession(selected);
  }

  useEffect(() => {
    const session = readSession();
    if (!session?.connected) return;

    let cancelled = false;
    setConnecting(true);
    adapter.connect()
      .then((next) => {
        if (cancelled) return;
        const selected = chooseAccount(next, session.selectedAccountId);
        setAccounts(next);
        setAccount(selected);
        if (selected) writeSession(selected);
      })
      .catch(() => {
        if (!cancelled) {
          clearSession();
          setAccounts([]);
          setAccount(undefined);
        }
      })
      .finally(() => {
        if (!cancelled) setConnecting(false);
      });

    return () => {
      cancelled = true;
    };
  }, [adapter]);

  async function connect() {
    setConnecting(true);
    try {
      await loadAccounts();
    } finally {
      setConnecting(false);
    }
  }

  function disconnect() {
    adapter.disconnect();
    clearSession();
    setAccounts([]);
    setAccount(undefined);
  }

  function select(accountId: string) {
    const selected = accounts.find((item) =>
      item.accountId.toLowerCase() === accountId.toLowerCase()
    );
    setAccount(selected);
    if (selected) writeSession(selected);
  }

  return (
    <WalletContext.Provider value={{
      adapter,
      account,
      accounts,
      connecting,
      connect,
      select,
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
