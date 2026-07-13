use std::{
    fmt,
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) const ENGINE_SELF_CHECK_TIMEOUT: Duration = Duration::from_secs(3);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub(crate) enum TimedCommandError {
    Spawn(String),
    Wait(String),
    TimedOut { timeout: Duration },
}

impl TimedCommandError {
    pub(crate) fn is_timeout(&self) -> bool {
        matches!(self, Self::TimedOut { .. })
    }
}

impl fmt::Display for TimedCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(message) | Self::Wait(message) => formatter.write_str(message),
            Self::TimedOut { timeout } if timeout.subsec_nanos() == 0 => {
                write!(formatter, "timed out after {} seconds", timeout.as_secs())
            }
            Self::TimedOut { timeout } => {
                write!(
                    formatter,
                    "timed out after {} milliseconds",
                    timeout.as_millis()
                )
            }
        }
    }
}

pub(crate) fn run_command_with_timeout(
    executable: &Path,
    arguments: &[&str],
    timeout: Duration,
) -> Result<Output, TimedCommandError> {
    if timeout.is_zero() {
        return Err(TimedCommandError::TimedOut { timeout });
    }

    let executable_display = executable.to_string_lossy();
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            TimedCommandError::Spawn(format!(
                "unable to start self-check process {executable_display}: {error}"
            ))
        })?;

    let mut stdout_reader = child.stdout.take().map(read_pipe_in_thread);
    let mut stderr_reader = child.stderr.take().map(read_pipe_in_thread);
    let started_at = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                terminate_child(&mut child);
                let _ = join_pipe_reader(stdout_reader.take());
                let _ = join_pipe_reader(stderr_reader.take());
                return Err(TimedCommandError::Wait(format!(
                    "unable to inspect self-check process {executable_display}: {error}"
                )));
            }
        }

        let elapsed = started_at.elapsed();
        if elapsed >= timeout {
            terminate_child(&mut child);
            let _ = join_pipe_reader(stdout_reader.take());
            let _ = join_pipe_reader(stderr_reader.take());
            return Err(TimedCommandError::TimedOut { timeout });
        }

        thread::sleep(PROCESS_POLL_INTERVAL.min(timeout.saturating_sub(elapsed)));
    };

    Ok(Output {
        status,
        stdout: join_pipe_reader(stdout_reader.take()),
        stderr: join_pipe_reader(stderr_reader.take()),
    })
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_pipe_in_thread<R>(mut pipe: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn join_pipe_reader(reader: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn self_check_process_returns_timeout_and_reaps_hanging_child() {
        let started_at = Instant::now();
        let error =
            run_command_with_timeout(Path::new("/bin/sleep"), &["5"], Duration::from_millis(75))
                .expect_err("hanging self-check must time out");

        assert!(error.is_timeout());
        assert!(error.to_string().contains("75 milliseconds"));
        assert!(started_at.elapsed() < Duration::from_secs(2));
    }
}
