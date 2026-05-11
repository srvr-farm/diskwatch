use std::env;
use std::io::{self, ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const STDERR_DIAGNOSTIC_LIMIT: usize = 512;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionalCommandOutput {
    pub output: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct OptionalCommandBudget {
    started: Instant,
    total: Duration,
    per_command: Duration,
}

impl OptionalCommandBudget {
    pub fn new(total: Duration, per_command: Duration) -> Self {
        Self {
            started: Instant::now(),
            total,
            per_command,
        }
    }

    pub fn remaining_timeout(&self) -> Option<Duration> {
        let remaining = self.total.checked_sub(self.started.elapsed())?;
        if remaining.is_zero() || self.per_command.is_zero() {
            None
        } else {
            Some(remaining.min(self.per_command))
        }
    }

    pub fn exhausted(&self) -> bool {
        self.remaining_timeout().is_none()
    }
}

pub fn run_optional_budgeted(
    program: &str,
    args: &[&str],
    budget: &OptionalCommandBudget,
) -> Option<OptionalCommandOutput> {
    budget
        .remaining_timeout()
        .map(|timeout| run_optional(program, args, timeout))
}

pub fn run_optional(program: &str, args: &[&str], timeout: Duration) -> OptionalCommandOutput {
    if find_in_path(program).is_none() {
        return OptionalCommandOutput {
            output: None,
            diagnostic: Some(format!("{program} not found")),
        };
    }

    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // SAFETY: `pre_exec` runs after fork and before exec in the child. The closure only calls
    // `setpgid`, which is async-signal-safe on Linux, so it is safe for this context.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return OptionalCommandOutput {
                output: None,
                diagnostic: Some(format!("failed to run {program}: {error}")),
            };
        }
    };
    ensure_child_process_group(&child);
    let process_group = ProcessGroup::from_child(&child);

    let mut output =
        match CommandOutputReaders::new(child.stdout.take(), child.stderr.take(), program) {
            Ok(output) => output,
            Err(diagnostic) => {
                kill_process_group(process_group);
                let _ = child.wait();
                return OptionalCommandOutput {
                    output: None,
                    diagnostic: Some(diagnostic),
                };
            }
        };

    if timeout.is_zero() {
        return timeout_result(program, child, process_group, output, timeout);
    }

    let started = Instant::now();
    loop {
        if let Err(diagnostic) = output.drain(program) {
            kill_process_group(process_group);
            let _ = child.wait();
            return OptionalCommandOutput {
                output: None,
                diagnostic: Some(diagnostic),
            };
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                kill_process_group(process_group);
                if let Err(diagnostic) = output.drain(program) {
                    return OptionalCommandOutput {
                        output: None,
                        diagnostic: Some(diagnostic),
                    };
                };

                return output_result(program, status, output.into_bytes());
            }
            Ok(None) if started.elapsed() >= timeout => {
                return timeout_result(program, child, process_group, output, timeout);
            }
            Ok(None) => thread::sleep(poll_interval(started, timeout)),
            Err(error) => {
                kill_process_group(process_group);
                let _ = child.wait();
                return OptionalCommandOutput {
                    output: None,
                    diagnostic: Some(format!("failed while waiting for {program}: {error}")),
                };
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProcessGroup {
    pgid: libc::pid_t,
}

impl ProcessGroup {
    fn from_child(child: &Child) -> Self {
        Self {
            pgid: child.id() as libc::pid_t,
        }
    }
}

fn ensure_child_process_group(child: &Child) {
    let pid = child.id() as libc::pid_t;

    // SAFETY: `setpgid` is called with the spawned child pid and desired pgid. It does not
    // dereference pointers; failures are acceptable because child-side `pre_exec` also sets it.
    unsafe {
        let _ = libc::setpgid(pid, pid);
    }
}

struct CommandOutputBytes {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct CommandOutputReaders {
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    stdout_bytes: Vec<u8>,
    stderr_bytes: Vec<u8>,
}

impl CommandOutputReaders {
    fn new(
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
        program: &str,
    ) -> Result<Self, String> {
        if let Some(stdout) = stdout.as_ref() {
            set_nonblocking(stdout, program, "stdout")?;
        }
        if let Some(stderr) = stderr.as_ref() {
            set_nonblocking(stderr, program, "stderr")?;
        }

        Ok(Self {
            stdout,
            stderr,
            stdout_bytes: Vec::new(),
            stderr_bytes: Vec::new(),
        })
    }

    fn drain(&mut self, program: &str) -> Result<(), String> {
        drain_reader(program, "stdout", &mut self.stdout, &mut self.stdout_bytes)?;
        drain_reader(program, "stderr", &mut self.stderr, &mut self.stderr_bytes)
    }

    fn into_bytes(self) -> CommandOutputBytes {
        CommandOutputBytes {
            stdout: self.stdout_bytes,
            stderr: self.stderr_bytes,
        }
    }
}

fn set_nonblocking<R>(reader: &R, program: &str, stream_name: &str) -> Result<(), String>
where
    R: AsRawFd,
{
    let fd = reader.as_raw_fd();

    // SAFETY: `fcntl` operates on a valid pipe fd owned by the child handle and does not
    // dereference pointers.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(format!(
            "failed to read {program} {stream_name}: {}",
            io::Error::last_os_error()
        ));
    }

    // SAFETY: `fcntl` updates flags on the same valid fd. Failure is reported as a diagnostic.
    let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if result < 0 {
        return Err(format!(
            "failed to read {program} {stream_name}: {}",
            io::Error::last_os_error()
        ));
    }

    Ok(())
}

fn drain_reader<R>(
    program: &str,
    stream_name: &str,
    reader: &mut Option<R>,
    output: &mut Vec<u8>,
) -> Result<(), String>
where
    R: Read,
{
    let Some(stream) = reader.as_mut() else {
        return Ok(());
    };

    let mut buffer = [0_u8; 8192];
    let reached_eof = loop {
        match stream.read(&mut buffer) {
            Ok(0) => break true,
            Ok(bytes_read) => output.extend_from_slice(&buffer[..bytes_read]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => break false,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(format!("failed to read {program} {stream_name}: {error}"));
            }
        }
    };

    if reached_eof {
        *reader = None;
    }

    Ok(())
}

fn output_result(
    program: &str,
    status: ExitStatus,
    output: CommandOutputBytes,
) -> OptionalCommandOutput {
    if status.success() {
        return match String::from_utf8(output.stdout) {
            Ok(stdout) => OptionalCommandOutput {
                output: Some(stdout),
                diagnostic: None,
            },
            Err(error) => OptionalCommandOutput {
                output: None,
                diagnostic: Some(format!("{program} stdout was not valid UTF-8: {error}")),
            },
        };
    }

    match String::from_utf8(output.stdout) {
        Ok(stdout) => OptionalCommandOutput {
            output: if stdout.is_empty() {
                None
            } else {
                Some(stdout)
            },
            diagnostic: Some(exit_diagnostic(program, status, &output.stderr)),
        },
        Err(error) => OptionalCommandOutput {
            output: None,
            diagnostic: Some(format!("{program} stdout was not valid UTF-8: {error}")),
        },
    }
}

fn exit_diagnostic(program: &str, status: ExitStatus, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = truncate_diagnostic(stderr.trim());
    if stderr.is_empty() {
        format!("{program} exited with {status}")
    } else {
        format!("{program} exited with {status}: {stderr}")
    }
}

fn timeout_result(
    program: &str,
    mut child: Child,
    process_group: ProcessGroup,
    mut output: CommandOutputReaders,
    timeout: Duration,
) -> OptionalCommandOutput {
    kill_process_group(process_group);
    let _ = child.wait();
    let _ = output.drain(program);
    OptionalCommandOutput {
        output: None,
        diagnostic: Some(format!("{program} timed out after {timeout:?}")),
    }
}

fn kill_process_group(process_group: ProcessGroup) {
    if process_group.pgid <= 0 {
        return;
    }

    // SAFETY: `kill` is called with a negative process-group id derived from the spawned child
    // pid. It does not dereference pointers, and ESRCH is acceptable when the group already exited.
    unsafe {
        let _ = libc::kill(-process_group.pgid, libc::SIGKILL);
    }
}

fn truncate_diagnostic(diagnostic: &str) -> String {
    if diagnostic.len() <= STDERR_DIAGNOSTIC_LIMIT {
        return diagnostic.to_owned();
    }

    let mut truncated = diagnostic
        .char_indices()
        .take_while(|(index, _)| *index < STDERR_DIAGNOSTIC_LIMIT)
        .map(|(_, character)| character)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn poll_interval(started: Instant, timeout: Duration) -> Duration {
    timeout
        .checked_sub(started.elapsed())
        .unwrap_or(Duration::ZERO)
        .min(Duration::from_millis(10))
}

pub fn program_available(program: &str) -> bool {
    find_in_path(program).is_some()
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_command_as_diagnostic() {
        let result = run_optional(
            "definitely-not-diskwatch-command",
            &[],
            Duration::from_millis(50),
        );
        assert!(result.output.is_none());
        assert!(result.diagnostic.unwrap().contains("not found"));
    }

    #[test]
    fn captures_successful_stdout() {
        let result = run_optional("printf", &["hello"], Duration::from_secs(1));
        assert_eq!(result.output.as_deref(), Some("hello"));
        assert_eq!(result.diagnostic, None);
    }

    #[test]
    fn captures_large_stdout_without_timing_out() {
        let result = run_optional(
            "sh",
            &[
                "-c",
                "i=0; while [ $i -lt 200000 ]; do printf x; i=$((i + 1)); done",
            ],
            Duration::from_secs(2),
        );
        assert_eq!(result.output.as_ref().map(String::len), Some(200_000));
        assert_eq!(result.diagnostic, None);
    }

    #[test]
    fn timeout_kills_command_and_reports_timeout() {
        let result = run_optional("sh", &["-c", "sleep 1"], Duration::from_millis(25));
        assert!(result.output.is_none());
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("timed out")),
            "expected timeout diagnostic, got {:?}",
            result.diagnostic
        );
    }

    #[test]
    fn timeout_kills_descendant_holding_output_pipes() {
        let started = Instant::now();
        let result = run_optional("sh", &["-c", "sleep 1 & wait"], Duration::from_millis(25));

        assert!(result.output.is_none());
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("timed out")),
            "expected timeout diagnostic, got {:?}",
            result.diagnostic
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "timeout waited for descendant sleep: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn timeout_does_not_wait_for_escaped_descendant_holding_output_pipes() {
        if find_in_path("setsid").is_none() {
            return;
        }

        let started = Instant::now();
        let result = run_optional(
            "sh",
            &["-c", "setsid sh -c 'sleep 1' & wait"],
            Duration::from_millis(25),
        );

        assert!(result.output.is_none());
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("timed out")),
            "expected timeout diagnostic, got {:?}",
            result.diagnostic
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "timeout waited for escaped descendant sleep: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn successful_exit_cleans_up_lingering_descendant_holding_output_pipes() {
        let started = Instant::now();
        let result = run_optional(
            "sh",
            &["-c", "sleep 1 & printf done"],
            Duration::from_secs(2),
        );

        assert_eq!(result.output.as_deref(), Some("done"));
        assert_eq!(result.diagnostic, None);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "successful command waited for lingering descendant: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn successful_exit_does_not_wait_for_escaped_descendant_holding_output_pipes() {
        if find_in_path("setsid").is_none() {
            return;
        }

        let started = Instant::now();
        let result = run_optional(
            "sh",
            &["-c", "setsid sh -c 'sleep 1' & printf done"],
            Duration::from_secs(2),
        );

        assert_eq!(result.output.as_deref(), Some("done"));
        assert_eq!(result.diagnostic, None);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "successful command waited for escaped descendant: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn non_zero_exit_includes_stderr_without_empty_suffix() {
        let result = run_optional(
            "sh",
            &["-c", "printf 'problem details' >&2; exit 7"],
            Duration::from_secs(1),
        );
        assert!(result.output.is_none());
        let diagnostic = result.diagnostic.unwrap();
        assert!(diagnostic.contains("problem details"));
        assert!(!diagnostic.ends_with(": "));

        let result = run_optional("sh", &["-c", "exit 7"], Duration::from_secs(1));
        assert!(result.output.is_none());
        let diagnostic = result.diagnostic.unwrap();
        assert!(diagnostic.contains("exited with"));
        assert!(!diagnostic.ends_with(": "));
    }

    #[test]
    fn non_zero_exit_preserves_stdout_and_reports_diagnostic() {
        let result = run_optional(
            "sh",
            &[
                "-c",
                "printf 'SMART overall-health self-assessment test result: PASSED\n'; printf 'bitmask status' >&2; exit 4",
            ],
            Duration::from_secs(1),
        );

        assert!(
            result
                .output
                .as_deref()
                .is_some_and(|output| output.contains("PASSED")),
            "expected stdout to be preserved, got {:?}",
            result.output
        );
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("bitmask status")),
            "expected non-zero diagnostic, got {:?}",
            result.diagnostic
        );
    }

    #[test]
    fn invalid_utf8_stdout_reports_utf8_diagnostic() {
        let result = run_optional("sh", &["-c", "printf '\\377'"], Duration::from_secs(1));
        assert!(result.output.is_none());
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("UTF-8")),
            "expected UTF-8 diagnostic, got {:?}",
            result.diagnostic
        );
    }

    #[test]
    fn zero_timeout_reports_timeout() {
        let result = run_optional("printf", &["hello"], Duration::ZERO);
        assert!(result.output.is_none());
        assert!(
            result
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("timed out")),
            "expected timeout diagnostic, got {:?}",
            result.diagnostic
        );
    }

    #[test]
    fn budgeted_optional_runner_stops_after_total_timeout() {
        let budget = OptionalCommandBudget::new(Duration::from_millis(25), Duration::from_secs(1));
        let first = run_optional_budgeted("sh", &["-c", "sleep 1"], &budget);

        assert!(first
            .and_then(|result| result.diagnostic)
            .is_some_and(|diagnostic| diagnostic.contains("timed out")));
        assert!(budget.exhausted());
        assert!(run_optional_budgeted("printf", &["hello"], &budget).is_none());
    }
}
