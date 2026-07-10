export type TaskStatus =
  | "waiting"
  | "converting"
  | "completed"
  | "failed"
  | "cancelled";

export type LocalTask = {
  id: string;
  displayName: string;
  size: number;
  extension: string;
  sourcePath?: string;
  sourcePreview: string;
  outputPreview: string;
  status: TaskStatus;
  errorLog: string;
  createdAt: number;
};

export function getExtension(fileName: string): string {
  const dotIndex = fileName.lastIndexOf(".");
  if (dotIndex <= 0 || dotIndex === fileName.length - 1) {
    return "未知";
  }

  return fileName.slice(dotIndex + 1).toLowerCase();
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
  const fileWithOptionalPath = file as File & {
    path?: string;
    webkitRelativePath?: string;
  };
  const sourcePath = fileWithOptionalPath.path?.trim() || undefined;
  const desiredOutputName = `${getBaseName(displayName)}.${targetExtension}`;
  const outputName = getOutputName(desiredOutputName, existingOutputNames);

  return {
    id: `${now}-${Math.random().toString(36).slice(2, 9)}`,
    displayName,
    size: file.size,
    extension: getExtension(displayName),
    sourcePath,
    sourcePreview:
      sourcePath ||
      fileWithOptionalPath.webkitRelativePath ||
      displayName,
    outputPreview: `converted/${outputName}`,
    status: "waiting",
    errorLog: "",
    createdAt: now
  };
}
