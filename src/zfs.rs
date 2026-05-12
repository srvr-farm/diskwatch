use crate::commands;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

pub type KstatMap = HashMap<String, u64>;

const DEFAULT_KSTAT_ROOT: &str = "/proc/spl/kstat/zfs";
const KSTAT_FILE_LIMIT: usize = 256 * 1024;
const TXGS_FILE_LIMIT: usize = 512 * 1024;
const IOSTAT_MINIMUM_BUDGET: Duration = Duration::from_millis(1100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZfsCollectionMode {
    Shallow,
    Deep,
}

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
pub struct ZfsDataset {
    pub name: String,
    pub used_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub referenced_bytes: Option<u64>,
    pub mountpoint: Option<String>,
    pub compression: Option<String>,
    pub compressratio: Option<f64>,
    pub used_snap_bytes: Option<u64>,
    pub used_dataset_bytes: Option<u64>,
    pub used_refreservation_bytes: Option<u64>,
    pub used_child_bytes: Option<u64>,
    pub properties: HashMap<String, ZfsProperty>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZfsProperty {
    pub value: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsVdevIo {
    pub name: String,
    pub allocated_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub read_ops_per_sec: Option<f64>,
    pub write_ops_per_sec: Option<f64>,
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    pub total_wait_read_ns: Option<u64>,
    pub total_wait_write_ns: Option<u64>,
    pub disk_wait_read_ns: Option<u64>,
    pub disk_wait_write_ns: Option<u64>,
    pub sync_queue_wait_read_ns: Option<u64>,
    pub sync_queue_wait_write_ns: Option<u64>,
    pub async_queue_wait_read_ns: Option<u64>,
    pub async_queue_wait_write_ns: Option<u64>,
    pub scrub_wait_ns: Option<u64>,
    pub trim_wait_ns: Option<u64>,
    pub rebuild_wait_ns: Option<u64>,
    pub sync_read_queue_pending: Option<u64>,
    pub sync_read_queue_active: Option<u64>,
    pub sync_write_queue_pending: Option<u64>,
    pub sync_write_queue_active: Option<u64>,
    pub async_read_queue_pending: Option<u64>,
    pub async_read_queue_active: Option<u64>,
    pub async_write_queue_pending: Option<u64>,
    pub async_write_queue_active: Option<u64>,
    pub scrub_read_queue_pending: Option<u64>,
    pub scrub_read_queue_active: Option<u64>,
    pub trim_write_queue_pending: Option<u64>,
    pub trim_write_queue_active: Option<u64>,
    pub rebuild_write_queue_pending: Option<u64>,
    pub rebuild_write_queue_active: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsPoolKstats;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfsKernelStats {
    pub dbuf: Option<DbufStats>,
    pub dnode: Option<DnodeStats>,
    pub zil: Option<ZilStats>,
    pub zfetch: Option<ZfetchStats>,
    pub abd: Option<AbdStats>,
    pub txg: Option<TxgSummary>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DbufStats {
    pub cache_size_bytes: Option<u64>,
    pub cache_target_bytes: Option<u64>,
    pub hash_hits: Option<u64>,
    pub hash_misses: Option<u64>,
    pub cache_total_evicts: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DnodeStats {
    pub hold_alloc_hits: Option<u64>,
    pub hold_alloc_misses: Option<u64>,
    pub allocate: Option<u64>,
    pub buf_evict: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZilStats {
    pub commit_count: Option<u64>,
    pub itx_count: Option<u64>,
    pub itx_metaslab_normal_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZfetchStats {
    pub hits: Option<u64>,
    pub misses: Option<u64>,
    pub io_issued: Option<u64>,
    pub io_active: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbdStats {
    pub linear_count: Option<u64>,
    pub linear_data_size_bytes: Option<u64>,
    pub scatter_count: Option<u64>,
    pub scatter_data_size_bytes: Option<u64>,
    pub scatter_page_alloc_retry: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TxgSummary {
    pub latest_txg: Option<u64>,
    pub latest_dirty_bytes: Option<u64>,
    pub latest_read_bytes: Option<u64>,
    pub latest_written_bytes: Option<u64>,
    pub latest_reads: Option<u64>,
    pub latest_writes: Option<u64>,
}

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
    collect_with_availability(
        timeout,
        commands::program_available("zpool"),
        ZfsCollectionMode::Shallow,
    )
}

pub fn collect_budgeted(
    budget: &commands::OptionalCommandBudget,
    mode: ZfsCollectionMode,
) -> (ZfsSnapshot, Vec<String>) {
    if !commands::program_available("zpool") {
        return (ZfsSnapshot::default(), vec!["zpool not found".to_string()]);
    }

    collect_with_runner_budget_and_roots(
        mode,
        budget,
        Path::new(DEFAULT_KSTAT_ROOT),
        |program, args| commands::run_optional_budgeted(program, args, budget),
    )
}

fn collect_with_availability(
    timeout: Duration,
    zpool_available: bool,
    mode: ZfsCollectionMode,
) -> (ZfsSnapshot, Vec<String>) {
    if !zpool_available {
        return (ZfsSnapshot::default(), vec!["zpool not found".to_string()]);
    }

    let budget = commands::OptionalCommandBudget::new(timeout, timeout);
    collect_with_runner_budget_and_roots(
        mode,
        &budget,
        Path::new(DEFAULT_KSTAT_ROOT),
        |program, args| Some(commands::run_optional(program, args, timeout)),
    )
}

#[cfg(test)]
fn collect_with_runner_and_roots<F, P>(
    mode: ZfsCollectionMode,
    kstat_root: P,
    run: F,
) -> (ZfsSnapshot, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
    P: AsRef<Path>,
{
    let budget = commands::OptionalCommandBudget::new(
        Duration::from_millis(2500),
        Duration::from_millis(1500),
    );
    collect_with_runner_budget_and_roots(mode, &budget, kstat_root, run)
}

fn collect_with_runner_budget_and_roots<F, P>(
    mode: ZfsCollectionMode,
    budget: &commands::OptionalCommandBudget,
    kstat_root: P,
    mut run: F,
) -> (ZfsSnapshot, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
    P: AsRef<Path>,
{
    let mut diagnostics = Vec::new();
    let mut snapshot = ZfsSnapshot {
        deep: mode == ZfsCollectionMode::Deep,
        ..ZfsSnapshot::default()
    };

    let Some(list_result) = run(
        "zpool",
        &[
            "list",
            "-Hp",
            "-o",
            "name,size,allocated,free,capacity,dedupratio,fragmentation,health,altroot,autotrim",
        ],
    ) else {
        return (snapshot, diagnostics);
    };

    snapshot.pools = list_result
        .output
        .as_deref()
        .map(parse_zpool_list)
        .unwrap_or_default();
    if let Some(diagnostic) = list_result.diagnostic {
        diagnostics.push(diagnostic);
    }

    let Some(status_result) = run("zpool", &["status", "-P"]) else {
        collect_arcstats(kstat_root.as_ref(), &mut snapshot, &mut diagnostics);
        return (snapshot, diagnostics);
    };

    let statuses = status_result
        .output
        .as_deref()
        .map(parse_zpool_status)
        .unwrap_or_default();
    if let Some(diagnostic) = status_result.diagnostic {
        diagnostics.push(diagnostic);
    }

    apply_statuses_to_pools(&mut snapshot.pools, statuses);
    collect_arcstats(kstat_root.as_ref(), &mut snapshot, &mut diagnostics);

    if mode == ZfsCollectionMode::Shallow {
        return (snapshot, diagnostics);
    }

    let (kernel, kernel_diagnostics) = collect_kstats(kstat_root.as_ref(), &snapshot.pools);
    snapshot.kernel = kernel;
    diagnostics.extend(kernel_diagnostics);

    collect_deep_datasets(&mut snapshot, &mut diagnostics, budget, &mut run);
    collect_deep_iostat(&mut snapshot, &mut diagnostics, budget, &mut run);

    (snapshot, diagnostics)
}

fn apply_statuses_to_pools(pools: &mut [ZfsPool], statuses: Vec<ZpoolStatus>) {
    let statuses_by_name: HashMap<_, _> = statuses
        .into_iter()
        .map(|status| (status.name.clone(), status))
        .collect();

    for pool in pools {
        if let Some(status) = statuses_by_name.get(&pool.name) {
            pool.status = status.status.clone();
            pool.action = status.action.clone();
            pool.scan = status.scan.clone();
            pool.errors = status.errors.clone();
            pool.topology = status.topology.clone();
        }
    }
}

fn collect_arcstats(kstat_root: &Path, snapshot: &mut ZfsSnapshot, diagnostics: &mut Vec<String>) {
    if let Some(input) = read_optional_kstat(kstat_root, "arcstats", KSTAT_FILE_LIMIT, diagnostics)
    {
        snapshot.arc = parse_arcstats(&input);
    }
}

fn collect_deep_datasets<F>(
    snapshot: &mut ZfsSnapshot,
    diagnostics: &mut Vec<String>,
    budget: &commands::OptionalCommandBudget,
    run: &mut F,
) where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    if snapshot.pools.is_empty() {
        return;
    }
    if budget.remaining_timeout().is_none() {
        diagnostics.push("zfs list skipped: deep ZFS budget exhausted".to_string());
        return;
    }

    let pool_names: Vec<_> = snapshot
        .pools
        .iter()
        .map(|pool| pool.name.clone())
        .collect();
    let list_args = zfs_list_args(&pool_names);
    let Some(list_result) = run_with_owned_args(run, "zfs", &list_args) else {
        diagnostics.push("zfs list skipped: deep ZFS budget exhausted".to_string());
        return;
    };
    if let Some(diagnostic) = list_result.diagnostic {
        diagnostics.push(diagnostic);
    }
    snapshot.datasets = list_result
        .output
        .as_deref()
        .map(parse_zfs_list)
        .unwrap_or_default();

    if budget.remaining_timeout().is_none() {
        diagnostics.push("zfs get skipped: deep ZFS budget exhausted".to_string());
        return;
    }

    let get_args = zfs_get_args(&pool_names);
    let Some(get_result) = run_with_owned_args(run, "zfs", &get_args) else {
        diagnostics.push("zfs get skipped: deep ZFS budget exhausted".to_string());
        return;
    };
    if let Some(diagnostic) = get_result.diagnostic {
        diagnostics.push(diagnostic);
    }
    if let Some(output) = get_result.output.as_deref() {
        apply_zfs_get_properties(&mut snapshot.datasets, output);
    }
}

fn collect_deep_iostat<F>(
    snapshot: &mut ZfsSnapshot,
    diagnostics: &mut Vec<String>,
    budget: &commands::OptionalCommandBudget,
    run: &mut F,
) where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    if snapshot.pools.is_empty() {
        return;
    }
    if !budget_has_iostat_time(budget) {
        diagnostics.push("zpool iostat skipped: deep ZFS budget exhausted".to_string());
        return;
    }

    let pool_names: Vec<_> = snapshot
        .pools
        .iter()
        .map(|pool| pool.name.clone())
        .collect();
    let args = zpool_iostat_args(&pool_names);
    let Some(result) = run_with_owned_args(run, "zpool", &args) else {
        diagnostics.push("zpool iostat skipped: deep ZFS budget exhausted".to_string());
        return;
    };
    if let Some(diagnostic) = result.diagnostic {
        diagnostics.push(normalize_iostat_diagnostic(&diagnostic));
    }
    if let Some(output) = result.output.as_deref() {
        attach_iostat_rows(&mut snapshot.pools, parse_zpool_iostat(output));
    }
}

fn zfs_list_args(pool_names: &[String]) -> Vec<String> {
    let mut args = vec![
        "list".to_string(),
        "-Hp".to_string(),
        "-r".to_string(),
        "-t".to_string(),
        "filesystem,volume".to_string(),
        "-o".to_string(),
        "name,used,available,referenced,mountpoint,compression,compressratio,usedsnap,usedds,usedrefreserv,usedchild".to_string(),
    ];
    args.extend(pool_names.iter().cloned());
    args
}

fn zfs_get_args(pool_names: &[String]) -> Vec<String> {
    let mut args = vec![
        "get".to_string(),
        "-Hp".to_string(),
        "-r".to_string(),
        "-t".to_string(),
        "filesystem,volume".to_string(),
        "-o".to_string(),
        "name,property,value,source".to_string(),
        "recordsize,primarycache,secondarycache,sync,logbias,atime,dedup,checksum,encryption,readonly"
            .to_string(),
    ];
    args.extend(pool_names.iter().cloned());
    args
}

fn zpool_iostat_args(pool_names: &[String]) -> Vec<String> {
    let mut args = vec![
        "iostat".to_string(),
        "-Hp".to_string(),
        "-vlq".to_string(),
        "-y".to_string(),
    ];
    args.extend(pool_names.iter().cloned());
    args.push("1".to_string());
    args.push("1".to_string());
    args
}

fn run_with_owned_args<F>(
    run: &mut F,
    program: &str,
    args: &[String],
) -> Option<commands::OptionalCommandOutput>
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    let arg_refs: Vec<_> = args.iter().map(String::as_str).collect();
    run(program, &arg_refs)
}

fn budget_has_iostat_time(budget: &commands::OptionalCommandBudget) -> bool {
    budget
        .remaining_timeout()
        .is_some_and(|remaining| remaining >= IOSTAT_MINIMUM_BUDGET)
}

fn normalize_iostat_diagnostic(diagnostic: &str) -> String {
    if let Some(rest) = diagnostic.strip_prefix("zpool timed out") {
        format!("zpool iostat timed out{rest}")
    } else if diagnostic.contains("timed out") && !diagnostic.starts_with("zpool iostat") {
        format!("zpool iostat {diagnostic}")
    } else {
        diagnostic.to_string()
    }
}

fn attach_iostat_rows(pools: &mut [ZfsPool], rows: Vec<ZfsVdevIo>) {
    let pool_names: Vec<_> = pools.iter().map(|pool| pool.name.clone()).collect();
    let mut current_pool: Option<String> = None;

    for row in rows {
        if pool_names.iter().any(|name| name == &row.name) {
            current_pool = Some(row.name.clone());
        }
        let Some(pool_name) = current_pool.as_deref() else {
            continue;
        };
        if let Some(pool) = pools.iter_mut().find(|pool| pool.name == pool_name) {
            pool.vdev_io.push(row);
        }
    }
}

fn collect_kstats(kstat_root: &Path, pools: &[ZfsPool]) -> (ZfsKernelStats, Vec<String>) {
    let mut diagnostics = Vec::new();
    let dbuf = read_optional_kstat(kstat_root, "dbufstats", KSTAT_FILE_LIMIT, &mut diagnostics)
        .and_then(|input| parse_dbufstats(&input));
    let dnode = read_optional_kstat(kstat_root, "dnodestats", KSTAT_FILE_LIMIT, &mut diagnostics)
        .and_then(|input| parse_dnodestats(&input));
    let zil = read_optional_kstat(kstat_root, "zil", KSTAT_FILE_LIMIT, &mut diagnostics)
        .and_then(|input| parse_zil(&input));
    let zfetch = read_optional_kstat(
        kstat_root,
        "zfetchstats",
        KSTAT_FILE_LIMIT,
        &mut diagnostics,
    )
    .and_then(|input| parse_zfetchstats(&input));
    let abd = read_optional_kstat(kstat_root, "abdstats", KSTAT_FILE_LIMIT, &mut diagnostics)
        .and_then(|input| parse_abdstats(&input));
    let mut kernel = ZfsKernelStats {
        dbuf,
        dnode,
        zil,
        zfetch,
        abd,
        ..ZfsKernelStats::default()
    };

    for pool in pools {
        let relative = format!("{}/txgs", pool.name);
        if let Some(input) =
            read_optional_kstat(kstat_root, &relative, TXGS_FILE_LIMIT, &mut diagnostics)
        {
            kernel.txg = parse_txgs(&input);
            break;
        }
    }

    (kernel, diagnostics)
}

fn read_optional_kstat(
    root: &Path,
    relative: &str,
    max_bytes: usize,
    diagnostics: &mut Vec<String>,
) -> Option<String> {
    let path = root.join(relative);
    if !path.exists() {
        return None;
    }

    match read_kstat_file(&path, max_bytes) {
        Ok(contents) => Some(contents),
        Err(error) => {
            diagnostics.push(format!("zfs kstat {relative} unreadable: {error}"));
            None
        }
    }
}

fn read_kstat_file(path: &Path, max_bytes: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut contents = String::new();
    file.take(max_bytes as u64)
        .read_to_string(&mut contents)
        .map_err(|error| error.to_string())?;
    Ok(contents)
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

fn parse_optional_f64(value: &str) -> Option<f64> {
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

pub fn parse_zfs_list(input: &str) -> Vec<ZfsDataset> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').map(str::trim).collect();
            if fields.len() < 11 || fields[0].is_empty() {
                return None;
            }

            Some(ZfsDataset {
                name: fields[0].to_string(),
                used_bytes: parse_optional_u64(fields[1]),
                available_bytes: parse_optional_u64(fields[2]),
                referenced_bytes: parse_optional_u64(fields[3]),
                mountpoint: dash_to_none(fields[4]),
                compression: dash_to_none(fields[5]),
                compressratio: parse_ratio(fields[6]),
                used_snap_bytes: parse_optional_u64(fields[7]),
                used_dataset_bytes: parse_optional_u64(fields[8]),
                used_refreservation_bytes: parse_optional_u64(fields[9]),
                used_child_bytes: parse_optional_u64(fields[10]),
                properties: HashMap::new(),
            })
        })
        .collect()
}

pub fn apply_zfs_get_properties(datasets: &mut [ZfsDataset], input: &str) {
    let indexes_by_name: HashMap<_, _> = datasets
        .iter()
        .enumerate()
        .map(|(index, dataset)| (dataset.name.clone(), index))
        .collect();

    for line in input.lines() {
        let fields: Vec<_> = line.split('\t').map(str::trim).collect();
        if fields.len() < 4 {
            continue;
        }
        let Some(index) = indexes_by_name.get(fields[0]).copied() else {
            continue;
        };

        datasets[index].properties.insert(
            fields[1].to_string(),
            ZfsProperty {
                value: fields[2].to_string(),
                source: dash_to_none(fields[3]),
            },
        );
    }
}

pub fn parse_zpool_iostat(input: &str) -> Vec<ZfsVdevIo> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').map(str::trim).collect();
            if fields.len() < 32 || fields[0].is_empty() {
                return None;
            }

            Some(ZfsVdevIo {
                name: fields[0].to_string(),
                allocated_bytes: parse_optional_u64(fields[1]),
                free_bytes: parse_optional_u64(fields[2]),
                read_ops_per_sec: parse_optional_f64(fields[3]),
                write_ops_per_sec: parse_optional_f64(fields[4]),
                read_bytes_per_sec: parse_optional_f64(fields[5]),
                write_bytes_per_sec: parse_optional_f64(fields[6]),
                total_wait_read_ns: parse_optional_u64(fields[7]),
                total_wait_write_ns: parse_optional_u64(fields[8]),
                disk_wait_read_ns: parse_optional_u64(fields[9]),
                disk_wait_write_ns: parse_optional_u64(fields[10]),
                sync_queue_wait_read_ns: parse_optional_u64(fields[11]),
                sync_queue_wait_write_ns: parse_optional_u64(fields[12]),
                async_queue_wait_read_ns: parse_optional_u64(fields[13]),
                async_queue_wait_write_ns: parse_optional_u64(fields[14]),
                scrub_wait_ns: parse_optional_u64(fields[15]),
                trim_wait_ns: parse_optional_u64(fields[16]),
                rebuild_wait_ns: parse_optional_u64(fields[17]),
                sync_read_queue_pending: parse_optional_u64(fields[18]),
                sync_read_queue_active: parse_optional_u64(fields[19]),
                sync_write_queue_pending: parse_optional_u64(fields[20]),
                sync_write_queue_active: parse_optional_u64(fields[21]),
                async_read_queue_pending: parse_optional_u64(fields[22]),
                async_read_queue_active: parse_optional_u64(fields[23]),
                async_write_queue_pending: parse_optional_u64(fields[24]),
                async_write_queue_active: parse_optional_u64(fields[25]),
                scrub_read_queue_pending: parse_optional_u64(fields[26]),
                scrub_read_queue_active: parse_optional_u64(fields[27]),
                trim_write_queue_pending: parse_optional_u64(fields[28]),
                trim_write_queue_active: parse_optional_u64(fields[29]),
                rebuild_write_queue_pending: parse_optional_u64(fields[30]),
                rebuild_write_queue_active: parse_optional_u64(fields[31]),
            })
        })
        .collect()
}

pub fn parse_dbufstats(input: &str) -> Option<DbufStats> {
    let raw = parse_kstat_map(input);
    Some(DbufStats {
        cache_size_bytes: raw.get("cache_size_bytes").copied(),
        cache_target_bytes: raw.get("cache_target_bytes").copied(),
        hash_hits: raw.get("hash_hits").copied(),
        hash_misses: raw.get("hash_misses").copied(),
        cache_total_evicts: raw.get("cache_total_evicts").copied(),
    })
}

pub fn parse_dnodestats(input: &str) -> Option<DnodeStats> {
    let raw = parse_kstat_map(input);
    Some(DnodeStats {
        hold_alloc_hits: raw.get("dnode_hold_alloc_hits").copied(),
        hold_alloc_misses: raw.get("dnode_hold_alloc_misses").copied(),
        allocate: raw.get("dnode_allocate").copied(),
        buf_evict: raw.get("dnode_buf_evict").copied(),
    })
}

pub fn parse_zil(input: &str) -> Option<ZilStats> {
    let raw = parse_kstat_map(input);
    Some(ZilStats {
        commit_count: raw.get("zil_commit_count").copied(),
        itx_count: raw.get("zil_itx_count").copied(),
        itx_metaslab_normal_bytes: raw.get("zil_itx_metaslab_normal_bytes").copied(),
    })
}

pub fn parse_zfetchstats(input: &str) -> Option<ZfetchStats> {
    let raw = parse_kstat_map(input);
    Some(ZfetchStats {
        hits: raw.get("hits").copied(),
        misses: raw.get("misses").copied(),
        io_issued: raw.get("io_issued").copied(),
        io_active: raw.get("io_active").copied(),
    })
}

pub fn parse_abdstats(input: &str) -> Option<AbdStats> {
    let raw = parse_kstat_map(input);
    Some(AbdStats {
        linear_count: raw.get("linear_cnt").copied(),
        linear_data_size_bytes: raw.get("linear_data_size").copied(),
        scatter_count: raw.get("scatter_cnt").copied(),
        scatter_data_size_bytes: raw.get("scatter_data_size").copied(),
        scatter_page_alloc_retry: raw.get("scatter_page_alloc_retry").copied(),
    })
}

pub fn parse_txgs(input: &str) -> Option<TxgSummary> {
    let fields: Vec<_> = input
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.trim_start().starts_with("txg "))
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() >= 8).then_some(fields)
        })
        .next()?;

    Some(TxgSummary {
        latest_txg: parse_optional_u64(fields[0]),
        latest_dirty_bytes: parse_optional_u64(fields[3]),
        latest_read_bytes: parse_optional_u64(fields[4]),
        latest_written_bytes: parse_optional_u64(fields[5]),
        latest_reads: parse_optional_u64(fields[6]),
        latest_writes: parse_optional_u64(fields[7]),
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
    use std::fs;
    use tempfile::TempDir;

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
        let (snapshot, diagnostics) =
            collect_with_availability(Duration::from_secs(1), false, ZfsCollectionMode::Shallow);

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

    #[test]
    fn parses_zfs_dataset_usage() {
        let input = "data\t6311953548792\t11187890698760\t6311795099184\t/data\ton\t1.08\t0\t6311795099184\t0\t158449608\n";
        let datasets = parse_zfs_list(input);
        let dataset = &datasets[0];

        assert_eq!(dataset.name, "data");
        assert_eq!(dataset.used_bytes, Some(6_311_953_548_792));
        assert_eq!(dataset.available_bytes, Some(11_187_890_698_760));
        assert_eq!(dataset.mountpoint.as_deref(), Some("/data"));
        assert_eq!(dataset.compression.as_deref(), Some("on"));
        assert_eq!(dataset.compressratio, Some(1.08));
    }

    #[test]
    fn applies_zfs_get_properties_to_datasets() {
        let mut datasets = parse_zfs_list("data\t1\t2\t3\t/data\ton\t1.00\t0\t1\t0\t0\n");
        let input = "\
data\trecordsize\t131072\tdefault
data\tprimarycache\tall\tdefault
data\treadonly\toff\tdefault
";

        apply_zfs_get_properties(&mut datasets, input);

        assert_eq!(
            datasets[0].properties.get("recordsize").unwrap().value,
            "131072"
        );
        assert_eq!(
            datasets[0].properties.get("primarycache").unwrap().value,
            "all"
        );
    }

    #[test]
    fn parses_zpool_iostat_latency_and_queue_fields() {
        let input = "data\t10665755332608\t19295936524288\t0\t382\t0\t4010886\t-\t3094394\t-\t795672\t-\t-\t-\t2376632\t-\t-\t-\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n";
        let rows = parse_zpool_iostat(input);
        let row = &rows[0];

        assert_eq!(row.name, "data");
        assert_eq!(row.allocated_bytes, Some(10_665_755_332_608));
        assert_eq!(row.write_ops_per_sec, Some(382.0));
        assert_eq!(row.write_bytes_per_sec, Some(4_010_886.0));
        assert_eq!(row.total_wait_read_ns, None);
        assert_eq!(row.total_wait_write_ns, Some(3_094_394));
        assert_eq!(row.async_queue_wait_write_ns, Some(2_376_632));
        assert_eq!(row.sync_read_queue_pending, Some(0));
        assert_eq!(row.rebuild_write_queue_active, Some(0));
    }

    #[test]
    fn deep_collector_runs_scoped_dataset_and_single_iostat_commands() {
        let kstats = fake_kstat_root();
        let mut calls = Vec::new();
        let (snapshot, diagnostics) = collect_with_runner_and_roots(
            ZfsCollectionMode::Deep,
            kstats.path(),
            |program, args| {
                calls.push(format!("{program} {}", args.join(" ")));
                fake_success_for(program, args)
            },
        );

        assert!(snapshot.pools.iter().any(|pool| pool.name == "data"));
        assert!(calls.iter().any(|call| call == "zpool list -Hp -o name,size,allocated,free,capacity,dedupratio,fragmentation,health,altroot,autotrim"));
        assert!(calls.iter().any(|call| call == "zpool status -P"));
        assert!(calls.iter().any(|call| call == "zfs list -Hp -r -t filesystem,volume -o name,used,available,referenced,mountpoint,compression,compressratio,usedsnap,usedds,usedrefreserv,usedchild data"));
        assert!(calls.iter().any(|call| call == "zfs get -Hp -r -t filesystem,volume -o name,property,value,source recordsize,primarycache,secondarycache,sync,logbias,atime,dedup,checksum,encryption,readonly data"));
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.as_str() == "zpool iostat -Hp -vlq -y data 1 1")
                .count(),
            1
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn deep_collector_skips_zfs_list_when_budget_exhausted_after_pools() {
        let kstats = fake_kstat_root();
        let mut calls = Vec::new();
        let budget = commands::OptionalCommandBudget::new(
            Duration::from_millis(5),
            Duration::from_millis(5),
        );
        let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
            ZfsCollectionMode::Deep,
            &budget,
            kstats.path(),
            |program, args| {
                calls.push(format!("{program} {}", args.join(" ")));
                if program == "zpool" && args == ["status", "-P"] {
                    std::thread::sleep(Duration::from_millis(8));
                }
                fake_success_for(program, args)
            },
        );

        assert!(!calls.iter().any(|call| call.starts_with("zfs list ")));
        assert!(diagnostics
            .iter()
            .any(|d| d == "zfs list skipped: deep ZFS budget exhausted"));
    }

    #[test]
    fn deep_collector_skips_zfs_get_when_budget_exhausted_after_dataset_list() {
        let kstats = fake_kstat_root();
        let mut calls = Vec::new();
        let budget = commands::OptionalCommandBudget::new(
            Duration::from_millis(20),
            Duration::from_millis(20),
        );
        let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
            ZfsCollectionMode::Deep,
            &budget,
            kstats.path(),
            |program, args| {
                calls.push(format!("{program} {}", args.join(" ")));
                if program == "zfs" && args.first() == Some(&"list") {
                    std::thread::sleep(Duration::from_millis(25));
                }
                fake_success_for(program, args)
            },
        );

        assert!(calls.iter().any(|call| call.starts_with("zfs list ")));
        assert!(!calls.iter().any(|call| call.starts_with("zfs get ")));
        assert!(diagnostics
            .iter()
            .any(|d| d == "zfs get skipped: deep ZFS budget exhausted"));
    }

    #[test]
    fn deep_collector_rewrites_iostat_timeout_diagnostic() {
        let kstats = fake_kstat_root();
        let mut calls = Vec::new();
        let budget = commands::OptionalCommandBudget::new(
            Duration::from_millis(2500),
            Duration::from_millis(1500),
        );
        let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
            ZfsCollectionMode::Deep,
            &budget,
            kstats.path(),
            |program, args| {
                calls.push(format!("{program} {}", args.join(" ")));
                if program == "zpool" && args.first() == Some(&"iostat") {
                    return Some(commands::OptionalCommandOutput {
                        output: None,
                        diagnostic: Some("zpool timed out after 1.5s".to_string()),
                    });
                }
                fake_success_for(program, args)
            },
        );

        assert!(calls.iter().any(|call| call.starts_with("zpool iostat ")));
        assert!(diagnostics
            .iter()
            .any(|d| d.starts_with("zpool iostat timed out")));
    }

    #[test]
    fn deep_collector_skips_iostat_when_budget_cannot_cover_interval() {
        let kstats = fake_kstat_root();
        let mut calls = Vec::new();
        let budget = commands::OptionalCommandBudget::new(
            Duration::from_millis(900),
            Duration::from_millis(900),
        );
        let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
            ZfsCollectionMode::Deep,
            &budget,
            kstats.path(),
            |program, args| {
                calls.push(format!("{program} {}", args.join(" ")));
                fake_success_for(program, args)
            },
        );

        assert!(!calls.iter().any(|call| call.starts_with("zpool iostat ")));
        assert!(diagnostics
            .iter()
            .any(|d| d == "zpool iostat skipped: deep ZFS budget exhausted"));
    }

    #[test]
    fn parses_kernel_summary_kstats() {
        let dbuf = parse_dbufstats("name type data\ncache_size_bytes 4 278614528\ncache_target_bytes 4 310263808\nhash_hits 4 46158602\nhash_misses 4 4997783\ncache_total_evicts 4 235006\n").unwrap();
        let dnode = parse_dnodestats("name type data\ndnode_hold_alloc_hits 4 17992928\ndnode_hold_alloc_misses 4 6\ndnode_allocate 4 222738\ndnode_buf_evict 4 37777\n").unwrap();
        let zil = parse_zil("name type data\nzil_commit_count 4 679737\nzil_itx_count 4 2252838\nzil_itx_metaslab_normal_bytes 4 7734040792\n").unwrap();
        let zfetch = parse_zfetchstats(
            "name type data\nhits 4 130809\nmisses 4 2512441\nio_issued 4 16434\nio_active 4 0\n",
        )
        .unwrap();
        let abd = parse_abdstats("name type data\nlinear_cnt 4 95678\nlinear_data_size 4 90073088\nscatter_cnt 4 784605\nscatter_data_size 4 8236362240\nscatter_page_alloc_retry 4 0\n").unwrap();

        assert_eq!(dbuf.cache_size_bytes, Some(278_614_528));
        assert_eq!(dbuf.hash_hits, Some(46_158_602));
        assert_eq!(dnode.allocate, Some(222_738));
        assert_eq!(dnode.buf_evict, Some(37_777));
        assert_eq!(zil.commit_count, Some(679_737));
        assert_eq!(zfetch.io_issued, Some(16_434));
        assert_eq!(abd.scatter_data_size_bytes, Some(8_236_362_240));
    }

    #[test]
    fn parses_recent_txg_summary() {
        let input = "\
txg      birth            state ndirty       nread        nwritten     reads    writes   otime        qtime        wtime        stime
7628332  165111799183944  C     1797632      0            3854336      0        385      5119403164   3133         38560        312162505
";
        let summary = parse_txgs(input).unwrap();

        assert_eq!(summary.latest_txg, Some(7_628_332));
        assert_eq!(summary.latest_dirty_bytes, Some(1_797_632));
        assert_eq!(summary.latest_written_bytes, Some(3_854_336));
    }

    fn fake_kstat_root() -> TempDir {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("data")).unwrap();
        temp
    }

    fn fake_success_for(program: &str, args: &[&str]) -> Option<commands::OptionalCommandOutput> {
        let output = match (program, args.first().copied()) {
            ("zpool", Some("list")) => "data\t29961691856896\t10665749323776\t19295942533120\t35\t1.00\t1\tONLINE\t-\toff\n",
            ("zpool", Some("status")) => "\
  pool: data
 state: ONLINE
  scan: resilvered 97.1M in 00:23:33 with 0 errors on Sun May 10 17:56:02 2026
config:

\tNAME        STATE     READ WRITE CKSUM
\tdata        ONLINE       0     0     0
\t  raidz2-0  ONLINE       0     0     0
\t    /dev/sdb ONLINE      0     0     0

errors: No known data errors
",
            ("zfs", Some("list")) => "data\t6311953548792\t11187890698760\t6311795099184\t/data\ton\t1.08\t0\t6311795099184\t0\t158449608\n",
            ("zfs", Some("get")) => "data\trecordsize\t131072\tdefault\ndata\tprimarycache\tall\tdefault\n",
            ("zpool", Some("iostat")) => "data\t10665755332608\t19295936524288\t0\t382\t0\t4010886\t-\t3094394\t-\t795672\t-\t-\t-\t2376632\t-\t-\t-\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n",
            _ => return None,
        };
        Some(commands::OptionalCommandOutput {
            output: Some(output.to_string()),
            diagnostic: None,
        })
    }
}
