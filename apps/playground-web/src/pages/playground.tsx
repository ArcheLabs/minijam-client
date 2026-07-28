import { useState } from "react";
import counterC from "../examples/counter.c?raw";
import counterCpp from "../examples/counter.cpp?raw";
import { CodeEditor } from "../components/editor";

type Language = "c" | "cpp";

export function PlaygroundPage() {
  const [language, setLanguage] = useState<Language>("c");
  const [source, setSource] = useState(counterC);
  const [optimization, setOptimization] = useState("O0");

  function selectExample(next: Language) {
    setLanguage(next);
    setSource(next === "c" ? counterC : counterCpp);
  }

  return (
    <main className="workspace">
      <header className="topbar">
        <a className="brand" href="/" aria-label="MiniJAM Playground home">
          <span className="brand-mark">MJ</span>
          <span>MiniJAM <b>Playground</b></span>
        </a>
        <button className="wallet-button">Connect wallet</button>
      </header>

      <section className="hero">
        <div>
          <p className="eyebrow">Stage 0 · Refine → Vote → Accumulate</p>
          <h1>Write a service.<br />Watch consensus execute it.</h1>
        </div>
        <p className="hero-copy">
          Compile deterministic C or C++, deploy with your wallet, and send work
          through three independently verifying workers.
        </p>
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
          <CodeEditor language={language} value={source} onChange={setSource} />
        </div>

        <aside className="control-panel">
          <div className="step-label"><span>01</span> Build</div>
          <label>
            Optimization
            <select value={optimization} onChange={(event) => setOptimization(event.target.value)}>
              <option>O0</option>
              <option>O1</option>
              <option>O2</option>
              <option>Os</option>
            </select>
          </label>
          <button className="primary-button">Build service</button>
          <div className="empty-state" aria-live="polite">
            <span className="empty-icon">⌁</span>
            <strong>Ready to compile</strong>
            <p>Your artifact details and compiler diagnostics will appear here.</p>
          </div>
          <div className="divider" />
          <div className="step-label muted-step"><span>02</span> Deploy & run</div>
          <p className="muted">Build the service and connect an sr25519 wallet to continue.</p>
        </aside>
      </section>
    </main>
  );
}
