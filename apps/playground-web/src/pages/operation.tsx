import { navigate } from "../app";

export function OperationPage({ operationId }: { operationId: string }) {
  return (
    <main className="page narrow">
      <button className="text-button" onClick={() => navigate("/")}>← Playground</button>
      <p className="eyebrow">Finalized operation</p>
      <h1>Operation</h1>
      <div className="status-card">
        <span className="status-dot" />
        <div>
          <strong>Preparing</strong>
          <p>Reading operation state from the Playground API.</p>
        </div>
      </div>
      <dl className="data-grid">
        <div><dt>Operation ID</dt><dd className="mono">{operationId}</dd></div>
      </dl>
    </main>
  );
}
