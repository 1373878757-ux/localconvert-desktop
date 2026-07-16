import { describe, expect, it } from "vitest";
import { defaultPreferences } from "./preferences";
import {
  timestampTokens,
  validateOutputAffix,
  validateOutputPreferences
} from "./outputSettings";

describe("output settings", () => {
  it("accepts supported date and time tokens", () => {
    expect(validateOutputAffix("{date}_客户_{time}", "前缀").valid).toBe(true);
  });

  it("rejects path separators and unknown tokens", () => {
    expect(validateOutputAffix("../unsafe", "前缀").valid).toBe(false);
    expect(validateOutputAffix("{timestamp}", "前缀").valid).toBe(false);
  });

  it("requires a remembered folder and custom suffix when selected", () => {
    expect(
      validateOutputPreferences({
        ...defaultPreferences,
        outputLocationMode: "remembered-custom-folder",
        rememberedOutputFolder: ""
      }).valid
    ).toBe(false);
    expect(
      validateOutputPreferences({
        ...defaultPreferences,
        outputSuffixPreset: "custom",
        outputCustomSuffix: ""
      }).valid
    ).toBe(false);
  });

  it("formats safe local date and time tokens", () => {
    expect(timestampTokens(new Date(2026, 6, 17, 9, 8, 7))).toEqual({
      dateToken: "2026-07-17",
      timeToken: "09-08-07"
    });
  });
});
