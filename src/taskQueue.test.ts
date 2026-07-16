import { describe, expect, it, vi } from "vitest";
import { LocalTask } from "./taskUtils";
import {
  buildFailedTaskSummary,
  cloneTaskForRetry,
  filterTasks,
  isTerminalTask,
  removeTerminalTasks,
  summarizeTasks,
  unavailableRetryTaskNames
} from "./taskQueue";

function task(overrides: Partial<LocalTask> = {}): LocalTask {
  return {
    taskId: "task-1",
    displayName: "示例 文件.jpg",
    size: 120,
    extension: "jpg",
    sourcePath: "/Users/private/示例 文件.jpg",
    sourceKind: "native-path",
    sourcePreview: "/Users/private/示例 文件.jpg",
    outputPreview: "converted/示例 文件.png",
    status: "waiting",
    errorLog: "",
    createdAt: 1_700_000_000_000,
    ...overrides
  };
}

describe("task queue filters and counters", () => {
  const tasks = [
    task({ taskId: "running", status: "converting", operationType: "image_resize" }),
    task({ taskId: "success", status: "completed", reportStatus: "success" }),
    task({ taskId: "failed", status: "failed", reportStatus: "failed" }),
    task({ taskId: "cancelled", status: "cancelled", reportStatus: "cancelled" }),
    task({ taskId: "skipped", status: "completed", reportStatus: "skipped" }),
    task({ taskId: "smaller", status: "completed", reportStatus: "not_smaller" })
  ];

  it("filters every queue result category", () => {
    expect(filterTasks(tasks, "running", "").map((item) => item.taskId)).toEqual(["running"]);
    expect(filterTasks(tasks, "success", "").map((item) => item.taskId)).toEqual(["success"]);
    expect(filterTasks(tasks, "failed", "").map((item) => item.taskId)).toEqual(["failed"]);
    expect(filterTasks(tasks, "cancelled", "").map((item) => item.taskId)).toEqual(["cancelled"]);
    expect(filterTasks(tasks, "skipped", "").map((item) => item.taskId)).toEqual(["skipped", "smaller"]);
  });

  it("searches source name, output name, and operation label", () => {
    const searchable = task({
      operationType: "pdf_merge",
      reportOutputName: "merged result.pdf"
    });
    expect(filterTasks([searchable], "all", "示例 文件")).toHaveLength(1);
    expect(filterTasks([searchable], "all", "merged result")).toHaveLength(1);
    expect(filterTasks([searchable], "all", "合并 PDF")).toHaveLength(1);
    expect(filterTasks([searchable], "all", "找不到")).toHaveLength(0);
  });

  it("counts skipped and not-smaller together", () => {
    expect(summarizeTasks(tasks)).toEqual({
      total: 6,
      running: 1,
      success: 1,
      failed: 1,
      cancelled: 1,
      skipped: 2
    });
  });
});

describe("queue cleanup and retry", () => {
  it("removes terminal records without removing waiting or running tasks", () => {
    const waiting = task({ taskId: "waiting" });
    const running = task({ taskId: "running", status: "converting" });
    const failed = task({ taskId: "failed", status: "failed" });
    expect(isTerminalTask(waiting)).toBe(false);
    expect(isTerminalTask(running)).toBe(false);
    expect(removeTerminalTasks([waiting, running, failed]).map((item) => item.taskId)).toEqual([
      "waiting",
      "running"
    ]);
  });

  it("creates a new task ID and preserves retry parameters", () => {
    vi.spyOn(Math, "random").mockReturnValueOnce(0.25);
    const original = task({
      taskId: "old",
      status: "failed",
      reportStatus: "failed",
      retryDescriptor: { kind: "image-convert", targetFormat: "webp" },
      reportMessage: "failed"
    });
    const retry = cloneTaskForRetry(original, 1_800_000_000_000);
    expect(retry.taskId).not.toBe(original.taskId);
    expect(retry.retryDescriptor).toEqual(original.retryDescriptor);
    expect(retry.status).toBe("waiting");
    expect(retry.reportStatus).toBeUndefined();
    expect(original.taskId).toBe("old");
    expect(original.reportStatus).toBe("failed");
  });

  it("detects missing retry sources before execution", () => {
    const existing = task({ taskId: "existing", sourcePath: "/tmp/exists.jpg" });
    const missing = task({ taskId: "missing", sourcePath: "/tmp/missing.jpg" });
    expect(
      unavailableRetryTaskNames([existing, missing], ["/tmp/exists.jpg"])
    ).toEqual(["示例 文件.jpg"]);
  });
});

describe("failed summary privacy", () => {
  it("includes filename and operation but removes full paths", () => {
    const failed = task({
      status: "failed",
      reportStatus: "failed",
      operationType: "image_convert",
      reportMessage: "无法读取 /Users/private/示例 文件.jpg",
      finishedAt: 1_700_000_000_100
    });
    const summary = buildFailedTaskSummary([failed]);
    expect(summary).toContain("图片格式转换");
    expect(summary).toContain("文件：示例 文件.jpg");
    expect(summary).not.toContain("/Users/private");
    expect(summary).toContain("[本地路径已隐藏]");
  });
});
