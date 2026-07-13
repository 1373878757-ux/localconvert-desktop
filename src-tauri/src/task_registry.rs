use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendTaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BackendTaskStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ChildProcessState {
    Running,
    Exited(Option<i32>),
    Cancelled,
}

struct BackendTaskEntry {
    operation: String,
    status: Mutex<BackendTaskStatus>,
    cancel_requested: AtomicBool,
    child: Mutex<Option<Child>>,
}

#[derive(Clone)]
pub(crate) struct TaskControl {
    task_id: String,
    entry: Arc<BackendTaskEntry>,
}

#[derive(Clone, Default)]
pub(crate) struct BackendTaskRegistry {
    state: Arc<Mutex<BackendTaskRegistryState>>,
}

#[derive(Default)]
struct BackendTaskRegistryState {
    entries: HashMap<String, Arc<BackendTaskEntry>>,
    pending_cancellations: HashSet<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelTaskResponse {
    pub(crate) task_id: String,
    pub(crate) operation: String,
    pub(crate) status: &'static str,
    pub(crate) cancellation_requested: bool,
    pub(crate) process_termination_requested: bool,
    pub(crate) message: String,
}

impl BackendTaskRegistry {
    pub(crate) fn register(&self, task_id: &str, operation: &str) -> Result<TaskControl, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Backend taskId is required.".to_string());
        }

        let operation = operation.trim();
        if operation.is_empty() {
            return Err("Backend task operation is required.".to_string());
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "Backend task registry lock is unavailable.".to_string())?;
        let cancelled_before_registration = state.pending_cancellations.remove(task_id);

        if let Some(existing) = state.entries.get(task_id) {
            let status = existing
                .status
                .lock()
                .map_err(|_| "Backend task status lock is unavailable.".to_string())?;
            if !status.is_terminal() {
                return Err(format!("Backend task is already active: {task_id}"));
            }
        }

        let entry = Arc::new(BackendTaskEntry {
            operation: operation.to_string(),
            status: Mutex::new(if cancelled_before_registration {
                BackendTaskStatus::Cancelled
            } else {
                BackendTaskStatus::Queued
            }),
            cancel_requested: AtomicBool::new(cancelled_before_registration),
            child: Mutex::new(None),
        });
        state
            .entries
            .insert(task_id.to_string(), Arc::clone(&entry));

        Ok(TaskControl {
            task_id: task_id.to_string(),
            entry,
        })
    }

    pub(crate) fn finish(
        &self,
        task_id: &str,
        succeeded: bool,
    ) -> Result<BackendTaskStatus, String> {
        let entry = self.entry(task_id)?;
        let mut status = entry
            .status
            .lock()
            .map_err(|_| "Backend task status lock is unavailable.".to_string())?;

        if *status != BackendTaskStatus::Cancelled {
            *status = if succeeded {
                BackendTaskStatus::Completed
            } else {
                BackendTaskStatus::Failed
            };
        }

        Ok(*status)
    }

    #[cfg(test)]
    pub(crate) fn status(&self, task_id: &str) -> Result<BackendTaskStatus, String> {
        let entry = self.entry(task_id)?;
        let status = entry
            .status
            .lock()
            .map_err(|_| "Backend task status lock is unavailable.".to_string())?;
        Ok(*status)
    }

    pub(crate) fn cancel(&self, task_id: &str) -> Result<CancelTaskResponse, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Backend taskId is required.".to_string());
        }

        let entry = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Backend task registry lock is unavailable.".to_string())?;
            match state.entries.get(task_id).cloned() {
                Some(entry) => Some(entry),
                None => {
                    state.pending_cancellations.insert(task_id.to_string());
                    None
                }
            }
        };

        let Some(entry) = entry else {
            return Ok(CancelTaskResponse {
                task_id: task_id.to_string(),
                operation: "pending-registration".to_string(),
                status: BackendTaskStatus::Cancelled.as_str(),
                cancellation_requested: true,
                process_termination_requested: false,
                message: "Backend task cancellation accepted before registration.".to_string(),
            });
        };

        let control = TaskControl {
            task_id: task_id.to_string(),
            entry,
        };
        let operation = control.entry.operation.clone();

        let (status, cancellation_requested, message) = {
            let mut status = control
                .entry
                .status
                .lock()
                .map_err(|_| "Backend task status lock is unavailable.".to_string())?;

            match *status {
                BackendTaskStatus::Queued | BackendTaskStatus::Running => {
                    control.entry.cancel_requested.store(true, Ordering::SeqCst);
                    *status = BackendTaskStatus::Cancelled;
                    (
                        *status,
                        true,
                        "Backend task cancellation accepted.".to_string(),
                    )
                }
                BackendTaskStatus::Cancelled => (
                    *status,
                    true,
                    "Backend task was already cancelled.".to_string(),
                ),
                BackendTaskStatus::Completed | BackendTaskStatus::Failed => (
                    *status,
                    false,
                    "Backend task is already finished and cannot be cancelled.".to_string(),
                ),
            }
        };

        let process_termination_requested = if cancellation_requested {
            control.terminate_child()?
        } else {
            false
        };

        Ok(CancelTaskResponse {
            task_id: task_id.to_string(),
            operation,
            status: status.as_str(),
            cancellation_requested,
            process_termination_requested,
            message,
        })
    }

    fn entry(&self, task_id: &str) -> Result<Arc<BackendTaskEntry>, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Backend taskId is required.".to_string());
        }

        self.find_entry(task_id)?
            .ok_or_else(|| format!("Backend task was not found: {task_id}"))
    }

    fn find_entry(&self, task_id: &str) -> Result<Option<Arc<BackendTaskEntry>>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "Backend task registry lock is unavailable.".to_string())?
            .entries
            .get(task_id)
            .cloned())
    }
}

impl TaskControl {
    #[cfg(test)]
    pub(crate) fn detached(operation: &str) -> Self {
        Self {
            task_id: format!("detached-{operation}"),
            entry: Arc::new(BackendTaskEntry {
                operation: operation.to_string(),
                status: Mutex::new(BackendTaskStatus::Running),
                cancel_requested: AtomicBool::new(false),
                child: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn task_id(&self) -> &str {
        &self.task_id
    }

    pub(crate) fn mark_running(&self) -> Result<bool, String> {
        let mut status = self
            .entry
            .status
            .lock()
            .map_err(|_| "Backend task status lock is unavailable.".to_string())?;

        if *status == BackendTaskStatus::Queued {
            *status = BackendTaskStatus::Running;
            return Ok(true);
        }

        Ok(*status == BackendTaskStatus::Running)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.entry.cancel_requested.load(Ordering::SeqCst)
    }

    pub(crate) fn attach_child(&self, mut child: Child) -> Result<bool, String> {
        if self.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }

        let mut child_slot = self
            .entry
            .child
            .lock()
            .map_err(|_| "Backend child-process lock is unavailable.".to_string())?;
        if child_slot.is_some() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Backend task already owns a child process: {}",
                self.task_id
            ));
        }

        if self.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }

        *child_slot = Some(child);
        Ok(true)
    }

    pub(crate) fn poll_child(&self) -> Result<ChildProcessState, String> {
        if self.is_cancelled() {
            self.terminate_child()?;
            return Ok(ChildProcessState::Cancelled);
        }

        let mut child_slot = self
            .entry
            .child
            .lock()
            .map_err(|_| "Backend child-process lock is unavailable.".to_string())?;
        let child = child_slot
            .as_mut()
            .ok_or_else(|| format!("Backend task has no child process: {}", self.task_id))?;

        match child
            .try_wait()
            .map_err(|error| format!("Unable to inspect child process state: {error}"))?
        {
            Some(status) => {
                child_slot.take();
                if self.is_cancelled() {
                    Ok(ChildProcessState::Cancelled)
                } else {
                    Ok(ChildProcessState::Exited(status.code()))
                }
            }
            None => Ok(ChildProcessState::Running),
        }
    }

    pub(crate) fn terminate_child(&self) -> Result<bool, String> {
        let mut child_slot = self
            .entry
            .child
            .lock()
            .map_err(|_| "Backend child-process lock is unavailable.".to_string())?;
        let Some(child) = child_slot.as_mut() else {
            return Ok(false);
        };

        let was_running = child.try_wait().map_or(true, |status| status.is_none());
        if was_running {
            let _ = child.kill();
        }
        let _ = child.wait();
        child_slot.take();
        Ok(was_running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_backend_task_in_queued_state() {
        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("task-1", "qpdf-merge")
            .expect("task should register");

        assert_eq!(control.task_id(), "task-1");
        assert_eq!(registry.status("task-1"), Ok(BackendTaskStatus::Queued));
    }

    #[test]
    fn transitions_running_tasks_to_completed_or_failed() {
        let registry = BackendTaskRegistry::default();
        let completed = registry
            .register("task-complete", "qpdf-split")
            .expect("completed task should register");
        assert_eq!(completed.mark_running(), Ok(true));
        assert_eq!(
            registry.finish("task-complete", true),
            Ok(BackendTaskStatus::Completed)
        );

        let failed = registry
            .register("task-failed", "qpdf-rotate")
            .expect("failed task should register");
        assert_eq!(failed.mark_running(), Ok(true));
        assert_eq!(
            registry.finish("task-failed", false),
            Ok(BackendTaskStatus::Failed)
        );
    }

    #[test]
    fn cancellation_before_completion_is_terminal() {
        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("task-cancel", "qpdf-extract")
            .expect("task should register");
        assert_eq!(control.mark_running(), Ok(true));

        let response = registry
            .cancel("task-cancel")
            .expect("cancellation should succeed");

        assert!(response.cancellation_requested);
        assert_eq!(response.status, "cancelled");
        assert!(control.is_cancelled());
        assert_eq!(
            registry.status("task-cancel"),
            Ok(BackendTaskStatus::Cancelled)
        );
    }

    #[test]
    fn cancellation_before_registration_prevents_task_start() {
        let registry = BackendTaskRegistry::default();
        let response = registry
            .cancel("task-not-registered-yet")
            .expect("early cancellation should be recorded");

        assert!(response.cancellation_requested);
        assert_eq!(response.status, "cancelled");

        let control = registry
            .register("task-not-registered-yet", "qpdf-merge")
            .expect("cancelled task should still register for a terminal response");
        assert!(control.is_cancelled());
        assert_eq!(control.mark_running(), Ok(false));
        assert_eq!(
            registry.finish("task-not-registered-yet", true),
            Ok(BackendTaskStatus::Cancelled)
        );
    }

    #[test]
    fn concurrent_registration_and_cancellation_always_end_cancelled() {
        use std::{
            sync::{Arc, Barrier},
            thread,
        };

        for index in 0..50 {
            let registry = BackendTaskRegistry::default();
            let task_id = format!("task-race-{index}");
            let barrier = Arc::new(Barrier::new(3));

            let register_registry = registry.clone();
            let register_task_id = task_id.clone();
            let register_barrier = Arc::clone(&barrier);
            let register_thread = thread::spawn(move || {
                register_barrier.wait();
                register_registry.register(&register_task_id, "qpdf-merge")
            });

            let cancel_registry = registry.clone();
            let cancel_task_id = task_id.clone();
            let cancel_barrier = Arc::clone(&barrier);
            let cancel_thread = thread::spawn(move || {
                cancel_barrier.wait();
                cancel_registry.cancel(&cancel_task_id)
            });

            barrier.wait();
            let control = register_thread
                .join()
                .expect("registration thread should not panic")
                .expect("task should register");
            let response = cancel_thread
                .join()
                .expect("cancellation thread should not panic")
                .expect("task should cancel");

            assert!(response.cancellation_requested);
            assert!(control.is_cancelled());
            assert_eq!(registry.status(&task_id), Ok(BackendTaskStatus::Cancelled));
        }
    }

    #[test]
    fn late_success_cannot_overwrite_cancelled_status() {
        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("task-race", "qpdf-merge")
            .expect("task should register");
        assert_eq!(control.mark_running(), Ok(true));
        registry
            .cancel("task-race")
            .expect("cancellation should succeed");

        assert_eq!(
            registry.finish("task-race", true),
            Ok(BackendTaskStatus::Cancelled)
        );
        assert_eq!(
            registry.status("task-race"),
            Ok(BackendTaskStatus::Cancelled)
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_attached_child_process() {
        use std::process::Command;

        let registry = BackendTaskRegistry::default();
        let control = registry
            .register("task-child", "qpdf-merge")
            .expect("task should register");
        control.mark_running().expect("task should start");
        let child = Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("sleep fixture should start");
        assert_eq!(control.attach_child(child), Ok(true));

        let response = registry
            .cancel("task-child")
            .expect("cancellation should terminate child");

        assert!(response.process_termination_requested);
        assert_eq!(control.poll_child(), Ok(ChildProcessState::Cancelled));
    }
}
