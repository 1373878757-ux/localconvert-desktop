import React from "react";
import ReactDOM from "react-dom/client";
import "./splash.css";

function SplashScreen() {
  return (
    <main className="splash-shell" aria-label="LocalConvert Desktop 启动">
      <section className="splash-panel">
        <div className="splash-brand">
          <h1>LocalConvert Desktop</h1>
          <p>by 田宸宇</p>
          <p className="splash-slogan">让可能发生在这儿。</p>
        </div>

        <div className="splash-status" aria-label="启动状态">
          <p>正在准备本地转换工具...</p>
          <p>正在检查内置 qpdf 引擎...</p>
          <p>不上传，文件始终留在本机。</p>
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
