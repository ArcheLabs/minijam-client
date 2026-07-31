import { useEffect, useRef, useState } from "react";
import { navigate } from "../app";
import { useWallet } from "../wallet/context";

export function WalletButton() {
  const wallet = useWallet();
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: MouseEvent) => {
      if (!container.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  if (!wallet.account) {
    return (
      <button className="wallet-button" disabled={wallet.connecting} onClick={() => void wallet.connect()}>
        {wallet.connecting ? "Connecting…" : "Connect wallet"}
      </button>
    );
  }
  return (
    <div className="wallet-menu" ref={container}>
      <button
        className="wallet-account-button"
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((value) => !value)}
      >
        <span><b>{wallet.account.name}</b><small className="mono">{short(wallet.account.accountId)}</small></span>
        <span aria-hidden="true">⌄</span>
      </button>
      {open && (
        <div className="wallet-dropdown" role="menu" aria-label="Wallet menu">
          {wallet.accounts.length > 1 && (
            <label>Account
              <select
                aria-label="Wallet account"
                value={wallet.account.accountId}
                onChange={(event) => {
                  wallet.select(event.target.value);
                  setOpen(false);
                }}
              >
                {wallet.accounts.map((account) => (
                  <option key={account.accountId} value={account.accountId}>{account.name}</option>
                ))}
              </select>
            </label>
          )}
          <button role="menuitem" onClick={() => {
            setOpen(false);
            navigate("/services");
          }}>
            My Services
          </button>
          <button role="menuitem" onClick={() => {
            wallet.disconnect();
            setOpen(false);
            if (window.location.pathname === "/services") navigate("/");
          }}>
            Disconnect
          </button>
        </div>
      )}
    </div>
  );
}

function short(value: string) {
  return `${value.slice(0, 8)}…${value.slice(-6)}`;
}
