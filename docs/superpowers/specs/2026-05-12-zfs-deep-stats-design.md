# ZFS Deep Stats Design

## Purpose

`diskwatch` currently reports only a compact ZFS pool summary from `zpool list`
and `zpool status`. The next increment should make ZFS much more useful on
OpenZFS hosts while preserving the same terminal style as `cpuwatch`,
`memwatch`, and the existing `diskwatch` screens.

The feature targets the real ZFS host at `10.0.0.10`, where OpenZFS 2.2.2 is
installed and the `data` pool exposes pool, vdev, L2ARC, ARC, dataset, TXG, and
kernel kstat data.

## Goals

- Keep the default UI compact enough for the normal `diskwatch` screen.
- Add a `--zfs-deep` mode that shows detailed ZFS statistics.
- Report ZFS pool capacity, allocation, health, fragmentation, dedup ratio,
  autotrim, scan state, status text, known data errors, and topology.
- Report per-vdev and cache-device activity from `zpool iostat`, including
  read/write operations, bandwidth, allocation/free space, wait latency, and
  queue pending/active counts when available.
- Report ARC and L2ARC size, limits, hit/miss ratios, demand/prefetch split,
  data/metadata split, MRU/MFU split, memory pressure, L2ARC size, L2ARC
  read/write bytes, and L2ARC error counters.
- Report supporting ZFS kernel counters from `/proc/spl/kstat/zfs`, including
  dbuf cache, dnode, zfetch, ABD, ZIL, and pool TXG summaries.
- Report dataset usage and key dataset properties from `zfs list`/`zfs get`.
- Degrade cleanly when commands are absent, pools are absent, kstat files are
  unreadable, or a host is not running OpenZFS.
- Keep all collection read-only. No scrub, trim, clear, import, export, set,
  tune, or repair actions.

## Non-Goals

- No interactive drill-down or controls beyond the new `--zfs-deep` flag.
- No changes to pools, datasets, vdevs, cache devices, ZFS module parameters,
  or kernel state.
- No attempt to duplicate every field from `arc_summary` or `arcstat`.
  `diskwatch` should select high-value fields and keep raw counters structured
  enough to extend later.
- No external dependency on Python helper scripts.
- No hard requirement for root. Permission failures become diagnostics.

## User Experience

Default mode keeps the existing `zfs:` section but expands each pool into a
short multi-line summary:

```text
zfs:
  data
    health:        ONLINE
    size:          27.2 TiB
    allocated:     9.7 TiB
    free:          17.5 TiB
    capacity:      35%
    fragmentation: 1%
    dedup:         1.00x
    scan:          resilvered 97.1M in 00:23:33 with 0 errors on Sun May 10 17:56:02 2026
    errors:        No known data errors
```

`--zfs-deep` keeps the same top-level sections but adds longer ZFS output under
`zfs:`. The text report and TUI should use the same data model. The TUI can wrap
or truncate within its bordered panels; `--once --zfs-deep` is the complete
long-form view for terminals or logs.

The deep section should be organized in predictable subsections:

- `pools`: capacity, health, scan, errors, and topology.
- `vdev io`: pool, raidz/mirror/root vdevs, disks, and cache devices.
- `arc`: ARC and L2ARC health and effectiveness.
- `datasets`: dataset usage and important properties.
- `kernel`: dbuf, dnode, zfetch, ABD, ZIL, and TXG summaries.

## CLI

Add `--zfs-deep` to `DisplayOptions` and CLI parsing. The flag only affects the
amount of ZFS detail shown and collected. Existing flags keep their behavior:

- `--loop` controls loop devices and loop-backed filesystems.
- `--tmpfs` controls tmpfs filesystems.
- `--once` still samples twice with the chosen interval for disk activity.
  The first warm-up sample must skip optional command collection; the second
  reporting sample performs optional collection, including deep ZFS collection
  when requested.
- `--interval` still controls the normal sample interval.

## Data Sources

Use read-only sources in this order:

1. `zpool list -Hp -o name,size,allocated,free,capacity,dedupratio,fragmentation,health,altroot,autotrim`
   for parseable pool properties in bytes where possible.
2. `zpool status -P` for scan text, status/action text, error summary, and pool
   topology.
3. `zpool iostat -Hp -vlq -y <pool>... 1 1` in deep mode for parseable current
   interval vdev I/O, wait latency, and queue pending/active statistics. Run
   this as one command for all discovered pools, not once per pool.
4. `zfs list -Hp -r -t filesystem,volume -o name,used,available,referenced,mountpoint,compression,compressratio,usedsnap,usedds,usedrefreserv,usedchild <pool>...`
   for dataset usage.
5. `zfs get -Hp -r -t filesystem,volume -o name,property,value,source recordsize,primarycache,secondarycache,sync,logbias,atime,dedup,checksum,encryption,readonly <pool>...`
   for selected dataset properties when deep mode is enabled.
6. `/proc/spl/kstat/zfs/arcstats` for ARC and L2ARC counters.
7. `/proc/spl/kstat/zfs/{dbufstats,dnodestats,zfetchstats,abdstats,zil}` for
   global kernel cache and ZIL counters.
8. `/proc/spl/kstat/zfs/<pool>/{iostats,txgs,state,reads,dmu_tx_assign}` for
   pool-specific kernel data when readable.

All optional commands remain timeout-bound. Default mode keeps the current fast
optional-command budget. Deep mode uses a separate ZFS budget with a hard total
wall-clock target of about 2.5 seconds, a per-command timeout of about 1.5
seconds, and at most one `zpool iostat` command per refresh. If the budget
cannot cover the iostat interval, skip the deep I/O sample and emit a diagnostic
instead of blocking longer. For `--once --zfs-deep`, the warm-up sample must
skip optional commands and only the second reporting sample may run deep ZFS
collection. The `zpool iostat -y ... 1 1` values represent the one-second ZFS
interval immediately before the report is printed, while disk activity still
uses the user-requested `--interval` warm-up delta.

Deep-mode command collection should be ordered and bounded:

1. Run the default pool list/status commands first.
2. Read kstat files through bounded file reads, not shell commands. Each kstat
   read should have a small byte or line cap appropriate to the file. If a file
   is too large, unreadable, or missing, skip that file and emit a diagnostic.
3. Run `zfs list` if command budget remains; otherwise skip datasets and emit
   `zfs list skipped: deep ZFS budget exhausted`.
4. Run `zfs get` if command budget remains; otherwise render datasets without
   selected properties and emit `zfs get skipped: deep ZFS budget exhausted`.
5. Run the single `zpool iostat` command only if enough budget remains for its
   one-second interval plus command overhead; otherwise render vdev I/O as
   `N/A` and emit `zpool iostat skipped: deep ZFS budget exhausted`.

## Data Model

Replace the current `Vec<Zpool>` snapshot field with a richer ZFS snapshot:

```rust
pub struct ZfsSnapshot {
    pub pools: Vec<ZfsPool>,
    pub arc: Option<ArcStats>,
    pub datasets: Vec<ZfsDataset>,
    pub kernel: ZfsKernelStats,
}
```

`ZfsPool` should include:

- name, size bytes, allocated bytes, free bytes, capacity percent
- dedup ratio, fragmentation percent, health, altroot, autotrim
- scan text, status text, action text, error summary
- topology nodes from `zpool status`
- optional vdev I/O rows from `zpool iostat`, including wait latency fields
  and queue pending/active counts when supplied by `-lq`
- optional pool kstats

`ArcStats` should include raw counters plus derived metrics:

- ARC hit ratio, miss ratio, demand data hit ratio, demand metadata hit ratio,
  prefetch data hit ratio, prefetch metadata hit ratio
- ARC size, target, min, max, compressed size, uncompressed size, metadata size,
  data size, dnode size, dbuf size, MRU size, MFU size
- L2ARC hit ratio, size, asize, read bytes, write bytes, writes sent/done/error,
  checksum and I/O errors
- memory all/free/available, throttle/direct/indirect counters

`ZfsDataset` should include:

- name, used, available, referenced, mountpoint
- compression, compressratio
- used by snapshots, dataset, children, and refreservation
- optional properties such as recordsize, primarycache, secondarycache, sync,
  logbias, atime, dedup, checksum, encryption, and readonly when available
  from the explicit `zfs get` property list above

`ZfsKernelStats` should hold compact summaries for:

- dbuf cache size/target/hits/misses/evictions
- dnode hold/allocate/free counters
- zfetch hits/misses/issued/active streams
- ABD linear/scatter allocation sizes and allocation retry counters
- ZIL commit and intent log byte counters
- recent TXG dirty/read/write bytes and timing

## Parsing And Formatting

Parsers should keep units structured as bytes or numbers wherever command output
can produce parseable values. Formatting should happen in `render.rs`.

The `zpool status` topology parser should preserve hierarchy and group special
sections such as `cache`, `logs`, `spares`, `special`, and `dedup`. For the
`data` pool on `10.0.0.10`, this should show the root pool, `raidz2-0`, disks
`/dev/sda` through `/dev/sde`, and the cache device
`/dev/disk/by-id/nvme-eui.000000000000000100a075254d4fbfe0-part1`.

The `zpool iostat -Hp -vlq -y` parser should tolerate blank lines and should
parse the OpenZFS 2.2.2 positional field order. With `-p`, byte fields are exact
byte counts and latency fields are nanoseconds. The expected tab-separated
fields are:

1. name
2. alloc bytes
3. free bytes
4. read ops/sec
5. write ops/sec
6. read bytes/sec
7. write bytes/sec
8. total wait read ns
9. total wait write ns
10. disk wait read ns
11. disk wait write ns
12. sync queue wait read ns
13. sync queue wait write ns
14. async queue wait read ns
15. async queue wait write ns
16. scrub wait ns
17. trim wait ns
18. rebuild wait ns
19. sync read queue pending
20. sync read queue active
21. sync write queue pending
22. sync write queue active
23. async read queue pending
24. async read queue active
25. async write queue pending
26. async write queue active
27. scrub read queue pending
28. scrub read queue active
29. trim write queue pending
30. trim write queue active
31. rebuild write queue pending
32. rebuild write queue active

Missing fields represented by `-` become `None`. Tests should include the
combined `-lq` output shape from OpenZFS 2.2.2.

The `zfs get` parser should consume tab-separated `name,property,value,source`
records only for filesystems and volumes under the discovered pool names. It
should preserve property source for future rendering but only render value in
this increment. Snapshot and bookmark properties are out of scope.

Kstat parsers should share a helper for the common `name type data` format and
ignore the kstat header lines. Unknown counters should be retained in a map so
future render changes can use them without changing the reader.

## Snapshot Collection

Default mode should collect:

- pool list
- pool status
- ARC summary if `/proc/spl/kstat/zfs/arcstats` is readable and cheap

Deep mode should collect:

- everything from default mode
- one `zpool iostat -Hp -vlq -y <pool>... 1 1` command for all pools
- dataset list
- selected dataset properties from the explicit `zfs get` command above
- global kstats
- pool kstats

The optional command cache should include the ZFS depth mode in its freshness
key, so toggling `--zfs-deep` cannot reuse a shallow ZFS cache. It should not
invalidate slow ZFS, SMART, or LVM data solely because unrelated display flags
such as `--loop` or `--tmpfs` changed. Deep-mode cache refresh should still
avoid running long commands every screen draw.

## Error Handling

Diagnostics should be concise and source-specific:

- `zpool not found`
- `zfs not found`
- `zpool iostat timed out`
- `zpool iostat skipped: deep ZFS budget exhausted`
- `zfs list skipped: deep ZFS budget exhausted`
- `zfs get skipped: deep ZFS budget exhausted`
- `zfs dataset data unavailable`
- `zfs kstat arcstats unreadable: permission denied`
- `zfs pool data kstats unavailable`

Partial data is acceptable. For example, if kstats are unreadable but `zpool`
commands work, the pool and vdev sections should render while ARC/kernel
subsections show `N/A` or a diagnostic.

## Testing

Use test-driven implementation. Add parser fixtures based on sanitized output
from `10.0.0.10`:

- `zpool list -Hp` for pool properties.
- `zpool status -P data` for topology, scan, and errors.
- `zpool iostat -Hp -vlq -y data 1 1` for current interval vdev I/O, wait
  latency, and queue counts.
- `zfs list -Hp -r -t filesystem,volume` for dataset usage.
- `/proc/spl/kstat/zfs/arcstats` for ARC/L2ARC counters.
- `/proc/spl/kstat/zfs/dbufstats`, `dnodestats`, `zfetchstats`, `abdstats`,
  and `zil` for kernel summaries.
- `/proc/spl/kstat/zfs/data/txgs` for recent TXG parsing.

Unit tests should cover:

- pool property parsing into numeric values
- topology hierarchy parsing
- scan, status, action, and errors extraction
- vdev iostat current-sample parsing
- combined latency/queue iostat parsing
- kstat map parsing
- ARC/L2ARC derived ratio calculations, including zero-denominator cases
- dataset parsing
- render output for compact and deep ZFS modes. `--once --zfs-deep` must include
  all deep subsection headings (`pools`, `vdev io`, `arc`, `datasets`,
  `kernel`) and all diagnostics present in the snapshot. Full-fixture render
  tests, where all deep sources are present, must also include representative
  content from each subsection: pool health/capacity/errors, at least one
  topology row such as `raidz2-0` or `/dev/sdb`, vdev read/write rates, at
  least one latency field, at least one queue pending/active field, ARC hit
  ratio, L2ARC size or hit ratio, dataset usage for `data`, one selected
  dataset property such as `recordsize` or `primarycache`, and at least one
  kernel summary such as dbuf cache, ZIL commit count, or latest TXG bytes.
  Partial-data render tests should assert that unavailable subsections show
  `N/A` or a source-specific diagnostic rather than requiring representative
  content from a missing source. The TUI may truncate long row content to fit
  its panels, but it must still show the ZFS section heading, pool names, ARC
  summary when available, and diagnostics present in the snapshot.
- diagnostics for missing commands and unreadable kstats

Verification before completion:

- `cargo fmt --check`
- `cargo test`
- `cargo clippy -- -D warnings`
- `make check`
- `cargo run -- --once --interval 100ms`
- `cargo run -- --once --interval 100ms --zfs-deep` locally
- copy or run the branch on `10.0.0.10` and verify `--zfs-deep --once` against
  pool `data`

## Rollout

Implement the feature in small slices:

1. Add CLI/display option plumbing and shallow/deep ZFS snapshot shape.
2. Expand pool list/status parsing and rendering.
3. Add ARC/L2ARC kstat parsing and derived metrics.
4. Add dataset parsing.
5. Add deep vdev I/O parsing.
6. Add supporting kernel summaries.
7. Test on `10.0.0.10`, then push to `master` and let Woodpecker release.
