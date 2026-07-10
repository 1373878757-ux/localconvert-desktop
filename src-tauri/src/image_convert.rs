use crate::{image_engine, image_ops};
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
}

#[tauri::command]
pub fn image_convert_file(request: ImageConvertExecutionRequest) -> ImageConvertResult {
    let source_path = request.source.trim().to_string();
    let source_format = source_format_from_path(Path::new(&source_path)).unwrap_or_default();
    let target_format = normalize_target_format(&request.target_format).unwrap_or_default();

    execute_image_convert(&request).unwrap_or_else(|message| ImageConvertResult {
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
    })
}

fn execute_image_convert(
    request: &ImageConvertExecutionRequest,
) -> Result<ImageConvertResult, String> {
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
    fs::create_dir_all(&converted_folder).map_err(|error| {
        format!(
            "Unable to create converted output folder {}: {error}",
            path_to_string(&converted_folder)
        )
    })?;
    if output_path.exists() {
        return Err(format!(
            "Output image already exists and will not be overwritten: {}",
            path_to_string(&output_path)
        ));
    }

    let plan = build_image_convert_arguments(&source_path, &output_path, &target_format)?;
    let execution = run_image_engine_command(
        &image_engine_path,
        &plan.arguments,
        Duration::from_secs(IMAGE_CONVERT_TIMEOUT_SECONDS),
    )?;

    if execution.timed_out {
        remove_partial_output(&output_path);
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
        remove_partial_output(&output_path);
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

    let (output_bytes, width, height) = match validate_output_image(&output_path) {
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
    Ok(ImageConvertResult {
        success: true,
        operation: "convert",
        source_path: path_to_string(&source_path),
        output_path: path_to_string(&output_path),
        source_format,
        target_format,
        output_bytes,
        width,
        height,
        stdout: execution.stdout,
        stderr: execution.stderr,
        exit_code: execution.exit_code,
        timed_out: false,
        message: "Image conversion completed locally with bundled image-engine.".to_string(),
    })
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

fn run_image_engine_command(
    executable: &Path,
    arguments: &[OsString],
    timeout: Duration,
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
    let started_at = Instant::now();

    let (exit_code, timed_out) = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Unable to inspect image-engine process state: {error}"))?
        {
            break (status.code(), false);
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }

        thread::sleep(Duration::from_millis(50));
    };

    Ok(ImageEngineExecutionResult {
        stdout: join_pipe_reader(stdout_reader),
        stderr: join_pipe_reader(stderr_reader),
        exit_code,
        timed_out,
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
        remove_partial_output(output_path);
        return Err("image-engine reported success, but output image is empty.".to_string());
    }

    let (width, height) = image::image_dimensions(output_path).map_err(|error| {
        remove_partial_output(output_path);
        format!("image-engine output image could not be validated: {error}")
    })?;
    if width == 0 || height == 0 {
        remove_partial_output(output_path);
        return Err("image-engine output image has invalid dimensions.".to_string());
    }

    Ok((metadata.len(), width, height))
}

fn remove_partial_output(output_path: &Path) {
    if output_path.exists() {
        let _ = fs::remove_file(output_path);
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
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
        assert!(!empty.exists());

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
}
