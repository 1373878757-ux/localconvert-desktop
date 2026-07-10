use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const IMAGE_ENGINE_MISSING_MESSAGE: &str = "Not bundled yet.";
const IMAGE_ENGINE_VERSION_PREFIX: &str = "LocalConvert image-engine ";
const IMAGE_ENGINE_SELF_CHECK_OUTPUT: &str = "LocalConvert image-engine self-check ok";

pub struct ImageEngineDetection {
    pub status: &'static str,
    pub message: String,
}

pub fn detect_image_engine(platform: &str) -> ImageEngineDetection {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = image_engine_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    detect_image_engine_from_candidates(platform, &candidates, run_image_engine_smoke_check)
}

pub(crate) fn resolve_image_engine_sidecar_path(platform: &str) -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = std::env::current_exe()
        .ok()
        .and_then(|executable_path| executable_path.parent().map(Path::to_path_buf));
    let candidates = image_engine_candidate_paths(platform, manifest_dir, runtime_dir.as_deref());

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }

        let detection = detect_image_engine_from_candidates(
            platform,
            &[candidate.clone()],
            run_image_engine_smoke_check,
        );
        if detection.status == "available" {
            return Ok(candidate);
        }

        return Err(detection.message);
    }

    Err("image-engine sidecar is not bundled for this platform.".to_string())
}

pub(crate) fn current_platform_key() -> &'static str {
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

fn detect_image_engine_from_candidates<F>(
    platform: &str,
    candidates: &[PathBuf],
    smoke_check: F,
) -> ImageEngineDetection
where
    F: Fn(&Path) -> Result<String, String>,
{
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

    let version = match smoke_check(candidate) {
        Ok(version) => version,
        Err(error) => {
            return ImageEngineDetection {
                status: "error",
                message: format!("image-engine sidecar smoke check failed: {error}"),
            };
        }
    };

    ImageEngineDetection {
        status: "available",
        message: format!(
            "image-engine sidecar smoke check passed: {version} at {candidate_display}"
        ),
    }
}

fn run_image_engine_smoke_check(path: &Path) -> Result<String, String> {
    let version_output = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            format!(
                "unable to run image-engine --version for {}: {error}",
                path_to_string(path)
            )
        })?;
    let version_line = validated_image_engine_output(
        &version_output,
        "--version",
        |line| line.starts_with(IMAGE_ENGINE_VERSION_PREFIX),
        "image-engine --version did not return the expected version line",
    )?;

    let self_check_output = Command::new(path)
        .arg("--self-check")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            format!(
                "unable to run image-engine --self-check for {}: {error}",
                path_to_string(path)
            )
        })?;
    let _self_check_line = validated_image_engine_output(
        &self_check_output,
        "--self-check",
        |line| line == IMAGE_ENGINE_SELF_CHECK_OUTPUT,
        "image-engine --self-check did not return the expected smoke line",
    )?;

    Ok(version_line)
}

fn validated_image_engine_output<F>(
    output: &std::process::Output,
    command_name: &str,
    accept_line: F,
    missing_message: &str,
) -> Result<String, String>
where
    F: Fn(&str) -> bool,
{
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "image-engine {command_name} exited with status {}{}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| accept_line(line.trim()))
        .ok_or_else(|| missing_message.to_string())?;

    Ok(line.trim().to_string())
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn temp_fixture_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("localconvert-image-engine-{name}"))
            .join(unique.to_string())
            .join("image-engine")
    }

    #[test]
    fn resolves_image_engine_sidecar_paths() {
        let base = Path::new("/workspace/src-tauri");
        let paths =
            image_engine_candidate_paths("macos-aarch64", base, Some(Path::new("/app/runtime")));

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/workspace/src-tauri/binaries/macos-aarch64/image-engine"),
                PathBuf::from(
                    "/workspace/src-tauri/binaries/macos-aarch64/image-engine-aarch64-apple-darwin"
                ),
                PathBuf::from("/app/runtime/image-engine-aarch64-apple-darwin"),
                PathBuf::from("/app/runtime/image-engine"),
            ]
        );
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
        let missing = temp_fixture_path("missing");
        let detection = detect_image_engine_from_candidates("macos-aarch64", &[missing], |_| {
            panic!("smoke check should not run for a missing sidecar")
        });

        assert_eq!(detection.status, "not-installed");
        assert_eq!(detection.message, IMAGE_ENGINE_MISSING_MESSAGE);
    }

    #[test]
    fn image_engine_directory_path_returns_error() {
        let fixture = temp_fixture_path("directory");
        fs::create_dir_all(&fixture).expect("test image-engine directory should be created");

        let detection =
            detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                panic!("smoke check should not run for a directory sidecar")
            });

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("path is not a file"));

        let cleanup_root = fixture
            .ancestors()
            .nth(2)
            .expect("fixture should have a cleanup root");
        let _ = fs::remove_dir_all(cleanup_root);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_image_engine_returns_error_on_unix() {
        let fixture = temp_fixture_path("non-executable");
        fs::create_dir_all(fixture.parent().expect("fixture should have a parent"))
            .expect("fixture parent should be created");
        File::create(&fixture).expect("test image-engine fixture should be created");
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o644))
            .expect("fixture permissions should be set");

        let detection =
            detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                panic!("smoke check should not run for a non-executable sidecar")
            });

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("not executable"));

        let cleanup_root = fixture
            .ancestors()
            .nth(2)
            .expect("fixture should have a cleanup root");
        let _ = fs::remove_dir_all(cleanup_root);
    }

    #[test]
    fn executable_image_engine_fixture_returns_available_when_smoke_passes() {
        let fixture = temp_fixture_path("available");
        fs::create_dir_all(fixture.parent().expect("fixture should have a parent"))
            .expect("fixture parent should be created");
        File::create(&fixture).expect("test image-engine fixture should be created");

        #[cfg(unix)]
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755))
            .expect("fixture permissions should be set");

        let detection =
            detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                Ok("LocalConvert image-engine 0.2.0-preview.1".to_string())
            });

        assert_eq!(detection.status, "available");
        assert!(detection.message.contains("sidecar smoke check passed"));

        let cleanup_root = fixture
            .ancestors()
            .nth(2)
            .expect("fixture should have a cleanup root");
        let _ = fs::remove_dir_all(cleanup_root);
    }

    #[test]
    fn smoke_failure_returns_error() {
        let fixture = temp_fixture_path("smoke-failure");
        fs::create_dir_all(fixture.parent().expect("fixture should have a parent"))
            .expect("fixture parent should be created");
        File::create(&fixture).expect("test image-engine fixture should be created");

        #[cfg(unix)]
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o755))
            .expect("fixture permissions should be set");

        let detection =
            detect_image_engine_from_candidates("macos-aarch64", &[fixture.clone()], |_| {
                Err("bad smoke output".to_string())
            });

        assert_eq!(detection.status, "error");
        assert!(detection.message.contains("smoke check failed"));

        let cleanup_root = fixture
            .ancestors()
            .nth(2)
            .expect("fixture should have a cleanup root");
        let _ = fs::remove_dir_all(cleanup_root);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_macos_aarch64_image_engine_sidecar_passes_smoke_check() {
        let detection = detect_image_engine("macos-aarch64");

        assert_eq!(detection.status, "available");
        assert!(detection.message.contains("0.2.0-preview.1"));
    }
}
