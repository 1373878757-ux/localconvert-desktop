use crate::{
    output_finalize::{FinalizedOutput, TaskOutputWorkspace},
    task_registry::{ChildProcessState, TaskCommitError, TaskControl},
    timed_process::{run_command_with_timeout, ENGINE_SELF_CHECK_TIMEOUT},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const QPDF_ENGINE_MISSING_MESSAGE: &str = "Not bundled yet.";
const QPDF_MERGE_TIMEOUT_SECONDS: u64 = 120;
const QPDF_SPLIT_TIMEOUT_SECONDS: u64 = 120;
const QPDF_EXTRACT_TIMEOUT_SECONDS: u64 = 120;
const QPDF_ROTATE_TIMEOUT_SECONDS: u64 = 120;

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

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QpdfMergeResult {
    success: bool,
    operation: &'static str,
    output_path: String,
    output_bytes: u64,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QpdfSplitResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    output_directory: String,
    output_paths: Vec<String>,
    output_bytes: u64,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QpdfExtractResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    output_path: String,
    output_bytes: u64,
    pages: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QpdfRotateResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    output_path: String,
    output_bytes: u64,
    degrees: String,
    pages: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
struct QpdfExecutionResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct PageRange {
    start: u32,
    end: u32,
}

#[cfg(test)]
pub fn qpdf_merge_pdfs(request: QpdfMergeRequest) -> QpdfMergeResult {
    qpdf_merge_task(request, TaskControl::detached("qpdf-merge"))
}

pub(crate) fn qpdf_merge_task(request: QpdfMergeRequest, control: TaskControl) -> QpdfMergeResult {
    let output_path = request.output.trim().to_string();
    let mut result =
        execute_qpdf_merge(&request, &control).unwrap_or_else(|message| QpdfMergeResult {
            success: false,
            operation: "merge",
            output_path,
            output_bytes: 0,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            message,
        });
    if control.is_cancelled() {
        result.mark_cancelled();
    }
    result
}

#[cfg(test)]
pub fn qpdf_split_pdf(request: QpdfSplitRequest) -> QpdfSplitResult {
    qpdf_split_task(request, TaskControl::detached("qpdf-split"))
}

pub(crate) fn qpdf_split_task(request: QpdfSplitRequest, control: TaskControl) -> QpdfSplitResult {
    let source_path = request.source.trim().to_string();
    let output_directory = request.output_directory.trim().to_string();
    let mut result =
        execute_qpdf_split(&request, &control).unwrap_or_else(|message| QpdfSplitResult {
            success: false,
            operation: "split",
            source_path,
            output_directory,
            output_paths: Vec::new(),
            output_bytes: 0,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            message,
        });
    if control.is_cancelled() {
        result.mark_cancelled();
    }
    result
}

#[cfg(test)]
pub fn qpdf_extract_pages(request: QpdfExtractPagesRequest) -> QpdfExtractResult {
    qpdf_extract_task(request, TaskControl::detached("qpdf-extract"))
}

pub(crate) fn qpdf_extract_task(
    request: QpdfExtractPagesRequest,
    control: TaskControl,
) -> QpdfExtractResult {
    let source_path = request.source.trim().to_string();
    let output_path = request.output.trim().to_string();
    let pages = normalize_page_ranges(&request.pages).unwrap_or_default();

    let mut result =
        execute_qpdf_extract(&request, &control).unwrap_or_else(|message| QpdfExtractResult {
            success: false,
            operation: "extract",
            source_path,
            output_path,
            output_bytes: 0,
            pages,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            message,
        });
    if control.is_cancelled() {
        result.mark_cancelled();
    }
    result
}

#[cfg(test)]
pub fn qpdf_rotate_pages(request: QpdfRotatePagesRequest) -> QpdfRotateResult {
    qpdf_rotate_task(request, TaskControl::detached("qpdf-rotate"))
}

pub(crate) fn qpdf_rotate_task(
    request: QpdfRotatePagesRequest,
    control: TaskControl,
) -> QpdfRotateResult {
    let source_path = request.source.trim().to_string();
    let output_path = request.output.trim().to_string();
    let degrees = normalize_rotation_degrees(request.degrees).unwrap_or_default();
    let pages = normalize_optional_page_ranges(&request.pages).unwrap_or_default();

    let mut result =
        execute_qpdf_rotate(&request, &control).unwrap_or_else(|message| QpdfRotateResult {
            success: false,
            operation: "rotate",
            source_path,
            output_path,
            output_bytes: 0,
            degrees,
            pages,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            message,
        });
    if control.is_cancelled() {
        result.mark_cancelled();
    }
    result
}

impl QpdfMergeResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_bytes = 0;
        self.message = "PDF merge task was cancelled locally.".to_string();
    }
}

impl QpdfSplitResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.message = "PDF split task was cancelled locally.".to_string();
    }
}

impl QpdfExtractResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_bytes = 0;
        self.message = "PDF page extraction task was cancelled locally.".to_string();
    }
}

impl QpdfRotateResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_bytes = 0;
        self.message = "PDF rotate task was cancelled locally.".to_string();
    }
}

pub fn detect_qpdf_engine(platform: &str) -> QpdfEngineDetection {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = qpdf_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    detect_qpdf_engine_from_candidates(platform, &candidates, run_qpdf_version_smoke_check)
}

fn execute_qpdf_merge(
    request: &QpdfMergeRequest,
    control: &TaskControl,
) -> Result<QpdfMergeResult, String> {
    if control.is_cancelled() {
        return Err("PDF merge task was cancelled before execution.".to_string());
    }

    build_qpdf_merge_arguments(request)?;
    let output_path = PathBuf::from(validate_pdf_path(&request.output, "Output PDF")?);
    validate_merge_source_files(&request.sources)?;
    validate_merge_output_path(&output_path)?;

    let platform = current_platform_key();
    let qpdf_path = resolve_qpdf_sidecar_path(platform)?;
    let output_parent = output_path
        .parent()
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;
    let workspace = TaskOutputWorkspace::create(output_parent, control.task_id())?;
    let temp_output = workspace.temp_file("merged.pdf")?;
    let plan = build_qpdf_merge_arguments_for_output(&request.sources, &temp_output)?;

    let execution = run_qpdf_command(
        &qpdf_path,
        &plan.arguments,
        Duration::from_secs(QPDF_MERGE_TIMEOUT_SECONDS),
        control,
    )?;

    if execution.cancelled || control.is_cancelled() {
        return Ok(QpdfMergeResult {
            success: false,
            operation: "merge",
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "PDF merge task was cancelled locally.".to_string(),
        });
    }

    if execution.timed_out {
        return Ok(QpdfMergeResult {
            success: false,
            operation: "merge",
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: true,
            message: format!("qpdf merge timed out after {QPDF_MERGE_TIMEOUT_SECONDS} seconds."),
        });
    }

    if execution.exit_code != Some(0) {
        return Ok(QpdfMergeResult {
            success: false,
            operation: "merge",
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "qpdf merge failed.".to_string(),
        });
    }

    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(QpdfMergeResult {
                    success: false,
                    operation: "merge",
                    output_path: path_to_string(&output_path),
                    output_bytes: 0,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message: "PDF merge task was cancelled before output finalization.".to_string(),
                });
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(QpdfMergeResult {
                    success: false,
                    operation: "merge",
                    output_path: path_to_string(&output_path),
                    output_bytes: 0,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message,
                });
            }
        };

    Ok(QpdfMergeResult {
        success: true,
        operation: "merge",
        output_path: path_to_string(&output_path),
        output_bytes: finalized.bytes,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "PDF merge completed locally with bundled qpdf.".to_string(),
    })
}

fn execute_qpdf_split(
    request: &QpdfSplitRequest,
    control: &TaskControl,
) -> Result<QpdfSplitResult, String> {
    if control.is_cancelled() {
        return Err("PDF split task was cancelled before execution.".to_string());
    }

    build_qpdf_split_arguments(request)?;
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let source_path = PathBuf::from(&source);
    let output_directory = PathBuf::from(validate_path_like(
        &request.output_directory,
        "Output directory",
    )?);
    let filename_prefix = normalize_split_filename_prefix(request.filename_prefix.as_deref())?;

    validate_source_pdf_file(&source_path)?;
    validate_split_output_location(&source_path, &output_directory)?;

    let split_prefix = collision_safe_split_prefix(&output_directory, &filename_prefix)?;
    let platform = current_platform_key();
    let qpdf_path = resolve_qpdf_sidecar_path(platform)?;
    let workspace = TaskOutputWorkspace::create(&output_directory, control.task_id())?;
    let temp_pattern = workspace.temp_file(&format!("{split_prefix}-%d.pdf"))?;
    let plan = build_qpdf_split_arguments_for_pattern(&source, &temp_pattern)?;
    let execution = run_qpdf_command(
        &qpdf_path,
        &plan.arguments,
        Duration::from_secs(QPDF_SPLIT_TIMEOUT_SECONDS),
        control,
    )?;

    if execution.cancelled || control.is_cancelled() {
        return Ok(QpdfSplitResult {
            success: false,
            operation: "split",
            source_path: path_to_string(&source_path),
            output_directory: path_to_string(&output_directory),
            output_paths: Vec::new(),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "PDF split task was cancelled locally.".to_string(),
        });
    }

    if execution.timed_out {
        return Ok(QpdfSplitResult {
            success: false,
            operation: "split",
            source_path: path_to_string(&source_path),
            output_directory: path_to_string(&output_directory),
            output_paths: Vec::new(),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: true,
            message: format!("qpdf split timed out after {QPDF_SPLIT_TIMEOUT_SECONDS} seconds."),
        });
    }

    if execution.exit_code != Some(0) {
        return Ok(QpdfSplitResult {
            success: false,
            operation: "split",
            source_path: path_to_string(&source_path),
            output_directory: path_to_string(&output_directory),
            output_paths: Vec::new(),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "qpdf split failed.".to_string(),
        });
    }

    let split_outputs = collect_task_split_outputs(workspace.root(), &split_prefix);

    if split_outputs.is_empty() {
        return Ok(QpdfSplitResult {
            success: false,
            operation: "split",
            source_path: path_to_string(&source_path),
            output_directory: path_to_string(&output_directory),
            output_paths: Vec::new(),
            output_bytes: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "qpdf reported success, but no split output PDFs were found.".to_string(),
        });
    }

    let mappings = split_outputs
        .iter()
        .map(|temp_output| {
            let file_name = temp_output
                .file_name()
                .ok_or_else(|| "Split output must have a filename.".to_string())?;
            Ok((temp_output.clone(), output_directory.join(file_name)))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut published_on_failure: Vec<FinalizedOutput> = Vec::new();
    let finalized = match control.commit_outputs(|| {
        workspace.finalize_files(&mappings).map_err(|error| {
            published_on_failure = error.published;
            error.message
        })
    }) {
        Ok(finalized) => finalized,
        Err(TaskCommitError::Cancelled) => {
            return Ok(QpdfSplitResult {
                success: false,
                operation: "split",
                source_path: path_to_string(&source_path),
                output_directory: path_to_string(&output_directory),
                output_paths: Vec::new(),
                output_bytes: 0,
                stdout: execution.stdout,
                stderr: execution.stderr,
                exit_code: execution.exit_code,
                timed_out: false,
                message: "PDF split task was cancelled before output finalization.".to_string(),
            });
        }
        Err(TaskCommitError::Finalize(message)) => {
            let output_bytes = published_on_failure.iter().map(|output| output.bytes).sum();
            let output_paths = published_on_failure
                .iter()
                .map(|output| path_to_string(&output.path))
                .collect::<Vec<_>>();
            let message = if output_paths.is_empty() {
                message
            } else {
                format!(
                    "{message} {} output(s) were already finalized and were not deleted.",
                    output_paths.len()
                )
            };
            return Ok(QpdfSplitResult {
                success: false,
                operation: "split",
                source_path: path_to_string(&source_path),
                output_directory: path_to_string(&output_directory),
                output_paths,
                output_bytes,
                stdout: execution.stdout,
                stderr: execution.stderr,
                exit_code: execution.exit_code,
                timed_out: false,
                message,
            });
        }
    };

    let output_bytes = finalized.iter().map(|output| output.bytes).sum();

    Ok(QpdfSplitResult {
        success: true,
        operation: "split",
        source_path: path_to_string(&source_path),
        output_directory: path_to_string(&output_directory),
        output_paths: finalized
            .iter()
            .map(|output| path_to_string(&output.path))
            .collect(),
        output_bytes,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "PDF split completed locally with bundled qpdf.".to_string(),
    })
}

fn execute_qpdf_extract(
    request: &QpdfExtractPagesRequest,
    control: &TaskControl,
) -> Result<QpdfExtractResult, String> {
    if control.is_cancelled() {
        return Err("PDF page extraction task was cancelled before execution.".to_string());
    }

    build_qpdf_extract_arguments(request)?;
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let source_path = PathBuf::from(&source);
    let requested_output = PathBuf::from(validate_pdf_path(&request.output, "Output PDF")?);
    let pages = normalize_page_ranges(&request.pages)?;

    validate_source_pdf_file(&source_path)?;
    validate_single_pdf_output_location(&source_path, &requested_output)?;

    let output_path = collision_safe_pdf_output_path(&requested_output)?;
    let platform = current_platform_key();
    let qpdf_path = resolve_qpdf_sidecar_path(platform)?;
    let output_parent = output_path
        .parent()
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;
    let workspace = TaskOutputWorkspace::create(output_parent, control.task_id())?;
    let temp_output = workspace.temp_file("extracted.pdf")?;
    let plan = build_qpdf_extract_arguments_for_output(&source, &pages, &temp_output)?;

    let execution = run_qpdf_command(
        &qpdf_path,
        &plan.arguments,
        Duration::from_secs(QPDF_EXTRACT_TIMEOUT_SECONDS),
        control,
    )?;

    if execution.cancelled || control.is_cancelled() {
        return Ok(QpdfExtractResult {
            success: false,
            operation: "extract",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "PDF page extraction task was cancelled locally.".to_string(),
        });
    }

    if execution.timed_out {
        return Ok(QpdfExtractResult {
            success: false,
            operation: "extract",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: true,
            message: format!(
                "qpdf page extraction timed out after {QPDF_EXTRACT_TIMEOUT_SECONDS} seconds."
            ),
        });
    }

    if execution.exit_code != Some(0) {
        return Ok(QpdfExtractResult {
            success: false,
            operation: "extract",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "qpdf page extraction failed.".to_string(),
        });
    }

    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(QpdfExtractResult {
                    success: false,
                    operation: "extract",
                    source_path: path_to_string(&source_path),
                    output_path: path_to_string(&output_path),
                    output_bytes: 0,
                    pages,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message: "PDF page extraction task was cancelled before output finalization."
                        .to_string(),
                });
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(QpdfExtractResult {
                    success: false,
                    operation: "extract",
                    source_path: path_to_string(&source_path),
                    output_path: path_to_string(&output_path),
                    output_bytes: 0,
                    pages,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message,
                });
            }
        };

    Ok(QpdfExtractResult {
        success: true,
        operation: "extract",
        source_path: path_to_string(&source_path),
        output_path: path_to_string(&output_path),
        output_bytes: finalized.bytes,
        pages,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "PDF page extraction completed locally with bundled qpdf.".to_string(),
    })
}

fn execute_qpdf_rotate(
    request: &QpdfRotatePagesRequest,
    control: &TaskControl,
) -> Result<QpdfRotateResult, String> {
    if control.is_cancelled() {
        return Err("PDF rotate task was cancelled before execution.".to_string());
    }

    build_qpdf_rotate_arguments(request)?;
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let source_path = PathBuf::from(&source);
    let requested_output = PathBuf::from(validate_pdf_path(&request.output, "Output PDF")?);
    let degrees = normalize_rotation_degrees(request.degrees)?;
    let pages = normalize_optional_page_ranges(&request.pages)?;

    validate_source_pdf_file(&source_path)?;
    validate_single_pdf_output_location(&source_path, &requested_output)?;

    let output_path = collision_safe_pdf_output_path(&requested_output)?;
    let platform = current_platform_key();
    let qpdf_path = resolve_qpdf_sidecar_path(platform)?;
    let output_parent = output_path
        .parent()
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;
    let workspace = TaskOutputWorkspace::create(output_parent, control.task_id())?;
    let temp_output = workspace.temp_file("rotated.pdf")?;
    let plan = build_qpdf_rotate_arguments_for_output(&source, &temp_output, &degrees, &pages)?;

    let execution = run_qpdf_command(
        &qpdf_path,
        &plan.arguments,
        Duration::from_secs(QPDF_ROTATE_TIMEOUT_SECONDS),
        control,
    )?;

    if execution.cancelled || control.is_cancelled() {
        return Ok(QpdfRotateResult {
            success: false,
            operation: "rotate",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            degrees,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "PDF rotate task was cancelled locally.".to_string(),
        });
    }

    if execution.timed_out {
        return Ok(QpdfRotateResult {
            success: false,
            operation: "rotate",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            degrees,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: true,
            message: format!("qpdf rotate timed out after {QPDF_ROTATE_TIMEOUT_SECONDS} seconds."),
        });
    }

    if execution.exit_code != Some(0) {
        return Ok(QpdfRotateResult {
            success: false,
            operation: "rotate",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            output_bytes: 0,
            degrees,
            pages,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "qpdf rotate failed.".to_string(),
        });
    }

    let finalized = match control
        .commit_outputs(|| workspace.finalize_file(&temp_output, &output_path))
    {
        Ok(finalized) => finalized,
        Err(TaskCommitError::Cancelled) => {
            return Ok(QpdfRotateResult {
                success: false,
                operation: "rotate",
                source_path: path_to_string(&source_path),
                output_path: path_to_string(&output_path),
                output_bytes: 0,
                degrees,
                pages,
                stdout: execution.stdout,
                stderr: execution.stderr,
                exit_code: execution.exit_code,
                timed_out: false,
                message: "PDF rotate task was cancelled before output finalization.".to_string(),
            });
        }
        Err(TaskCommitError::Finalize(message)) => {
            return Ok(QpdfRotateResult {
                success: false,
                operation: "rotate",
                source_path: path_to_string(&source_path),
                output_path: path_to_string(&output_path),
                output_bytes: 0,
                degrees,
                pages,
                stdout: execution.stdout,
                stderr: execution.stderr,
                exit_code: execution.exit_code,
                timed_out: false,
                message,
            });
        }
    };

    Ok(QpdfRotateResult {
        success: true,
        operation: "rotate",
        source_path: path_to_string(&source_path),
        output_path: path_to_string(&output_path),
        output_bytes: finalized.bytes,
        degrees,
        pages,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "PDF rotate completed locally with bundled qpdf.".to_string(),
    })
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
    let output = run_command_with_timeout(path, &["--version"], ENGINE_SELF_CHECK_TIMEOUT)
        .map_err(|error| {
            if error.is_timeout() {
                format!(
                    "qpdf startup smoke check timed out after {} seconds",
                    ENGINE_SELF_CHECK_TIMEOUT.as_secs()
                )
            } else {
                format!(
                    "unable to run qpdf --version for {}: {error}",
                    path_to_string(path)
                )
            }
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

fn resolve_qpdf_sidecar_path(platform: &str) -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = qpdf_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }

        let detection = detect_qpdf_engine_from_candidates(
            platform,
            std::slice::from_ref(&candidate),
            run_qpdf_version_smoke_check,
        );
        if detection.status == "available" {
            return Ok(candidate);
        }

        return Err(detection.message);
    }

    Err("qpdf sidecar is not bundled for this platform.".to_string())
}

fn current_platform_key() -> &'static str {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    match (os, arch) {
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("android", "aarch64") => "android-aarch64",
        ("ios", "aarch64") => "ios-aarch64",
        _ => "unknown-unknown",
    }
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

    let output = validate_pdf_path(&request.output, "Output PDF")?;
    build_qpdf_merge_arguments_for_output(&request.sources, Path::new(&output))
}

fn build_qpdf_merge_arguments_for_output(
    sources: &[String],
    output: &Path,
) -> Result<QpdfCommandPlan, String> {
    if sources.len() < 2 {
        return Err("Merge requires at least two source PDFs.".to_string());
    }

    let mut arguments = vec!["--empty".to_string(), "--pages".to_string()];
    for source in sources {
        arguments.push(validate_pdf_path(source, "Source PDF")?);
    }
    arguments.push("--".to_string());
    arguments.push(validate_pdf_path(&path_to_string(output), "Output PDF")?);

    Ok(qpdf_plan(arguments))
}

fn build_qpdf_split_arguments(request: &QpdfSplitRequest) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let output_directory = validate_path_like(&request.output_directory, "Output directory")?;
    let filename_prefix = normalize_split_filename_prefix(request.filename_prefix.as_deref())?;
    let output_pattern = split_output_pattern(Path::new(&output_directory), &filename_prefix);

    build_qpdf_split_arguments_for_pattern(&source, &output_pattern)
}

fn build_qpdf_split_arguments_for_pattern(
    source: &str,
    output_pattern: &Path,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(source, "Source PDF")?;
    Ok(qpdf_plan(vec![
        "--split-pages".to_string(),
        source,
        path_to_string(output_pattern),
    ]))
}

fn build_qpdf_extract_arguments(
    request: &QpdfExtractPagesRequest,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let output = validate_pdf_path(&request.output, "Output PDF")?;
    let pages = normalize_page_ranges(&request.pages)?;

    build_qpdf_extract_arguments_for_output(&source, &pages, Path::new(&output))
}

fn build_qpdf_extract_arguments_for_output(
    source: &str,
    pages: &str,
    output: &Path,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(source, "Source PDF")?;
    let output = validate_pdf_path(&path_to_string(output), "Output PDF")?;

    Ok(qpdf_plan(vec![
        source,
        "--pages".to_string(),
        ".".to_string(),
        pages.to_string(),
        "--".to_string(),
        output,
    ]))
}

fn build_qpdf_rotate_arguments(
    request: &QpdfRotatePagesRequest,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(&request.source, "Source PDF")?;
    let output = validate_pdf_path(&request.output, "Output PDF")?;
    let pages = normalize_optional_page_ranges(&request.pages)?;
    let degrees = normalize_rotation_degrees(request.degrees)?;

    build_qpdf_rotate_arguments_for_output(&source, Path::new(&output), &degrees, &pages)
}

fn build_qpdf_rotate_arguments_for_output(
    source: &str,
    output: &Path,
    degrees: &str,
    pages: &str,
) -> Result<QpdfCommandPlan, String> {
    let source = validate_pdf_path(source, "Source PDF")?;
    let output = validate_pdf_path(&path_to_string(output), "Output PDF")?;

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

fn validate_pdf_path(value: &str, label: &str) -> Result<String, String> {
    let path = validate_path_like(value, label)?;
    if !has_pdf_extension(Path::new(&path)) {
        return Err(format!("{label} must use a .pdf extension: {path}"));
    }

    Ok(path)
}

fn has_pdf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn validate_merge_source_files(sources: &[String]) -> Result<(), String> {
    for source in sources {
        let source_path = PathBuf::from(validate_pdf_path(source, "Source PDF")?);
        validate_source_pdf_file(&source_path)?;
    }

    Ok(())
}

fn validate_source_pdf_file(source_path: &Path) -> Result<(), String> {
    if !has_pdf_extension(source_path) {
        return Err(format!(
            "Source PDF must use a .pdf extension: {}",
            path_to_string(source_path)
        ));
    }

    let metadata = fs::metadata(source_path).map_err(|error| {
        format!(
            "Source PDF does not exist or cannot be inspected: {}: {error}",
            path_to_string(source_path)
        )
    })?;

    if !metadata.is_file() {
        return Err(format!(
            "Source PDF is not a file: {}",
            path_to_string(source_path)
        ));
    }

    Ok(())
}

fn validate_merge_output_path(output_path: &Path) -> Result<(), String> {
    if !has_pdf_extension(output_path) {
        return Err(format!(
            "Output PDF must use a .pdf extension: {}",
            path_to_string(output_path)
        ));
    }

    let parent = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;

    if parent.file_name().and_then(|name| name.to_str()) != Some("converted") {
        return Err(format!(
            "Output PDF must be planned inside a converted folder: {}",
            path_to_string(output_path)
        ));
    }

    if output_path.exists() {
        return Err(format!(
            "Output PDF already exists and will not be overwritten: {}",
            path_to_string(output_path)
        ));
    }

    if parent.exists() && !parent.is_dir() {
        return Err(format!(
            "Converted output path exists but is not a folder: {}",
            path_to_string(parent)
        ));
    }

    Ok(())
}

fn validate_split_output_directory(output_directory: &Path) -> Result<(), String> {
    if output_directory.file_name().and_then(|name| name.to_str()) != Some("converted") {
        return Err(format!(
            "Split output directory must be a converted folder: {}",
            path_to_string(output_directory)
        ));
    }

    if output_directory.exists() && !output_directory.is_dir() {
        return Err(format!(
            "Converted output path exists but is not a folder: {}",
            path_to_string(output_directory)
        ));
    }

    Ok(())
}

fn validate_split_output_location(
    source_path: &Path,
    output_directory: &Path,
) -> Result<(), String> {
    validate_split_output_directory(output_directory)?;

    let source_parent = source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Source PDF must have a parent folder.".to_string())?;
    let output_parent = output_directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Split output directory must have a parent folder.".to_string())?;

    if output_parent != source_parent {
        return Err(format!(
            "Split output directory must be the converted folder next to the source PDF: {}",
            path_to_string(output_directory)
        ));
    }

    Ok(())
}

fn validate_single_pdf_output_location(
    source_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    validate_merge_output_path_shape(output_path)?;

    let source_parent = source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Source PDF must have a parent folder.".to_string())?;
    let output_parent = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;
    let output_grandparent = output_parent
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output PDF converted folder must have a parent folder.".to_string())?;

    if output_grandparent != source_parent {
        return Err(format!(
            "Output PDF must be planned inside the converted folder next to the source PDF: {}",
            path_to_string(output_path)
        ));
    }

    Ok(())
}

fn validate_merge_output_path_shape(output_path: &Path) -> Result<(), String> {
    if !has_pdf_extension(output_path) {
        return Err(format!(
            "Output PDF must use a .pdf extension: {}",
            path_to_string(output_path)
        ));
    }

    let parent = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;

    if parent.file_name().and_then(|name| name.to_str()) != Some("converted") {
        return Err(format!(
            "Output PDF must be planned inside a converted folder: {}",
            path_to_string(output_path)
        ));
    }

    if parent.exists() && !parent.is_dir() {
        return Err(format!(
            "Converted output path exists but is not a folder: {}",
            path_to_string(parent)
        ));
    }

    Ok(())
}

fn collision_safe_pdf_output_path(desired_output_path: &Path) -> Result<PathBuf, String> {
    validate_merge_output_path_shape(desired_output_path)?;

    if !desired_output_path.exists() {
        return Ok(desired_output_path.to_path_buf());
    }

    let parent = desired_output_path
        .parent()
        .ok_or_else(|| "Output PDF must have a converted output folder.".to_string())?;
    let stem = desired_output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "Output PDF must have a valid filename.".to_string())?;
    let extension = desired_output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("pdf");

    let mut index = 1;
    loop {
        let candidate = parent.join(format!("{stem} ({index}).{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
        index += 1;
    }
}

fn normalize_split_filename_prefix(prefix: Option<&str>) -> Result<String, String> {
    let prefix = prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("page");

    if prefix
        .chars()
        .any(|character| matches!(character, '/' | '\\' | '\0') || character.is_control())
    {
        return Err("Split filename prefix contains an invalid path character.".to_string());
    }

    Ok(prefix.to_string())
}

fn split_output_pattern(output_directory: &Path, filename_prefix: &str) -> PathBuf {
    output_directory.join(format!("{filename_prefix}-%d.pdf"))
}

fn collision_safe_split_prefix(
    output_directory: &Path,
    desired_prefix: &str,
) -> Result<String, String> {
    if !split_prefix_has_collision(output_directory, desired_prefix)? {
        return Ok(desired_prefix.to_string());
    }

    let mut index = 1;
    loop {
        let candidate = format!("{desired_prefix} ({index})");
        if !split_prefix_has_collision(output_directory, &candidate)? {
            return Ok(candidate);
        }
        index += 1;
    }
}

fn split_prefix_has_collision(output_directory: &Path, prefix: &str) -> Result<bool, String> {
    if !output_directory.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(output_directory).map_err(|error| {
        format!(
            "Unable to inspect converted output folder {}: {error}",
            path_to_string(output_directory)
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "Unable to inspect converted output folder {}: {error}",
                path_to_string(output_directory)
            )
        })?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if is_split_output_name_for_prefix(&file_name, prefix) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn collect_task_split_outputs(workspace: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut outputs = Vec::new();
    let Ok(entries) = fs::read_dir(workspace) else {
        return outputs;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if is_split_output_name_for_prefix(&file_name, prefix)
            && path.is_file()
            && has_pdf_extension(&path)
        {
            outputs.push(path);
        }
    }

    outputs.sort();
    outputs
}

fn is_split_output_name_for_prefix(file_name: &str, prefix: &str) -> bool {
    file_name.starts_with(&format!("{prefix}-")) && file_name.to_lowercase().ends_with(".pdf")
}

fn run_qpdf_command(
    executable: &Path,
    arguments: &[String],
    timeout: Duration,
    control: &TaskControl,
) -> Result<QpdfExecutionResult, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Unable to start bundled qpdf sidecar {}: {error}",
                path_to_string(executable)
            )
        })?;

    let stdout_reader = child.stdout.take().map(read_pipe_in_thread);
    let stderr_reader = child.stderr.take().map(read_pipe_in_thread);
    if !control.attach_child(child)? {
        return Ok(QpdfExecutionResult {
            stdout: join_pipe_reader(stdout_reader),
            stderr: join_pipe_reader(stderr_reader),
            exit_code: None,
            timed_out: false,
            cancelled: true,
        });
    }
    let start = Instant::now();

    let (exit_code, timed_out, cancelled) = loop {
        let process_state = match control.poll_child() {
            Ok(state) => state,
            Err(error) => {
                let _ = control.terminate_child();
                return Err(format!("Unable to inspect qpdf process state: {error}"));
            }
        };

        match process_state {
            ChildProcessState::Running => {}
            ChildProcessState::Exited(exit_code) => break (exit_code, false, false),
            ChildProcessState::Cancelled => break (None, false, true),
        }

        if start.elapsed() >= timeout {
            let _ = control.terminate_child();
            let cancelled = control.is_cancelled();
            break (None, !cancelled, cancelled);
        }

        thread::sleep(Duration::from_millis(50));
    };

    Ok(QpdfExecutionResult {
        stdout: join_pipe_reader(stdout_reader),
        stderr: join_pipe_reader(stderr_reader),
        exit_code,
        timed_out,
        cancelled,
    })
}

fn read_pipe_in_thread<R>(mut pipe: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = pipe.read_to_end(&mut buffer);
        buffer
    })
}

fn join_pipe_reader(reader: Option<thread::JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

fn normalize_rotation_degrees(degrees: i16) -> Result<String, String> {
    match degrees {
        90 | 180 | 270 => Ok(format!("+{degrees}")),
        -90 | -180 | -270 => Ok(degrees.to_string()),
        _ => Err("Rotation degrees must be one of 90, 180, 270, -90, -180, or -270.".to_string()),
    }
}

fn normalize_optional_page_ranges(input: &str) -> Result<String, String> {
    if input.trim().is_empty() {
        return Ok("1-z".to_string());
    }

    normalize_page_ranges(input)
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
    fn plans_merge_arguments_with_chinese_paths_and_spaces() {
        let plan = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec![
                "/Users/mac/Desktop/客户 文件/合同 一.pdf".to_string(),
                "/Users/mac/Desktop/客户 文件/合同 二.pdf".to_string(),
            ],
            output: "/Users/mac/Desktop/客户 文件/converted/合同 合并.pdf".to_string(),
        })
        .expect("merge arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "--empty",
                "--pages",
                "/Users/mac/Desktop/客户 文件/合同 一.pdf",
                "/Users/mac/Desktop/客户 文件/合同 二.pdf",
                "--",
                "/Users/mac/Desktop/客户 文件/converted/合同 合并.pdf",
            ]
        );
    }

    #[test]
    fn rejects_invalid_merge_requests() {
        let too_few_sources = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec!["/Users/mac/Desktop/one.pdf".to_string()],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        });
        assert_eq!(
            too_few_sources,
            Err("Merge requires at least two source PDFs.".to_string())
        );

        let empty_path = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec!["/Users/mac/Desktop/one.pdf".to_string(), " ".to_string()],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        });
        assert_eq!(empty_path, Err("Source PDF path is required.".to_string()));
    }

    #[test]
    fn rejects_non_pdf_merge_paths() {
        let non_pdf_source = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec![
                "/Users/mac/Desktop/one.pdf".to_string(),
                "/Users/mac/Desktop/two.docx".to_string(),
            ],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        });
        assert_eq!(
            non_pdf_source,
            Err("Source PDF must use a .pdf extension: /Users/mac/Desktop/two.docx".to_string())
        );

        let non_pdf_output = build_qpdf_merge_arguments(&QpdfMergeRequest {
            sources: vec![
                "/Users/mac/Desktop/one.pdf".to_string(),
                "/Users/mac/Desktop/two.pdf".to_string(),
            ],
            output: "/Users/mac/Desktop/converted/merged.txt".to_string(),
        });
        assert_eq!(
            non_pdf_output,
            Err(
                "Output PDF must use a .pdf extension: /Users/mac/Desktop/converted/merged.txt"
                    .to_string()
            )
        );
    }

    #[test]
    fn refuses_existing_merge_output() {
        let case_dir = temp_fixture_path("overwrite");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let output = converted_dir.join("merged.pdf");
        File::create(&output).expect("existing output fixture should be created");

        let result = validate_merge_output_path(&output);

        assert_eq!(
            result,
            Err(format!(
                "Output PDF already exists and will not be overwritten: {}",
                path_to_string(&output)
            ))
        );

        let _ = fs::remove_dir_all(case_dir);
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
    fn plans_split_arguments_with_chinese_paths_and_spaces() {
        let plan = build_qpdf_split_arguments(&QpdfSplitRequest {
            source: "/Users/mac/Desktop/客户 文件/报告 原件.pdf".to_string(),
            output_directory: "/Users/mac/Desktop/客户 文件/converted".to_string(),
            filename_prefix: Some("报告 页面".to_string()),
        })
        .expect("split arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "--split-pages",
                "/Users/mac/Desktop/客户 文件/报告 原件.pdf",
                "/Users/mac/Desktop/客户 文件/converted/报告 页面-%d.pdf",
            ]
        );
    }

    #[test]
    fn rejects_invalid_split_requests() {
        let empty_source = build_qpdf_split_arguments(&QpdfSplitRequest {
            source: " ".to_string(),
            output_directory: "/Users/mac/Desktop/converted".to_string(),
            filename_prefix: Some("page".to_string()),
        });
        assert_eq!(
            empty_source,
            Err("Source PDF path is required.".to_string())
        );

        let non_pdf_source = build_qpdf_split_arguments(&QpdfSplitRequest {
            source: "/Users/mac/Desktop/report.txt".to_string(),
            output_directory: "/Users/mac/Desktop/converted".to_string(),
            filename_prefix: Some("page".to_string()),
        });
        assert_eq!(
            non_pdf_source,
            Err("Source PDF must use a .pdf extension: /Users/mac/Desktop/report.txt".to_string())
        );

        let invalid_prefix = build_qpdf_split_arguments(&QpdfSplitRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            output_directory: "/Users/mac/Desktop/converted".to_string(),
            filename_prefix: Some("bad/prefix".to_string()),
        });
        assert_eq!(
            invalid_prefix,
            Err("Split filename prefix contains an invalid path character.".to_string())
        );
    }

    #[test]
    fn rejects_missing_split_source_file() {
        let case_dir = temp_fixture_path("missing-split-source");
        let result = qpdf_split_pdf(QpdfSplitRequest {
            source: path_to_string(&case_dir.join("missing.pdf")),
            output_directory: path_to_string(&case_dir.join("converted")),
            filename_prefix: Some("missing-page".to_string()),
        });

        assert!(!result.success);
        assert!(result.message.contains("Source PDF does not exist"));
    }

    #[test]
    fn plans_collision_safe_split_prefixes_without_overwrite() {
        let case_dir = temp_fixture_path("split-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        fs::write(converted_dir.join("报告 页面-1.pdf"), b"existing")
            .expect("existing split output should be written");

        let prefix = collision_safe_split_prefix(&converted_dir, "报告 页面")
            .expect("collision-safe split prefix should be planned");
        let pattern = split_output_pattern(&converted_dir, &prefix);

        assert_eq!(prefix, "报告 页面 (1)");
        assert_eq!(pattern, converted_dir.join("报告 页面 (1)-%d.pdf"));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn collects_split_outputs_only_from_task_workspace() {
        let case_dir = temp_fixture_path("split-output-validation");
        let converted_dir = case_dir.join("converted");
        let workspace = converted_dir.join(".localconvert-task-test");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        fs::create_dir_all(&workspace).expect("task workspace should be created");
        fs::write(converted_dir.join("report-page-1.pdf"), b"existing")
            .expect("existing split output should be written");
        fs::write(workspace.join("report-page (1)-1.pdf"), b"new")
            .expect("new split output should be written");
        fs::write(workspace.join("report-page (1)-note.txt"), b"not pdf")
            .expect("non-pdf fixture should be written");

        let outputs = collect_task_split_outputs(&workspace, "report-page (1)");

        assert_eq!(outputs, vec![workspace.join("report-page (1)-1.pdf")]);

        let _ = fs::remove_dir_all(case_dir);
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
    fn rejects_invalid_extract_requests() {
        let empty_source = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: " ".to_string(),
            pages: "1".to_string(),
            output: "/Users/mac/Desktop/converted/extracted.pdf".to_string(),
        });
        assert_eq!(
            empty_source,
            Err("Source PDF path is required.".to_string())
        );

        let empty_pages = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: " ".to_string(),
            output: "/Users/mac/Desktop/converted/extracted.pdf".to_string(),
        });
        assert_eq!(empty_pages, Err("Page range is required.".to_string()));

        let invalid_page_range = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: "1,,3".to_string(),
            output: "/Users/mac/Desktop/converted/extracted.pdf".to_string(),
        });
        assert_eq!(
            invalid_page_range,
            Err("Page range contains an empty segment.".to_string())
        );

        let non_pdf_source = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/report.docx".to_string(),
            pages: "1".to_string(),
            output: "/Users/mac/Desktop/converted/extracted.pdf".to_string(),
        });
        assert_eq!(
            non_pdf_source,
            Err("Source PDF must use a .pdf extension: /Users/mac/Desktop/report.docx".to_string())
        );

        let non_pdf_output = build_qpdf_extract_arguments(&QpdfExtractPagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: "1".to_string(),
            output: "/Users/mac/Desktop/converted/extracted.txt".to_string(),
        });
        assert_eq!(
            non_pdf_output,
            Err(
                "Output PDF must use a .pdf extension: /Users/mac/Desktop/converted/extracted.txt"
                    .to_string()
            )
        );
    }

    #[test]
    fn rejects_missing_extract_source_file() {
        let case_dir = temp_fixture_path("missing-extract-source");
        let result = qpdf_extract_pages(QpdfExtractPagesRequest {
            source: path_to_string(&case_dir.join("missing.pdf")),
            pages: "1".to_string(),
            output: path_to_string(&case_dir.join("converted").join("missing extracted.pdf")),
        });

        assert!(!result.success);
        assert!(result.message.contains("Source PDF does not exist"));
    }

    #[test]
    fn plans_collision_safe_extract_output_without_overwrite() {
        let case_dir = temp_fixture_path("extract-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let output = converted_dir.join("report extracted.pdf");
        let first_collision = converted_dir.join("report extracted (1).pdf");
        fs::write(&output, b"existing").expect("existing extract output should be written");
        fs::write(&first_collision, b"existing")
            .expect("existing extract collision output should be written");

        let planned_output = collision_safe_pdf_output_path(&output)
            .expect("collision-safe extract output should be planned");

        assert_eq!(
            planned_output,
            converted_dir.join("report extracted (2).pdf")
        );
        assert_eq!(
            fs::read(&output).expect("existing output should remain readable"),
            b"existing"
        );

        let _ = fs::remove_dir_all(case_dir);
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

    #[test]
    fn plans_rotate_arguments_with_default_all_pages() {
        let plan = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: " ".to_string(),
            degrees: -90,
            output: "/Users/mac/Desktop/converted/report rotated.pdf".to_string(),
        })
        .expect("rotate arguments should be planned");

        assert_eq!(
            plan.arguments,
            vec![
                "/Users/mac/Desktop/report.pdf",
                "/Users/mac/Desktop/converted/report rotated.pdf",
                "--rotate=-90:1-z",
            ]
        );
    }

    #[test]
    fn accepts_and_rejects_rotate_angles() {
        for (degrees, expected) in [
            (90, "+90"),
            (180, "+180"),
            (270, "+270"),
            (-90, "-90"),
            (-180, "-180"),
            (-270, "-270"),
        ] {
            assert_eq!(
                normalize_rotation_degrees(degrees).expect("angle should be accepted"),
                expected
            );
        }

        for degrees in [0, 45, 360, -45] {
            assert!(
                normalize_rotation_degrees(degrees).is_err(),
                "{degrees} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_invalid_rotate_requests() {
        let empty_source = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: " ".to_string(),
            pages: "1".to_string(),
            degrees: 90,
            output: "/Users/mac/Desktop/converted/report.pdf".to_string(),
        });
        assert_eq!(
            empty_source,
            Err("Source PDF path is required.".to_string())
        );

        let non_pdf_source = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: "/Users/mac/Desktop/report.docx".to_string(),
            pages: "1".to_string(),
            degrees: 90,
            output: "/Users/mac/Desktop/converted/report.pdf".to_string(),
        });
        assert_eq!(
            non_pdf_source,
            Err("Source PDF must use a .pdf extension: /Users/mac/Desktop/report.docx".to_string())
        );

        let non_pdf_output = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: "1".to_string(),
            degrees: 90,
            output: "/Users/mac/Desktop/converted/report.txt".to_string(),
        });
        assert_eq!(
            non_pdf_output,
            Err(
                "Output PDF must use a .pdf extension: /Users/mac/Desktop/converted/report.txt"
                    .to_string()
            )
        );

        let invalid_page_range = build_qpdf_rotate_arguments(&QpdfRotatePagesRequest {
            source: "/Users/mac/Desktop/report.pdf".to_string(),
            pages: "3-1".to_string(),
            degrees: 90,
            output: "/Users/mac/Desktop/converted/report.pdf".to_string(),
        });
        assert_eq!(
            invalid_page_range,
            Err("Page range start must be before end: 3-1".to_string())
        );
    }

    #[test]
    fn rejects_missing_rotate_source_file() {
        let case_dir = temp_fixture_path("missing-rotate-source");
        let result = qpdf_rotate_pages(QpdfRotatePagesRequest {
            source: path_to_string(&case_dir.join("missing.pdf")),
            pages: "".to_string(),
            degrees: 90,
            output: path_to_string(&case_dir.join("converted").join("missing rotated.pdf")),
        });

        assert!(!result.success);
        assert!(result.message.contains("Source PDF does not exist"));
    }

    #[test]
    fn plans_collision_safe_rotate_output_without_overwrite() {
        let case_dir = temp_fixture_path("rotate-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let output = converted_dir.join("report rotated.pdf");
        let first_collision = converted_dir.join("report rotated (1).pdf");
        fs::write(&output, b"existing").expect("existing rotate output should be written");
        fs::write(&first_collision, b"existing")
            .expect("existing rotate collision output should be written");

        let planned_output = collision_safe_pdf_output_path(&output)
            .expect("collision-safe rotate output should be planned");

        assert_eq!(planned_output, converted_dir.join("report rotated (2).pdf"));
        assert_eq!(
            fs::read(&output).expect("existing output should remain readable"),
            b"existing"
        );

        let _ = fs::remove_dir_all(case_dir);
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

    fn write_tiny_pdf(path: &Path, label: &str) {
        write_tiny_pdf_pages(path, &[label]);
    }

    fn write_tiny_pdf_pages(path: &Path, labels: &[&str]) {
        let labels = if labels.is_empty() {
            vec!["Page"]
        } else {
            labels.to_vec()
        };
        let page_count = labels.len();
        let font_object_id = 3;
        let page_object_ids = (0..page_count)
            .map(|index| 4 + (index * 2))
            .collect::<Vec<_>>();
        let kids = page_object_ids
            .iter()
            .map(|object_id| format!("{object_id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut objects = vec![
            "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
            format!("2 0 obj << /Type /Pages /Kids [{kids}] /Count {page_count} >> endobj\n"),
            "3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
        ];

        for (index, label) in labels.iter().enumerate() {
            let page_object_id = 4 + (index * 2);
            let content_object_id = page_object_id + 1;
            let stream = format!("BT /F1 24 Tf 72 720 Td ({label}) Tj ET\n");
            objects.push(format!(
                "{page_object_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_object_id} 0 R >> >> /Contents {content_object_id} 0 R >> endobj\n"
            ));
            objects.push(format!(
                "{content_object_id} 0 obj << /Length {} >> stream\n{}endstream endobj\n",
                stream.len(),
                stream
            ));
        }

        let mut data = Vec::from("%PDF-1.4\n".as_bytes());
        let mut offsets = vec![0usize];
        for object in objects {
            offsets.push(data.len());
            data.extend_from_slice(object.as_bytes());
        }

        let xref_offset = data.len();
        data.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
        );
        for offset in offsets.iter().skip(1) {
            data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        data.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                offsets.len(),
                xref_offset
            )
            .as_bytes(),
        );

        fs::write(path, data).expect("tiny PDF fixture should be written");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn qpdf_page_count(path: &Path) -> usize {
        let qpdf = resolve_qpdf_sidecar_path("macos-aarch64")
            .expect("bundled qpdf sidecar should resolve");
        let output = std::process::Command::new(qpdf)
            .arg("--show-npages")
            .arg(path)
            .output()
            .expect("qpdf page count command should run");

        assert!(
            output.status.success(),
            "qpdf page count failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
            .expect("qpdf page count output should be numeric")
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

        let detection = detect_qpdf_engine_from_candidates(
            "macos-aarch64",
            std::slice::from_ref(&fixture),
            |_| panic!("smoke check should not run for a non-executable sidecar"),
        );

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

        let detection = detect_qpdf_engine_from_candidates(
            "macos-aarch64",
            std::slice::from_ref(&fixture),
            |_| Ok("qpdf version 12.3.2".to_string()),
        );

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

        let detection = detect_qpdf_engine_from_candidates(
            "macos-aarch64",
            std::slice::from_ref(&fixture),
            |_| Err("bad version output".to_string()),
        );

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("smoke check failed"));

        let _ = fs::remove_file(fixture);
    }

    #[test]
    fn smoke_timeout_returns_visible_engine_error() {
        let fixture = temp_fixture_path("smoke-timeout");
        File::create(&fixture).expect("test qpdf fixture should be created");

        #[cfg(unix)]
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755))
            .expect("test fixture permissions should be set");

        let detection = detect_qpdf_engine_from_candidates(
            "macos-aarch64",
            std::slice::from_ref(&fixture),
            |_| Err("qpdf startup smoke check timed out after 3 seconds".to_string()),
        );

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("timed out after 3 seconds"));

        let _ = fs::remove_file(fixture);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_macos_aarch64_qpdf_sidecar_passes_version_smoke_check() {
        let detection = detect_qpdf_engine("macos-aarch64");

        assert_eq!(detection.status, "available");
        assert!(detection.message.contains("qpdf version 12.3.2"));
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_merge_execution_writes_only_planned_output() {
        let case_dir = temp_fixture_path("execute-merge");
        fs::create_dir_all(&case_dir).expect("merge smoke directory should be created");
        let first = case_dir.join("one.pdf");
        let second = case_dir.join("two.pdf");
        let output = case_dir.join("converted").join("merged.pdf");
        write_tiny_pdf(&first, "One");
        write_tiny_pdf(&second, "Two");
        let first_before = fs::read(&first).expect("first source should be readable");
        let second_before = fs::read(&second).expect("second source should be readable");

        let result = qpdf_merge_pdfs(QpdfMergeRequest {
            sources: vec![path_to_string(&first), path_to_string(&second)],
            output: path_to_string(&output),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(result.output_path, path_to_string(&output));
        assert!(result.output_bytes > 0);
        assert!(output.exists());
        assert_eq!(
            fs::read(&first).expect("first source should remain readable"),
            first_before
        );
        assert_eq!(
            fs::read(&second).expect("second source should remain readable"),
            second_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_extract_execution_writes_only_planned_output() {
        let case_dir = temp_fixture_path("execute-extract");
        fs::create_dir_all(&case_dir).expect("extract smoke directory should be created");
        let source = case_dir.join("客户 文件.pdf");
        let output = case_dir.join("converted").join("客户 文件 extracted.pdf");
        write_tiny_pdf_pages(&source, &["One", "Two", "Three", "Four"]);
        let source_before = fs::read(&source).expect("source should be readable");

        let result = qpdf_extract_pages(QpdfExtractPagesRequest {
            source: path_to_string(&source),
            pages: "1,3".to_string(),
            output: path_to_string(&output),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(result.source_path, path_to_string(&source));
        assert_eq!(result.output_path, path_to_string(&output));
        assert_eq!(result.pages, "1,3");
        assert!(result.output_bytes > 0);
        assert!(output.exists());
        assert_eq!(qpdf_page_count(&output), 2);
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_extract_execution_avoids_overwriting_existing_output() {
        let case_dir = temp_fixture_path("execute-extract-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("report.pdf");
        let desired_output = converted_dir.join("report extracted.pdf");
        write_tiny_pdf_pages(&source, &["One", "Two", "Three"]);
        fs::write(&desired_output, b"keep me")
            .expect("existing extract output fixture should be written");

        let result = qpdf_extract_pages(QpdfExtractPagesRequest {
            source: path_to_string(&source),
            pages: "2-3".to_string(),
            output: path_to_string(&desired_output),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(
            fs::read(&desired_output).expect("existing output should remain readable"),
            b"keep me"
        );
        assert_eq!(
            result.output_path,
            path_to_string(&converted_dir.join("report extracted (1).pdf"))
        );
        assert_eq!(qpdf_page_count(Path::new(&result.output_path)), 2);

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_split_execution_writes_outputs_only_in_converted() {
        let case_dir = temp_fixture_path("execute-split");
        fs::create_dir_all(&case_dir).expect("split smoke directory should be created");
        let source = case_dir.join("客户 文件.pdf");
        let converted_dir = case_dir.join("converted");
        write_tiny_pdf_pages(&source, &["第一页", "Second page"]);
        let source_before = fs::read(&source).expect("source should be readable");

        let result = qpdf_split_pdf(QpdfSplitRequest {
            source: path_to_string(&source),
            output_directory: path_to_string(&converted_dir),
            filename_prefix: Some("客户 文件-page".to_string()),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(result.source_path, path_to_string(&source));
        assert_eq!(result.output_directory, path_to_string(&converted_dir));
        assert_eq!(result.output_paths.len(), 2);
        assert!(result.output_bytes > 0);
        for output_path in &result.output_paths {
            let output = PathBuf::from(output_path);
            assert!(output.starts_with(&converted_dir));
            assert!(output.exists());
            assert!(fs::metadata(&output).expect("output metadata").len() > 0);
        }
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_split_execution_avoids_overwriting_existing_outputs() {
        let case_dir = temp_fixture_path("execute-split-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("report.pdf");
        let existing_output = converted_dir.join("report-page-1.pdf");
        write_tiny_pdf_pages(&source, &["One", "Two"]);
        fs::write(&existing_output, b"keep me")
            .expect("existing split output fixture should be written");

        let result = qpdf_split_pdf(QpdfSplitRequest {
            source: path_to_string(&source),
            output_directory: path_to_string(&converted_dir),
            filename_prefix: Some("report-page".to_string()),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(
            fs::read(&existing_output).expect("existing output should remain readable"),
            b"keep me"
        );
        assert!(result
            .output_paths
            .iter()
            .all(|output| output.contains("report-page (1)-")));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_rotate_execution_writes_only_planned_output() {
        let case_dir = temp_fixture_path("execute-rotate");
        fs::create_dir_all(&case_dir).expect("rotate smoke directory should be created");
        let source = case_dir.join("扫描 件.pdf");
        let output = case_dir.join("converted").join("扫描 件 rotated.pdf");
        write_tiny_pdf(&source, "Rotate");
        let source_before = fs::read(&source).expect("source should be readable");

        let result = qpdf_rotate_pages(QpdfRotatePagesRequest {
            source: path_to_string(&source),
            pages: "".to_string(),
            degrees: 90,
            output: path_to_string(&output),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(result.source_path, path_to_string(&source));
        assert_eq!(result.output_path, path_to_string(&output));
        assert_eq!(result.degrees, "+90");
        assert_eq!(result.pages, "1-z");
        assert!(result.output_bytes > 0);
        assert!(output.exists());
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn qpdf_rotate_execution_avoids_overwriting_existing_output() {
        let case_dir = temp_fixture_path("execute-rotate-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("report.pdf");
        let desired_output = converted_dir.join("report rotated.pdf");
        write_tiny_pdf(&source, "Rotate");
        fs::write(&desired_output, b"keep me")
            .expect("existing rotate output fixture should be written");

        let result = qpdf_rotate_pages(QpdfRotatePagesRequest {
            source: path_to_string(&source),
            pages: "1".to_string(),
            degrees: 180,
            output: path_to_string(&desired_output),
        });

        assert!(result.success, "{}", result.message);
        assert_eq!(
            fs::read(&desired_output).expect("existing output should remain readable"),
            b"keep me"
        );
        assert_eq!(
            result.output_path,
            path_to_string(&converted_dir.join("report rotated (1).pdf"))
        );
        assert!(Path::new(&result.output_path).exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn invalid_command_requests_return_validation_errors_before_engine_error() {
        let result = qpdf_merge_pdfs(QpdfMergeRequest {
            sources: vec!["/Users/mac/Desktop/one.pdf".to_string()],
            output: "/Users/mac/Desktop/converted/merged.pdf".to_string(),
        });

        assert!(!result.success);
        assert_eq!(result.message, "Merge requires at least two source PDFs.");
    }
}
