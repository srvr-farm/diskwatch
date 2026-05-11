use crate::commands;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Zpool {
    pub name: String,
    pub size: String,
    pub allocated: String,
    pub free: String,
    pub health: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZpoolStatus {
    pub name: String,
    pub state: String,
    pub status: Option<String>,
}

pub fn parse_zpool_list(input: &str) -> Vec<Zpool> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').map(str::trim).collect();
            if fields.len() < 5 || fields[0].is_empty() {
                return None;
            }

            Some(Zpool {
                name: fields[0].to_string(),
                size: fields[1].to_string(),
                allocated: fields[2].to_string(),
                free: fields[3].to_string(),
                health: fields[4].to_string(),
                status: None,
            })
        })
        .collect()
}

pub fn parse_zpool_status(input: &str) -> Vec<ZpoolStatus> {
    let mut statuses = Vec::new();
    let mut current: Option<ZpoolStatus> = None;
    let mut status_lines: Vec<String> = Vec::new();
    let mut capturing_status = false;

    for line in input.lines() {
        let trimmed = line.trim();

        if let Some(name) = trimmed.strip_prefix("pool:") {
            finish_status_text(current.as_mut(), &mut status_lines);
            if let Some(status) = current.take() {
                statuses.push(status);
            }
            current = Some(ZpoolStatus {
                name: name.trim().to_string(),
                state: String::new(),
                status: None,
            });
            capturing_status = false;
            continue;
        }

        if let Some(state) = trimmed.strip_prefix("state:") {
            finish_status_text(current.as_mut(), &mut status_lines);
            if let Some(current) = current.as_mut() {
                current.state = state.trim().to_string();
            }
            capturing_status = false;
            continue;
        }

        if let Some(status) = trimmed.strip_prefix("status:") {
            finish_status_text(current.as_mut(), &mut status_lines);
            status_lines.push(status.trim().to_string());
            capturing_status = true;
            continue;
        }

        if capturing_status {
            if is_top_level_zpool_field(trimmed) {
                finish_status_text(current.as_mut(), &mut status_lines);
                capturing_status = false;
            } else if !trimmed.is_empty() {
                status_lines.push(trimmed.to_string());
            }
        }
    }

    finish_status_text(current.as_mut(), &mut status_lines);
    if let Some(status) = current {
        statuses.push(status);
    }

    statuses
}

pub fn collect(timeout: Duration) -> (Vec<Zpool>, Vec<String>) {
    collect_with_availability(timeout, commands::program_available("zpool"))
}

pub fn collect_budgeted(budget: &commands::OptionalCommandBudget) -> (Vec<Zpool>, Vec<String>) {
    if !commands::program_available("zpool") {
        return (Vec::new(), vec!["zpool not found".to_string()]);
    }

    collect_with_runner(|program, args| commands::run_optional_budgeted(program, args, budget))
}

fn collect_with_availability(
    timeout: Duration,
    zpool_available: bool,
) -> (Vec<Zpool>, Vec<String>) {
    if !zpool_available {
        return (Vec::new(), vec!["zpool not found".to_string()]);
    }

    collect_with_runner(|program, args| Some(commands::run_optional(program, args, timeout)))
}

fn collect_with_runner<F>(mut run: F) -> (Vec<Zpool>, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    let mut diagnostics = Vec::new();
    let Some(list_result) = run(
        "zpool",
        &["list", "-H", "-o", "name,size,alloc,free,health"],
    ) else {
        return (Vec::new(), diagnostics);
    };

    let mut pools = list_result
        .output
        .as_deref()
        .map(parse_zpool_list)
        .unwrap_or_default();
    if let Some(diagnostic) = list_result.diagnostic {
        diagnostics.push(diagnostic);
    }

    let Some(status_result) = run("zpool", &["status"]) else {
        return (pools, diagnostics);
    };

    let statuses = status_result
        .output
        .as_deref()
        .map(parse_zpool_status)
        .unwrap_or_default();
    if let Some(diagnostic) = status_result.diagnostic {
        diagnostics.push(diagnostic);
    }

    let statuses_by_name: HashMap<_, _> = statuses
        .into_iter()
        .map(|status| (status.name, status.status))
        .collect();

    for pool in &mut pools {
        pool.status = statuses_by_name.get(&pool.name).cloned().flatten();
    }

    (pools, diagnostics)
}

fn finish_status_text(current: Option<&mut ZpoolStatus>, status_lines: &mut Vec<String>) {
    if let Some(current) = current {
        if !status_lines.is_empty() {
            current.status = Some(status_lines.join(" "));
            status_lines.clear();
        }
    } else {
        status_lines.clear();
    }
}

fn is_top_level_zpool_field(trimmed: &str) -> bool {
    matches!(
        trimmed.split_once(':').map(|(name, _)| name),
        Some("action" | "see" | "scan" | "config" | "errors" | "pool" | "state")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zpool_list() {
        let input = "tank\t1.81T\t930G\t930G\tONLINE\n";
        let pools = parse_zpool_list(input);
        assert_eq!(pools[0].name, "tank");
        assert_eq!(pools[0].size, "1.81T");
        assert_eq!(pools[0].health, "ONLINE");
    }

    #[test]
    fn parses_zpool_status() {
        let input =
            "  pool: tank\n state: DEGRADED\nstatus: One or more devices could not be used.\n";
        let statuses = parse_zpool_status(input);
        assert_eq!(statuses[0].name, "tank");
        assert_eq!(statuses[0].state, "DEGRADED");
        assert!(statuses[0]
            .status
            .as_deref()
            .unwrap()
            .contains("devices could not be used"));
    }

    #[test]
    fn missing_zpool_reports_one_diagnostic() {
        let (pools, diagnostics) = collect_with_availability(Duration::from_secs(1), false);

        assert!(pools.is_empty());
        assert_eq!(diagnostics, ["zpool not found"]);
    }
}
