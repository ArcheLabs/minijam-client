import { useWallet } from "../wallet/context";

export function WalletButton() {
  const wallet = useWallet();
  if (!wallet.account) {
    return (
      <button className="wallet-button" disabled={wallet.connecting} onClick={() => void wallet.connect()}>
        {wallet.connecting ? "Connecting…" : "Connect wallet"}
      </button>
    );
  }
  return (
    <div className="wallet-connected">
      <span><b>{wallet.account.name}</b><small className="mono">{short(wallet.account.accountId)}</small></span>
      {wallet.accounts.length > 1 && (
        <select aria-label="Wallet account" value={wallet.account.accountId} onChange={(event) => wallet.select(event.target.value)}>
          {wallet.accounts.map((account) => <option key={account.accountId} value={account.accountId}>{account.name}</option>)}
        </select>
      )}
      <button className="text-button" onClick={wallet.disconnect}>Disconnect</button>
    </div>
  );
}

function short(value: string) {
  return `${value.slice(0, 8)}…${value.slice(-6)}`;
}
