import { ChangeEvent, DragEvent, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  LocalTask,
  TaskStatus,
  createTaskFromFile,
  formatBytes,
  getOutputName
} from "./taskUtils";

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

const statusLabels: Record<TaskStatus, string> = {
  waiting: "Waiting",
  converting: "Converting",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled"
};

const outputNameExample = [
  getOutputName("report.pdf", []),
  getOutputName("report.pdf", ["report.pdf"]),
  getOutputName("report.pdf", ["report.pdf", "report (1).pdf"])
];

function App() {
  const [selfCheck, setSelfCheck] = useState<EngineSelfCheck>(fallbackSelfCheck);
  const [tasks, setTasks] = useState<LocalTask[]>([]);
  const [dragActive, setDragActive] = useState(false);
  const [folderMessage, setFolderMessage] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);

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

  const summary = useMemo(
    () =>
      tasks.reduce<Record<TaskStatus, number>>(
        (counts, task) => {
          counts[task.status] += 1;
          return counts;
        },
        {
          waiting: 0,
          converting: 0,
          completed: 0,
          failed: 0,
          cancelled: 0
        }
      ),
    [tasks]
  );

  const selectedTaskWithError = tasks.find((task) => task.errorLog);

  function addFiles(fileList: FileList | File[]) {
    const files = Array.from(fileList);
    if (files.length === 0) {
      return;
    }

    setTasks((currentTasks) => {
      const outputNames = currentTasks.map((task) =>
        task.outputPreview.replace(/^converted\//, "")
      );
      const createdTasks: LocalTask[] = [];

      for (const file of files) {
        const task = createTaskFromFile(
          file,
          [...outputNames, ...createdTasks.map((item) => item.outputPreview.replace(/^converted\//, ""))]
        );
        createdTasks.push(task);
      }

      return [...createdTasks, ...currentTasks];
    });
  }

  function handleInputChange(event: ChangeEvent<HTMLInputElement>) {
    if (event.currentTarget.files) {
      addFiles(event.currentTarget.files);
    }
    event.currentTarget.value = "";
  }

  function handleDrop(event: DragEvent<HTMLElement>) {
    event.preventDefault();
    setDragActive(false);
    addFiles(event.dataTransfer.files);
  }

  function updateTaskStatus(taskId: string, status: TaskStatus) {
    setTasks((currentTasks) =>
      currentTasks.map((task) => {
        if (task.id !== taskId) {
          return task;
        }

        return {
          ...task,
          status,
          errorLog:
            status === "failed"
              ? "Demo failure log only. No conversion process was started and no file contents were read."
              : task.errorLog
        };
      })
    );
  }

  function retryTask(taskId: string) {
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        task.id === taskId
          ? {
              ...task,
              status: "waiting",
              errorLog: ""
            }
          : task
      )
    );
  }

  function cancelTask(taskId: string) {
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        task.id === taskId && task.status !== "completed"
          ? {
              ...task,
              status: "cancelled"
            }
          : task
      )
    );
  }

  function removeTask(taskId: string) {
    setTasks((currentTasks) => currentTasks.filter((task) => task.id !== taskId));
  }

  function clearCompletedTasks() {
    setTasks((currentTasks) =>
      currentTasks.filter((task) => task.status !== "completed")
    );
  }

  return (
    <main className="app-shell">
      <header className="top-bar">
        <div>
          <p className="eyebrow">Desktop full edition</p>
          <h1>LocalConvert Desktop</h1>
          <p className="summary">
            Local file intake and task queue planning UI. No conversion is wired,
            no files are uploaded, and no file contents are read in this pass.
          </p>
        </div>
        <div className="platforms" aria-label="v1 platform targets">
          <span>Windows x64</span>
          <span>macOS Apple Silicon</span>
        </div>
      </header>

      <section className="dashboard-grid">
        <section className="panel intake-panel" aria-label="File intake">
          <div
            className={dragActive ? "drop-zone is-active" : "drop-zone"}
            onDragEnter={(event) => {
              event.preventDefault();
              setDragActive(true);
            }}
            onDragOver={(event) => {
              event.preventDefault();
              setDragActive(true);
            }}
            onDragLeave={(event) => {
              event.preventDefault();
              setDragActive(false);
            }}
            onDrop={handleDrop}
          >
            <p className="panel-kicker">Local intake</p>
            <h2>Drop files here</h2>
            <p>
              Tasks are created from local WebView file metadata only: name,
              size, extension, and a best-effort display path.
            </p>
            <div className="button-row">
              <button type="button" onClick={() => fileInputRef.current?.click()}>
                Select files
              </button>
              <button
                type="button"
                className="secondary-button"
                onClick={() =>
                  setFolderMessage(
                    "Folder selection will be enabled later through native Tauri permissions after the conversion pipeline exists."
                  )
                }
              >
                Select folder
              </button>
            </div>
            <input
              ref={fileInputRef}
              className="hidden-input"
              type="file"
              multiple
              onChange={handleInputChange}
            />
          </div>
          {folderMessage ? <p className="notice">{folderMessage}</p> : null}
        </section>

        <aside className="panel engine-panel" aria-label="Engine self-check">
          <p className="panel-kicker">Engine self-check</p>
          <h2>Conversion disabled</h2>
          <p className="panel-note">
            Platform: {selfCheck.platform}. Engines are intentionally not
            bundled yet, so this UI cannot perform conversion.
          </p>
          <dl className="engine-list">
            {selfCheck.engines.map((engine) => (
              <div className="engine-row" key={engine.name}>
                <dt>{engine.name}</dt>
                <dd>{engine.message}</dd>
              </div>
            ))}
          </dl>
        </aside>
      </section>

      <section className="summary-grid" aria-label="Task status summary">
        {(Object.keys(statusLabels) as TaskStatus[]).map((status) => (
          <article className="summary-card" key={status}>
            <span className={`status-chip status-${status}`}>
              {statusLabels[status]}
            </span>
            <strong>{summary[status]}</strong>
          </article>
        ))}
      </section>

      <section className="content-grid">
        <section className="panel queue-panel" aria-label="Task queue">
          <div className="section-heading">
            <div>
              <p className="panel-kicker">Task queue</p>
              <h2>Local tasks</h2>
            </div>
            <button
              type="button"
              className="secondary-button"
              onClick={clearCompletedTasks}
              disabled={summary.completed === 0}
            >
              Clear completed
            </button>
          </div>

          {tasks.length === 0 ? (
            <div className="empty-state">
              <h3>No tasks yet</h3>
              <p>Select or drop files to create waiting tasks.</p>
            </div>
          ) : (
            <div className="task-list">
              {tasks.map((task) => (
                <article className="task-card" key={task.id}>
                  <div className="task-main">
                    <span className={`status-chip status-${task.status}`}>
                      {statusLabels[task.status]}
                    </span>
                    <div>
                      <h3>{task.displayName}</h3>
                      <p>
                        {task.extension.toUpperCase()} · {formatBytes(task.size)}
                      </p>
                    </div>
                  </div>
                  <dl className="task-meta">
                    <div>
                      <dt>Source</dt>
                      <dd>{task.sourcePreview}</dd>
                    </div>
                    <div>
                      <dt>Output preview</dt>
                      <dd>{task.outputPreview}</dd>
                    </div>
                  </dl>
                  <div className="task-actions">
                    <button
                      type="button"
                      onClick={() => updateTaskStatus(task.id, "waiting")}
                      disabled={task.status === "waiting"}
                    >
                      Set waiting
                    </button>
                    <button
                      type="button"
                      onClick={() => updateTaskStatus(task.id, "converting")}
                      disabled={task.status === "completed"}
                    >
                      Simulate converting
                    </button>
                    <button
                      type="button"
                      onClick={() => updateTaskStatus(task.id, "completed")}
                      disabled={task.status === "cancelled"}
                    >
                      Complete
                    </button>
                    <button
                      type="button"
                      onClick={() => updateTaskStatus(task.id, "failed")}
                      disabled={task.status === "completed"}
                    >
                      Fail
                    </button>
                    <button
                      type="button"
                      onClick={() => cancelTask(task.id)}
                      disabled={task.status === "completed" || task.status === "cancelled"}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      onClick={() => retryTask(task.id)}
                      disabled={task.status !== "failed"}
                    >
                      Retry
                    </button>
                    <button
                      type="button"
                      className="ghost-button"
                      onClick={() => removeTask(task.id)}
                    >
                      Remove
                    </button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>

        <aside className="side-stack">
          <section className="panel rules-panel">
            <p className="panel-kicker">Output rule</p>
            <h2>converted folder next to source</h2>
            <p>
              Output previews follow the README rule and never overwrite source
              files. No folder is created in this UI-only pass.
            </p>
            <code>{outputNameExample.join("  ->  ")}</code>
          </section>

          <section className="panel log-panel">
            <p className="panel-kicker">Error log</p>
            <h2>Placeholder</h2>
            <pre>
              {selectedTaskWithError?.errorLog ||
                "No error log yet. Failed demo tasks will show a local placeholder here."}
            </pre>
          </section>

          <section className="panel privacy-panel">
            <p className="panel-kicker">Privacy</p>
            <h2>Local-only by design</h2>
            <ul>
              <li>No upload.</li>
              <li>No network call.</li>
              <li>No file contents read.</li>
              <li>No conversion process spawned.</li>
            </ul>
          </section>
        </aside>
      </section>
    </main>
  );
}

export default App;
