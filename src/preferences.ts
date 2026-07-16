export type ToolSection =
  | "pdf"
  | "image-convert"
  | "image-resize"
  | "image-compress"
  | "metadata-cleanup"
  | "report-export";

export type EnabledImageFormat = "jpg" | "png" | "webp";
export type ResizeMode = "fit" | "width" | "height";
export type ReportFormat = "csv" | "json";
export type OutputLocationMode =
  | "converted-folder-next-to-source"
  | "same-folder-as-source"
  | "ask-every-time"
  | "remembered-custom-folder";
export type OutputSuffixPreset =
  | "current"
  | "converted"
  | "resized"
  | "compressed"
  | "cleaned"
  | "custom";

export type UserPreferences = {
  schemaVersion: 1;
  activeTool: ToolSection;
  imageTargetFormat: EnabledImageFormat;
  resizeMode: ResizeMode;
  resizeWidth: number;
  resizeHeight: number;
  jpegCompressionQuality: number;
  webpCompressionQuality: number;
  preferencesPanelExpanded: boolean;
  reportFormat: ReportFormat;
  outputLocationMode: OutputLocationMode;
  rememberedOutputFolder: string;
  outputPrefix: string;
  outputSuffixPreset: OutputSuffixPreset;
  outputCustomSuffix: string;
  outputSettingsExpanded: boolean;
};

export type PreferenceInputState = Omit<
  UserPreferences,
  | "schemaVersion"
  | "resizeWidth"
  | "resizeHeight"
  | "jpegCompressionQuality"
  | "webpCompressionQuality"
> & {
  resizeWidth: string;
  resizeHeight: string;
  jpegCompressionQuality: string;
  webpCompressionQuality: string;
};

export type BuiltInPresetId =
  | "convert-png"
  | "convert-webp"
  | "resize-1920-width"
  | "compress-common"
  | "clean-metadata";

export type BuiltInPreset = {
  id: BuiltInPresetId;
  label: string;
};

export const maxPreferenceResizeDimension = 16_384;
export const maxPreferenceResizePixels = 64_000_000;
export const minPreferenceCompressionQuality = 40;
export const maxPreferenceCompressionQuality = 95;

export const defaultPreferences: UserPreferences = {
  schemaVersion: 1,
  activeTool: "pdf",
  imageTargetFormat: "webp",
  resizeMode: "fit",
  resizeWidth: 1_920,
  resizeHeight: 1_080,
  jpegCompressionQuality: 82,
  webpCompressionQuality: 80,
  preferencesPanelExpanded: true,
  reportFormat: "csv",
  outputLocationMode: "converted-folder-next-to-source",
  rememberedOutputFolder: "",
  outputPrefix: "",
  outputSuffixPreset: "current",
  outputCustomSuffix: "",
  outputSettingsExpanded: true
};

export const builtInPresets: readonly BuiltInPreset[] = [
  { id: "convert-png", label: "图片转 PNG" },
  { id: "convert-webp", label: "图片转 WebP" },
  { id: "resize-1920-width", label: "图片缩小到 1920 宽以内" },
  { id: "compress-common", label: "图片压缩为常用体积" },
  { id: "clean-metadata", label: "清理图片隐私元数据" }
];

const enabledImageExtensions = new Set(["jpg", "jpeg", "png", "webp"]);

export function isEnabledImageExtension(extension: string): boolean {
  return enabledImageExtensions.has(extension.toLowerCase());
}

export function applyBuiltInPreset(
  current: UserPreferences,
  presetId: BuiltInPresetId
): UserPreferences {
  switch (presetId) {
    case "convert-png":
      return {
        ...current,
        activeTool: "image-convert",
        imageTargetFormat: "png"
      };
    case "convert-webp":
      return {
        ...current,
        activeTool: "image-convert",
        imageTargetFormat: "webp"
      };
    case "resize-1920-width":
      return {
        ...current,
        activeTool: "image-resize",
        resizeMode: "width",
        resizeWidth: 1_920
      };
    case "compress-common":
      return {
        ...current,
        activeTool: "image-compress",
        jpegCompressionQuality: 82,
        webpCompressionQuality: 80
      };
    case "clean-metadata":
      return {
        ...current,
        activeTool: "metadata-cleanup"
      };
  }
}

export function buildPreferenceSnapshot(
  input: PreferenceInputState,
  fallback: UserPreferences = defaultPreferences
): UserPreferences {
  const resizeWidth = parseSafeInteger(
    input.resizeWidth,
    fallback.resizeWidth,
    1,
    maxPreferenceResizeDimension
  );
  let resizeHeight = parseSafeInteger(
    input.resizeHeight,
    fallback.resizeHeight,
    1,
    maxPreferenceResizeDimension
  );

  if (resizeWidth * resizeHeight > maxPreferenceResizePixels) {
    resizeHeight = Math.max(1, Math.floor(maxPreferenceResizePixels / resizeWidth));
  }

  return {
    schemaVersion: 1,
    activeTool: input.activeTool,
    imageTargetFormat: input.imageTargetFormat,
    resizeMode: input.resizeMode,
    resizeWidth,
    resizeHeight,
    jpegCompressionQuality: parseSafeInteger(
      input.jpegCompressionQuality,
      fallback.jpegCompressionQuality,
      minPreferenceCompressionQuality,
      maxPreferenceCompressionQuality
    ),
    webpCompressionQuality: parseSafeInteger(
      input.webpCompressionQuality,
      fallback.webpCompressionQuality,
      minPreferenceCompressionQuality,
      maxPreferenceCompressionQuality
    ),
    preferencesPanelExpanded: input.preferencesPanelExpanded,
    reportFormat: input.reportFormat,
    outputLocationMode:
      input.outputLocationMode === "remembered-custom-folder" &&
      !input.rememberedOutputFolder.trim()
        ? "converted-folder-next-to-source"
        : input.outputLocationMode,
    rememberedOutputFolder: input.rememberedOutputFolder.trim(),
    outputPrefix: input.outputPrefix.trim(),
    outputSuffixPreset:
      input.outputSuffixPreset === "custom" &&
      !input.outputCustomSuffix.trim()
        ? "current"
        : input.outputSuffixPreset,
    outputCustomSuffix: input.outputCustomSuffix.trim(),
    outputSettingsExpanded: input.outputSettingsExpanded
  };
}

function parseSafeInteger(
  input: string,
  fallback: number,
  minimum: number,
  maximum: number
): number {
  const normalized = input.trim();
  if (!/^\d+$/.test(normalized)) {
    return fallback;
  }

  const parsed = Number(normalized);
  if (!Number.isSafeInteger(parsed)) {
    return fallback;
  }

  return Math.min(maximum, Math.max(minimum, parsed));
}
