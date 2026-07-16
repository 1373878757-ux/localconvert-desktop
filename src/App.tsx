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
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  LocalTask,
  NativePathMetadata,
  TaskReportStatus,
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

type ImageResizeResult = {
  success: boolean;
  operation: "resize";
  sourcePath: string;
  outputPath: string;
  sourceFormat: string;
  mode: ResizeMode;
  maxWidth: number | null;
  maxHeight: number | null;
  sourceWidth: number;
  sourceHeight: number;
  outputWidth: number;
  outputHeight: number;
  resized: boolean;
  outputBytes: number;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type ImageCompressResult = {
  success: boolean;
  operation: "compress";
  sourcePath: string;
  plannedOutputPath: string;
  outputPath: string;
  sourceFormat: string;
  quality: number | null;
  lossless: boolean;
  published: boolean;
  smaller: boolean;
  sourceBytes: number;
  encodedBytes: number;
  outputBytes: number;
  savedBytes: number;
  width: number;
  height: number;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
  message: string;
};

type ImageCleanMetadataResult = {
  success: boolean;
  operation: "clean-metadata";
  sourcePath: string;
  plannedOutputPath: string;
  outputPath: string;
  sourceFormat: string;
  changed: boolean;
  published: boolean;
  metadataItemsRemoved: number;
  removedKinds: string[];
  pixelDataPreserved: boolean;
  reencoded: boolean;
  sourceBytes: number;
  outputBytes: number;
  sourceWidth: number;
  sourceHeight: number;
  outputWidth: number;
  outputHeight: number;
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
type ResizeMode = "fit" | "width" | "height";

type ResizeInputValidation = {
  valid: boolean;
  maxWidth?: number;
  maxHeight?: number;
  message: string;
};

type CompressionQualityValidation = {
  valid: boolean;
  value?: number;
  message: string;
};

type ReportFormat = "csv" | "json";

type TaskReportRecord = {
  taskId: string;
  operationType: string;
  sourcePath: string;
  sourceName: string;
  sourceExtension: string;
  outputPath: string;
  outputName: string;
  outputExtension: string;
  status: TaskReportStatus;
  startedAt: string | null;
  finishedAt: string | null;
  durationMs: number | null;
  sourceBytes: number | null;
  outputBytes: number | null;
  savedBytes: number | null;
  savedPercent: number | null;
  message: string;
};

type ExportTaskReportResult = {
  success: boolean;
  destinationPath: string;
  format: ReportFormat;
  bytesWritten: number;
  taskCount: number;
  message: string;
};

type RevealLocalFileResult = {
  success: boolean;
  targetPath: string;
  containingFolderPath: string;
  message: string;
};

type CopyErrorSummaryResult = {
  success: boolean;
  summary: string;
  message: string;
};

type TaskReportCompletion = {
  status: TaskReportStatus;
  message: string;
  outputPath?: string;
  outputName?: string;
  outputExtension?: string;
  outputBytes?: number;
  savedBytes?: number;
  savedPercent?: number;
  outputLocationPath?: string;
};

const maxResizeDimension = 16_384;
const maxResizePixels = 64_000_000;
const minCompressionQuality = 40;
const maxCompressionQuality = 95;
const appVersion = "0.6.1";

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

function startTaskOperation(
  task: LocalTask,
  operationType: string,
  startedAt: number
): LocalTask {
  return {
    ...task,
    operationType,
    startedAt,
    finishedAt: undefined,
    reportStatus: undefined,
    reportOutputPath: undefined,
    reportOutputName: undefined,
    reportOutputExtension: undefined,
    reportOutputBytes: undefined,
    reportSavedBytes: undefined,
    reportSavedPercent: undefined,
    reportMessage: undefined,
    outputLocationPath: undefined
  };
}

function finishTaskOperation(
  task: LocalTask,
  completion: TaskReportCompletion,
  finishedAt: number
): LocalTask {
  if (task.reportStatus === "cancelled") {
    return task;
  }

  return {
    ...task,
    finishedAt,
    reportStatus: completion.status,
    reportOutputPath: completion.outputPath,
    reportOutputName: completion.outputName,
    reportOutputExtension: completion.outputExtension,
    reportOutputBytes: completion.outputBytes,
    reportSavedBytes: completion.savedBytes,
    reportSavedPercent: completion.savedPercent,
    reportMessage: completion.message,
    outputLocationPath:
      completion.outputLocationPath ?? completion.outputPath ?? undefined
  };
}

function clearTaskReport(task: LocalTask): LocalTask {
  return {
    ...task,
    operationType: undefined,
    startedAt: undefined,
    finishedAt: undefined,
    reportStatus: undefined,
    reportOutputPath: undefined,
    reportOutputName: undefined,
    reportOutputExtension: undefined,
    reportOutputBytes: undefined,
    reportSavedBytes: undefined,
    reportSavedPercent: undefined,
    reportMessage: undefined,
    outputLocationPath: undefined
  };
}

function copyableTaskError(task: LocalTask): string {
  const eligibleStatus =
    task.reportStatus === "failed" ||
    task.reportStatus === "unsupported" ||
    task.reportStatus === "skipped";

  if (!eligibleStatus) {
    return "";
  }

  return (task.reportMessage || task.errorLog).trim();
}

function fileNameFromPath(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  return normalized.split("/").pop() || "";
}

function buildTaskReportRecord(task: LocalTask): TaskReportRecord | null {
  if (!task.reportStatus || !task.operationType) {
    return null;
  }

  const outputPath = task.reportOutputPath ?? "";
  const outputName = task.reportOutputName ?? fileNameFromPath(outputPath);
  const outputExtension =
    task.reportOutputExtension ?? (outputName ? getExtension(outputName) : "");
  const durationMs =
    task.startedAt !== undefined && task.finishedAt !== undefined
      ? Math.max(0, task.finishedAt - task.startedAt)
      : null;

  return {
    taskId: task.taskId,
    operationType: task.operationType,
    sourcePath: task.sourcePath ?? "",
    sourceName: task.displayName,
    sourceExtension: task.extension === "未知" ? "" : task.extension,
    outputPath,
    outputName,
    outputExtension: outputExtension === "未知" ? "" : outputExtension,
    status: task.reportStatus,
    startedAt:
      task.startedAt === undefined ? null : new Date(task.startedAt).toISOString(),
    finishedAt:
      task.finishedAt === undefined
        ? null
        : new Date(task.finishedAt).toISOString(),
    durationMs,
    sourceBytes: Number.isFinite(task.size) ? task.size : null,
    outputBytes: task.reportOutputBytes ?? null,
    savedBytes: task.reportSavedBytes ?? null,
    savedPercent: task.reportSavedPercent ?? null,
    message: task.reportMessage ?? task.errorLog
  };
}

function reportFilename(format: ReportFormat, now = new Date()): string {
  const pad = (value: number) => value.toString().padStart(2, "0");
  const date = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  return `localconvert-report-${date}-${time}.${format}`;
}

function parseResizeDimension(value: string, label: string) {
  const normalized = value.trim();
  if (!/^\d+$/.test(normalized)) {
    return { value: undefined, message: `${label}必须是正整数。` };
  }

  const dimension = Number(normalized);
  if (!Number.isSafeInteger(dimension) || dimension <= 0) {
    return { value: undefined, message: `${label}必须大于 0。` };
  }
  if (dimension > maxResizeDimension) {
    return {
      value: undefined,
      message: `${label}不能超过 ${maxResizeDimension} 像素。`
    };
  }

  return { value: dimension, message: "" };
}

function validateResizeInputs(
  mode: ResizeMode,
  widthInput: string,
  heightInput: string
): ResizeInputValidation {
  if (mode === "fit" || mode === "width") {
    const width = parseResizeDimension(widthInput, "最大宽度");
    if (!width.value) {
      return { valid: false, message: width.message };
    }

    if (mode === "width") {
      return {
        valid: true,
        maxWidth: width.value,
        message: `将按最大宽度 ${width.value} 像素等比缩小，不会放大较小图片。`
      };
    }

    const height = parseResizeDimension(heightInput, "最大高度");
    if (!height.value) {
      return { valid: false, message: height.message };
    }
    if (width.value * height.value > maxResizePixels) {
      return {
        valid: false,
        message: `宽高范围不能超过 ${maxResizePixels.toLocaleString("zh-CN")} 像素的安全上限。`
      };
    }

    return {
      valid: true,
      maxWidth: width.value,
      maxHeight: height.value,
      message: `将适应 ${width.value} × ${height.value} 像素范围，保持宽高比且不放大。`
    };
  }

  const height = parseResizeDimension(heightInput, "最大高度");
  if (!height.value) {
    return { valid: false, message: height.message };
  }
  return {
    valid: true,
    maxHeight: height.value,
    message: `将按最大高度 ${height.value} 像素等比缩小，不会放大较小图片。`
  };
}

function validateCompressionQuality(
  value: string,
  label: string
): CompressionQualityValidation {
  const normalized = value.trim();
  if (!/^\d+$/.test(normalized)) {
    return { valid: false, message: `${label}必须是整数。` };
  }

  const quality = Number(normalized);
  if (
    !Number.isSafeInteger(quality) ||
    quality < minCompressionQuality ||
    quality > maxCompressionQuality
  ) {
    return {
      valid: false,
      message: `${label}必须在 ${minCompressionQuality} 到 ${maxCompressionQuality} 之间。`
    };
  }

  return { valid: true, value: quality, message: "" };
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
  const [resizeMode, setResizeMode] = useState<ResizeMode>("fit");
  const [resizeWidth, setResizeWidth] = useState("1920");
  const [resizeHeight, setResizeHeight] = useState("1080");
  const [jpegCompressionQuality, setJpegCompressionQuality] = useState("82");
  const [webpCompressionQuality, setWebpCompressionQuality] = useState("80");
  const [reportFormat, setReportFormat] = useState<ReportFormat>("csv");
  const [reportExporting, setReportExporting] = useState(false);
  const [reportExportMessage, setReportExportMessage] = useState("");
  const [reportExportError, setReportExportError] = useState("");
  const [lastReportPath, setLastReportPath] = useState("");
  const [copiedTaskId, setCopiedTaskId] = useState("");
  const [showClearHistoryConfirmation, setShowClearHistoryConfirmation] =
    useState(false);
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
        for (let attempt = 0; attempt < 240; attempt += 1) {
          const startup = await invoke<StartupStatus>("startup_status");
          if (startup.completed) {
            if (startup.error) {
              setStartupError(startup.error);
            }
            setSelfCheck(startup.selfCheck ?? fallbackSelfCheck);
            return;
          }

          await new Promise<void>((resolve) => {
            window.setTimeout(resolve, 50);
          });
        }

        setSelfCheck(fallbackSelfCheck);
        setStartupError("等待启动引擎自检完成超时。");
        return;
      } catch {
        setSelfCheck(fallbackSelfCheck);
        setStartupError("启动状态不可用，未重复运行引擎自检。");
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

  const reportRecords = useMemo(
    () =>
      tasks
        .map(buildTaskReportRecord)
        .filter((record): record is TaskReportRecord => record !== null),
    [tasks]
  );

  const selectedTaskWithError = tasks.find((task) => task.errorLog);
  const inspectorErrorLog =
    (reportExportError ? `报告导出问题:\n${reportExportError}` : "") ||
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
    { label: "图片工具", enabled: imageEngineAvailable, note: imageEngineAvailable ? "可用" : "不可用" },
    { label: "批量队列", enabled: true, note: "本地" },
    { label: "文档转 PDF", enabled: false, note: "稍后" },
    { label: "图片压缩", enabled: imageEngineAvailable, note: imageEngineAvailable ? "Preview" : "不可用" },
    { label: "隐私清理", enabled: imageEngineAvailable, note: imageEngineAvailable ? "Preview 0.5" : "不可用" },
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
  const resizeInputValidation = validateResizeInputs(
    resizeMode,
    resizeWidth,
    resizeHeight
  );
  const canResizeSelectedImages =
    imageEngineAvailable &&
    resizeInputValidation.valid &&
    selectedTasks.length > 0 &&
    selectedUnsupportedImageTasks.length === 0 &&
    selectedImageTasksWithoutPath.length === 0 &&
    selectedCancelledImageTasks.length === 0 &&
    selectedConvertingImageTasks.length === 0;
  const selectedJpegTasks = selectedEnabledImageTasks.filter(
    (task) => canonicalImageFormat(task.extension) === "jpg"
  );
  const selectedWebpTasks = selectedEnabledImageTasks.filter(
    (task) => canonicalImageFormat(task.extension) === "webp"
  );
  const jpegQualityValidation = validateCompressionQuality(
    jpegCompressionQuality,
    "JPEG 质量"
  );
  const webpQualityValidation = validateCompressionQuality(
    webpCompressionQuality,
    "WebP 质量"
  );
  const canCompressSelectedImages =
    imageEngineAvailable &&
    selectedTasks.length > 0 &&
    selectedUnsupportedImageTasks.length === 0 &&
    selectedImageTasksWithoutPath.length === 0 &&
    selectedCancelledImageTasks.length === 0 &&
    selectedConvertingImageTasks.length === 0 &&
    (selectedJpegTasks.length === 0 || jpegQualityValidation.valid) &&
    (selectedWebpTasks.length === 0 || webpQualityValidation.valid);
  const canCleanSelectedImageMetadata =
    imageEngineAvailable &&
    selectedTasks.length > 0 &&
    selectedUnsupportedImageTasks.length === 0 &&
    selectedImageTasksWithoutPath.length === 0 &&
    selectedCancelledImageTasks.length === 0 &&
    selectedConvertingImageTasks.length === 0;

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

  function formatImageResizeLog(result: ImageResizeResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `输出: ${result.outputPath || "未写入"}`,
      `格式: ${result.sourceFormat || "未知"}`,
      `模式: ${result.mode || "未知"}`,
      `源尺寸: ${result.sourceWidth > 0 && result.sourceHeight > 0 ? `${result.sourceWidth} × ${result.sourceHeight}` : "未验证"}`,
      `输出尺寸: ${result.outputWidth > 0 && result.outputHeight > 0 ? `${result.outputWidth} × ${result.outputHeight}` : "未验证"}`,
      `实际缩小: ${result.resized ? "是" : "否（未放大）"}`,
      `输出大小: ${result.outputBytes} 字节`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatImageCompressLog(result: ImageCompressResult) {
    const savedPercent =
      result.sourceBytes > 0 && result.savedBytes > 0
        ? ((result.savedBytes / result.sourceBytes) * 100).toFixed(1)
        : "0.0";
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `计划输出: ${result.plannedOutputPath || "未规划"}`,
      `实际输出: ${result.outputPath || "未生成"}`,
      `格式: ${result.sourceFormat || "未知"}`,
      `模式: ${result.lossless ? "PNG 无损优化" : `质量 ${result.quality ?? "未知"}`}`,
      `尺寸: ${result.width > 0 && result.height > 0 ? `${result.width} × ${result.height}` : "未验证"}`,
      `源大小: ${formatBytes(result.sourceBytes)}`,
      `编码结果: ${formatBytes(result.encodedBytes)}`,
      `已发布: ${result.published ? "是" : "否"}`,
      `节省: ${formatBytes(result.savedBytes)} (${savedPercent}%)`,
      `退出码: ${result.exitCode ?? "无"}`,
      `是否超时: ${result.timedOut ? "是" : "否"}`,
      result.stdout ? `stdout:\n${result.stdout}` : "stdout: <空>",
      result.stderr ? `stderr:\n${result.stderr}` : "stderr: <空>"
    ].join("\n");
  }

  function formatImageMetadataCleanupLog(result: ImageCleanMetadataResult) {
    return [
      `引擎消息: ${result.message}`,
      `源文件: ${result.sourcePath || "不可用"}`,
      `计划输出: ${result.plannedOutputPath || "未规划"}`,
      `实际输出: ${result.outputPath || "未生成"}`,
      `格式: ${result.sourceFormat || "未知"}`,
      `已清理: ${result.changed ? "是" : "否"}`,
      `已移除项目: ${result.metadataItemsRemoved}`,
      `类型: ${result.removedKinds.length > 0 ? result.removedKinds.join("、") : "未发现"}`,
      `像素编码保持: ${result.pixelDataPreserved ? "是" : "否"}`,
      `高质量重编码: ${result.reencoded ? "是（用于固化 JPEG 方向）" : "否"}`,
      `源尺寸: ${result.sourceWidth > 0 && result.sourceHeight > 0 ? `${result.sourceWidth} × ${result.sourceHeight}` : "未验证"}`,
      `输出尺寸: ${result.outputWidth > 0 && result.outputHeight > 0 ? `${result.outputWidth} × ${result.outputHeight}` : "未生成"}`,
      `源大小: ${formatBytes(result.sourceBytes)}`,
      `输出大小: ${formatBytes(result.outputBytes)}`,
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
      return "image-engine 可用，JPG、PNG、WebP 本地转换、改尺寸、压缩与元数据清理已通过自检。";
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
      return `所选任务中有 ${selectedHeicImageTasks.length} 个 HEIC 文件。当前 Preview 未启用 HEIC 解码、方向处理或格式转换，不会启动任务。`;
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

  function imageResizeGuidance() {
    if (!imageEngineAvailable) {
      return "内置 image-engine 不可用，图片改尺寸保持禁用。";
    }

    if (selectedTasks.length === 0) {
      return "请在任务队列中选择 JPG、JPEG、PNG 或 WebP 图片。";
    }

    if (selectedHeicImageTasks.length > 0) {
      return `所选任务中有 ${selectedHeicImageTasks.length} 个 HEIC 文件。当前 Preview 未启用 HEIC 解码、方向处理或改尺寸，不会启动任务。`;
    }

    if (selectedUnsupportedImageTasks.length > 0) {
      return "所选任务中含有不支持的格式。图片改尺寸仅启用 JPG/JPEG、PNG 和 WebP。";
    }

    if (selectedImageTasksWithoutPath.length > 0) {
      return "部分图片只有预览元数据。真实改尺寸需要通过原生“选择文件”或桌面拖放重新导入。";
    }

    if (selectedCancelledImageTasks.length > 0) {
      return "已取消的图片任务不能改尺寸，请先重试或移除。";
    }

    if (selectedConvertingImageTasks.length > 0) {
      return "所选图片中已有任务正在处理，请等待完成。";
    }

    if (!resizeInputValidation.valid) {
      return resizeInputValidation.message;
    }

    return `已选择 ${selectedRealLocalImageTasks.length} 张本地图片。${resizeInputValidation.message}`;
  }

  function imageCompressionGuidance() {
    if (!imageEngineAvailable) {
      return "内置 image-engine 不可用，图片压缩保持禁用。";
    }

    if (selectedTasks.length === 0) {
      return "请在任务队列中选择 JPG、JPEG、PNG 或 WebP 图片。";
    }

    if (selectedHeicImageTasks.length > 0) {
      return `所选任务中有 ${selectedHeicImageTasks.length} 个 HEIC 文件。当前 Preview 未启用 HEIC 解码、方向处理或压缩，不会启动任务。`;
    }

    if (selectedUnsupportedImageTasks.length > 0) {
      return "所选任务中含有不支持的格式。图片压缩仅启用 JPG/JPEG、PNG 和 WebP。";
    }

    if (selectedImageTasksWithoutPath.length > 0) {
      return "部分图片只有预览元数据。真实压缩需要通过原生“选择文件”或桌面拖放重新导入。";
    }

    if (selectedCancelledImageTasks.length > 0) {
      return "已取消的图片任务不能压缩，请先重试或移除。";
    }

    if (selectedConvertingImageTasks.length > 0) {
      return "所选图片中已有任务正在处理，请等待完成。";
    }

    if (selectedJpegTasks.length > 0 && !jpegQualityValidation.valid) {
      return jpegQualityValidation.message;
    }

    if (selectedWebpTasks.length > 0 && !webpQualityValidation.valid) {
      return webpQualityValidation.message;
    }

    return `已选择 ${selectedRealLocalImageTasks.length} 张本地图片。JPEG 质量 ${jpegQualityValidation.value ?? 82}，WebP 质量 ${webpQualityValidation.value ?? 80}，PNG 固定无损优化。仅在结果更小时生成新文件。`;
  }

  function imageMetadataCleanupGuidance() {
    if (!imageEngineAvailable) {
      return "内置 image-engine 不可用，图片元数据清理保持禁用。";
    }
    if (selectedTasks.length === 0) {
      return "请在任务队列中选择 JPG、JPEG、PNG 或 WebP 图片。";
    }
    if (selectedHeicImageTasks.length > 0) {
      return `所选任务中有 ${selectedHeicImageTasks.length} 个 HEIC 文件。当前 Preview 未启用 HEIC 元数据或方向处理，不会启动清理。`;
    }
    if (selectedUnsupportedImageTasks.length > 0) {
      return "所选任务中含有不支持的格式。元数据清理仅启用 JPG/JPEG、PNG 和 WebP。";
    }
    if (selectedImageTasksWithoutPath.length > 0) {
      return "部分图片只有预览元数据。真实清理需要通过原生“选择文件”或桌面拖放重新导入。";
    }
    if (selectedCancelledImageTasks.length > 0) {
      return "已取消的图片任务不能清理，请先重试或移除。";
    }
    if (selectedConvertingImageTasks.length > 0) {
      return "所选图片中已有任务正在处理，请等待完成。";
    }
    return `已选择 ${selectedRealLocalImageTasks.length} 张本地图片。将尽力移除常见隐私元数据；若未发现可清理内容，不生成新文件。`;
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

  async function exportTaskReport() {
    if (reportRecords.length === 0) {
      setReportExportMessage("");
      setReportExportError("没有可导出的任务结果。");
      return;
    }

    setReportExporting(true);
    setReportExportMessage("");
    setReportExportError("");
    const generatedAt = new Date();

    try {
      const destinationPath = await save({
        title: "导出处理报告",
        defaultPath: reportFilename(reportFormat, generatedAt),
        filters: [
          {
            name: reportFormat === "csv" ? "CSV 报告" : "JSON 报告",
            extensions: [reportFormat]
          }
        ]
      });

      if (!destinationPath) {
        setReportExportMessage("已取消导出，未写入报告。任务记录保持不变。");
        return;
      }

      const result = await invoke<ExportTaskReportResult>("export_task_report", {
        request: {
          destinationPath,
          format: reportFormat,
          appVersion,
          generatedAt: generatedAt.toISOString(),
          tasks: reportRecords
        }
      });

      setReportExportMessage(
        `${result.message}：${result.destinationPath}（${result.taskCount} 条，${formatBytes(result.bytesWritten)}）`
      );
      setLastReportPath(result.destinationPath);
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "报告写入失败。";
      setReportExportError(message);
    } finally {
      setReportExporting(false);
    }
  }

  async function revealTaskOutput(task: LocalTask) {
    if (!task.outputLocationPath) {
      setFolderMessage("该任务没有可打开的已发布输出文件。");
      return;
    }

    try {
      const result = await invoke<RevealLocalFileResult>("reveal_local_file", {
        path: task.outputLocationPath
      });
      setFolderMessage(`${result.message} ${result.targetPath}`);
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "无法打开输出位置。";
      setFolderMessage(message);
    }
  }

  async function revealLastReport() {
    if (!lastReportPath) {
      setReportExportError("当前会话还没有成功导出的报告。");
      return;
    }

    setReportExportError("");
    try {
      const result = await invoke<RevealLocalFileResult>("reveal_local_file", {
        path: lastReportPath
      });
      setReportExportMessage(`${result.message} ${result.targetPath}`);
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "无法打开报告位置。";
      setReportExportError(message);
    }
  }

  async function copyTaskError(task: LocalTask) {
    const message = copyableTaskError(task);
    if (!message) {
      setFolderMessage("该任务没有可复制的错误摘要。");
      return;
    }

    try {
      const result = await invoke<CopyErrorSummaryResult>("copy_error_summary", {
        message
      });
      setCopiedTaskId(task.taskId);
      setFolderMessage(`${result.message}：${result.summary}`);
      window.setTimeout(() => {
        setCopiedTaskId((currentTaskId) =>
          currentTaskId === task.taskId ? "" : currentTaskId
        );
      }, 1800);
    } catch (error) {
      const message =
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "剪贴板当前不可用。";
      setFolderMessage(message);
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
          ? clearTaskReport({
              ...task,
              backendTaskId: undefined,
              status: "waiting",
              errorLog: ""
            })
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
      const finishedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== taskId) {
            return currentTask;
          }
          const reportTask = currentTask.operationType
            ? currentTask
            : startTaskOperation(
                currentTask,
                "queue_cancel",
                currentTask.createdAt
              );
          return finishTaskOperation(
            {
              ...reportTask,
              backendTaskId: undefined,
              status: "cancelled",
              errorLog: ""
            },
            {
              status: "cancelled",
              message: "等待任务已在本地取消，未启动任何转换进程。"
            },
            finishedAt
          );
        })
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

      const cancellationMessage = response.processTerminationRequested
        ? "任务已取消，正在运行的本地转换进程已终止。"
        : "任务已取消；后端未发现仍在运行的子进程。";
      const finishedAt = Date.now();

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.backendTaskId === backendTaskId
            ? finishTaskOperation(
                {
                  ...currentTask,
                  status: "cancelled",
                  errorLog: ""
                },
                { status: "cancelled", message: cancellationMessage },
                finishedAt
              )
            : currentTask
        )
      );
      setFolderMessage(cancellationMessage);
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

  function clearTaskHistory() {
    if (summary.converting > 0) {
      setFolderMessage("请先取消并等待所有运行中的任务结束，再清空记录。");
      return;
    }

    setShowClearHistoryConfirmation(true);
  }

  function confirmClearTaskHistory() {
    setTasks([]);
    setSelectedTaskIds(new Set());
    cancelledTaskIdsRef.current.clear();
    setCopiedTaskId("");
    setReportExportMessage("");
    setReportExportError("");
    setShowClearHistoryConfirmation(false);
    setFolderMessage("已清空当前界面记录，未删除任何源文件、输出文件或报告。");
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
    const startedAt = Date.now();
    setTasks((currentTasks) =>
      currentTasks.map((task) =>
        taskIds.has(task.taskId)
          ? {
              ...startTaskOperation(task, "pdf_merge", startedAt),
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
      const finishedAt = Date.now();

      setTasks((currentTasks) =>
        currentTasks.map((task) => {
          if (!taskIds.has(task.taskId)) {
            return task;
          }

          const status =
            task.status === "cancelled" ? "cancelled" : resultStatus;
          const outputPath =
            status === "completed"
              ? result.outputPath || outputPlan.plannedOutputPath
              : "";
          return finishTaskOperation(
            {
              ...task,
              backendTaskId: undefined,
              status,
              outputPreview: outputPath || task.outputPreview,
              errorLog: status === "failed" ? log : ""
            },
            {
              status:
                status === "cancelled"
                  ? "cancelled"
                  : status === "completed"
                    ? "success"
                    : "failed",
              message: result.message,
              outputPath,
              outputName: fileNameFromPath(outputPath),
              outputExtension: outputPath ? "pdf" : "",
              outputBytes: result.outputBytes
            },
            finishedAt
          );
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
      const finishedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((task) => {
          if (!taskIds.has(task.taskId)) {
            return task;
          }
          if (task.status === "cancelled") {
            return { ...task, backendTaskId: undefined };
          }
          return finishTaskOperation(
            {
              ...task,
              backendTaskId: undefined,
              status: "failed",
              errorLog: message
            },
            { status: "failed", message },
            finishedAt
          );
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
    const startedAt = Date.now();
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...startTaskOperation(currentTask, "pdf_split", startedAt),
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
      const finishedAt = Date.now();

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          const outputPaths = status === "completed" ? result.outputPaths : [];
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status,
              outputPreview:
                status === "completed" ? outputPreview : currentTask.outputPreview,
              errorLog: status === "failed" ? log : ""
            },
            {
              status:
                status === "cancelled"
                  ? "cancelled"
                  : status === "completed"
                    ? "success"
                    : "failed",
              message: result.message,
              outputPath: outputPaths.join(" | "),
              outputName: outputPaths.map(fileNameFromPath).join(" | "),
              outputExtension: outputPaths.length > 0 ? "pdf" : "",
              outputBytes: result.outputBytes,
              outputLocationPath: outputPaths[0]
            },
            finishedAt
          );
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
      const finishedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status: "failed",
              errorLog: message
            },
            { status: "failed", message },
            finishedAt
          );
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
    const startedAt = Date.now();
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...startTaskOperation(currentTask, "pdf_extract_pages", startedAt),
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
      const finishedAt = Date.now();

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          const outputPath = status === "completed" ? result.outputPath : "";
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status,
              outputPreview: outputPath || currentTask.outputPreview,
              errorLog: status === "failed" ? log : ""
            },
            {
              status:
                status === "cancelled"
                  ? "cancelled"
                  : status === "completed"
                    ? "success"
                    : "failed",
              message: result.message,
              outputPath,
              outputName: fileNameFromPath(outputPath),
              outputExtension: outputPath ? "pdf" : "",
              outputBytes: result.outputBytes
            },
            finishedAt
          );
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
      const finishedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status: "failed",
              errorLog: message
            },
            { status: "failed", message },
            finishedAt
          );
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
    const startedAt = Date.now();
    setTasks((currentTasks) =>
      currentTasks.map((currentTask) =>
        currentTask.taskId === task.taskId
          ? {
              ...startTaskOperation(currentTask, "pdf_rotate", startedAt),
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
      const finishedAt = Date.now();

      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }

          const status =
            currentTask.status === "cancelled" ? "cancelled" : resultStatus;
          const outputPath = status === "completed" ? result.outputPath : "";
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status,
              outputPreview: outputPath || currentTask.outputPreview,
              errorLog: status === "failed" ? log : ""
            },
            {
              status:
                status === "cancelled"
                  ? "cancelled"
                  : status === "completed"
                    ? "success"
                    : "failed",
              message: result.message,
              outputPath,
              outputName: fileNameFromPath(outputPath),
              outputExtension: outputPath ? "pdf" : "",
              outputBytes: result.outputBytes
            },
            finishedAt
          );
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
      const finishedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) => {
          if (currentTask.taskId !== task.taskId) {
            return currentTask;
          }
          if (currentTask.status === "cancelled") {
            return { ...currentTask, backendTaskId: undefined };
          }
          return finishTaskOperation(
            {
              ...currentTask,
              backendTaskId: undefined,
              status: "failed",
              errorLog: message
            },
            { status: "failed", message },
            finishedAt
          );
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
      const startedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
            ? {
                ...startTaskOperation(
                  currentTask,
                  "image_convert",
                  startedAt
                ),
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
        const finishedAt = Date.now();

        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }

            const status =
              currentTask.status === "cancelled"
                ? "cancelled"
                : resultStatus;
            const outputPath =
              status === "completed" && result.outputPath
                ? result.outputPath
                : "";
            return finishTaskOperation(
              {
                ...currentTask,
                backendTaskId: undefined,
                status,
                outputPreview: outputPath || currentTask.outputPreview,
                errorLog: status === "failed" ? log : ""
              },
              {
                status:
                  status === "cancelled"
                    ? "cancelled"
                    : status === "completed"
                      ? "success"
                      : "failed",
                message: result.message,
                outputPath,
                outputName: fileNameFromPath(outputPath),
                outputExtension: outputPath ? result.targetFormat : "",
                outputBytes: result.outputBytes
              },
              finishedAt
            );
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
        const finishedAt = Date.now();
        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }
            const cancelled =
              currentTask.status === "cancelled" || wasCancelled;
            return finishTaskOperation(
              {
                  ...currentTask,
                  backendTaskId: undefined,
                  status: cancelled ? "cancelled" : "failed",
                  errorLog: cancelled ? "" : message
              },
              {
                status: cancelled ? "cancelled" : "failed",
                message: cancelled ? "图片转换已取消。" : message
              },
              finishedAt
            );
          })
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

  async function resizeSelectedImages() {
    if (!canResizeSelectedImages) {
      setFolderMessage(imageResizeGuidance());
      return;
    }

    const resizeTasks = selectedRealLocalImageTasks;
    const mode = resizeMode;
    const dimensions = validateResizeInputs(mode, resizeWidth, resizeHeight);
    if (!dimensions.valid) {
      setFolderMessage(dimensions.message);
      return;
    }

    let successCount = 0;
    let failureCount = 0;
    let cancelledCount = 0;
    let unchangedCount = 0;
    const outputSummaries: string[] = [];

    for (const task of resizeTasks) {
      if (cancelledTaskIdsRef.current.has(task.taskId)) {
        cancelledCount += 1;
        continue;
      }

      if (!task.sourcePath) {
        failureCount += 1;
        continue;
      }

      await planBackendOutput(task, task.extension);
      const backendTaskId = createBackendTaskId("image-resize", task.taskId);
      cancelledTaskIdsRef.current.delete(task.taskId);
      const startedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
            ? {
                ...startTaskOperation(currentTask, "image_resize", startedAt),
                backendTaskId,
                status: "converting",
                errorLog: ""
              }
            : currentTask
        )
      );

      try {
        const response = await invoke<BackendTaskResponse<ImageResizeResult>>(
          "image_resize_file",
          {
            taskId: backendTaskId,
            request: {
              source: task.sourcePath,
              mode,
              maxWidth: dimensions.maxWidth ?? null,
              maxHeight: dimensions.maxHeight ?? null
            }
          }
        );
        const result = response.result;
        const resultStatus = backendResponseStatus(response);
        const log = formatImageResizeLog(result);
        if (resultStatus === "cancelled") {
          cancelledCount += 1;
        } else if (result.success) {
          successCount += 1;
          if (!result.resized) {
            unchangedCount += 1;
          }
          outputSummaries.push(
            `${result.outputPath} (${result.outputWidth} × ${result.outputHeight})`
          );
        } else {
          failureCount += 1;
        }
        const finishedAt = Date.now();

        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }

            const status =
              currentTask.status === "cancelled" ? "cancelled" : resultStatus;
            const outputPath =
              status === "completed" && result.outputPath
                ? result.outputPath
                : "";
            return finishTaskOperation(
              {
                ...currentTask,
                backendTaskId: undefined,
                status,
                outputPreview: outputPath || currentTask.outputPreview,
                errorLog: status === "failed" ? log : ""
              },
              {
                status:
                  status === "cancelled"
                    ? "cancelled"
                    : status === "completed"
                      ? "success"
                      : "failed",
                message: result.message,
                outputPath,
                outputName: fileNameFromPath(outputPath),
                outputExtension: outputPath ? result.sourceFormat : "",
                outputBytes: result.outputBytes
              },
              finishedAt
            );
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
              : "图片改尺寸失败。";
        const finishedAt = Date.now();
        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }
            const cancelled =
              currentTask.status === "cancelled" || wasCancelled;
            return finishTaskOperation(
              {
                  ...currentTask,
                  backendTaskId: undefined,
                  status: cancelled ? "cancelled" : "failed",
                  errorLog: cancelled ? "" : message
              },
              {
                status: cancelled ? "cancelled" : "failed",
                message: cancelled ? "图片改尺寸已取消。" : message
              },
              finishedAt
            );
          })
        );
      }
    }

    if (failureCount === 0 && cancelledCount === 0) {
      setFolderMessage(
        `已在本机完成 ${successCount} 张图片改尺寸。${unchangedCount > 0 ? `其中 ${unchangedCount} 张未放大并保持原尺寸。` : ""}输出：${outputSummaries.join(" | ")}`
      );
    } else {
      setFolderMessage(
        `图片改尺寸结束：成功 ${successCount}，失败 ${failureCount}，已取消 ${cancelledCount}。${failureCount > 0 ? "请查看失败任务的错误日志。" : "未保留已取消任务的未完成输出。"}`
      );
    }
  }

  async function compressSelectedImages() {
    if (!canCompressSelectedImages) {
      setFolderMessage(imageCompressionGuidance());
      return;
    }

    const compressionTasks = selectedRealLocalImageTasks;
    const jpegQuality = jpegQualityValidation.value;
    const webpQuality = webpQualityValidation.value;
    let publishedCount = 0;
    let notSmallerCount = 0;
    let failureCount = 0;
    let cancelledCount = 0;
    const outputPaths: string[] = [];
    const notSmallerNames: string[] = [];

    for (const task of compressionTasks) {
      if (cancelledTaskIdsRef.current.has(task.taskId)) {
        cancelledCount += 1;
        continue;
      }

      if (!task.sourcePath) {
        failureCount += 1;
        continue;
      }

      const format = canonicalImageFormat(task.extension);
      const quality =
        format === "jpg"
          ? jpegQuality
          : format === "webp"
            ? webpQuality
            : undefined;
      const backendTaskId = createBackendTaskId("image-compress", task.taskId);
      cancelledTaskIdsRef.current.delete(task.taskId);
      const startedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
            ? {
                ...startTaskOperation(
                  currentTask,
                  "image_compress",
                  startedAt
                ),
                backendTaskId,
                status: "converting",
                errorLog: ""
              }
            : currentTask
        )
      );

      try {
        const response = await invoke<BackendTaskResponse<ImageCompressResult>>(
          "image_compress_file",
          {
            taskId: backendTaskId,
            request: {
              source: task.sourcePath,
              quality: quality ?? null
            }
          }
        );
        const result = response.result;
        const resultStatus = backendResponseStatus(response);
        const log = formatImageCompressLog(result);
        if (resultStatus === "cancelled") {
          cancelledCount += 1;
        } else if (result.success && result.published) {
          publishedCount += 1;
          outputPaths.push(result.outputPath);
        } else if (result.success) {
          notSmallerCount += 1;
          notSmallerNames.push(task.displayName);
        } else {
          failureCount += 1;
        }
        const finishedAt = Date.now();

        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }

            const status =
              currentTask.status === "cancelled" ? "cancelled" : resultStatus;
            const outputPath =
              status === "completed" && result.published && result.outputPath
                ? result.outputPath
                : "";
            const savedPercent =
              result.sourceBytes > 0 && result.savedBytes > 0
                ? Number(
                    ((result.savedBytes / result.sourceBytes) * 100).toFixed(2)
                  )
                : 0;
            return finishTaskOperation(
              {
                ...currentTask,
                backendTaskId: undefined,
                status,
                outputPreview: outputPath || currentTask.outputPreview,
                errorLog: status === "failed" ? log : ""
              },
              {
                status:
                  status === "cancelled"
                    ? "cancelled"
                    : status === "failed"
                      ? "failed"
                      : result.published
                        ? "success"
                        : "not_smaller",
                message: result.message,
                outputPath,
                outputName: fileNameFromPath(outputPath),
                outputExtension: outputPath ? result.sourceFormat : "",
                outputBytes: result.outputBytes,
                savedBytes: result.savedBytes,
                savedPercent
              },
              finishedAt
            );
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
              : "图片压缩失败。";
        const finishedAt = Date.now();
        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }
            const cancelled =
              currentTask.status === "cancelled" || wasCancelled;
            return finishTaskOperation(
              {
                  ...currentTask,
                  backendTaskId: undefined,
                  status: cancelled ? "cancelled" : "failed",
                  errorLog: cancelled ? "" : message
              },
              {
                status: cancelled ? "cancelled" : "failed",
                message: cancelled ? "图片压缩已取消。" : message
              },
              finishedAt
            );
          })
        );
      }
    }

    const notSmallerSummary =
      notSmallerCount > 0
        ? `压缩后未变小，未生成新文件。共 ${notSmallerCount} 个：${notSmallerNames.join("、")}。`
        : "";
    if (failureCount === 0 && cancelledCount === 0) {
      setFolderMessage(
        `图片压缩处理完成：生成 ${publishedCount} 个文件。${notSmallerSummary}${outputPaths.length > 0 ? `输出：${outputPaths.join(" | ")}` : ""}`
      );
    } else {
      setFolderMessage(
        `图片压缩结束：生成 ${publishedCount}，未变小 ${notSmallerCount}，失败 ${failureCount}，已取消 ${cancelledCount}。${notSmallerSummary}${failureCount > 0 ? "请查看失败任务的错误日志。" : "未保留已取消任务的未完成输出。"}`
      );
    }
  }

  async function cleanSelectedImageMetadata() {
    if (!canCleanSelectedImageMetadata) {
      setFolderMessage(imageMetadataCleanupGuidance());
      return;
    }

    const cleanupTasks = selectedRealLocalImageTasks;
    let publishedCount = 0;
    let noMetadataCount = 0;
    let failureCount = 0;
    let cancelledCount = 0;
    const outputPaths: string[] = [];
    const noMetadataNames: string[] = [];

    for (const task of cleanupTasks) {
      if (cancelledTaskIdsRef.current.has(task.taskId)) {
        cancelledCount += 1;
        continue;
      }
      if (!task.sourcePath) {
        failureCount += 1;
        continue;
      }

      const backendTaskId = createBackendTaskId("image-clean-metadata", task.taskId);
      cancelledTaskIdsRef.current.delete(task.taskId);
      const startedAt = Date.now();
      setTasks((currentTasks) =>
        currentTasks.map((currentTask) =>
          currentTask.taskId === task.taskId
            ? {
                ...startTaskOperation(
                  currentTask,
                  "image_metadata_cleanup",
                  startedAt
                ),
                backendTaskId,
                status: "converting",
                errorLog: ""
              }
            : currentTask
        )
      );

      try {
        const response = await invoke<BackendTaskResponse<ImageCleanMetadataResult>>(
          "image_clean_metadata_file",
          {
            taskId: backendTaskId,
            request: { source: task.sourcePath }
          }
        );
        const result = response.result;
        const resultStatus = backendResponseStatus(response);
        const log = formatImageMetadataCleanupLog(result);
        if (resultStatus === "cancelled") {
          cancelledCount += 1;
        } else if (result.success && result.published) {
          publishedCount += 1;
          outputPaths.push(result.outputPath);
        } else if (result.success) {
          noMetadataCount += 1;
          noMetadataNames.push(task.displayName);
        } else {
          failureCount += 1;
        }
        const finishedAt = Date.now();

        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }
            const status =
              currentTask.status === "cancelled" ? "cancelled" : resultStatus;
            const outputPath =
              status === "completed" && result.published && result.outputPath
                ? result.outputPath
                : "";
            return finishTaskOperation(
              {
                ...currentTask,
                backendTaskId: undefined,
                status,
                outputPreview:
                  status === "completed"
                    ? outputPath || result.message
                    : currentTask.outputPreview,
                errorLog: status === "failed" ? log : ""
              },
              {
                status:
                  status === "cancelled"
                    ? "cancelled"
                    : status === "failed"
                      ? "failed"
                      : result.published
                        ? "success"
                        : "skipped",
                message: result.message,
                outputPath,
                outputName: fileNameFromPath(outputPath),
                outputExtension: outputPath ? result.sourceFormat : "",
                outputBytes: result.outputBytes
              },
              finishedAt
            );
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
              : "图片元数据清理失败。";
        const finishedAt = Date.now();
        setTasks((currentTasks) =>
          currentTasks.map((currentTask) => {
            if (currentTask.taskId !== task.taskId) {
              return currentTask;
            }
            const cancelled =
              currentTask.status === "cancelled" || wasCancelled;
            return finishTaskOperation(
              {
                  ...currentTask,
                  backendTaskId: undefined,
                  status: cancelled ? "cancelled" : "failed",
                  errorLog: cancelled ? "" : message
              },
              {
                status: cancelled ? "cancelled" : "failed",
                message: cancelled ? "图片元数据清理已取消。" : message
              },
              finishedAt
            );
          })
        );
      }
    }

    const noMetadataSummary =
      noMetadataCount > 0
        ? `未发现可清理的元数据，未生成新文件。共 ${noMetadataCount} 个：${noMetadataNames.join("、")}。`
        : "";
    if (failureCount === 0 && cancelledCount === 0) {
      setFolderMessage(
        `图片元数据清理完成：生成 ${publishedCount} 个 cleaned 文件。${noMetadataSummary}${outputPaths.length > 0 ? `输出：${outputPaths.join(" | ")}` : ""}`
      );
    } else {
      setFolderMessage(
        `图片元数据清理结束：生成 ${publishedCount}，无可清理内容 ${noMetadataCount}，失败 ${failureCount}，已取消 ${cancelledCount}。${noMetadataSummary}${failureCount > 0 ? "请查看失败任务的错误日志。" : "未保留已取消任务的未完成输出。"}`
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
                  ? "图片转换、改尺寸、压缩与隐私清理已启用"
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
            <span>PDF 工具与 JPG、PNG、WebP 图片转换、改尺寸、压缩和元数据清理已启用。Office 等其他能力仍未启用。</span>
          </div>
        </aside>

        <section className="main-pane" aria-label="文件队列工作台">
          <div className="pane-header">
            <div>
              <p className="section-kicker">{activeTool}</p>
              <h2>文件导入</h2>
            </div>
            <div className="pane-header-actions">
              <button
                type="button"
                className="secondary-button"
                onClick={clearCompletedTasks}
                disabled={summary.completed === 0}
              >
                清除已完成
              </button>
              <button
                type="button"
                className="secondary-button"
                onClick={clearTaskHistory}
                disabled={tasks.length === 0 || summary.converting > 0}
                title={
                  summary.converting > 0
                    ? "请先取消运行中的任务。"
                    : "只清空当前界面记录，不删除任何文件。"
                }
              >
                清空记录
              </button>
            </div>
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

          <section className="image-tools-panel" aria-label="Preview 0.5 图片工具">
            <div className="pdf-tools-header">
              <div>
                <p className="section-kicker">Preview 0.5 · 本地功能预览</p>
                <h2>图片工具</h2>
                <p>
                  {imageEngineAvailable
                    ? "JPG、PNG、WebP 格式转换、等比改尺寸、同格式压缩与元数据清理已启用，仅在本机执行。"
                    : "图片引擎不可用，图片操作保持禁用。"}
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

              <article className="pdf-tool-card image-tool-card image-resize-card">
                <h3>图片改尺寸 <span className="preview-label">Preview</span></h3>
                <p>保持原格式和宽高比，结果写入 converted 文件夹，不放大较小图片。</p>
                <fieldset className="resize-mode-fieldset">
                  <legend>调整方式</legend>
                  <div className="resize-mode-control">
                    {([
                      ["fit", "适应范围"],
                      ["width", "仅宽度"],
                      ["height", "仅高度"]
                    ] as const).map(([value, label]) => (
                      <label
                        className={resizeMode === value ? "is-selected" : ""}
                        key={value}
                      >
                        <input
                          type="radio"
                          name="resize-mode"
                          value={value}
                          checked={resizeMode === value}
                          onChange={() => setResizeMode(value)}
                        />
                        <span>{label}</span>
                      </label>
                    ))}
                  </div>
                </fieldset>
                <div className="resize-dimension-grid">
                  <label>
                    <span>最大宽度</span>
                    <input
                      type="number"
                      inputMode="numeric"
                      min="1"
                      max={maxResizeDimension}
                      step="1"
                      value={resizeWidth}
                      disabled={resizeMode === "height"}
                      onChange={(event) => setResizeWidth(event.currentTarget.value)}
                    />
                  </label>
                  <label>
                    <span>最大高度</span>
                    <input
                      type="number"
                      inputMode="numeric"
                      min="1"
                      max={maxResizeDimension}
                      step="1"
                      value={resizeHeight}
                      disabled={resizeMode === "width"}
                      onChange={(event) => setResizeHeight(event.currentTarget.value)}
                    />
                  </label>
                </div>
                <div className="resize-invariants" aria-label="固定调整规则">
                  <label><input type="checkbox" checked disabled readOnly />保持宽高比</label>
                  <label><input type="checkbox" checked disabled readOnly />不放大小图</label>
                </div>
                <p className={resizeInputValidation.valid ? "resize-guidance" : "resize-guidance is-error"}>
                  {imageResizeGuidance()}
                </p>
                <button
                  type="button"
                  onClick={() => void resizeSelectedImages()}
                  disabled={!canResizeSelectedImages}
                >
                  改尺寸并保存
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card image-compression-card">
                <h3>图片压缩 <span className="preview-label">Preview 0.4</span></h3>
                <p>保持源格式。JPEG/WebP 使用质量压缩，PNG 仅做无损优化；结果未变小时不生成新文件。</p>
                <div className="compression-quality-grid">
                  <label>
                    <span>JPEG 质量</span>
                    <div className="quality-control">
                      <input
                        type="range"
                        min={minCompressionQuality}
                        max={maxCompressionQuality}
                        step="1"
                        value={jpegQualityValidation.value ?? 82}
                        onChange={(event) =>
                          setJpegCompressionQuality(event.currentTarget.value)
                        }
                        aria-label="JPEG 压缩质量滑块"
                      />
                      <input
                        type="number"
                        inputMode="numeric"
                        min={minCompressionQuality}
                        max={maxCompressionQuality}
                        step="1"
                        value={jpegCompressionQuality}
                        onChange={(event) =>
                          setJpegCompressionQuality(event.currentTarget.value)
                        }
                        aria-label="JPEG 压缩质量"
                      />
                    </div>
                  </label>
                  <label>
                    <span>WebP 质量</span>
                    <div className="quality-control">
                      <input
                        type="range"
                        min={minCompressionQuality}
                        max={maxCompressionQuality}
                        step="1"
                        value={webpQualityValidation.value ?? 80}
                        onChange={(event) =>
                          setWebpCompressionQuality(event.currentTarget.value)
                        }
                        aria-label="WebP 压缩质量滑块"
                      />
                      <input
                        type="number"
                        inputMode="numeric"
                        min={minCompressionQuality}
                        max={maxCompressionQuality}
                        step="1"
                        value={webpCompressionQuality}
                        onChange={(event) =>
                          setWebpCompressionQuality(event.currentTarget.value)
                        }
                        aria-label="WebP 压缩质量"
                      />
                    </div>
                  </label>
                </div>
                <div className="resize-invariants" aria-label="固定压缩规则">
                  <label><input type="checkbox" checked disabled readOnly />PNG 无损优化</label>
                  <label><input type="checkbox" checked disabled readOnly />仅结果更小时生成</label>
                  <label><input type="checkbox" checked disabled readOnly />保持源格式</label>
                </div>
                <p
                  className={
                    canCompressSelectedImages || selectedTasks.length === 0
                      ? "resize-guidance"
                      : "resize-guidance is-error"
                  }
                >
                  {imageCompressionGuidance()}
                </p>
                <button
                  type="button"
                  onClick={() => void compressSelectedImages()}
                  disabled={!canCompressSelectedImages}
                >
                  压缩所选图片
                </button>
              </article>

              <article className="pdf-tool-card image-tool-card">
                <h3>图片元数据清理 <span className="preview-label">Preview 0.5</span></h3>
                <p>尽力移除常见隐私元数据，保持源格式并写入 cleaned 输出，不修改原文件。</p>
                <ul className="image-conversion-matrix" aria-label="元数据清理范围">
                  <li>JPEG：EXIF / GPS / XMP / IPTC / 相机信息</li>
                  <li>PNG：EXIF、文本与时间信息块</li>
                  <li>WebP：EXIF 与 XMP 信息块</li>
                </ul>
                <div className="resize-invariants" aria-label="固定清理规则">
                  <label><input type="checkbox" checked disabled readOnly />保持源格式</label>
                  <label><input type="checkbox" checked disabled readOnly />无内容则不生成</label>
                  <label><input type="checkbox" checked disabled readOnly />不修改原文件</label>
                </div>
                <p className="resize-guidance">
                  {imageMetadataCleanupGuidance()}
                </p>
                <p className="resize-guidance">
                  最佳努力清理：不保证所有私有厂商字段都能完全移除。带方向标记的 JPEG 会先固化方向并以质量 95 重编码。
                </p>
                <button
                  type="button"
                  onClick={() => void cleanSelectedImageMetadata()}
                  disabled={!canCleanSelectedImageMetadata}
                >
                  清理所选图片元数据
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
                <p>将文件拖到上方区域，或点击“选择文件”开始。</p>
                <ul>
                  <li>所有处理均在本机完成。</li>
                  <li>原文件不会被覆盖。</li>
                  <li>任务完成后可以导出 CSV 或 JSON 报告。</li>
                </ul>
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
                      {task.status === "completed" && task.outputLocationPath ? (
                        <button
                          type="button"
                          className="small-button secondary-button"
                          onClick={() => void revealTaskOutput(task)}
                        >
                          打开输出位置
                        </button>
                      ) : null}
                      {copyableTaskError(task) ? (
                        <button
                          type="button"
                          className="small-button secondary-button"
                          onClick={() => void copyTaskError(task)}
                        >
                          {copiedTaskId === task.taskId
                            ? "已复制"
                            : "复制错误信息"}
                        </button>
                      ) : null}
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
            <p className="about-version">LocalConvert Desktop · Preview 0.6.1</p>
            <p>
              <strong>by 田宸宇</strong>
            </p>
            <p className="about-slogan">让可能发生在这儿。</p>
            <p>纯本地处理：不上传、不依赖服务器、不采集遥测。</p>
            <p className="about-preview-note">
              macOS Apple Silicon 预览版，尚未完成 Developer ID 签名与 Apple 公证。请仅从官方 GitHub Release 页面下载，并核对发布页 SHA-256。
            </p>
          </section>

          <section className="inspector-card">
            <h2>输出规则</h2>
            <p>默认输出到源文件旁边的 converted 文件夹。</p>
            <pre>{outputNameExample.join("\n")}</pre>
          </section>

          <section className="inspector-card report-export-card">
            <h2>导出处理报告</h2>
            <p>
              可导出 {reportRecords.length} 条任务结果。报告仅包含应用已知的任务元数据，不包含文件内容或原始图片元数据。
            </p>
            <label className="report-format-field">
              <span>报告格式</span>
              <select
                aria-label="报告格式"
                value={reportFormat}
                onChange={(event) => {
                  setReportFormat(event.currentTarget.value as ReportFormat);
                  setReportExportMessage("");
                  setReportExportError("");
                }}
              >
                <option value="csv">CSV</option>
                <option value="json">JSON</option>
              </select>
            </label>
            <button
              type="button"
              onClick={() => void exportTaskReport()}
              disabled={reportRecords.length === 0 || reportExporting}
            >
              {reportExporting ? "正在导出..." : "导出报告"}
            </button>
            {lastReportPath ? (
              <div className="report-location">
                <span title={lastReportPath}>
                  最近报告：{fileNameFromPath(lastReportPath)}
                </span>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() => void revealLastReport()}
                >
                  打开报告位置
                </button>
              </div>
            ) : null}
            <p className="report-privacy-note">
              隐私提醒：报告可能包含本机文件路径。分享前请先检查；不会导出文件内容、GPS 值或原始 EXIF/XMP/IPTC 数据。
            </p>
            {reportExportMessage ? (
              <p className="report-export-message" role="status">
                {reportExportMessage}
              </p>
            ) : null}
            {reportExportError ? (
              <p className="report-export-error" role="alert">
                {reportExportError}
              </p>
            ) : null}
          </section>

          <section className="inspector-card">
            <h2>引擎状态</h2>
            <p>
              {startupError
                ? `启动初始化报告问题：${startupError}`
                : qpdfAvailable || imageEngineAvailable
                  ? "PDF 工具与 JPG、PNG、WebP 图片转换、改尺寸、压缩和元数据清理会按可用引擎状态在本机启用。"
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
                "暂无错误日志。PDF 或图片操作失败时会显示在这里。"}
            </pre>
          </section>

          <section className="inspector-card">
            <h2>本地隐私</h2>
            <ul>
              <li>不上传。</li>
              <li>不会修改原文件。</li>
              <li>PDF 合并、拆分、页面提取和旋转仅使用内置本地 qpdf。</li>
              <li>JPG、PNG、WebP 转换、改尺寸、压缩和元数据清理仅使用内置本地 image-engine。</li>
              <li>元数据清理为最佳努力，不宣称法证级彻底清除。</li>
              <li>Office、HEIC 与其他未列出的图片操作仍未启用。</li>
            </ul>
          </section>
        </aside>
      </div>
      {showClearHistoryConfirmation ? (
        <div className="confirmation-overlay">
          <section
            className="confirmation-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="clear-history-title"
            aria-describedby="clear-history-description"
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setShowClearHistoryConfirmation(false);
              }
            }}
          >
            <h2 id="clear-history-title">清空任务记录？</h2>
            <p id="clear-history-description">
              只会清空当前界面记录，不会删除任何文件。
            </p>
            <div className="confirmation-actions">
              <button
                type="button"
                className="secondary-button"
                autoFocus
                onClick={() => setShowClearHistoryConfirmation(false)}
              >
                取消
              </button>
              <button type="button" onClick={confirmClearTaskHistory}>
                确认清空
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}

export default App;
