import { useEffect, useState } from "react";
import { navigate } from "../app";
import { playgroundApi, type Operation } from "../api/playground";
import { ErrorPanel } from "../components/error-panel";
import { errorMessage } from "../lib/errors";

const statusLabel: Record<Operation["status"], string> = {
  prepared: "Preparing",
  submitted: "Submitted",
  waiting_receipt: "Waiting for finality",
  submitting_preimage: "Publishing code",
  waiting_preimage: "Waiting for finalized code",
  tracking_work: "Processing work",
  succeeded: "Completed",
  failed: "Failed"
};

export function OperationPage({ operationId }: { operationId: string }) {
  const [operation, setOperation] = useState<Operation>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    let stopped = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const next = await playgroundApi.getOperation(operationId);
        if (stopped) return;
        setOperation(next);
        setError(undefined);
        if (next.status !== "succeeded" && next.status !== "failed") {
          timer = window.setTimeout(poll, document.hidden ? 5000 : 1500);
        }
      } catch (cause) {
        if (!stopped) {
          setError(errorMessage(cause));
          timer = window.setTimeout(poll, 3000);
        }
      }
    };
    void poll();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  }, [operationId]);

  const serviceId = operation?.result?.serviceId;
  return (
    <main className="page narrow">
      <button className="text-button" onClick={() => navigate("/")}>← Playground</button>
      <p className="eyebrow">Finalized operation</p>
      <h1>{operation ? capitalize(operation.kind) : "Operation"}</h1>
      <ErrorPanel message={error ?? operation?.error} />
      <div className={`status-card ${operation?.status === "failed" ? "failed" : ""}`} aria-live="polite">
        <span className="status-dot" />
        <div>
          <strong>{operation ? statusLabel[operation.status] : "Loading…"}</strong>
          <p>{operation?.status === "tracking_work"
            ? "Waiting for Worker candidate, votes, and accumulation."
            : "This status comes directly from the Playground API."}</p>
        </div>
      </div>
      <dl className="data-grid">
        <Row label="Operation ID" value={operationId} />
        <Row label="Type" value={operation?.kind} />
        <Row label="Package hash" value={String(operation?.request.packageHash ?? "")} />
        <Row label="Bundle CID" value={String(operation?.request.bundleCid ?? "")} />
        <Row label="Extrinsic hash" value={operation?.extrinsicHash} />
        <Row label="Service ID" value={serviceId?.toString()} />
        <Row label="Work ID" value={operation?.result?.workId?.toString()} />
        <Row label="Execution receipt" value={operation?.result?.executionReceipt} />
      </dl>
      {serviceId != null && operation?.status === "succeeded" && (
        <button className="primary-button inline" onClick={() => navigate(`/services/${serviceId}`)}>
          Open Service {serviceId}
        </button>
      )}
      {operation?.kind === "work" && operation.status === "succeeded" && (
        <button className="primary-button inline" onClick={() => navigate(`/services/${String(operation.request.serviceId)}`)}>
          View finalized Service state
        </button>
      )}
    </main>
  );
}

function Row({ label, value }: { label: string; value?: string }) {
  if (!value) return null;
  return <div><dt>{label}</dt><dd className="mono">{value}</dd></div>;
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
