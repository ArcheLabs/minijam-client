import { useState } from "react";
import counterC from "../examples/counter.c?raw";
import counterCpp from "../examples/counter.cpp?raw";
import { CodeEditor } from "../components/editor";
import { ErrorPanel } from "../components/error-panel";
import { WalletButton } from "../components/wallet-button";
import { ConfirmAction } from "../components/confirm-action";
import { playgroundApi, type BuildArtifact } from "../api/playground";
import { requestFaucet, type FaucetEvent } from "../api/playground";
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
  const [faucet, setFaucet] = useState<FaucetEvent>();

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

  async function getMini() {
    try {
      const account = requireWritableAccount();
      const address = account.address;
      if (!address) throw new Error("Reconnect the wallet before requesting test MINI.");
      const message = `faucet:${address}:${Date.now()}`;
      const payload = `0x${[...new TextEncoder().encode(message)].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
      setFaucet({ status: "requesting" });
      const signature = await wallet.adapter.sign(account, payload);
      await requestFaucet({ address, signature, message }, async (event) => {
        setFaucet(event);
        if (event.status === "succeeded") {
          await playgroundApi.getFaucetBalance(address).catch(() => undefined);
        }
      });
    } catch (cause) {
      setFaucet({ status: "failed", error: errorMessage(cause) });
    }
  }

  const toolchain = artifact
    ? `${artifact.toolchain.clang} · ${artifact.toolchain.polkavm}`
    : "";

  return (
    <main className="workspace">
      <header className="topbar">
        <a className="brand" href="/" aria-label="MiniJAM Playground home">
          <span className="brand-mark">MJ</span>
          <span>MiniJAM <b>Playground</b></span>
        </a>
        <WalletButton />
      </header>

      <section className="hero">
        <div>
          <p className="eyebrow">Stage 0 · Refine → Vote → Accumulate</p>
          <h1>Write a service.<br />Watch consensus execute it.</h1>
        </div>
        <p className="hero-copy">Compile deterministic C or C++, deploy with your wallet, and send work through three independently verifying workers.</p>
      </section>

      <ErrorPanel message={error} />
      <section className="faucet-strip" aria-live="polite">
        <div>
          <span className="eyebrow">Test funds</span>
          <strong>Get test MINI</strong>
          <p>{faucetLabel(faucet)}</p>
        </div>
        <button className="secondary-button faucet-button" disabled={!wallet.account || faucet?.status === "requesting"} onClick={() => void getMini()}>
          {faucet?.status === "requesting" ? "Requesting..." : "Get test MINI"}
        </button>
      </section>
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
          <div className="work-form">
            <label>Service ID<input inputMode="numeric" value={serviceId} onChange={(event) => setServiceId(event.target.value)} /></label>
            <label>Counter increment<input inputMode="numeric" value={increment} onChange={(event) => setIncrement(event.target.value)} /></label>
            <button className="secondary-button" disabled={submitting} onClick={() => void prepareWork()}>Run work</button>
          </div>
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

function faucetLabel(event?: FaucetEvent) {
  if (!event) return "Uses the connected wallet account and Stage 0 faucet limits.";
  if (event.status === "requesting") return "Requesting wallet signature and sending the faucet request.";
  if (event.status === "broadcasting") return "Faucet transfer is being broadcast.";
  if (event.status === "broadcasted") return "Faucet transfer was broadcast.";
  if (event.status === "succeeded") return `Faucet transfer included${event.blockHash ? ` in ${event.blockHash}` : ""}.`;
  if (event.status === "limited") return "This address has already used its daily faucet request.";
  if (event.status === "over-cap") return "This address is already above the faucet balance cap.";
  if (event.status === "unavailable") return "Faucet is unavailable.";
  return event.error ?? "Faucet request failed.";
}

function bytesToBase64(bytes: Uint8Array) {
  return btoa(String.fromCharCode(...bytes));
}
