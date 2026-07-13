import {
  ChangeEvent,
  DragEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  LocalTask,
  NativePathMetadata,
  TaskStatus,
  createTaskFromFile,
  createTaskFromNativePathMetadata,
  formatBytes,
  getBaseName,
  getExtension,
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

type RejectedNativePath = {
  sourcePath: string;
  message: string;
};

type NativePathInspection = {
  files: NativePathMetadata[];
  rejected: RejectedNativePath[];
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

type ImageConvertResult = {
  success: boolean;
  operation: "convert";
  sourcePath: string;
  outputPath: string;
  sourceFormat: string;
  targetFormat: string;
  outputBytes: number;
  width: number;
  height: number;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type BackendTaskStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

type BackendTaskResponse<T> = {
  taskId: string;
  operation: string;
  status: BackendTaskStatus;
  result: T;
};

type CancelTaskResponse = {
  taskId: string;
  operation: string;
  status: BackendTaskStatus;
  cancellationRequested: boolean;
  processTerminationRequested: boolean;
  message: string;
};

type EnabledImageFormat = "jpg" | "png" | "webp";

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

const enabledImageExtensions = new Set(["jpg", "jpeg", "png", "webp"]);

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

function backendResponseStatus<T extends { success: boolean }>(
  response: BackendTaskResponse<T>
): TaskStatus {
  if (response.status === "cancelled") {
    return "cancelled";
  }

  return response.status === "completed" && response.result.success
    ? "completed"
    : "failed";
}

function createBackendTaskId(operation: string, taskId: string): string {
  return `${operation}-${taskId}-${Date.now()}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

function App() {
  const [selfCheck, setSelfCheck] = useState<EngineSelfCheck>(fallbackSelfCheck);
  const [tasks, setTasks] = useState<LocalTask[]>([]);
  const [activeTool, setActiveTool] = useState("PDF 工具");
  const [dragActive, setDragActive] = useState(false);
  const [folderMessage, setFolderMessage] = useState("");
  const [intakeMessage, setIntakeMessage] = useState("");
  const [intakeError, setIntakeError] = useState("");
  const [startupError, setStartupError] = useState("");
  const [extractPageRange, setExtractPageRange] = useState("");
  const [imageTargetFormat, setImageTargetFormat] =
    useState<EnabledImageFormat>("webp");
  const [selectedTaskIds, setSelectedTaskIds] = useState<Set<string>>(
    () => new Set()
  );
  const fileInputRef = useRef<HTMLInputElement>(null);
  const nativePathIntakeRef = useRef<(paths: string[]) => Promise<void>>(
    async () => undefined
  );
  const cancelledTaskIdsRef = useRef<Set<string>>(new Set());

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
    (intakeError ? `文件导入问题:\n${intakeError}` : "") ||
    (startupError ? `启动初始化问题:\n${startupError}` : "");
  const qpdfEngine = selfCheck.engines.find((engine) => engine.name === "qpdf");
  const qpdfAvailable = qpdfEngine?.status === "available";
  const imageEngine = selfCheck.engines.find(
    (engine) => engine.name === "image-engine"
  );
  const imageEngineAvailable = imageEngine?.status === "available";
  const toolCategories = [
    { label: "PDF 工具", enabled: qpdfAvailable, note: qpdfAvailable ? "可用" : "不可用" },
    { label: "图片转换", enabled: imageEngineAvailable, note: imageEngineAvailable ? "可用" : "不可用" },
    { label: "批量队列", enabled: true, note: "本地" },
    { label: "文档转 PDF", enabled: false, note: "稍后" },
    { label: "图片压缩", enabled: false, note: "稍后" },
    { label: "图片转 PDF", enabled: false, note: "稍后" }
  ];
  const realLocalPdfTasks = tasks.filter(
    (task) =>
      task.extension === "pdf" &&
      task.sourceKind === "native-path" &&
      Boolean(task.sourcePath) &&
      task.status !== "cancelled"
  );
  const realLocalImageTasks = tasks.filter(
    (task) =>
      enabledImageExtensions.has(task.extension) &&
      task.sourceKind === "native-path" &&
      Boolean(task.sourcePath) &&
      task.status !== "cancelled"
  );
  const selectedTasks = useMemo(
    () => tasks.filter((task) => selectedTaskIds.has(task.taskId)),
    [selectedTaskIds, tasks]
  );
  const selectedPdfTasks = selectedTasks.filter((task) => task.extension === "pdf");
  const selectedNonPdfTasks = selectedTasks.filter((task) => task.extension !== "pdf");
  const selectedPdfTasksWithoutPath = selectedPdfTasks.filter(
    (task) => task.sourceKind !== "native-path" || !task.sourcePath
  );
  const selectedCancelledPdfTasks = selectedPdfTasks.filter(
    (task) => task.status === "cancelled"
  );
  const selectedRealLocalPdfTasks = selectedPdfTasks.filter(
    (task) =>
      task.sourceKind === "native-path" &&
      Boolean(task.sourcePath) &&
      task.status !== "cancelled"
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

  const selectedEnabledImageTasks = selectedTasks.filter((task) =>
    enabledImageExtensions.has(task.extension)
  );
  const selectedHeicImageTasks = selectedTasks.filter(
    (task) => task.extension === "heic"
  );
  const selectedUnsupportedImageTasks = selectedTasks.filter(
    (task) => !enabledImageExtensions.has(task.extension)
  );
  const selectedImageTasksWithoutPath = selectedEnabledImageTasks.filter(
    (task) => task.sourceKind !== "native-path" || !task.sourcePath
  );
  const selectedCancelledImageTasks = selectedEnabledImageTasks.filter(
    (task) => task.status === "cancelled"
  );
  const selectedConvertingImageTasks = selectedEnabledImageTasks.filter(
    (task) => task.status === "converting"
  );
  const selectedSameFormatImageTasks = selectedEnabledImageTasks.filter(
    (task) => canonicalImageFormat(task.extension) === imageTargetFormat
  );
  const selectedRealLocalImageTasks = selectedEnabledImageTasks.filter(
    (task) =>
      task.sourceKind === "native-path" &&
      Boolean(task.sourcePath) &&
      task.status !== "cancelled"
  );
  const canConvertSelectedImages =
    imageEngineAvailable &&
    selectedTasks.length > 0 &&
    selectedUnsupportedImageTasks.length === 0 &&
    selectedImageTasksWithoutPath.length === 0 &&
    selectedCancelledImageTasks.length === 0 &&
    selectedConvertingImageTasks.length === 0 &&
    selectedSameFormatImageTasks.length === 0;

  async function planBackendOutput(task: LocalTask, targetExtension = "pdf") {
    if (task.sourceKind !== "native-path" || !task.sourcePath) {
      return;
    }

    try {
      const plan = await invoke<OutputPathPlan>("plan_output_path", {
        request: {
          source: task.sourcePath,
          targetExtension,
          outputStrategy: "converted-folder-next-to-source"
        }
      });

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
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

  function canonicalImageFormat(extension: string): EnabledImageFormat | "" {
    if (extension === "jpg" || extension === "jpeg") {
      return "jpg";
    }
    if (extension === "png" || extension === "webp") {
      return extension;
    }
    return "";
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

  function formatImageConvertLog(result: ImageConvertResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `输出: ${result.outputPath || "未写入"}`,
      `格式: ${result.sourceFormat || "未知"} → ${result.targetFormat || "未知"}`,
      `尺寸: ${result.width > 0 && result.height > 0 ? `${result.width} × ${result.height}` : "未验证"}`,
      `输出大小: ${result.outputBytes} 字节`,
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

    if (engine.name === "image-engine" && engine.status === "available") {
      return "image-engine 可用，JPG、PNG、WebP 本地转换已通过自检。";
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
      task.sourceKind === "native-path" &&
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
      new Set(
        tasks
          .filter((task) => task.extension === "pdf")
          .map((task) => task.taskId)
      )
    );
  }

  function selectImageTasks() {
    setSelectedTaskIds(
      new Set(
        tasks
          .filter((task) => enabledImageExtensions.has(task.extension))
          .map((task) => task.taskId)
      )
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
      return "当前 qpdf 工具仅支持 PDF 任务。图片请使用图片转换面板。";
    }

    if (selectedPdfTasksWithoutPath.length > 0) {
      return "部分所选 PDF 只有预览元数据，没有本地路径。真实 PDF 工具需要通过原生“选择文件”或桌面拖放重新导入。";
    }

    if (selectedCancelledPdfTasks.length > 0) {
      return "已取消的 PDF 任务不能执行 qpdf 操作。请先重试或移除它们。";
    }

    if (selectedHasConvertingTask) {
      return "所选 PDF 中已有任务正在处理。请等待完成后再启动新的 qpdf 操作。";
    }

    return `已选择 ${selectedRealLocalPdfTasks.length} 个可用本地 PDF。合并需要 2 个或更多；拆分、旋转和提取需要正好 1 个。`;
  }

  function imageToolsGuidance() {
    if (!imageEngineAvailable) {
      return "内置 image-engine 不可用。图片转换会保持禁用，直到本地引擎通过自检。";
    }

    if (selectedTasks.length === 0) {
      return "请在任务队列中选择 JPG、JPEG、PNG 或 WebP 图片。";
    }

    if (selectedHeicImageTasks.length > 0) {
      return `所选任务中有 ${selectedHeicImageTasks.length} 个 HEIC 文件。当前 image-engine 不支持 HEIC 解码或方向处理，不会启动转换。`;
    }

    if (selectedUnsupportedImageTasks.length > 0) {
      return "所选任务中含有当前不支持的格式。图片转换仅启用 JPG/JPEG、PNG 和 WebP。";
    }

    if (selectedImageTasksWithoutPath.length > 0) {
      return "部分所选图片只有预览元数据，没有本地路径。真实图片转换需要通过原生“选择文件”或桌面拖放重新导入。";
    }

    if (selectedCancelledImageTasks.length > 0) {
      return "已取消的图片任务不能转换，请先重试或移除。";
    }

    if (selectedConvertingImageTasks.length > 0) {
      return "所选图片中已有任务正在处理，请等待完成。";
    }

    if (selectedSameFormatImageTasks.length > 0) {
      return `有 ${selectedSameFormatImageTasks.length} 张图片已经是 ${imageTargetFormat.toUpperCase()}，请选择不同的输出格式。`;
    }

    return `已选择 ${selectedRealLocalImageTasks.length} 张本地图片，将转换为 ${imageTargetFormat.toUpperCase()}。`;
  }

  function addPreviewFiles(fileList: FileList | File[]) {
    const files = Array.from(fileList);
    if (files.length === 0) {
      return;
    }

    const outputNames = tasks.map((task) =>
      outputNameFromPreview(task.outputPreview)
    );
    const createdTasks: LocalTask[] = [];

    for (const file of files) {
      const extension = getExtension(file.name || "");
      const targetExtension = enabledImageExtensions.has(extension)
        ? imageTargetFormat
        : "pdf";
      const task = createTaskFromFile(file, [
        ...outputNames,
        ...createdTasks.map((item) => outputNameFromPreview(item.outputPreview))
      ], Date.now(), targetExtension);
      createdTasks.push(task);
    }

    setTasks((currentTasks) => {
      return [...createdTasks, ...currentTasks];
    });

    setIntakeError("");
    setIntakeMessage(
      `已添加 ${createdTasks.length} 个预览任务。这些条目没有本地绝对路径，真实 PDF 和图片工具不可用；请使用桌面端原生选择或拖放导入。`
    );
  }

  async function addNativePaths(paths: string[]) {
    if (paths.length === 0) {
      return;
    }

    setIntakeError("");

    let inspection: NativePathInspection;
    try {
      inspection = await invoke<NativePathInspection>("inspect_native_paths", {
        request: { paths }
      });
    } catch (error) {
      const message =
        typeof error === "string" ? error : "原生本地路径检查失败。";
      setIntakeError(message);
      setIntakeMessage("未能添加本地文件。请查看右侧错误日志后重试。");
      return;
    }

    const outputNames = tasks.map((task) =>
      outputNameFromPreview(task.outputPreview)
    );
    const createdTasks: LocalTask[] = [];

    for (const metadata of inspection.files) {
      const targetExtension = enabledImageExtensions.has(metadata.extension)
        ? imageTargetFormat
        : "pdf";
      const task = createTaskFromNativePathMetadata(
        metadata,
        [
          ...outputNames,
          ...createdTasks.map((item) =>
            outputNameFromPreview(item.outputPreview)
          )
        ],
        Date.now(),
        targetExtension
      );
      createdTasks.push(task);
    }

    if (createdTasks.length > 0) {
      setTasks((currentTasks) => [...createdTasks, ...currentTasks]);
      for (const task of createdTasks) {
        void planBackendOutput(
          task,
          enabledImageExtensions.has(task.extension) ? imageTargetFormat : "pdf"
        );
      }
    }

    if (inspection.rejected.length > 0) {
      setIntakeError(
        inspection.rejected
          .map(
            (item) =>
              `${item.sourcePath || "<空路径>"}\n${item.message}`
          )
          .join("\n\n")
      );
    }

    if (createdTasks.length > 0) {
      setIntakeMessage(
        inspection.rejected.length > 0
          ? `已通过原生本地路径添加 ${createdTasks.length} 个文件，另有 ${inspection.rejected.length} 项未添加。`
          : `已通过原生本地路径添加 ${createdTasks.length} 个文件。`
      );
    } else {
      setIntakeMessage("没有可添加的本地文件。文件夹和无效路径不会进入任务队列。");
    }
  }

  nativePathIntakeRef.current = addNativePaths;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function registerNativeDragDrop() {
      try {
        const stopListening = await getCurrentWebview().onDragDropEvent(
          (event) => {
            if (event.payload.type === "enter" || event.payload.type === "over") {
              setDragActive(true);
              return;
            }

            setDragActive(false);
            if (event.payload.type === "drop") {
              void nativePathIntakeRef.current(event.payload.paths);
            }
          }
        );

        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      } catch {
        // Browser-only development keeps the metadata-preview fallback below.
      }
    }

    void registerNativeDragDrop();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  async function selectNativeFiles() {
    setIntakeError("");

    try {
      const selected = await open({
        title: "选择本地文件",
        multiple: true,
        directory: false
      });
      const paths = Array.isArray(selected)
        ? selected
        : selected
          ? [selected]
          : [];
      await addNativePaths(paths);
    } catch {
      setIntakeMessage(
        "原生文件选择当前不可用，已切换到浏览器预览模式。预览任务没有本地路径，不能运行真实 PDF 或图片工具。"
      );
      fileInputRef.current?.click();
    }
  }

  function handleImageTargetChange(event: ChangeEvent<HTMLSelectElement>) {
    const targetFormat = event.currentTarget.value as EnabledImageFormat;
    setImageTargetFormat(targetFormat);

    setTasks((currentTasks) => {
      const reservedOutputNames = currentTasks
        .filter((task) => !selectedTaskIds.has(task.taskId))
        .map((task) => outputNameFromPreview(task.outputPreview));

      return currentTasks.map((task) => {
        if (
          !selectedTaskIds.has(task.taskId) ||
          !enabledImageExtensions.has(task.extension)
        ) {
          return task;
        }

        const outputName = getOutputName(
          `${getBaseName(task.displayName)}.${targetFormat}`,
          reservedOutputNames
        );
        reservedOutputNames.push(outputName);
        return {
          ...task,
          outputPreview: task.sourcePath
            ? task.outputPreview
            : `converted/${outputName}`
        };
      });
    });

    for (const task of selectedEnabledImageTasks) {
      if (task.sourceKind === "native-path" && task.sourcePath) {
        void planBackendOutput(task, targetFormat);
      }
    }
  }

  function handleInputChange(event: ChangeEvent<HTMLInputElement>) {
    if (event.currentTarget.files) {
      addPreviewFiles(event.currentTarget.files);
    }
    event.currentTarget.value = "";
  }

  function handleDrop(event: DragEvent<HTMLElement>) {
    event.preventDefault();
    setDragActive(false);
    addPreviewFiles(event.dataTransfer.files);
  }

  function retryTask(taskId: string) {
    cancelledTaskIdsRef.current.delete(taskId);
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        task.taskId === taskId
          ? {
              ...task,
              backendTaskId: undefined,
              status: "waiting",
              errorLog: ""
            }
          : task
      )
    );
  }

  async function cancelTask(taskId: string) {
    const task = tasks.find((candidate) => candidate.taskId === taskId);
    if (!task || task.status === "completed" || task.status === "cancelled") {
      return;
    }

    if (task.status !== "converting") {
      cancelledTaskIdsRef.current.add(taskId);
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === taskId
            ? {
                ...currentTask,
                backendTaskId: undefined,
                status: "cancelled",
                errorLog: ""
              }
            : currentTask
        )
      );
      setFolderMessage("等待任务已在本地取消，未启动任何转换进程。");
      return;
    }

    if (!task.backendTaskId) {
      const message =
        "该运行任务缺少后端任务 ID，无法确认进程已停止；请等待任务结束后重试。";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === taskId
            ? { ...currentTask, errorLog: message }
            : currentTask
        )
      );
      setFolderMessage(message);
      return;
    }

    const backendTaskId = task.backendTaskId;
    try {
      const response = await invoke<CancelTaskResponse>("cancel_task", {
        taskId: backendTaskId
      });

      if (!response.cancellationRequested && response.status !== "cancelled") {
        setFolderMessage("后端任务已经结束，无法再取消。其最终结果会显示在队列中。");
        return;
      }

      const linkedTaskIds = tasks
        .filter((candidate) => candidate.backendTaskId === backendTaskId)
        .map((candidate) => candidate.taskId);
      for (const linkedTaskId of linkedTaskIds) {
        cancelledTaskIdsRef.current.add(linkedTaskId);
      }

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.backendTaskId === backendTaskId
            ? {
                ...currentTask,
                status: "cancelled",
                errorLog: ""
              }
            : currentTask
        )
      );
      setFolderMessage(
        response.processTerminationRequested
          ? "任务已取消，正在运行的本地转换进程已终止。"
          : "任务已取消；后端未发现仍在运行的子进程。"
      );
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "后端取消请求失败。";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.backendTaskId === backendTaskId
            ? {
                ...currentTask,
                errorLog: `取消失败：${message}`
              }
            : currentTask
        )
      );
      setFolderMessage("未能确认本地进程已停止。任务仍保持处理中，请查看错误日志。");
    }
  }

  function removeTask(taskId: string) {
    const task = tasks.find((candidate) => candidate.taskId === taskId);
    if (task?.status === "converting") {
      setFolderMessage("运行中的任务不能直接移除。请先取消并等待后端确认。");
      return;
    }

    cancelledTaskIdsRef.current.delete(taskId);
    setTasks((currentTasks) =>
      currentTasks.filter((currentTask) => currentTask.taskId !== taskId)
    );
    setSelectedTaskIds((currentIds) => {
      const nextIds = new Set(currentIds);
      nextIds.delete(taskId);
      return nextIds;
    });
  }

  function clearCompletedTasks() {
    const completedTaskIds = new Set(
      tasks
        .filter((task) => task.status === "completed")
        .map((task) => task.taskId)
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

    const taskIds = new Set(mergeTasks.map((task) => task.taskId));
    const backendTaskId = createBackendTaskId(
      "qpdf-merge",
      mergeTasks[0].taskId
    );
    for (const taskId of taskIds) {
      cancelledTaskIdsRef.current.delete(taskId);
    }
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        taskIds.has(task.taskId)
          ? {
              ...task,
              backendTaskId,
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
      const response = await invoke<BackendTaskResponse<QpdfMergeResult>>(
        "qpdf_merge_pdfs",
        {
          taskId: backendTaskId,
          request: {
            sources: mergeTasks.map((task) => task.sourcePath),
            output: outputPlan.plannedOutputPath
          }
        }
      );
      const result = response.result;
      const resultStatus = backendResponseStatus(response);
      const log = formatMergeLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((task) => {
          if (!taskIds.has(task.taskId)) {
            return task;
          }

          const status =
            task.status === "cancelled" ? "cancelled" : resultStatus;
          return {
            ...task,
            backendTaskId: undefined,
            status,
            outputPreview:
              status === "completed"
                ? result.outputPath || outputPlan.plannedOutputPath
                : task.outputPreview,
            errorLog: status === "failed" ? log : ""
          };
        })
      );
      setFolderMessage(
        resultStatus === "cancelled"
          ? "PDF 合并已在本地取消，未保留未完成的输出。"
          : result.success
            ? `已在本地合并 ${mergeTasks.length} 个 PDF。输出：${result.outputPath}`
            : "PDF 合并失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "PDF 合并失败。";
      setTasks((currentTasks) =>
        currentTasks.map((task) => {
          if (!taskIds.has(task.taskId)) {
            return task;
          }
          if (task.status === "cancelled") {
            return { ...task, backendTaskId: undefined };
          }
          return {
            ...task,
            backendTaskId: undefined,
            status: "failed",
            errorLog: message
          };
        })
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

    const backendTaskId = createBackendTaskId("qpdf-split", task.taskId);
    cancelledTaskIdsRef.current.delete(task.taskId);
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...currentTask,
              backendTaskId,
              status: "converting",
              errorLog: ""
            }
          : currentTask
      )
    );

    try {
      const response = await invoke<BackendTaskResponse<QpdfSplitResult>>(
        "qpdf_split_pdf",
        {
          taskId: backendTaskId,
          request: {
            source: task.sourcePath,
            outputDirectory: buildSiblingDirectory(task.sourcePath, "converted"),
            filenamePrefix: `${getBaseName(task.displayName)}-page`
          }
        }
      );
      const result = response.result;
      const resultStatus = backendResponseStatus(response);
      const log = formatSplitLog(result);
      const outputPreview = result.success
        ? `${result.outputPaths.length} 个文件，位于 ${result.outputDirectory}`
        : task.outputPreview;

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          return {
            ...currentTask,
            backendTaskId: undefined,
            status,
            outputPreview: status === "completed" ? outputPreview : currentTask.outputPreview,
            errorLog: status === "failed" ? log : ""
          };
        })
      );
      setFolderMessage(
        resultStatus === "cancelled"
          ? "PDF 拆分已在本地取消，未保留未完成的拆分文件。"
          : result.success
            ? `已在本地拆分 PDF，生成 ${result.outputPaths.length} 个文件：${result.outputPaths.join(" | ")}`
            : "PDF 拆分失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "PDF 拆分失败。";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return {
            ...currentTask,
            backendTaskId: undefined,
            status: "failed",
            errorLog: message
          };
        })
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

    const backendTaskId = createBackendTaskId("qpdf-extract", task.taskId);
    cancelledTaskIdsRef.current.delete(task.taskId);
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...currentTask,
              backendTaskId,
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
      const response = await invoke<BackendTaskResponse<QpdfExtractResult>>(
        "qpdf_extract_pages",
        {
          taskId: backendTaskId,
          request: {
            source: task.sourcePath,
            pages,
            output: outputPlan.plannedOutputPath
          }
        }
      );
      const result = response.result;
      const resultStatus = backendResponseStatus(response);
      const log = formatExtractLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          return {
            ...currentTask,
            backendTaskId: undefined,
            status,
            outputPreview:
              status === "completed" ? result.outputPath : currentTask.outputPreview,
            errorLog: status === "failed" ? log : ""
          };
        })
      );
      setFolderMessage(
        resultStatus === "cancelled"
          ? "PDF 页面提取已在本地取消，未保留未完成的输出。"
          : result.success
            ? `已在本地提取页面 ${result.pages}。输出：${result.outputPath}`
            : "PDF 页面提取失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "PDF 页面提取失败。";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return {
            ...currentTask,
            backendTaskId: undefined,
            status: "failed",
            errorLog: message
          };
        })
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

    const backendTaskId = createBackendTaskId("qpdf-rotate", task.taskId);
    cancelledTaskIdsRef.current.delete(task.taskId);
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...currentTask,
              backendTaskId,
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
      const response = await invoke<BackendTaskResponse<QpdfRotateResult>>(
        "qpdf_rotate_pages",
        {
          taskId: backendTaskId,
          request: {
            source: task.sourcePath,
            pages: "",
            degrees,
            output: outputPlan.plannedOutputPath
          }
        }
      );
      const result = response.result;
      const resultStatus = backendResponseStatus(response);
      const log = formatRotateLog(result);

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          return {
            ...currentTask,
            backendTaskId: undefined,
            status,
            outputPreview:
              status === "completed" ? result.outputPath : currentTask.outputPreview,
            errorLog: status === "failed" ? log : ""
          };
        })
      );
      setFolderMessage(
        resultStatus === "cancelled"
          ? "PDF 旋转已在本地取消，未保留未完成的输出。"
          : result.success
            ? `已在本地旋转 PDF（${result.degrees}）。输出：${result.outputPath}`
            : "PDF 旋转失败。请查看失败任务的错误日志。"
      );
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "PDF 旋转失败。";
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return {
            ...currentTask,
            backendTaskId: undefined,
            status: "failed",
            errorLog: message
          };
        })
      );
      setFolderMessage("PDF 旋转失败。请查看失败任务的错误日志。");
    }
  }

  async function convertSelectedImages() {
    if (!canConvertSelectedImages) {
      setFolderMessage(imageToolsGuidance());
      return;
    }

    const conversionTasks = selectedRealLocalImageTasks;
    const targetFormat = imageTargetFormat;

    let successCount = 0;
    let failureCount = 0;
    let cancelledCount = 0;
    const outputPaths: string[] = [];

    for (const task of conversionTasks) {
      if (cancelledTaskIdsRef.current.has(task.taskId)) {
        cancelledCount += 1;
        continue;
      }

      if (!task.sourcePath) {
        failureCount += 1;
        continue;
      }

      const backendTaskId = createBackendTaskId(
        "image-convert",
        task.taskId
      );
      cancelledTaskIdsRef.current.delete(task.taskId);
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
            ? {
                ...currentTask,
                backendTaskId,
                status: "converting",
                errorLog: ""
              }
            : currentTask
        )
      );

      try {
        const response = await invoke<BackendTaskResponse<ImageConvertResult>>(
          "image_convert_file",
          {
            taskId: backendTaskId,
            request: {
              source: task.sourcePath,
              targetFormat
            }
          }
        );
        const result = response.result;
        const resultStatus = backendResponseStatus(response);
        const log = formatImageConvertLog(result);
        if (resultStatus === "cancelled") {
          cancelledCount += 1;
        } else if (result.success) {
          successCount += 1;
          outputPaths.push(result.outputPath);
        } else {
          failureCount += 1;
        }

        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }

            const status =
              currentTask.status === "cancelled"
                ? "cancelled"
                : resultStatus;
            return {
              ...currentTask,
              backendTaskId: undefined,
              status,
              outputPreview:
                status === "completed" && result.outputPath
                  ? result.outputPath
                  : currentTask.outputPreview,
              errorLog: status === "failed" ? log : ""
            };
          })
        );
      } catch (error) {
        const wasCancelled = cancelledTaskIdsRef.current.has(task.taskId);
        if (wasCancelled) {
          cancelledCount += 1;
        } else {
          failureCount += 1;
        }
        const message =
          typeof error === "string"
            ? error
            : error instanceof Error
              ? error.message
              : "图片转换失败。";
        setTasks((currentTasks) =>
          currentTasks.map((currentTask) =>
            currentTask.taskId === task.taskId
              ? {
                  ...currentTask,
                  backendTaskId: undefined,
                  status:
                    currentTask.status === "cancelled" || wasCancelled
                      ? "cancelled"
                      : "failed",
                  errorLog:
                    currentTask.status === "cancelled" || wasCancelled
                      ? ""
                      : message
                }
              : currentTask
          )
        );
      }
    }

    if (failureCount === 0 && cancelledCount === 0) {
      setFolderMessage(
        `已在本机完成 ${successCount} 张图片转换。输出：${outputPaths.join(" | ")}`
      );
    } else {
      setFolderMessage(
        `图片转换结束：成功 ${successCount}，失败 ${failureCount}，已取消 ${cancelledCount}。${failureCount > 0 ? "请查看失败任务的错误日志。" : "未保留已取消任务的未完成输出。"}`
      );
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
            {qpdfAvailable && imageEngineAvailable
              ? "PDF 与图片工具已启用"
              : qpdfAvailable
                ? "PDF qpdf 工具已启用"
                : imageEngineAvailable
                  ? "图片转换已启用"
                  : "转换未启用"}
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
            <strong>本地工具预览</strong>
            <span>PDF 工具与 JPG、PNG、WebP 图片转换已启用。Office 等其他能力仍未启用。</span>
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
              <p>原生入口仅读取本地路径和文件元数据，不读取文件内容。</p>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => void selectNativeFiles()}>
                选择文件
              </button>
              <button
                type="button"
                className="secondary-button"
                onClick={() =>
                  setFolderMessage(
                    "文件夹批量导入尚未启用。当前请使用“选择文件”或将文件直接拖入窗口。"
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

          {intakeMessage ? <p className="inline-note">{intakeMessage}</p> : null}
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

          <section className="image-tools-panel" aria-label="Preview 0.2 图片转换">
            <div className="pdf-tools-header">
              <div>
                <p className="section-kicker">Preview 0.2 · 本地功能预览</p>
                <h2>图片格式转换</h2>
                <p>
                  {imageEngineAvailable
                    ? "JPG、PNG、WebP 已启用，转换仅在本机执行。"
                    : "图片引擎不可用，转换操作保持禁用。"}
                </p>
              </div>
              <div className="selection-tools">
                <button
                  type="button"
                  className="secondary-button"
                  onClick={selectImageTasks}
                  disabled={tasks.every(
                    (task) => !enabledImageExtensions.has(task.extension)
                  )}
                >
                  选择全部图片
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={clearTaskSelection}
                  disabled={selectedTaskIds.size === 0}
                >
                  清除选择
                </button>
              </div>
            </div>

            <div className="pdf-tools-status">
              <strong>{imageEngineAvailable ? "image-engine 可用" : "image-engine 不可用"}</strong>
              <span>{imageToolsGuidance()}</span>
            </div>

            <div className="image-format-list" aria-label="图片格式状态">
              <span className="format-enabled">JPG</span>
              <span className="format-enabled">PNG</span>
              <span className="format-enabled">WebP</span>
              <span className="planned-later">AVIF 稍后</span>
              <span className="planned-later">TIFF 稍后</span>
              <span className="planned-later">HEIC 稍后</span>
            </div>

            <div className="pdf-tool-grid image-tool-grid">
              <article className="pdf-tool-card image-tool-card">
                <h3>图片格式转换</h3>
                <p>选择任务和输出格式。结果写入源文件旁边的 converted 文件夹，不覆盖原文件。</p>
                <ul className="image-conversion-matrix" aria-label="已启用的图片转换方向">
                  <li>JPG/JPEG → PNG、WebP</li>
                  <li>PNG → JPG、WebP</li>
                  <li>WebP → JPG、PNG</li>
                </ul>
                <label className="image-target-field">
                  <span>输出格式</span>
                  <select
                    value={imageTargetFormat}
                    onChange={handleImageTargetChange}
                    aria-label="图片输出格式"
                  >
                    <option value="jpg">JPG</option>
                    <option value="png">PNG</option>
                    <option value="webp">WebP</option>
                  </select>
                </label>
                <button
                  type="button"
                  onClick={() => void convertSelectedImages()}
                  disabled={!canConvertSelectedImages}
                >
                  转换所选图片
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>图片压缩</h3>
                <p>高清、平衡、小体积等本地压缩预设仍在规划中。</p>
                <button type="button" disabled>
                  稍后启用
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>图片改尺寸</h3>
                <p>按宽度、高度或等比规则批量调整尺寸仍在规划中。</p>
                <button type="button" disabled>
                  稍后启用
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>移除图片元数据</h3>
                <p>独立移除 EXIF 等图片元数据的工具仍在规划中。</p>
                <button type="button" disabled>
                  稍后启用
                </button>
              </article>
            </div>
          </section>

          <section className="queue-panel" aria-label="任务队列">
            <div className="queue-heading">
              <div>
                <h2>任务队列</h2>
                <span>
                  {tasks.length} 个任务 · {realLocalPdfTasks.length} 个本地 PDF · {realLocalImageTasks.length} 张本地图片 · {selectedTasks.length} 个已选
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
                  <article className="task-row" role="row" key={task.taskId}>
                    <label className="select-cell" aria-label={`选择 ${task.displayName}`}>
                      <input
                        type="checkbox"
                        checked={selectedTaskIds.has(task.taskId)}
                        onChange={() => toggleTaskSelection(task.taskId)}
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
                      <small
                        className={
                          task.sourceKind === "native-path" ? "" : "path-warning"
                        }
                        title={
                          task.sourceKind === "native-path"
                            ? task.sourcePreview
                            : "只有预览元数据；真实 PDF 和图片工具需要原生本地路径。"
                        }
                      >
                        {task.sourceKind === "native-path"
                          ? `${task.sourcePreview} · 原生本地路径`
                          : `${task.sourcePreview} · 仅有预览元数据，真实工具需要本地路径`}
                      </small>
                    </div>
                    <div className="task-actions">
                      <button
                        type="button"
                        className="small-button"
                        onClick={() => void cancelTask(task.taskId)}
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
                        onClick={() => retryTask(task.taskId)}
                        disabled={task.status !== "failed"}
                      >
                        重试
                      </button>
                      <button
                        type="button"
                        className="small-button ghost-button"
                        onClick={() => removeTask(task.taskId)}
                        disabled={task.status === "converting"}
                        title={
                          task.status === "converting"
                            ? "请先取消运行中的后端任务，再移除。"
                            : "从队列中移除任务"
                        }
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
                : qpdfAvailable || imageEngineAvailable
                  ? "PDF 工具与 JPG、PNG、WebP 图片转换会按可用引擎状态在本机启用。"
                  : "转换引擎不可用。"}
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
                "暂无错误日志。PDF 操作或图片转换失败时会显示在这里。"}
            </pre>
          </section>

          <section className="inspector-card">
            <h2>本地隐私</h2>
            <ul>
              <li>不上传。</li>
              <li>不会修改原文件。</li>
              <li>PDF 合并、拆分、页面提取和旋转仅使用内置本地 qpdf。</li>
              <li>JPG、PNG、WebP 转换仅使用内置本地 image-engine。</li>
              <li>Office、其他图片格式与其他图片操作仍未启用。</li>
            </ul>
          </section>
        </aside>
      </div>
    </main>
  );
}

export default App;
