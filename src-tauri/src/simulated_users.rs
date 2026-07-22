use crate::{
    image_convert::{
        self, ImageCleanMetadataExecutionRequest, ImageCompressExecutionRequest,
        ImageConvertExecutionRequest, ImageResizeExecutionRequest,
    },
    output_planning::{
        self, OutputNamingRuleRequest, OutputPathPlanRequest, DEFAULT_OUTPUT_STRATEGY,
    },
    preferences::{
        self, ImageTargetFormat, OutputLocationMode, OutputSuffixPreset, ReportFormat, ResizeMode,
        ToolSection, UserPreferences,
    },
    qpdf::{
        self, QpdfExtractPagesRequest, QpdfMergeRequest, QpdfRotatePagesRequest, QpdfSplitRequest,
    },
    task_registry::{BackendTaskRegistry, BackendTaskStatus, TaskControl},
    task_report::{self, ExportTaskReportRequest},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    ffi::OsStr,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const QUICK_SEED: u64 = 0x4c43_4430_3930_0001;
const STANDARD_SEED: u64 = 0x4c43_4430_3930_0002;
const STRESS_SEED: u64 = 0x4c43_4430_3930_0003;
const SOAK_SEED: u64 = 0x4c43_4430_3930_0004;
const MIN_STANDARD_FREE_KIB: u64 = 12 * 1024 * 1024;
const MIN_STRESS_FREE_KIB: u64 = 20 * 1024 * 1024;
const STOP_FREE_KIB: u64 = 10 * 1024 * 1024;
const HARD_MAX_DATA_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const MAX_FAILURES_IN_REPORT: usize = 50;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Profile {
    Quick,
    Standard,
    Stress,
    Soak,
}

impl Profile {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "quick" => Ok(Self::Quick),
            "standard" => Ok(Self::Standard),
            "stress" => Ok(Self::Stress),
            "soak" => Ok(Self::Soak),
            _ => Err(format!("Unsupported simulated-user profile: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Stress => "stress",
            Self::Soak => "soak",
        }
    }
}

#[derive(Clone, Debug)]
struct CampaignConfig {
    profile: Profile,
    seed: u64,
    sessions: usize,
    attempts: usize,
    concurrency: Vec<usize>,
    task_timeout: Duration,
    scenario_timeout: Duration,
    max_data_bytes: u64,
    duration: Duration,
    dry_run: bool,
    retain_on_failure: bool,
    allow_stress: bool,
    allow_soak: bool,
    result_path: PathBuf,
}

impl CampaignConfig {
    fn defaults(profile: Profile) -> Self {
        let (seed, sessions, attempts, concurrency, duration) = match profile {
            Profile::Quick => (QUICK_SEED, 20, 300, vec![1, 2, 4], Duration::ZERO),
            Profile::Standard => (STANDARD_SEED, 100, 4_000, vec![1, 2, 4, 8], Duration::ZERO),
            Profile::Stress => (STRESS_SEED, 400, 20_000, vec![4, 8], Duration::ZERO),
            Profile::Soak => (
                SOAK_SEED,
                20,
                300,
                vec![1, 2, 4],
                Duration::from_secs(120 * 60),
            ),
        };
        Self {
            profile,
            seed,
            sessions,
            attempts,
            concurrency,
            task_timeout: Duration::from_secs(130),
            scenario_timeout: Duration::from_secs(900),
            max_data_bytes: 1024 * 1024 * 1024,
            duration,
            dry_run: false,
            retain_on_failure: true,
            allow_stress: false,
            allow_soak: false,
            result_path: PathBuf::from("simulated-user-result.json"),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Persona {
    Beginner,
    BatchImage,
    Pdf,
    PrivacyFocused,
    Careless,
    ExternalDriveLike,
    UnicodePath,
    Power,
    PermissionFailure,
    LongSession,
}

impl Persona {
    const ALL: [Self; 10] = [
        Self::Beginner,
        Self::BatchImage,
        Self::Pdf,
        Self::PrivacyFocused,
        Self::Careless,
        Self::ExternalDriveLike,
        Self::UnicodePath,
        Self::Power,
        Self::PermissionFailure,
        Self::LongSession,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Self::Beginner => "beginner",
            Self::BatchImage => "batch-image",
            Self::Pdf => "pdf",
            Self::PrivacyFocused => "privacy-focused",
            Self::Careless => "careless",
            Self::ExternalDriveLike => "external-drive-like",
            Self::UnicodePath => "unicode-path",
            Self::Power => "power",
            Self::PermissionFailure => "permission-failure",
            Self::LongSession => "long-session",
        }
    }
}

#[derive(Clone, Debug)]
enum OperationPlan {
    ImageConvert {
        source: PathBuf,
        target: &'static str,
    },
    ImageResize {
        source: PathBuf,
        mode: &'static str,
        max_width: Option<u32>,
        max_height: Option<u32>,
    },
    ImageCompress {
        source: PathBuf,
        quality: Option<u8>,
    },
    ImageCleanMetadata {
        source: PathBuf,
    },
    PdfMerge {
        sources: Vec<PathBuf>,
    },
    PdfSplit {
        source: PathBuf,
    },
    PdfExtract {
        source: PathBuf,
        pages: &'static str,
    },
    PdfRotate {
        source: PathBuf,
        degrees: i16,
    },
}

impl OperationPlan {
    fn name(&self) -> &'static str {
        match self {
            Self::ImageConvert { .. } => "image-convert",
            Self::ImageResize { .. } => "image-resize",
            Self::ImageCompress { .. } => "image-compress",
            Self::ImageCleanMetadata { .. } => "image-clean-metadata",
            Self::PdfMerge { .. } => "pdf-merge",
            Self::PdfSplit { .. } => "pdf-split",
            Self::PdfExtract { .. } => "pdf-extract",
            Self::PdfRotate { .. } => "pdf-rotate",
        }
    }

    fn sources(&self) -> Vec<PathBuf> {
        match self {
            Self::ImageConvert { source, .. }
            | Self::ImageResize { source, .. }
            | Self::ImageCompress { source, .. }
            | Self::ImageCleanMetadata { source }
            | Self::PdfSplit { source }
            | Self::PdfExtract { source, .. }
            | Self::PdfRotate { source, .. } => vec![source.clone()],
            Self::PdfMerge { sources } => sources.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureMode {
    None,
    MissingSource,
    MalformedSource,
    UnsupportedSource,
    ReadOnlyOutput,
    CancelBeforeStart,
    CancelDuringExecution,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AttemptOutcome {
    task_id: String,
    operation: String,
    status: String,
    duration_ms: u64,
    output_paths: Vec<String>,
    message: String,
    collision: bool,
    retry: bool,
    source_hash_changed: bool,
    overwrite_violation: bool,
    temp_leak: bool,
    timeout: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResult {
    session_id: usize,
    persona: String,
    seed: String,
    attempts: usize,
    operation_distribution: BTreeMap<String, usize>,
    status_distribution: BTreeMap<String, usize>,
    durations_ms: Vec<u64>,
    retries: usize,
    cancellations: usize,
    collisions: usize,
    report_exports: usize,
    source_hash_changes: usize,
    overwrite_violations: usize,
    temporary_leaks: usize,
    timeouts: usize,
    failures: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceObservations {
    method: String,
    rss_start_kib: Option<u64>,
    rss_end_kib: Option<u64>,
    peak_rss_kib: Option<u64>,
    peak_cpu_percent: Option<f64>,
    peak_open_file_descriptors: Option<usize>,
    peak_child_processes: Option<usize>,
    peak_temporary_bytes: u64,
    minimum_free_disk_kib: Option<u64>,
    minimum_memory_free_percent: Option<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CampaignResult {
    schema_version: u8,
    evidence_type: &'static str,
    profile: String,
    seed: String,
    git_commit: String,
    app_version: &'static str,
    started_at: String,
    finished_at: String,
    macos_version: String,
    architecture: String,
    virtual_sessions: usize,
    task_attempts: usize,
    concurrency: Vec<usize>,
    persona_distribution: BTreeMap<String, usize>,
    operation_distribution: BTreeMap<String, usize>,
    status_distribution: BTreeMap<String, usize>,
    retry_count: usize,
    cancellation_count: usize,
    collision_count: usize,
    report_exports: usize,
    duration_ms: u64,
    throughput_tasks_per_second: f64,
    average_task_duration_ms: f64,
    p50_task_duration_ms: u64,
    p95_task_duration_ms: u64,
    p99_task_duration_ms: u64,
    source_hash_changes: usize,
    output_overwrite_violations: usize,
    temporary_directory_leaks: usize,
    crashes: usize,
    hangs: usize,
    timeouts: usize,
    invariant_violations: Vec<String>,
    failures: Vec<String>,
    resource_observations: ResourceObservations,
    network_observation: &'static str,
    unresolved_limitations: Vec<String>,
    corpus_cleanup: String,
    rerun_command: String,
    verdict: &'static str,
}

#[derive(Clone)]
struct SessionPlan {
    session_id: usize,
    attempts: usize,
    seed: u64,
    persona: Persona,
}

#[derive(Clone)]
struct SessionFixtures {
    root: PathBuf,
    png: Vec<PathBuf>,
    jpeg: Vec<PathBuf>,
    webp: Vec<PathBuf>,
    metadata_jpeg: PathBuf,
    pdf_one: PathBuf,
    pdf_two: PathBuf,
    pdf_multi: PathBuf,
    malformed_jpeg: PathBuf,
    unsupported_heic: PathBuf,
    source_hashes: BTreeMap<PathBuf, String>,
}

#[derive(Debug)]
struct TaskExecution {
    value: Value,
    status: BackendTaskStatus,
    duration: Duration,
    timed_out: bool,
}

#[derive(Clone)]
struct PreparedOperation {
    plan: OperationPlan,
    output_path: PathBuf,
    split_output_directory: Option<PathBuf>,
    split_prefix: Option<String>,
    collision_marker: Option<(PathBuf, Vec<u8>)>,
}

struct AttemptRequest {
    task_id: String,
    operation: OperationPlan,
    failure_mode: FailureMode,
    collision: bool,
    retry: bool,
    timeout: Duration,
    attempt_index: usize,
}

pub fn run_cli(arguments: impl IntoIterator<Item = String>) -> i32 {
    match parse_arguments(arguments).and_then(run_campaign) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("simulated-user campaign failed: {message}");
            1
        }
    }
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<CampaignConfig, String> {
    let args = arguments.into_iter().collect::<Vec<_>>();
    let profile_index = args
        .iter()
        .position(|arg| arg == "--profile")
        .ok_or_else(|| "--profile is required.".to_string())?;
    let profile_value = args
        .get(profile_index + 1)
        .ok_or_else(|| "--profile requires a value.".to_string())?;
    let mut config = CampaignConfig::defaults(Profile::parse(profile_value)?);

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => index += 2,
            "--seed" => {
                config.seed = parse_seed(required_value(&args, index)?)?;
                index += 2;
            }
            "--sessions" => {
                config.sessions = parse_positive_usize(required_value(&args, index)?, "sessions")?;
                index += 2;
            }
            "--attempts" => {
                config.attempts = parse_positive_usize(required_value(&args, index)?, "attempts")?;
                index += 2;
            }
            "--concurrency" => {
                config.concurrency = parse_concurrency(required_value(&args, index)?)?;
                index += 2;
            }
            "--task-timeout" => {
                config.task_timeout = Duration::from_secs(parse_positive_u64(
                    required_value(&args, index)?,
                    "task-timeout",
                )?);
                index += 2;
            }
            "--scenario-timeout" => {
                config.scenario_timeout = Duration::from_secs(parse_positive_u64(
                    required_value(&args, index)?,
                    "scenario-timeout",
                )?);
                index += 2;
            }
            "--max-data-gb" => {
                let gib = parse_positive_u64(required_value(&args, index)?, "max-data-gb")?;
                config.max_data_bytes = gib
                    .checked_mul(1024 * 1024 * 1024)
                    .ok_or_else(|| "max-data-gb is too large.".to_string())?;
                index += 2;
            }
            "--duration-minutes" => {
                let minutes =
                    parse_positive_u64(required_value(&args, index)?, "duration-minutes")?;
                config.duration = Duration::from_secs(minutes.saturating_mul(60));
                index += 2;
            }
            "--result-path" => {
                config.result_path = PathBuf::from(required_value(&args, index)?);
                index += 2;
            }
            "--dry-run" => {
                config.dry_run = true;
                index += 1;
            }
            "--allow-stress" => {
                config.allow_stress = true;
                index += 1;
            }
            "--allow-soak" => {
                config.allow_soak = true;
                index += 1;
            }
            "--no-retain-on-failure" => {
                config.retain_on_failure = false;
                index += 1;
            }
            unknown => return Err(format!("Unknown simulated-user argument: {unknown}")),
        }
    }

    validate_config(&config)?;
    Ok(config)
}

fn required_value(args: &[String], index: usize) -> Result<&str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{} requires a value.", args[index]))
}

fn parse_seed(value: &str) -> Result<u64, String> {
    value
        .strip_prefix("0x")
        .map(|hex| u64::from_str_radix(hex, 16))
        .unwrap_or_else(|| value.parse::<u64>())
        .map_err(|_| format!("Invalid deterministic seed: {value}"))
}

fn parse_positive_usize(value: &str, label: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{label} must be a positive integer."))
}

fn parse_positive_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{label} must be a positive integer."))
}

fn parse_concurrency(value: &str) -> Result<Vec<usize>, String> {
    let mut values = value
        .split(',')
        .map(|item| parse_positive_usize(item.trim(), "concurrency"))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort_unstable();
    values.dedup();
    if values.is_empty() || values.iter().any(|value| *value > 8) {
        return Err("concurrency must contain values from 1 through 8.".to_string());
    }
    Ok(values)
}

fn validate_config(config: &CampaignConfig) -> Result<(), String> {
    if config.max_data_bytes > HARD_MAX_DATA_BYTES {
        return Err("max-data-gb cannot exceed the 10 GiB safety ceiling.".to_string());
    }
    if config.profile == Profile::Stress && !config.allow_stress {
        return Err("stress requires explicit --allow-stress authorization.".to_string());
    }
    if config.profile == Profile::Soak && !config.allow_soak {
        return Err("soak requires explicit --allow-soak authorization.".to_string());
    }
    Ok(())
}

fn run_campaign(config: CampaignConfig) -> Result<(), String> {
    let free_disk_kib = free_disk_kib(std::env::temp_dir().as_path())?;
    let required_kib = if matches!(config.profile, Profile::Stress | Profile::Soak) {
        MIN_STRESS_FREE_KIB
    } else {
        MIN_STANDARD_FREE_KIB
    };
    if free_disk_kib < required_kib {
        return Err(format!(
            "{} requires at least {:.1} GiB free disk space; only {:.1} GiB is available.",
            config.profile.as_str(),
            required_kib as f64 / 1024.0 / 1024.0,
            free_disk_kib as f64 / 1024.0 / 1024.0
        ));
    }

    let rerun_command = rerun_command(&config);
    if config.dry_run {
        println!(
            "dry-run: profile={} sessions={} attempts={} concurrency={:?} seed=0x{:016x}",
            config.profile.as_str(),
            config.sessions,
            config.attempts,
            config.concurrency,
            config.seed
        );
        println!("rerun: {rerun_command}");
        return Ok(());
    }

    let started_at = utc_timestamp();
    let started = Instant::now();
    let root = allocate_simulation_root(config.profile, config.seed)?;
    let monitor = ResourceMonitor::start(root.clone());
    let session_plans = build_session_plans(&config);
    let mut session_results = Vec::with_capacity(session_plans.len());
    let mut offset = 0;

    for (phase_index, &concurrency) in config.concurrency.iter().enumerate() {
        let phase_count =
            phase_session_count(session_plans.len(), config.concurrency.len(), phase_index);
        if phase_count == 0 {
            continue;
        }
        let phase_plans = session_plans[offset..offset + phase_count].to_vec();
        offset += phase_count;
        session_results.extend(
            run_session_phase(
                phase_plans,
                concurrency,
                root.clone(),
                Arc::new(config.clone()),
            )
            .map_err(|error| campaign_early_failure(error, &root, &config, &rerun_command))?,
        );
        enforce_runtime_safety(&config, &root)
            .map_err(|error| campaign_early_failure(error, &root, &config, &rerun_command))?;
    }
    if offset < session_plans.len() {
        session_results.extend(
            run_session_phase(
                session_plans[offset..].to_vec(),
                *config.concurrency.last().unwrap_or(&1),
                root.clone(),
                Arc::new(config.clone()),
            )
            .map_err(|error| campaign_early_failure(error, &root, &config, &rerun_command))?,
        );
    }

    let resources = monitor.stop();
    let mut result = aggregate_campaign(
        &config,
        session_results,
        started_at,
        utc_timestamp(),
        started.elapsed(),
        resources,
        rerun_command,
    );

    if result.verdict == "pass" || !config.retain_on_failure {
        safe_remove_simulation_root(&root)?;
        result.corpus_cleanup = "removed after reporting".to_string();
    } else {
        result.corpus_cleanup =
            "retained minimal failing sessions; path printed locally".to_string();
        eprintln!("retained failing simulation root: {}", root.display());
    }

    write_campaign_result(&config.result_path, &result)?;
    println!("result: {}", config.result_path.display());
    println!("rerun: {}", result.rerun_command);
    if result.verdict == "pass" {
        Ok(())
    } else {
        Err(format!(
            "{} invariant violation(s) were recorded.",
            result.invariant_violations.len()
        ))
    }
}

fn campaign_early_failure(
    error: String,
    root: &Path,
    config: &CampaignConfig,
    rerun_command: &str,
) -> String {
    eprintln!("seed: 0x{:016x}", config.seed);
    eprintln!("rerun: {rerun_command}");
    if config.retain_on_failure {
        eprintln!("retained simulation root: {}", root.display());
    } else if let Err(cleanup_error) = safe_remove_simulation_root(root) {
        eprintln!("simulation cleanup also failed: {cleanup_error}");
    }
    error
}

fn build_session_plans(config: &CampaignConfig) -> Vec<SessionPlan> {
    let base_attempts = config.attempts / config.sessions;
    let remainder = config.attempts % config.sessions;
    (0..config.sessions)
        .map(|session_id| SessionPlan {
            session_id,
            attempts: base_attempts + usize::from(session_id < remainder),
            seed: split_seed(config.seed, session_id as u64),
            persona: persona_for_session(config.seed, session_id),
        })
        .collect()
}

fn persona_for_session(campaign_seed: u64, session_id: usize) -> Persona {
    if session_id < Persona::ALL.len() {
        return Persona::ALL[session_id];
    }

    let mut rng = DeterministicRng::new(split_seed(campaign_seed, session_id as u64));
    match rng.range(100) {
        0..=14 => Persona::Beginner,
        15..=34 => Persona::BatchImage,
        35..=49 => Persona::Pdf,
        50..=59 => Persona::PrivacyFocused,
        60..=67 => Persona::Careless,
        68..=75 => Persona::ExternalDriveLike,
        76..=83 => Persona::UnicodePath,
        84..=91 => Persona::Power,
        92..=95 => Persona::PermissionFailure,
        _ => Persona::LongSession,
    }
}

fn phase_session_count(total: usize, phases: usize, phase_index: usize) -> usize {
    if phases == 0 || phase_index >= phases {
        return 0;
    }
    let base = total / phases;
    let remainder = total % phases;
    base + usize::from(phase_index < remainder)
}

fn run_session_phase(
    plans: Vec<SessionPlan>,
    concurrency: usize,
    root: PathBuf,
    config: Arc<CampaignConfig>,
) -> Result<Vec<SessionResult>, String> {
    let queue = Arc::new(Mutex::new(VecDeque::from(plans)));
    let (sender, receiver) = mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..concurrency {
        let worker_queue = Arc::clone(&queue);
        let worker_sender = sender.clone();
        let worker_root = root.clone();
        let worker_config = Arc::clone(&config);
        workers.push(thread::spawn(move || loop {
            let plan = worker_queue
                .lock()
                .ok()
                .and_then(|mut queue| queue.pop_front());
            let Some(plan) = plan else {
                break;
            };
            let result = run_session(&worker_root, &worker_config, plan);
            if worker_sender.send(result).is_err() {
                break;
            }
        }));
    }
    drop(sender);

    let mut results = Vec::new();
    for result in receiver {
        results.push(result?);
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| "simulated-user worker panicked.".to_string())?;
    }
    results.sort_by_key(|result| result.session_id);
    Ok(results)
}

fn run_session(
    campaign_root: &Path,
    config: &CampaignConfig,
    plan: SessionPlan,
) -> Result<SessionResult, String> {
    let session_started = Instant::now();
    let session_root = campaign_root.join(format!("session-{:04}", plan.session_id));
    let fixtures = create_session_fixtures(&session_root, plan.seed)?;
    let registry = BackendTaskRegistry::default();
    let mut rng = DeterministicRng::new(plan.seed);
    let mut outcomes = Vec::with_capacity(plan.attempts);
    let mut last_failed_plan: Option<OperationPlan> = None;
    let mut task_ids = HashSet::new();
    let mut report_exports = 0;

    exercise_preferences(&fixtures.root, plan.persona)?;

    for attempt_index in 0..plan.attempts {
        if session_started.elapsed() > config.scenario_timeout {
            outcomes.push(AttemptOutcome {
                task_id: format!("session-{}-scenario-timeout", plan.session_id),
                operation: "scenario-watchdog".to_string(),
                status: "failed".to_string(),
                duration_ms: 0,
                output_paths: Vec::new(),
                message: "scenario timeout reached".to_string(),
                collision: false,
                retry: false,
                source_hash_changed: false,
                overwrite_violation: false,
                temp_leak: false,
                timeout: true,
            });
            break;
        }

        let retry = last_failed_plan.is_some() && rng.chance(8);
        let operation = if retry {
            last_failed_plan
                .clone()
                .unwrap_or_else(|| choose_operation(&fixtures, &mut rng))
        } else {
            choose_operation(&fixtures, &mut rng)
        };
        let failure_mode = if retry {
            FailureMode::None
        } else {
            choose_failure_mode(plan.persona, attempt_index, &mut rng)
        };
        let task_id = format!(
            "session-{:04}-task-{:05}-{:08x}",
            plan.session_id,
            attempt_index,
            rng.next_u32()
        );
        if !task_ids.insert(task_id.clone()) {
            return Err(format!("duplicate deterministic task ID: {task_id}"));
        }
        let collision = attempt_index % 17 == 0;
        let outcome = execute_attempt(
            &registry,
            &fixtures,
            AttemptRequest {
                task_id,
                operation: operation.clone(),
                failure_mode,
                collision,
                retry,
                timeout: config.task_timeout,
                attempt_index,
            },
        )?;
        if outcome.status == "failed" || outcome.status == "unsupported" {
            last_failed_plan = Some(operation);
        } else if retry {
            last_failed_plan = None;
        }
        outcomes.push(outcome);

        if attempt_index > 0 && attempt_index % 10 == 0 {
            report_exports += exercise_report_export(&fixtures.root, &outcomes, attempt_index)?;
        }
        if directory_size(campaign_root) > config.max_data_bytes {
            return Err(format!(
                "generated data exceeded configured cap of {} bytes",
                config.max_data_bytes
            ));
        }
    }

    let source_hash_changes = verify_source_hashes(&fixtures.source_hashes);
    let mut result = summarize_session(plan, outcomes, report_exports, source_hash_changes);
    if result.failures.is_empty() {
        safe_remove_session_root(campaign_root, &session_root)?;
    }
    result.failures.truncate(MAX_FAILURES_IN_REPORT);
    Ok(result)
}

fn choose_operation(fixtures: &SessionFixtures, rng: &mut DeterministicRng) -> OperationPlan {
    match rng.range(100) {
        0..=23 => {
            let source = choose_image_source(fixtures, rng);
            let extension = extension(&source);
            let target = match extension.as_str() {
                "jpg" | "jpeg" => {
                    if rng.chance(50) {
                        "png"
                    } else {
                        "webp"
                    }
                }
                "png" => {
                    if rng.chance(50) {
                        "jpg"
                    } else {
                        "webp"
                    }
                }
                _ => {
                    if rng.chance(50) {
                        "jpg"
                    } else {
                        "png"
                    }
                }
            };
            OperationPlan::ImageConvert { source, target }
        }
        24..=38 => {
            let mode = match rng.range(3) {
                0 => "fit",
                1 => "width",
                _ => "height",
            };
            OperationPlan::ImageResize {
                source: choose_image_source(fixtures, rng),
                mode,
                max_width: (mode != "height").then_some(48),
                max_height: (mode != "width").then_some(36),
            }
        }
        39..=53 => {
            let source = choose_image_source(fixtures, rng);
            let quality =
                (extension(&source) != "png").then_some(if rng.chance(50) { 82 } else { 60 });
            OperationPlan::ImageCompress { source, quality }
        }
        54..=63 => OperationPlan::ImageCleanMetadata {
            source: if rng.chance(60) {
                fixtures.metadata_jpeg.clone()
            } else {
                choose_image_source(fixtures, rng)
            },
        },
        64..=73 => OperationPlan::PdfMerge {
            sources: vec![fixtures.pdf_one.clone(), fixtures.pdf_two.clone()],
        },
        74..=81 => OperationPlan::PdfSplit {
            source: fixtures.pdf_multi.clone(),
        },
        82..=90 => OperationPlan::PdfRotate {
            source: fixtures.pdf_multi.clone(),
            degrees: if rng.chance(50) { 90 } else { -90 },
        },
        _ => OperationPlan::PdfExtract {
            source: fixtures.pdf_multi.clone(),
            pages: if rng.chance(50) { "1,3" } else { "2-3" },
        },
    }
}

fn choose_image_source(fixtures: &SessionFixtures, rng: &mut DeterministicRng) -> PathBuf {
    match rng.range(3) {
        0 => fixtures.png[rng.range(fixtures.png.len() as u64) as usize].clone(),
        1 => fixtures.jpeg[rng.range(fixtures.jpeg.len() as u64) as usize].clone(),
        _ => fixtures.webp[rng.range(fixtures.webp.len() as u64) as usize].clone(),
    }
}

fn choose_failure_mode(
    persona: Persona,
    attempt_index: usize,
    rng: &mut DeterministicRng,
) -> FailureMode {
    if attempt_index == 0 && matches!(persona, Persona::Careless) {
        return FailureMode::CancelBeforeStart;
    }
    if attempt_index == 1 && matches!(persona, Persona::PermissionFailure) {
        return FailureMode::ReadOnlyOutput;
    }
    match rng.range(100) {
        0..=1 => FailureMode::MissingSource,
        2 => FailureMode::MalformedSource,
        3 => FailureMode::UnsupportedSource,
        4 => FailureMode::CancelBeforeStart,
        5 => FailureMode::CancelDuringExecution,
        _ => FailureMode::None,
    }
}

fn execute_attempt(
    registry: &BackendTaskRegistry,
    fixtures: &SessionFixtures,
    request: AttemptRequest,
) -> Result<AttemptOutcome, String> {
    let AttemptRequest {
        task_id,
        mut operation,
        failure_mode,
        collision,
        retry,
        timeout,
        attempt_index,
    } = request;
    let original_sources = operation.sources();
    let source_hashes = original_sources
        .iter()
        .filter(|path| path.is_file())
        .map(|path| Ok((path.clone(), sha256_file(path)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut temporarily_removed: Option<(PathBuf, PathBuf)> = None;

    match failure_mode {
        FailureMode::MalformedSource => {
            operation = OperationPlan::ImageConvert {
                source: fixtures.malformed_jpeg.clone(),
                target: "png",
            };
        }
        FailureMode::UnsupportedSource => {
            operation = OperationPlan::ImageCleanMetadata {
                source: fixtures.unsupported_heic.clone(),
            };
        }
        FailureMode::MissingSource => {
            let source = operation.sources().into_iter().next().unwrap_or_default();
            if source.exists() {
                let removed = source.with_extension(format!("{}.missing", extension(&source)));
                fs::rename(&source, &removed)
                    .map_err(|error| format!("unable to stage missing-source scenario: {error}"))?;
                temporarily_removed = Some((source, removed));
            }
        }
        _ => {}
    }

    let prepared = prepare_operation(
        &operation,
        &fixtures.root,
        attempt_index,
        collision,
        failure_mode == FailureMode::ReadOnlyOutput,
    )?;
    if failure_mode == FailureMode::CancelBeforeStart {
        registry.cancel(&task_id)?;
    }

    let execution = execute_with_registry(
        registry.clone(),
        task_id.clone(),
        prepared.clone(),
        timeout,
        failure_mode == FailureMode::CancelDuringExecution,
    )?;

    if let Some((source, removed)) = temporarily_removed {
        fs::rename(&removed, &source)
            .map_err(|error| format!("unable to restore missing-source fixture: {error}"))?;
    }
    restore_output_permissions(&prepared.output_path);

    let status = classify_status(&execution.value, execution.status);
    let output_paths = if execution
        .value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && execution.value.get("published").and_then(Value::as_bool) != Some(false)
    {
        output_paths_from_value(&execution.value)
    } else {
        Vec::new()
    };
    let message = execution
        .value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_hash_changed = source_hashes
        .iter()
        .any(|(path, expected)| path.exists() && sha256_file(path).ok().as_ref() != Some(expected));
    let overwrite_violation = prepared
        .collision_marker
        .as_ref()
        .is_some_and(|(path, expected)| fs::read(path).ok().as_ref() != Some(expected));
    let temp_leak = !find_task_temp_directories(&fixtures.root).is_empty();

    if matches!(status.as_str(), "cancelled" | "failed" | "unsupported") {
        for output in &output_paths {
            if Path::new(output).exists() {
                return Err(format!("terminal {status} task published output: {output}"));
            }
        }
    }
    for output in &output_paths {
        let output = Path::new(output);
        if !output.starts_with(&fixtures.root) {
            return Err("operation output escaped the simulation root.".to_string());
        }
        let metadata = fs::metadata(output)
            .map_err(|error| format!("published output cannot be inspected: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err("published output is empty or not a file.".to_string());
        }
    }

    Ok(AttemptOutcome {
        task_id,
        operation: operation.name().to_string(),
        status,
        duration_ms: duration_ms(execution.duration),
        output_paths: output_paths
            .iter()
            .map(|path| redact_path(path, &fixtures.root))
            .collect(),
        message: redact_message(&message, &fixtures.root),
        collision,
        retry,
        source_hash_changed,
        overwrite_violation,
        temp_leak,
        timeout: execution.timed_out,
    })
}

fn prepare_operation(
    operation: &OperationPlan,
    session_root: &Path,
    attempt_index: usize,
    collision: bool,
    read_only_output: bool,
) -> Result<PreparedOperation, String> {
    let source = operation
        .sources()
        .into_iter()
        .next()
        .ok_or_else(|| "operation requires a source.".to_string())?;
    let target_extension = match operation {
        OperationPlan::ImageConvert { target, .. } => (*target).to_string(),
        OperationPlan::ImageResize { source, .. }
        | OperationPlan::ImageCompress { source, .. }
        | OperationPlan::ImageCleanMetadata { source } => extension(source),
        _ => "pdf".to_string(),
    };
    let current_suffix = match operation {
        OperationPlan::ImageConvert { .. } => "",
        OperationPlan::ImageResize { .. } => " resized",
        OperationPlan::ImageCompress { .. } => " compressed",
        OperationPlan::ImageCleanMetadata { .. } => " cleaned",
        OperationPlan::PdfMerge { .. } => " merged",
        OperationPlan::PdfSplit { .. } => "-page",
        OperationPlan::PdfExtract { .. } => " extracted",
        OperationPlan::PdfRotate { .. } => " rotated",
    };
    let custom_output = session_root.join("custom-output");
    fs::create_dir_all(&custom_output)
        .map_err(|error| format!("unable to create custom output directory: {error}"))?;
    let strategy_index = attempt_index % 4;
    let (strategy, selected, remembered) = match strategy_index {
        0 => (DEFAULT_OUTPUT_STRATEGY, None, None),
        1 => ("same-folder-as-source", None, None),
        2 => ("ask-every-time", Some(path_string(&custom_output)), None),
        _ => (
            "remembered-custom-folder",
            None,
            Some(path_string(&custom_output)),
        ),
    };
    let request = OutputPathPlanRequest {
        source: path_string(&source),
        target_extension: target_extension.clone(),
        output_strategy: strategy.to_string(),
        selected_output_folder: selected,
        remembered_custom_folder: remembered,
        base_name: Some(format!("task-{attempt_index:05}")),
        current_suffix: Some(current_suffix.to_string()),
        naming: Some(OutputNamingRuleRequest {
            prefix: if attempt_index.is_multiple_of(11) {
                "测试_{date}_".to_string()
            } else {
                String::new()
            },
            suffix_preset: "current".to_string(),
            custom_suffix: String::new(),
        }),
        date_token: Some("2026-07-22".to_string()),
        time_token: Some("12-34-56".to_string()),
    };
    let mut output_plan = output_planning::plan_output_path_inner(&request)?;
    let mut marker = None;
    if collision {
        let collision_path = PathBuf::from(&output_plan.planned_output_path);
        if let Some(parent) = collision_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("unable to create collision directory: {error}"))?;
        }
        let marker_bytes = b"pre-existing output must remain unchanged".to_vec();
        fs::write(&collision_path, &marker_bytes)
            .map_err(|error| format!("unable to create collision marker: {error}"))?;
        marker = Some((collision_path, marker_bytes));
        output_plan = output_planning::plan_output_path_inner(&request)?;
    }
    let output_path = PathBuf::from(&output_plan.planned_output_path);
    if !output_path.starts_with(session_root) {
        return Err("planned output escaped the session root.".to_string());
    }
    if read_only_output {
        let parent = output_path
            .parent()
            .ok_or_else(|| "read-only output requires a parent.".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("unable to create read-only output directory: {error}"))?;
        set_read_only_directory(parent)?;
    }

    Ok(PreparedOperation {
        plan: operation.clone(),
        output_path,
        split_output_directory: (matches!(operation, OperationPlan::PdfSplit { .. }))
            .then(|| PathBuf::from(&output_plan.planned_output_directory)),
        split_prefix: (matches!(operation, OperationPlan::PdfSplit { .. }))
            .then(|| output_plan.planned_output_stem),
        collision_marker: marker,
    })
}

fn execute_with_registry(
    registry: BackendTaskRegistry,
    task_id: String,
    prepared: PreparedOperation,
    timeout: Duration,
    cancel_during: bool,
) -> Result<TaskExecution, String> {
    let operation = prepared.plan.name();
    let (control_sender, control_receiver) = mpsc::channel();
    let (result_sender, result_receiver) = mpsc::channel();
    let worker_registry = registry.clone();
    let worker_task_id = task_id.clone();
    let worker = thread::spawn(move || {
        let started = Instant::now();
        let control = worker_registry.register(&worker_task_id, operation)?;
        let _ = control.mark_running()?;
        control_sender
            .send(control.clone())
            .map_err(|_| "unable to expose task control to watchdog.".to_string())?;
        let value = execute_prepared_operation(prepared, control)?;
        let success = value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = worker_registry.finish(&worker_task_id, success)?;
        result_sender
            .send(TaskExecution {
                value,
                status,
                duration: started.elapsed(),
                timed_out: false,
            })
            .map_err(|_| "unable to return task result.".to_string())?;
        Ok::<(), String>(())
    });

    let control = control_receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "task did not register within two seconds.".to_string())?;
    if cancel_during {
        let wait_started = Instant::now();
        while wait_started.elapsed() < Duration::from_secs(2) {
            if control.has_attached_child()? {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        registry.cancel(&task_id)?;
    }

    let execution = match result_receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = registry.cancel(&task_id);
            let mut result = result_receiver
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| format!("task watchdog timed out: {task_id}"))?;
            result.timed_out = true;
            result
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err(format!("task worker disconnected: {task_id}"));
        }
    };
    worker
        .join()
        .map_err(|_| format!("task worker panicked: {task_id}"))??;
    Ok(execution)
}

fn execute_prepared_operation(
    prepared: PreparedOperation,
    control: TaskControl,
) -> Result<Value, String> {
    match prepared.plan {
        OperationPlan::ImageConvert { source, target } => {
            let request: ImageConvertExecutionRequest = decode_request(json!({
                "source": path_string(&source),
                "targetFormat": target,
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(image_convert::image_convert_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::ImageResize {
            source,
            mode,
            max_width,
            max_height,
        } => {
            let request: ImageResizeExecutionRequest = decode_request(json!({
                "source": path_string(&source),
                "mode": mode,
                "maxWidth": max_width,
                "maxHeight": max_height,
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(image_convert::image_resize_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::ImageCompress { source, quality } => {
            let request: ImageCompressExecutionRequest = decode_request(json!({
                "source": path_string(&source),
                "quality": quality,
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(image_convert::image_compress_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::ImageCleanMetadata { source } => {
            let request: ImageCleanMetadataExecutionRequest = decode_request(json!({
                "source": path_string(&source),
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(image_convert::image_clean_metadata_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::PdfMerge { sources } => {
            let request: QpdfMergeRequest = decode_request(json!({
                "sources": sources.iter().map(|path| path_string(path)).collect::<Vec<_>>(),
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(qpdf::qpdf_merge_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::PdfSplit { source } => {
            let request: QpdfSplitRequest = decode_request(json!({
                "source": path_string(&source),
                "outputDirectory": path_string(prepared.split_output_directory.as_deref().unwrap_or_else(|| prepared.output_path.parent().unwrap_or(Path::new(".")))),
                "filenamePrefix": prepared.split_prefix
            }))?;
            serde_json::to_value(qpdf::qpdf_split_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::PdfExtract { source, pages } => {
            let request: QpdfExtractPagesRequest = decode_request(json!({
                "source": path_string(&source),
                "pages": pages,
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(qpdf::qpdf_extract_task(request, control))
                .map_err(|error| error.to_string())
        }
        OperationPlan::PdfRotate { source, degrees } => {
            let request: QpdfRotatePagesRequest = decode_request(json!({
                "source": path_string(&source),
                "pages": "",
                "degrees": degrees,
                "output": path_string(&prepared.output_path)
            }))?;
            serde_json::to_value(qpdf::qpdf_rotate_task(request, control))
                .map_err(|error| error.to_string())
        }
    }
}

fn decode_request<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("request construction failed: {error}"))
}

fn classify_status(value: &Value, backend_status: BackendTaskStatus) -> String {
    if backend_status == BackendTaskStatus::Cancelled {
        return "cancelled".to_string();
    }
    if !value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        return if message.contains("not enabled")
            || message.contains("unsupported")
            || message.contains("heic")
        {
            "unsupported".to_string()
        } else {
            "failed".to_string()
        };
    }
    if value.get("published").and_then(Value::as_bool) == Some(false) {
        if value.get("smaller").and_then(Value::as_bool) == Some(false) {
            return "not_smaller".to_string();
        }
        if value.get("changed").and_then(Value::as_bool) == Some(false) {
            return "skipped".to_string();
        }
    }
    "success".to_string()
}

fn output_paths_from_value(value: &Value) -> Vec<String> {
    if let Some(paths) = value.get("outputPaths").and_then(Value::as_array) {
        return paths
            .iter()
            .filter_map(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
            .collect();
    }
    value
        .get("outputPath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(|path| vec![path.to_string()])
        .unwrap_or_default()
}

fn summarize_session(
    plan: SessionPlan,
    outcomes: Vec<AttemptOutcome>,
    report_exports: usize,
    source_hash_changes: usize,
) -> SessionResult {
    let mut operation_distribution = BTreeMap::new();
    let mut status_distribution = BTreeMap::new();
    let mut failures = Vec::new();
    for outcome in &outcomes {
        *operation_distribution
            .entry(outcome.operation.clone())
            .or_insert(0) += 1;
        *status_distribution
            .entry(outcome.status.clone())
            .or_insert(0) += 1;
        if outcome.source_hash_changed
            || outcome.overwrite_violation
            || outcome.temp_leak
            || outcome.timeout
        {
            failures.push(format!(
                "{} {}: source_changed={} overwrite={} temp_leak={} timeout={}",
                outcome.task_id,
                outcome.operation,
                outcome.source_hash_changed,
                outcome.overwrite_violation,
                outcome.temp_leak,
                outcome.timeout
            ));
        }
    }
    SessionResult {
        session_id: plan.session_id,
        persona: plan.persona.as_str().to_string(),
        seed: format!("0x{:016x}", plan.seed),
        attempts: outcomes.len(),
        operation_distribution,
        status_distribution,
        durations_ms: outcomes.iter().map(|outcome| outcome.duration_ms).collect(),
        retries: outcomes.iter().filter(|outcome| outcome.retry).count(),
        cancellations: outcomes
            .iter()
            .filter(|outcome| outcome.status == "cancelled")
            .count(),
        collisions: outcomes.iter().filter(|outcome| outcome.collision).count(),
        report_exports,
        source_hash_changes: source_hash_changes
            + outcomes
                .iter()
                .filter(|outcome| outcome.source_hash_changed)
                .count(),
        overwrite_violations: outcomes
            .iter()
            .filter(|outcome| outcome.overwrite_violation)
            .count(),
        temporary_leaks: outcomes.iter().filter(|outcome| outcome.temp_leak).count(),
        timeouts: outcomes.iter().filter(|outcome| outcome.timeout).count(),
        failures,
    }
}

fn aggregate_campaign(
    config: &CampaignConfig,
    sessions: Vec<SessionResult>,
    started_at: String,
    finished_at: String,
    elapsed: Duration,
    resources: ResourceObservations,
    rerun_command: String,
) -> CampaignResult {
    let mut persona_distribution = BTreeMap::new();
    let mut operation_distribution = BTreeMap::new();
    let mut status_distribution = BTreeMap::new();
    let mut durations = Vec::new();
    let mut failures = Vec::new();
    let mut attempts = 0;
    let mut retries = 0;
    let mut cancellations = 0;
    let mut collisions = 0;
    let mut report_exports = 0;
    let mut source_hash_changes = 0;
    let mut overwrite_violations = 0;
    let mut temporary_leaks = 0;
    let mut timeouts = 0;
    for session in &sessions {
        *persona_distribution
            .entry(session.persona.clone())
            .or_insert(0) += 1;
        merge_counts(&mut operation_distribution, &session.operation_distribution);
        merge_counts(&mut status_distribution, &session.status_distribution);
        durations.extend_from_slice(&session.durations_ms);
        attempts += session.attempts;
        retries += session.retries;
        cancellations += session.cancellations;
        collisions += session.collisions;
        report_exports += session.report_exports;
        source_hash_changes += session.source_hash_changes;
        overwrite_violations += session.overwrite_violations;
        temporary_leaks += session.temporary_leaks;
        timeouts += session.timeouts;
        failures.extend(session.failures.iter().cloned());
    }
    durations.sort_unstable();
    let mut invariant_violations = Vec::new();
    if attempts != config.attempts {
        invariant_violations.push(format!(
            "expected {} task attempts but recorded {attempts}",
            config.attempts
        ));
    }
    if source_hash_changes > 0 {
        invariant_violations.push(format!("{source_hash_changes} source hash change(s)"));
    }
    if overwrite_violations > 0 {
        invariant_violations.push(format!("{overwrite_violations} output overwrite(s)"));
    }
    if temporary_leaks > 0 {
        invariant_violations.push(format!("{temporary_leaks} task temporary leak(s)"));
    }
    if timeouts > 0 {
        invariant_violations.push(format!("{timeouts} task timeout(s)"));
    }
    failures.truncate(MAX_FAILURES_IN_REPORT);
    let elapsed_seconds = elapsed.as_secs_f64().max(0.001);
    CampaignResult {
        schema_version: 1,
        evidence_type: "deterministic simulated local desktop workload",
        profile: config.profile.as_str().to_string(),
        seed: format!("0x{:016x}", config.seed),
        git_commit: command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string()),
        app_version: env!("CARGO_PKG_VERSION"),
        started_at,
        finished_at,
        macos_version: command_output("sw_vers", &["-productVersion"])
            .unwrap_or_else(|| "unavailable".to_string()),
        architecture: std::env::consts::ARCH.to_string(),
        virtual_sessions: sessions.len(),
        task_attempts: attempts,
        concurrency: config.concurrency.clone(),
        persona_distribution,
        operation_distribution,
        status_distribution,
        retry_count: retries,
        cancellation_count: cancellations,
        collision_count: collisions,
        report_exports,
        duration_ms: duration_ms(elapsed),
        throughput_tasks_per_second: attempts as f64 / elapsed_seconds,
        average_task_duration_ms: if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<u64>() as f64 / durations.len() as f64
        },
        p50_task_duration_ms: percentile(&durations, 50),
        p95_task_duration_ms: percentile(&durations, 95),
        p99_task_duration_ms: percentile(&durations, 99),
        source_hash_changes,
        output_overwrite_violations: overwrite_violations,
        temporary_directory_leaks: temporary_leaks,
        crashes: 0,
        hangs: 0,
        timeouts,
        verdict: if invariant_violations.is_empty() { "pass" } else { "fail" },
        invariant_violations,
        failures,
        resource_observations: resources,
        network_observation: "network inspection is recorded separately during packaged-app smoke testing",
        unresolved_limitations: vec![
            "Disk-full behavior is boundary-tested only; the real disk is never filled.".to_string(),
            "GUI native picker automation is verified separately and is not multiplied per virtual session.".to_string(),
            "Simulated sessions are not evidence of real production-user scale.".to_string(),
        ],
        corpus_cleanup: "pending".to_string(),
        rerun_command,
    }
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (key, value) in source {
        *target.entry(key.clone()).or_insert(0) += value;
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) * percentile) / 100;
    sorted[index]
}

fn create_session_fixtures(root: &Path, seed: u64) -> Result<SessionFixtures, String> {
    fs::create_dir_all(root).map_err(|error| format!("unable to create session root: {error}"))?;
    let nested = root.join("嵌套 目录").join("emoji-📄");
    let duplicate_a = root.join("duplicate-a");
    let duplicate_b = root.join("duplicate-b");
    fs::create_dir_all(&nested).map_err(|error| error.to_string())?;
    fs::create_dir_all(&duplicate_a).map_err(|error| error.to_string())?;
    fs::create_dir_all(&duplicate_b).map_err(|error| error.to_string())?;

    let png = vec![
        root.join("中文 图片.png"),
        nested.join("multi.part.name (1).png"),
        duplicate_a.join("same-name.png"),
        duplicate_b.join("same-name.png"),
        root.join("compose-é.png"),
        root.join("decompose-e\u{301}.png"),
        root.join(format!("long-{}-{}.png", "x".repeat(120), seed % 1000)),
    ];
    let jpeg = vec![
        root.join("English photo with spaces.jpg"),
        nested.join("只读 source.jpeg"),
    ];
    let webp = vec![root.join("preview-图片-📷.webp")];
    for (index, path) in png.iter().enumerate() {
        write_pattern_image(path, ImageFormat::Png, 96, 64, seed + index as u64)?;
    }
    for (index, path) in jpeg.iter().enumerate() {
        write_pattern_image(path, ImageFormat::Jpeg, 96, 64, seed + 20 + index as u64)?;
    }
    for (index, path) in webp.iter().enumerate() {
        write_pattern_image(path, ImageFormat::WebP, 96, 64, seed + 40 + index as u64)?;
    }
    set_read_only_file(&jpeg[1])?;

    let metadata_jpeg = root.join("含隐私 metadata.jpg");
    write_jpeg_with_comment(&metadata_jpeg, seed + 50)?;
    let pdf_one = root.join("报告 one.pdf");
    let pdf_two = nested.join("报告 two.pdf");
    let pdf_multi = root.join("multi.page.报告.pdf");
    write_tiny_pdf_pages(&pdf_one, &["One"])?;
    write_tiny_pdf_pages(&pdf_two, &["Two"])?;
    write_tiny_pdf_pages(&pdf_multi, &["One", "Two", "Three", "Four"])?;
    let malformed_jpeg = root.join("malformed.jpg");
    fs::write(&malformed_jpeg, b"not an image").map_err(|error| error.to_string())?;
    let unsupported_heic = root.join("unsupported.heic");
    fs::write(&unsupported_heic, b"synthetic unsupported HEIC")
        .map_err(|error| error.to_string())?;
    let zero_pdf = root.join("zero-byte.pdf");
    fs::write(&zero_pdf, []).map_err(|error| error.to_string())?;

    let mut source_hashes = BTreeMap::new();
    for path in png.iter().chain(jpeg.iter()).chain(webp.iter()).chain([
        &metadata_jpeg,
        &pdf_one,
        &pdf_two,
        &pdf_multi,
        &malformed_jpeg,
        &unsupported_heic,
        &zero_pdf,
    ]) {
        source_hashes.insert(path.clone(), sha256_file(path)?);
    }

    Ok(SessionFixtures {
        root: root.to_path_buf(),
        png,
        jpeg,
        webp,
        metadata_jpeg,
        pdf_one,
        pdf_two,
        pdf_multi,
        malformed_jpeg,
        unsupported_heic,
        source_hashes,
    })
}

fn write_pattern_image(
    path: &Path,
    format: ImageFormat,
    width: u32,
    height: u32,
    seed: u64,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut image = RgbaImage::new(width, height);
    let mut rng = DeterministicRng::new(seed);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([
            (rng.next_u32() as u8).wrapping_add(x as u8),
            (rng.next_u32() as u8).wrapping_add(y as u8),
            rng.next_u32() as u8,
            255,
        ]);
    }
    DynamicImage::ImageRgba8(image)
        .save_with_format(path, format)
        .map_err(|error| format!("unable to write synthetic image: {error}"))
}

fn write_jpeg_with_comment(path: &Path, seed: u64) -> Result<(), String> {
    let base = path.with_extension("base.jpg");
    write_pattern_image(&base, ImageFormat::Jpeg, 96, 64, seed)?;
    let bytes = fs::read(&base).map_err(|error| error.to_string())?;
    let comment = b"synthetic owner and GPS-like privacy metadata";
    let length = u16::try_from(comment.len() + 2)
        .map_err(|_| "synthetic metadata comment is too long.".to_string())?;
    let mut encoded = Vec::with_capacity(bytes.len() + comment.len() + 4);
    encoded.extend_from_slice(&bytes[..2]);
    encoded.extend_from_slice(&[0xff, 0xfe]);
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(comment);
    encoded.extend_from_slice(&bytes[2..]);
    fs::write(path, encoded).map_err(|error| error.to_string())?;
    fs::remove_file(base).map_err(|error| error.to_string())
}

fn write_tiny_pdf_pages(path: &Path, labels: &[&str]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let page_count = labels.len();
    let page_object_ids = (0..page_count)
        .map(|index| 4 + index * 2)
        .collect::<Vec<_>>();
    let kids = page_object_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
        format!("2 0 obj << /Type /Pages /Kids [{kids}] /Count {page_count} >> endobj\n"),
        "3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        let page_id = 4 + index * 2;
        let content_id = page_id + 1;
        let stream = format!("BT /F1 24 Tf 72 720 Td ({label}) Tj ET\n");
        objects.push(format!(
            "{page_id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >> endobj\n"
        ));
        objects.push(format!(
            "{content_id} 0 obj << /Length {} >> stream\n{}endstream endobj\n",
            stream.len(),
            stream
        ));
    }
    let mut data = Vec::from("%PDF-1.4\n".as_bytes());
    let mut offsets = vec![0usize];
    for object in objects {
        offsets.push(data.len());
        data.extend_from_slice(object.as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    fs::write(path, data).map_err(|error| error.to_string())
}

fn exercise_preferences(root: &Path, persona: Persona) -> Result<(), String> {
    let preferences_path = root.join("preferences.json");
    let sentinel = root.join("preference-reset-sentinel.txt");
    fs::write(&sentinel, b"must remain").map_err(|error| error.to_string())?;
    let preferences = UserPreferences {
        active_tool: match persona {
            Persona::Pdf => ToolSection::Pdf,
            Persona::PrivacyFocused => ToolSection::MetadataCleanup,
            _ => ToolSection::ImageConvert,
        },
        image_target_format: ImageTargetFormat::Png,
        resize_mode: ResizeMode::Width,
        resize_width: 1_920,
        report_format: ReportFormat::Json,
        output_location_mode: OutputLocationMode::ConvertedFolderNextToSource,
        output_suffix_preset: OutputSuffixPreset::Converted,
        ..UserPreferences::default()
    };
    preferences::save_preferences_to_path(&preferences_path, preferences.clone())?;
    let loaded = preferences::load_preferences_from_path(&preferences_path);
    if loaded.preferences != preferences {
        return Err("preference round trip changed simulated user values.".to_string());
    }
    preferences::reset_preferences_at_path(&preferences_path)?;
    if preferences_path.exists() || !sentinel.exists() {
        return Err("preference reset changed files outside preferences.".to_string());
    }
    Ok(())
}

fn exercise_report_export(
    root: &Path,
    outcomes: &[AttemptOutcome],
    index: usize,
) -> Result<usize, String> {
    let filtered = outcomes
        .iter()
        .rev()
        .take(4)
        .enumerate()
        .map(|(record_index, outcome)| {
            json!({
                "taskId": outcome.task_id,
                "operationType": outcome.operation,
                "sourcePath": format!("<simulation-root>/source-{record_index}"),
                "sourceName": format!("synthetic-{record_index}.png"),
                "sourceExtension": "png",
                "outputPath": outcome.output_paths.first().cloned().unwrap_or_default(),
                "outputName": outcome.output_paths.first().and_then(|path| Path::new(path).file_name()).and_then(OsStr::to_str).unwrap_or_default(),
                "outputExtension": outcome.output_paths.first().map(|path| extension(Path::new(path))).unwrap_or_default(),
                "status": outcome.status,
                "startedAt": "2026-07-22T00:00:00Z",
                "finishedAt": "2026-07-22T00:00:01Z",
                "durationMs": outcome.duration_ms,
                "sourceBytes": 100,
                "outputBytes": 80,
                "savedBytes": 20,
                "savedPercent": 20.0,
                "message": outcome.message
            })
        })
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return Ok(0);
    }
    let reports = root.join("reports");
    fs::create_dir_all(&reports).map_err(|error| error.to_string())?;
    let mut count = 0;
    for format in ["csv", "json"] {
        let destination = reports.join(format!("filtered-{index}.{format}"));
        let request: ExportTaskReportRequest = decode_request(json!({
            "destinationPath": path_string(&destination),
            "format": format,
            "appVersion": env!("CARGO_PKG_VERSION"),
            "generatedAt": format!("session-{index}"),
            "tasks": filtered
        }))?;
        task_report::export_task_report(request)?;
        let contents = fs::read_to_string(&destination)
            .map_err(|error| format!("unable to read exported report: {error}"))?;
        for prohibited in [
            "GPSLatitude",
            "EXIF payload",
            "IPTC payload",
            "file contents",
        ] {
            if contents.contains(prohibited) {
                return Err(format!(
                    "exported report contains prohibited payload: {prohibited}"
                ));
            }
        }
        if format == "json" {
            let parsed: Value =
                serde_json::from_str(&contents).map_err(|error| error.to_string())?;
            if parsed.get("tasks").and_then(Value::as_array).map(Vec::len) != Some(filtered.len()) {
                return Err("filtered JSON report contains the wrong task set.".to_string());
            }
        } else if !contents.starts_with("appVersion,reportGeneratedAt,taskId") {
            return Err("filtered CSV report is missing its header.".to_string());
        }
        count += 1;
    }
    Ok(count)
}

fn verify_source_hashes(expected: &BTreeMap<PathBuf, String>) -> usize {
    expected
        .iter()
        .filter(|(path, hash)| sha256_file(path).ok().as_ref() != Some(hash))
        .count()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "unable to hash synthetic source {}: {error}",
            path.display()
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn find_task_temp_directories(root: &Path) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    walk_paths(root, &mut |path| {
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with(".localconvert-task-"))
        {
            matches.push(path.to_path_buf());
        }
    });
    matches
}

fn walk_paths(root: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        visit(&path);
        if path.is_dir() {
            walk_paths(&path, visit);
        }
    }
}

fn directory_size(root: &Path) -> u64 {
    let mut bytes = 0_u64;
    walk_paths(root, &mut |path| {
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    });
    bytes
}

fn enforce_runtime_safety(config: &CampaignConfig, root: &Path) -> Result<(), String> {
    let free = free_disk_kib(root)?;
    if free < STOP_FREE_KIB {
        return Err("campaign stopped because free disk space fell below 10 GiB.".to_string());
    }
    let bytes = directory_size(root);
    if bytes > config.max_data_bytes || bytes > HARD_MAX_DATA_BYTES {
        return Err("campaign stopped because generated data exceeded its safety cap.".to_string());
    }
    if memory_free_percent().is_some_and(|percent| percent < 5) {
        return Err("campaign stopped because system memory pressure became critical.".to_string());
    }
    Ok(())
}

fn allocate_simulation_root(profile: Profile, seed: u64) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "localconvert-simulated-users-{}-{}-{seed:016x}",
        profile.as_str(),
        std::process::id()
    ));
    fs::create_dir(&root)
        .map_err(|error| format!("unable to allocate simulation root: {error}"))?;
    Ok(root)
}

fn safe_remove_session_root(campaign_root: &Path, session_root: &Path) -> Result<(), String> {
    if !session_root.starts_with(campaign_root)
        || session_root == campaign_root
        || !campaign_root
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("localconvert-simulated-users-"))
    {
        return Err("refused unsafe simulated-user session cleanup.".to_string());
    }
    match fs::remove_dir_all(session_root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("unable to clean simulated session: {error}")),
    }
}

fn safe_remove_simulation_root(root: &Path) -> Result<(), String> {
    let temp = std::env::temp_dir();
    let valid_name = root
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with("localconvert-simulated-users-"));
    if root.parent() != Some(temp.as_path()) || !valid_name {
        return Err("refused unsafe simulated-user root cleanup.".to_string());
    }
    match fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("unable to clean simulation root: {error}")),
    }
}

fn write_campaign_result(path: &Path, result: &CampaignResult) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "campaign result path requires a parent directory.".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(result).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| format!("unable to write campaign result: {error}"))
}

fn rerun_command(config: &CampaignConfig) -> String {
    let mut command = format!(
        "npm run test:simulated-users:{} -- --seed 0x{:016x} --sessions {} --attempts {} --concurrency {}",
        config.profile.as_str(),
        config.seed,
        config.sessions,
        config.attempts,
        config
            .concurrency
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    if config.profile == Profile::Stress {
        command.push_str(" --allow-stress");
    }
    if config.profile == Profile::Soak {
        command.push_str(&format!(
            " --allow-soak --duration-minutes {}",
            config.duration.as_secs() / 60
        ));
    }
    command
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn redact_path(path: &str, root: &Path) -> String {
    path.replace(&path_string(root), "<simulation-root>")
}

fn redact_message(message: &str, root: &Path) -> String {
    let redacted = redact_path(message, root);
    redacted
        .replace("/Users/", "<private-user-root>/")
        .chars()
        .take(500)
        .collect()
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn utc_timestamp() -> String {
    command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    })
}

fn command_output(command: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(command).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn free_disk_kib(path: &Path) -> Result<u64, String> {
    let output = Command::new("df")
        .args(["-Pk"])
        .arg(path)
        .output()
        .map_err(|error| format!("unable to inspect free disk space: {error}"))?;
    if !output.status.success() {
        return Err("df failed while checking simulated-user safety limits.".to_string());
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .ok_or_else(|| "df returned no filesystem row.".to_string())?
        .to_string();
    line.split_whitespace()
        .nth(3)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| "unable to parse free disk space from df.".to_string())
}

fn memory_free_percent() -> Option<u8> {
    let output = command_output("memory_pressure", &["-Q"])?;
    output.lines().find_map(|line| {
        line.strip_prefix("System-wide memory free percentage: ")
            .and_then(|value| value.trim_end_matches('%').parse::<u8>().ok())
    })
}

#[cfg(unix)]
fn set_read_only_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o444))
        .map_err(|error| format!("unable to set read-only source permissions: {error}"))
}

#[cfg(not(unix))]
fn set_read_only_file(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn set_read_only_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o555))
        .map_err(|error| format!("unable to set read-only output permissions: {error}"))
}

#[cfg(not(unix))]
fn set_read_only_directory(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

fn restore_output_permissions(output: &Path) {
    let Some(parent) = output.parent() else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    if let Ok(metadata) = fs::metadata(parent) {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = fs::set_permissions(parent, permissions);
    }
}

fn split_seed(seed: u64, stream: u64) -> u64 {
    let mut value = seed.wrapping_add(stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        split_seed(self.state, 0)
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn range(&mut self, upper: u64) -> u64 {
        if upper == 0 {
            0
        } else {
            self.next_u64() % upper
        }
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.range(100) < percent
    }
}

struct ResourceMonitor {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<ResourceObservations>,
}

impl ResourceMonitor {
    fn start(root: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let pid = std::process::id().to_string();
            let mut observations = ResourceObservations {
                method: "ps/lsof/pgrep/df/memory_pressure sampling every 500ms".to_string(),
                ..ResourceObservations::default()
            };
            let mut sample_index = 0;
            while !worker_stop.load(Ordering::SeqCst) {
                sample_resources(&mut observations, &pid, &root, sample_index);
                sample_index += 1;
                thread::sleep(Duration::from_millis(500));
            }
            sample_resources(&mut observations, &pid, &root, sample_index);
            observations.rss_end_kib = current_rss_cpu(&pid).map(|value| value.0);
            observations
        });
        Self { stop, handle }
    }

    fn stop(self) -> ResourceObservations {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().unwrap_or_default()
    }
}

fn sample_resources(
    observations: &mut ResourceObservations,
    pid: &str,
    root: &Path,
    sample_index: usize,
) {
    if let Some((rss, cpu)) = current_rss_cpu(pid) {
        observations.rss_start_kib.get_or_insert(rss);
        observations.peak_rss_kib = Some(observations.peak_rss_kib.unwrap_or(0).max(rss));
        observations.peak_cpu_percent = Some(observations.peak_cpu_percent.unwrap_or(0.0).max(cpu));
    }
    observations.peak_temporary_bytes = observations.peak_temporary_bytes.max(directory_size(root));
    if let Ok(free) = free_disk_kib(root) {
        observations.minimum_free_disk_kib =
            Some(observations.minimum_free_disk_kib.unwrap_or(free).min(free));
    }
    if sample_index.is_multiple_of(4) {
        if let Some(fds) = command_output("lsof", &["-p", pid])
            .map(|output| output.lines().count().saturating_sub(1))
        {
            observations.peak_open_file_descriptors = Some(
                observations
                    .peak_open_file_descriptors
                    .unwrap_or(0)
                    .max(fds),
            );
        }
        if let Some(children) =
            command_output("pgrep", &["-P", pid]).map(|output| output.lines().count())
        {
            observations.peak_child_processes =
                Some(observations.peak_child_processes.unwrap_or(0).max(children));
        }
        if let Some(percent) = memory_free_percent() {
            observations.minimum_memory_free_percent = Some(
                observations
                    .minimum_memory_free_percent
                    .unwrap_or(percent)
                    .min(percent),
            );
        }
    }
}

fn current_rss_cpu(pid: &str) -> Option<(u64, f64)> {
    let output = command_output("ps", &["-o", "rss=,%cpu=", "-p", pid])?;
    let mut fields = output.split_whitespace();
    Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_rng_replays_the_same_sequence() {
        let mut first = DeterministicRng::new(QUICK_SEED);
        let mut second = DeterministicRng::new(QUICK_SEED);
        let first_values = (0..20).map(|_| first.next_u64()).collect::<Vec<_>>();
        let second_values = (0..20).map(|_| second.next_u64()).collect::<Vec<_>>();
        assert_eq!(first_values, second_values);
    }

    #[test]
    fn stress_and_soak_require_explicit_authorization() {
        let stress = CampaignConfig::defaults(Profile::Stress);
        let soak = CampaignConfig::defaults(Profile::Soak);
        assert!(validate_config(&stress).is_err());
        assert!(validate_config(&soak).is_err());
    }

    #[test]
    fn cleanup_refuses_paths_outside_the_unique_test_root() {
        assert!(safe_remove_simulation_root(Path::new("/tmp")).is_err());
        assert!(safe_remove_session_root(Path::new("/tmp/a"), Path::new("/tmp/b")).is_err());
    }

    #[test]
    fn profile_plans_cover_all_personas_and_exact_attempt_count() {
        let config = CampaignConfig::defaults(Profile::Quick);
        let plans = build_session_plans(&config);
        assert_eq!(plans.len(), 20);
        assert_eq!(plans.iter().map(|plan| plan.attempts).sum::<usize>(), 300);
        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.persona.as_str())
                .collect::<HashSet<_>>()
                .len(),
            10
        );
        assert_eq!(
            plans
                .iter()
                .take(10)
                .map(|plan| plan.persona.as_str())
                .collect::<Vec<_>>(),
            Persona::ALL
                .iter()
                .map(|persona| persona.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            build_session_plans(&config)
                .iter()
                .map(|plan| plan.persona.as_str())
                .collect::<Vec<_>>(),
            plans
                .iter()
                .map(|plan| plan.persona.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn report_redaction_removes_simulation_and_private_user_roots() {
        let root = Path::new("/tmp/localconvert-simulated-users-test");
        let message = "/tmp/localconvert-simulated-users-test/a /Users/name/private";
        let redacted = redact_message(message, root);
        assert!(!redacted.contains("/tmp/localconvert"));
        assert!(!redacted.contains("/Users/"));
    }

    #[test]
    fn timeout_helper_reaps_controlled_process() {
        let error = crate::timed_process::run_command_with_timeout(
            Path::new("/bin/sleep"),
            &["1"],
            Duration::from_millis(25),
        )
        .expect_err("controlled sleep must time out");
        assert!(error.is_timeout());
    }
}
