use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use tauri::Manager;

const PREFERENCES_FILE_NAME: &str = "preferences.json";
const PREFERENCES_TEMP_FILE_NAME: &str = "preferences.json.tmp";
const PREFERENCES_SCHEMA_VERSION: u8 = 1;
const MAX_RESIZE_DIMENSION: u32 = 16_384;
const MAX_RESIZE_PIXELS: u64 = 64_000_000;
const MIN_COMPRESSION_QUALITY: u8 = 40;
const MAX_COMPRESSION_QUALITY: u8 = 95;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSection {
    #[default]
    Pdf,
    ImageConvert,
    ImageResize,
    ImageCompress,
    MetadataCleanup,
    ReportExport,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageTargetFormat {
    Jpg,
    Png,
    #[default]
    Webp,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResizeMode {
    #[default]
    Fit,
    Width,
    Height,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    #[default]
    Csv,
    Json,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct UserPreferences {
    pub schema_version: u8,
    pub active_tool: ToolSection,
    pub image_target_format: ImageTargetFormat,
    pub resize_mode: ResizeMode,
    pub resize_width: u32,
    pub resize_height: u32,
    pub jpeg_compression_quality: u8,
    pub webp_compression_quality: u8,
    pub preferences_panel_expanded: bool,
    pub report_format: ReportFormat,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            active_tool: ToolSection::Pdf,
            image_target_format: ImageTargetFormat::Webp,
            resize_mode: ResizeMode::Fit,
            resize_width: 1_920,
            resize_height: 1_080,
            jpeg_compression_quality: 82,
            webp_compression_quality: 80,
            preferences_panel_expanded: true,
            report_format: ReportFormat::Csv,
        }
    }
}

impl UserPreferences {
    fn normalized(mut self) -> Self {
        self.schema_version = PREFERENCES_SCHEMA_VERSION;
        self.resize_width = self.resize_width.clamp(1, MAX_RESIZE_DIMENSION);
        self.resize_height = self.resize_height.clamp(1, MAX_RESIZE_DIMENSION);

        let requested_pixels = u64::from(self.resize_width) * u64::from(self.resize_height);
        if requested_pixels > MAX_RESIZE_PIXELS {
            self.resize_height = (MAX_RESIZE_PIXELS / u64::from(self.resize_width))
                .max(1)
                .try_into()
                .unwrap_or(UserPreferences::default().resize_height);
        }

        self.jpeg_compression_quality = self
            .jpeg_compression_quality
            .clamp(MIN_COMPRESSION_QUALITY, MAX_COMPRESSION_QUALITY);
        self.webp_compression_quality = self
            .webp_compression_quality
            .clamp(MIN_COMPRESSION_QUALITY, MAX_COMPRESSION_QUALITY);
        self
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesLoadResult {
    pub preferences: UserPreferences,
    pub used_defaults: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesMutationResult {
    pub preferences: UserPreferences,
    pub message: String,
}

#[tauri::command]
pub fn load_preferences(app: tauri::AppHandle) -> PreferencesLoadResult {
    match preferences_path(&app) {
        Ok(path) => load_preferences_from_path(&path),
        Err(error) => PreferencesLoadResult {
            preferences: UserPreferences::default(),
            used_defaults: true,
            warning: Some(error),
        },
    }
}

#[tauri::command]
pub fn save_preferences(
    app: tauri::AppHandle,
    preferences: UserPreferences,
) -> Result<PreferencesMutationResult, String> {
    let path = preferences_path(&app)?;
    save_preferences_to_path(&path, preferences)
}

#[tauri::command]
pub fn reset_preferences(app: tauri::AppHandle) -> Result<PreferencesMutationResult, String> {
    let path = preferences_path(&app)?;
    reset_preferences_at_path(&path)
}

fn preferences_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(PREFERENCES_FILE_NAME))
        .map_err(|error| format!("无法定位本机偏好设置目录：{error}"))
}

fn load_preferences_from_path(path: &Path) -> PreferencesLoadResult {
    if !path.exists() {
        return PreferencesLoadResult {
            preferences: UserPreferences::default(),
            used_defaults: true,
            warning: None,
        };
    }

    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            return PreferencesLoadResult {
                preferences: UserPreferences::default(),
                used_defaults: true,
                warning: Some(format!("无法读取本机偏好设置，已使用安全默认值：{error}")),
            };
        }
    };

    let parsed = match serde_json::from_str::<UserPreferences>(&contents) {
        Ok(preferences) => preferences,
        Err(error) => {
            return PreferencesLoadResult {
                preferences: UserPreferences::default(),
                used_defaults: true,
                warning: Some(format!("偏好设置文件已损坏，已使用安全默认值：{error}")),
            };
        }
    };

    if parsed.schema_version != PREFERENCES_SCHEMA_VERSION {
        return PreferencesLoadResult {
            preferences: UserPreferences::default(),
            used_defaults: true,
            warning: Some(format!(
                "偏好设置版本 {} 暂不受支持，已使用安全默认值。",
                parsed.schema_version
            )),
        };
    }

    let normalized = parsed.clone().normalized();
    let adjusted = parsed != normalized;
    PreferencesLoadResult {
        preferences: normalized,
        used_defaults: adjusted,
        warning: adjusted.then(|| "部分偏好设置超出安全范围，已自动调整。".to_string()),
    }
}

fn save_preferences_to_path(
    path: &Path,
    preferences: UserPreferences,
) -> Result<PreferencesMutationResult, String> {
    let preferences = preferences.normalized();
    let parent = path
        .parent()
        .ok_or_else(|| "偏好设置路径缺少父目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建本机偏好设置目录：{error}"))?;

    let serialized = serde_json::to_vec_pretty(&preferences)
        .map_err(|error| format!("无法序列化偏好设置：{error}"))?;
    let temporary_path = parent.join(PREFERENCES_TEMP_FILE_NAME);
    let write_result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary_path)
            .map_err(|error| format!("无法创建偏好设置临时文件：{error}"))?;
        file.write_all(&serialized)
            .map_err(|error| format!("无法写入偏好设置临时文件：{error}"))?;
        file.sync_all()
            .map_err(|error| format!("无法同步偏好设置临时文件：{error}"))?;

        #[cfg(target_os = "windows")]
        if path.exists() {
            fs::remove_file(path).map_err(|error| format!("无法替换旧偏好设置文件：{error}"))?;
        }

        fs::rename(&temporary_path, path)
            .map_err(|error| format!("无法发布偏好设置文件：{error}"))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result?;

    Ok(PreferencesMutationResult {
        preferences,
        message: "偏好设置已保存到本机。".to_string(),
    })
}

fn reset_preferences_at_path(path: &Path) -> Result<PreferencesMutationResult, String> {
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("无法重置偏好设置：{error}"))?;
    }
    if let Some(parent) = path.parent() {
        let temporary_path = parent.join(PREFERENCES_TEMP_FILE_NAME);
        if temporary_path.exists() {
            fs::remove_file(&temporary_path)
                .map_err(|error| format!("无法清理偏好设置临时文件：{error}"))?;
        }
    }

    Ok(PreferencesMutationResult {
        preferences: UserPreferences::default(),
        message: "偏好设置已重置；任务记录和文件未更改。".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "localconvert-preferences-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn preferences_round_trip_through_json() {
        let preferences = UserPreferences {
            active_tool: ToolSection::ImageResize,
            image_target_format: ImageTargetFormat::Png,
            resize_mode: ResizeMode::Width,
            resize_width: 2_048,
            report_format: ReportFormat::Json,
            preferences_panel_expanded: false,
            ..UserPreferences::default()
        };

        let serialized = serde_json::to_string(&preferences).expect("serialize preferences");
        let decoded: UserPreferences =
            serde_json::from_str(&serialized).expect("deserialize preferences");

        assert_eq!(decoded, preferences);
    }

    #[test]
    fn corrupt_preferences_fall_back_without_deleting_file() {
        let directory = test_directory("corrupt");
        fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join(PREFERENCES_FILE_NAME);
        fs::write(&path, "{not-json").expect("write corrupt preferences");

        let result = load_preferences_from_path(&path);

        assert_eq!(result.preferences, UserPreferences::default());
        assert!(result.used_defaults);
        assert!(result.warning.is_some());
        assert!(path.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn unsupported_schema_falls_back_safely() {
        let directory = test_directory("schema");
        fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join(PREFERENCES_FILE_NAME);
        let preferences = UserPreferences {
            schema_version: PREFERENCES_SCHEMA_VERSION + 1,
            ..UserPreferences::default()
        };
        fs::write(
            &path,
            serde_json::to_vec(&preferences).expect("serialize preferences"),
        )
        .expect("write preferences");

        let result = load_preferences_from_path(&path);

        assert_eq!(result.preferences, UserPreferences::default());
        assert!(result.used_defaults);
        assert!(result.warning.expect("warning").contains("暂不受支持"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn invalid_numeric_values_are_clamped_to_safe_ranges() {
        let normalized = UserPreferences {
            resize_width: 100_000,
            resize_height: 100_000,
            jpeg_compression_quality: 1,
            webp_compression_quality: 100,
            ..UserPreferences::default()
        }
        .normalized();

        assert_eq!(normalized.resize_width, MAX_RESIZE_DIMENSION);
        assert!(normalized.resize_height <= MAX_RESIZE_DIMENSION);
        assert!(
            u64::from(normalized.resize_width) * u64::from(normalized.resize_height)
                <= MAX_RESIZE_PIXELS
        );
        assert_eq!(normalized.jpeg_compression_quality, MIN_COMPRESSION_QUALITY);
        assert_eq!(normalized.webp_compression_quality, MAX_COMPRESSION_QUALITY);
    }

    #[test]
    fn default_compression_qualities_remain_valid() {
        let preferences = UserPreferences::default();
        assert!((MIN_COMPRESSION_QUALITY..=MAX_COMPRESSION_QUALITY)
            .contains(&preferences.jpeg_compression_quality));
        assert!((MIN_COMPRESSION_QUALITY..=MAX_COMPRESSION_QUALITY)
            .contains(&preferences.webp_compression_quality));
    }

    #[test]
    fn reset_removes_only_preference_owned_files() {
        let directory = test_directory("reset");
        fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join(PREFERENCES_FILE_NAME);
        let sibling = directory.join("keep-this-report.json");
        fs::write(&path, "preferences").expect("write preferences");
        fs::write(directory.join(PREFERENCES_TEMP_FILE_NAME), "temporary")
            .expect("write temp preferences");
        fs::write(&sibling, "report").expect("write sibling file");

        let result = reset_preferences_at_path(&path).expect("reset preferences");

        assert_eq!(result.preferences, UserPreferences::default());
        assert!(!path.exists());
        assert!(!directory.join(PREFERENCES_TEMP_FILE_NAME).exists());
        assert!(sibling.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn serialized_preferences_exclude_paths_history_and_metadata() {
        let serialized = serde_json::to_string(&UserPreferences::default())
            .expect("serialize default preferences")
            .to_ascii_lowercase();

        for forbidden in [
            "sourcepath",
            "outputpath",
            "reportpath",
            "taskhistory",
            "exif",
            "gps",
            "xmp",
            "iptc",
            "token",
        ] {
            assert!(!serialized.contains(forbidden), "found {forbidden}");
        }
    }
}
