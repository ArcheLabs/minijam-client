import { useEffect, useState } from "react";
import counterC from "../examples/counter.c?raw";
import { navigate } from "../app";
import {
  playgroundApi,
  type BuildArtifact,
  type ServiceView,
  type StorageView
} from "../api/playground";
import { ErrorPanel } from "../components/error-panel";
import { AppHeader } from "../components/app-header";
import { CodeEditor } from "../components/editor";
import { ConfirmAction } from "../components/confirm-action";
import { useWallet } from "../wallet/context";
import { signAction } from "../actions/signed-action";
import { errorMessage } from "../lib/errors";
import { STAGE0_ACCUMULATE_GAS_LIMIT } from "../lib/protocol";

type Pending =
  | { kind: "upgrade"; params: Record<string, unknown> }
  | { kind: "work"; params: Record<string, unknown> };

export function ServicePage({ serviceId }: { serviceId: number }) {
  const wallet = useWallet();
  const [service, setService] = useState<ServiceView>();
  const [source, setSource] = useState(counterC);
  const [artifact, setArtifact] = useState<BuildArtifact>();
  const [building, setBuilding] = useState(false);
  const [payloadMode, setPayloadMode] = useState<"increment" | "utf8" | "hex">("increment");
  const [payload, setPayload] = useState("1");
  const [extrinsics, setExtrinsics] = useState("");
  const [storageKey, setStorageKey] = useState("counter");
  const [storageResult, setStorageResult] = useState<StorageView>();
  const [readingStorage, setReadingStorage] = useState(false);
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState<Pending>();
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    playgroundApi.getService(serviceId).then(setService).catch((cause) => setError(errorMessage(cause)));
  }, [serviceId]);

  const isController = Boolean(
    service && wallet.account &&
    service.controller.toLowerCase() === wallet.account.accountId.toLowerCase()
  );

  async function buildUpgrade() {
    setBuilding(true);
    setError(undefined);
    try {
      const result = await playgroundApi.build({ language: "c", source, optimization: "O0" });
      if (!result.success || !result.blobBase64 || !result.codeHash) {
        throw new Error(result.diagnostics.join("\n") || "Compilation failed.");
      }
      setArtifact(result);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBuilding(false);
    }
  }

  function prepareUpgrade() {
    if (!isController) return setError("The connected account is not the finalized Service Controller.");
    if (!artifact?.blobBase64 || !artifact.codeHash) return setError("Build the upgraded source first.");
    setPending({
      kind: "upgrade",
      params: {
        serviceId,
        blobBase64: artifact.blobBase64,
        codeHash: artifact.codeHash,
        minItemGas: STAGE0_ACCUMULATE_GAS_LIMIT,
        minMemoGas: 1
      }
    });
  }

  function prepareWork() {
    if (!service) return setError("Service details are still loading.");
    try {
      setPending({
        kind: "work",
        params: {
          serviceId,
          serviceCodeHash: service.codeHash,
          payloadBase64: encodePayload(payloadMode, payload),
          extrinsicsBase64: extrinsics.split("\n").map((line) => line.trim()).filter(Boolean).map(hexToBase64)
        }
      });
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function confirm() {
    if (!pending || !wallet.account) return;
    setSubmitting(true);
    try {
      const authorization = await signAction(
        wallet.adapter,
        wallet.account,
        pending.kind === "upgrade" ? "upgrade_service" : "work",
        pending.params
      );
      const operation = pending.kind === "upgrade"
        ? await playgroundApi.upgradeService(serviceId, { authorization, ...pending.params })
        : await playgroundApi.submitWork({ authorization, ...pending.params });
      navigate(`/operations/${operation.operationId}`);
    } catch (cause) {
      setError(errorMessage(cause));
      setPending(undefined);
    } finally {
      setSubmitting(false);
    }
  }

  async function readStorage() {
    setReadingStorage(true);
    setError(undefined);
    try {
      const key = `0x${[...new TextEncoder().encode(storageKey)].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
      const result = await playgroundApi.getServiceStorage(serviceId, key);
      setStorageResult(result);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setReadingStorage(false);
    }
  }

  return (
    <main className="page service-page">
      <AppHeader />
      <button className="text-button page-back" onClick={() => navigate("/")}>← Playground</button>
      <p className="eyebrow">Finalized service</p>
      <h1>Service {serviceId}</h1>
      <ErrorPanel message={error} />
      {!isController && wallet.account && <div className="permission-note" role="status">Upgrade remains Controller-only. Work is open to any signed Experience user.</div>}
      <section className="service-grid">
        <div className="panel">
          <h2>On-chain details</h2>
          <dl className="data-grid compact">
            <Row label="Controller" value={service?.controller} />
            <Row label="Code hash" value={service?.codeHash} />
            <Row label="Code length" value={service ? `${service.codeLength} bytes` : undefined} />
            <Row label="Preimage" value={service ? (service.preimageReady ? "Available" : "Pending") : undefined} />
            <Row label="Finalized block" value={service?.finalizedBlock} />
          </dl>
          <h3>Observe storage</h3>
          <label>UTF-8 key<input value={storageKey} onChange={(event) => {
            setStorageKey(event.target.value);
            setStorageResult(undefined);
          }} /></label>
          <button className="secondary-button" disabled={readingStorage} onClick={() => void readStorage()}>
            {readingStorage ? "Reading…" : "Read finalized value"}
          </button>
          {storageResult && (
            <div className="storage-result">
              {storageResult.value !== null ? (
                <>
                  <span className="mono">{storageResult.value}</span>
                  <b>{decodeCounter(storageResult.value)}</b>
                </>
              ) : (
                <b>No finalized value exists for this key.</b>
              )}
              <small className="mono">Finalized block: {storageResult.finalizedBlock}</small>
            </div>
          )}
        </div>
        <div className="panel action-stack">
          <h2>Submit Work</h2>
          <label>Payload encoding<select value={payloadMode} onChange={(event) => setPayloadMode(event.target.value as typeof payloadMode)}>
            <option value="increment">Counter increment</option><option value="utf8">UTF-8</option><option value="hex">Raw hex</option>
          </select></label>
          <label>Payload<input value={payload} onChange={(event) => setPayload(event.target.value)} /></label>
          <label>Optional extrinsics <small>one hex value per line</small><textarea rows={3} value={extrinsics} onChange={(event) => setExtrinsics(event.target.value)} /></label>
          <button className="primary-button" disabled={!service || submitting} onClick={prepareWork}>Run Work</button>
        </div>
      </section>
      <section className="upgrade-section">
        <div><p className="eyebrow">Upgrade</p><h2>Build new service code</h2></div>
        <CodeEditor language="c" value={source} onChange={setSource} />
        <div className="upgrade-actions">
          <button className="secondary-button" disabled={building} onClick={() => void buildUpgrade()}>{building ? "Building…" : "Build upgrade"}</button>
          <span className="mono">{artifact?.codeHash ?? "No upgraded artifact yet"}</span>
          <button className="primary-button inline" disabled={!artifact || !isController || submitting} onClick={prepareUpgrade}>Upgrade Service</button>
        </div>
      </section>
      {pending && wallet.account && <ConfirmAction
        title={pending.kind === "upgrade" ? "Upgrade Service" : "Submit Work"}
        details={[
          { label: "Controller", value: wallet.account.accountId },
          { label: "Service ID", value: String(serviceId) },
          { label: "Code Hash", value: String(pending.params.codeHash ?? pending.params.serviceCodeHash) }
        ]}
        busy={submitting}
        onClose={() => setPending(undefined)}
        onConfirm={() => void confirm()}
      />}
    </main>
  );
}

function Row({ label, value }: { label: string; value?: string }) {
  if (!value) return null;
  return <div><dt>{label}</dt><dd className="mono">{value}</dd></div>;
}

function encodePayload(mode: "increment" | "utf8" | "hex", value: string) {
  if (mode === "increment") {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setBigInt64(0, BigInt(value), true);
    return bytesToBase64(bytes);
  }
  if (mode === "utf8") return bytesToBase64(new TextEncoder().encode(value));
  return hexToBase64(value);
}

function hexToBase64(value: string) {
  const clean = value.replace(/^0x/, "");
  if (clean.length % 2 || !/^[0-9a-f]*$/i.test(clean)) throw new Error("Raw hex input is invalid.");
  return bytesToBase64(Uint8Array.from(clean.match(/.{2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? []));
}

function bytesToBase64(bytes: Uint8Array) {
  return btoa(String.fromCharCode(...bytes));
}

function decodeCounter(hex: string) {
  const bytes = Uint8Array.from(hex.slice(2).match(/.{2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? []);
  return bytes.length === 8 ? `Counter: ${new DataView(bytes.buffer).getBigInt64(0, true)}` : "Raw value";
}
