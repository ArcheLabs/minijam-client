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
        <span><b>Playground</b></span>
        {import.meta.env.VITE_LOCAL_DEVELOPMENT === "true" && (
          <span className="local-development-badge">Local Development</span>
        )}
      </a>
      <WalletButton />
    </header>
  );
}
