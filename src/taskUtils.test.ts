import { describe, expect, it } from "vitest";
import { mapBackendTaskStatus } from "./taskUtils";

describe("backend task status mapping", () => {
  it("preserves the existing terminal-state mapping", () => {
    expect(mapBackendTaskStatus("completed", true)).toBe("completed");
    expect(mapBackendTaskStatus("completed", false)).toBe("failed");
    expect(mapBackendTaskStatus("failed", false)).toBe("failed");
    expect(mapBackendTaskStatus("cancelled", true)).toBe("cancelled");
    expect(mapBackendTaskStatus("running", true)).toBe("failed");
    expect(mapBackendTaskStatus("queued", true)).toBe("failed");
  });
});
