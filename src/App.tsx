import {
  ChangeEvent,
  DragEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
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

const toolCategories = [
  "Documents to PDF",
  "Image Conversion",
  "Image Compression",
  "Images to PDF",
  "PDF Tools",
  "Batch Queue"
];

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
  const [activeTool, setActiveTool] = useState("Batch Queue");
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
        const task = createTaskFromFile(file, [
          ...outputNames,
          ...createdTasks.map((item) =>
            item.outputPreview.replace(/^converted\//, "")
          )
        ]);
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
    setTasks((currentTasks) =>
      currentTasks.filter((task) => task.id !== taskId)
    );
  }

  function clearCompletedTasks() {
    setTasks((currentTasks) =>
      currentTasks.filter((task) => task.status !== "completed")
    );
  }

  return (
    <main className="app-shell">
      <header className="top-bar">
        <div className="app-title">
          <h1>LocalConvert Desktop</h1>
          <p>Files stay on this computer.</p>
        </div>
        <div className="privacy-status" aria-label="Local privacy status">
          <span>No upload</span>
          <span>Local queue</span>
          <span>Conversion disabled</span>
        </div>
      </header>

      <div className="workbench">
        <aside className="sidebar" aria-label="Tool categories">
          <div className="sidebar-section-title">Tools</div>
          <nav className="tool-nav">
            {toolCategories.map((tool) => (
              <button
                type="button"
                className={activeTool === tool ? "tool-item is-active" : "tool-item"}
                key={tool}
                onClick={() => setActiveTool(tool)}
              >
                {tool}
              </button>
            ))}
          </nav>
          <div className="sidebar-note">
            <strong>Desktop full edition</strong>
            <span>Windows x64 and macOS Apple Silicon are the v1 targets.</span>
          </div>
        </aside>

        <section className="main-pane" aria-label="File queue workbench">
          <div className="pane-header">
            <div>
              <p className="section-kicker">{activeTool}</p>
              <h2>File intake</h2>
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

          <section
            className={dragActive ? "drop-zone is-active" : "drop-zone"}
            aria-label="Drop files"
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
            <div>
              <h3>Drag files here or select files.</h3>
              <p>Name, size, extension, and display path only.</p>
            </div>
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
          </section>

          {folderMessage ? <p className="inline-note">{folderMessage}</p> : null}

          <section className="status-summary" aria-label="Task status summary">
            {(Object.keys(statusLabels) as TaskStatus[]).map((status) => (
              <article className="summary-cell" key={status}>
                <span>{statusLabels[status]}</span>
                <strong>{summary[status]}</strong>
              </article>
            ))}
          </section>

          <section className="queue-panel" aria-label="Task queue">
            <div className="queue-heading">
              <h2>Task queue</h2>
              <span>{tasks.length} total</span>
            </div>

            {tasks.length === 0 ? (
              <div className="empty-state">
                <h3>No tasks yet</h3>
                <p>Add files to create waiting tasks.</p>
              </div>
            ) : (
              <div className="task-table" role="table" aria-label="Local tasks">
                <div className="task-table-head" role="row">
                  <span>File</span>
                  <span>Status</span>
                  <span>Output preview</span>
                  <span>Actions</span>
                </div>
                {tasks.map((task) => (
                  <article className="task-row" role="row" key={task.id}>
                    <div className="file-cell">
                      <strong>{task.displayName}</strong>
                      <span>
                        {task.extension.toUpperCase()} · {formatBytes(task.size)}
                      </span>
                    </div>
                    <span className={`status-chip status-${task.status}`}>
                      {statusLabels[task.status]}
                    </span>
                    <div className="output-cell">
                      <span>{task.outputPreview}</span>
                      <small>{task.sourcePreview}</small>
                    </div>
                    <div className="task-actions">
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => updateTaskStatus(task.id, "waiting")}
                        disabled={task.status === "waiting"}
                      >
                        Wait
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => updateTaskStatus(task.id, "converting")}
                        disabled={task.status === "completed"}
                      >
                        Convert
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => updateTaskStatus(task.id, "completed")}
                        disabled={task.status === "cancelled"}
                      >
                        Done
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => updateTaskStatus(task.id, "failed")}
                        disabled={task.status === "completed"}
                      >
                        Fail
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => cancelTask(task.id)}
                        disabled={
                          task.status === "completed" ||
                          task.status === "cancelled"
                        }
                      >
                        Cancel
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => retryTask(task.id)}
                        disabled={task.status !== "failed"}
                      >
                        Retry
                      </button>
                      <button
                        type="button"
                        className="small-button ghost-button"
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
        </section>

        <aside className="inspector" aria-label="Inspector">
          <section className="inspector-card">
            <h2>Output rule</h2>
            <p>Use a converted folder next to the source file.</p>
            <pre>{outputNameExample.join("\n")}</pre>
          </section>

          <section className="inspector-card">
            <h2>Engine status</h2>
            <p>Conversion engines are not bundled yet.</p>
            <dl className="engine-list">
              {selfCheck.engines.map((engine) => (
                <div className="engine-row" key={engine.name}>
                  <dt>{engine.name}</dt>
                  <dd>{engine.message}</dd>
                </div>
              ))}
            </dl>
          </section>

          <section className="inspector-card">
            <h2>Error log</h2>
            <pre className="log-box">
              {selectedTaskWithError?.errorLog ||
                "No error log. Failed demo tasks appear here."}
            </pre>
          </section>

          <section className="inspector-card">
            <h2>Local privacy</h2>
            <ul>
              <li>No upload.</li>
              <li>No file contents read.</li>
              <li>No conversion process started.</li>
            </ul>
          </section>
        </aside>
      </div>
    </main>
  );
}

export default App;
