# diskwatch Design

## Purpose

`diskwatch` is a read-only Linux storage monitor for terminal users who want the
same workflow and visual style as `cpuwatch` and `memwatch`. It shows local
storage activity, capacity, topology, RAID/ZFS/LVM state, and health details
without changing disks, filesystems, pools, arrays, or kernel settings.

The first release targets Linux only. It must run usefully without root by using
baseline `/proc` and `/sys` data, then add richer read-only details from optional
system commands when those commands are installed and readable.

## Goals

- Match the `cpuwatch` and `memwatch` Rust TUI shape: `--interval`, `--once`,
  `q`/Esc/Ctrl-C quit keys, a cyan title line, bordered text panels, and yellow
  diagnostics.
- Report per-device read and write throughput, read and write IOPS, and disk
  busy/utilization from `/proc/diskstats` deltas.
- Report block-device inventory from `/sys/block`, including size, type,
  rotational/SSD hint, model, vendor, serial, and sector sizes when available.
- Report mounted filesystem capacity using mount data plus `statvfs`.
- Report storage stack state for ZFS pools, mdraid arrays, and LVM volumes when
  supported by local data or optional commands.
- Report SMART health details when `smartctl` is installed and can read a
  device.
- Degrade to `N/A` fields and diagnostics for missing commands, missing
  permissions, unsupported storage stacks, or parse failures.
- Provide Makefile package/install targets and Woodpecker release automation
  that mirror `cpuwatch` and `memwatch`, including the `RELEASE_TOKEN` secret.

## Non-Goals

- No write actions, repair actions, scrubs, SMART tests, RAID changes, pool
  changes, mount changes, or filesystem changes.
- No cross-platform support in the first release.
- No interactive drill-down, sorting controls, filtering controls, or history
  graphs in the first release.
- No hard dependency on ZFS, mdadm, LVM, `lsblk`, or `smartctl`.

## Architecture

The crate is organized like the existing watch tools:

- `src/main.rs` calls `diskwatch::run()`.
- `src/lib.rs` owns `run()`, `run_with_cli()`, `--once`, and the TUI event loop.
- `src/cli.rs` parses `--interval` and `--once`.
- `src/snapshot.rs` coordinates the collectors and rate trackers.
- `src/render.rs` renders both the TUI and the `--once` text report.

Storage-specific collectors are kept focused:

- `src/diskstats.rs` parses `/proc/diskstats` and computes rates from two
  samples and elapsed time.
- `src/block.rs` reads `/sys/block` inventory and block-device metadata.
- `src/filesystems.rs` reads mounts and filesystem capacity through `statvfs`.
- `src/raid.rs` parses `/proc/mdstat` and optionally augments it with
  read-only `mdadm` output.
- `src/zfs.rs` optionally reads `zpool list` and `zpool status`.
- `src/lvm.rs` optionally reads `pvs`, `vgs`, and `lvs`.
- `src/smart.rs` optionally reads `smartctl` health data.
- `src/commands.rs` provides a small timeout-aware helper for optional
  read-only command execution.

The public snapshot model should normalize collector output into these groups:

- Device activity.
- Filesystem capacity.
- Block-device inventory.
- Storage stacks.
- Device health.
- Diagnostics.

## Data Flow

1. `Sampler::default()` points at Linux defaults: `/proc/diskstats`,
   `/proc/mounts`, `/sys/block`, and `/proc/mdstat`.
2. The first sample records disk counters and returns static inventory with
   activity rates unavailable.
3. Each later sample reads fresh counters and computes read rate, write rate,
   read IOPS, write IOPS, and busy percentage using the elapsed time.
4. Static and slow-changing inventory is collected each sample for correctness,
   but command collectors must fail quickly and never block the TUI for long.
5. Optional command output is parsed into structured data. Missing commands and
   command failures become diagnostics instead of fatal errors.
6. `--once` performs the same warm-up pattern as the other watch tools: sample,
   sleep for the requested interval, sample again, and print the text report.

## UI

The default TUI uses the same visual language as `cpuwatch` and `memwatch`:
plain terminal text, cyan title, bordered panels, and a diagnostics band only
when diagnostics exist.

Primary panels:

- `Activity`: device name, read rate, write rate, read IOPS, write IOPS, and
  busy percentage.
- `Space`: mounted filesystem, mountpoint, used, free, total, and percent used.
- `Devices`: block-device size, type, rotational/SSD hint, model, vendor, and
  serial where readable.
- `Stacks`: ZFS pools, mdraid arrays, and LVM PV/VG/LV status and capacity.
- `Health`: SMART health, temperature, power-on hours, and wear/lifetime fields
  where available.

The layout can use two columns like the existing tools. Small terminals should
wrap or truncate text conservatively rather than introduce a new interaction
model.

The `--once` report uses stable section names:

- `activity:`
- `filesystems:`
- `devices:`
- `zfs:`
- `mdraid:`
- `lvm:`
- `smart:`
- `diagnostics:`

## Error Handling

`diskwatch` should keep running when individual data sources fail. Collector
functions should return empty structured data plus a diagnostic string when the
failure is useful to the user. Parser functions should be unit tested and should
ignore unknown fields unless the missing field prevents a useful record from
being produced.

Optional command behavior:

- If a command is not found, skip its section or show `N/A` and add a concise
  diagnostic.
- If a command exits unsuccessfully, include a diagnostic with the command name
  and enough context to understand the missing data.
- If output is partially parseable, keep parsed records and report a diagnostic
  only when the partial parse materially affects the visible result.

Permissions:

- The tool should run as a normal user.
- Installed packages may apply `cap_dac_read_search+ep`, matching the existing
  watch-tool pattern, to improve read access to protected metadata.
- The tool must not require root or sudo for normal startup.

## Build, Install, Packaging, And Release

The Makefile should mirror the existing watch tools with these targets:

- `build`
- `install`
- `install-binary`
- `capability`
- `show-capability`
- `uninstall`
- `test`
- `fmt`
- `clippy`
- `check`
- `package`
- `package-deb`
- `package-rpm`
- `check-packages`
- `package-clean`
- `clean`

Defaults:

- `BIN ?= diskwatch`
- `CAPABILITY ?= cap_dac_read_search+ep`
- package summary: Linux storage monitor terminal TUI
- package description: read-only Linux storage monitor for disk activity,
  filesystem space, block devices, ZFS pools, RAID arrays, LVM, and SMART
  health.

Woodpecker should match `cpuwatch` and `memwatch`:

- Run on push, pull request, tag, and manual events.
- Run `make check`.
- Build a release binary artifact.
- On tags, build Debian and RPM packages and run package checks.
- On pushes to `master`, create an annotated `v0.1.<pipeline>` release tag.
- On tag events, publish artifacts to GitHub with `woodpeckerci/plugin-release`.
- Use the same `RELEASE_TOKEN` secret name.

## Testing

Unit tests should cover:

- CLI interval parsing, `--once`, and zero-interval rejection.
- `/proc/diskstats` parsing and delta-based throughput/IOPS/busy calculations.
- `/sys/block` fixture parsing for size, rotational flag, sector sizes, and
  device identity fields.
- Mount parsing and filesystem capacity formatting. Where `statvfs` is hard to
  fixture directly, isolate it behind a small function so the summary logic can
  be tested.
- `/proc/mdstat` parsing.
- Representative `zpool list`, `zpool status`, `pvs`, `vgs`, `lvs`, and
  `smartctl` outputs.
- Text report section coverage in `render.rs`.
- Quit-key behavior in `lib.rs`.
- Package artifact validation through `scripts/check-packages.sh`.

Verification before completion should include:

- `cargo fmt --check`
- `cargo test`
- `cargo clippy -- -D warnings`
- `make check`
- `make package` when local package tools are available
- `make check-packages` when package artifacts are built
