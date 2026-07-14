use crate::{
    image_convert::{
        self, ImageCompressExecutionRequest, ImageCompressResult, ImageConvertExecutionRequest,
        ImageConvertResult, ImageResizeExecutionRequest, ImageResizeResult,
    },
    qpdf::{
        self, QpdfExtractPagesRequest, QpdfExtractResult, QpdfMergeRequest, QpdfMergeResult,
        QpdfRotatePagesRequest, QpdfRotateResult, QpdfSplitRequest, QpdfSplitResult,
    },
    task_registry::{BackendTaskRegistry, BackendTaskStatus, CancelTaskResponse, TaskControl},
};
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendTaskResponse<T> {
    task_id: String,
    operation: &'static str,
    status: &'static str,
    result: T,
}

trait TaskExecutionResult {
    fn succeeded(&self) -> bool;
    fn mark_cancelled(&mut self);
}

macro_rules! impl_task_execution_result {
    ($result:ty) => {
        impl TaskExecutionResult for $result {
            fn succeeded(&self) -> bool {
                <$result>::succeeded(self)
            }

            fn mark_cancelled(&mut self) {
                <$result>::mark_cancelled(self);
            }
        }
    };
}

impl_task_execution_result!(QpdfMergeResult);
impl_task_execution_result!(QpdfSplitResult);
impl_task_execution_result!(QpdfExtractResult);
impl_task_execution_result!(QpdfRotateResult);
impl_task_execution_result!(ImageConvertResult);
impl_task_execution_result!(ImageResizeResult);
impl_task_execution_result!(ImageCompressResult);

async fn run_backend_task<T, F>(
    app: tauri::AppHandle,
    task_id: String,
    operation: &'static str,
    work: F,
) -> Result<BackendTaskResponse<T>, String>
where
    T: TaskExecutionResult + Send + 'static,
    F: FnOnce(TaskControl) -> T + Send + 'static,
{
    let registry = app.state::<BackendTaskRegistry>().inner().clone();
    let control = registry.register(&task_id, operation)?;
    let task_id = control.task_id().to_string();
    let worker_control = control.clone();

    let worker_result = tauri::async_runtime::spawn_blocking(move || {
        let _ = worker_control.mark_running();
        work(worker_control)
    })
    .await;

    let mut result = match worker_result {
        Ok(result) => result,
        Err(error) => {
            let _ = registry.cancel(&task_id);
            return Err(format!("Backend task worker failed: {error}"));
        }
    };

    let status = registry.finish(&task_id, result.succeeded())?;
    if status == BackendTaskStatus::Cancelled {
        result.mark_cancelled();
    }

    Ok(BackendTaskResponse {
        task_id,
        operation,
        status: status.as_str(),
        result,
    })
}

#[tauri::command]
pub(crate) async fn qpdf_merge_pdfs(
    app: tauri::AppHandle,
    task_id: String,
    request: QpdfMergeRequest,
) -> Result<BackendTaskResponse<QpdfMergeResult>, String> {
    run_backend_task(app, task_id, "qpdf-merge", move |control| {
        qpdf::qpdf_merge_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn qpdf_split_pdf(
    app: tauri::AppHandle,
    task_id: String,
    request: QpdfSplitRequest,
) -> Result<BackendTaskResponse<QpdfSplitResult>, String> {
    run_backend_task(app, task_id, "qpdf-split", move |control| {
        qpdf::qpdf_split_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn qpdf_extract_pages(
    app: tauri::AppHandle,
    task_id: String,
    request: QpdfExtractPagesRequest,
) -> Result<BackendTaskResponse<QpdfExtractResult>, String> {
    run_backend_task(app, task_id, "qpdf-extract", move |control| {
        qpdf::qpdf_extract_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn qpdf_rotate_pages(
    app: tauri::AppHandle,
    task_id: String,
    request: QpdfRotatePagesRequest,
) -> Result<BackendTaskResponse<QpdfRotateResult>, String> {
    run_backend_task(app, task_id, "qpdf-rotate", move |control| {
        qpdf::qpdf_rotate_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn image_convert_file(
    app: tauri::AppHandle,
    task_id: String,
    request: ImageConvertExecutionRequest,
) -> Result<BackendTaskResponse<ImageConvertResult>, String> {
    run_backend_task(app, task_id, "image-convert", move |control| {
        image_convert::image_convert_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn image_resize_file(
    app: tauri::AppHandle,
    task_id: String,
    request: ImageResizeExecutionRequest,
) -> Result<BackendTaskResponse<ImageResizeResult>, String> {
    run_backend_task(app, task_id, "image-resize", move |control| {
        image_convert::image_resize_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn image_compress_file(
    app: tauri::AppHandle,
    task_id: String,
    request: ImageCompressExecutionRequest,
) -> Result<BackendTaskResponse<ImageCompressResult>, String> {
    run_backend_task(app, task_id, "image-compress", move |control| {
        image_convert::image_compress_task(request, control)
    })
    .await
}

#[tauri::command]
pub(crate) async fn cancel_task(
    app: tauri::AppHandle,
    task_id: String,
) -> Result<CancelTaskResponse, String> {
    let registry = app.state::<BackendTaskRegistry>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.cancel(&task_id))
        .await
        .map_err(|error| format!("Backend cancellation worker failed: {error}"))?
}
