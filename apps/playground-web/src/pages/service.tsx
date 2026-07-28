import { navigate } from "../app";

export function ServicePage({ serviceId }: { serviceId: number }) {
  return (
    <main className="page narrow">
      <button className="text-button" onClick={() => navigate("/")}>← Playground</button>
      <p className="eyebrow">Finalized service</p>
      <h1>Service {serviceId}</h1>
      <section className="panel">
        <h2>On-chain details</h2>
        <p className="muted">Loading finalized Controller, code and preimage state.</p>
      </section>
    </main>
  );
}
