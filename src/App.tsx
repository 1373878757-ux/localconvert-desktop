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

type StartupStatus = {
  completed: boolean;
  selfCheck: EngineSelfCheck | null;
  error: string | null;
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
  platform: "桌面预览",
  fullEdition: true,
  conversionEnabled: false,
  engines: [
    {
      name: "LibreOffice headless",
      status: "not-installed",
      requiredForV1: true,
      message: "尚未内置。"
    },
    {
      name: "qpdf",
      status: "not-installed",
      requiredForV1: true,
      message: "尚未内置。"
    },
    {
      name: "PDFium",
      status: "not-installed",
      requiredForV1: true,
      message: "尚未内置。"
    },
    {
      name: "image-engine",
      status: "not-installed",
      requiredForV1: true,
      message: "尚未内置。"
    }
  ]
};

const toolCategories = [
  { label: "PDF 工具", enabled: true, note: "可用" },
  { label: "批量队列", enabled: true, note: "本地" },
  { label: "文档转 PDF", enabled: false, note: "稍后" },
  { label: "图片转换", enabled: false, note: "稍后" },
  { label: "图片压缩", enabled: false, note: "稍后" },
  { label: "图片转 PDF", enabled: false, note: "稍后" }
];

const statusLabels: Record<TaskStatus, string> = {
  waiting: "等待中",
  converting: "处理中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消"
};

const outputNameExample = [
  getOutputName("report.pdf", []),
  getOutputName("report.pdf", ["report.pdf"]),
  getOutputName("report.pdf", ["report.pdf", "report (1).pdf"])
];

function App() {
  const [selfCheck, setSelfCheck] = useState<EngineSelfCheck>(fallbackSelfCheck);
  const [tasks, setTasks] = useState<LocalTask[]>([]);
  const [activeTool, setActiveTool] = useState("PDF 工具");
  const [dragActive, setDragActive] = useState(false);
  const [folderMessage, setFolderMessage] = useState("");
  const [startupError, setStartupError] = useState("");
  const [extractPageRange, setExtractPageRange] = useState("");
  const [selectedTaskIds, setSelectedTaskIds] = useState<Set<string>>(
    () => new Set()
  );
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    async function loadSelfCheck() {
      try {
        const startup = await invoke<StartupStatus>("startup_status");
        if (startup.error) {
          setStartupError(startup.error);
        }
        if (startup.selfCheck) {
          setSelfCheck(startup.selfCheck);
          return;
        }
        if (startup.completed) {
          setSelfCheck(fallbackSelfCheck);
          return;
        }
      } catch {
        // Fall back to the direct command for development builds that predate startup status.
      }

      try {
        setSelfCheck(await invoke<EngineSelfCheck>("engine_self_check"));
      } catch {
        setSelfCheck(fallbackSelfCheck);
        setStartupError("启动状态和引擎自检不可用。");
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
  const inspectorErrorLog =
    selectedTaskWithError?.errorLog ||
    (startupError ? `启动初始化问题:\n${startupError}` : "");
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
      `引擎消息: ${result.message}`,
      `输出: ${result.outputPath || "未写入"}`,
      `输出大小: ${result.outputBytes} 字节`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatSplitLog(result: QpdfSplitResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `输出文件夹: ${result.outputDirectory || "未创建"}`,
      `输出数量: ${result.outputPaths.length}`,
      result.outputPaths.length > 0
        ? `输出路径:\n${result.outputPaths.join("\n")}`
        : "输出路径: <无>",
      `输出大小: ${result.outputBytes} 字节`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatExtractLog(result: QpdfExtractResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `输出: ${result.outputPath || "未写入"}`,
      `输出大小: ${result.outputBytes} 字节`,
      `页面: ${result.pages || "未选择"}`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatRotateLog(result: QpdfRotateResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `输出: ${result.outputPath || "未写入"}`,
      `输出大小: ${result.outputBytes} 字节`,
      `旋转: ${result.degrees || "未应用"}`,
      `页面: ${result.pages || "未选择"}`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatEngineMessage(engine: EngineStatus) {
    if (engine.status === "not-installed") {
      return "尚未内置。";
    }

    if (engine.name === "qpdf" && engine.status === "available") {
      return "qpdf 可用，内置引擎自检已通过。";
    }

    if (engine.status === "error") {
      return `本地检查失败：${engine.message}`;
    }

    return engine.message;
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
      return "内置 qpdf 不可用。PDF 工具会保持禁用，直到内置 qpdf 程序通过自检。";
    }

    if (selectedTasks.length === 0) {
      return "请在任务队列中选择 PDF 任务，以启用本地 PDF 工具。";
    }

    if (selectedNonPdfTasks.length > 0) {
      return "当前 qpdf 工具仅支持 PDF 任务。Office 和图片转换尚未启用。";
    }

    if (selectedPdfTasksWithoutPath.length > 0) {
      return "部分所选 PDF 只有显示元数据。qpdf 工具需要桌面端提供真实本地路径。";
    }

    if (selectedCancelledPdfTasks.length > 0) {
      return "已取消的 PDF 任务不能执行 qpdf 操作。请先重试或移除它们。";
    }

    if (selectedHasConvertingTask) {
      return "所选 PDF 中已有任务正在处理。请等待完成后再启动新的 qpdf 操作。";
    }

    return `已选择 ${selectedRealLocalPdfTasks.length} 个可用本地 PDF。合并需要 2 个或更多；拆分、旋转和提取需要正好 1 个。`;
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
        throw new Error("PDF 合并需要真实的本地源文件路径。");
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
          ? `已在本地合并 ${mergeTasks.length} 个 PDF。输出：${result.outputPath}`
          : "PDF 合并失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF 合并失败。";
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
      setFolderMessage("PDF 合并失败。请查看失败任务的错误日志。");
    }
  }

  async function splitPdfTask(task: LocalTask) {
    if (!canSplitPdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF 拆分需要内置 qpdf 和 1 个带真实路径的本地 PDF。"
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
        ? `${result.outputPaths.length} 个文件，位于 ${result.outputDirectory}`
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
          ? `已在本地拆分 PDF，生成 ${result.outputPaths.length} 个文件：${result.outputPaths.join(" | ")}`
          : "PDF 拆分失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF 拆分失败。";
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
      setFolderMessage("PDF 拆分失败。请查看失败任务的错误日志。");
    }
  }

  async function extractPdfPages(task: LocalTask) {
    if (!canExtractPdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF 页面提取需要内置 qpdf 和 1 个带真实路径的本地 PDF。"
      );
      return;
    }

    const pages = extractPageRange.trim();
    if (!pages) {
      setFolderMessage("请输入要提取的页面范围，例如 1,3,5-7。");
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
          ? `已在本地提取页面 ${result.pages}。输出：${result.outputPath}`
          : "PDF 页面提取失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF 页面提取失败。";
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
      setFolderMessage("PDF 页面提取失败。请查看失败任务的错误日志。");
    }
  }

  async function rotatePdfTask(task: LocalTask, degrees: 90 | 180 | -90) {
    if (!canRotatePdfTask(task) || !task.sourcePath) {
      setFolderMessage(
        "PDF 旋转需要内置 qpdf 和 1 个带真实路径的本地 PDF。"
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
          ? `已在本地旋转 PDF（${result.degrees}）。输出：${result.outputPath}`
          : "PDF 旋转失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "PDF 旋转失败。";
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
      setFolderMessage("PDF 旋转失败。请查看失败任务的错误日志。");
    }
  }

  return (
    <main className="app-shell">
      <header className="top-bar">
        <div className="app-title">
          <h1>LocalConvert Desktop</h1>
          <p>by 田宸宇</p>
        </div>
        <div className="privacy-status" aria-label="本地隐私状态">
          <span>文件仅在本机处理</span>
          <span>不上传</span>
          <span>本地队列</span>
          <span>
            {qpdfAvailable ? "PDF qpdf 工具已启用" : "转换未启用"}
          </span>
        </div>
      </header>

      <div className="workbench">
        <aside className="sidebar" aria-label="工具分类">
          <div className="sidebar-section-title">工具</div>
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
                    ? `${tool.label} 在此预览版中可用。`
                    : `${tool.label} 尚未启用。`
                }
              >
                <span>{tool.label}</span>
                <small>{tool.note}</small>
              </button>
            ))}
          </nav>
          <div className="sidebar-note">
            <strong>PDF 工具预览</strong>
            <span>当前仅启用 qpdf PDF 工具。Office 和图片工具仍未启用。</span>
          </div>
        </aside>

        <section className="main-pane" aria-label="文件队列工作台">
          <div className="pane-header">
            <div>
              <p className="section-kicker">{activeTool}</p>
              <h2>文件导入</h2>
            </div>
            <button
              type="button"
              className="secondary-button"
              onClick={clearCompletedTasks}
              disabled={summary.completed === 0}
            >
              清除已完成
            </button>
          </div>

          <section
            className={dragActive ? "drop-zone is-active" : "drop-zone"}
            aria-label="拖放文件"
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
              <h3>将文件拖到这里，或点击选择文件。</h3>
              <p>仅保存名称、大小、扩展名和显示路径。</p>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => fileInputRef.current?.click()}>
                选择文件
              </button>
              <button
                type="button"
                className="secondary-button"
                onClick={() =>
                  setFolderMessage(
                    "文件夹选择将在转换流程完成后，通过原生 Tauri 权限启用。"
                  )
                }
              >
                选择文件夹
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

          <section className="status-summary" aria-label="任务状态汇总">
            {(Object.keys(statusLabels) as TaskStatus[]).map((status) => (
              <article className="summary-cell" key={status}>
                <span>{statusLabels[status]}</span>
                <strong>{summary[status]}</strong>
              </article>
            ))}
          </section>

          <section className="pdf-tools-panel" aria-label="PDF 工具">
            <div className="pdf-tools-header">
              <div>
                <p className="section-kicker">PDF 工具</p>
                <h2>本地 qpdf 工具</h2>
                <p>文件仅在本机处理。PDF 工具使用内置 qpdf。</p>
              </div>
              <div className="selection-tools">
                <button type="button" className="secondary-button" onClick={selectPdfTasks}>
                  选择 PDF
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={clearTaskSelection}
                  disabled={selectedTasks.length === 0}
                >
                  清除选择
                </button>
              </div>
            </div>

            <div className="pdf-tools-status">
              <strong>
                {selectedTasks.length} 个已选 · {selectedRealLocalPdfTasks.length} 个可用本地 PDF
              </strong>
              <span>{pdfToolsGuidance()}</span>
            </div>

            <div className="pdf-tool-grid">
              <article className="pdf-tool-card">
                <h3>合并所选 PDF</h3>
                <p>需要选择 2 个或更多带真实本地路径的 PDF。</p>
                <button
                  type="button"
                  onClick={() => void mergePdfTasks()}
                  disabled={!canMergeSelectedPdfs}
                >
                  合并所选 PDF
                </button>
              </article>

              <article className="pdf-tool-card">
                <h3>拆分所选 PDF</h3>
                <p>需要正好选择 1 个带真实本地路径的 PDF。</p>
                <button
                  type="button"
                  onClick={() => selectedSinglePdfTask && void splitPdfTask(selectedSinglePdfTask)}
                  disabled={!canRunSelectedSinglePdfTool}
                >
                  拆分所选 PDF
                </button>
              </article>

              <article className="pdf-tool-card">
                <h3>旋转所选 PDF</h3>
                <p>需要正好选择 1 个 PDF。选择左转、右转或 180 度。</p>
                <div className="segmented-actions">
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void rotatePdfTask(selectedSinglePdfTask, -90)
                    }
                    disabled={!canRunSelectedSinglePdfTool}
                  >
                    左转 90°
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      selectedSinglePdfTask && void rotatePdfTask(selectedSinglePdfTask, 90)
                    }
                    disabled={!canRunSelectedSinglePdfTool}
                  >
                    右转 90°
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
                <h3>提取所选 PDF 页面</h3>
                <p>需要正好选择 1 个 PDF，并填写页码范围。</p>
                <div className="extract-panel-control">
                  <input
                    aria-label="要提取的页面范围"
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
                    提取页面
                  </button>
                </div>
              </article>
            </div>
          </section>

          <section className="image-tools-panel" aria-label="Preview 0.2 图片工具规划">
            <div className="pdf-tools-header">
              <div>
                <p className="section-kicker">Preview 0.2</p>
                <h2>图片工具规划</h2>
                <p>图片转换引擎尚未内置，当前版本仅启用 PDF 工具。</p>
              </div>
            </div>

            <div className="image-format-list" aria-label="计划支持的图片格式">
              <span>JPG</span>
              <span>PNG</span>
              <span>WebP</span>
              <span>AVIF</span>
              <span>TIFF</span>
              <span className="planned-later">HEIC 稍后</span>
            </div>

            <div className="pdf-tool-grid image-tool-grid">
              <article className="pdf-tool-card image-tool-card">
                <h3>图片格式转换</h3>
                <p>计划支持 JPG、PNG、WebP、AVIF 和 TIFF 输出。</p>
                <button type="button" disabled>
                  引擎尚未内置
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>图片压缩</h3>
                <p>计划提供高清、平衡、小体积等本地压缩预设。</p>
                <button type="button" disabled>
                  引擎尚未内置
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>图片改尺寸</h3>
                <p>计划支持按宽度、高度或等比规则批量调整尺寸。</p>
                <button type="button" disabled>
                  引擎尚未内置
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>移除图片元数据</h3>
                <p>计划在本机移除 EXIF 等图片元数据，不上传文件。</p>
                <button type="button" disabled>
                  引擎尚未内置
                </button>
              </article>
            </div>
          </section>

          <section className="queue-panel" aria-label="任务队列">
            <div className="queue-heading">
              <div>
                <h2>任务队列</h2>
                <span>
                  {tasks.length} 个任务 · {realLocalPdfTasks.length} 个本地 PDF · {selectedTasks.length} 个已选
                </span>
              </div>
            </div>

            {tasks.length === 0 ? (
              <div className="empty-state">
                <h3>暂无任务</h3>
                <p>添加文件后会创建等待任务。</p>
              </div>
            ) : (
              <div className="task-table" role="table" aria-label="本地任务">
                <div className="task-table-head" role="row">
                  <span>选择</span>
                  <span>文件</span>
                  <span>状态</span>
                  <span>输出预览</span>
                  <span>操作</span>
                </div>
                {tasks.map((task) => (
                  <article className="task-row" role="row" key={task.id}>
                    <label className="select-cell" aria-label={`选择 ${task.displayName}`}>
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
                          : `${task.sourcePreview} · 仅显示元数据`}
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
                        取消
                      </button>
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => retryTask(task.id)}
                        disabled={task.status !== "failed"}
                      >
                        重试
                      </button>
                      <button
                        type="button"
                        className="small-button ghost-button"
                        onClick={() => removeTask(task.id)}
                      >
                        移除
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </section>
        </section>

        <aside className="inspector" aria-label="检查器">
          <section className="inspector-card">
            <h2>关于</h2>
            <p>
              <strong>by 田宸宇</strong>
            </p>
            <p className="about-slogan">让可能发生在这儿。</p>
            <p>纯本地文件转换工具。</p>
          </section>

          <section className="inspector-card">
            <h2>输出规则</h2>
            <p>默认输出到源文件旁边的 converted 文件夹。</p>
            <pre>{outputNameExample.join("\n")}</pre>
          </section>

          <section className="inspector-card">
            <h2>引擎状态</h2>
            <p>
              {startupError
                ? `启动初始化报告问题：${startupError}`
                : qpdfAvailable
                  ? "PDF 合并、拆分、页面提取和旋转已通过内置 qpdf 在本地启用。其他转换仍未启用。"
                  : "转换引擎尚未内置。"}
            </p>
            <dl className="engine-list">
              {selfCheck.engines.map((engine) => (
                <div className="engine-row" key={engine.name}>
                  <dt>{engine.name}</dt>
                  <dd>{formatEngineMessage(engine)}</dd>
                </div>
              ))}
            </dl>
          </section>

          <section className="inspector-card">
            <h2>错误日志</h2>
            <pre className="log-box">
              {inspectorErrorLog ||
                "暂无错误日志。PDF 合并、拆分、页面提取、旋转或演示任务失败时会显示在这里。"}
            </pre>
          </section>

          <section className="inspector-card">
            <h2>本地隐私</h2>
            <ul>
              <li>不上传。</li>
              <li>不会修改原文件。</li>
              <li>PDF 合并、拆分、页面提取和旋转仅使用内置本地 qpdf。</li>
              <li>其他转换操作仍未启用。</li>
            </ul>
          </section>
        </aside>
      </div>
    </main>
  );
}

export default App;
