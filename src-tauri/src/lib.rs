use serde::Serialize;

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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_self_check])
        .run(tauri::generate_context!())
        .expect("failed to run LocalConvert Desktop");
}
