import { useState } from "react";
import counterC from "../examples/counter.c?raw";
import counterCpp from "../examples/counter.cpp?raw";
import { CodeEditor } from "../components/editor";
import { ErrorPanel } from "../components/error-panel";
import { AppHeader } from "../components/app-header";
import { ConfirmAction } from "../components/confirm-action";
import { playgroundApi, type BuildArtifact } from "../api/playground";
import { useWallet } from "../wallet/context";
import { signAction } from "../actions/signed-action";
import { navigate } from "../app";
import { errorMessage } from "../lib/errors";
import { STAGE0_ACCUMULATE_GAS_LIMIT } from "../lib/protocol";
import { SR25519_ONLY_ERROR } from "../wallet/adapter";

type Language = "c" | "cpp";
type BuildState = "IDLE" | "BUILDING" | "SUCCEEDED" | "FAILED";
type PendingAction =
  | { kind: "deploy"; params: Record<string, unknown> }
  | { kind: "work"; params: Record<string, unknown> };

export function PlaygroundPage() {
  const wallet = useWallet();
  const [language, setLanguage] = useState<Language>("c");
  const [source, setSource] = useState(counterC);
  const [optimization, setOptimization] = useState("O0");
  const [buildState, setBuildState] = useState<BuildState>("IDLE");
  const [artifact, setArtifact] = useState<BuildArtifact>();
  const [diagnostics, setDiagnostics] = useState<string[]>([]);
  const [error, setError] = useState<string>();
  const [serviceId, setServiceId] = useState("");
  const [increment, setIncrement] = useState("1");
  const [pending, setPending] = useState<PendingAction>();
  const [submitting, setSubmitting] = useState(false);

  function selectExample(next: Language) {
    setLanguage(next);
    setSource(next === "c" ? counterC : counterCpp);
    setBuildState("IDLE");
    setArtifact(undefined);
  }

  async function build() {
    setBuildState("BUILDING");
    setError(undefined);
    setDiagnostics([]);
    try {
      const result = await playgroundApi.build({ language, source, optimization });
      setDiagnostics(result.diagnostics);
      if (!result.success || !result.blobBase64 || !result.codeHash || result.codeLength == null) {
        setBuildState("FAILED");
        return;
      }
      setArtifact(result);
      setBuildState("SUCCEEDED");
    } catch (cause) {
      setBuildState("FAILED");
      setError(errorMessage(cause));
    }
  }

  function requireWritableAccount() {
    if (!wallet.account) throw new Error("Connect a wallet before submitting an operation.");
    if (wallet.account.type !== "sr25519") throw new Error(SR25519_ONLY_ERROR);
    return wallet.account;
  }

  function prepareDeploy() {
    try {
      requireWritableAccount();
      if (!artifact?.blobBase64 || !artifact.codeHash || artifact.codeLength == null) {
        throw new Error("Build the service successfully before deploying.");
      }
      setPending({
        kind: "deploy",
        params: {
          blobBase64: artifact.blobBase64,
          codeHash: artifact.codeHash,
          minItemGas: STAGE0_ACCUMULATE_GAS_LIMIT,
          minMemoGas: 1
        }
      });
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function prepareWork() {
    try {
      const account = requireWritableAccount();
      const id = Number(serviceId);
      if (!Number.isSafeInteger(id) || id < 0) throw new Error("Enter a valid Service ID.");
      const service = await playgroundApi.getService(id);
      if (service.controller.toLowerCase() !== account.accountId.toLowerCase()) {
        throw new Error("The connected account is not the finalized Service Controller.");
      }
      const value = BigInt(increment);
      const bytes = new Uint8Array(8);
      new DataView(bytes.buffer).setBigInt64(0, value, true);
      setPending({
        kind: "work",
        params: {
          serviceId: id,
          serviceCodeHash: service.codeHash,
          payloadBase64: bytesToBase64(bytes),
          extrinsicsBase64: []
        }
      });
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function confirm() {
    if (!pending) return;
    setSubmitting(true);
    setError(undefined);
    try {
      const account = requireWritableAccount();
      const authorization = await signAction(wallet.adapter, account, pending.kind === "deploy" ? "create_service" : "work", pending.params);
      const operation = pending.kind === "deploy"
        ? await playgroundApi.createService({ authorization, ...pending.params })
        : await playgroundApi.submitWork({ authorization, ...pending.params });
      setPending(undefined);
      navigate(`/operations/${operation.operationId}`);
    } catch (cause) {
      setError(errorMessage(cause));
      setPending(undefined);
    } finally {
      setSubmitting(false);
    }
  }

  const toolchain = artifact
    ? `${artifact.toolchain.clang} · ${artifact.toolchain.polkavm}`
    : "";

  return (
    <main className="workspace">
      <AppHeader />

      <ErrorPanel message={error} />
      <section className="builder-grid">
        <div className="code-panel">
          <div className="toolbar">
            <div className="segmented" aria-label="Example language">
              <button className={language === "c" ? "active" : ""} onClick={() => selectExample("c")}>Counter C</button>
              <button className={language === "cpp" ? "active" : ""} onClick={() => selectExample("cpp")}>Counter C++</button>
            </div>
            <button className="text-button" onClick={() => selectExample(language)}>Reset example</button>
          </div>
          <CodeEditor language={language} value={source} onChange={(next) => {
            setSource(next);
            setBuildState("IDLE");
          }} />
        </div>

        <aside className="control-panel">
          <div className="step-label"><span>01</span> Build</div>
          <label>Optimization
            <select value={optimization} onChange={(event) => setOptimization(event.target.value)}>
              <option>O0</option><option>Os</option>
            </select>
          </label>
          <button className="primary-button" disabled={buildState === "BUILDING"} onClick={() => void build()}>
            {buildState === "BUILDING" ? "Building…" : "Build service"}
          </button>
          <div aria-live="polite">
            {buildState === "SUCCEEDED" && artifact ? (
              <dl className="artifact">
                <div><dt>Status</dt><dd className="success">SUCCEEDED</dd></div>
                <div><dt>Code hash</dt><dd className="mono">{artifact.codeHash}</dd></div>
                <div><dt>Blob size</dt><dd>{artifact.codeLength} bytes</dd></div>
                <div><dt>Toolchain</dt><dd>{toolchain}</dd></div>
                <div><dt>Build</dt><dd>{language.toUpperCase()} · {optimization}</dd></div>
              </dl>
            ) : (
              <div className="empty-state">
                <span className="empty-icon">⌁</span>
                <strong>{buildState === "FAILED" ? "Build failed" : "Ready to compile"}</strong>
                <p>{buildState === "FAILED" ? "Review the compiler diagnostics below." : "Artifact details and diagnostics will appear here."}</p>
              </div>
            )}
            {diagnostics.length > 0 && <pre className="diagnostics">{diagnostics.join("\n")}</pre>}
          </div>
          <div className="divider" />
          <div className="step-label"><span>02</span> Deploy & run</div>
          <button className="secondary-button" disabled={buildState !== "SUCCEEDED" || submitting} onClick={prepareDeploy}>Deploy service</button>
          {/* <div className="work-form">
            <label>Service ID<input inputMode="numeric" value={serviceId} onChange={(event) => setServiceId(event.target.value)} /></label>
            <label>Counter increment<input inputMode="numeric" value={increment} onChange={(event) => setIncrement(event.target.value)} /></label>
            <button className="secondary-button" disabled={submitting} onClick={() => void prepareWork()}>Run work</button>
          </div> */}
        </aside>
      </section>
      {pending && wallet.account && (
        <ConfirmAction
          title={pending.kind === "deploy" ? "Create Service" : "Submit Work"}
          details={pending.kind === "deploy"
            ? [
                { label: "Controller", value: wallet.account.accountId },
                { label: "Code Hash", value: String(pending.params.codeHash) },
                { label: "Code Length", value: `${artifact?.codeLength ?? 0} bytes` }
              ]
            : [
                { label: "Controller", value: wallet.account.accountId },
                { label: "Service ID", value: String(pending.params.serviceId) },
                { label: "Code Hash", value: String(pending.params.serviceCodeHash) }
              ]}
          busy={submitting}
          onClose={() => setPending(undefined)}
          onConfirm={() => void confirm()}
        />
      )}
    </main>
  );
}

function bytesToBase64(bytes: Uint8Array) {
  return btoa(String.fromCharCode(...bytes));
}
