use crate::commands;
use std::collections::HashMap;
use std::time::Duration;

pub type KstatMap = HashMap<String, u64>;

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
pub struct ArcStats {
    pub raw: KstatMap,
    pub hit_ratio_percent: Option<f64>,
    pub miss_ratio_percent: Option<f64>,
    pub demand_data_hit_ratio_percent: Option<f64>,
    pub demand_metadata_hit_ratio_percent: Option<f64>,
    pub prefetch_data_hit_ratio_percent: Option<f64>,
    pub prefetch_metadata_hit_ratio_percent: Option<f64>,
    pub size_bytes: Option<u64>,
    pub target_bytes: Option<u64>,
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub compressed_size_bytes: Option<u64>,
    pub uncompressed_size_bytes: Option<u64>,
    pub data_size_bytes: Option<u64>,
    pub metadata_size_bytes: Option<u64>,
    pub dbuf_size_bytes: Option<u64>,
    pub dnode_size_bytes: Option<u64>,
    pub mru_size_bytes: Option<u64>,
    pub mfu_size_bytes: Option<u64>,
    pub l2_hit_ratio_percent: Option<f64>,
    pub l2_size_bytes: Option<u64>,
    pub l2_asize_bytes: Option<u64>,
    pub l2_read_bytes: Option<u64>,
    pub l2_write_bytes: Option<u64>,
    pub l2_writes_sent: Option<u64>,
    pub l2_writes_done: Option<u64>,
    pub l2_writes_error: Option<u64>,
    pub l2_cksum_bad: Option<u64>,
    pub l2_io_error: Option<u64>,
    pub memory_all_bytes: Option<u64>,
    pub memory_free_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub memory_throttle_count: Option<u64>,
    pub memory_direct_count: Option<u64>,
    pub memory_indirect_count: Option<u64>,
}

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

pub fn parse_kstat_map(input: &str) -> KstatMap {
    input
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let _kind = fields.next()?;
            let data = fields.next()?;
            let value = data.parse().ok()?;
            Some((name.to_string(), value))
        })
        .collect()
}

pub fn parse_arcstats(input: &str) -> Option<ArcStats> {
    let raw = parse_kstat_map(input);
    Some(ArcStats {
        hit_ratio_percent: ratio_from_keys(&raw, "hits", "misses"),
        miss_ratio_percent: ratio_from_keys(&raw, "misses", "hits"),
        demand_data_hit_ratio_percent: ratio_from_keys(
            &raw,
            "demand_data_hits",
            "demand_data_misses",
        ),
        demand_metadata_hit_ratio_percent: ratio_from_keys(
            &raw,
            "demand_metadata_hits",
            "demand_metadata_misses",
        ),
        prefetch_data_hit_ratio_percent: ratio_from_keys(
            &raw,
            "prefetch_data_hits",
            "prefetch_data_misses",
        ),
        prefetch_metadata_hit_ratio_percent: ratio_from_keys(
            &raw,
            "prefetch_metadata_hits",
            "prefetch_metadata_misses",
        ),
        size_bytes: raw.get("size").copied(),
        target_bytes: raw.get("c").copied(),
        min_bytes: raw.get("c_min").copied(),
        max_bytes: raw.get("c_max").copied(),
        compressed_size_bytes: raw.get("compressed_size").copied(),
        uncompressed_size_bytes: raw.get("uncompressed_size").copied(),
        data_size_bytes: raw.get("data_size").copied(),
        metadata_size_bytes: raw.get("metadata_size").copied(),
        dbuf_size_bytes: raw.get("dbuf_size").copied(),
        dnode_size_bytes: raw.get("dnode_size").copied(),
        mru_size_bytes: raw.get("mru_size").copied(),
        mfu_size_bytes: raw.get("mfu_size").copied(),
        l2_hit_ratio_percent: ratio_from_keys(&raw, "l2_hits", "l2_misses"),
        l2_size_bytes: raw.get("l2_size").copied(),
        l2_asize_bytes: raw.get("l2_asize").copied(),
        l2_read_bytes: raw.get("l2_read_bytes").copied(),
        l2_write_bytes: raw.get("l2_write_bytes").copied(),
        l2_writes_sent: raw.get("l2_writes_sent").copied(),
        l2_writes_done: raw.get("l2_writes_done").copied(),
        l2_writes_error: raw.get("l2_writes_error").copied(),
        l2_cksum_bad: raw.get("l2_cksum_bad").copied(),
        l2_io_error: raw.get("l2_io_error").copied(),
        memory_all_bytes: raw.get("memory_all_bytes").copied(),
        memory_free_bytes: raw.get("memory_free_bytes").copied(),
        memory_available_bytes: raw.get("memory_available_bytes").copied(),
        memory_throttle_count: raw.get("memory_throttle_count").copied(),
        memory_direct_count: raw.get("memory_direct_count").copied(),
        memory_indirect_count: raw.get("memory_indirect_count").copied(),
        raw,
    })
}

fn ratio_from_keys(stats: &KstatMap, numerator_key: &str, other_key: &str) -> Option<f64> {
    let numerator = *stats.get(numerator_key)?;
    let other = *stats.get(other_key)?;
    ratio_percent(numerator, numerator.checked_add(other)?)
}

fn ratio_percent(numerator: u64, denominator: u64) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some((numerator as f64 / denominator as f64) * 100.0)
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

    #[test]
    fn parses_kstat_name_type_data_rows() {
        let input = "\
9 1 0x01 147 39984 4452996882 165618915963102
name                            type data
hits                            4    90
misses                          4    10
size                            4    1024
l2_hits                         4    5
";
        let stats = parse_kstat_map(input);

        assert_eq!(stats.get("hits"), Some(&90));
        assert_eq!(stats.get("misses"), Some(&10));
        assert_eq!(stats.get("size"), Some(&1024));
        assert_eq!(stats.get("l2_hits"), Some(&5));
    }

    #[test]
    fn derives_arc_and_l2arc_ratios() {
        let stats = parse_arcstats(
            "\
name type data
hits 4 90
misses 4 10
demand_data_hits 4 45
demand_data_misses 4 5
demand_metadata_hits 4 40
demand_metadata_misses 4 10
prefetch_data_hits 4 4
prefetch_data_misses 4 6
prefetch_metadata_hits 4 1
prefetch_metadata_misses 4 9
c 4 2048
c_min 4 1024
c_max 4 4096
size 4 1536
compressed_size 4 1400
uncompressed_size 4 3000
data_size 4 512
metadata_size 4 256
dbuf_size 4 64
dnode_size 4 32
mru_size 4 128
mfu_size 4 256
l2_hits 4 5
l2_misses 4 15
l2_size 4 8192
l2_asize 4 4096
l2_read_bytes 4 1000
l2_write_bytes 4 2000
l2_writes_sent 4 7
l2_writes_done 4 6
l2_writes_error 4 1
l2_cksum_bad 4 2
l2_io_error 4 3
memory_all_bytes 4 100000
memory_free_bytes 4 20000
memory_available_bytes 4 50000
memory_throttle_count 4 4
memory_direct_count 4 5
memory_indirect_count 4 6
",
        )
        .unwrap();

        assert_eq!(stats.hit_ratio_percent, Some(90.0));
        assert_eq!(stats.l2_hit_ratio_percent, Some(25.0));
        assert_eq!(stats.size_bytes, Some(1536));
        assert_eq!(stats.compressed_size_bytes, Some(1400));
        assert_eq!(stats.uncompressed_size_bytes, Some(3000));
        assert_eq!(stats.dbuf_size_bytes, Some(64));
        assert_eq!(stats.dnode_size_bytes, Some(32));
        assert_eq!(stats.l2_size_bytes, Some(8192));
        assert_eq!(stats.l2_asize_bytes, Some(4096));
        assert_eq!(stats.l2_writes_sent, Some(7));
        assert_eq!(stats.l2_writes_done, Some(6));
        assert_eq!(stats.l2_cksum_bad, Some(2));
        assert_eq!(stats.l2_io_error, Some(3));
        assert_eq!(stats.memory_all_bytes, Some(100000));
        assert_eq!(stats.memory_free_bytes, Some(20000));
        assert_eq!(stats.memory_available_bytes, Some(50000));
        assert_eq!(stats.memory_throttle_count, Some(4));
        assert_eq!(stats.memory_direct_count, Some(5));
        assert_eq!(stats.memory_indirect_count, Some(6));
    }
}
