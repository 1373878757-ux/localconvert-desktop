use crate::{
    image_engine, image_ops,
    output_finalize::TaskOutputWorkspace,
    task_registry::{ChildProcessState, TaskCommitError, TaskControl},
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const IMAGE_CONVERT_TIMEOUT_SECONDS: u64 = 120;
const MAX_RESIZE_DIMENSION: u32 = 16_384;
const MAX_RESIZE_PIXELS: u64 = 64_000_000;
const MIN_COMPRESSION_QUALITY: u8 = 40;
const MAX_COMPRESSION_QUALITY: u8 = 95;
const DEFAULT_JPEG_QUALITY: u8 = 82;
const DEFAULT_WEBP_QUALITY: u8 = 80;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConvertExecutionRequest {
    source: String,
    target_format: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageConvertResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    output_path: String,
    source_format: String,
    target_format: String,
    output_bytes: u64,
    width: u32,
    height: u32,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResizeExecutionRequest {
    source: String,
    mode: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageResizeResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    output_path: String,
    source_format: String,
    mode: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    resized: bool,
    output_bytes: u64,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCompressExecutionRequest {
    source: String,
    quality: Option<u8>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageCompressResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    planned_output_path: String,
    output_path: String,
    source_format: String,
    quality: Option<u8>,
    lossless: bool,
    published: bool,
    smaller: bool,
    source_bytes: u64,
    encoded_bytes: u64,
    output_bytes: u64,
    saved_bytes: u64,
    width: u32,
    height: u32,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCleanMetadataExecutionRequest {
    source: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageCleanMetadataResult {
    success: bool,
    operation: &'static str,
    source_path: String,
    planned_output_path: String,
    output_path: String,
    source_format: String,
    changed: bool,
    published: bool,
    metadata_items_removed: u32,
    removed_kinds: Vec<String>,
    pixel_data_preserved: bool,
    reencoded: bool,
    source_bytes: u64,
    output_bytes: u64,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeMode {
    Fit,
    Width,
    Height,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageResizeSidecarReport {
    operation: String,
    mode: String,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    resized: bool,
    upscaled: bool,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageCompressionSidecarReport {
    operation: String,
    format: String,
    quality: Option<u8>,
    lossless: bool,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    source_bytes: u64,
    output_bytes: u64,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ImageMetadataCleanupSidecarReport {
    operation: String,
    format: String,
    changed: bool,
    metadata_items_removed: u32,
    removed_kinds: Vec<String>,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    source_bytes: u64,
    output_bytes: u64,
    pixel_data_preserved: bool,
    reencoded: bool,
}

struct ResizeFailureContext<'a> {
    source_path: &'a Path,
    output_path: &'a Path,
    source_format: &'a str,
    mode: ResizeMode,
    request: &'a ImageResizeExecutionRequest,
}

struct CompressionFailureContext<'a> {
    source_path: &'a Path,
    planned_output_path: &'a Path,
    source_format: &'a str,
    quality: Option<u8>,
    lossless: bool,
    source_bytes: u64,
}

struct MetadataCleanupFailureContext<'a> {
    source_path: &'a Path,
    planned_output_path: &'a Path,
    source_format: &'a str,
    source_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct ImageEngineCommandPlan {
    executable: &'static str,
    arguments: Vec<OsString>,
}

#[derive(Debug, PartialEq, Eq)]
struct ImageEngineExecutionResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
}

#[cfg(test)]
pub fn image_convert_file(request: ImageConvertExecutionRequest) -> ImageConvertResult {
    image_convert_task(request, TaskControl::detached("image-convert"))
}

pub(crate) fn image_convert_task(
    request: ImageConvertExecutionRequest,
    control: TaskControl,
) -> ImageConvertResult {
    let source_path = request.source.trim().to_string();
    let source_format = source_format_from_path(Path::new(&source_path)).unwrap_or_default();
    let target_format = normalize_target_format(&request.target_format).unwrap_or_default();

    let mut result =
        execute_image_convert(&request, &control).unwrap_or_else(|message| ImageConvertResult {
            success: false,
            operation: "convert",
            source_path,
            output_path: String::new(),
            source_format,
            target_format,
            output_bytes: 0,
            width: 0,
            height: 0,
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
pub fn image_resize_file(request: ImageResizeExecutionRequest) -> ImageResizeResult {
    image_resize_task(request, TaskControl::detached("image-resize"))
}

pub(crate) fn image_resize_task(
    request: ImageResizeExecutionRequest,
    control: TaskControl,
) -> ImageResizeResult {
    let source_path = request.source.trim().to_string();
    let source_format = source_format_from_path(Path::new(&source_path)).unwrap_or_default();
    let mode = normalize_resize_mode(&request.mode)
        .map(ResizeMode::as_str)
        .unwrap_or_default()
        .to_string();

    let mut result =
        execute_image_resize(&request, &control).unwrap_or_else(|message| ImageResizeResult {
            success: false,
            operation: "resize",
            source_path,
            output_path: String::new(),
            source_format,
            mode,
            max_width: request.max_width,
            max_height: request.max_height,
            source_width: 0,
            source_height: 0,
            output_width: 0,
            output_height: 0,
            resized: false,
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
pub fn image_compress_file(request: ImageCompressExecutionRequest) -> ImageCompressResult {
    image_compress_task(request, TaskControl::detached("image-compress"))
}

pub(crate) fn image_compress_task(
    request: ImageCompressExecutionRequest,
    control: TaskControl,
) -> ImageCompressResult {
    let source_path = request.source.trim().to_string();
    let source_format = source_format_from_path(Path::new(&source_path)).unwrap_or_default();
    let quality = normalize_compression_quality(&source_format, request.quality)
        .ok()
        .flatten();

    let mut result =
        execute_image_compress(&request, &control).unwrap_or_else(|message| ImageCompressResult {
            success: false,
            operation: "compress",
            source_path,
            planned_output_path: String::new(),
            output_path: String::new(),
            source_format: source_format.clone(),
            quality,
            lossless: source_format == "png",
            published: false,
            smaller: false,
            source_bytes: 0,
            encoded_bytes: 0,
            output_bytes: 0,
            saved_bytes: 0,
            width: 0,
            height: 0,
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
pub fn image_clean_metadata_file(
    request: ImageCleanMetadataExecutionRequest,
) -> ImageCleanMetadataResult {
    image_clean_metadata_task(request, TaskControl::detached("image-clean-metadata"))
}

pub(crate) fn image_clean_metadata_task(
    request: ImageCleanMetadataExecutionRequest,
    control: TaskControl,
) -> ImageCleanMetadataResult {
    let source_path = request.source.trim().to_string();
    let source_format = source_format_from_path(Path::new(&source_path)).unwrap_or_default();

    let mut result = execute_image_clean_metadata(&request, &control).unwrap_or_else(|message| {
        ImageCleanMetadataResult {
            success: false,
            operation: "clean-metadata",
            source_path,
            planned_output_path: String::new(),
            output_path: String::new(),
            source_format,
            changed: false,
            published: false,
            metadata_items_removed: 0,
            removed_kinds: Vec::new(),
            pixel_data_preserved: false,
            reencoded: false,
            source_bytes: 0,
            output_bytes: 0,
            source_width: 0,
            source_height: 0,
            output_width: 0,
            output_height: 0,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            message,
        }
    });
    if control.is_cancelled() {
        result.mark_cancelled();
    }
    result
}

fn execute_image_convert(
    request: &ImageConvertExecutionRequest,
    control: &TaskControl,
) -> Result<ImageConvertResult, String> {
    if control.is_cancelled() {
        return Err("Image conversion task was cancelled before execution.".to_string());
    }

    let source_path = validate_source_image(&request.source)?;
    let source_format = source_format_from_path(&source_path)?;
    let target_format = normalize_target_format(&request.target_format)?;
    if source_format == target_format {
        return Err("Source and target image formats must differ.".to_string());
    }

    let planned_output =
        image_ops::plan_image_output(&path_to_string(&source_path), &target_format)?;
    let converted_folder = PathBuf::from(&planned_output.planned_converted_folder_path);
    let output_path = PathBuf::from(&planned_output.planned_output_path);
    validate_planned_output_location(&source_path, &converted_folder, &output_path)?;

    let image_engine_path =
        image_engine::resolve_image_engine_sidecar_path(image_engine::current_platform_key())?;
    let workspace = TaskOutputWorkspace::create(&converted_folder, control.task_id())?;
    let temp_output = workspace.temp_file(&format!("output.{target_format}"))?;
    let plan = build_image_convert_arguments(&source_path, &temp_output, &target_format)?;
    let execution = run_image_engine_command(
        &image_engine_path,
        &plan.arguments,
        Duration::from_secs(IMAGE_CONVERT_TIMEOUT_SECONDS),
        control,
    )?;

    if execution.cancelled || control.is_cancelled() {
        return Ok(ImageConvertResult {
            success: false,
            operation: "convert",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            source_format,
            target_format,
            output_bytes: 0,
            width: 0,
            height: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "Image conversion task was cancelled locally.".to_string(),
        });
    }

    if execution.timed_out {
        return Ok(ImageConvertResult {
            success: false,
            operation: "convert",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            source_format,
            target_format,
            output_bytes: 0,
            width: 0,
            height: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: true,
            message: format!(
                "image-engine conversion timed out after {IMAGE_CONVERT_TIMEOUT_SECONDS} seconds."
            ),
        });
    }

    if execution.exit_code != Some(0) {
        return Ok(ImageConvertResult {
            success: false,
            operation: "convert",
            source_path: path_to_string(&source_path),
            output_path: path_to_string(&output_path),
            source_format,
            target_format,
            output_bytes: 0,
            width: 0,
            height: 0,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "image-engine conversion failed.".to_string(),
        });
    }

    let (_, width, height) = match validate_output_image(&temp_output) {
        Ok(output) => output,
        Err(message) => {
            return Ok(ImageConvertResult {
                success: false,
                operation: "convert",
                source_path: path_to_string(&source_path),
                output_path: path_to_string(&output_path),
                source_format,
                target_format,
                output_bytes: 0,
                width: 0,
                height: 0,
                stdout: execution.stdout,
                stderr: execution.stderr,
                exit_code: execution.exit_code,
                timed_out: false,
                message,
            });
        }
    };
    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(ImageConvertResult {
                    success: false,
                    operation: "convert",
                    source_path: path_to_string(&source_path),
                    output_path: path_to_string(&output_path),
                    source_format,
                    target_format,
                    output_bytes: 0,
                    width: 0,
                    height: 0,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message: "Image conversion task was cancelled before output finalization."
                        .to_string(),
                });
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(ImageConvertResult {
                    success: false,
                    operation: "convert",
                    source_path: path_to_string(&source_path),
                    output_path: path_to_string(&output_path),
                    source_format,
                    target_format,
                    output_bytes: 0,
                    width: 0,
                    height: 0,
                    stdout: execution.stdout,
                    stderr: execution.stderr,
                    exit_code: execution.exit_code,
                    timed_out: false,
                    message,
                });
            }
        };
    Ok(ImageConvertResult {
        success: true,
        operation: "convert",
        source_path: path_to_string(&source_path),
        output_path: path_to_string(&output_path),
        source_format,
        target_format,
        output_bytes: finalized.bytes,
        width,
        height,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "Image conversion completed locally with bundled image-engine.".to_string(),
    })
}

fn execute_image_resize(
    request: &ImageResizeExecutionRequest,
    control: &TaskControl,
) -> Result<ImageResizeResult, String> {
    if control.is_cancelled() {
        return Err("Image resize task was cancelled before execution.".to_string());
    }

    let source_path = validate_source_image(&request.source)?;
    let source_format = source_format_from_path(&source_path)?;
    let output_extension = resize_output_extension(&source_path)?;
    let mode = normalize_resize_mode(&request.mode)?;
    validate_resize_dimensions(mode, request.max_width, request.max_height)?;

    let planned_output =
        image_ops::plan_image_output(&path_to_string(&source_path), &output_extension)?;
    let converted_folder = PathBuf::from(&planned_output.planned_converted_folder_path);
    let output_path = PathBuf::from(&planned_output.planned_output_path);
    validate_planned_output_location(&source_path, &converted_folder, &output_path)?;

    let image_engine_path =
        image_engine::resolve_image_engine_sidecar_path(image_engine::current_platform_key())?;
    let workspace = TaskOutputWorkspace::create(&converted_folder, control.task_id())?;
    let temp_output = workspace.temp_file(&format!("output.{output_extension}"))?;
    let plan = build_image_resize_arguments(
        &source_path,
        &temp_output,
        mode,
        request.max_width,
        request.max_height,
    )?;
    let execution = run_image_engine_command(
        &image_engine_path,
        &plan.arguments,
        Duration::from_secs(IMAGE_CONVERT_TIMEOUT_SECONDS),
        control,
    )?;
    let failure_context = ResizeFailureContext {
        source_path: &source_path,
        output_path: &output_path,
        source_format: &source_format,
        mode,
        request,
    };

    if execution.cancelled || control.is_cancelled() {
        return Ok(resize_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "Image resize task was cancelled locally.",
        ));
    }

    if execution.timed_out {
        return Ok(resize_failure_result(
            &failure_context,
            Some(&execution),
            true,
            &format!(
                "image-engine resize timed out after {IMAGE_CONVERT_TIMEOUT_SECONDS} seconds."
            ),
        ));
    }

    if execution.exit_code != Some(0) {
        return Ok(resize_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine resize failed.",
        ));
    }

    let report = match parse_resize_sidecar_report(&execution.stdout, mode) {
        Ok(report) => report,
        Err(message) => {
            return Ok(resize_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };
    let (_, output_width, output_height) = match validate_output_image(&temp_output) {
        Ok(output) => output,
        Err(message) => {
            return Ok(resize_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };
    if (report.output_width, report.output_height) != (output_width, output_height) {
        return Ok(resize_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine resize report does not match the validated output dimensions.",
        ));
    }

    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(resize_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    "Image resize task was cancelled before output finalization.",
                ));
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(resize_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    &message,
                ));
            }
        };

    Ok(ImageResizeResult {
        success: true,
        operation: "resize",
        source_path: path_to_string(&source_path),
        output_path: path_to_string(&output_path),
        source_format,
        mode: mode.as_str().to_string(),
        max_width: request.max_width,
        max_height: request.max_height,
        source_width: report.source_width,
        source_height: report.source_height,
        output_width: report.output_width,
        output_height: report.output_height,
        resized: report.resized,
        output_bytes: finalized.bytes,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: if report.resized {
            "Image resize completed locally with bundled image-engine.".to_string()
        } else {
            "Image resize completed locally without upscaling; the oriented source dimensions were kept."
                .to_string()
        },
    })
}

fn execute_image_compress(
    request: &ImageCompressExecutionRequest,
    control: &TaskControl,
) -> Result<ImageCompressResult, String> {
    if control.is_cancelled() {
        return Err("Image compression task was cancelled before execution.".to_string());
    }

    let source_path = validate_source_image(&request.source)?;
    let source_format = source_format_from_path(&source_path)?;
    let output_extension = resize_output_extension(&source_path)?;
    let quality = normalize_compression_quality(&source_format, request.quality)?;
    let lossless = source_format == "png";
    let source_bytes = fs::metadata(&source_path)
        .map_err(|error| format!("Unable to inspect source image size: {error}"))?
        .len();

    let planned_output =
        image_ops::plan_compressed_image_output(&path_to_string(&source_path), &output_extension)?;
    let converted_folder = PathBuf::from(&planned_output.planned_converted_folder_path);
    let output_path = PathBuf::from(&planned_output.planned_output_path);
    validate_planned_output_location(&source_path, &converted_folder, &output_path)?;

    let image_engine_path =
        image_engine::resolve_image_engine_sidecar_path(image_engine::current_platform_key())?;
    let workspace = TaskOutputWorkspace::create(&converted_folder, control.task_id())?;
    let temp_output = workspace.temp_file(&format!("compressed-output.{output_extension}"))?;
    let plan = build_image_compress_arguments(&source_path, &temp_output, quality)?;
    let execution = run_image_engine_command(
        &image_engine_path,
        &plan.arguments,
        Duration::from_secs(IMAGE_CONVERT_TIMEOUT_SECONDS),
        control,
    )?;
    let failure_context = CompressionFailureContext {
        source_path: &source_path,
        planned_output_path: &output_path,
        source_format: &source_format,
        quality,
        lossless,
        source_bytes,
    };

    if execution.cancelled || control.is_cancelled() {
        return Ok(compression_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "Image compression task was cancelled locally.",
        ));
    }

    if execution.timed_out {
        return Ok(compression_failure_result(
            &failure_context,
            Some(&execution),
            true,
            &format!(
                "image-engine compression timed out after {IMAGE_CONVERT_TIMEOUT_SECONDS} seconds."
            ),
        ));
    }

    if execution.exit_code != Some(0) {
        return Ok(compression_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine compression failed.",
        ));
    }

    let report = match parse_compression_sidecar_report(
        &execution.stdout,
        &source_format,
        quality,
        source_bytes,
    ) {
        Ok(report) => report,
        Err(message) => {
            return Ok(compression_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };
    let (encoded_bytes, width, height) = match validate_output_image(&temp_output) {
        Ok(output) => output,
        Err(message) => {
            return Ok(compression_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };
    if report.output_bytes != encoded_bytes
        || (report.output_width, report.output_height) != (width, height)
        || (report.source_width, report.source_height) != (width, height)
    {
        return Ok(compression_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine compression report does not match the validated output.",
        ));
    }

    if encoded_bytes >= source_bytes {
        return Ok(ImageCompressResult {
            success: true,
            operation: "compress",
            source_path: path_to_string(&source_path),
            planned_output_path: path_to_string(&output_path),
            output_path: String::new(),
            source_format,
            quality,
            lossless,
            published: false,
            smaller: false,
            source_bytes,
            encoded_bytes,
            output_bytes: 0,
            saved_bytes: 0,
            width,
            height,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "压缩后未变小，未生成新文件".to_string(),
        });
    }

    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(compression_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    "Image compression task was cancelled before output finalization.",
                ));
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(compression_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    &message,
                ));
            }
        };

    Ok(ImageCompressResult {
        success: true,
        operation: "compress",
        source_path: path_to_string(&source_path),
        planned_output_path: path_to_string(&output_path),
        output_path: path_to_string(&finalized.path),
        source_format,
        quality,
        lossless,
        published: true,
        smaller: true,
        source_bytes,
        encoded_bytes,
        output_bytes: finalized.bytes,
        saved_bytes: source_bytes - finalized.bytes,
        width,
        height,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "Image compression completed locally with bundled image-engine.".to_string(),
    })
}

fn execute_image_clean_metadata(
    request: &ImageCleanMetadataExecutionRequest,
    control: &TaskControl,
) -> Result<ImageCleanMetadataResult, String> {
    if control.is_cancelled() {
        return Err("Image metadata cleanup task was cancelled before execution.".to_string());
    }

    let source_path = validate_source_image(&request.source)?;
    let source_format = source_format_from_path(&source_path)?;
    let output_extension = resize_output_extension(&source_path)?;
    let source_bytes = fs::metadata(&source_path)
        .map_err(|error| format!("Unable to inspect source image size: {error}"))?
        .len();
    let planned_output =
        image_ops::plan_cleaned_image_output(&path_to_string(&source_path), &output_extension)?;
    let converted_folder = PathBuf::from(&planned_output.planned_converted_folder_path);
    let output_path = PathBuf::from(&planned_output.planned_output_path);
    validate_planned_output_location(&source_path, &converted_folder, &output_path)?;

    let image_engine_path =
        image_engine::resolve_image_engine_sidecar_path(image_engine::current_platform_key())?;
    let workspace = TaskOutputWorkspace::create(&converted_folder, control.task_id())?;
    let temp_output = workspace.temp_file(&format!("cleaned-output.{output_extension}"))?;
    let plan = build_image_clean_metadata_arguments(&source_path, &temp_output)?;
    let execution = run_image_engine_command(
        &image_engine_path,
        &plan.arguments,
        Duration::from_secs(IMAGE_CONVERT_TIMEOUT_SECONDS),
        control,
    )?;
    let failure_context = MetadataCleanupFailureContext {
        source_path: &source_path,
        planned_output_path: &output_path,
        source_format: &source_format,
        source_bytes,
    };

    if execution.cancelled || control.is_cancelled() {
        return Ok(metadata_cleanup_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "Image metadata cleanup task was cancelled locally.",
        ));
    }
    if execution.timed_out {
        return Ok(metadata_cleanup_failure_result(
            &failure_context,
            Some(&execution),
            true,
            &format!(
                "image-engine metadata cleanup timed out after {IMAGE_CONVERT_TIMEOUT_SECONDS} seconds."
            ),
        ));
    }
    if execution.exit_code != Some(0) {
        return Ok(metadata_cleanup_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine metadata cleanup failed.",
        ));
    }

    let report = match parse_metadata_cleanup_sidecar_report(
        &execution.stdout,
        &source_format,
        source_bytes,
    ) {
        Ok(report) => report,
        Err(message) => {
            return Ok(metadata_cleanup_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };

    if !report.changed {
        if temp_output.exists() {
            return Ok(metadata_cleanup_failure_result(
                &failure_context,
                Some(&execution),
                false,
                "image-engine reported no metadata change but created an output file.",
            ));
        }
        return Ok(ImageCleanMetadataResult {
            success: true,
            operation: "clean-metadata",
            source_path: path_to_string(&source_path),
            planned_output_path: path_to_string(&output_path),
            output_path: String::new(),
            source_format,
            changed: false,
            published: false,
            metadata_items_removed: 0,
            removed_kinds: Vec::new(),
            pixel_data_preserved: true,
            reencoded: false,
            source_bytes,
            output_bytes: 0,
            source_width: report.source_width,
            source_height: report.source_height,
            output_width: report.output_width,
            output_height: report.output_height,
            stdout: execution.stdout,
            stderr: execution.stderr,
            exit_code: execution.exit_code,
            timed_out: false,
            message: "未发现可清理的元数据，未生成新文件".to_string(),
        });
    }

    let (output_bytes, output_width, output_height) = match validate_output_image(&temp_output) {
        Ok(output) => output,
        Err(message) => {
            return Ok(metadata_cleanup_failure_result(
                &failure_context,
                Some(&execution),
                false,
                &message,
            ));
        }
    };
    if report.output_bytes != output_bytes
        || (report.output_width, report.output_height) != (output_width, output_height)
    {
        return Ok(metadata_cleanup_failure_result(
            &failure_context,
            Some(&execution),
            false,
            "image-engine metadata cleanup report does not match the validated output.",
        ));
    }

    let finalized =
        match control.commit_outputs(|| workspace.finalize_file(&temp_output, &output_path)) {
            Ok(finalized) => finalized,
            Err(TaskCommitError::Cancelled) => {
                return Ok(metadata_cleanup_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    "Image metadata cleanup task was cancelled before output finalization.",
                ));
            }
            Err(TaskCommitError::Finalize(message)) => {
                return Ok(metadata_cleanup_failure_result(
                    &failure_context,
                    Some(&execution),
                    false,
                    &message,
                ));
            }
        };

    Ok(ImageCleanMetadataResult {
        success: true,
        operation: "clean-metadata",
        source_path: path_to_string(&source_path),
        planned_output_path: path_to_string(&output_path),
        output_path: path_to_string(&finalized.path),
        source_format,
        changed: true,
        published: true,
        metadata_items_removed: report.metadata_items_removed,
        removed_kinds: report.removed_kinds,
        pixel_data_preserved: report.pixel_data_preserved,
        reencoded: report.reencoded,
        source_bytes,
        output_bytes: finalized.bytes,
        source_width: report.source_width,
        source_height: report.source_height,
        output_width,
        output_height,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "图片元数据已在本机清理完成。".to_string(),
    })
}

fn metadata_cleanup_failure_result(
    context: &MetadataCleanupFailureContext<'_>,
    execution: Option<&ImageEngineExecutionResult>,
    timed_out: bool,
    message: &str,
) -> ImageCleanMetadataResult {
    ImageCleanMetadataResult {
        success: false,
        operation: "clean-metadata",
        source_path: path_to_string(context.source_path),
        planned_output_path: path_to_string(context.planned_output_path),
        output_path: String::new(),
        source_format: context.source_format.to_string(),
        changed: false,
        published: false,
        metadata_items_removed: 0,
        removed_kinds: Vec::new(),
        pixel_data_preserved: false,
        reencoded: false,
        source_bytes: context.source_bytes,
        output_bytes: 0,
        source_width: 0,
        source_height: 0,
        output_width: 0,
        output_height: 0,
        stdout: execution
            .map(|execution| execution.stdout.clone())
            .unwrap_or_default(),
        stderr: execution
            .map(|execution| execution.stderr.clone())
            .unwrap_or_default(),
        exit_code: execution.and_then(|execution| execution.exit_code),
        timed_out,
        message: message.to_string(),
    }
}

fn compression_failure_result(
    context: &CompressionFailureContext<'_>,
    execution: Option<&ImageEngineExecutionResult>,
    timed_out: bool,
    message: &str,
) -> ImageCompressResult {
    ImageCompressResult {
        success: false,
        operation: "compress",
        source_path: path_to_string(context.source_path),
        planned_output_path: path_to_string(context.planned_output_path),
        output_path: String::new(),
        source_format: context.source_format.to_string(),
        quality: context.quality,
        lossless: context.lossless,
        published: false,
        smaller: false,
        source_bytes: context.source_bytes,
        encoded_bytes: 0,
        output_bytes: 0,
        saved_bytes: 0,
        width: 0,
        height: 0,
        stdout: execution
            .map(|execution| execution.stdout.clone())
            .unwrap_or_default(),
        stderr: execution
            .map(|execution| execution.stderr.clone())
            .unwrap_or_default(),
        exit_code: execution.and_then(|execution| execution.exit_code),
        timed_out,
        message: message.to_string(),
    }
}

fn resize_failure_result(
    context: &ResizeFailureContext<'_>,
    execution: Option<&ImageEngineExecutionResult>,
    timed_out: bool,
    message: &str,
) -> ImageResizeResult {
    ImageResizeResult {
        success: false,
        operation: "resize",
        source_path: path_to_string(context.source_path),
        output_path: path_to_string(context.output_path),
        source_format: context.source_format.to_string(),
        mode: context.mode.as_str().to_string(),
        max_width: context.request.max_width,
        max_height: context.request.max_height,
        source_width: 0,
        source_height: 0,
        output_width: 0,
        output_height: 0,
        resized: false,
        output_bytes: 0,
        stdout: execution
            .map(|execution| execution.stdout.clone())
            .unwrap_or_default(),
        stderr: execution
            .map(|execution| execution.stderr.clone())
            .unwrap_or_default(),
        exit_code: execution.and_then(|execution| execution.exit_code),
        timed_out,
        message: message.to_string(),
    }
}

impl ImageConvertResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_bytes = 0;
        self.width = 0;
        self.height = 0;
        self.message = "Image conversion task was cancelled locally.".to_string();
    }
}

impl ImageResizeResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.source_width = 0;
        self.source_height = 0;
        self.output_width = 0;
        self.output_height = 0;
        self.resized = false;
        self.output_bytes = 0;
        self.message = "Image resize task was cancelled locally.".to_string();
    }
}

impl ImageCompressResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_path.clear();
        self.published = false;
        self.smaller = false;
        self.encoded_bytes = 0;
        self.output_bytes = 0;
        self.saved_bytes = 0;
        self.width = 0;
        self.height = 0;
        self.message = "Image compression task was cancelled locally.".to_string();
    }
}

impl ImageCleanMetadataResult {
    pub(crate) fn succeeded(&self) -> bool {
        self.success
    }

    pub(crate) fn mark_cancelled(&mut self) {
        self.success = false;
        self.output_path.clear();
        self.changed = false;
        self.published = false;
        self.metadata_items_removed = 0;
        self.removed_kinds.clear();
        self.pixel_data_preserved = false;
        self.reencoded = false;
        self.output_bytes = 0;
        self.source_width = 0;
        self.source_height = 0;
        self.output_width = 0;
        self.output_height = 0;
        self.message = "Image metadata cleanup task was cancelled locally.".to_string();
    }
}

fn validate_source_image(source: &str) -> Result<PathBuf, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("Source image path is required.".to_string());
    }
    if source.chars().any(|character| character == '\0') {
        return Err("Source image path contains an invalid null character.".to_string());
    }

    let source_path = PathBuf::from(source);
    source_format_from_path(&source_path)?;
    let metadata = fs::metadata(&source_path).map_err(|error| {
        format!(
            "Source image does not exist or cannot be inspected: {}: {error}",
            path_to_string(&source_path)
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Source image path is not a file: {}",
            path_to_string(&source_path)
        ));
    }

    Ok(source_path)
}

fn source_format_from_path(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Source image must include a supported file extension.".to_string())?;

    match extension.as_str() {
        "jpg" | "jpeg" => Ok("jpg".to_string()),
        "png" => Ok("png".to_string()),
        "webp" => Ok("webp".to_string()),
        "avif" | "tif" | "tiff" | "heic" => Err(format!(
            "Image format .{extension} is planned but not enabled for conversion yet."
        )),
        _ => Err(format!(
            "Source image must use a .jpg, .jpeg, .png, or .webp extension: {}",
            path_to_string(path)
        )),
    }
}

fn normalize_target_format(target_format: &str) -> Result<String, String> {
    match target_format
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Ok("jpg".to_string()),
        "png" => Ok("png".to_string()),
        "webp" => Ok("webp".to_string()),
        "" => Err("Target image format is required.".to_string()),
        value => Err(format!(
            "Target image format .{value} is not enabled. Use jpg, png, or webp."
        )),
    }
}

fn normalize_compression_quality(
    source_format: &str,
    requested_quality: Option<u8>,
) -> Result<Option<u8>, String> {
    match source_format {
        "jpg" => {
            let quality = requested_quality.unwrap_or(DEFAULT_JPEG_QUALITY);
            validate_compression_quality(quality)?;
            Ok(Some(quality))
        }
        "webp" => {
            let quality = requested_quality.unwrap_or(DEFAULT_WEBP_QUALITY);
            validate_compression_quality(quality)?;
            Ok(Some(quality))
        }
        "png" if requested_quality.is_some() => {
            Err("PNG optimization is lossless and does not accept a quality value.".to_string())
        }
        "png" => Ok(None),
        _ => Err("Image compression supports only JPG/JPEG, PNG, and WebP.".to_string()),
    }
}

fn validate_compression_quality(quality: u8) -> Result<(), String> {
    if !(MIN_COMPRESSION_QUALITY..=MAX_COMPRESSION_QUALITY).contains(&quality) {
        return Err(format!(
            "Image compression quality must be between {MIN_COMPRESSION_QUALITY} and {MAX_COMPRESSION_QUALITY}."
        ));
    }
    Ok(())
}

impl ResizeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::Width => "width",
            Self::Height => "height",
        }
    }
}

fn normalize_resize_mode(mode: &str) -> Result<ResizeMode, String> {
    match mode.trim() {
        "fit" => Ok(ResizeMode::Fit),
        "width" => Ok(ResizeMode::Width),
        "height" => Ok(ResizeMode::Height),
        _ => Err("Image resize mode must be fit, width, or height.".to_string()),
    }
}

fn validate_resize_dimensions(
    mode: ResizeMode,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<(), String> {
    if matches!(max_width, Some(0)) || matches!(max_height, Some(0)) {
        return Err("Image resize dimensions must be greater than zero.".to_string());
    }
    if max_width.is_some_and(|width| width > MAX_RESIZE_DIMENSION)
        || max_height.is_some_and(|height| height > MAX_RESIZE_DIMENSION)
    {
        return Err(format!(
            "Image resize dimensions must not exceed {MAX_RESIZE_DIMENSION} pixels per edge."
        ));
    }

    match (mode, max_width, max_height) {
        (ResizeMode::Fit, Some(width), Some(height)) => {
            if u64::from(width) * u64::from(height) > MAX_RESIZE_PIXELS {
                return Err(format!(
                    "Image resize bounds exceed the {MAX_RESIZE_PIXELS}-pixel safety limit."
                ));
            }
            Ok(())
        }
        (ResizeMode::Width, Some(_), None) | (ResizeMode::Height, None, Some(_)) => Ok(()),
        (ResizeMode::Fit, _, _) => {
            Err("Fit resize mode requires both maxWidth and maxHeight.".to_string())
        }
        (ResizeMode::Width, _, _) => {
            Err("Width-only resize mode requires only maxWidth.".to_string())
        }
        (ResizeMode::Height, _, _) => {
            Err("Height-only resize mode requires only maxHeight.".to_string())
        }
    }
}

fn resize_output_extension(source_path: &Path) -> Result<String, String> {
    source_format_from_path(source_path)?;
    source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Source image must include a supported file extension.".to_string())
}

fn validate_planned_output_location(
    source_path: &Path,
    converted_folder: &Path,
    output_path: &Path,
) -> Result<(), String> {
    let expected_folder = source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("converted"))
        .ok_or_else(|| "Source image must have a parent folder.".to_string())?;
    if converted_folder != expected_folder || output_path.parent() != Some(converted_folder) {
        return Err(
            "Image output must be inside the source-adjacent converted folder.".to_string(),
        );
    }
    if output_path == source_path {
        return Err("Image output must not replace the source image.".to_string());
    }

    Ok(())
}

fn build_image_convert_arguments(
    source_path: &Path,
    output_path: &Path,
    target_format: &str,
) -> Result<ImageEngineCommandPlan, String> {
    source_format_from_path(source_path)?;
    let target_format = normalize_target_format(target_format)?;
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(&target_format))
    {
        return Err("Output image extension must match the target format.".to_string());
    }

    Ok(ImageEngineCommandPlan {
        executable: "image-engine",
        arguments: vec![
            OsString::from("convert"),
            OsString::from("--input"),
            source_path.as_os_str().to_os_string(),
            OsString::from("--output"),
            output_path.as_os_str().to_os_string(),
            OsString::from("--format"),
            OsString::from(target_format),
        ],
    })
}

fn build_image_resize_arguments(
    source_path: &Path,
    output_path: &Path,
    mode: ResizeMode,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<ImageEngineCommandPlan, String> {
    let source_extension = resize_output_extension(source_path)?;
    validate_resize_dimensions(mode, max_width, max_height)?;
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(&source_extension))
    {
        return Err("Resize output extension must match the source image extension.".to_string());
    }

    let mut arguments = vec![
        OsString::from("resize"),
        OsString::from("--input"),
        source_path.as_os_str().to_os_string(),
        OsString::from("--output"),
        output_path.as_os_str().to_os_string(),
        OsString::from("--mode"),
        OsString::from(mode.as_str()),
    ];
    if let Some(width) = max_width {
        arguments.push(OsString::from("--max-width"));
        arguments.push(OsString::from(width.to_string()));
    }
    if let Some(height) = max_height {
        arguments.push(OsString::from("--max-height"));
        arguments.push(OsString::from(height.to_string()));
    }

    Ok(ImageEngineCommandPlan {
        executable: "image-engine",
        arguments,
    })
}

fn build_image_compress_arguments(
    source_path: &Path,
    output_path: &Path,
    quality: Option<u8>,
) -> Result<ImageEngineCommandPlan, String> {
    let source_format = source_format_from_path(source_path)?;
    let source_extension = resize_output_extension(source_path)?;
    let quality = normalize_compression_quality(&source_format, quality)?;
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(&source_extension))
    {
        return Err(
            "Compressed output extension must match the source image extension.".to_string(),
        );
    }

    let mut arguments = vec![
        OsString::from("compress"),
        OsString::from("--input"),
        source_path.as_os_str().to_os_string(),
        OsString::from("--output"),
        output_path.as_os_str().to_os_string(),
    ];
    if let Some(quality) = quality {
        arguments.push(OsString::from("--quality"));
        arguments.push(OsString::from(quality.to_string()));
    }

    Ok(ImageEngineCommandPlan {
        executable: "image-engine",
        arguments,
    })
}

fn build_image_clean_metadata_arguments(
    source_path: &Path,
    output_path: &Path,
) -> Result<ImageEngineCommandPlan, String> {
    let source_extension = resize_output_extension(source_path)?;
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(&source_extension))
    {
        return Err(
            "Metadata cleanup output extension must match the source image extension.".to_string(),
        );
    }

    Ok(ImageEngineCommandPlan {
        executable: "image-engine",
        arguments: vec![
            OsString::from("clean-metadata"),
            OsString::from("--input"),
            source_path.as_os_str().to_os_string(),
            OsString::from("--output"),
            output_path.as_os_str().to_os_string(),
        ],
    })
}

fn parse_resize_sidecar_report(
    stdout: &str,
    expected_mode: ResizeMode,
) -> Result<ImageResizeSidecarReport, String> {
    let report: ImageResizeSidecarReport = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("image-engine resize returned an invalid report: {error}"))?;
    if report.operation != "resize" || report.mode != expected_mode.as_str() {
        return Err(
            "image-engine resize report does not match the requested operation.".to_string(),
        );
    }
    if report.source_width == 0
        || report.source_height == 0
        || report.output_width == 0
        || report.output_height == 0
    {
        return Err("image-engine resize report contains invalid dimensions.".to_string());
    }
    if report.upscaled {
        return Err("image-engine resize reported an unsafe upscale operation.".to_string());
    }
    if report.output_width > report.source_width || report.output_height > report.source_height {
        return Err(
            "image-engine resize output exceeds the oriented source dimensions.".to_string(),
        );
    }
    if report.output_width > MAX_RESIZE_DIMENSION
        || report.output_height > MAX_RESIZE_DIMENSION
        || u64::from(report.output_width) * u64::from(report.output_height) > MAX_RESIZE_PIXELS
    {
        return Err("image-engine resize output exceeds the configured safety limits.".to_string());
    }
    if report.resized
        != ((report.source_width, report.source_height)
            != (report.output_width, report.output_height))
    {
        return Err("image-engine resize report has an inconsistent resized flag.".to_string());
    }

    Ok(report)
}

fn parse_compression_sidecar_report(
    stdout: &str,
    expected_format: &str,
    expected_quality: Option<u8>,
    expected_source_bytes: u64,
) -> Result<ImageCompressionSidecarReport, String> {
    let report: ImageCompressionSidecarReport = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("image-engine compression returned an invalid report: {error}"))?;
    if report.operation != "compress" || report.format != expected_format {
        return Err(
            "image-engine compression report does not match the requested operation.".to_string(),
        );
    }
    if report.quality != expected_quality || report.lossless != (expected_format == "png") {
        return Err(
            "image-engine compression report has inconsistent quality settings.".to_string(),
        );
    }
    if report.source_bytes != expected_source_bytes || report.output_bytes == 0 {
        return Err("image-engine compression report has invalid byte counts.".to_string());
    }
    if report.source_width == 0
        || report.source_height == 0
        || report.output_width == 0
        || report.output_height == 0
        || (report.source_width, report.source_height)
            != (report.output_width, report.output_height)
    {
        return Err("image-engine compression report has invalid dimensions.".to_string());
    }

    Ok(report)
}

fn parse_metadata_cleanup_sidecar_report(
    stdout: &str,
    expected_format: &str,
    expected_source_bytes: u64,
) -> Result<ImageMetadataCleanupSidecarReport, String> {
    let report: ImageMetadataCleanupSidecarReport =
        serde_json::from_str(stdout.trim()).map_err(|error| {
            format!("image-engine metadata cleanup returned an invalid report: {error}")
        })?;
    if report.operation != "clean-metadata" || report.format != expected_format {
        return Err(
            "image-engine metadata cleanup report does not match the requested operation."
                .to_string(),
        );
    }
    if report.source_bytes != expected_source_bytes
        || report.source_width == 0
        || report.source_height == 0
        || report.output_width == 0
        || report.output_height == 0
    {
        return Err("image-engine metadata cleanup report has invalid source details.".to_string());
    }
    if report.changed {
        if report.metadata_items_removed == 0
            || report.removed_kinds.is_empty()
            || report.output_bytes == 0
        {
            return Err(
                "image-engine metadata cleanup report has inconsistent removal details."
                    .to_string(),
            );
        }
        if report.reencoded == report.pixel_data_preserved {
            return Err(
                "image-engine metadata cleanup report has inconsistent pixel preservation details."
                    .to_string(),
            );
        }
    } else if report.metadata_items_removed != 0
        || !report.removed_kinds.is_empty()
        || report.output_bytes != 0
        || !report.pixel_data_preserved
        || report.reencoded
        || (report.source_width, report.source_height)
            != (report.output_width, report.output_height)
    {
        return Err(
            "image-engine metadata cleanup report has inconsistent no-change details.".to_string(),
        );
    }

    Ok(report)
}

fn run_image_engine_command(
    executable: &Path,
    arguments: &[OsString],
    timeout: Duration,
    control: &TaskControl,
) -> Result<ImageEngineExecutionResult, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Unable to start bundled image-engine sidecar {}: {error}",
                path_to_string(executable)
            )
        })?;

    let stdout_reader = child.stdout.take().map(read_pipe_in_thread);
    let stderr_reader = child.stderr.take().map(read_pipe_in_thread);
    if !control.attach_child(child)? {
        return Ok(ImageEngineExecutionResult {
            stdout: join_pipe_reader(stdout_reader),
            stderr: join_pipe_reader(stderr_reader),
            exit_code: None,
            timed_out: false,
            cancelled: true,
        });
    }
    let started_at = Instant::now();

    let (exit_code, timed_out, cancelled) = loop {
        let process_state = match control.poll_child() {
            Ok(state) => state,
            Err(error) => {
                let _ = control.terminate_child();
                return Err(format!(
                    "Unable to inspect image-engine process state: {error}"
                ));
            }
        };

        match process_state {
            ChildProcessState::Running => {}
            ChildProcessState::Exited(exit_code) => break (exit_code, false, false),
            ChildProcessState::Cancelled => break (None, false, true),
        }

        if started_at.elapsed() >= timeout {
            let _ = control.terminate_child();
            let cancelled = control.is_cancelled();
            break (None, !cancelled, cancelled);
        }

        thread::sleep(Duration::from_millis(50));
    };

    Ok(ImageEngineExecutionResult {
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

fn validate_output_image(output_path: &Path) -> Result<(u64, u32, u32), String> {
    let metadata = fs::metadata(output_path).map_err(|error| {
        format!(
            "image-engine reported success, but output image is missing: {}: {error}",
            path_to_string(output_path)
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("image-engine reported success, but output image is empty.".to_string());
    }

    let (width, height) = image::image_dimensions(output_path)
        .map_err(|error| format!("image-engine output image could not be validated: {error}"))?;
    if width == 0 || height == 0 {
        return Err("image-engine output image has invalid dimensions.".to_string());
    }

    Ok((metadata.len(), width, height))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_registry::{BackendTaskRegistry, BackendTaskStatus};
    use image::{
        codecs::{
            jpeg::JpegEncoder,
            png::{CompressionType as PngCompressionType, FilterType as PngFilterType, PngEncoder},
        },
        DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("localconvert-image-convert-{name}"))
            .join(unique.to_string())
    }

    fn create_test_png(path: &Path) -> Vec<u8> {
        let image =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 3, Rgba([40, 100, 180, 220])));
        image
            .save_with_format(path, ImageFormat::Png)
            .expect("test PNG should be written");
        fs::read(path).expect("test PNG bytes should be readable")
    }

    fn create_test_rgb_image(path: &Path, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(5, 4, Rgb([40, 100, 180])));
        image
            .save_with_format(path, format)
            .expect("test image should be written");
        fs::read(path).expect("test image bytes should be readable")
    }

    fn patterned_image(width: u32, height: u32) -> DynamicImage {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([
                ((x * 13 + y * 7) % 256) as u8,
                ((x * 3 + y * 17) % 256) as u8,
                ((x * 19 + y * 5) % 256) as u8,
                255,
            ]);
        }
        DynamicImage::ImageRgba8(image)
    }

    fn noisy_image(width: u32, height: u32) -> DynamicImage {
        let mut image = RgbaImage::new(width, height);
        let mut state = 0x4c4f_4341_u32;
        for pixel in image.pixels_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let red = state as u8;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let green = (state >> 8) as u8;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let blue = (state >> 16) as u8;
            *pixel = Rgba([red, green, blue, 255]);
        }
        DynamicImage::ImageRgba8(image)
    }

    fn create_high_quality_jpeg(path: &Path) -> Vec<u8> {
        let image = noisy_image(256, 192);
        let mut encoded = Vec::new();
        image
            .write_with_encoder(JpegEncoder::new_with_quality(&mut encoded, 100))
            .expect("high-quality test JPEG should encode");
        fs::write(path, &encoded).expect("test JPEG should be written");
        encoded
    }

    fn create_jpeg_with_comment_metadata(path: &Path) -> Vec<u8> {
        let base_path = path.with_extension("base.jpg");
        let base = create_high_quality_jpeg(&base_path);
        let comment = b"private camera owner and location";
        let length = u16::try_from(comment.len() + 2).expect("test comment should fit");
        let mut encoded = Vec::with_capacity(base.len() + comment.len() + 4);
        encoded.extend_from_slice(&base[..2]);
        encoded.extend_from_slice(&[0xff, 0xfe]);
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(comment);
        encoded.extend_from_slice(&base[2..]);
        fs::write(path, &encoded).expect("metadata JPEG should be written");
        let _ = fs::remove_file(base_path);
        encoded
    }

    fn create_lossless_webp(path: &Path) -> Vec<u8> {
        let image = noisy_image(256, 192);
        image
            .save_with_format(path, ImageFormat::WebP)
            .expect("lossless test WebP should be written");
        fs::read(path).expect("test WebP bytes should be readable")
    }

    fn create_png_with_compression(path: &Path, compression: PngCompressionType) -> Vec<u8> {
        let image = patterned_image(128, 96);
        let mut encoded = Vec::new();
        image
            .write_with_encoder(PngEncoder::new_with_quality(
                &mut encoded,
                compression,
                PngFilterType::Adaptive,
            ))
            .expect("test PNG should encode");
        fs::write(path, &encoded).expect("test PNG should be written");
        encoded
    }

    fn assert_image_format(path: &Path, expected: ImageFormat) {
        let reader = image::ImageReader::open(path)
            .expect("output image should open")
            .with_guessed_format()
            .expect("output image format should be detected");
        assert_eq!(reader.format(), Some(expected));
    }

    #[test]
    fn validates_enabled_source_and_target_formats() {
        assert_eq!(
            source_format_from_path(Path::new("sample.JPEG")),
            Ok("jpg".to_string())
        );
        assert_eq!(normalize_target_format(" .WEBP "), Ok("webp".to_string()));
        assert!(source_format_from_path(Path::new("sample.avif")).is_err());
        assert!(normalize_target_format("tiff").is_err());
        assert!(source_format_from_path(Path::new("sample.gif")).is_err());
    }

    #[test]
    fn plans_argument_array_for_chinese_paths_and_spaces() {
        let source = Path::new("/Users/mac/客户 图片/input one.png");
        let output = Path::new("/Users/mac/客户 图片/converted/input one.webp");
        let plan = build_image_convert_arguments(source, output, "webp")
            .expect("argument plan should be valid");

        assert_eq!(plan.executable, "image-engine");
        assert_eq!(
            plan.arguments,
            vec![
                OsString::from("convert"),
                OsString::from("--input"),
                source.as_os_str().to_os_string(),
                OsString::from("--output"),
                output.as_os_str().to_os_string(),
                OsString::from("--format"),
                OsString::from("webp"),
            ]
        );
    }

    #[test]
    fn validates_resize_modes_limits_and_argument_arrays() {
        assert!(validate_resize_dimensions(ResizeMode::Fit, Some(1600), Some(1200)).is_ok());
        assert!(validate_resize_dimensions(ResizeMode::Width, Some(1600), None).is_ok());
        assert!(validate_resize_dimensions(ResizeMode::Height, None, Some(1200)).is_ok());
        assert!(validate_resize_dimensions(ResizeMode::Fit, Some(1600), None).is_err());
        assert!(validate_resize_dimensions(ResizeMode::Width, Some(0), None).is_err());
        assert!(validate_resize_dimensions(ResizeMode::Width, Some(16_385), None).is_err());
        assert!(validate_resize_dimensions(ResizeMode::Fit, Some(10_000), Some(10_000)).is_err());

        let source = Path::new("/Users/mac/客户 图片/input one.jpeg");
        let output = Path::new("/Users/mac/客户 图片/converted/input one.jpeg");
        let plan =
            build_image_resize_arguments(source, output, ResizeMode::Fit, Some(1600), Some(1200))
                .expect("resize argument plan should be valid");

        assert_eq!(plan.executable, "image-engine");
        assert_eq!(
            plan.arguments,
            vec![
                OsString::from("resize"),
                OsString::from("--input"),
                source.as_os_str().to_os_string(),
                OsString::from("--output"),
                output.as_os_str().to_os_string(),
                OsString::from("--mode"),
                OsString::from("fit"),
                OsString::from("--max-width"),
                OsString::from("1600"),
                OsString::from("--max-height"),
                OsString::from("1200"),
            ]
        );
    }

    #[test]
    fn validates_structured_resize_sidecar_report() {
        let report = parse_resize_sidecar_report(
            r#"{"operation":"resize","mode":"width","sourceWidth":400,"sourceHeight":200,"outputWidth":100,"outputHeight":50,"resized":true,"upscaled":false}"#,
            ResizeMode::Width,
        )
        .expect("valid resize report should parse");
        assert_eq!((report.output_width, report.output_height), (100, 50));

        assert!(parse_resize_sidecar_report(
            r#"{"operation":"resize","mode":"width","sourceWidth":100,"sourceHeight":50,"outputWidth":200,"outputHeight":100,"resized":true,"upscaled":true}"#,
            ResizeMode::Width,
        )
        .is_err());
    }

    #[test]
    fn validates_compression_quality_arguments_and_reports() {
        assert_eq!(normalize_compression_quality("jpg", None), Ok(Some(82)));
        assert_eq!(normalize_compression_quality("webp", None), Ok(Some(80)));
        assert_eq!(normalize_compression_quality("png", None), Ok(None));
        assert!(normalize_compression_quality("jpg", Some(39)).is_err());
        assert!(normalize_compression_quality("webp", Some(96)).is_err());
        assert!(normalize_compression_quality("png", Some(80)).is_err());

        let source = Path::new("/Users/mac/客户 图片/input one.jpeg");
        let output = Path::new("/Users/mac/客户 图片/converted/input one compressed.jpeg");
        let plan = build_image_compress_arguments(source, output, Some(82))
            .expect("compression arguments should be valid");
        assert_eq!(plan.executable, "image-engine");
        assert_eq!(
            plan.arguments,
            vec![
                OsString::from("compress"),
                OsString::from("--input"),
                source.as_os_str().to_os_string(),
                OsString::from("--output"),
                output.as_os_str().to_os_string(),
                OsString::from("--quality"),
                OsString::from("82"),
            ]
        );

        let report = parse_compression_sidecar_report(
            r#"{"operation":"compress","format":"jpg","quality":82,"lossless":false,"sourceWidth":400,"sourceHeight":200,"outputWidth":400,"outputHeight":200,"sourceBytes":1000,"outputBytes":700}"#,
            "jpg",
            Some(82),
            1000,
        )
        .expect("valid compression report should parse");
        assert_eq!(report.output_bytes, 700);
        assert!(parse_compression_sidecar_report(
            r#"{"operation":"compress","format":"jpg","quality":82,"lossless":false,"sourceWidth":400,"sourceHeight":200,"outputWidth":200,"outputHeight":100,"sourceBytes":1000,"outputBytes":700}"#,
            "jpg",
            Some(82),
            1000,
        )
        .is_err());
    }

    #[test]
    fn rejects_missing_non_image_and_same_format_requests() {
        let missing = image_convert_file(ImageConvertExecutionRequest {
            source: "/missing/source.png".to_string(),
            target_format: "webp".to_string(),
        });
        assert!(!missing.success);
        assert!(missing.message.contains("does not exist"));

        let case_dir = temp_case_dir("validation");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("source.png");
        create_test_png(&source);
        let same_format = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "png".to_string(),
        });
        assert!(!same_format.success);
        assert!(same_format.message.contains("must differ"));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_heic_before_creating_output_folder() {
        let case_dir = temp_case_dir("heic-disabled");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("手机 照片.heic");
        fs::write(&source, b"not-decoded").expect("HEIC placeholder should be written");

        let result = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "jpg".to_string(),
        });

        assert!(!result.success);
        assert!(result.message.contains("planned but not enabled"));
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_heic_resize_before_creating_output_folder() {
        let case_dir = temp_case_dir("heic-resize-disabled");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("手机 照片.heic");
        fs::write(&source, b"not-decoded").expect("HEIC placeholder should be written");

        let result = image_resize_file(ImageResizeExecutionRequest {
            source: path_to_string(&source),
            mode: "fit".to_string(),
            max_width: Some(1200),
            max_height: Some(1200),
        });

        assert!(!result.success);
        assert!(result.message.contains("planned but not enabled"));
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_heic_compression_before_creating_output_folder() {
        let case_dir = temp_case_dir("heic-compression-disabled");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("手机 照片.heic");
        fs::write(&source, b"not-decoded").expect("HEIC fixture should be written");

        let result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&source),
            quality: None,
        });

        assert!(!result.success);
        assert!(result.message.contains("planned but not enabled"));
        assert!(!case_dir.join("converted").exists());

        let unsupported = case_dir.join("unsupported.gif");
        fs::write(&unsupported, b"not-decoded").expect("unsupported fixture should be written");
        let unsupported_result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&unsupported),
            quality: None,
        });
        assert!(!unsupported_result.success);
        assert!(unsupported_result.message.contains("must use a .jpg"));
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn cancelled_image_task_does_not_start_or_create_output_folder() {
        let case_dir = temp_case_dir("cancel-before-start");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("cancel me.png");
        create_test_png(&source);

        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("image-cancel-before-start", "image-convert")
            .expect("image task should register");
        registry
            .cancel("image-cancel-before-start")
            .expect("image task should cancel");

        let result = image_convert_task(
            ImageConvertExecutionRequest {
                source: path_to_string(&source),
                target_format: "webp".to_string(),
            },
            control,
        );

        assert!(!result.success);
        assert!(result.message.contains("cancelled locally"));
        assert_eq!(
            registry.status("image-cancel-before-start"),
            Ok(BackendTaskStatus::Cancelled)
        );
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn cancelled_resize_task_does_not_start_or_create_output_folder() {
        let case_dir = temp_case_dir("resize-cancel-before-start");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("cancel resize.png");
        create_test_png(&source);

        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("resize-cancel-before-start", "image-resize")
            .expect("resize task should register");
        registry
            .cancel("resize-cancel-before-start")
            .expect("resize task should cancel");

        let result = image_resize_task(
            ImageResizeExecutionRequest {
                source: path_to_string(&source),
                mode: "width".to_string(),
                max_width: Some(2),
                max_height: None,
            },
            control,
        );

        assert!(!result.success);
        assert!(result.message.contains("cancelled locally"));
        assert_eq!(
            registry.status("resize-cancel-before-start"),
            Ok(BackendTaskStatus::Cancelled)
        );
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn cancelled_compression_task_does_not_publish_or_create_output_folder() {
        let case_dir = temp_case_dir("compression-cancel-before-start");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("cancel compression.jpg");
        create_high_quality_jpeg(&source);

        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("compression-cancel-before-start", "image-compress")
            .expect("compression task should register");
        registry
            .cancel("compression-cancel-before-start")
            .expect("compression task should cancel");

        let result = image_compress_task(
            ImageCompressExecutionRequest {
                source: path_to_string(&source),
                quality: Some(82),
            },
            control,
        );

        assert!(!result.success);
        assert!(!result.published);
        assert!(result.message.contains("cancelled locally"));
        assert_eq!(
            registry.status("compression-cancel-before-start"),
            Ok(BackendTaskStatus::Cancelled)
        );
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn plans_metadata_cleanup_arguments_as_an_array_for_chinese_paths() {
        let plan = build_image_clean_metadata_arguments(
            Path::new("/tmp/客户 文件/手机 照片.jpeg"),
            Path::new("/tmp/客户 文件/converted/手机 照片 cleaned.jpeg"),
        )
        .expect("metadata cleanup arguments should be planned");

        assert_eq!(plan.executable, "image-engine");
        assert_eq!(
            plan.arguments,
            vec![
                OsString::from("clean-metadata"),
                OsString::from("--input"),
                OsString::from("/tmp/客户 文件/手机 照片.jpeg"),
                OsString::from("--output"),
                OsString::from("/tmp/客户 文件/converted/手机 照片 cleaned.jpeg"),
            ]
        );
    }

    #[test]
    fn validates_metadata_cleanup_sidecar_change_and_no_change_reports() {
        let changed = parse_metadata_cleanup_sidecar_report(
            r#"{"operation":"clean-metadata","format":"jpg","changed":true,"metadataItemsRemoved":2,"removedKinds":["EXIF and GPS","XMP"],"sourceWidth":40,"sourceHeight":24,"outputWidth":40,"outputHeight":24,"sourceBytes":900,"outputBytes":700,"pixelDataPreserved":true,"reencoded":false}"#,
            "jpg",
            900,
        )
        .expect("changed cleanup report should validate");
        assert!(changed.changed);
        assert_eq!(changed.metadata_items_removed, 2);

        let unchanged = parse_metadata_cleanup_sidecar_report(
            r#"{"operation":"clean-metadata","format":"png","changed":false,"metadataItemsRemoved":0,"removedKinds":[],"sourceWidth":12,"sourceHeight":10,"outputWidth":12,"outputHeight":10,"sourceBytes":400,"outputBytes":0,"pixelDataPreserved":true,"reencoded":false}"#,
            "png",
            400,
        )
        .expect("no-change cleanup report should validate");
        assert!(!unchanged.changed);

        assert!(parse_metadata_cleanup_sidecar_report(
            r#"{"operation":"clean-metadata","format":"png","changed":true,"metadataItemsRemoved":0,"removedKinds":[],"sourceWidth":12,"sourceHeight":10,"outputWidth":12,"outputHeight":10,"sourceBytes":400,"outputBytes":300,"pixelDataPreserved":true,"reencoded":false}"#,
            "png",
            400,
        )
        .is_err());
    }

    #[test]
    fn cancelled_metadata_cleanup_does_not_start_or_create_output_folder() {
        let case_dir = temp_case_dir("metadata-cleanup-cancel-before-start");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("cancel cleanup.jpg");
        create_jpeg_with_comment_metadata(&source);

        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("metadata-cleanup-cancel", "image-clean-metadata")
            .expect("metadata cleanup task should register");
        registry
            .cancel("metadata-cleanup-cancel")
            .expect("metadata cleanup task should cancel");
        let result = image_clean_metadata_task(
            ImageCleanMetadataExecutionRequest {
                source: path_to_string(&source),
            },
            control,
        );

        assert!(!result.success);
        assert!(!result.published);
        assert!(result.message.contains("cancelled locally"));
        assert_eq!(
            registry.status("metadata-cleanup-cancel"),
            Ok(BackendTaskStatus::Cancelled)
        );
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn metadata_cleanup_rejects_heic_before_creating_output_folder() {
        let case_dir = temp_case_dir("metadata-cleanup-heic");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("phone.heic");
        fs::write(&source, b"unsupported fixture").expect("HEIC fixture should be written");

        let result = image_clean_metadata_file(ImageCleanMetadataExecutionRequest {
            source: path_to_string(&source),
        });

        assert!(!result.success);
        assert!(result.message.contains("planned but not enabled"));
        assert!(!case_dir.join("converted").exists());

        let unsupported = case_dir.join("animation.gif");
        fs::write(&unsupported, b"unsupported fixture")
            .expect("unsupported fixture should be written");
        let unsupported_result = image_clean_metadata_file(ImageCleanMetadataExecutionRequest {
            source: path_to_string(&unsupported),
        });
        assert!(!unsupported_result.success);
        assert!(unsupported_result.message.contains("must use a .jpg"));
        assert!(!case_dir.join("converted").exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn collision_planning_selects_next_available_name() {
        let case_dir = temp_case_dir("collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("report.png");
        create_test_png(&source);
        fs::write(converted_dir.join("report.webp"), b"existing")
            .expect("first collision should be created");
        fs::write(converted_dir.join("report (1).webp"), b"existing")
            .expect("second collision should be created");

        let plan = image_ops::plan_image_output(&path_to_string(&source), "webp")
            .expect("collision-safe output should be planned");
        assert_eq!(plan.planned_output_filename, "report (2).webp");

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn output_validation_rejects_empty_files_and_accepts_images() {
        let case_dir = temp_case_dir("output-validation");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let empty = case_dir.join("empty.png");
        fs::write(&empty, []).expect("empty output should be created");
        assert!(validate_output_image(&empty).is_err());
        assert!(empty.exists());

        let valid = case_dir.join("valid.png");
        create_test_png(&valid);
        let (bytes, width, height) =
            validate_output_image(&valid).expect("valid output should pass");
        assert!(bytes > 0);
        assert_eq!((width, height), (4, 3));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_converts_png_to_webp_and_preserves_source() {
        let case_dir = temp_case_dir("bundled-smoke").join("客户 文件 with spaces");
        fs::create_dir_all(&case_dir).expect("smoke directory should be created");
        let source = case_dir.join("图片 示例.png");
        let source_before = create_test_png(&source);

        let result = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "webp".to_string(),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert!(Path::new(&result.output_path).exists());
        assert_eq!((result.width, result.height), (4, 3));
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("smoke directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_converts_jpeg_to_png_and_preserves_source() {
        let case_dir = temp_case_dir("jpeg-to-png").join("客户 文件 with spaces");
        fs::create_dir_all(&case_dir).expect("smoke directory should be created");
        let source = case_dir.join("手机 照片.jpg");
        let source_before = create_test_rgb_image(&source, ImageFormat::Jpeg);

        let result = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "png".to_string(),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert_eq!((result.width, result.height), (5, 4));
        assert_image_format(Path::new(&result.output_path), ImageFormat::Png);
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("smoke directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_converts_webp_to_jpeg_and_preserves_source() {
        let case_dir = temp_case_dir("webp-to-jpeg").join("客户 文件 with spaces");
        fs::create_dir_all(&case_dir).expect("smoke directory should be created");
        let source = case_dir.join("网页 图片.webp");
        let source_before = create_test_rgb_image(&source, ImageFormat::WebP);

        let result = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "jpg".to_string(),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert_eq!((result.width, result.height), (5, 4));
        assert_image_format(Path::new(&result.output_path), ImageFormat::Jpeg);
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("smoke directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_keeps_existing_output_and_uses_collision_suffix() {
        let case_dir = temp_case_dir("end-to-end-collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("report.png");
        create_test_png(&source);
        let existing_output = converted_dir.join("report.webp");
        fs::write(&existing_output, b"existing-user-output")
            .expect("existing output should be written");

        let result = image_convert_file(ImageConvertExecutionRequest {
            source: path_to_string(&source),
            target_format: "webp".to_string(),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert_eq!(
            PathBuf::from(&result.output_path),
            converted_dir.join("report (1).webp")
        );
        assert_eq!(
            fs::read(&existing_output).expect("existing output should remain readable"),
            b"existing-user-output"
        );
        assert_image_format(Path::new(&result.output_path), ImageFormat::WebP);

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_resizes_png_without_overwrite_or_source_changes() {
        let case_dir = temp_case_dir("resize-end-to-end-collision").join("客户 文件 with spaces");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("图片 示例.png");
        let source_before = create_test_png(&source);
        let existing_output = converted_dir.join("图片 示例.png");
        fs::write(&existing_output, b"existing-user-output")
            .expect("existing output should be written");

        let result = image_resize_file(ImageResizeExecutionRequest {
            source: path_to_string(&source),
            mode: "width".to_string(),
            max_width: Some(2),
            max_height: None,
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert_eq!((result.source_width, result.source_height), (4, 3));
        assert_eq!((result.output_width, result.output_height), (2, 1));
        assert!(result.resized);
        assert_eq!(
            PathBuf::from(&result.output_path),
            converted_dir.join("图片 示例 (1).png")
        );
        assert_eq!(
            fs::read(&existing_output).expect("existing output should remain readable"),
            b"existing-user-output"
        );
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("smoke directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_resize_does_not_upscale() {
        let case_dir = temp_case_dir("resize-no-upscale");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("small.webp");
        let source_before = create_test_rgb_image(&source, ImageFormat::WebP);

        let result = image_resize_file(ImageResizeExecutionRequest {
            source: path_to_string(&source),
            mode: "fit".to_string(),
            max_width: Some(1000),
            max_height: Some(1000),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert_eq!((result.source_width, result.source_height), (5, 4));
        assert_eq!((result.output_width, result.output_height), (5, 4));
        assert!(!result.resized);
        assert!(result.message.contains("without upscaling"));
        assert_eq!(
            fs::read(&source).expect("source should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_compresses_jpeg_without_overwrite_or_source_changes() {
        let case_dir = temp_case_dir("compress-jpeg-collision").join("客户 文件 with spaces");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("报告 照片.jpg");
        let source_before = create_high_quality_jpeg(&source);
        let existing_output = converted_dir.join("报告 照片 compressed.jpg");
        fs::write(&existing_output, b"existing-user-output")
            .expect("existing output should be written");

        let result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&source),
            quality: Some(60),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert!(result.published);
        assert!(result.smaller);
        assert_eq!(result.source_format, "jpg");
        assert_eq!(result.quality, Some(60));
        assert!(result.output_bytes < result.source_bytes);
        assert_eq!(
            PathBuf::from(&result.output_path),
            converted_dir.join("报告 照片 compressed (1).jpg")
        );
        assert_image_format(Path::new(&result.output_path), ImageFormat::Jpeg);
        assert_eq!(fs::read(&existing_output).unwrap(), b"existing-user-output");
        assert_eq!(fs::read(&source).unwrap(), source_before);

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("smoke directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_compresses_webp_and_png_in_their_source_formats() {
        let case_dir = temp_case_dir("compress-webp-png");
        fs::create_dir_all(&case_dir).expect("test directory should be created");

        let webp_source = case_dir.join("网页 图片.webp");
        let webp_before = create_lossless_webp(&webp_source);
        let webp_result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&webp_source),
            quality: Some(60),
        });
        assert!(
            webp_result.success && webp_result.published,
            "{}\n{}",
            webp_result.message,
            webp_result.stderr
        );
        assert!(webp_result.output_bytes < webp_result.source_bytes);
        assert_image_format(Path::new(&webp_result.output_path), ImageFormat::WebP);
        assert_eq!(fs::read(&webp_source).unwrap(), webp_before);

        let png_source = case_dir.join("无损 图片.png");
        let png_before = create_png_with_compression(&png_source, PngCompressionType::Uncompressed);
        let png_result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&png_source),
            quality: None,
        });
        assert!(
            png_result.success && png_result.published,
            "{}\n{}",
            png_result.message,
            png_result.stderr
        );
        assert!(png_result.lossless);
        assert_eq!(png_result.quality, None);
        assert!(png_result.output_bytes < png_result.source_bytes);
        assert_image_format(Path::new(&png_result.output_path), ImageFormat::Png);
        assert_eq!(fs::read(&png_source).unwrap(), png_before);

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_does_not_publish_png_when_optimization_is_not_smaller() {
        let case_dir = temp_case_dir("compress-not-smaller");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("already optimized.png");
        let source_before = create_png_with_compression(&source, PngCompressionType::Best);

        let result = image_compress_file(ImageCompressExecutionRequest {
            source: path_to_string(&source),
            quality: None,
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert!(!result.published);
        assert!(!result.smaller);
        assert!(result.output_path.is_empty());
        assert_eq!(result.message, "压缩后未变小，未生成新文件");
        assert_eq!(fs::read(&source).unwrap(), source_before);
        assert!(!Path::new(&result.planned_output_path).exists());
        let task_workspaces = fs::read_dir(case_dir.join("converted"))
            .expect("converted folder should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".localconvert-task-")
            })
            .count();
        assert_eq!(task_workspaces, 0);

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_cleans_metadata_with_collision_safe_output() {
        let case_dir = temp_case_dir("metadata-cleanup-e2e").join("客户 文件 with spaces");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("手机 隐私.jpg");
        let source_before = create_jpeg_with_comment_metadata(&source);
        let existing_output = converted_dir.join("手机 隐私 cleaned.jpg");
        fs::write(&existing_output, b"existing-user-output")
            .expect("existing output should be written");

        let result = image_clean_metadata_file(ImageCleanMetadataExecutionRequest {
            source: path_to_string(&source),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert!(result.changed);
        assert!(result.published);
        assert!(result.metadata_items_removed >= 1);
        assert!(result.pixel_data_preserved);
        assert!(!result.reencoded);
        assert_eq!(
            PathBuf::from(&result.output_path),
            converted_dir.join("手机 隐私 cleaned (1).jpg")
        );
        assert_image_format(Path::new(&result.output_path), ImageFormat::Jpeg);
        let cleaned = fs::read(&result.output_path).expect("cleaned image should be readable");
        assert!(!cleaned
            .windows(b"private camera owner and location".len())
            .any(|window| window == b"private camera owner and location"));
        assert_eq!(fs::read(&existing_output).unwrap(), b"existing-user-output");
        assert_eq!(fs::read(&source).unwrap(), source_before);

        let _ = fs::remove_dir_all(
            case_dir
                .parent()
                .expect("test directory should have a cleanup parent"),
        );
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_sidecar_skips_metadata_free_image_without_temp_leftovers() {
        let case_dir = temp_case_dir("metadata-cleanup-no-change");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let source = case_dir.join("plain.png");
        let source_before = create_test_png(&source);

        let result = image_clean_metadata_file(ImageCleanMetadataExecutionRequest {
            source: path_to_string(&source),
        });

        assert!(result.success, "{}\n{}", result.message, result.stderr);
        assert!(!result.changed);
        assert!(!result.published);
        assert!(result.output_path.is_empty());
        assert_eq!(result.message, "未发现可清理的元数据，未生成新文件");
        assert!(!Path::new(&result.planned_output_path).exists());
        assert_eq!(fs::read(&source).unwrap(), source_before);
        let task_workspaces = fs::read_dir(case_dir.join("converted"))
            .expect("converted folder should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".localconvert-task-")
            })
            .count();
        assert_eq!(task_workspaces, 0);

        let _ = fs::remove_dir_all(case_dir);
    }
}
