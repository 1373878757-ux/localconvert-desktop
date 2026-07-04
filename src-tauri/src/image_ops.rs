use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const DEFAULT_OUTPUT_STRATEGY: &str = "converted-folder-next-to-source";
const COLLISION_STRATEGY_EXPLANATION: &str =
    "Creates a converted folder next to the source file and appends (1), (2), ... when a filename already exists.";
const IMAGE_ENGINE_STATUS: &str = "not-bundled";
const IMAGE_ENGINE_MESSAGE: &str = "Image engine is not bundled yet.";
const IMAGE_ENGINE_MISSING_MESSAGE: &str = "Not bundled yet.";
const INPUT_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "tiff", "tif", "heic"];
const OUTPUT_FORMATS: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "tiff"];
const COMPRESSION_PRESETS: &[&str] = &["high-quality", "balanced", "small-size"];

pub struct ImageEngineDetection {
    pub status: &'static str,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConvertRequest {
    source: String,
    target_format: String,
    output_strategy: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCompressRequest {
    source: String,
    preset: String,
    output_format: Option<String>,
    output_strategy: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResizeRequest {
    source: String,
    width: Option<u32>,
    height: Option<u32>,
    output_format: Option<String>,
    output_strategy: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRemoveMetadataRequest {
    source: String,
    output_format: Option<String>,
    output_strategy: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageOperationPlan {
    operation: &'static str,
    source_display_name: String,
    source_extension: String,
    target_format: String,
    planned_converted_folder_path: String,
    planned_output_filename: String,
    planned_output_path: String,
    collision_strategy_explanation: &'static str,
    engine_status: &'static str,
    conversion_enabled: bool,
    message: &'static str,
}

struct PlannedImageOutput {
    source_display_name: String,
    source_extension: String,
    planned_converted_folder_path: String,
    planned_output_filename: String,
    planned_output_path: String,
}

#[tauri::command]
pub fn plan_image_convert(request: ImageConvertRequest) -> Result<ImageOperationPlan, String> {
    validate_output_strategy(&request.output_strategy)?;
    let target_format = normalize_output_format(&request.target_format)?;
    plan_image_operation("convert", &request.source, &target_format)
}

#[tauri::command]
pub fn plan_image_compress(request: ImageCompressRequest) -> Result<ImageOperationPlan, String> {
    validate_output_strategy(&request.output_strategy)?;
    validate_compression_preset(&request.preset)?;
    let target_format =
        target_format_or_source_default(&request.source, request.output_format.as_deref())?;
    plan_image_operation("compress", &request.source, &target_format)
}

#[tauri::command]
pub fn plan_image_resize(request: ImageResizeRequest) -> Result<ImageOperationPlan, String> {
    validate_output_strategy(&request.output_strategy)?;
    validate_resize_dimensions(request.width, request.height)?;
    let target_format =
        target_format_or_source_default(&request.source, request.output_format.as_deref())?;
    plan_image_operation("resize", &request.source, &target_format)
}

#[tauri::command]
pub fn plan_image_remove_metadata(
    request: ImageRemoveMetadataRequest,
) -> Result<ImageOperationPlan, String> {
    validate_output_strategy(&request.output_strategy)?;
    let target_format =
        target_format_or_source_default(&request.source, request.output_format.as_deref())?;
    plan_image_operation("remove-metadata", &request.source, &target_format)
}

pub fn detect_image_engine(platform: &str) -> ImageEngineDetection {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = image_engine_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    detect_image_engine_from_candidates(platform, &candidates)
}

fn detect_image_engine_from_candidates(
    platform: &str,
    candidates: &[PathBuf],
) -> ImageEngineDetection {
    let Some(candidate) = candidates.iter().find(|path| path.exists()) else {
        return ImageEngineDetection {
            status: "not-installed",
            message: IMAGE_ENGINE_MISSING_MESSAGE.to_string(),
        };
    };

    let candidate_display = path_to_string(candidate);
    let Ok(metadata) = fs::metadata(candidate) else {
        return ImageEngineDetection {
            status: "error",
            message: format!(
                "image-engine sidecar exists but could not be inspected: {candidate_display}"
            ),
        };
    };

    if !metadata.is_file() {
        return ImageEngineDetection {
            status: "error",
            message: format!("image-engine sidecar path is not a file: {candidate_display}"),
        };
    }

    if requires_executable_permission(platform) && !is_executable(&metadata) {
        return ImageEngineDetection {
            status: "error",
            message: format!("image-engine sidecar is not executable: {candidate_display}"),
        };
    }

    ImageEngineDetection {
        status: "available",
        message: format!(
            "image-engine sidecar detected; version check is not enabled yet: {candidate_display}"
        ),
    }
}

fn plan_image_operation(
    operation: &'static str,
    source: &str,
    target_format: &str,
) -> Result<ImageOperationPlan, String> {
    let output = plan_image_output(source, target_format)?;

    Ok(ImageOperationPlan {
        operation,
        source_display_name: output.source_display_name,
        source_extension: output.source_extension,
        target_format: target_format.to_string(),
        planned_converted_folder_path: output.planned_converted_folder_path,
        planned_output_filename: output.planned_output_filename,
        planned_output_path: output.planned_output_path,
        collision_strategy_explanation: COLLISION_STRATEGY_EXPLANATION,
        engine_status: IMAGE_ENGINE_STATUS,
        conversion_enabled: false,
        message: IMAGE_ENGINE_MESSAGE,
    })
}

fn plan_image_output(source: &str, target_format: &str) -> Result<PlannedImageOutput, String> {
    let trimmed_source = source.trim();
    if trimmed_source.is_empty() {
        return Err("Source image path is required.".to_string());
    }

    let source_path = Path::new(trimmed_source);
    let source_extension = supported_input_extension(source_path)?;
    let source_display_name = source_display_name(source_path, trimmed_source);
    let source_base_name = source_base_name(&source_display_name);
    let converted_folder_path = converted_folder_for_source(source_path);
    let existing_names = existing_output_names(&converted_folder_path)?;
    let planned_output_filename =
        next_available_output_name(&source_base_name, target_format, &existing_names);
    let planned_output_path = converted_folder_path.join(&planned_output_filename);

    Ok(PlannedImageOutput {
        source_display_name,
        source_extension,
        planned_converted_folder_path: path_to_string(&converted_folder_path),
        planned_output_filename,
        planned_output_path: path_to_string(&planned_output_path),
    })
}

fn validate_output_strategy(output_strategy: &Option<String>) -> Result<(), String> {
    match output_strategy.as_deref().map(str::trim) {
        None | Some("") | Some(DEFAULT_OUTPUT_STRATEGY) => Ok(()),
        Some(strategy) => Err(format!("Unsupported output strategy: {strategy}")),
    }
}

fn supported_input_extension(source_path: &Path) -> Result<String, String> {
    let extension = normalized_path_extension(source_path)
        .ok_or_else(|| "Source image must include a supported file extension.".to_string())?;

    if is_supported_input_extension(&extension) {
        Ok(extension)
    } else {
        Err(format!("Unsupported image input extension: {extension}"))
    }
}

fn normalized_path_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
}

fn normalize_output_format(format: &str) -> Result<String, String> {
    let normalized = format.trim().trim_start_matches('.').to_ascii_lowercase();

    if normalized.is_empty() {
        return Err("Target image format is required.".to_string());
    }

    if normalized
        .chars()
        .any(|character| character == '/' || character == '\\' || character.is_whitespace())
    {
        return Err("Target image format must be a plain extension, not a path.".to_string());
    }

    if is_supported_output_format(&normalized) {
        Ok(normalized)
    } else if normalized == "heic" {
        Err("HEIC is planned as input only until a real image engine is bundled.".to_string())
    } else {
        Err(format!("Unsupported image output format: {normalized}"))
    }
}

fn target_format_or_source_default(
    source: &str,
    requested_format: Option<&str>,
) -> Result<String, String> {
    if let Some(format) = requested_format {
        return normalize_output_format(format);
    }

    let source_extension = supported_input_extension(Path::new(source.trim()))?;
    match source_extension.as_str() {
        "tif" => Ok("tiff".to_string()),
        "heic" => Err(
            "HEIC is planned as input only; choose a supported output format for planning."
                .to_string(),
        ),
        _ => normalize_output_format(&source_extension),
    }
}

fn validate_compression_preset(preset: &str) -> Result<(), String> {
    let normalized = preset.trim().to_ascii_lowercase();
    if COMPRESSION_PRESETS.contains(&normalized.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "Unsupported image compression preset: {normalized}. Use high-quality, balanced, or small-size."
        ))
    }
}

fn validate_resize_dimensions(width: Option<u32>, height: Option<u32>) -> Result<(), String> {
    if width.is_none() && height.is_none() {
        return Err("Image resize planning requires width, height, or both.".to_string());
    }

    if matches!(width, Some(0)) || matches!(height, Some(0)) {
        return Err("Image resize dimensions must be greater than zero.".to_string());
    }

    Ok(())
}

fn is_supported_input_extension(extension: &str) -> bool {
    INPUT_EXTENSIONS.contains(&extension)
}

fn is_supported_output_format(format: &str) -> bool {
    OUTPUT_FORMATS.contains(&format)
}

fn source_display_name(source_path: &Path, fallback_source: &str) -> String {
    source_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_source)
        .to_string()
}

fn source_base_name(source_display_name: &str) -> String {
    Path::new(source_display_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("untitled")
        .to_string()
}

fn converted_folder_for_source(source_path: &Path) -> PathBuf {
    source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join("converted"))
        .unwrap_or_else(|| PathBuf::from("converted"))
}

fn existing_output_names(converted_folder_path: &Path) -> Result<HashSet<String>, String> {
    if !converted_folder_path.exists() {
        return Ok(HashSet::new());
    }

    if !converted_folder_path.is_dir() {
        return Err(format!(
            "Planned converted output path is not a folder: {}",
            path_to_string(converted_folder_path)
        ));
    }

    let entries = fs::read_dir(converted_folder_path).map_err(|error| {
        format!(
            "Unable to inspect converted output folder {}: {error}",
            path_to_string(converted_folder_path)
        )
    })?;

    let mut names = HashSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Unable to inspect a converted output folder entry in {}: {error}",
                path_to_string(converted_folder_path)
            )
        })?;

        names.insert(entry.file_name().to_string_lossy().into_owned());
    }

    Ok(names)
}

fn next_available_output_name(
    source_base_name: &str,
    target_extension: &str,
    existing_names: &HashSet<String>,
) -> String {
    let desired_name = format!("{source_base_name}.{target_extension}");
    if !existing_names.contains(&desired_name) {
        return desired_name;
    }

    let mut index = 1;
    loop {
        let candidate = format!("{source_base_name} ({index}).{target_extension}");
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn image_engine_candidate_paths(
    platform: &str,
    src_tauri_dir: &Path,
    runtime_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths = vec![
        src_tauri_dir
            .join("binaries")
            .join(platform)
            .join(image_engine_raw_filename(platform)),
        src_tauri_dir
            .join("binaries")
            .join(platform)
            .join(image_engine_prepared_filename(platform)),
    ];

    if let Some(runtime_dir) = runtime_dir {
        paths.push(runtime_dir.join(image_engine_prepared_filename(platform)));
        paths.push(runtime_dir.join(image_engine_raw_filename(platform)));
    }

    paths
}

fn image_engine_raw_filename(platform: &str) -> String {
    if platform.starts_with("windows-") {
        "image-engine.exe".to_string()
    } else {
        "image-engine".to_string()
    }
}

fn image_engine_prepared_filename(platform: &str) -> String {
    let extension = if platform.starts_with("windows-") {
        ".exe"
    } else {
        ""
    };

    format!("image-engine-{}{}", target_triple(platform), extension)
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
            .join(format!("localconvert-image-plan-{name}"))
            .join(unique.to_string())
    }

    #[test]
    fn detects_supported_input_extensions() {
        for extension in INPUT_EXTENSIONS {
            let source = PathBuf::from(format!("sample.{extension}"));
            assert_eq!(
                supported_input_extension(&source),
                Ok((*extension).to_string())
            );
        }
    }

    #[test]
    fn rejects_unsupported_input_extensions() {
        let result = plan_image_convert(ImageConvertRequest {
            source: "sample.gif".to_string(),
            target_format: "png".to_string(),
            output_strategy: None,
        });

        assert!(result
            .expect_err("gif should not be accepted")
            .contains("Unsupported image input extension"));
    }

    #[test]
    fn validates_output_formats() {
        assert_eq!(normalize_output_format(" .WEBP "), Ok("webp".to_string()));
        assert!(normalize_output_format("gif").is_err());
        assert!(normalize_output_format("../png").is_err());
    }

    #[test]
    fn treats_heic_as_input_only() {
        let plan = plan_image_convert(ImageConvertRequest {
            source: "照片.heic".to_string(),
            target_format: "jpg".to_string(),
            output_strategy: None,
        })
        .expect("HEIC input can be planned when the output format is supported");

        assert_eq!(plan.source_extension, "heic");
        assert_eq!(plan.target_format, "jpg");
        assert!(normalize_output_format("heic").is_err());
    }

    #[test]
    fn preserves_chinese_filenames() {
        let source = temp_case_dir("chinese").join("客户 图片.png");
        let plan = plan_image_convert(ImageConvertRequest {
            source: path_to_string(&source),
            target_format: "webp".to_string(),
            output_strategy: None,
        })
        .expect("Chinese filename should be planned");

        assert_eq!(plan.source_display_name, "客户 图片.png");
        assert_eq!(plan.planned_output_filename, "客户 图片.webp");
    }

    #[test]
    fn preserves_spaces_in_paths() {
        let source = temp_case_dir("spaces")
            .join("folder with spaces")
            .join("quarterly image final.jpeg");
        let plan = plan_image_convert(ImageConvertRequest {
            source: path_to_string(&source),
            target_format: "png".to_string(),
            output_strategy: None,
        })
        .expect("space path should be planned");

        assert_eq!(plan.source_display_name, "quarterly image final.jpeg");
        assert_eq!(plan.planned_output_filename, "quarterly image final.png");
        assert!(plan
            .planned_converted_folder_path
            .contains("folder with spaces"));
    }

    #[test]
    fn applies_collision_safe_naming() {
        let case_dir = temp_case_dir("collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        File::create(converted_dir.join("report.webp")).expect("first collision should exist");
        File::create(converted_dir.join("report (1).webp")).expect("second collision should exist");

        let source = case_dir.join("report.png");
        let plan = plan_image_convert(ImageConvertRequest {
            source: path_to_string(&source),
            target_format: "webp".to_string(),
            output_strategy: Some(DEFAULT_OUTPUT_STRATEGY.to_string()),
        })
        .expect("collision planning should succeed");

        assert_eq!(plan.planned_output_filename, "report (2).webp");
        assert_eq!(
            plan.planned_output_path,
            path_to_string(&converted_dir.join("report (2).webp"))
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn resolves_image_engine_sidecar_paths() {
        let base = Path::new("/workspace/src-tauri");
        let paths =
            image_engine_candidate_paths("macos-aarch64", base, Some(Path::new("/app/runtime")));

        assert_eq!(
            paths[0],
            base.join("binaries")
                .join("macos-aarch64")
                .join("image-engine")
        );
        assert_eq!(
            paths[1],
            base.join("binaries")
                .join("macos-aarch64")
                .join("image-engine-aarch64-apple-darwin")
        );
        assert_eq!(
            paths[2],
            Path::new("/app/runtime").join("image-engine-aarch64-apple-darwin")
        );
        assert_eq!(paths[3], Path::new("/app/runtime").join("image-engine"));

        assert_eq!(
            image_engine_raw_filename("windows-x86_64"),
            "image-engine.exe"
        );
        assert_eq!(
            image_engine_prepared_filename("windows-x86_64"),
            "image-engine-x86_64-pc-windows-msvc.exe"
        );
    }

    #[test]
    fn missing_image_engine_returns_not_installed() {
        let missing = temp_case_dir("missing-engine").join("image-engine");
        let detection = detect_image_engine_from_candidates("macos-aarch64", &[missing]);

        assert_eq!(detection.status, "not-installed");
        assert_eq!(detection.message, IMAGE_ENGINE_MISSING_MESSAGE);
    }

    #[test]
    fn image_engine_directory_path_returns_error() {
        let case_dir = temp_case_dir("directory-engine");
        fs::create_dir_all(&case_dir).expect("directory fixture should be created");
        let detection = detect_image_engine_from_candidates("macos-aarch64", &[case_dir.clone()]);

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("path is not a file"));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_image_engine_returns_error_on_unix() {
        let case_dir = temp_case_dir("non-executable-engine");
        let fixture = case_dir.join("image-engine");
        fs::create_dir_all(fixture.parent().expect("fixture should have a parent"))
            .expect("fixture parent should be created");
        File::create(&fixture).expect("fixture file should be created");
        let mut permissions = fs::metadata(&fixture)
            .expect("fixture metadata should be available")
            .permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&fixture, permissions).expect("fixture permissions should be set");

        let detection = detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()]);

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("not executable"));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[cfg(unix)]
    #[test]
    fn executable_image_engine_fixture_returns_available_without_smoke_check() {
        let case_dir = temp_case_dir("executable-engine");
        let fixture = case_dir.join("image-engine");
        fs::create_dir_all(fixture.parent().expect("fixture should have a parent"))
            .expect("fixture parent should be created");
        File::create(&fixture).expect("fixture file should be created");
        let mut permissions = fs::metadata(&fixture)
            .expect("fixture metadata should be available")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fixture, permissions).expect("fixture permissions should be set");

        let detection = detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()]);

        assert_eq!(detection.status, "available");
        assert!(detection
            .message
            .contains("version check is not enabled yet"));

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn returns_engine_not_bundled_response() {
        let plan = plan_image_convert(ImageConvertRequest {
            source: "sample.png".to_string(),
            target_format: "avif".to_string(),
            output_strategy: None,
        })
        .expect("valid request should return disabled plan");

        assert_eq!(plan.engine_status, "not-bundled");
        assert!(!plan.conversion_enabled);
        assert_eq!(plan.message, IMAGE_ENGINE_MESSAGE);
    }

    #[test]
    fn validates_compress_resize_and_metadata_requests() {
        let compressed = plan_image_compress(ImageCompressRequest {
            source: "sample.jpg".to_string(),
            preset: "balanced".to_string(),
            output_format: None,
            output_strategy: None,
        })
        .expect("balanced compression should be planned");
        assert_eq!(compressed.operation, "compress");
        assert_eq!(compressed.target_format, "jpg");

        assert!(plan_image_compress(ImageCompressRequest {
            source: "sample.jpg".to_string(),
            preset: "tiny".to_string(),
            output_format: None,
            output_strategy: None,
        })
        .is_err());

        let resized = plan_image_resize(ImageResizeRequest {
            source: "sample.tif".to_string(),
            width: Some(1280),
            height: None,
            output_format: None,
            output_strategy: None,
        })
        .expect("resize with one dimension should be planned");
        assert_eq!(resized.operation, "resize");
        assert_eq!(resized.target_format, "tiff");

        assert!(plan_image_resize(ImageResizeRequest {
            source: "sample.png".to_string(),
            width: Some(0),
            height: None,
            output_format: None,
            output_strategy: None,
        })
        .is_err());

        let metadata_plan = plan_image_remove_metadata(ImageRemoveMetadataRequest {
            source: "sample.webp".to_string(),
            output_format: None,
            output_strategy: None,
        })
        .expect("metadata removal should be planned");
        assert_eq!(metadata_plan.operation, "remove-metadata");
    }
}
