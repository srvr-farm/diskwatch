use crate::commands;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsSnapshot {
    pub deep: bool,
    pub pools: Vec<ZfsPool>,
    pub arc: Option<ArcStats>,
    pub datasets: Vec<ZfsDataset>,
    pub kernel: ZfsKernelStats,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsPool {
    pub name: String,
    pub size_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub capacity_percent: Option<f64>,
    pub dedup_ratio: Option<f64>,
    pub fragmentation_percent: Option<f64>,
    pub health: String,
    pub altroot: Option<String>,
    pub autotrim: Option<String>,
    pub scan: Option<String>,
    pub status: Option<String>,
    pub action: Option<String>,
    pub errors: Option<String>,
    pub topology: Vec<ZfsTopologyNode>,
    pub vdev_io: Vec<ZfsVdevIo>,
    pub pool_kstats: ZfsPoolKstats,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZpoolStatus {
    pub name: String,
    pub state: String,
    pub status: Option<String>,
    pub action: Option<String>,
    pub scan: Option<String>,
    pub errors: Option<String>,
    pub topology: Vec<ZfsTopologyNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZfsTopologyNode {
    pub name: String,
    pub role: ZfsTopologyRole,
    pub depth: usize,
    pub state: Option<String>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
    pub checksum_errors: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZfsTopologyRole {
    #[default]
    Pool,
    Vdev,
    Disk,
    SpecialGroup,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArcStats;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsDataset;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsVdevIo;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsPoolKstats;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsKernelStats;

pub fn parse_zpool_list(input: &str) -> Vec<ZfsPool> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').map(str::trim).collect();
            if fields.len() < 5 || fields[0].is_empty() {
                return None;
            }

            if fields.len() >= 10 {
                Some(ZfsPool {
                    name: fields[0].to_string(),
                    size_bytes: parse_optional_u64(fields[1]),
                    allocated_bytes: parse_optional_u64(fields[2]),
                    free_bytes: parse_optional_u64(fields[3]),
                    capacity_percent: parse_percent(fields[4]),
                    dedup_ratio: parse_ratio(fields[5]),
                    fragmentation_percent: parse_percent(fields[6]),
                    health: fields[7].to_string(),
                    altroot: dash_to_none(fields[8]),
                    autotrim: dash_to_none(fields[9]),
                    ..ZfsPool::default()
                })
            } else {
                Some(ZfsPool {
                    name: fields[0].to_string(),
                    size_bytes: parse_optional_u64(fields[1]),
                    allocated_bytes: parse_optional_u64(fields[2]),
                    free_bytes: parse_optional_u64(fields[3]),
                    health: fields[4].to_string(),
                    ..ZfsPool::default()
                })
            }
        })
        .collect()
}

pub fn parse_zpool_status(input: &str) -> Vec<ZpoolStatus> {
    let mut statuses = Vec::new();
    let mut current: Option<ZpoolStatus> = None;
    let mut captured_lines: Vec<String> = Vec::new();
    let mut captured_field: Option<CapturedStatusField> = None;
    let mut in_config = false;

    for line in input.lines() {
        let trimmed = line.trim();

        if let Some(name) = trimmed.strip_prefix("pool:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            if let Some(status) = current.take() {
                statuses.push(status);
            }
            current = Some(ZpoolStatus {
                name: name.trim().to_string(),
                ..ZpoolStatus::default()
            });
            in_config = false;
            continue;
        }

        if let Some(state) = trimmed.strip_prefix("state:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            if let Some(current) = current.as_mut() {
                current.state = state.trim().to_string();
            }
            in_config = false;
            continue;
        }

        if let Some(status) = trimmed.strip_prefix("status:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            captured_lines.push(status.trim().to_string());
            captured_field = Some(CapturedStatusField::Status);
            in_config = false;
            continue;
        }

        if let Some(action) = trimmed.strip_prefix("action:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            captured_lines.push(action.trim().to_string());
            captured_field = Some(CapturedStatusField::Action);
            in_config = false;
            continue;
        }

        if let Some(scan) = trimmed.strip_prefix("scan:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            if let Some(current) = current.as_mut() {
                current.scan = dash_to_none(scan.trim());
            }
            in_config = false;
            continue;
        }

        if trimmed == "config:" {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            in_config = true;
            continue;
        }

        if let Some(errors) = trimmed.strip_prefix("errors:") {
            finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            if let Some(current) = current.as_mut() {
                current.errors = dash_to_none(errors.trim());
            }
            in_config = false;
            continue;
        }

        if captured_field.is_some() {
            if is_top_level_zpool_field(trimmed) {
                finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
            } else if !trimmed.is_empty() {
                captured_lines.push(trimmed.to_string());
                continue;
            }
        }

        if in_config {
            if let Some(current) = current.as_mut() {
                if let Some(node) = parse_topology_line(line, &current.name) {
                    current.topology.push(node);
                }
            }
        }
    }

    finish_captured_text(current.as_mut(), captured_field.take(), &mut captured_lines);
    if let Some(status) = current {
        statuses.push(status);
    }

    statuses
}

pub fn collect(timeout: Duration) -> (ZfsSnapshot, Vec<String>) {
    collect_with_availability(timeout, commands::program_available("zpool"))
}

pub fn collect_budgeted(budget: &commands::OptionalCommandBudget) -> (ZfsSnapshot, Vec<String>) {
    if !commands::program_available("zpool") {
        return (ZfsSnapshot::default(), vec!["zpool not found".to_string()]);
    }

    collect_with_runner(|program, args| commands::run_optional_budgeted(program, args, budget))
}

fn collect_with_availability(
    timeout: Duration,
    zpool_available: bool,
) -> (ZfsSnapshot, Vec<String>) {
    if !zpool_available {
        return (ZfsSnapshot::default(), vec!["zpool not found".to_string()]);
    }

    collect_with_runner(|program, args| Some(commands::run_optional(program, args, timeout)))
}

fn collect_with_runner<F>(mut run: F) -> (ZfsSnapshot, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    let mut diagnostics = Vec::new();
    let Some(list_result) = run(
        "zpool",
        &[
            "list",
            "-Hp",
            "-o",
            "name,size,allocated,free,capacity,dedupratio,fragmentation,health,altroot,autotrim",
        ],
    ) else {
        return (ZfsSnapshot::default(), diagnostics);
    };

    let mut pools = list_result
        .output
        .as_deref()
        .map(parse_zpool_list)
        .unwrap_or_default();
    if let Some(diagnostic) = list_result.diagnostic {
        diagnostics.push(diagnostic);
    }

    let Some(status_result) = run("zpool", &["status", "-P"]) else {
        return (
            ZfsSnapshot {
                pools,
                ..ZfsSnapshot::default()
            },
            diagnostics,
        );
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
        .map(|status| (status.name.clone(), status))
        .collect();

    for pool in &mut pools {
        if let Some(status) = statuses_by_name.get(&pool.name) {
            pool.status = status.status.clone();
            pool.action = status.action.clone();
            pool.scan = status.scan.clone();
            pool.errors = status.errors.clone();
            pool.topology = status.topology.clone();
        }
    }

    (
        ZfsSnapshot {
            pools,
            ..ZfsSnapshot::default()
        },
        diagnostics,
    )
}

#[derive(Debug, Clone, Copy)]
enum CapturedStatusField {
    Status,
    Action,
}

fn finish_captured_text(
    current: Option<&mut ZpoolStatus>,
    field: Option<CapturedStatusField>,
    captured_lines: &mut Vec<String>,
) {
    let Some(field) = field else {
        captured_lines.clear();
        return;
    };
    let Some(current) = current else {
        captured_lines.clear();
        return;
    };
    if captured_lines.is_empty() {
        return;
    }

    let value = Some(captured_lines.join(" "));
    match field {
        CapturedStatusField::Status => current.status = value,
        CapturedStatusField::Action => current.action = value,
    }
    captured_lines.clear();
}

fn parse_topology_line(line: &str, pool_name: &str) -> Option<ZfsTopologyNode> {
    let without_tabs = line.trim_start_matches('\t');
    let trimmed = without_tabs.trim();
    if trimmed.is_empty() || trimmed.starts_with("NAME ") {
        return None;
    }

    let fields: Vec<_> = without_tabs.split_whitespace().collect();
    let name = *fields.first()?;
    if name == "NAME" {
        return None;
    }

    let leading_spaces = without_tabs
        .chars()
        .take_while(|character| *character == ' ')
        .count();
    let depth = leading_spaces / 2;
    let role = topology_role(name, pool_name, depth, fields.len());

    Some(ZfsTopologyNode {
        name: name.to_string(),
        role,
        depth,
        state: fields.get(1).and_then(|value| dash_to_none(value)),
        read_errors: fields.get(2).and_then(|value| parse_optional_u64(value)),
        write_errors: fields.get(3).and_then(|value| parse_optional_u64(value)),
        checksum_errors: fields.get(4).and_then(|value| parse_optional_u64(value)),
    })
}

fn topology_role(name: &str, pool_name: &str, depth: usize, field_count: usize) -> ZfsTopologyRole {
    if is_special_topology_group(name) && field_count == 1 {
        return ZfsTopologyRole::SpecialGroup;
    }
    if name == pool_name && depth == 0 {
        return ZfsTopologyRole::Pool;
    }
    if looks_like_leaf_device(name) {
        return ZfsTopologyRole::Disk;
    }
    if depth <= 1 {
        ZfsTopologyRole::Vdev
    } else {
        ZfsTopologyRole::Disk
    }
}

fn looks_like_leaf_device(name: &str) -> bool {
    name.starts_with("/dev/")
        || name.starts_with("/dev/disk/")
        || name.starts_with("ata-")
        || name.starts_with("scsi-")
        || name.starts_with("nvme-")
        || name.starts_with("wwn-")
        || name.starts_with("eui.")
}

fn is_special_topology_group(name: &str) -> bool {
    matches!(
        name,
        "cache" | "logs" | "log" | "spares" | "special" | "dedup" | "replacing"
    )
}

fn parse_optional_u64(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn parse_percent(value: &str) -> Option<f64> {
    let trimmed = value.trim().trim_end_matches('%');
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn parse_ratio(value: &str) -> Option<f64> {
    let trimmed = value.trim().trim_end_matches('x');
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn dash_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_top_level_zpool_field(trimmed: &str) -> bool {
    matches!(
        trimmed.split_once(':').map(|(name, _)| name),
        Some("action" | "see" | "scan" | "config" | "errors" | "pool" | "state" | "status")
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
        assert_eq!(pools[0].size_bytes, None);
        assert_eq!(pools[0].health, "ONLINE");
    }

    #[test]
    fn parses_zpool_list_properties_as_numbers() {
        let input =
            "data\t29961691856896\t10665749323776\t19295942533120\t35\t1.00\t1\tONLINE\t-\toff\n";
        let snapshot = parse_zpool_list(input);
        let pool = &snapshot[0];

        assert_eq!(pool.name, "data");
        assert_eq!(pool.size_bytes, Some(29_961_691_856_896));
        assert_eq!(pool.allocated_bytes, Some(10_665_749_323_776));
        assert_eq!(pool.free_bytes, Some(19_295_942_533_120));
        assert_eq!(pool.capacity_percent, Some(35.0));
        assert_eq!(pool.dedup_ratio, Some(1.0));
        assert_eq!(pool.fragmentation_percent, Some(1.0));
        assert_eq!(pool.health, "ONLINE");
        assert_eq!(pool.autotrim.as_deref(), Some("off"));
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
    fn parses_zpool_status_scan_errors_and_topology() {
        let input = "\
  pool: data
 state: ONLINE
status: One or more devices has experienced an unrecoverable error.
action: Replace the faulted device, or use 'zpool clear'.
  scan: resilvered 97.1M in 00:23:33 with 0 errors on Sun May 10 17:56:02 2026
config:

\tNAME                                                               STATE     READ WRITE CKSUM
\tdata                                                               ONLINE       0     0     0
\t  raidz2-0                                                         ONLINE       0     0     0
\t    /dev/sdb                                                       ONLINE       0     0     0
\tcache
\t  /dev/disk/by-id/nvme-eui.000000000000000100a075254d4fbfe0-part1  ONLINE       0     0     0

errors: No known data errors
";

        let statuses = parse_zpool_status(input);
        let status = &statuses[0];
        assert_eq!(status.name, "data");
        assert_eq!(status.state, "ONLINE");
        assert!(status
            .status
            .as_deref()
            .unwrap()
            .contains("unrecoverable error"));
        assert!(status.action.as_deref().unwrap().contains("zpool clear"));
        assert!(status.scan.as_deref().unwrap().contains("resilvered 97.1M"));
        assert_eq!(status.errors.as_deref(), Some("No known data errors"));
        assert!(status
            .topology
            .iter()
            .any(|node| node.name == "raidz2-0" && node.depth == 1));
        assert!(status
            .topology
            .iter()
            .any(|node| node.name == "/dev/sdb" && node.depth == 2));
        assert!(status
            .topology
            .iter()
            .any(|node| node.name == "cache" && node.role == ZfsTopologyRole::SpecialGroup));
    }

    #[test]
    fn missing_zpool_reports_one_diagnostic() {
        let (snapshot, diagnostics) = collect_with_availability(Duration::from_secs(1), false);

        assert!(snapshot.pools.is_empty());
        assert_eq!(diagnostics, ["zpool not found"]);
    }
}
