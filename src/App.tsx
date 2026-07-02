import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type EngineStatus = {
  name: string;
  status: "not-installed" | "available" | "error";
  requiredForV1: boolean;
  message: string;
};

type EngineSelfCheck = {
  platform: string;
  fullEdition: boolean;
  conversionEnabled: boolean;
  engines: EngineStatus[];
};

const fallbackSelfCheck: EngineSelfCheck = {
  platform: "desktop scaffold",
  fullEdition: true,
  conversionEnabled: false,
  engines: [
    {
      name: "LibreOffice headless",
      status: "not-installed",
      requiredForV1: true,
      message: "Not bundled yet."
    },
    {
      name: "qpdf",
      status: "not-installed",
      requiredForV1: true,
      message: "Not bundled yet."
    },
    {
      name: "PDFium",
      status: "not-installed",
      requiredForV1: true,
      message: "Not bundled yet."
    },
    {
      name: "image-engine",
      status: "not-installed",
      requiredForV1: true,
      message: "Not bundled yet."
    }
  ]
};

const principles = [
  "Pure local conversion",
  "No upload",
  "No server dependency",
  "Original files are preserved"
];

const platforms = ["Windows x64", "macOS Apple Silicon"];

function App() {
  const [selfCheck, setSelfCheck] = useState<EngineSelfCheck>(fallbackSelfCheck);

  useEffect(() => {
    async function loadSelfCheck() {
      try {
        setSelfCheck(await invoke<EngineSelfCheck>("engine_self_check"));
      } catch {
        setSelfCheck(fallbackSelfCheck);
      }
    }

    void loadSelfCheck();
  }, []);

  return (
    <main className="app-shell">
      <section className="hero">
        <div className="hero-copy">
          <p className="eyebrow">Desktop full edition</p>
          <h1>LocalConvert Desktop</h1>
          <p className="summary">
            A pure local desktop conversion workspace for office files, images,
            and PDFs. The current scaffold proves the app shell and engine
            diagnostics before real conversion is wired.
          </p>
          <div className="hero-actions" aria-label="v1 platform targets">
            {platforms.map((platform) => (
              <span className="platform-pill" key={platform}>
                {platform}
              </span>
            ))}
          </div>
        </div>

        <aside className="status-panel" aria-label="Engine self-check status">
          <div>
            <p className="panel-kicker">Engine self-check</p>
            <h2>Bundled engines planned</h2>
          </div>
          <p className="panel-note">
            Conversion is disabled until the required desktop engines are
            bundled and verified.
          </p>
          <dl className="status-list">
            {selfCheck.engines.map((engine) => (
              <div className="status-row" key={engine.name}>
                <dt>{engine.name}</dt>
                <dd>{engine.message}</dd>
              </div>
            ))}
          </dl>
        </aside>
      </section>

      <section className="principles" aria-label="Product principles">
        {principles.map((principle) => (
          <article className="principle-card" key={principle}>
            <h2>{principle}</h2>
            <p>
              {principle === "Bundled engines planned"
                ? "Runtime users should not install conversion tools manually."
                : "Defined in README.md as a non-negotiable project rule."}
            </p>
          </article>
        ))}
        <article className="principle-card">
          <h2>Bundled engines planned</h2>
          <p>LibreOffice, qpdf, PDFium, and image-engine are not bundled yet.</p>
        </article>
      </section>

      <section className="scaffold-note">
        <h2>Scaffold status</h2>
        <p>
          This app does not perform real conversion yet. The frontend calls a
          Rust backend command stub, and the command reports all required
          engines as not installed or not bundled yet.
        </p>
        <p className="mono-line">
          Platform target: {selfCheck.platform} | Conversion enabled:{" "}
          {selfCheck.conversionEnabled ? "yes" : "no"}
        </p>
      </section>
    </main>
  );
}

export default App;
