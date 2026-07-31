import { WalletButton } from "./wallet-button";

export function AppHeader() {
  return (
    <header className="topbar">
      <a className="brand" href="/" aria-label="MiniJAM Playground home">
        <span className="brand-mark">MJ</span>
        <span>MiniJAM <b>Playground</b></span>
      </a>
      <WalletButton />
    </header>
  );
}
