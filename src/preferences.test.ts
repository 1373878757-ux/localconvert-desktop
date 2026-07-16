import { describe, expect, it } from "vitest";
import {
  applyBuiltInPreset,
  buildPreferenceSnapshot,
  builtInPresets,
  defaultPreferences,
  isEnabledImageExtension,
  maxPreferenceResizePixels
} from "./preferences";

describe("local preference defaults", () => {
  it("keeps compression defaults inside the enabled range", () => {
    expect(defaultPreferences.jpegCompressionQuality).toBe(82);
    expect(defaultPreferences.webpCompressionQuality).toBe(80);
  });

  it("falls back or clamps invalid numeric input safely", () => {
    const snapshot = buildPreferenceSnapshot({
      ...defaultPreferences,
      resizeWidth: "999999",
      resizeHeight: "999999",
      jpegCompressionQuality: "not-a-number",
      webpCompressionQuality: "100"
    });

    expect(snapshot.jpegCompressionQuality).toBe(82);
    expect(snapshot.webpCompressionQuality).toBe(95);
    expect(snapshot.resizeWidth * snapshot.resizeHeight).toBeLessThanOrEqual(
      maxPreferenceResizePixels
    );
  });
});

describe("built-in presets", () => {
  it("prefills existing options without exposing an execution action", () => {
    const png = applyBuiltInPreset(defaultPreferences, "convert-png");
    const resize = applyBuiltInPreset(defaultPreferences, "resize-1920-width");
    const compress = applyBuiltInPreset(defaultPreferences, "compress-common");
    const clean = applyBuiltInPreset(defaultPreferences, "clean-metadata");

    expect(png).toMatchObject({
      activeTool: "image-convert",
      imageTargetFormat: "png"
    });
    expect(resize).toMatchObject({
      activeTool: "image-resize",
      resizeMode: "width",
      resizeWidth: 1920
    });
    expect(compress).toMatchObject({
      activeTool: "image-compress",
      jpegCompressionQuality: 82,
      webpCompressionQuality: 80
    });
    expect(clean.activeTool).toBe("metadata-cleanup");
    expect(Object.keys(png)).not.toContain("execute");
    expect(builtInPresets).toHaveLength(5);
  });

  it("does not enable HEIC after any preset is applied", () => {
    for (const preset of builtInPresets) {
      applyBuiltInPreset(defaultPreferences, preset.id);
      expect(isEnabledImageExtension("heic")).toBe(false);
    }
  });
});
