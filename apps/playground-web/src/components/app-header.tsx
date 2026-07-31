import { WalletButton } from "./wallet-button";
import logoUrl from "../public/minijam.svg";

export function AppHeader() {
  return (
    <header className="topbar">
      <a className="brand" href="/" aria-label="MiniJAM Playground home">
        <img
          className="brand-logo"
          src={logoUrl}
          alt="MiniJAM"
          style={{ width: 34, height: 34, objectFit: "contain" }}
        />
        <span>MiniJAM <b>Playground</b></span>
      </a>
      <WalletButton />
    </header>
  );
}
