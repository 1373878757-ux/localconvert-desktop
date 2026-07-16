import {
  LocalTask,
  TaskReportStatus,
  createLocalTaskId
} from "./taskUtils";

export type TaskQueueFilter =
  | "all"
  | "running"
  | "success"
  | "failed"
  | "cancelled"
  | "skipped";

export type TaskQueueSummary = {
  total: number;
  running: number;
  success: number;
  failed: number;
  cancelled: number;
  skipped: number;
};

export const taskQueueFilterLabels: Record<TaskQueueFilter, string> = {
  all: "全部",
  running: "处理中",
  success: "成功",
  failed: "失败",
  cancelled: "已取消",
  skipped: "跳过 / 未变小"
};

export const operationLabels: Record<string, string> = {
  pdf_merge: "合并 PDF",
  pdf_split: "拆分 PDF",
  pdf_extract_pages: "提取 PDF 页面",
  pdf_rotate: "旋转 PDF",
  image_convert: "图片格式转换",
  image_resize: "图片改尺寸",
  image_compress: "图片压缩",
  image_metadata_cleanup: "图片隐私清理",
  queue_cancel: "取消等待任务"
};

export function taskOperationLabel(task: LocalTask): string {
  return task.operationType
    ? operationLabels[task.operationType] ?? task.operationType
    : "尚未处理";
}

function reportMatches(
  reportStatus: TaskReportStatus | undefined,
  filter: TaskQueueFilter
): boolean {
  if (filter === "success") {
    return reportStatus === "success";
  }
  if (filter === "failed") {
    return reportStatus === "failed" || reportStatus === "unsupported";
  }
  if (filter === "cancelled") {
    return reportStatus === "cancelled";
  }
  if (filter === "skipped") {
    return reportStatus === "skipped" || reportStatus === "not_smaller";
  }
  return false;
}

export function taskMatchesFilter(
  task: LocalTask,
  filter: TaskQueueFilter
): boolean {
  if (filter === "all") {
    return true;
  }
  if (filter === "running") {
    return task.status === "converting";
  }
  if (reportMatches(task.reportStatus, filter)) {
    return true;
  }
  if (task.reportStatus) {
    return false;
  }
  if (filter === "success") {
    return task.status === "completed";
  }
  if (filter === "failed") {
    return task.status === "failed";
  }
  return filter === "cancelled" && task.status === "cancelled";
}

export function filterTasks(
  tasks: readonly LocalTask[],
  filter: TaskQueueFilter,
  search: string
): LocalTask[] {
  const query = search.trim().toLocaleLowerCase("zh-CN");
  return tasks.filter((task) => {
    if (!taskMatchesFilter(task, filter)) {
      return false;
    }
    if (!query) {
      return true;
    }

    const haystack = [
      task.displayName,
      task.reportOutputName ?? "",
      task.reportOutputPath
        ? fileNameFromPath(task.reportOutputPath)
        : fileNameFromPath(task.outputPreview),
      taskOperationLabel(task)
    ]
      .join("\n")
      .toLocaleLowerCase("zh-CN");
    return haystack.includes(query);
  });
}

export function summarizeTasks(tasks: readonly LocalTask[]): TaskQueueSummary {
  const summary: TaskQueueSummary = {
    total: tasks.length,
    running: 0,
    success: 0,
    failed: 0,
    cancelled: 0,
    skipped: 0
  };

  for (const task of tasks) {
    if (taskMatchesFilter(task, "running")) summary.running += 1;
    if (taskMatchesFilter(task, "success")) summary.success += 1;
    if (taskMatchesFilter(task, "failed")) summary.failed += 1;
    if (taskMatchesFilter(task, "cancelled")) summary.cancelled += 1;
    if (taskMatchesFilter(task, "skipped")) summary.skipped += 1;
  }
  return summary;
}

export function isTerminalTask(task: LocalTask): boolean {
  return task.status !== "waiting" && task.status !== "converting";
}

export function removeTerminalTasks(tasks: readonly LocalTask[]): LocalTask[] {
  return tasks.filter((task) => !isTerminalTask(task));
}

export function isRetryableFailure(task: LocalTask): boolean {
  return (
    Boolean(task.retryDescriptor) &&
    (task.reportStatus === "failed" ||
      task.reportStatus === "unsupported" ||
      (!task.reportStatus && task.status === "failed"))
  );
}

export function cloneTaskForRetry(
  task: LocalTask,
  now = Date.now()
): LocalTask {
  return {
    ...task,
    taskId: createLocalTaskId(now),
    backendTaskId: undefined,
    status: "waiting",
    errorLog: "",
    createdAt: now,
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

export function unavailableRetryTaskNames(
  tasks: readonly LocalTask[],
  availableSourcePaths: readonly string[]
): string[] {
  const available = new Set(availableSourcePaths);
  return tasks
    .filter((task) => !task.sourcePath || !available.has(task.sourcePath))
    .map((task) => task.displayName);
}

export function conciseTaskError(task: LocalTask): string {
  const sourcePath = task.sourcePath ?? "";
  const outputPath = task.reportOutputPath ?? "";
  let message = (task.reportMessage || task.errorLog || "未提供错误详情")
    .split(/\r?\n/)
    .find((line) => line.trim().length > 0)
    ?.trim() ?? "未提供错误详情";

  for (const privatePath of [sourcePath, outputPath]) {
    if (privatePath) {
      message = message.replaceAll(privatePath, "[本地路径已隐藏]");
    }
  }
  message = message
    .replace(/(?:[A-Za-z]:\\|\/Users\/|\/home\/)[^\s,;，；]+/g, "[本地路径已隐藏]")
    .slice(0, 240);
  return message;
}

export function buildFailedTaskSummary(tasks: readonly LocalTask[]): string {
  return tasks
    .filter(isRetryableOrReportedFailure)
    .map((task, index) => {
      const timestamp = new Date(
        task.finishedAt ?? task.createdAt
      ).toLocaleString("zh-CN", { hour12: false });
      return [
        `${index + 1}. ${taskOperationLabel(task)}`,
        `文件：${task.displayName}`,
        `错误：${conciseTaskError(task)}`,
        `时间：${timestamp}`
      ].join("\n");
    })
    .join("\n\n");
}

function isRetryableOrReportedFailure(task: LocalTask): boolean {
  return (
    task.reportStatus === "failed" ||
    task.reportStatus === "unsupported" ||
    (!task.reportStatus && task.status === "failed")
  );
}

function fileNameFromPath(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  return normalized.split("/").pop() ?? "";
}
