use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static WORKSPACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FinalizedOutput {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
}

#[derive(Debug)]
pub(crate) struct BatchFinalizeError {
    pub(crate) message: String,
    pub(crate) published: Vec<FinalizedOutput>,
}

pub(crate) struct TaskOutputWorkspace {
    root: PathBuf,
    final_parent: PathBuf,
}

impl TaskOutputWorkspace {
    pub(crate) fn create(final_parent: &Path, task_id: &str) -> Result<Self, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Task output workspace requires a taskId.".to_string());
        }

        if final_parent.exists() && !final_parent.is_dir() {
            return Err(format!(
                "Output path exists but is not a folder: {}",
                path_to_string(final_parent)
            ));
        }
        fs::create_dir_all(final_parent).map_err(|error| {
            format!(
                "Unable to create output folder {}: {error}",
                path_to_string(final_parent)
            )
        })?;

        let mut hasher = DefaultHasher::new();
        task_id.hash(&mut hasher);
        let task_hash = hasher.finish();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();

        for _ in 0..128 {
            let sequence = WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let candidate = final_parent.join(format!(
                ".localconvert-task-{}-{task_hash:016x}-{timestamp:x}-{sequence:x}",
                process::id()
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    return Ok(Self {
                        root: candidate,
                        final_parent: final_parent.to_path_buf(),
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Unable to create task-owned output workspace in {}: {error}",
                        path_to_string(final_parent)
                    ));
                }
            }
        }

        Err("Unable to allocate a unique task-owned output workspace.".to_string())
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn temp_file(&self, file_name: &str) -> Result<PathBuf, String> {
        let path = Path::new(file_name);
        let mut components = path.components();
        let is_single_normal_component =
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
        if !is_single_normal_component {
            return Err("Task temporary output must use a plain filename.".to_string());
        }

        Ok(self.root.join(path))
    }

    pub(crate) fn finalize_file(
        &self,
        temp_path: &Path,
        final_path: &Path,
    ) -> Result<FinalizedOutput, String> {
        let bytes = self.validate_mapping(temp_path, final_path)?;
        self.publish_validated(temp_path, final_path, bytes)
    }

    pub(crate) fn finalize_files(
        &self,
        mappings: &[(PathBuf, PathBuf)],
    ) -> Result<Vec<FinalizedOutput>, BatchFinalizeError> {
        if mappings.is_empty() {
            return Err(BatchFinalizeError {
                message: "No task-owned outputs were provided for finalization.".to_string(),
                published: Vec::new(),
            });
        }

        let mut validated = Vec::with_capacity(mappings.len());
        let mut final_paths = HashSet::with_capacity(mappings.len());
        for (temp_path, final_path) in mappings {
            if !final_paths.insert(final_path.clone()) {
                return Err(BatchFinalizeError {
                    message: format!(
                        "Duplicate final output path is not allowed: {}",
                        path_to_string(final_path)
                    ),
                    published: Vec::new(),
                });
            }
            match self.validate_mapping(temp_path, final_path) {
                Ok(bytes) => validated.push((temp_path, final_path, bytes)),
                Err(message) => {
                    return Err(BatchFinalizeError {
                        message,
                        published: Vec::new(),
                    });
                }
            }
        }

        let mut published = Vec::with_capacity(validated.len());
        for (temp_path, final_path, bytes) in validated {
            match self.publish_validated(temp_path, final_path, bytes) {
                Ok(output) => published.push(output),
                Err(message) => {
                    return Err(BatchFinalizeError { message, published });
                }
            }
        }

        Ok(published)
    }

    fn validate_mapping(&self, temp_path: &Path, final_path: &Path) -> Result<u64, String> {
        if temp_path.parent() != Some(self.root.as_path()) {
            return Err(format!(
                "Temporary output is not owned by this task workspace: {}",
                path_to_string(temp_path)
            ));
        }
        if final_path.parent() != Some(self.final_parent.as_path()) {
            return Err(format!(
                "Final output must stay inside the selected output folder: {}",
                path_to_string(final_path)
            ));
        }

        let metadata = fs::metadata(temp_path).map_err(|error| {
            format!(
                "Task output is missing or cannot be inspected: {}: {error}",
                path_to_string(temp_path)
            )
        })?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "Task output is empty or is not a file: {}",
                path_to_string(temp_path)
            ));
        }

        Ok(metadata.len())
    }

    fn publish_validated(
        &self,
        temp_path: &Path,
        final_path: &Path,
        bytes: u64,
    ) -> Result<FinalizedOutput, String> {
        fs::hard_link(temp_path, final_path).map_err(|error| {
            if error.kind() == ErrorKind::AlreadyExists || final_path.exists() {
                format!(
                    "Final output already exists and will not be overwritten: {}",
                    path_to_string(final_path)
                )
            } else {
                format!(
                    "Unable to atomically finalize output {}: {error}",
                    path_to_string(final_path)
                )
            }
        })?;

        let _ = fs::remove_file(temp_path);
        Ok(FinalizedOutput {
            path: final_path.to_path_buf(),
            bytes,
        })
    }
}

impl Drop for TaskOutputWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_dir(name: &str) -> PathBuf {
        let sequence = WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "localconvert-output-finalize-{name}-{}-{sequence}",
            process::id()
        ))
    }

    #[test]
    fn finalizes_temp_output_without_overwrite_for_chinese_paths_and_spaces() {
        let case_dir = case_dir("success");
        let converted = case_dir.join("客户 文件").join("converted");
        let workspace = TaskOutputWorkspace::create(&converted, "任务 一")
            .expect("workspace should be created");
        let workspace_path = workspace.root().to_path_buf();
        let temp = workspace
            .temp_file("output.pdf")
            .expect("temp path should be valid");
        let final_path = converted.join("报告 结果.pdf");
        fs::write(&temp, b"validated output").expect("temp output should be written");

        let finalized = workspace
            .finalize_file(&temp, &final_path)
            .expect("output should finalize");

        assert_eq!(finalized.path, final_path);
        assert_eq!(finalized.bytes, 16);
        assert_eq!(fs::read(&finalized.path).unwrap(), b"validated output");
        assert!(!temp.exists());
        drop(workspace);
        assert!(!workspace_path.exists());
        assert!(finalized.path.exists());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn final_path_appearing_after_planning_is_never_overwritten() {
        let case_dir = case_dir("collision");
        let converted = case_dir.join("converted");
        let workspace = TaskOutputWorkspace::create(&converted, "collision-task")
            .expect("workspace should be created");
        let workspace_path = workspace.root().to_path_buf();
        let temp = workspace.temp_file("output.pdf").unwrap();
        let final_path = converted.join("report.pdf");
        fs::write(&temp, b"new output").unwrap();
        fs::write(&final_path, b"user file").unwrap();

        let error = workspace
            .finalize_file(&temp, &final_path)
            .expect_err("collision must fail");

        assert!(error.contains("will not be overwritten"));
        assert_eq!(fs::read(&final_path).unwrap(), b"user file");
        drop(workspace);
        assert!(!workspace_path.exists());
        assert_eq!(fs::read(&final_path).unwrap(), b"user file");

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn cancellation_style_cleanup_removes_only_task_owned_workspace() {
        let case_dir = case_dir("cleanup");
        let converted = case_dir.join("converted");
        fs::create_dir_all(&converted).unwrap();
        let preexisting = converted.join("keep.pdf");
        fs::write(&preexisting, b"keep me").unwrap();
        let workspace = TaskOutputWorkspace::create(&converted, "cancel-task").unwrap();
        let workspace_path = workspace.root().to_path_buf();
        let temp = workspace.temp_file("partial.pdf").unwrap();
        fs::write(&temp, b"partial").unwrap();

        drop(workspace);

        assert!(!workspace_path.exists());
        assert_eq!(fs::read(&preexisting).unwrap(), b"keep me");

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn rejects_foreign_temp_paths_without_deleting_them() {
        let case_dir = case_dir("foreign");
        let converted = case_dir.join("converted");
        let workspace = TaskOutputWorkspace::create(&converted, "foreign-task").unwrap();
        let foreign = case_dir.join("foreign.pdf");
        let final_path = converted.join("result.pdf");
        fs::write(&foreign, b"foreign").unwrap();

        assert!(workspace.finalize_file(&foreign, &final_path).is_err());
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
        assert!(!final_path.exists());

        let _ = fs::remove_dir_all(case_dir);
    }
}
