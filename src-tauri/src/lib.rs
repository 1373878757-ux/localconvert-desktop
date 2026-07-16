use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

mod backend_tasks;
mod image_convert;
mod image_engine;
mod image_ops;
mod native_intake;
mod output_finalize;
mod output_planning;
mod preferences;
mod qpdf;
mod task_registry;
mod task_report;
mod task_usability;
mod timed_process;

const MIN_SPLASH_DISPLAY_TIME: Duration = Duration::from_millis(1800);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineSelfCheck {
    platform: String,
    full_edition: bool,
    conversion_enabled: bool,
    engines: Vec<EngineStatus>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineStatus {
    name: &'static str,
    status: &'static str,
    required_for_v1: bool,
    message: String,
}

#[derive(Clone, Default)]
struct StartupState {
    status: Arc<Mutex<StartupStatus>>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupStatus {
    completed: bool,
    self_check: Option<EngineSelfCheck>,
    error: Option<String>,
}

impl StartupState {
    fn snapshot(&self) -> StartupStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn complete(&self, self_check: Option<EngineSelfCheck>, error: Option<String>) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        status.completed = true;
        status.self_check = self_check;
        status.error = error;
    }

    fn complete_with_self_check(&self, self_check: EngineSelfCheck) {
        let error = startup_engine_error_summary(&self_check);
        self.complete(Some(self_check), error);
    }

    fn record_error(&self, message: String) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        status.error = Some(match status.error.take() {
            Some(existing) => format!("{existing}\n{message}"),
            None => message,
        });
    }
}

fn startup_engine_error_summary(self_check: &EngineSelfCheck) -> Option<String> {
    let errors = self_check
        .engines
        .iter()
        .filter(|engine| engine.status == "error")
        .map(|engine| format!("{}: {}", engine.name, engine.message))
        .collect::<Vec<_>>();

    (!errors.is_empty()).then(|| errors.join("\n"))
}

fn complete_startup_check_result(
    startup_state: &StartupState,
    self_check_result: thread::Result<EngineSelfCheck>,
) {
    match self_check_result {
        Ok(self_check) => startup_state.complete_with_self_check(self_check),
        Err(_) => startup_state.complete(
            None,
            Some("Startup initialization failed during engine self-check.".to_string()),
        ),
    }
}

#[tauri::command]
fn engine_self_check() -> EngineSelfCheck {
    build_engine_self_check()
}

#[tauri::command]
fn startup_status(startup_state: tauri::State<'_, StartupState>) -> StartupStatus {
    startup_state.snapshot()
}

fn build_engine_self_check() -> EngineSelfCheck {
    let platform = current_platform_key();
    let qpdf_status = qpdf::detect_qpdf_engine(&platform);
    let image_engine_status = image_engine::detect_image_engine(&platform);

    EngineSelfCheck {
        platform,
        full_edition: true,
        conversion_enabled: false,
        engines: vec![
            EngineStatus {
                name: "LibreOffice headless",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.".to_string(),
            },
            EngineStatus {
                name: "qpdf",
                status: qpdf_status.status,
                required_for_v1: true,
                message: qpdf_status.message,
            },
            EngineStatus {
                name: "PDFium",
                status: "not-installed",
                required_for_v1: true,
                message: "Not bundled yet.".to_string(),
            },
            EngineStatus {
                name: "image-engine",
                status: image_engine_status.status,
                required_for_v1: true,
                message: image_engine_status.message,
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(StartupState::default())
        .manage(task_registry::BackendTaskRegistry::default())
        .setup(|app| {
            let startup_state = app.state::<StartupState>().inner().clone();
            let app_handle = app.handle().clone();

            thread::spawn(move || {
                run_startup_sequence(startup_state, app_handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_self_check,
            startup_status,
            output_planning::plan_output_path,
            preferences::load_preferences,
            preferences::save_preferences,
            preferences::reset_preferences,
            native_intake::inspect_native_paths,
            backend_tasks::image_convert_file,
            backend_tasks::image_resize_file,
            backend_tasks::image_compress_file,
            backend_tasks::image_clean_metadata_file,
            image_ops::plan_image_convert,
            image_ops::plan_image_compress,
            image_ops::plan_image_resize,
            image_ops::plan_image_remove_metadata,
            backend_tasks::qpdf_merge_pdfs,
            backend_tasks::qpdf_split_pdf,
            backend_tasks::qpdf_extract_pages,
            backend_tasks::qpdf_rotate_pages,
            backend_tasks::cancel_task,
            task_report::export_task_report,
            task_usability::reveal_local_file,
            task_usability::copy_error_summary,
            task_usability::copy_failed_task_summary
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LocalConvert Desktop");
}

fn run_startup_sequence(startup_state: StartupState, app_handle: tauri::AppHandle) {
    let started_at = Instant::now();
    let self_check_result = std::panic::catch_unwind(build_engine_self_check);
    complete_startup_check_result(&startup_state, self_check_result);

    if let Some(remaining) = MIN_SPLASH_DISPLAY_TIME.checked_sub(started_at.elapsed()) {
        thread::sleep(remaining);
    }

    let startup_state_for_ui = startup_state.clone();
    let app_handle_for_ui = app_handle.clone();
    if let Err(error) = app_handle.run_on_main_thread(move || {
        match app_handle_for_ui.get_webview_window("main") {
            Some(window) => {
                if let Err(error) = window.show() {
                    startup_state_for_ui
                        .record_error(format!("Unable to show main window: {error}"));
                }
                if let Err(error) = window.set_focus() {
                    startup_state_for_ui
                        .record_error(format!("Unable to focus main window: {error}"));
                }
            }
            None => startup_state_for_ui
                .record_error("Main window was not available at startup.".to_string()),
        };

        if let Some(window) = app_handle_for_ui.get_webview_window("splashscreen") {
            if let Err(error) = window.close() {
                startup_state_for_ui
                    .record_error(format!("Unable to close splash screen: {error}"));
            }
        }
    }) {
        startup_state.record_error(format!(
            "Unable to schedule startup window handoff: {error}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_self_check_keeps_qpdf_status_dynamic_when_image_detection_runs() {
        let self_check = build_engine_self_check();
        let qpdf_engine = self_check
            .engines
            .iter()
            .find(|engine| engine.name == "qpdf")
            .expect("qpdf engine status should be present");
        let expected_qpdf_status = qpdf::detect_qpdf_engine(&self_check.platform);

        assert_eq!(qpdf_engine.status, expected_qpdf_status.status);
        assert_eq!(qpdf_engine.message, expected_qpdf_status.message);

        let image_engine = self_check
            .engines
            .iter()
            .find(|engine| engine.name == "image-engine")
            .expect("image-engine status should be present");
        assert!(image_engine.required_for_v1);
    }

    #[test]
    fn startup_state_stores_engine_timeout_error() {
        let startup_state = StartupState::default();
        let timeout_message = "qpdf startup smoke check timed out after 3 seconds";
        let self_check = EngineSelfCheck {
            platform: "macos-aarch64".to_string(),
            full_edition: true,
            conversion_enabled: false,
            engines: vec![EngineStatus {
                name: "qpdf",
                status: "error",
                required_for_v1: true,
                message: timeout_message.to_string(),
            }],
        };

        complete_startup_check_result(&startup_state, Ok(self_check));
        let snapshot = startup_state.snapshot();

        assert!(snapshot.completed);
        assert!(snapshot
            .error
            .as_deref()
            .is_some_and(|error| error.contains(timeout_message)));
        let stored_check = snapshot
            .self_check
            .expect("timed-out engine result should remain available to the UI");
        assert_eq!(stored_check.engines[0].status, "error");
        assert_eq!(stored_check.engines[0].message, timeout_message);
    }

    #[test]
    fn startup_state_completes_after_panicking_engine_check_result() {
        let startup_state = StartupState::default();
        let panic_result: thread::Result<EngineSelfCheck> =
            Err(Box::new("engine self-check panic"));

        complete_startup_check_result(&startup_state, panic_result);
        let snapshot = startup_state.snapshot();

        assert!(snapshot.completed);
        assert!(snapshot.self_check.is_none());
        assert_eq!(
            snapshot.error.as_deref(),
            Some("Startup initialization failed during engine self-check.")
        );
    }
}
