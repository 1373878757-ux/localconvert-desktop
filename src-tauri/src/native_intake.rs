use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const NATIVE_PATH_SOURCE_KIND: &str = "native-path";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectNativePathsRequest {
    paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePathMetadata {
    source_path: String,
    display_name: String,
    extension: String,
    size: u64,
    source_kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedNativePath {
    source_path: String,
    message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePathInspection {
    files: Vec<NativePathMetadata>,
    rejected: Vec<RejectedNativePath>,
}

#[tauri::command]
pub fn inspect_native_paths(
    request: InspectNativePathsRequest,
) -> Result<NativePathInspection, String> {
    inspect_native_paths_inner(&request)
}

fn inspect_native_paths_inner(
    request: &InspectNativePathsRequest,
) -> Result<NativePathInspection, String> {
    if request.paths.is_empty() {
        return Err("至少需要一个本地文件路径。".to_string());
    }

    let mut files = Vec::new();
    let mut rejected = Vec::new();

    for source in &request.paths {
        match inspect_native_path(source) {
            Ok(metadata) => files.push(metadata),
            Err(message) => rejected.push(RejectedNativePath {
                source_path: source.clone(),
                message,
            }),
        }
    }

    Ok(NativePathInspection { files, rejected })
}

fn inspect_native_path(source: &str) -> Result<NativePathMetadata, String> {
    if source.trim().is_empty() {
        return Err("本地文件路径不能为空。".to_string());
    }

    let path = Path::new(source);
    if !path.is_absolute() {
        return Err("必须使用绝对本地路径。".to_string());
    }

    let metadata =
        fs::metadata(path).map_err(|error| format!("无法读取本地文件元数据：{error}"))?;
    if !metadata.is_file() {
        return Err("仅支持文件，文件夹导入尚未启用。".to_string());
    }

    let display_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| "无法确定本地文件名。".to_string())?;
    let extension = path
        .extension()
        .filter(|extension| !extension.is_empty())
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "未知".to_string());

    Ok(NativePathMetadata {
        source_path: path.to_string_lossy().into_owned(),
        display_name,
        extension,
        size: metadata.len(),
        source_kind: NATIVE_PATH_SOURCE_KIND.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "localconvert-native-intake-{}-{name}-{unique}",
            std::process::id()
        ))
    }

    fn request(paths: Vec<String>) -> InspectNativePathsRequest {
        InspectNativePathsRequest { paths }
    }

    fn path_to_string(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn inspects_native_file_metadata_without_reading_contents() {
        let case_dir = temp_case_dir("metadata");
        fs::create_dir_all(&case_dir).expect("fixture directory should be created");
        let source = case_dir.join("report.PDF");
        fs::write(&source, b"local metadata fixture").expect("fixture should be written");

        let result = inspect_native_paths_inner(&request(vec![path_to_string(&source)]))
            .expect("native path inspection should succeed");

        assert!(result.rejected.is_empty());
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].display_name, "report.PDF");
        assert_eq!(result.files[0].extension, "pdf");
        assert_eq!(result.files[0].size, 22);
        assert_eq!(result.files[0].source_kind, "native-path");
        assert_eq!(result.files[0].source_path, path_to_string(&source));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn preserves_chinese_names_and_spaces_in_absolute_paths() {
        let case_dir = temp_case_dir("中文 路径");
        fs::create_dir_all(&case_dir).expect("fixture directory should be created");
        let source = case_dir.join("客户 报告 终稿.pdf");
        fs::write(&source, b"pdf").expect("fixture should be written");

        let result = inspect_native_paths_inner(&request(vec![path_to_string(&source)]))
            .expect("Chinese path inspection should succeed");

        assert_eq!(result.files[0].display_name, "客户 报告 终稿.pdf");
        assert_eq!(result.files[0].source_path, path_to_string(&source));
        assert!(result.rejected.is_empty());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_missing_files_and_directories_without_blocking_valid_files() {
        let case_dir = temp_case_dir("mixed");
        fs::create_dir_all(&case_dir).expect("fixture directory should be created");
        let valid = case_dir.join("valid.png");
        let missing = case_dir.join("missing.pdf");
        fs::write(&valid, b"png").expect("fixture should be written");

        let result = inspect_native_paths_inner(&request(vec![
            path_to_string(&missing),
            path_to_string(&case_dir),
            path_to_string(&valid),
        ]))
        .expect("mixed inspection should return per-path results");

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].display_name, "valid.png");
        assert_eq!(result.rejected.len(), 2);
        assert!(result.rejected[0]
            .message
            .contains("无法读取本地文件元数据"));
        assert_eq!(
            result.rejected[1].message,
            "仅支持文件，文件夹导入尚未启用。"
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_empty_requests_and_relative_paths() {
        let empty_error = inspect_native_paths_inner(&request(Vec::new()))
            .expect_err("empty request should fail");
        assert_eq!(empty_error, "至少需要一个本地文件路径。");

        let result = inspect_native_paths_inner(&request(vec!["relative/report.pdf".to_string()]))
            .expect("relative path should be reported as a rejected item");
        assert!(result.files.is_empty());
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].message, "必须使用绝对本地路径。");
    }
}
