export type TaskStatus =
  | "waiting"
  | "converting"
  | "completed"
  | "failed"
  | "cancelled";

export type TaskSourceKind = "native-path" | "browser-preview";

export type TaskReportStatus =
  | "success"
  | "failed"
  | "cancelled"
  | "skipped"
  | "not_smaller"
  | "unsupported";

export type BackendTaskStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export type NativePathMetadata = {
  sourcePath: string;
  displayName: string;
  extension: string;
  size: number;
  sourceKind: "native-path";
};

export type RetrySourceSnapshot = {
  sourcePath: string;
  displayName: string;
  extension: string;
  size: number;
};

export type TaskRetryDescriptor =
  | { kind: "pdf-merge"; sources: RetrySourceSnapshot[] }
  | { kind: "pdf-split" }
  | { kind: "pdf-extract"; pages: string }
  | { kind: "pdf-rotate"; degrees: 90 | 180 | -90 }
  | { kind: "image-convert"; targetFormat: "jpg" | "png" | "webp" }
  | {
      kind: "image-resize";
      mode: "fit" | "width" | "height";
      maxWidth?: number;
      maxHeight?: number;
    }
  | { kind: "image-compress"; jpegQuality: number; webpQuality: number }
  | { kind: "image-clean-metadata" };

export type LocalTask = {
  taskId: string;
  backendTaskId?: string;
  displayName: string;
  size: number;
  extension: string;
  sourcePath?: string;
  sourceKind: TaskSourceKind;
  sourcePreview: string;
  outputPreview: string;
  status: TaskStatus;
  errorLog: string;
  createdAt: number;
  operationType?: string;
  startedAt?: number;
  finishedAt?: number;
  reportStatus?: TaskReportStatus;
  reportOutputPath?: string;
  reportOutputName?: string;
  reportOutputExtension?: string;
  reportOutputBytes?: number;
  reportSavedBytes?: number;
  reportSavedPercent?: number;
  reportMessage?: string;
  outputLocationPath?: string;
  retryDescriptor?: TaskRetryDescriptor;
  retryGroupId?: string;
};

export function getExtension(fileName: string): string {
  const dotIndex = fileName.lastIndexOf(".");
  if (dotIndex <= 0 || dotIndex === fileName.length - 1) {
    return "未知";
  }

  return fileName.slice(dotIndex + 1).toLowerCase();
}

export function mapBackendTaskStatus(
  status: BackendTaskStatus,
  success: boolean
): TaskStatus {
  if (status === "cancelled") {
    return "cancelled";
  }

  return status === "completed" && success ? "completed" : "failed";
}

export function getBaseName(fileName: string): string {
  const dotIndex = fileName.lastIndexOf(".");
  if (dotIndex <= 0) {
    return fileName || "未命名";
  }

  return fileName.slice(0, dotIndex);
}

export function getOutputName(
  desiredName: string,
  existingNames: readonly string[]
): string {
  const existing = new Set(existingNames);
  if (!existing.has(desiredName)) {
    return desiredName;
  }

  const dotIndex = desiredName.lastIndexOf(".");
  const base = dotIndex > 0 ? desiredName.slice(0, dotIndex) : desiredName;
  const extension = dotIndex > 0 ? desiredName.slice(dotIndex) : "";
  let index = 1;

  while (existing.has(`${base} (${index})${extension}`)) {
    index += 1;
  }

  return `${base} (${index})${extension}`;
}

export function formatBytes(size: number): string {
  if (!Number.isFinite(size) || size <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  const unitIndex = Math.min(
    Math.floor(Math.log(size) / Math.log(1024)),
    units.length - 1
  );
  const value = size / 1024 ** unitIndex;
  const precision = unitIndex === 0 ? 0 : 1;

  return `${value.toFixed(precision)} ${units[unitIndex]}`;
}

export function createTaskFromFile(
  file: File,
  existingOutputNames: readonly string[],
  now = Date.now(),
  targetExtension = "pdf"
): LocalTask {
  const displayName = file.name || "未命名";
  const desiredOutputName = `${getBaseName(displayName)}.${targetExtension}`;
  const outputName = getOutputName(desiredOutputName, existingOutputNames);

  return {
    taskId: createLocalTaskId(now),
    displayName,
    size: file.size,
    extension: getExtension(displayName),
    sourceKind: "browser-preview",
    sourcePreview: file.webkitRelativePath || displayName,
    outputPreview: `converted/${outputName}`,
    status: "waiting",
    errorLog: "",
    createdAt: now
  };
}

export function createTaskFromNativePathMetadata(
  metadata: NativePathMetadata,
  existingOutputNames: readonly string[],
  now = Date.now(),
  targetExtension = "pdf"
): LocalTask {
  const displayName = metadata.displayName || "未命名";
  const desiredOutputName = `${getBaseName(displayName)}.${targetExtension}`;
  const outputName = getOutputName(desiredOutputName, existingOutputNames);

  return {
    taskId: createLocalTaskId(now),
    displayName,
    size: metadata.size,
    extension: metadata.extension || getExtension(displayName),
    sourcePath: metadata.sourcePath,
    sourceKind: "native-path",
    sourcePreview: metadata.sourcePath,
    outputPreview: `converted/${outputName}`,
    status: "waiting",
    errorLog: "",
    createdAt: now
  };
}

export function createLocalTaskId(now = Date.now()): string {
  return `${now}-${Math.random().toString(36).slice(2, 9)}`;
}
