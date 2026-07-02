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
  { label: "PDF Tools", enabled: true, note: "Enabled" },
  { label: "Batch Queue", enabled: true, note: "Local" },
  { label: "Documents to PDF", enabled: false, note: "Later" },
  { label: "Image Conversion", enabled: false, note: "Later" },
  { label: "Image Compression", enabled: false, note: "Later" },
  { label: "Images to PDF", enabled: false, note: "Later" }
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
  const [activeTool, setActiveTool] = useState("PDF Tools");
  const [dragActive, setDragActive] = useState(false);
  const [folderMessage, setFolderMessage] = useState("");
  const [extractPageRange, setExtractPageRange] = useState("");
  const [selectedTaskIds, setSelectedTaskIds] = useState<Set<string>>(
    () => new Set()
  );
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
  const selectedTasks = useMemo(
    () => tasks.filter((task) => selectedTaskIds.has(task.id)),
    [selectedTaskIds, tasks]
  );
  const selectedPdfTasks = selectedTasks.filter((task) => task.extension === "pdf");
  const selectedNonPdfTasks = selectedTasks.filter((task) => task.extension !== "pdf");
  const selectedPdfTasksWithoutPath = selectedPdfTasks.filter(
    (task) => !task.sourcePath
  );
  const selectedCancelledPdfTasks = selectedPdfTasks.filter(
    (task) => task.status === "cancelled"
  );
  const selectedRealLocalPdfTasks = selectedPdfTasks.filter(
    (task) => Boolean(task.sourcePath) && task.status !== "cancelled"
  );
  const selectedSinglePdfTask =
    selectedRealLocalPdfTasks.length === 1 ? selectedRealLocalPdfTasks[0] : undefined;
  const selectedHasConvertingTask = selectedRealLocalPdfTasks.some(
    (task) => task.status === "converting"
  );
  const canMergeSelectedPdfs =
    qpdfAvailable &&
    selectedNonPdfTasks.length === 0 &&
    selectedPdfTasksWithoutPath.length === 0 &&
    selectedRealLocalPdfTasks.length >= 2 &&
    !selectedHasConvertingTask;
  const canRunSelectedSinglePdfTool =
    qpdfAvailable &&
    selectedNonPdfTasks.length === 0 &&
    selectedPdfTasksWithoutPath.length === 0 &&
    selectedRealLocalPdfTasks.length === 1 &&
    !selectedHasConvertingTask;
  const canExtractSelectedPages =
    canRunSelectedSinglePdfTool && extractPageRange.trim().length > 0;

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

  function toggleTaskSelection(taskId: string) {
    setSelectedTaskIds((currentIds) => {
      const nextIds = new Set(currentIds);
      if (nextIds.has(taskId)) {
        nextIds.delete(taskId);
      } else {
        nextIds.add(taskId);
      }
      return nextIds;
    });
  }

  function selectPdfTasks() {
    setSelectedTaskIds(
      new Set(tasks.filter((task) => task.extension === "pdf").map((task) => task.id))
    );
  }

  function clearTaskSelection() {
    setSelectedTaskIds(new Set());
  }

  function pdfToolsGuidance() {
    if (!qpdfAvailable) {
      return "Bundled qpdf is unavailable. PDF tools stay disabled until the local sidecar passes self-check.";
    }

    if (selectedTasks.length === 0) {
      return "Select PDF tasks in the queue to enable local PDF tools.";
    }

    if (selectedNonPdfTasks.length > 0) {
      return "Only PDF tasks can use the current qpdf tools. Office and image conversions are not enabled yet.";
    }

    if (selectedPdfTasksWithoutPath.length > 0) {
      return "Some selected PDFs only have display metadata. qpdf tools need a real local file path from the desktop app.";
    }

    if (selectedCancelledPdfTasks.length > 0) {
      return "Cancelled PDF tasks cannot run qpdf operations. Retry or remove them before using PDF tools.";
    }

    if (selectedHasConvertingTask) {
      return "A selected PDF is already running. Wait for it to finish before starting another qpdf operation.";
    }

    return `${selectedRealLocalPdfTasks.length} local PDF task${
      selectedRealLocalPdfTasks.length === 1 ? "" : "s"
    } selected. Merge needs 2 or more; split, rotate, and extract need exactly 1.`;
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
    setSelectedTaskIds((currentIds) => {
      const nextIds = new Set(currentIds);
      nextIds.delete(taskId);
      return nextIds;
    });
  }

  function clearCompletedTasks() {
    const completedTaskIds = new Set(
      tasks.filter((task) => task.status === "completed").map((task) => task.id)
    );
    setTasks((currentTasks) =>
      currentTasks.filter((task) => task.status !== "completed")
    );
    setSelectedTaskIds((currentIds) => {
      const nextIds = new Set(currentIds);
      for (const taskId of completedTaskIds) {
        nextIds.delete(taskId);
      }
      return nextIds;
    });
  }

  async function mergePdfTasks() {
    const mergeTasks = selectedRealLocalPdfTasks;
    if (!canMergeSelectedPdfs) {
      setFolderMessage(pdfToolsGuidance());
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
          ? `Merged ${mergeTasks.length} PDFs locally. Output: ${result.outputPath}`
          : "PDF merge failed locally. Check the failed task error log."
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
      setFolderMessage("PDF merge failed locally. Check the failed task error log.");
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
          ? `Split PDF locally into ${result.outputPaths.length} files: ${result.outputPaths.join(" | ")}`
          : "PDF split failed locally. Check the failed task error log."
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
      setFolderMessage("PDF split failed locally. Check the failed task error log.");
    }
  }

  async function extractPdfPages(task: LocalTask) {
    if (!canExtractPdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF page extraction requires bundled qpdf and one local PDF file with a real path."
      );
      return;
    }

    const pages = extractPageRange.trim();
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
          ? `Extracted pages ${result.pages} locally. Output: ${result.outputPath}`
          : "PDF page extraction failed locally. Check the failed task error log."
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
      setFolderMessage("PDF page extraction failed locally. Check the failed task error log.");
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
          ? `Rotated PDF locally (${result.degrees}). Output: ${result.outputPath}`
          : "PDF rotate failed locally. Check the failed task error log."
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
      setFolderMessage("PDF rotate failed locally. Check the failed task error log.");
    }
  }

  return (
    <main className="app-shell">
      <header className="top-bar">
        <div className="app-title">
          <h1>LocalConvert Desktop</h1>
          <p>by 田宸宇</p>
        </div>
        <div className="privacy-status" aria-label="Local privacy status">
          <span>Files stay on this computer</span>
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
                className={activeTool === tool.label ? "tool-item is-active" : "tool-item"}
                key={tool.label}
                onClick={() => setActiveTool(tool.label)}
                disabled={!tool.enabled}
                title={
                  tool.enabled
                    ? `${tool.label} is available in this preview.`
                    : `${tool.label} is not enabled yet.`
                }
              >
                <span>{tool.label}</span>
                <small>{tool.note}</small>
              </button>
            ))}
          </nav>
          <div className="sidebar-note">
            <strong>PDF tools preview</strong>
            <span>Only qpdf PDF tools are enabled. Office and image tools remain off.</span>
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

          <section className="pdf-tools-panel" aria-label="PDF tools">
            <div className="pdf-tools-header">
              <div>
                <p className="section-kicker">PDF Tools</p>
                <h2>Local qpdf tools</h2>
                <p>Files stay on this computer. PDF tools use bundled qpdf.</p>
              </div>
              <div className="selection-tools">
                <button type="button" className="secondary-button" onClick={selectPdfTasks}>
                  Select PDFs
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={clearTaskSelection}
                  disabled={selectedTasks.length === 0}
                >
                  Clear selection
                </button>
              </div>
            </div>

            <div className="pdf-tools-status">
              <strong>
                {selectedTasks.length} selected · {selectedRealLocalPdfTasks.length} usable local PDFs
              </strong>
              <span>{pdfToolsGuidance()}</span>
            </div>

            <div className="pdf-tool-grid">
              <article className="pdf-tool-card">
                <h3>Merge selected PDFs</h3>
                <p>Requires 2 or more selected PDFs with real local paths.</p>
                <button
                  type="button"
                  onClick={() => void mergePdfTasks()}
                  disabled={!canMergeSelectedPdfs}
                >
                  Merge selected PDFs
                </button>
              </article>

              <article className="pdf-tool-card">
                <h3>Split selected PDF</h3>
                <p>Requires exactly 1 selected PDF with a real local path.</p>
                <button
                  type="button"
                  onClick={() => selectedSinglePdfTask && void splitPdfTask(selectedSinglePdfTask)}
                  disabled={!canRunSelectedSinglePdfTool}
                >
                  Split selected PDF
                </button>
              </article>

              <article className="pdf-tool-card">
                <h3>Rotate selected PDF</h3>
                <p>Requires exactly 1 selected PDF. Choose left, right, or 180.</p>
                <div className="segmented-actions">
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void rotatePdfTask(selectedSinglePdfTask, -90)
                    }
                    disabled={!canRunSelectedSinglePdfTool}
                  >
                    Left 90
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void rotatePdfTask(selectedSinglePdfTask, 90)
                    }
                    disabled={!canRunSelectedSinglePdfTool}
                  >
                    Right 90
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void rotatePdfTask(selectedSinglePdfTask, 180)
                    }
                    disabled={!canRunSelectedSinglePdfTool}
                  >
                    180
                  </button>
                </div>
              </article>

              <article className="pdf-tool-card">
                <h3>Extract pages from selected PDF</h3>
                <p>Requires exactly 1 selected PDF and a page range.</p>
                <div className="extract-panel-control">
                  <input
                    aria-label="Pages to extract from selected PDF"
                    className="page-range-input"
                    placeholder="1,3,5-7"
                    value={extractPageRange}
                    onChange={(event) => setExtractPageRange(event.currentTarget.value)}
                  />
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void extractPdfPages(selectedSinglePdfTask)
                    }
                    disabled={!canExtractSelectedPages}
                  >
                    Extract pages
                  </button>
                </div>
              </article>
            </div>
          </section>

          <section className="queue-panel" aria-label="Task queue">
            <div className="queue-heading">
              <div>
                <h2>Task queue</h2>
                <span>
                  {tasks.length} total · {realLocalPdfTasks.length} local PDFs · {selectedTasks.length} selected
                </span>
              </div>
            </div>

            {tasks.length === 0 ? (
              <div className="empty-state">
                <h3>No tasks yet</h3>
                <p>Add files to create waiting tasks.</p>
              </div>
            ) : (
              <div className="task-table" role="table" aria-label="Local tasks">
                <div className="task-table-head" role="row">
                  <span>Select</span>
                  <span>File</span>
                  <span>Status</span>
                  <span>Output preview</span>
                  <span>Actions</span>
                </div>
                {tasks.map((task) => (
                  <article className="task-row" role="row" key={task.id}>
                    <label className="select-cell" aria-label={`Select ${task.displayName}`}>
                      <input
                        type="checkbox"
                        checked={selectedTaskIds.has(task.id)}
                        onChange={() => toggleTaskSelection(task.id)}
                      />
                    </label>
                    <div className="file-cell">
                      <strong>{task.displayName}</strong>
                      <span>
                        {task.extension.toUpperCase()} · {formatBytes(task.size)}
                      </span>
                    </div>
                    <span className={`status-chip status-${task.status}`}>
                      {statusLabels[task.status]}
                    </span>
                    <div className="output-cell" title={task.outputPreview}>
                      <span>{task.outputPreview}</span>
                      <small className={task.sourcePath ? "" : "path-warning"}>
                        {task.sourcePath
                          ? task.sourcePreview
                          : `${task.sourcePreview} · display metadata only`}
                      </small>
                    </div>
                    <div className="task-actions">
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
            <h2>About</h2>
            <p>
              <strong>Created by 田宸宇</strong>
            </p>
            <p>Local-only file conversion tool.</p>
          </section>

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
