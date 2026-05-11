use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

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

    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return OptionalCommandOutput {
                output: None,
                diagnostic: Some(format!("failed to run {program}: {error}")),
            };
        }
    };

    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = match child.wait_with_output() {
                    Ok(output) => output,
                    Err(error) => {
                        return OptionalCommandOutput {
                            output: None,
                            diagnostic: Some(format!(
                                "failed to collect {program} output: {error}"
                            )),
                        };
                    }
                };

                if status.success() {
                    return match String::from_utf8(output.stdout) {
                        Ok(stdout) => OptionalCommandOutput {
                            output: Some(stdout),
                            diagnostic: None,
                        },
                        Err(error) => OptionalCommandOutput {
                            output: None,
                            diagnostic: Some(format!(
                                "{program} stdout was not valid UTF-8: {error}"
                            )),
                        },
                    };
                }

                return OptionalCommandOutput {
                    output: None,
                    diagnostic: Some(format!(
                        "{program} exited with {status}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                };
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return OptionalCommandOutput {
                    output: None,
                    diagnostic: Some(format!("{program} timed out after {timeout:?}")),
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return OptionalCommandOutput {
                    output: None,
                    diagnostic: Some(format!("failed while waiting for {program}: {error}")),
                };
            }
        }
    }
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
}
