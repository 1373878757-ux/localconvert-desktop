import {
  OutputLocationMode,
  OutputSuffixPreset,
  UserPreferences
} from "./preferences";

export type OutputRuleValidation = {
  valid: boolean;
  message: string;
};

export type OutputPlanningPreferences = Pick<
  UserPreferences,
  | "outputLocationMode"
  | "rememberedOutputFolder"
  | "outputPrefix"
  | "outputSuffixPreset"
  | "outputCustomSuffix"
>;

export const outputLocationLabels: Record<OutputLocationMode, string> = {
  "converted-folder-next-to-source": "源文件旁的 converted 文件夹",
  "same-folder-as-source": "源文件所在文件夹",
  "ask-every-time": "每次处理前选择文件夹",
  "remembered-custom-folder": "记住的自定义文件夹"
};

export const outputSuffixLabels: Record<OutputSuffixPreset, string> = {
  current: "沿用当前操作命名",
  converted: "_converted",
  resized: "_resized",
  compressed: "_compressed",
  cleaned: "_cleaned",
  custom: "自定义后缀"
};

const unsafeFilenameCharacters = /[\\/:*?"<>|\u0000-\u001f\u007f]/;
const supportedTokenPattern = /\{(?:date|time)\}/g;

export function validateOutputAffix(
  value: string,
  label: string,
  allowEmpty = true
): OutputRuleValidation {
  const normalized = value.trim();
  if (!normalized) {
    return allowEmpty
      ? { valid: true, message: "" }
      : { valid: false, message: `${label}不能为空。` };
  }
  if (normalized.length > 80) {
    return { valid: false, message: `${label}不能超过 80 个字符。` };
  }
  if (unsafeFilenameCharacters.test(normalized)) {
    return {
      valid: false,
      message: `${label}不能包含路径分隔符或 : * ? " < > | 等不安全字符。`
    };
  }
  const withoutTokens = normalized.replace(supportedTokenPattern, "");
  if (withoutTokens.includes("{") || withoutTokens.includes("}")) {
    return {
      valid: false,
      message: `${label}仅支持 {date} 和 {time} 标记。`
    };
  }
  return { valid: true, message: "" };
}

export function validateOutputPreferences(
  preferences: OutputPlanningPreferences
): OutputRuleValidation {
  const prefix = validateOutputAffix(preferences.outputPrefix, "文件名前缀");
  if (!prefix.valid) {
    return prefix;
  }
  if (preferences.outputSuffixPreset === "custom") {
    const suffix = validateOutputAffix(
      preferences.outputCustomSuffix,
      "自定义后缀",
      false
    );
    if (!suffix.valid) {
      return suffix;
    }
  }
  if (
    preferences.outputLocationMode === "remembered-custom-folder" &&
    !preferences.rememberedOutputFolder.trim()
  ) {
    return { valid: false, message: "请先选择要记住的自定义输出文件夹。" };
  }
  return { valid: true, message: "" };
}

export function timestampTokens(now = new Date()) {
  const pad = (value: number) => value.toString().padStart(2, "0");
  return {
    dateToken: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
    timeToken: `${pad(now.getHours())}-${pad(now.getMinutes())}-${pad(now.getSeconds())}`
  };
}
