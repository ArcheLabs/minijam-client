import { useEffect, useState } from "react";
import { navigate } from "../app";
import {
  PlaygroundApiError,
  playgroundApi,
  type ServiceView
} from "../api/playground";
import { ErrorPanel } from "../components/error-panel";
import { AppHeader } from "../components/app-header";
import { WalletButton } from "../components/wallet-button";
import { errorMessage } from "../lib/errors";
import {
  readRememberedServices,
  removeRememberedService,
  type RememberedService
} from "../lib/service-history";
import { useWallet } from "../wallet/context";

interface ServiceItem {
  remembered: RememberedService;
  service?: ServiceView;
  unavailable?: boolean;
}

export function ServicesPage() {
  const wallet = useWallet();
  const [items, setItems] = useState<ServiceItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let stopped = false;
    const accountId = wallet.account?.accountId;
    if (!accountId) {
      setItems([]);
      setError(undefined);
      return;
    }

    setLoading(true);
    setError(undefined);
    void (async () => {
      try {
        const { genesisHash } = await playgroundApi.getConfig();
        const remembered = readRememberedServices(genesisHash, accountId);
        const resolved = await Promise.all(remembered.map(async (entry): Promise<ServiceItem | null> => {
          try {
            const service = await playgroundApi.getService(entry.serviceId);
            if (service.controller.toLowerCase() !== accountId.toLowerCase()) {
              removeRememberedService(genesisHash, accountId, entry.serviceId);
              return null;
            }
            return { remembered: entry, service };
          } catch (cause) {
            if (cause instanceof PlaygroundApiError && cause.status === 404) {
              removeRememberedService(genesisHash, accountId, entry.serviceId);
              return null;
            }
            return { remembered: entry, unavailable: true };
          }
        }));
        if (!stopped) setItems(resolved.filter((item): item is ServiceItem => item !== null));
      } catch (cause) {
        if (!stopped) setError(errorMessage(cause));
      } finally {
        if (!stopped) setLoading(false);
      }
    })();

    return () => {
      stopped = true;
    };
  }, [wallet.account?.accountId]);

  return (
    <main className="page narrow services-page">
      <AppHeader />
      <button className="text-button page-back" onClick={() => navigate("/")}>← Playground</button>
      <p className="eyebrow">Browser history</p>
      <h1>My Services</h1>
      {!wallet.account ? (
        <section className="panel services-empty">
          <p>Connect a wallet to view Services deployed from this browser.</p>
          <WalletButton />
        </section>
      ) : (
        <>
          <p className="muted">This list contains Services deployed from this browser.</p>
          <ErrorPanel message={error} />
          {loading && <p className="muted" aria-live="polite">Loading Services…</p>}
          {!loading && !error && items.length === 0 && (
            <p className="panel services-empty">
              No Services have been deployed from this browser with the connected account.
            </p>
          )}
          <div className="services-list">
            {items.map(({ remembered, service, unavailable }) => (
              <article className="panel service-list-item" key={remembered.serviceId}>
                <div>
                  <p className="eyebrow">Service ID</p>
                  <h2>{remembered.serviceId}</h2>
                </div>
                {unavailable ? (
                  <strong className="unavailable">Unavailable</strong>
                ) : (
                  <dl className="data-grid compact">
                    <Row label="Code hash" value={shortHash(service?.codeHash ?? remembered.codeHash)} />
                    <Row label="Finalized block" value={service?.finalizedBlock} />
                  </dl>
                )}
                <button
                  className="primary-button inline"
                  onClick={() => navigate(`/services/${remembered.serviceId}`)}
                >
                  Open Service
                </button>
              </article>
            ))}
          </div>
        </>
      )}
    </main>
  );
}

function Row({ label, value }: { label: string; value?: string }) {
  if (!value) return null;
  return <div><dt>{label}</dt><dd className="mono">{value}</dd></div>;
}

function shortHash(value?: string) {
  return value ? `${value.slice(0, 10)}…${value.slice(-8)}` : undefined;
}
