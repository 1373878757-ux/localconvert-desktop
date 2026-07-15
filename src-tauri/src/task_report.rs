use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReportFormat {
    Csv,
    Json,
}

impl ReportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    fn as_str(self) -> &'static str {
        self.extension()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TaskReportStatus {
    Success,
    Failed,
    Cancelled,
    Skipped,
    NotSmaller,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskReportRecord {
    task_id: String,
    operation_type: String,
    source_path: String,
    source_name: String,
    source_extension: String,
    output_path: String,
    output_name: String,
    output_extension: String,
    status: TaskReportStatus,
    started_at: Option<String>,
    finished_at: Option<String>,
    duration_ms: Option<u64>,
    source_bytes: Option<u64>,
    output_bytes: Option<u64>,
    saved_bytes: Option<u64>,
    saved_percent: Option<f64>,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExportTaskReportRequest {
    destination_path: String,
    format: ReportFormat,
    app_version: String,
    generated_at: String,
    tasks: Vec<TaskReportRecord>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportTaskReportResult {
    success: bool,
    destination_path: String,
    format: &'static str,
    bytes_written: u64,
    task_count: usize,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonTaskReport<'a> {
    app_version: &'a str,
    generated_at: &'a str,
    tasks: &'a [TaskReportRecord],
}

const CSV_HEADERS: [&str; 19] = [
    "appVersion",
    "reportGeneratedAt",
    "taskId",
    "operationType",
    "sourcePath",
    "sourceName",
    "sourceExtension",
    "outputPath",
    "outputName",
    "outputExtension",
    "status",
    "startedAt",
    "finishedAt",
    "durationMs",
    "sourceBytes",
    "outputBytes",
    "savedBytes",
    "savedPercent",
    "message",
];

#[tauri::command]
pub(crate) fn export_task_report(
    request: ExportTaskReportRequest,
) -> Result<ExportTaskReportResult, String> {
    validate_request(&request)?;
    let contents = serialize_report(&request)?;
    let destination = Path::new(request.destination_path.trim());

    fs::write(destination, contents.as_bytes())
        .map_err(|error| report_write_error(destination, error))?;
    let bytes_written = fs::metadata(destination)
        .map_err(|error| report_write_error(destination, error))?
        .len();

    Ok(ExportTaskReportResult {
        success: true,
        destination_path: destination.to_string_lossy().into_owned(),
        format: request.format.as_str(),
        bytes_written,
        task_count: request.tasks.len(),
        message: "报告已导出",
    })
}

fn validate_request(request: &ExportTaskReportRequest) -> Result<(), String> {
    if request.destination_path.trim().is_empty() {
        return Err("未选择报告保存位置。".to_string());
    }
    if request.app_version.trim().is_empty() {
        return Err("报告缺少应用版本。".to_string());
    }
    if request.generated_at.trim().is_empty() {
        return Err("报告缺少生成时间。".to_string());
    }
    if request.tasks.is_empty() {
        return Err("没有可导出的任务结果。".to_string());
    }

    let destination = Path::new(request.destination_path.trim());
    let actual_extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !actual_extension.eq_ignore_ascii_case(request.format.extension()) {
        return Err(format!(
            "报告文件扩展名必须是 .{}。",
            request.format.extension()
        ));
    }
    if destination.is_dir() {
        return Err("所选报告保存位置是文件夹，请选择文件名。".to_string());
    }

    for task in &request.tasks {
        if task.task_id.trim().is_empty() {
            return Err("报告任务缺少 taskId。".to_string());
        }
        if task.operation_type.trim().is_empty() {
            return Err(format!("任务 {} 缺少 operationType。", task.task_id));
        }
        if task.source_name.trim().is_empty() {
            return Err(format!("任务 {} 缺少 sourceName。", task.task_id));
        }
        if let Some(saved_percent) = task.saved_percent {
            if !saved_percent.is_finite() || !(0.0..=100.0).contains(&saved_percent) {
                return Err(format!(
                    "任务 {} 的 savedPercent 必须在 0 到 100 之间。",
                    task.task_id
                ));
            }
        }
    }

    Ok(())
}

fn serialize_report(request: &ExportTaskReportRequest) -> Result<String, String> {
    match request.format {
        ReportFormat::Json => serde_json::to_string_pretty(&JsonTaskReport {
            app_version: &request.app_version,
            generated_at: &request.generated_at,
            tasks: &request.tasks,
        })
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("JSON 报告生成失败：{error}")),
        ReportFormat::Csv => Ok(serialize_csv_report(request)),
    }
}

fn serialize_csv_report(request: &ExportTaskReportRequest) -> String {
    let mut rows = Vec::with_capacity(request.tasks.len() + 1);
    rows.push(CSV_HEADERS.join(","));

    for task in &request.tasks {
        let values = [
            request.app_version.clone(),
            request.generated_at.clone(),
            task.task_id.clone(),
            task.operation_type.clone(),
            task.source_path.clone(),
            task.source_name.clone(),
            task.source_extension.clone(),
            task.output_path.clone(),
            task.output_name.clone(),
            task.output_extension.clone(),
            task_report_status_name(task.status).to_string(),
            task.started_at.clone().unwrap_or_default(),
            task.finished_at.clone().unwrap_or_default(),
            optional_integer(task.duration_ms),
            optional_integer(task.source_bytes),
            optional_integer(task.output_bytes),
            optional_integer(task.saved_bytes),
            task.saved_percent
                .map(|percent| format!("{percent:.2}"))
                .unwrap_or_default(),
            task.message.clone(),
        ];
        rows.push(
            values
                .iter()
                .map(|value| escape_csv_field(value))
                .collect::<Vec<_>>()
                .join(","),
        );
    }

    format!("{}\r\n", rows.join("\r\n"))
}

fn task_report_status_name(status: TaskReportStatus) -> &'static str {
    match status {
        TaskReportStatus::Success => "success",
        TaskReportStatus::Failed => "failed",
        TaskReportStatus::Cancelled => "cancelled",
        TaskReportStatus::Skipped => "skipped",
        TaskReportStatus::NotSmaller => "not_smaller",
        TaskReportStatus::Unsupported => "unsupported",
    }
}

fn optional_integer<T: ToString>(value: Option<T>) -> String {
    value.map(|number| number.to_string()).unwrap_or_default()
}

fn escape_csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn report_write_error(destination: &Path, error: io::Error) -> String {
    if error.kind() == ErrorKind::PermissionDenied {
        return "没有权限写入所选位置。请选择其他文件夹后重试。".to_string();
    }

    format!("报告写入失败（{}）：{error}", destination.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(status: TaskReportStatus, task_id: &str) -> TaskReportRecord {
        TaskReportRecord {
            task_id: task_id.to_string(),
            operation_type: "image_compress".to_string(),
            source_path: "/Users/test/客户 文件.jpg".to_string(),
            source_name: "客户 文件.jpg".to_string(),
            source_extension: "jpg".to_string(),
            output_path: "/Users/test/converted/客户 文件 compressed.jpg".to_string(),
            output_name: "客户 文件 compressed.jpg".to_string(),
            output_extension: "jpg".to_string(),
            status,
            started_at: Some("2026-07-15T12:00:00.000Z".to_string()),
            finished_at: Some("2026-07-15T12:00:01.250Z".to_string()),
            duration_ms: Some(1_250),
            source_bytes: Some(2_000),
            output_bytes: Some(1_490),
            saved_bytes: Some(510),
            saved_percent: Some(25.5),
            message: "处理完成".to_string(),
        }
    }

    fn request(format: ReportFormat, tasks: Vec<TaskReportRecord>) -> ExportTaskReportRequest {
        ExportTaskReportRequest {
            destination_path: format!("/tmp/localconvert-report.{}", format.extension()),
            format,
            app_version: "0.6.0".to_string(),
            generated_at: "2026-07-15T12:00:02.000Z".to_string(),
            tasks,
        }
    }

    #[test]
    fn serializes_pretty_json_with_success_failure_and_cancelled_tasks() {
        let report = request(
            ReportFormat::Json,
            vec![
                task(TaskReportStatus::Success, "success-1"),
                task(TaskReportStatus::Failed, "failed-1"),
                task(TaskReportStatus::Cancelled, "cancelled-1"),
            ],
        );

        let output = serialize_report(&report).expect("JSON report should serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("JSON report should parse");

        assert_eq!(parsed["appVersion"], "0.6.0");
        assert_eq!(parsed["generatedAt"], "2026-07-15T12:00:02.000Z");
        assert_eq!(parsed["tasks"][0]["status"], "success");
        assert_eq!(parsed["tasks"][1]["status"], "failed");
        assert_eq!(parsed["tasks"][2]["status"], "cancelled");
        assert!(output.contains("\n  \"tasks\": ["));
    }

    #[test]
    fn csv_escapes_commas_quotes_and_line_breaks() {
        let mut record = task(TaskReportStatus::Failed, "csv-1");
        record.source_name = "report, \"final\".jpg".to_string();
        record.message = "first line\r\nsecond, \"quoted\" line".to_string();
        let report = request(ReportFormat::Csv, vec![record]);

        let output = serialize_report(&report).expect("CSV report should serialize");

        assert!(output.starts_with("appVersion,reportGeneratedAt,taskId"));
        assert!(output.contains("\"report, \"\"final\"\".jpg\""));
        assert!(output.contains("\"first line\r\nsecond, \"\"quoted\"\" line\""));
        assert!(output.ends_with("\r\n"));
    }

    #[test]
    fn csv_formats_savings_deterministically() {
        let output = serialize_report(&request(
            ReportFormat::Csv,
            vec![task(TaskReportStatus::Success, "saving-1")],
        ))
        .expect("CSV report should serialize");

        assert!(output.contains(",2000,1490,510,25.50,处理完成\r\n"));
    }

    #[test]
    fn request_schema_rejects_raw_metadata_payloads() {
        let value = json!({
            "destinationPath": "/tmp/localconvert-report.json",
            "format": "json",
            "appVersion": "0.6.0",
            "generatedAt": "2026-07-15T12:00:02.000Z",
            "tasks": [{
                "taskId": "privacy-1",
                "operationType": "image_metadata_cleanup",
                "sourcePath": "/Users/test/photo.jpg",
                "sourceName": "photo.jpg",
                "sourceExtension": "jpg",
                "outputPath": "",
                "outputName": "",
                "outputExtension": "",
                "status": "skipped",
                "startedAt": null,
                "finishedAt": null,
                "durationMs": null,
                "sourceBytes": 2000,
                "outputBytes": null,
                "savedBytes": null,
                "savedPercent": null,
                "message": "未发现可清理的元数据",
                "fileContents": "source-file-bytes-must-never-be-exported",
                "gps": "31.2304,121.4737",
                "rawExif": "secret-camera-serial",
                "xmpPayload": "private-xmp-value",
                "iptcPayload": "private-iptc-value"
            }]
        });

        let error = serde_json::from_value::<ExportTaskReportRequest>(value)
            .expect_err("unknown raw metadata fields must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn report_status_names_remain_stable() {
        let statuses = [
            (TaskReportStatus::Success, "success"),
            (TaskReportStatus::Failed, "failed"),
            (TaskReportStatus::Cancelled, "cancelled"),
            (TaskReportStatus::Skipped, "skipped"),
            (TaskReportStatus::NotSmaller, "not_smaller"),
            (TaskReportStatus::Unsupported, "unsupported"),
        ];

        for (status, expected) in statuses {
            assert_eq!(task_report_status_name(status), expected);
        }
    }
}
