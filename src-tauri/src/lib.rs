use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_OUTPUT_STRATEGY: &str = "converted-folder-next-to-source";
const COLLISION_STRATEGY_EXPLANATION: &str =
    "Creates a converted folder next to the source file and appends (1), (2), ... when a filename already exists.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineSelfCheck {
    platform: String,
    full_edition: bool,
    conversion_enabled: bool,
    engines: Vec<EngineStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineStatus {
    name: &'static str,
    status: &'static str,
    required_for_v1: bool,
    message: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputPathPlanRequest {
    source: String,
    target_extension: String,
    output_strategy: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct OutputPathPlan {
    source_display_name: String,
    target_extension: String,
    planned_converted_folder_path: String,
    planned_output_filename: String,
    planned_output_path: String,
    collision_strategy_explanation: &'static str,
}

#[tauri::command]
fn engine_self_check() -> EngineSelfCheck {
    EngineSelfCheck {
        platform: current_platform_key(),
        full_edition: true,
        conversion_enabled: false,
        engines: vec![
            EngineStatus {
                name: "LibreOffice headless",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.",
            },
            EngineStatus {
                name: "qpdf",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.",
            },
            EngineStatus {
                name: "PDFium",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.",
            },
            EngineStatus {
                name: "image-engine",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.",
            },
        ],
    }
}

fn current_platform_key() -> String {
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

    format!("{os}-{arch}")
}

#[tauri::command]
fn plan_output_path(request: OutputPathPlanRequest) -> Result<OutputPathPlan, String> {
    plan_output_path_inner(&request)
}

fn plan_output_path_inner(request: &OutputPathPlanRequest) -> Result<OutputPathPlan, String> {
    if request.output_strategy != DEFAULT_OUTPUT_STRATEGY {
        return Err(format!(
            "Unsupported output strategy: {}",
            request.output_strategy
        ));
    }

    let source = request.source.trim();
    if source.is_empty() {
        return Err("Source is required for output planning.".to_string());
    }

    let target_extension = normalize_target_extension(&request.target_extension)?;
    let source_path = Path::new(source);
    let source_display_name = source_display_name(source_path, source);
    let source_base_name = source_base_name(&source_display_name);
    let converted_folder_path = converted_folder_for_source(source_path);
    let existing_names = existing_output_names(&converted_folder_path)?;
    let planned_output_filename =
        next_available_output_name(&source_base_name, &target_extension, &existing_names);
    let planned_output_path = converted_folder_path.join(&planned_output_filename);

    Ok(OutputPathPlan {
        source_display_name,
        target_extension,
        planned_converted_folder_path: path_to_string(&converted_folder_path),
        planned_output_filename,
        planned_output_path: path_to_string(&planned_output_path),
        collision_strategy_explanation: COLLISION_STRATEGY_EXPLANATION,
    })
}

fn normalize_target_extension(extension: &str) -> Result<String, String> {
    let normalized = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("Target extension is required for output planning.".to_string());
    }

    if normalized
        .chars()
        .any(|character| character == '/' || character == '\\' || character.is_whitespace())
    {
        return Err("Target extension must be a plain extension, not a path.".to_string());
    }

    Ok(normalized)
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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            engine_self_check,
            plan_output_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LocalConvert Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, File},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn request(source: String) -> OutputPathPlanRequest {
        OutputPathPlanRequest {
            source,
            target_extension: "pdf".to_string(),
            output_strategy: DEFAULT_OUTPUT_STRATEGY.to_string(),
        }
    }

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "localconvert-output-plan-{}-{name}",
                std::process::id()
            ))
            .join(unique.to_string())
    }

    #[test]
    fn plans_display_name_without_parent_as_converted_preview() {
        let plan = plan_output_path_inner(&request("report.docx".to_string()))
            .expect("display-name planning should succeed");

        assert_eq!(plan.source_display_name, "report.docx");
        assert_eq!(plan.target_extension, "pdf");
        assert_eq!(plan.planned_converted_folder_path, "converted");
        assert_eq!(plan.planned_output_filename, "report.pdf");
        assert_eq!(plan.planned_output_path, "converted/report.pdf");
    }

    #[test]
    fn increments_existing_collision_names() {
        let case_dir = temp_case_dir("collision");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("test converted directory should be created");
        File::create(converted_dir.join("report.pdf")).expect("first collision file should exist");
        File::create(converted_dir.join("report (1).pdf"))
            .expect("second collision file should exist");

        let source = case_dir.join("report.docx");
        let plan = plan_output_path_inner(&request(path_to_string(&source)))
            .expect("real-path planning should succeed");

        assert_eq!(plan.source_display_name, "report.docx");
        assert_eq!(plan.planned_output_filename, "report (2).pdf");
        assert_eq!(
            plan.planned_output_path,
            path_to_string(&converted_dir.join("report (2).pdf"))
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn preserves_spaces_in_output_names() {
        let source = temp_case_dir("spaces").join("quarterly report final.docx");
        let plan = plan_output_path_inner(&request(path_to_string(&source)))
            .expect("space filename planning should succeed");

        assert_eq!(plan.source_display_name, "quarterly report final.docx");
        assert_eq!(plan.planned_output_filename, "quarterly report final.pdf");
    }

    #[test]
    fn preserves_chinese_characters_in_output_names() {
        let source = temp_case_dir("chinese").join("客户 报告.docx");
        let plan = plan_output_path_inner(&request(path_to_string(&source)))
            .expect("Chinese filename planning should succeed");

        assert_eq!(plan.source_display_name, "客户 报告.docx");
        assert_eq!(plan.planned_output_filename, "客户 报告.pdf");
    }

    #[test]
    fn handles_long_filenames() {
        let long_base = "annual-report-".repeat(12);
        let source = format!("{long_base}.docx");
        let plan = plan_output_path_inner(&request(source)).expect("long filename should succeed");

        assert_eq!(plan.source_display_name, format!("{long_base}.docx"));
        assert_eq!(plan.planned_output_filename, format!("{long_base}.pdf"));
    }

    #[test]
    fn normalizes_target_extension() {
        let plan = plan_output_path_inner(&OutputPathPlanRequest {
            source: "report.docx".to_string(),
            target_extension: " .PDF ".to_string(),
            output_strategy: DEFAULT_OUTPUT_STRATEGY.to_string(),
        })
        .expect("target extension should be normalized");

        assert_eq!(plan.target_extension, "pdf");
        assert_eq!(plan.planned_output_filename, "report.pdf");
    }
}
