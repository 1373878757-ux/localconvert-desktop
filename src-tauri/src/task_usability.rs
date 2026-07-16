use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

const MAX_ERROR_SUMMARY_CHARS: usize = 320;
const MAX_FAILED_SUMMARY_CHARS: usize = 8_000;
const FALLBACK_ERROR_SUMMARY: &str = "任务未完成。请在应用内查看错误日志。";

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RevealLocalFileResult {
    success: bool,
    target_path: String,
    containing_folder_path: String,
    message: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CopyErrorSummaryResult {
    success: bool,
    summary: String,
    message: &'static str,
}

#[tauri::command]
pub(crate) fn reveal_local_file(
    app: AppHandle,
    path: String,
) -> Result<RevealLocalFileResult, String> {
    let target_path = validate_reveal_target(&path)?;
    let containing_folder = containing_folder(&target_path)?;

    app.opener()
        .reveal_item_in_dir(&target_path)
        .map_err(|error| format!("无法在文件管理器中定位该文件：{error}"))?;

    Ok(RevealLocalFileResult {
        success: true,
        target_path: path_to_string(&target_path),
        containing_folder_path: path_to_string(&containing_folder),
        message: "已打开文件所在位置。",
    })
}

#[tauri::command]
pub(crate) fn copy_error_summary(
    app: AppHandle,
    message: String,
) -> Result<CopyErrorSummaryResult, String> {
    let summary = concise_error_summary(&message);

    app.clipboard()
        .write_text(summary.clone())
        .map_err(|error| format!("剪贴板当前不可用：{error}"))?;

    Ok(CopyErrorSummaryResult {
        success: true,
        summary,
        message: "已复制",
    })
}

#[tauri::command]
pub(crate) fn copy_failed_task_summary(
    app: AppHandle,
    message: String,
) -> Result<CopyErrorSummaryResult, String> {
    let summary = safe_failed_task_summary(&message);

    app.clipboard()
        .write_text(summary.clone())
        .map_err(|error| format!("剪贴板当前不可用：{error}"))?;

    Ok(CopyErrorSummaryResult {
        success: true,
        summary,
        message: "已复制",
    })
}

fn validate_reveal_target(raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("没有可打开的本地文件路径。".to_string());
    }

    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err("只能打开经过验证的绝对本地路径。".to_string());
    }

    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("文件已不存在，无法打开输出位置：{trimmed}")
        } else {
            format!("无法检查本地文件路径 {trimmed}：{error}")
        }
    })?;

    if !metadata.is_file() {
        return Err("输出位置操作只接受现有本地文件。".to_string());
    }

    fs::canonicalize(path).map_err(|error| format!("无法解析本地文件路径 {trimmed}：{error}"))
}

fn containing_folder(target_path: &Path) -> Result<PathBuf, String> {
    let parent = target_path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| "无法确定该文件的所在文件夹。".to_string())?;
    Ok(parent.to_path_buf())
}

fn concise_error_summary(raw_message: &str) -> String {
    let first_safe_line = raw_message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| !is_diagnostic_or_raw_metadata_line(line));

    let Some(line) = first_safe_line else {
        return FALLBACK_ERROR_SUMMARY.to_string();
    };

    let safe_prefix = strip_inline_raw_metadata(line);
    let normalized = safe_prefix
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_string();

    if normalized.is_empty() {
        return FALLBACK_ERROR_SUMMARY.to_string();
    }

    truncate_summary(&normalized)
}

fn safe_failed_task_summary(raw_message: &str) -> String {
    let summary = raw_message
        .lines()
        .map(str::trim)
        .filter(|line| !is_diagnostic_or_raw_metadata_line(line))
        .map(strip_inline_raw_metadata)
        .map(redact_private_path_suffix)
        .map(|line| {
            line.chars()
                .filter(|character| !character.is_control())
                .collect::<String>()
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if summary.is_empty() {
        return FALLBACK_ERROR_SUMMARY.to_string();
    }

    truncate_to_chars(&summary, MAX_FAILED_SUMMARY_CHARS)
}

fn redact_private_path_suffix(line: &str) -> String {
    let unix_path = ["/Users/", "/home/"]
        .iter()
        .filter_map(|marker| line.find(marker))
        .min();
    let windows_path = line
        .char_indices()
        .collect::<Vec<_>>()
        .windows(3)
        .find_map(|window| {
            let [(start, drive), (_, colon), (_, slash)] = window else {
                return None;
            };
            (drive.is_ascii_alphabetic() && *colon == ':' && (*slash == '\\' || *slash == '/'))
                .then_some(*start)
        });
    let path_start = unix_path.into_iter().chain(windows_path).min();

    match path_start {
        Some(index) => format!("{}[本地路径已隐藏]", line[..index].trim_end()),
        None => line.to_string(),
    }
}

fn truncate_to_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect::<String>()
}

fn is_diagnostic_or_raw_metadata_line(line: &str) -> bool {
    let lowercase = line.to_ascii_lowercase();
    let prefixes = [
        "stdout:",
        "stderr:",
        "exif:",
        "exif=",
        "gps:",
        "gps=",
        "xmp:",
        "xmp=",
        "iptc:",
        "iptc=",
        "raw metadata:",
        "raw_metadata:",
    ];

    prefixes.iter().any(|prefix| lowercase.starts_with(prefix))
        || lowercase.starts_with("<x:xmpmeta")
        || lowercase.starts_with("<rdf:rdf")
}

fn strip_inline_raw_metadata(line: &str) -> &str {
    const MARKERS: [&str; 10] = [
        " EXIF:",
        " EXIF=",
        " GPS:",
        " GPS=",
        " XMP:",
        " XMP=",
        " IPTC:",
        " IPTC=",
        " raw metadata:",
        " raw_metadata:",
    ];

    let lowercase = line.to_ascii_lowercase();
    let cutoff = MARKERS
        .iter()
        .filter_map(|marker| lowercase.find(&marker.to_ascii_lowercase()))
        .min()
        .unwrap_or(line.len());

    &line[..cutoff]
}

fn truncate_summary(summary: &str) -> String {
    if summary.chars().count() <= MAX_ERROR_SUMMARY_CHARS {
        return summary.to_string();
    }

    let mut truncated = summary
        .chars()
        .take(MAX_ERROR_SUMMARY_CHARS.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, File},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("localconvert-task-usability-{name}"))
            .join(unique.to_string())
    }

    #[test]
    fn reveal_target_rejects_empty_relative_and_missing_paths() {
        assert_eq!(
            validate_reveal_target(" ").expect_err("empty path should fail"),
            "没有可打开的本地文件路径。"
        );
        assert_eq!(
            validate_reveal_target("converted/report.pdf").expect_err("relative path should fail"),
            "只能打开经过验证的绝对本地路径。"
        );

        let missing = temp_case_dir("missing").join("不存在.pdf");
        let error = validate_reveal_target(&path_to_string(&missing))
            .expect_err("missing path should fail");
        assert!(error.contains("文件已不存在"));
    }

    #[test]
    fn reveal_target_resolves_file_and_parent_without_shell_planning() {
        let case_dir = temp_case_dir("中文 path with spaces");
        fs::create_dir_all(&case_dir).expect("test directory should exist");
        let file = case_dir.join("输出 report.pdf");
        File::create(&file).expect("test file should exist");

        let target = validate_reveal_target(&path_to_string(&file))
            .expect("existing absolute file should validate");
        let parent = containing_folder(&target).expect("file should have a parent");

        assert_eq!(
            target,
            fs::canonicalize(&file).expect("file should resolve")
        );
        assert_eq!(
            parent,
            fs::canonicalize(&case_dir).expect("parent should resolve")
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn reveal_target_rejects_directories() {
        let case_dir = temp_case_dir("directory");
        fs::create_dir_all(&case_dir).expect("test directory should exist");

        let error = validate_reveal_target(&path_to_string(&case_dir))
            .expect_err("directory should not be accepted as an output file");
        assert_eq!(error, "输出位置操作只接受现有本地文件。");

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn copied_error_summary_excludes_diagnostics_and_raw_metadata_payloads() {
        let message = concat!(
            "不支持 HEIC 元数据清理。 EXIF: hidden-device-data\n",
            "GPS: 31.2304,121.4737\n",
            "XMP: <secret/>\n",
            "IPTC: private payload\n",
            "stderr: decoder detail"
        );

        let summary = concise_error_summary(message);

        assert_eq!(summary, "不支持 HEIC 元数据清理。");
        assert!(!summary.contains("hidden-device-data"));
        assert!(!summary.contains("31.2304"));
        assert!(!summary.contains("<secret/>"));
        assert!(!summary.contains("private payload"));
    }

    #[test]
    fn copied_error_summary_uses_safe_fallback_for_payload_only_input() {
        let summary = concise_error_summary("EXIF: secret\nGPS: 1,2\n<x:xmpmeta>raw</x:xmpmeta>");

        assert_eq!(summary, FALLBACK_ERROR_SUMMARY);
    }

    #[test]
    fn copied_error_summary_is_concise() {
        let long_message = "处理失败".repeat(200);
        let summary = concise_error_summary(&long_message);

        assert_eq!(summary.chars().count(), MAX_ERROR_SUMMARY_CHARS);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn failed_task_summary_keeps_structure_and_redacts_private_paths() {
        let message = concat!(
            "1. 图片格式转换\n",
            "文件：示例.jpg\n",
            "错误：无法读取 /Users/private/示例.jpg\n",
            "时间：2026/7/17 10:00:00\n\n",
            "2. 合并 PDF\n",
            "文件：report.pdf\n",
            "错误：C:\\Private\\report.pdf 无法打开\n",
            "EXIF: secret payload\n"
        );

        let summary = safe_failed_task_summary(message);

        assert!(summary.contains("图片格式转换"));
        assert!(summary.contains("合并 PDF"));
        assert!(summary.contains("[本地路径已隐藏]"));
        assert!(!summary.contains("/Users/private"));
        assert!(!summary.contains("C:\\Private"));
        assert!(!summary.contains("secret payload"));
    }
}
