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
  getBaseName,
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

type OutputPathPlan = {
  sourceDisplayName: string;
  targetExtension: string;
  plannedConvertedFolderPath: string;
  plannedOutputFilename: string;
  plannedOutputPath: string;
  collisionStrategyExplanation: string;
};

type QpdfMergeResult = {
  success: boolean;
  operation: "merge";
  outputPath: string;
  outputBytes: number;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type QpdfSplitResult = {
  success: boolean;
  operation: "split";
  sourcePath: string;
  outputDirectory: string;
  outputPaths: string[];
  outputBytes: number;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type QpdfExtractResult = {
  success: boolean;
  operation: "extract";
  sourcePath: string;
  outputPath: string;
  outputBytes: number;
  pages: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type QpdfRotateResult = {
  success: boolean;
  operation: "rotate";
  sourcePath: string;
  outputPath: string;
  outputBytes: number;
  degrees: string;
  pages: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
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
  const [extractPageRanges, setExtractPageRanges] = useState<Record<string, string>>({});
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
  const qpdfEngine = selfCheck.engines.find((engine) => engine.name === "qpdf");
  const qpdfAvailable = qpdfEngine?.status === "available";
  const realLocalPdfTasks = tasks.filter(
    (task) =>
      task.extension === "pdf" &&
      Boolean(task.sourcePath) &&
      task.status !== "cancelled"
  );
  const canMergePdfs =
    qpdfAvailable &&
    realLocalPdfTasks.length >= 2 &&
    realLocalPdfTasks.every((task) => task.status !== "converting");

  async function planBackendOutput(task: LocalTask) {
    if (!task.sourcePath) {
      return;
    }

    try {
      const plan = await invoke<OutputPathPlan>("plan_output_path", {
        request: {
          source: task.sourcePath,
          targetExtension: "pdf",
          outputStrategy: "converted-folder-next-to-source"
        }
      });

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                outputPreview: plan.plannedOutputPath
              }
            : currentTask
        )
      );
    } catch {
      // Keep the frontend-only preview if backend planning is unavailable.
    }
  }

  function outputNameFromPreview(outputPreview: string): string {
    const normalizedPreview = outputPreview.replaceAll("\\", "/");
    return normalizedPreview.split("/").pop() || outputPreview;
  }

  function buildSiblingPath(sourcePath: string, fileName: string): string {
    const separator =
      sourcePath.includes("\\") && !sourcePath.includes("/") ? "\\" : "/";
    const lastSeparatorIndex = sourcePath.lastIndexOf(separator);
    if (lastSeparatorIndex < 0) {
      return fileName;
    }

    return `${sourcePath.slice(0, lastSeparatorIndex + 1)}${fileName}`;
  }

  function buildSiblingDirectory(sourcePath: string, directoryName: string): string {
    const separator =
      sourcePath.includes("\\") && !sourcePath.includes("/") ? "\\" : "/";
    const lastSeparatorIndex = sourcePath.lastIndexOf(separator);
    if (lastSeparatorIndex < 0) {
      return directoryName;
    }

    return `${sourcePath.slice(0, lastSeparatorIndex + 1)}${directoryName}`;
  }

  function formatMergeLog(result: QpdfMergeResult) {
    return [
      result.message,
      `Output: ${result.outputPath || "not written"}`,
      `Output bytes: ${result.outputBytes}`,
      `Exit code: ${result.exitCode ?? "none"}`,
      `Timed out: ${result.timedOut ? "yes" : "no"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <empty>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <empty>"
    ].join("\n");
  }

  function formatSplitLog(result: QpdfSplitResult) {
    return [
      result.message,
      `Source: ${result.sourcePath || "not available"}`,
      `Output folder: ${result.outputDirectory || "not created"}`,
      `Outputs: ${result.outputPaths.length}`,
      result.outputPaths.length > 0
        ? `Output paths:\n${result.outputPaths.join("\n")}`
        : "Output paths: <none>",
      `Output bytes: ${result.outputBytes}`,
      `Exit code: ${result.exitCode ?? "none"}`,
      `Timed out: ${result.timedOut ? "yes" : "no"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <empty>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <empty>"
    ].join("\n");
  }

  function formatExtractLog(result: QpdfExtractResult) {
    return [
      result.message,
      `Source: ${result.sourcePath || "not available"}`,
      `Output: ${result.outputPath || "not written"}`,
      `Output bytes: ${result.outputBytes}`,
      `Pages: ${result.pages || "not selected"}`,
      `Exit code: ${result.exitCode ?? "none"}`,
      `Timed out: ${result.timedOut ? "yes" : "no"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <empty>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <empty>"
    ].join("\n");
  }

  function formatRotateLog(result: QpdfRotateResult) {
    return [
      result.message,
      `Source: ${result.sourcePath || "not available"}`,
      `Output: ${result.outputPath || "not written"}`,
      `Output bytes: ${result.outputBytes}`,
      `Rotation: ${result.degrees || "not applied"}`,
      `Pages: ${result.pages || "not selected"}`,
      `Exit code: ${result.exitCode ?? "none"}`,
      `Timed out: ${result.timedOut ? "yes" : "no"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <empty>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <empty>"
    ].join("\n");
  }

  function canSplitPdfTask(task: LocalTask) {
    return (
      qpdfAvailable &&
      task.extension === "pdf" &&
      Boolean(task.sourcePath) &&
      task.status !== "converting" &&
      task.status !== "cancelled"
    );
  }

  function canRotatePdfTask(task: LocalTask) {
    return canSplitPdfTask(task);
  }

  function canExtractPdfTask(task: LocalTask) {
    return canSplitPdfTask(task);
  }

  function addFiles(fileList: FileList | File[]) {
    const files = Array.from(fileList);
    if (files.length === 0) {
      return;
    }

    const outputNames = tasks.map((task) =>
      outputNameFromPreview(task.outputPreview)
    );
    const createdTasks: LocalTask[] = [];

    for (const file of files) {
      const task = createTaskFromFile(file, [
        ...outputNames,
        ...createdTasks.map((item) => outputNameFromPreview(item.outputPreview))
      ]);
      createdTasks.push(task);
    }

    setTasks((currentTasks) => {
      return [...createdTasks, ...currentTasks];
    });

    for (const task of createdTasks) {
      void planBackendOutput(task);
    }
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

  async function mergePdfTasks() {
    const mergeTasks = realLocalPdfTasks;
    if (!qpdfAvailable || mergeTasks.length < 2) {
      setFolderMessage(
        "PDF merge requires bundled qpdf and at least two local PDF files with real paths."
      );
      return;
    }

    const taskIds = new Set(mergeTasks.map((task) => task.id));
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        taskIds.has(task.id)
          ? {
              ...task,
              status: "converting",
              errorLog: ""
            }
          : task
      )
    );

    try {
      const firstSourcePath = mergeTasks[0].sourcePath;
      if (!firstSourcePath) {
        throw new Error("PDF merge requires real local source paths.");
      }

      const outputSource = buildSiblingPath(firstSourcePath, "merged.pdf");
      const outputPlan = await invoke<OutputPathPlan>("plan_output_path", {
        request: {
          source: outputSource,
          targetExtension: "pdf",
          outputStrategy: "converted-folder-next-to-source"
        }
      });
      const result = await invoke<QpdfMergeResult>("qpdf_merge_pdfs", {
        request: {
          sources: mergeTasks.map((task) => task.sourcePath),
          output: outputPlan.plannedOutputPath
        }
      });
      const log = formatMergeLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((task) =>
          taskIds.has(task.id)
            ? {
                ...task,
                status: result.success ? "completed" : "failed",
                outputPreview: result.outputPath || outputPlan.plannedOutputPath,
                errorLog: result.success ? "" : log
              }
            : task
        )
      );
      setFolderMessage(
        result.success
          ? `PDF merge completed locally: ${result.outputPath}`
          : "PDF merge failed locally. See the error log."
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF merge failed locally.";
      setTasks((currentTasks) =>
        currentTasks.map((task) =>
          taskIds.has(task.id)
            ? {
                ...task,
                status: "failed",
                errorLog: message
              }
            : task
        )
      );
      setFolderMessage("PDF merge failed locally. See the error log.");
    }
  }

  async function splitPdfTask(task: LocalTask) {
    if (!canSplitPdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF split requires bundled qpdf and one local PDF file with a real path."
      );
      return;
    }

    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.id === task.id
          ? {
              ...currentTask,
              status: "converting",
              errorLog: ""
            }
          : currentTask
      )
    );

    try {
      const result = await invoke<QpdfSplitResult>("qpdf_split_pdf", {
        request: {
          source: task.sourcePath,
          outputDirectory: buildSiblingDirectory(task.sourcePath, "converted"),
          filenamePrefix: `${getBaseName(task.displayName)}-page`
        }
      });
      const log = formatSplitLog(result);
      const outputPreview = result.success
        ? `${result.outputPaths.length} files in ${result.outputDirectory}`
        : task.outputPreview;

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: result.success ? "completed" : "failed",
                outputPreview,
                errorLog: result.success ? "" : log
              }
            : currentTask
        )
      );
      setFolderMessage(
        result.success
          ? `PDF split completed locally: ${result.outputPaths.length} files in ${result.outputDirectory}`
          : "PDF split failed locally. See the error log."
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF split failed locally.";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: "failed",
                errorLog: message
              }
            : currentTask
        )
      );
      setFolderMessage("PDF split failed locally. See the error log.");
    }
  }

  async function extractPdfPages(task: LocalTask) {
    if (!canExtractPdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF page extraction requires bundled qpdf and one local PDF file with a real path."
      );
      return;
    }

    const pages = (extractPageRanges[task.id] || "").trim();
    if (!pages) {
      setFolderMessage("Enter a page range before extracting pages, for example 1,3,5-7.");
      return;
    }

    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.id === task.id
          ? {
              ...currentTask,
              status: "converting",
              errorLog: ""
            }
          : currentTask
      )
    );

    try {
      const outputSource = buildSiblingPath(
        task.sourcePath,
        `${getBaseName(task.displayName)} extracted.pdf`
      );
      const outputPlan = await invoke<OutputPathPlan>("plan_output_path", {
        request: {
          source: outputSource,
          targetExtension: "pdf",
          outputStrategy: "converted-folder-next-to-source"
        }
      });
      const result = await invoke<QpdfExtractResult>("qpdf_extract_pages", {
        request: {
          source: task.sourcePath,
          pages,
          output: outputPlan.plannedOutputPath
        }
      });
      const log = formatExtractLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: result.success ? "completed" : "failed",
                outputPreview: result.success ? result.outputPath : task.outputPreview,
                errorLog: result.success ? "" : log
              }
            : currentTask
        )
      );
      setFolderMessage(
        result.success
          ? `PDF page extraction completed locally: ${result.outputPath}`
          : "PDF page extraction failed locally. See the error log."
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF page extraction failed locally.";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: "failed",
                errorLog: message
              }
            : currentTask
        )
      );
      setFolderMessage("PDF page extraction failed locally. See the error log.");
    }
  }

  async function rotatePdfTask(task: LocalTask, degrees: 90 | 180 | -90) {
    if (!canRotatePdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF rotate requires bundled qpdf and one local PDF file with a real path."
      );
      return;
    }

    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.id === task.id
          ? {
              ...currentTask,
              status: "converting",
              errorLog: ""
            }
          : currentTask
      )
    );

    try {
      const outputSource = buildSiblingPath(
        task.sourcePath,
        `${getBaseName(task.displayName)} rotated.pdf`
      );
      const outputPlan = await invoke<OutputPathPlan>("plan_output_path", {
        request: {
          source: outputSource,
          targetExtension: "pdf",
          outputStrategy: "converted-folder-next-to-source"
        }
      });
      const result = await invoke<QpdfRotateResult>("qpdf_rotate_pages", {
        request: {
          source: task.sourcePath,
          pages: "",
          degrees,
          output: outputPlan.plannedOutputPath
        }
      });
      const log = formatRotateLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: result.success ? "completed" : "failed",
                outputPreview: result.success ? result.outputPath : task.outputPreview,
                errorLog: result.success ? "" : log
              }
            : currentTask
        )
      );
      setFolderMessage(
        result.success
          ? `PDF rotate completed locally: ${result.outputPath}`
          : "PDF rotate failed locally. See the error log."
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF rotate failed locally.";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.id === task.id
            ? {
                ...currentTask,
                status: "failed",
                errorLog: message
              }
            : currentTask
        )
      );
      setFolderMessage("PDF rotate failed locally. See the error log.");
    }
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
          <span>
            {qpdfAvailable ? "PDF qpdf tools enabled" : "Conversion disabled"}
          </span>
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
              <div>
                <h2>Task queue</h2>
                <span>
                  {tasks.length} total · {realLocalPdfTasks.length} local PDFs
                </span>
              </div>
              <button
                type="button"
                onClick={() => void mergePdfTasks()}
                disabled={!canMergePdfs}
                title={
                  qpdfAvailable
                    ? "Merge all queued local PDF files with bundled qpdf."
                    : "Bundled qpdf must be available before PDF merge can run."
                }
              >
                Merge PDFs
              </button>
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
                      {task.extension === "pdf" && task.sourcePath ? (
                        <button
                          type="button"
                          className="small-button"
                          onClick={() => void splitPdfTask(task)}
                          disabled={!canSplitPdfTask(task)}
                          title={
                            qpdfAvailable
                              ? "Split this one local PDF with bundled qpdf."
                              : "Bundled qpdf must be available before PDF split can run."
                          }
                        >
                          Split PDF
                        </button>
                      ) : null}
                      {task.extension === "pdf" && task.sourcePath ? (
                        <span className="extract-control">
                          <input
                            aria-label={`Pages to extract from ${task.displayName}`}
                            className="page-range-input"
                            placeholder="1,3,5-7"
                            value={extractPageRanges[task.id] || ""}
                            onChange={(event) =>
                              setExtractPageRanges((currentRanges) => ({
                                ...currentRanges,
                                [task.id]: event.currentTarget.value
                              }))
                            }
                          />
                          <button
                            type="button"
                            className="small-button"
                            onClick={() => void extractPdfPages(task)}
                            disabled={!canExtractPdfTask(task)}
                            title={
                              qpdfAvailable
                                ? "Extract selected pages from this one local PDF with bundled qpdf."
                                : "Bundled qpdf must be available before PDF page extraction can run."
                            }
                          >
                            Extract Pages
                          </button>
                        </span>
                      ) : null}
                      {task.extension === "pdf" && task.sourcePath ? (
                        <>
                          <span className="action-label">Rotate</span>
                          <button
                            type="button"
                            className="small-button"
                            onClick={() => void rotatePdfTask(task, -90)}
                            disabled={!canRotatePdfTask(task)}
                            title={
                              qpdfAvailable
                                ? "Rotate this one local PDF left 90 degrees with bundled qpdf."
                                : "Bundled qpdf must be available before PDF rotate can run."
                            }
                          >
                            Left 90
                          </button>
                          <button
                            type="button"
                            className="small-button"
                            onClick={() => void rotatePdfTask(task, 90)}
                            disabled={!canRotatePdfTask(task)}
                            title={
                              qpdfAvailable
                                ? "Rotate this one local PDF right 90 degrees with bundled qpdf."
                                : "Bundled qpdf must be available before PDF rotate can run."
                            }
                          >
                            Right 90
                          </button>
                          <button
                            type="button"
                            className="small-button"
                            onClick={() => void rotatePdfTask(task, 180)}
                            disabled={!canRotatePdfTask(task)}
                            title={
                              qpdfAvailable
                                ? "Rotate this one local PDF 180 degrees with bundled qpdf."
                                : "Bundled qpdf must be available before PDF rotate can run."
                            }
                          >
                            180
                          </button>
                        </>
                      ) : null}
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
            <p>
              {qpdfAvailable
                ? "PDF merge, split, page extraction, and rotate are enabled locally with bundled qpdf. Other conversions remain disabled."
                : "Conversion engines are not bundled yet."}
            </p>
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
                "No error log. Failed PDF merge, split, page extraction, rotate, or demo tasks appear here."}
            </pre>
          </section>

          <section className="inspector-card">
            <h2>Local privacy</h2>
            <ul>
              <li>No upload.</li>
              <li>PDF merge, split, page extraction, and rotate run with bundled local qpdf only.</li>
              <li>Other conversion operations remain disabled.</li>
            </ul>
          </section>
        </aside>
      </div>
    </main>
  );
}

export default App;
