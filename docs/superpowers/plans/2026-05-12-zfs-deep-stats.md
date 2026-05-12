# ZFS Deep Stats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add compact and deep OpenZFS reporting to `diskwatch`, including pool details, topology, vdev I/O, ARC/L2ARC, datasets, and kernel kstat summaries.

**Architecture:** Keep the existing collector/render split. `src/zfs.rs` owns ZFS parsing, bounded file reads, command collection, and derived ZFS metrics; `src/snapshot.rs` decides whether shallow or deep ZFS collection is needed; `src/render.rs` formats compact and deep ZFS sections. Deep collection is gated by `--zfs-deep`, has a separate hard ZFS budget, and `--once` only runs optional/deep collection on the second reporting sample.

**Tech Stack:** Rust 1.88, clap, ratatui, existing timeout-aware optional command helper, Linux/OpenZFS command and procfs interfaces.

---

## File Structure

- Modify `src/cli.rs`: add `--zfs-deep` flag and parser test.
- Modify `src/lib.rs`: pass `zfs_deep` into `DisplayOptions`; make `--once` skip optional collection on warm-up and collect on the reporting sample.
- Modify `src/snapshot.rs`: add ZFS depth to display/cache state; add optional-collection control for warm-up samples; call the new shallow/deep ZFS collector.
- Replace most of `src/zfs.rs`: richer ZFS data model, parser functions, bounded kstat reads, shallow/deep collectors.
- Modify `src/render.rs`: render compact ZFS by default and long-form subsections when `--zfs-deep` is active.
- Modify `README.md`: document `--zfs-deep` and the ZFS data sources.
- Keep tests inline in each Rust module, matching existing repo style.

## Task 1: CLI Plumbing And `--once` Optional-Collection Control

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Modify: `src/snapshot.rs`

- [ ] **Step 1: Write failing CLI test for `--zfs-deep`**

Add to `src/cli.rs` tests:

```rust
#[test]
fn parses_zfs_deep_display_flag() {
    let cli = Cli::try_parse_from(["diskwatch", "--zfs-deep"]).unwrap();
    assert!(cli.zfs_deep);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test cli::tests::parses_zfs_deep_display_flag`

Expected: FAIL because `Cli` has no `zfs_deep` field.

- [ ] **Step 3: Add CLI field**

Add to `Cli`:

```rust
#[arg(long = "zfs-deep")]
pub zfs_deep: bool,
```

- [ ] **Step 4: Run CLI test to verify it passes**

Run: `cargo test cli::tests::parses_zfs_deep_display_flag`

Expected: PASS.

- [ ] **Step 5: Write failing snapshot/lib tests for display option and once warm-up behavior**

In `src/snapshot.rs`, extend `DisplayOptions`:

```rust
pub struct DisplayOptions {
    pub show_loop: bool,
    pub show_tmpfs: bool,
    pub zfs_deep: bool,
}
```

Add a test that proves optional cache freshness changes with ZFS depth but not loop/tmpfs flags:

```rust
#[test]
fn optional_cache_key_tracks_zfs_depth_only() {
    let temp = TempDir::new().unwrap();
    let diskstats = temp.path().join("diskstats");
    let sys_block = temp.path().join("sys/block");
    let mounts = temp.path().join("mounts");
    let mdstat = temp.path().join("mdstat");
    write(&diskstats, "");
    write(&sys_block.join("sda/size"), "2097152\n");
    write(&mounts, "");
    write(&mdstat, "");

    let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
    sampler.optional_commands_enabled = true;
    sampler.optional_cache.zfs_deep = false;
    sampler.optional_cache.device_names = vec!["sda".to_string()];
    sampler.optional_cache.collected_at = Some(Instant::now());

    assert!(sampler.optional_cache_is_fresh(&["sda".to_string()], false, Instant::now()));
    assert!(!sampler.optional_cache_is_fresh(&["sda".to_string()], true, Instant::now()));
}
```

In `src/lib.rs`, add a `run_once`-level unit test if practical by exposing a small helper, or add a `Sampler` method test in `src/snapshot.rs`:

```rust
#[test]
fn warmup_sample_can_skip_optional_commands() {
    let diskstats = NamedTempFile::new().unwrap();
    let mut sampler = Sampler::new_for_tests(diskstats.path().to_path_buf());
    sampler.optional_commands_enabled = true;

    let snapshot = sampler.sample_at_with_optional(Instant::now(), false);

    assert!(snapshot.zfs.is_empty());
    assert!(snapshot.diagnostics.is_empty());
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test snapshot::tests::optional_cache_key_tracks_zfs_depth_only snapshot::tests::warmup_sample_can_skip_optional_commands`

Expected: FAIL because `DisplayOptions::zfs_deep`, `OptionalCommandCache::zfs_deep`, and `sample_at_with_optional` do not exist.

- [ ] **Step 7: Implement minimal plumbing**

Update `src/lib.rs`:

```rust
let display_options = DisplayOptions {
    show_loop: cli.show_loop,
    show_tmpfs: cli.show_tmpfs,
    zfs_deep: cli.zfs_deep,
};
```

Change `run_once`:

```rust
let _ = sampler.sample_at_with_optional(Instant::now(), false);
thread::sleep(interval);
let snapshot = sampler.sample_at_with_optional(Instant::now(), true);
```

In `src/snapshot.rs`:

- Add `zfs_deep: bool` to `DisplayOptions`.
- Add `zfs_deep: bool` to `OptionalCommandCache`.
- Add:

```rust
pub fn sample_at_with_optional(&mut self, now: Instant, collect_optional: bool) -> Snapshot
```

Have `sample()` and `sample_at()` call it with `true`.

Update `optional_cache_is_fresh` signature to include `zfs_deep: bool` and compare it to `self.optional_cache.zfs_deep`.

- [ ] **Step 8: Run targeted tests**

Run: `cargo test cli::tests::parses_zfs_deep_display_flag snapshot::tests::optional_cache_key_tracks_zfs_depth_only snapshot::tests::warmup_sample_can_skip_optional_commands`

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/cli.rs src/lib.rs src/snapshot.rs
git commit -m "feat: add zfs deep mode plumbing"
```

## Task 2: Rich ZFS Pool Model, Pool List, And Status Parsing

**Files:**
- Modify: `src/zfs.rs`
- Modify: `src/snapshot.rs`
- Modify: `src/render.rs`

- [ ] **Step 1: Write failing pool property parser test**

In `src/zfs.rs`:

```rust
#[test]
fn parses_zpool_list_properties_as_numbers() {
    let input = "data\t29961691856896\t10665749323776\t19295942533120\t35\t1.00\t1\tONLINE\t-\toff\n";
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
```

- [ ] **Step 2: Write failing status parser test**

```rust
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
    assert!(status
        .action
        .as_deref()
        .unwrap()
        .contains("zpool clear"));
    assert!(status.scan.as_deref().unwrap().contains("resilvered 97.1M"));
    assert_eq!(status.errors.as_deref(), Some("No known data errors"));
    assert!(status.topology.iter().any(|node| node.name == "raidz2-0" && node.depth == 1));
    assert!(status.topology.iter().any(|node| node.name == "/dev/sdb" && node.depth == 2));
    assert!(status.topology.iter().any(|node| node.name == "cache" && node.role == ZfsTopologyRole::SpecialGroup));
}
```

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test zfs::tests::parses_zpool_list_properties_as_numbers zfs::tests::parses_zpool_status_scan_errors_and_topology`

Expected: FAIL because the current `Zpool` model stores strings and status parsing omits scan/errors/topology.

- [ ] **Step 4: Implement model and parsers**

Replace the current `Zpool`/`ZpoolStatus` model with:

```rust
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZfsTopologyRole {
    Pool,
    Vdev,
    Disk,
    SpecialGroup,
}
```

Add helpers:

```rust
fn parse_optional_u64(value: &str) -> Option<u64>
fn parse_percent(value: &str) -> Option<f64>
fn parse_ratio(value: &str) -> Option<f64>
fn dash_to_none(value: &str) -> Option<String>
```

Keep old function names where possible (`parse_zpool_list`, `parse_zpool_status`) so integration changes are contained.

- [ ] **Step 5: Update snapshot defaults and old tests**

Change `Snapshot.zfs` and `OptionalCommandCache.zfs` from `Vec<Zpool>` to `ZfsSnapshot`.

Update existing assertions from `snapshot.zfs.is_empty()` to `snapshot.zfs.pools.is_empty()`.

Update the Task 1 warm-up test to assert `snapshot.zfs.pools.is_empty()` after the model migration.

Update the cached optional command test to construct `ZfsSnapshot { pools: vec![ZfsPool { name: "tank".to_string(), health: "ONLINE".to_string(), ..Default::default() }], ..Default::default() }`.

- [ ] **Step 6: Update compact render**

Replace `write_zfs_lines` with compact multi-line fields:

```text
data
  health:        ONLINE
  size:          27.2 TiB
  allocated:     9.7 TiB
  free:          17.5 TiB
  capacity:      35.0%
  fragmentation: 1.0%
  dedup:         1.00x
  status:        One or more devices has experienced an unrecoverable error.
  action:        Replace the faulted device, or use 'zpool clear'.
  scan:          ...
  errors:        No known data errors
```

- [ ] **Step 7: Run targeted tests**

Run: `cargo test zfs render snapshot`

Expected: PASS after updating expected output.

- [ ] **Step 8: Commit**

```bash
git add src/zfs.rs src/snapshot.rs src/render.rs
git commit -m "feat: expand zfs pool summary"
```

## Task 3: Kstat Parser And ARC/L2ARC Metrics

**Files:**
- Modify: `src/zfs.rs`
- Modify: `src/render.rs`

- [ ] **Step 1: Write failing kstat parser test**

```rust
#[test]
fn parses_kstat_name_type_data_rows() {
    let input = "\
9 1 0x01 147 39984 4452996882 165618915963102
name                            type data
hits                            4    90
misses                          4    10
size                            4    1024
l2_hits                         4    5
l2_misses                       4    15
";
    let stats = parse_kstat_map(input);

    assert_eq!(stats.get("hits"), Some(&90));
    assert_eq!(stats.get("misses"), Some(&10));
    assert_eq!(stats.get("size"), Some(&1024));
    assert_eq!(stats.get("l2_hits"), Some(&5));
}
```

- [ ] **Step 2: Write failing ARC derived metrics test**

```rust
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
```

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test zfs::tests::parses_kstat_name_type_data_rows zfs::tests::derives_arc_and_l2arc_ratios`

Expected: FAIL because kstat and ARC parsers do not exist.

- [ ] **Step 4: Implement kstat and ARC structs**

Add:

```rust
pub type KstatMap = std::collections::HashMap<String, u64>;

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
```

Add safe ratio helper:

```rust
fn ratio_percent(numerator: u64, denominator: u64) -> Option<f64>
```

- [ ] **Step 5: Render ARC compact line only**

In compact mode, show only ARC hit ratio and size if present. Leave the full deep ARC subsection for Task 7, after the deep render mode is introduced.

- [ ] **Step 6: Run targeted tests**

Run: `cargo test zfs::tests::parses_kstat_name_type_data_rows zfs::tests::derives_arc_and_l2arc_ratios render`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/zfs.rs src/render.rs
git commit -m "feat: parse zfs arc stats"
```

## Task 4: Dataset List And Property Parsing

**Files:**
- Modify: `src/zfs.rs`
- Modify: `src/render.rs`

- [ ] **Step 1: Write failing dataset list parser test**

```rust
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
```

- [ ] **Step 2: Write failing property parser test**

```rust
#[test]
fn applies_zfs_get_properties_to_datasets() {
    let mut datasets = parse_zfs_list("data\t1\t2\t3\t/data\ton\t1.00\t0\t1\t0\t0\n");
    let input = "\
data\trecordsize\t131072\tdefault
data\tprimarycache\tall\tdefault
data\treadonly\toff\tdefault
";

    apply_zfs_get_properties(&mut datasets, input);

    assert_eq!(datasets[0].properties.get("recordsize").unwrap().value, "131072");
    assert_eq!(datasets[0].properties.get("primarycache").unwrap().value, "all");
}
```

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test zfs::tests::parses_zfs_dataset_usage zfs::tests::applies_zfs_get_properties_to_datasets`

Expected: FAIL because dataset types and parsers do not exist.

- [ ] **Step 4: Implement dataset types/parsers**

Add:

```rust
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

pub struct ZfsProperty {
    pub value: String,
    pub source: Option<String>,
}
```

Implement `parse_zfs_list` and `apply_zfs_get_properties`.

- [ ] **Step 5: Defer deep dataset render**

Do not render the deep dataset subsection yet. Task 7 introduces the deep render mode and should render datasets there. Keep this task focused on data parsing and model population.

Task 7 deep render should later include:

```text
datasets:
  data used=5.7 TiB avail=10.2 TiB ref=5.7 TiB mount=/data compress=on ratio=1.08x
    recordsize: 128.0 KiB
    primarycache: all
```

- [ ] **Step 6: Run targeted tests**

Run: `cargo test zfs::tests::parses_zfs_dataset_usage zfs::tests::applies_zfs_get_properties_to_datasets`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/zfs.rs
git commit -m "feat: parse zfs datasets"
```

## Task 5: Vdev I/O Parser For `zpool iostat -Hp -vlq -y`

**Files:**
- Modify: `src/zfs.rs`
- Modify: `src/render.rs`

- [ ] **Step 1: Write failing vdev I/O parser test**

```rust
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
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test zfs::tests::parses_zpool_iostat_latency_and_queue_fields`

Expected: FAIL because parser/type does not exist.

- [ ] **Step 3: Implement `ZfsVdevIo` and parser**

Add all 32 positional fields described in the spec. Use `Option<u64>` for bytes, ns, and queue counts; use `Option<f64>` for ops/sec and bytes/sec if render expects rates as floats.

Parser requirements:

- Skip blank lines.
- Skip rows with fewer than 32 fields.
- Treat `-` as `None`.
- Accept cache rows with by-id names.

- [ ] **Step 4: Defer vdev I/O render**

Do not render the deep vdev I/O subsection yet. Task 7 introduces the deep render mode and should render vdev I/O there.

Task 7 deep render should later include a short row per pool/vdev:

```text
vdev io:
  data read=0 B/s write=3.8 MiB/s rops=0.0/s wops=382.0/s total_wait_w=3.1 ms asyncq_wait_w=2.4 ms syncq_r=0/0 rebuildq_w=0/0
```

- [ ] **Step 5: Run targeted tests**

Run: `cargo test zfs::tests::parses_zpool_iostat_latency_and_queue_fields`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/zfs.rs
git commit -m "feat: parse zfs vdev io"
```

## Task 6: Deep Collector, Bounded Kstat Reads, And Budget Diagnostics

**Files:**
- Modify: `src/zfs.rs`
- Modify: `src/snapshot.rs`

- [ ] **Step 1: Write failing collector tests with fake runner**

In `src/zfs.rs`, add tests for collector command shape:

```rust
#[test]
fn deep_collector_runs_scoped_dataset_and_single_iostat_commands() {
    let mut calls = Vec::new();
    let (snapshot, diagnostics) = collect_with_runner_and_roots(
        ZfsCollectionMode::Deep,
        fake_kstat_root(),
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
```

Add a budget-exhaustion test:

```rust
#[test]
fn deep_collector_reports_budget_exhaustion_for_skipped_sources() {
    let budget = OptionalCommandBudget::new(Duration::from_millis(1), Duration::from_millis(1));
    let (_snapshot, diagnostics) = collect_budgeted_with_mode(&budget, ZfsCollectionMode::Deep);

    assert!(diagnostics
        .iter()
        .any(|d| d == "zpool iostat skipped: deep ZFS budget exhausted"));
}
```

Add `zfs list` and `zfs get` budget tests. Use a fake runner that sleeps long
enough to exhaust the real `OptionalCommandBudget` after the named stage:

```rust
#[test]
fn deep_collector_skips_zfs_list_when_budget_exhausted_after_pools() {
    let mut calls = Vec::new();
    let budget = OptionalCommandBudget::new(Duration::from_millis(5), Duration::from_millis(5));
    let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
        ZfsCollectionMode::Deep,
        &budget,
        fake_kstat_root(),
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
    let mut calls = Vec::new();
    let budget = OptionalCommandBudget::new(Duration::from_millis(20), Duration::from_millis(20));
    let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
        ZfsCollectionMode::Deep,
        &budget,
        fake_kstat_root(),
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
```

Add a source-specific iostat timeout diagnostic test:

```rust
#[test]
fn deep_collector_rewrites_iostat_timeout_diagnostic() {
    let mut calls = Vec::new();
    let budget = OptionalCommandBudget::new(Duration::from_millis(2500), Duration::from_millis(1500));
    let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
        ZfsCollectionMode::Deep,
        &budget,
        fake_kstat_root(),
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
```

Add an iostat preflight test that proves the runner is not called when too
little budget remains:

```rust
#[test]
fn deep_collector_skips_iostat_when_budget_cannot_cover_interval() {
    let mut calls = Vec::new();
    let budget = OptionalCommandBudget::new(Duration::from_millis(900), Duration::from_millis(900));
    let (_snapshot, diagnostics) = collect_with_runner_budget_and_roots(
        ZfsCollectionMode::Deep,
        &budget,
        fake_kstat_root(),
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
```

Add kernel summary parser tests before implementing kstat summaries:

```rust
#[test]
fn parses_kernel_summary_kstats() {
    let dbuf = parse_dbufstats("name type data\ncache_size_bytes 4 278614528\ncache_target_bytes 4 310263808\nhash_hits 4 46158602\nhash_misses 4 4997783\ncache_total_evicts 4 235006\n").unwrap();
    let dnode = parse_dnodestats("name type data\ndnode_hold_alloc_hits 4 17992928\ndnode_hold_alloc_misses 4 6\ndnode_allocate 4 222738\ndnode_buf_evict 4 37777\n").unwrap();
    let zil = parse_zil("name type data\nzil_commit_count 4 679737\nzil_itx_count 4 2252838\nzil_itx_metaslab_normal_bytes 4 7734040792\n").unwrap();
    let zfetch = parse_zfetchstats("name type data\nhits 4 130809\nmisses 4 2512441\nio_issued 4 16434\nio_active 4 0\n").unwrap();
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
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test zfs::tests::deep_collector_runs_scoped_dataset_and_single_iostat_commands zfs::tests::deep_collector_reports_budget_exhaustion_for_skipped_sources zfs::tests::deep_collector_skips_zfs_list_when_budget_exhausted_after_pools zfs::tests::deep_collector_skips_zfs_get_when_budget_exhausted_after_dataset_list zfs::tests::deep_collector_rewrites_iostat_timeout_diagnostic zfs::tests::deep_collector_skips_iostat_when_budget_cannot_cover_interval zfs::tests::parses_kernel_summary_kstats zfs::tests::parses_recent_txg_summary`

Expected: FAIL because deep collector and kernel summary APIs do not exist.

- [ ] **Step 3: Implement collector mode**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZfsCollectionMode {
    Shallow,
    Deep,
}
```

Change public collector to:

```rust
pub fn collect_budgeted(
    budget: &commands::OptionalCommandBudget,
    mode: ZfsCollectionMode,
) -> (ZfsSnapshot, Vec<String>)
```

Implement shallow:

- `zpool list`
- `zpool status`
- bounded `arcstats` if readable

Implement deep:

- shallow data
- bounded kstat reads
- before running `zfs list`, check `budget.remaining_timeout()`. If none,
  skip datasets and add `zfs list skipped: deep ZFS budget exhausted`.
- before running `zfs get`, check `budget.remaining_timeout()`. If none, keep
  dataset usage without selected properties and add
  `zfs get skipped: deep ZFS budget exhausted`.
- one `zpool iostat -Hp -vlq -y <pool>... 1 1`
- before running `zpool iostat`, require remaining budget of at least about
  1.1 seconds. If the remaining budget is lower, do not call the runner and add
  `zpool iostat skipped: deep ZFS budget exhausted`.
- if the command helper reports a timeout diagnostic for the `zpool iostat`
  subcommand, rewrite it to begin with `zpool iostat timed out` so diagnostics
  identify the failing ZFS source, not only the `zpool` binary.

- [ ] **Step 4: Implement bounded kstat reads**

Add helpers:

```rust
fn read_kstat_file(path: &Path, max_bytes: usize) -> Result<String, String>
fn collect_kstats(root: &Path, pools: &[ZfsPool]) -> (ZfsKernelStats, Vec<String>)
```

Use caps:

- `arcstats`, `dbufstats`, `dnodestats`, `zfetchstats`, `abdstats`, `zil`, `iostats`, `state`, `dmu_tx_assign`: 256 KiB each.
- `txgs`: read enough lines for recent entries, cap at 512 KiB.
- `reads`: read header and a bounded number of rows, cap at 128 KiB.

- [ ] **Step 5: Wire snapshot collector**

In `src/snapshot.rs`, use:

```rust
let zfs_mode = if self.display_options.zfs_deep {
    zfs::ZfsCollectionMode::Deep
} else {
    zfs::ZfsCollectionMode::Shallow
};
let zfs_budget = if self.display_options.zfs_deep {
    OptionalCommandBudget::new(DEEP_ZFS_TOTAL_BUDGET, DEEP_ZFS_COMMAND_TIMEOUT)
} else {
    OptionalCommandBudget::new(OPTIONAL_COMMAND_TOTAL_BUDGET, DEFAULT_COMMAND_TIMEOUT)
};
let (zfs, zfs_diagnostics) = zfs::collect_budgeted(&zfs_budget, zfs_mode);
```

Keep mdadm/LVM/SMART on the existing fast optional budget.

- [ ] **Step 6: Run targeted tests**

Run: `cargo test zfs snapshot`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/zfs.rs src/snapshot.rs
git commit -m "feat: collect deep zfs stats"
```

## Task 7: Deep ZFS Rendering

**Files:**
- Modify: `src/render.rs`
- Modify: `src/snapshot.rs` if render needs display options in `Snapshot`

- [ ] **Step 1: Write failing deep render test**

In `src/render.rs`, construct a `Snapshot` with `display_options.zfs_deep` or a `zfs.deep` marker, full ZFS data, and assert concrete lines:

```rust
#[test]
fn text_report_renders_deep_zfs_sections() {
    let snapshot = full_zfs_snapshot_fixture();
    let report = format_text_report(&snapshot);

    assert!(report.contains("pools:"));
    assert!(report.contains("data"));
    assert!(report.contains("health:"));
    assert!(report.contains("status:"));
    assert!(report.contains("action:"));
    assert!(report.contains("raidz2-0"));
    assert!(report.contains("vdev io:"));
    assert!(report.contains("write="));
    assert!(report.contains("total_wait_w="));
    assert!(report.contains("syncq_r="));
    assert!(report.contains("arc:"));
    assert!(report.contains("hit="));
    assert!(report.contains("l2"));
    assert!(report.contains("datasets:"));
    assert!(report.contains("recordsize:"));
    assert!(report.contains("kernel:"));
    assert!(report.contains("dbuf"));
    assert!(report.contains("dnode"));
    assert!(report.contains("abd"));
}
```

Add partial-data test:

```rust
#[test]
fn text_report_renders_deep_zfs_partial_data_with_diagnostics() {
    let snapshot = Snapshot {
        zfs: ZfsSnapshot {
            pools: vec![ZfsPool { name: "data".to_string(), health: "ONLINE".to_string(), ..Default::default() }],
            ..Default::default()
        },
        diagnostics: vec!["zfs kstat arcstats unreadable: permission denied".to_string()],
        ..Snapshot::default()
    };
    let report = format_text_report(&snapshot);

    assert!(report.contains("arc:"));
    assert!(report.contains("N/A"));
    assert!(report.contains("zfs kstat arcstats unreadable: permission denied"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test render::tests::text_report_renders_deep_zfs_sections render::tests::text_report_renders_deep_zfs_partial_data_with_diagnostics`

Expected: FAIL because deep render output does not exist.

- [ ] **Step 3: Implement render mode**

Add `zfs_deep: bool` to `Snapshot` or use `snapshot.zfs.deep` so `render.rs` can decide compact versus deep.

Implement:

```rust
fn write_zfs_compact_lines(...)
fn write_zfs_deep_lines(...)
fn write_zfs_pool_lines(...)
fn write_zfs_vdev_io_lines(...)
fn write_zfs_arc_lines(...)
fn write_zfs_dataset_lines(...)
fn write_zfs_kernel_lines(...)
```

Keep labels aligned like device detail fields.

- [ ] **Step 4: Run render tests**

Run: `cargo test render`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/render.rs src/snapshot.rs
git commit -m "feat: render deep zfs stats"
```

## Task 8: README And Local Verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write README update**

Document:

- `--zfs-deep`
- ZFS sources: `zpool list`, `zpool status`, `zpool iostat`, `zfs list`, `zfs get`, `/proc/spl/kstat/zfs`
- Deep mode can spend about one second collecting `zpool iostat`
- Permission failures degrade to diagnostics

- [ ] **Step 2: Run full verification locally**

Run:

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
make check
cargo run -- --once --interval 100ms
cargo run -- --once --interval 100ms --zfs-deep
git diff --check
```

Expected: all pass; local `--zfs-deep` may show `zpool not found` or `N/A` if the local host has no ZFS.

- [ ] **Step 3: Verify on `10.0.0.10`**

Build locally:

```bash
cargo build --release
```

Copy and run:

```bash
scp target/release/diskwatch 10.0.0.10:/tmp/diskwatch
ssh 10.0.0.10 '/tmp/diskwatch --once --interval 100ms --zfs-deep'
```

Expected output includes:

- `data`
- `raidz2-0`
- one of `/dev/sdb`, `/dev/sdc`, `/dev/sdd`, `/dev/sde`, `/dev/sda`
- `arc:`
- `l2`
- `datasets:`
- `kernel:`

- [ ] **Step 4: Commit README**

```bash
git add README.md
git commit -m "docs: document deep zfs stats"
```

## Task 9: Final Checks, Push, And Release Trigger

**Files:**
- No new files expected.

- [ ] **Step 1: Re-run final verification**

Run:

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
make check
cargo run -- --once --interval 100ms
cargo run -- --once --interval 100ms --zfs-deep
git diff --check
git status --short --branch
```

Expected: verification passes and the branch is clean except expected ahead commits.

- [ ] **Step 2: Push to master**

```bash
git push origin master
```

Expected: push succeeds.

- [ ] **Step 3: Monitor Woodpecker**

On `10.0.0.10`, query Woodpecker state as done previously:

```bash
ssh 10.0.0.10 'sudo sqlite3 -header -column /data/var/lib/docker/volumes/woodpecker-ci_woodpecker-server-data/_data/woodpecker.sqlite "SELECT id, number, event, status, \"commit\", branch, ref, started, finished FROM pipelines WHERE repo_id=(SELECT id FROM repos WHERE full_name='\''srvr-farm/diskwatch'\'') ORDER BY id DESC LIMIT 4;"'
```

Expected: latest push pipeline succeeds, creates a release tag, and the tag pipeline publishes release assets.
