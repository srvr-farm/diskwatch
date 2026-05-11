use std::env;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const STDERR_DIAGNOSTIC_LIMIT: usize = 512;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionalCommandOutput {
    pub output: Option<String>,
    pub diagnostic: Option<String>,
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

    let stdout = child.stdout.take().map(spawn_reader);
    let stderr = child.stderr.take().map(spawn_reader);

    if timeout.is_zero() {
        return timeout_result(program, child, process_group, stdout, stderr, timeout);
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                kill_process_group(process_group);
                let output = match collect_output(program, stdout, stderr) {
                    Ok(output) => output,
                    Err(diagnostic) => {
                        return OptionalCommandOutput {
                            output: None,
                            diagnostic: Some(diagnostic),
                        }
                    }
                };

                return output_result(program, status, output);
            }
            Ok(None) if started.elapsed() >= timeout => {
                return timeout_result(program, child, process_group, stdout, stderr, timeout);
            }
            Ok(None) => thread::sleep(poll_interval(started, timeout)),
            Err(error) => {
                kill_process_group(process_group);
                let _ = child.wait();
                let _ = collect_output(program, stdout, stderr);
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

fn spawn_reader<R>(mut reader: R) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn collect_output(
    program: &str,
    stdout: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
) -> Result<CommandOutputBytes, String> {
    Ok(CommandOutputBytes {
        stdout: join_reader(program, "stdout", stdout)?,
        stderr: join_reader(program, "stderr", stderr)?,
    })
}

fn join_reader(
    program: &str,
    stream_name: &str,
    reader: Option<JoinHandle<io::Result<Vec<u8>>>>,
) -> Result<Vec<u8>, String> {
    let Some(reader) = reader else {
        return Ok(Vec::new());
    };

    match reader.join() {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("failed to read {program} {stream_name}: {error}")),
        Err(_) => Err(format!("{program} {stream_name} reader failed")),
    }
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
    stdout: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    timeout: Duration,
) -> OptionalCommandOutput {
    kill_process_group(process_group);
    let _ = child.wait();
    let _ = collect_output(program, stdout, stderr);
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
}
