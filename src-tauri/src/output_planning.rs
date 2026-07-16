use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const DEFAULT_OUTPUT_STRATEGY: &str = "converted-folder-next-to-source";
const SAME_FOLDER_STRATEGY: &str = "same-folder-as-source";
const ASK_EVERY_TIME_STRATEGY: &str = "ask-every-time";
const REMEMBERED_CUSTOM_FOLDER_STRATEGY: &str = "remembered-custom-folder";
const COLLISION_STRATEGY_EXPLANATION: &str =
    "Never overwrites an existing file; appends (1), (2), ... when needed.";
const MAX_AFFIX_CHARS: usize = 80;
const MAX_FILENAME_CHARS: usize = 240;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct OutputNamingRuleRequest {
    pub(crate) prefix: String,
    pub(crate) suffix_preset: String,
    pub(crate) custom_suffix: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutputPathPlanRequest {
    pub(crate) source: String,
    pub(crate) target_extension: String,
    pub(crate) output_strategy: String,
    #[serde(default)]
    pub(crate) selected_output_folder: Option<String>,
    #[serde(default)]
    pub(crate) remembered_custom_folder: Option<String>,
    #[serde(default)]
    pub(crate) base_name: Option<String>,
    #[serde(default)]
    pub(crate) current_suffix: Option<String>,
    #[serde(default)]
    pub(crate) naming: Option<OutputNamingRuleRequest>,
    #[serde(default)]
    pub(crate) date_token: Option<String>,
    #[serde(default)]
    pub(crate) time_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutputPathPlan {
    pub(crate) source_display_name: String,
    pub(crate) target_extension: String,
    pub(crate) planned_output_directory: String,
    pub(crate) planned_converted_folder_path: String,
    pub(crate) planned_output_stem: String,
    pub(crate) planned_output_filename: String,
    pub(crate) planned_output_path: String,
    pub(crate) collision_strategy_explanation: &'static str,
    pub(crate) used_fallback: bool,
    pub(crate) warning: Option<String>,
}

#[tauri::command]
pub(crate) fn plan_output_path(request: OutputPathPlanRequest) -> Result<OutputPathPlan, String> {
    plan_output_path_inner(&request)
}

pub(crate) fn plan_output_path_inner(
    request: &OutputPathPlanRequest,
) -> Result<OutputPathPlan, String> {
    let source = request.source.trim();
    if source.is_empty() {
        return Err("Source is required for output planning.".to_string());
    }

    let source_path = Path::new(source);
    let source_display_name = display_name(source_path, source);
    let target_extension = normalize_extension(&request.target_extension)?;
    let (output_directory, used_fallback, warning) =
        resolve_output_directory(request, source_path)?;
    let base_name = request
        .base_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| source_stem(&source_display_name));
    let naming = request.naming.clone().unwrap_or_default();
    let tokens = timestamp_tokens(request.date_token.as_deref(), request.time_token.as_deref())?;
    let desired_stem = build_output_stem(
        &base_name,
        request.current_suffix.as_deref().unwrap_or_default(),
        &naming,
        &tokens,
    )?;
    let desired_path = output_directory.join(format!("{desired_stem}.{target_extension}"));
    let planned_output_path = collision_safe_output_path(&desired_path, &[source_path])?;

    let planned_output_filename = planned_output_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Planned output must have a valid filename.".to_string())?
        .to_string();
    let planned_output_stem = planned_output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "Planned output must have a valid filename stem.".to_string())?
        .to_string();
    let output_directory_display = path_to_string(&output_directory);

    Ok(OutputPathPlan {
        source_display_name,
        target_extension,
        planned_output_directory: output_directory_display.clone(),
        planned_converted_folder_path: output_directory_display,
        planned_output_stem,
        planned_output_filename,
        planned_output_path: path_to_string(&planned_output_path),
        collision_strategy_explanation: COLLISION_STRATEGY_EXPLANATION,
        used_fallback,
        warning,
    })
}

pub(crate) fn prepare_execution_output(
    desired_output_path: &Path,
    expected_extension: &str,
    source_paths: &[&Path],
) -> Result<PathBuf, String> {
    if !desired_output_path.is_absolute() {
        return Err("Final output path must be absolute.".to_string());
    }

    let expected_extension = normalize_extension(expected_extension)?;
    let actual_extension = desired_output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !actual_extension.eq_ignore_ascii_case(&expected_extension) {
        return Err(format!(
            "Final output must keep the .{expected_extension} extension."
        ));
    }

    let parent = validate_output_parent(desired_output_path)?;
    let file_name = desired_output_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Final output must have a valid filename.".to_string())?;
    validate_filename(file_name)?;

    let candidate = collision_safe_output_path(desired_output_path, source_paths)?;
    if candidate.parent() != Some(parent) {
        return Err("Final output escaped the selected output folder.".to_string());
    }
    if source_paths.iter().any(|source| candidate == **source) {
        return Err("Final output must not replace a source file.".to_string());
    }

    Ok(candidate)
}

pub(crate) fn validate_output_directory_for_execution(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Output folder path must be absolute.".to_string());
    }
    if path.exists() && !path.is_dir() {
        return Err(format!(
            "Output path exists but is not a folder: {}",
            path_to_string(path)
        ));
    }
    Ok(())
}

fn resolve_output_directory(
    request: &OutputPathPlanRequest,
    source_path: &Path,
) -> Result<(PathBuf, bool, Option<String>), String> {
    let source_parent = source_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let default_directory = source_parent
        .map(|parent| parent.join("converted"))
        .unwrap_or_else(|| PathBuf::from("converted"));

    match request.output_strategy.trim() {
        "" | DEFAULT_OUTPUT_STRATEGY => Ok((default_directory, false, None)),
        SAME_FOLDER_STRATEGY => Ok((
            source_parent
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            false,
            None,
        )),
        ASK_EVERY_TIME_STRATEGY => {
            let selected = required_folder(
                request.selected_output_folder.as_deref(),
                "Choose an output folder before starting this operation.",
            )?;
            validate_existing_directory(&selected)?;
            Ok((selected, false, None))
        }
        REMEMBERED_CUSTOM_FOLDER_STRATEGY => {
            let Some(folder) = request
                .remembered_custom_folder
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Ok((
                    default_directory,
                    true,
                    Some(
                        "记住的输出文件夹尚未设置，已回退到源文件旁的 converted 文件夹。"
                            .to_string(),
                    ),
                ));
            };
            let folder = PathBuf::from(folder);
            match validate_existing_directory(&folder) {
                Ok(()) => Ok((folder, false, None)),
                Err(_) => Ok((
                    default_directory,
                    true,
                    Some(
                        "记住的输出文件夹当前不可用，已回退到源文件旁的 converted 文件夹。"
                            .to_string(),
                    ),
                )),
            }
        }
        strategy => Err(format!("Unsupported output strategy: {strategy}")),
    }
}

fn required_folder(value: Option<&str>, missing_message: &str) -> Result<PathBuf, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| missing_message.to_string())
}

fn validate_existing_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Selected output folder must use an absolute path.".to_string());
    }
    if !path.is_dir() {
        return Err(format!(
            "Selected output folder is unavailable: {}",
            path_to_string(path)
        ));
    }
    Ok(())
}

fn build_output_stem(
    base_name: &str,
    current_suffix: &str,
    naming: &OutputNamingRuleRequest,
    tokens: &TimestampTokens,
) -> Result<String, String> {
    let base_name = base_name.trim();
    if base_name.is_empty() {
        return Err("Output filename cannot be empty.".to_string());
    }
    validate_plain_component(base_name, "Output base name", MAX_FILENAME_CHARS)?;

    let prefix = expand_affix(&naming.prefix, "Output prefix", tokens)?;
    let suffix_template = match naming.suffix_preset.trim() {
        "" | "current" => current_suffix,
        "converted" => "_converted",
        "resized" => "_resized",
        "compressed" => "_compressed",
        "cleaned" => "_cleaned",
        "custom" => {
            if naming.custom_suffix.trim().is_empty() {
                return Err("自定义后缀不能为空。".to_string());
            }
            naming.custom_suffix.as_str()
        }
        preset => return Err(format!("Unsupported output suffix preset: {preset}")),
    };
    let suffix = expand_affix(suffix_template, "Output suffix", tokens)?;
    let stem = format!("{prefix}{base_name}{suffix}");
    validate_plain_component(&stem, "Output filename", MAX_FILENAME_CHARS)?;
    validate_reserved_filename(&stem)?;
    Ok(stem)
}

fn expand_affix(template: &str, label: &str, tokens: &TimestampTokens) -> Result<String, String> {
    let template = template.trim();
    validate_plain_component(template, label, MAX_AFFIX_CHARS)?;
    let expanded = template
        .replace("{date}", &tokens.date)
        .replace("{time}", &tokens.time);
    if expanded.contains('{') || expanded.contains('}') {
        return Err(format!(
            "{label} contains an unsupported token. Only {{date}} and {{time}} are allowed."
        ));
    }
    validate_plain_component(&expanded, label, MAX_AFFIX_CHARS)?;
    Ok(expanded)
}

fn validate_plain_component(value: &str, label: &str, max_chars: usize) -> Result<(), String> {
    if value.chars().count() > max_chars {
        return Err(format!(
            "{label} is too long (maximum {max_chars} characters)."
        ));
    }
    if value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
    }) {
        return Err(format!("{label} contains an unsafe filename character."));
    }
    Ok(())
}

fn validate_filename(file_name: &str) -> Result<(), String> {
    validate_plain_component(file_name, "Output filename", MAX_FILENAME_CHARS)?;
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    validate_reserved_filename(stem)
}

fn validate_reserved_filename(stem: &str) -> Result<(), String> {
    let trimmed = stem.trim();
    if trimmed.is_empty() || trimmed.ends_with('.') {
        return Err("Output filename cannot be empty or end with a dot.".to_string());
    }
    let upper = trimmed.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0');
    if reserved {
        return Err("Output filename uses a reserved system name.".to_string());
    }
    Ok(())
}

fn normalize_extension(extension: &str) -> Result<String, String> {
    let normalized = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err("Target extension must be a plain alphanumeric extension.".to_string());
    }
    Ok(normalized)
}

fn validate_output_parent(path: &Path) -> Result<&Path, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Final output must have a parent folder.".to_string())?;
    validate_output_directory_for_execution(parent)?;
    Ok(parent)
}

fn collision_safe_output_path(
    desired_path: &Path,
    unavailable_paths: &[&Path],
) -> Result<PathBuf, String> {
    let parent = desired_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output path must have a parent folder.".to_string())?;
    if parent.exists() && !parent.is_dir() {
        return Err(format!(
            "Output path exists but is not a folder: {}",
            path_to_string(parent)
        ));
    }
    if !desired_path.exists() && !unavailable_paths.contains(&desired_path) {
        return Ok(desired_path.to_path_buf());
    }

    let stem = desired_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "Output path must have a valid filename stem.".to_string())?;
    let extension = desired_path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| "Output path must keep a file extension.".to_string())?;
    for index in 1_u32.. {
        let candidate = parent.join(format!("{stem} ({index}).{extension}"));
        if !candidate.exists() && !unavailable_paths.contains(&candidate.as_path()) {
            return Ok(candidate);
        }
    }
    unreachable!("an available collision-safe filename should be found")
}

fn display_name(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn source_stem(display_name: &str) -> String {
    Path::new(display_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("untitled")
        .to_string()
}

#[derive(Debug)]
struct TimestampTokens {
    date: String,
    time: String,
}

fn timestamp_tokens(date: Option<&str>, time: Option<&str>) -> Result<TimestampTokens, String> {
    match (date, time) {
        (Some(date), Some(time)) => {
            validate_date_token(date)?;
            validate_time_token(time)?;
            Ok(TimestampTokens {
                date: date.to_string(),
                time: time.to_string(),
            })
        }
        (None, None) => Ok(current_utc_tokens()),
        _ => Err("Date and time tokens must be provided together.".to_string()),
    }
}

fn validate_date_token(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err("Date token must use YYYY-MM-DD.".to_string())
    }
}

fn validate_time_token(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() == 8
        && bytes[2] == b'-'
        && bytes[5] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err("Time token must use HH-MM-SS.".to_string())
    }
}

fn current_utc_tokens() -> TimestampTokens {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    TimestampTokens {
        date: format!("{year:04}-{month:02}-{day:02}"),
        time: format!("{hour:02}-{minute:02}-{second:02}"),
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_case_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "localconvert-output-planning-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn request(source: &Path) -> OutputPathPlanRequest {
        OutputPathPlanRequest {
            source: path_to_string(source),
            target_extension: "pdf".to_string(),
            output_strategy: DEFAULT_OUTPUT_STRATEGY.to_string(),
            selected_output_folder: None,
            remembered_custom_folder: None,
            base_name: None,
            current_suffix: None,
            naming: None,
            date_token: Some("2026-07-17".to_string()),
            time_token: Some("09-08-07".to_string()),
        }
    }

    #[test]
    fn keeps_default_converted_folder_behavior() {
        let source = temp_case_dir("default").join("报告 文件.docx");
        let plan = plan_output_path_inner(&request(&source)).expect("plan output");
        assert_eq!(plan.planned_output_filename, "报告 文件.pdf");
        assert_eq!(
            plan.planned_output_path,
            path_to_string(&source.parent().unwrap().join("converted/报告 文件.pdf"))
        );
    }

    #[test]
    fn supports_same_folder_and_custom_folder() {
        let root = temp_case_dir("locations");
        let custom = root.join("客户 输出");
        fs::create_dir_all(&custom).unwrap();
        let source = root.join("source/report.pdf");
        let mut same = request(&source);
        same.output_strategy = SAME_FOLDER_STRATEGY.to_string();
        assert_eq!(
            plan_output_path_inner(&same)
                .unwrap()
                .planned_output_directory,
            path_to_string(source.parent().unwrap())
        );

        let mut remembered = request(&source);
        remembered.output_strategy = REMEMBERED_CUSTOM_FOLDER_STRATEGY.to_string();
        remembered.remembered_custom_folder = Some(path_to_string(&custom));
        assert_eq!(
            plan_output_path_inner(&remembered)
                .unwrap()
                .planned_output_directory,
            path_to_string(&custom)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_remembered_folder_falls_back_safely() {
        let source = temp_case_dir("fallback").join("report.pdf");
        let mut request = request(&source);
        request.output_strategy = REMEMBERED_CUSTOM_FOLDER_STRATEGY.to_string();
        request.remembered_custom_folder = Some("/missing/localconvert-output".to_string());
        let plan = plan_output_path_inner(&request).unwrap();
        assert!(plan.used_fallback);
        assert!(plan.warning.is_some());
        assert!(plan.planned_output_directory.ends_with("converted"));
    }

    #[test]
    fn applies_prefix_suffix_and_time_tokens() {
        let source = temp_case_dir("tokens").join("照片.jpg");
        let mut request = request(&source);
        request.target_extension = "webp".to_string();
        request.naming = Some(OutputNamingRuleRequest {
            prefix: "{date}_".to_string(),
            suffix_preset: "custom".to_string(),
            custom_suffix: "_{time}".to_string(),
        });
        let plan = plan_output_path_inner(&request).unwrap();
        assert_eq!(
            plan.planned_output_filename,
            "2026-07-17_照片_09-08-07.webp"
        );
    }

    #[test]
    fn rejects_unsafe_and_reserved_names() {
        let source = temp_case_dir("unsafe").join("report.pdf");
        let mut unsafe_request = request(&source);
        unsafe_request.naming = Some(OutputNamingRuleRequest {
            prefix: "../".to_string(),
            ..OutputNamingRuleRequest::default()
        });
        assert!(plan_output_path_inner(&unsafe_request)
            .unwrap_err()
            .contains("unsafe"));

        let mut reserved = request(&source);
        reserved.base_name = Some("CON".to_string());
        assert!(plan_output_path_inner(&reserved)
            .unwrap_err()
            .contains("reserved"));
    }

    #[test]
    fn preserves_extension_and_never_selects_source_as_final_output() {
        let root = temp_case_dir("source-protection");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("report.pdf");
        fs::write(&source, b"source").unwrap();
        let mut request = request(&source);
        request.output_strategy = SAME_FOLDER_STRATEGY.to_string();
        let plan = plan_output_path_inner(&request).unwrap();
        assert_eq!(
            Path::new(&plan.planned_output_path).extension().unwrap(),
            "pdf"
        );
        assert_ne!(Path::new(&plan.planned_output_path), source);
        assert_eq!(plan.planned_output_filename, "report (1).pdf");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn execution_planning_handles_late_collision_without_overwrite() {
        let root = temp_case_dir("late-collision");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.pdf");
        let desired = root.join("result.pdf");
        fs::write(&source, b"source").unwrap();
        fs::write(&desired, b"existing").unwrap();
        let output = prepare_execution_output(&desired, "pdf", &[&source]).unwrap();
        assert_eq!(output, root.join("result (1).pdf"));
        assert_eq!(fs::read(&desired).unwrap(), b"existing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn civil_date_conversion_matches_unix_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }
}
