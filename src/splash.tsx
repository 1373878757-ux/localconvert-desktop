import React from "react";
import ReactDOM from "react-dom/client";
import "./splash.css";

function SplashScreen() {
  return (
    <main className="splash-shell" aria-label="LocalConvert Desktop startup">
      <section className="splash-panel">
        <div className="splash-brand">
          <h1>LocalConvert Desktop</h1>
          <p>by 田宸宇</p>
          <p className="splash-slogan">让可能，发生在这儿。</p>
        </div>

        <div className="splash-status" aria-label="Startup status">
          <p>Preparing local conversion tools...</p>
          <p>Checking bundled qpdf engine...</p>
          <p>No upload. Files stay on this computer.</p>
        </div>
      </section>
    </main>
  );
}

ReactDOM.createRoot(
  document.getElementById("splash-root") as HTMLElement
).render(
  <React.StrictMode>
    <SplashScreen />
  </React.StrictMode>
);
