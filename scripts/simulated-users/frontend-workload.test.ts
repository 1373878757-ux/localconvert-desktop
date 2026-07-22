import { describe, expect, it, vi } from "vitest";
import { applyBuiltInPreset, builtInPresets, defaultPreferences, isEnabledImageExtension } from "../../src/preferences";
import {
  cloneTaskForRetry,
  filterTasks,
  removeTerminalTasks,
  summarizeTasks,
  type TaskQueueFilter
} from "../../src/taskQueue";
import type { LocalTask, TaskReportStatus, TaskStatus } from "../../src/taskUtils";

const taskCount = 10_000;
const filters: TaskQueueFilter[] = ["all", "running", "success", "failed", "cancelled", "skipped"];

describe("10,000-record simulated queue state", () => {
  it("filters, searches, counts, retries, clears, and builds report selections without mutation", () => {
    const tasks = buildTasks(taskCount, 0x4c43443039300001n);
    const original = JSON.stringify(tasks);
    const started = performance.now();

    for (const filter of filters) {
      const filtered = filterTasks(tasks, filter, "");
      expect(filtered.every((task) => taskMatchesExpectedFilter(task, filter))).toBe(true);
    }
    expect(filterTasks(tasks, "all", "中文").length).toBeGreaterThan(0);
    expect(filterTasks(tasks, "all", "REPORT RESULT").length).toBeGreaterThan(0);
    expect(filterTasks(tasks, "all", "e\u0301").length).toBeGreaterThan(0);
    expect(filterTasks(tasks, "all", "é")).toHaveLength(0);

    const summary = summarizeTasks(tasks);
    expect(summary.total).toBe(taskCount);
    expect(Object.values(summary).every(Number.isFinite)).toBe(true);

    const failed = filterTasks(tasks, "failed", "").slice(0, 100);
    vi.spyOn(Math, "random").mockImplementation(seededRandom(0x90n));
    const retries = failed.map((task, index) => cloneTaskForRetry(task, 1_900_000_000_000 + index));
    expect(new Set(retries.map((task) => task.taskId)).size).toBe(retries.length);
    expect(retries.every((task) => task.status === "waiting" && task.reportStatus === undefined)).toBe(true);
    expect(failed.every((task, index) => task.taskId !== retries[index].taskId)).toBe(true);

    const active = removeTerminalTasks(tasks);
    expect(active.every((task) => task.status === "waiting" || task.status === "converting")).toBe(true);
    const filteredReportIds = filterTasks(tasks, "skipped", "").map((task) => task.taskId);
    expect(filteredReportIds.every((id) => tasks.some((task) => task.taskId === id))).toBe(true);
    expect(JSON.stringify(tasks)).toBe(original);

    const elapsed = performance.now() - started;
    expect(elapsed).toBeLessThan(5_000);
  });

  it("keeps built-in presets parameter-only and HEIC disabled", () => {
    for (const preset of builtInPresets) {
      const next = applyBuiltInPreset(defaultPreferences, preset.id);
      expect(Object.keys(next)).not.toContain("execute");
      expect(isEnabledImageExtension("heic")).toBe(false);
    }
  });
});

function buildTasks(count: number, seed: bigint): LocalTask[] {
  const random = xorshift(seed);
  const statuses: TaskStatus[] = ["waiting", "converting", "completed", "failed", "cancelled"];
  const reports: TaskReportStatus[] = ["success", "failed", "cancelled", "skipped", "not_smaller", "unsupported"];
  const operations = ["pdf_merge", "image_convert", "image_resize", "image_compress", "image_metadata_cleanup"];
  return Array.from({ length: count }, (_, index) => {
    const status = statuses[Math.floor(random() * statuses.length)];
    const reportStatus = status === "waiting" || status === "converting"
      ? undefined
      : reports[Math.floor(random() * reports.length)];
    const unicodeName = index % 41 === 0 ? "中文 文件.jpg" : index % 43 === 0 ? "cafe\u0301.png" : `fixture-${index}.webp`;
    return {
      taskId: `task-${index}`,
      displayName: unicodeName,
      size: 1024 + index,
      extension: unicodeName.split(".").pop() ?? "jpg",
      sourcePath: `/simulation/session-${index % 100}/${unicodeName}`,
      sourceKind: "native-path",
      sourcePreview: unicodeName,
      outputPreview: `converted/output-${index}.png`,
      status,
      errorLog: status === "failed" ? "controlled failure" : "",
      createdAt: 1_800_000_000_000 + index,
      operationType: operations[index % operations.length],
      reportStatus,
      reportOutputName: index % 37 === 0 ? `report result ${index}.csv` : undefined,
      retryDescriptor: status === "failed" ? { kind: "image-convert", targetFormat: "png" } : undefined
    };
  });
}

function taskMatchesExpectedFilter(task: LocalTask, filter: TaskQueueFilter): boolean {
  if (filter === "all") return true;
  if (filter === "running") return task.status === "converting";
  if (filter === "success") return task.reportStatus === "success" || (!task.reportStatus && task.status === "completed");
  if (filter === "failed") return task.reportStatus === "failed" || task.reportStatus === "unsupported" || (!task.reportStatus && task.status === "failed");
  if (filter === "cancelled") return task.reportStatus === "cancelled" || (!task.reportStatus && task.status === "cancelled");
  return task.reportStatus === "skipped" || task.reportStatus === "not_smaller";
}

function xorshift(seed: bigint): () => number {
  let state = seed || 1n;
  return () => {
    state ^= state << 13n;
    state ^= state >> 7n;
    state ^= state << 17n;
    state &= (1n << 64n) - 1n;
    return Number(state & 0xffff_ffffn) / 0x1_0000_0000;
  };
}

function seededRandom(seed: bigint): () => number {
  return xorshift(seed);
}
