use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const QPDF_EXECUTION_DISABLED_ERROR: &str =
    "qpdf engine smoke checks are enabled, but PDF output-writing operations are not enabled yet.";

const QPDF_ENGINE_MISSING_MESSAGE: &str = "Not bundled yet.";

pub struct QpdfEngineDetection {
    pub status: &'static str,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QpdfMergeRequest {
    sources: Vec<String>,
    output: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QpdfSplitRequest {
    source: String,
    output_directory: String,
    filename_prefix: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QpdfExtractPagesRequest {
    source: String,
    pages: String,
    output: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QpdfRotatePagesRequest {
    source: String,
    pages: String,
    degrees: i16,
    output: String,
}

#[derive(Debug, PartialEq, Eq)]
struct QpdfCommandPlan {
    executable: &'static str,
    arguments: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct PageRange {
    start: u32,
    end: u32,
}

#[tauri::command]
pub fn qpdf_merge_pdfs(request: QpdfMergeRequest) -> Result<(), String> {
    build_qpdf_merge_arguments(&request)?;
    Err(qpdf_execution_disabled_error())
}

#[tauri::command]
pub fn qpdf_split_pdf(request: QpdfSplitRequest) -> Result<(), String> {
    build_qpdf_split_arguments(&request)?;
    Err(qpdf_execution_disabled_error())
}

#[tauri::command]
pub fn qpdf_extract_pages(request: QpdfExtractPagesRequest) -> Result<(), String> {
    build_qpdf_extract_arguments(&request)?;
    Err(qpdf_execution_disabled_error())
}

#[tauri::command]
pub fn qpdf_rotate_pages(request: QpdfRotatePagesRequest) -> Result<(), String> {
    build_qpdf_rotate_arguments(&request)?;
    Err(qpdf_execution_disabled_error())
}

pub fn detect_qpdf_engine(platform: &str) -> QpdfEngineDetection {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = qpdf_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    detect_qpdf_engine_from_candidates(platform, &candidates, run_qpdf_version_smoke_check)
}

fn qpdf_execution_disabled_error() -> String {
    QPDF_EXECUTION_DISABLED_ERROR.to_string()
}

fn detect_qpdf_engine_from_candidates<F>(
    platform: &str,
    candidates: &[PathBuf],
    smoke_check: F,
) -> QpdfEngineDetection
where
    F: Fn(&Path) -> Result<String, String>,
{
    let Some(candidate) = candidates.iter().find(|path| path.exists()) else {
        return QpdfEngineDetection {
            status: "not-installed",
            message: QPDF_ENGINE_MISSING_MESSAGE.to_string(),
        };
    };

    let candidate_display = path_to_string(candidate);
    let Ok(metadata) = fs::metadata(candidate) else {
        return QpdfEngineDetection {
            status: "error",
            message: format!("qpdf sidecar exists but could not be inspected: {candidate_display}"),
        };
    };

    if !metadata.is_file() {
        return QpdfEngineDetection {
            status: "error",
            message: format!("qpdf sidecar path is not a file: {candidate_display}"),
        };
    }

    if requires_executable_permission(platform) && !is_executable(&metadata) {
        return QpdfEngineDetection {
            status: "error",
            message: format!("qpdf sidecar is not executable: {candidate_display}"),
        };
    }

    let version = match smoke_check(candidate) {
        Ok(version) => version,
        Err(error) => {
            return QpdfEngineDetection {
                status: "error",
                message: format!("qpdf sidecar smoke check failed: {error}"),
            };
        }
    };

    QpdfEngineDetection {
        status: "available",
        message: format!("qpdf sidecar smoke check passed: {version} at {candidate_display}"),
    }
}

fn run_qpdf_version_smoke_check(path: &Path) -> Result<String, String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| {
            format!(
                "unable to run qpdf --version for {}: {error}",
                path_to_string(path)
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "qpdf --version exited with status {}{}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout
        .lines()
        .find(|line| line.starts_with("qpdf version "))
        .ok_or_else(|| "qpdf --version did not return a qpdf version line".to_string())?;

    Ok(version_line.to_string())
}

fn qpdf_candidate_paths(
    platform: &str,
    src_tauri_dir: &Path,
    runtime_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths = vec![
        src_tauri_dir
            .join("binaries")
            .join(platform)
            .join(qpdf_raw_filename(platform)),
        src_tauri_dir
            .join("binaries")
            .join(platform)
            .join(qpdf_prepared_filename(platform)),
    ];

    if let Some(runtime_dir) = runtime_dir {
        paths.push(runtime_dir.join(qpdf_prepared_filename(platform)));
        paths.push(runtime_dir.join(qpdf_raw_filename(platform)));
    }

    paths
}

fn qpdf_raw_filename(platform: &str) -> String {
    if platform.starts_with("windows-") {
        "qpdf.exe".to_string()
    } else {
        "qpdf".to_string()
    }
}

fn qpdf_prepared_filename(platform: &str) -> String {
    let extension = if platform.starts_with("windows-") {
        ".exe"
    } else {
        ""
    };

    format!("qpdf-{}{}", target_triple(platform), extension)
}

fn target_triple(platform: &str) -> &'static str {
    match platform {
        "windows-x86_64" => "x86_64-pc-windows-msvc",
        "windows-aarch64" => "aarch64-pc-windows-msvc",
        "macos-aarch64" => "aarch64-apple-darwin",
        "macos-x86_64" => "x86_64-apple-darwin",
        "linux-x86_64" => "x86_64-unknown-linux-gnu",
        _ => "unknown",
    }
}

fn requires_executable_permission(platform: &str) -> bool {
    platform.starts_with("macos-") || platform.starts_with("linux-")
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

fn build_qpdf_merge_arguments(request: &QpdfMergeRequest) -> Result<QpdfCommandPlan, String> {
    if request.sources.len() < 2 {
        return Err("Merge requires at least two source PDFs.".to_string());
    }

    let mut arguments = vec!["--empty".to_string(), "--pages".to_string()];
    for source in &request.sources {
        arguments.push(validate_path_like(source, "Source PDF")?);
    }
    arguments.push("--".to_string());
    arguments.push(validate_path_like(&request.output, "Output PDF")?);

    Ok(qpdf_plan(arguments))
}

fn build_qpdf_split_arguments(request: &QpdfSplitRequest) -> Result<QpdfCommandPlan, String> {
    let source = validate_path_like(&request.source, "Source PDF")?;
    let output_directory = validate_path_like(&request.output_directory, "Output directory")?;
    let filename_prefix = request
        .filename_prefix
        .as_deref()
        .map(str::trim)
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or("page");
    let output_pattern = Path::new(&output_directory).join(format!("{filename_prefix}-%d.pdf"));

    Ok(qpdf_plan(vec![
        "--split-pages".to_string(),
        source,
        path_to_string(&output_pattern),
    ]))
}

fn build_qpdf_extract_arguments(
    request: &QpdfExtractPagesRequest,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_path_like(&request.source, "Source PDF")?;
    let output = validate_path_like(&request.output, "Output PDF")?;
    let pages = normalize_page_ranges(&request.pages)?;

    Ok(qpdf_plan(vec![
        source,
        "--pages".to_string(),
        ".".to_string(),
        pages,
        "--".to_string(),
        output,
    ]))
}

fn build_qpdf_rotate_arguments(
    request: &QpdfRotatePagesRequest,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_path_like(&request.source, "Source PDF")?;
    let output = validate_path_like(&request.output, "Output PDF")?;
    let pages = normalize_page_ranges(&request.pages)?;
    let degrees = normalize_rotation_degrees(request.degrees)?;

    Ok(qpdf_plan(vec![
        source,
        output,
        format!("--rotate={degrees}:{pages}"),
    ]))
}

fn qpdf_plan(arguments: Vec<String>) -> QpdfCommandPlan {
    QpdfCommandPlan {
        executable: "qpdf",
        arguments,
    }
}

fn validate_path_like(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} path is required."));
    }

    if trimmed.chars().any(|character| character == '\0') {
        return Err(format!("{label} path contains an invalid null character."));
    }

    Ok(trimmed.to_string())
}

fn normalize_rotation_degrees(degrees: i16) -> Result<String, String> {
    match degrees {
        90 | 180 | 270 => Ok(format!("+{degrees}")),
        -90 | -180 | -270 => Ok(degrees.to_string()),
        _ => Err("Rotation degrees must be one of 90, 180, 270, -90, -180, or -270.".to_string()),
    }
}

fn normalize_page_ranges(input: &str) -> Result<String, String> {
    let ranges = parse_page_ranges(input)?;
    Ok(ranges
        .iter()
        .map(PageRange::to_qpdf_range)
        .collect::<Vec<_>>()
        .join(","))
}

fn parse_page_ranges(input: &str) -> Result<Vec<PageRange>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Page range is required.".to_string());
    }

    let mut ranges = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("Page range contains an empty segment.".to_string());
        }

        ranges.push(parse_page_range_segment(part)?);
    }

    Ok(ranges)
}

fn parse_page_range_segment(segment: &str) -> Result<PageRange, String> {
    if segment.contains('-') {
        let parts = segment.split('-').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(format!("Invalid page range segment: {segment}"));
        }

        let start = parse_page_number(parts[0].trim())?;
        let end = parse_page_number(parts[1].trim())?;
        if start > end {
            return Err(format!("Page range start must be before end: {segment}"));
        }

        return Ok(PageRange { start, end });
    }

    let page = parse_page_number(segment)?;
    Ok(PageRange {
        start: page,
        end: page,
    })
}

fn parse_page_number(value: &str) -> Result<u32, String> {
    let page = value
        .parse::<u32>()
        .map_err(|_| format!("Invalid page number: {value}"))?;
    if page == 0 {
        return Err("Page numbers start at 1.".to_string());
    }

    Ok(page)
}

impl PageRange {
    fn to_qpdf_range(&self) -> String {
        if self.start == self.end {
            self.start.to_string()
        } else {
            format!("{}-{}", self.start, self.end)
        }
    }
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

    #[test]
    fn parses_page_ranges() {
        assert_eq!(
            parse_page_ranges("1, 3 - 5, 8").expect("page ranges should parse"),
            vec![
                PageRange { start: 1, end: 1 },
                PageRange { start: 3, end: 5 },
                PageRange { start: 8, end: 8 },
            ]
        );
        assert_eq!(
            normalize_page_ranges("1, 3 - 5, 8").expect("page ranges should normalize"),
            "1,3-5,8"
        );
    }

    #[test]
    fn rejects_invalid_page_ranges() {
        for invalid_range in ["", "0", "3-1", "a", "1,,2", "1-2-3"] {
            assert!(
                parse_page_ranges(invalid_range).is_err(),
                "{invalid_range} should be rejected"
            );
        }
    }

    #[test]
    fn plans_merge_arguments_as_data() {
        let plan = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec![
                "/Users/mac/Desktop/input one.pdf".to_string(),
                "/Users/mac/Desktop/input two.pdf".to_string(),
            ],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        })
        .expect("merge arguments should be planned");

        assert_eq!(plan.executable, "qpdf");
        assert_eq!(
            plan.arguments,
            vec![
                "--empty",
                "--pages",
                "/Users/mac/Desktop/input one.pdf",
                "/Users/mac/Desktop/input two.pdf",
                "--",
                "/Users/mac/Desktop/converted/merged.pdf",
            ]
        );
    }

    #[test]
    fn plans_split_arguments_as_data() {
        let plan = build_qpdf_split_arguments(&QpdfSplitRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            output_directory: "/Users/mac/Desktop/converted".to_string(),
            filename_prefix: Some("page".to_string()),
        })
        .expect("split arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "--split-pages",
                "/Users/mac/Desktop/report.pdf",
                "/Users/mac/Desktop/converted/page-%d.pdf",
            ]
        );
    }

    #[test]
    fn plans_extract_arguments_with_chinese_and_spaces() {
        let plan = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/客户 文件/合同 原件.pdf".to_string(),
            pages: "2, 4-6".to_string(),
            output: "/Users/mac/Desktop/客户 文件/converted/合同 摘要.pdf".to_string(),
        })
        .expect("extract arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "/Users/mac/Desktop/客户 文件/合同 原件.pdf",
                "--pages",
                ".",
                "2,4-6",
                "--",
                "/Users/mac/Desktop/客户 文件/converted/合同 摘要.pdf",
            ]
        );
    }

    #[test]
    fn plans_rotate_arguments_with_chinese_and_spaces() {
        let plan = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: "/Users/mac/Desktop/客户 文件/扫描 件.pdf".to_string(),
            pages: "1, 3-4".to_string(),
            degrees: 90,
            output: "/Users/mac/Desktop/客户 文件/converted/扫描 件.pdf".to_string(),
        })
        .expect("rotate arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "/Users/mac/Desktop/客户 文件/扫描 件.pdf",
                "/Users/mac/Desktop/客户 文件/converted/扫描 件.pdf",
                "--rotate=+90:1,3-4",
            ]
        );
    }

    fn temp_fixture_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "localconvert-qpdf-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn resolves_platform_qpdf_sidecar_paths() {
        let base = Path::new("/app/src-tauri");

        assert_eq!(qpdf_raw_filename("windows-x86_64"), "qpdf.exe");
        assert_eq!(
            qpdf_prepared_filename("windows-x86_64"),
            "qpdf-x86_64-pc-windows-msvc.exe"
        );
        assert_eq!(
            qpdf_prepared_filename("macos-aarch64"),
            "qpdf-aarch64-apple-darwin"
        );

        let paths = qpdf_candidate_paths("macos-aarch64", base, Some(Path::new("/app/runtime")));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/app/src-tauri/binaries/macos-aarch64/qpdf"),
                PathBuf::from("/app/src-tauri/binaries/macos-aarch64/qpdf-aarch64-apple-darwin"),
                PathBuf::from("/app/runtime/qpdf-aarch64-apple-darwin"),
                PathBuf::from("/app/runtime/qpdf"),
            ]
        );
    }

    #[test]
    fn missing_qpdf_sidecar_returns_not_installed() {
        let missing = temp_fixture_path("missing");
        let detection = detect_qpdf_engine_from_candidates("macos-aarch64", &[missing], |_| {
            panic!("smoke check should not run for a missing sidecar")
        });

        assert_eq!(detection.status, "not-installed");
        assert_eq!(detection.message, QPDF_ENGINE_MISSING_MESSAGE);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_qpdf_sidecar_returns_error_on_unix() {
        let fixture = temp_fixture_path("non-executable");
        File::create(&fixture).expect("test qpdf fixture should be created");
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o644))
            .expect("test fixture permissions should be set");

        let detection =
            detect_qpdf_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                panic!("smoke check should not run for a non-executable sidecar")
            });

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("not executable"));

        let _ = fs::remove_file(fixture);
    }

    #[test]
    fn executable_qpdf_fixture_returns_available() {
        let fixture = temp_fixture_path("available");
        File::create(&fixture).expect("test qpdf fixture should be created");

        #[cfg(unix)]
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755))
            .expect("test fixture permissions should be set");

        let detection =
            detect_qpdf_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                Ok("qpdf version 12.3.2".to_string())
            });

        assert_eq!(detection.status, "available");
        assert!(detection
            .message
            .contains("qpdf sidecar smoke check passed"));

        let _ = fs::remove_file(fixture);
    }

    #[test]
    fn smoke_failure_returns_error() {
        let fixture = temp_fixture_path("smoke-failure");
        File::create(&fixture).expect("test qpdf fixture should be created");

        #[cfg(unix)]
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755))
            .expect("test fixture permissions should be set");

        let detection =
            detect_qpdf_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                Err("bad version output".to_string())
            });

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("smoke check failed"));

        let _ = fs::remove_file(fixture);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_macos_aarch64_qpdf_sidecar_passes_version_smoke_check() {
        let detection = detect_qpdf_engine("macos-aarch64");

        assert_eq!(detection.status, "available");
        assert!(detection.message.contains("qpdf version 12.3.2"));
    }

    #[test]
    fn valid_command_requests_return_execution_disabled_error() {
        let result = qpdf_extract_pages(QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: "1".to_string(),
            output: "/Users/mac/Desktop/converted/report.pdf".to_string(),
        });

        assert_eq!(result, Err(qpdf_execution_disabled_error()));
    }

    #[test]
    fn invalid_command_requests_return_validation_errors_before_engine_error() {
        let result = qpdf_merge_pdfs(QpdfMergeRequest {
            sources: vec!["/Users/mac/Desktop/one.pdf".to_string()],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        });

        assert_eq!(
            result,
            Err("Merge requires at least two source PDFs.".to_string())
        );
    }
}
