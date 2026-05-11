# diskwatch - Linux Storage Monitor TUI

`diskwatch` is a read-only Linux storage monitor and terminal TUI for disk
activity, filesystem space, block devices, ZFS pools, mdraid arrays, LVM state,
and SMART health. It is useful when you want a lightweight Rust companion to
tools like `iostat`, `df`, `lsblk`, `zpool`, `mdadm`, `lvs`, and `smartctl`.

The default mode is an interactive terminal UI. A `--once` mode is also
available for scripts, diagnostics, CI logs, and non-interactive environments.

## What You Can Monitor

Use `diskwatch` when you want to:

- Monitor per-device read throughput, write throughput, read IOPS, write IOPS,
  and busy percentage from Linux disk statistics.
- Inspect mounted filesystem capacity, including used, available, total, and
  percent used. Kernel pseudo filesystems are filtered, while capacity-bearing
  mounts such as container overlay roots are retained. Tmpfs mounts are hidden
  by default and can be shown with `--tmpfs`. Remote and FUSE mounts are skipped
  to avoid blocking on stale network filesystems.
- Inspect block-device inventory, including size, type, rotational hint, logical
  and physical sector size, vendor, model, and serial where readable.
- Check ZFS pool capacity and health when `zpool` is installed.
- Check mdraid array state from `/proc/mdstat` and optional `mdadm` output.
- Check LVM physical volumes, volume groups, and logical volumes when LVM tools
  are installed.
- Check SMART health, temperature, power-on hours, and wear/lifetime fields when
  `smartctl` is installed and allowed to read a device.

## Quick Start

Run from the repository without installing:

```sh
cargo run -- --once
cargo run -- --interval 500ms
```

Build and install the release binary:

```sh
make install
diskwatch --once
diskwatch
```

If `/usr/local/bin` is not in your `PATH`, either add it or install with a custom
`PREFIX`, `BINDIR`, or `INSTALL_PATH`.

## Supported Systems

`diskwatch` targets Linux systems that expose storage activity through procfs,
block-device metadata through sysfs, filesystem capacity through mounted
filesystems, and optional storage-stack details through local read-only command
line tools.

| System type | Support level | Notes |
| --- | --- | --- |
| Bare-metal Linux with procfs and sysfs | Full | Expected to show disk activity, mounted filesystem space, and block-device inventory. Optional ZFS, mdraid, LVM, and SMART sections depend on local tooling and permissions. |
| Linux VMs and containers | Partial | `/proc/diskstats`, `/proc/mounts`, and `/sys/block` may be filtered or virtualized. Optional device health data is often hidden. |
| Linux hosts without ZFS, mdraid, LVM, or SMART tools | Partial | Core activity, space, and block-device panels can still work. Missing optional tools become `N/A` values and diagnostics. |
| macOS, Windows, BSD, WSL without Linux storage procfs/sysfs access | Not supported for useful runtime data | The crate may compile on some non-Linux targets, but the monitor expects Linux `/proc`, `/sys`, and storage command interfaces. |

The TUI requires an interactive terminal. Use `--once` for automation or
non-interactive environments.

## Data Sources

| Data | Source |
| --- | --- |
| Device activity counters | `/proc/diskstats` |
| Mounted filesystem list | `/proc/mounts` |
| Filesystem capacity | `statvfs` |
| Block-device inventory | `/sys/block` |
| mdraid state | `/proc/mdstat` |
| mdraid details | `mdadm --detail --scan` |
| ZFS pools | `zpool list` and `zpool status` |
| LVM state | `pvs`, `vgs`, and `lvs` |
| SMART health | `smartctl` |

`diskwatch` does not change filesystems, mount state, RAID arrays, ZFS pools,
LVM volumes, SMART settings, kernel tunables, or any other system
configuration.

Optional command output is cached for 30 seconds in the TUI and collected under
a short aggregate budget so slower tools such as `zpool`, `mdadm`, LVM commands,
or per-device `smartctl` checks cannot multiply into long UI stalls. Core
activity, filesystem, block-device, and `/proc/mdstat` data are still refreshed
on the normal interval.

Mounted filesystem capacity uses synchronous local `statvfs` calls. Remote and
FUSE filesystem types such as NFS, CIFS, sshfs, and similar mounts are skipped
so a stale mount cannot freeze the monitor.

## Prerequisites

Required for building:

- Rust 1.88 or newer, matching the crate's `rust-version`.
- Cargo.

Optional but recommended:

- `make`, for the repository build/install targets.
- `sudo`, `setcap`, and `getcap`, for installing the binary with Linux file
  capabilities.
- `zpool`, if you want ZFS pool details.
- `mdadm`, if you want mdraid details beyond `/proc/mdstat`.
- LVM tools (`pvs`, `vgs`, `lvs`), if you want LVM details.
- `smartctl`, usually from smartmontools, if you want SMART health data.

On Debian or Ubuntu-style systems, the optional runtime tools are typically in:

```sh
sudo apt install make libcap2-bin zfsutils-linux mdadm lvm2 smartmontools
```

On Fedora-style systems:

```sh
sudo dnf install make libcap zfs-fuse mdadm lvm2 smartmontools
```

Distribution package names vary. Optional storage tools are discovered in
standard local system command directories such as `/usr/bin`, `/usr/sbin`,
`/usr/local/bin`, and `/usr/local/sbin`; install the package that provides them
for your distribution.

## Building

Build a debug binary:

```sh
cargo build
```

Build an optimized release binary:

```sh
cargo build --release
```

The release binary is written to:

```sh
target/release/diskwatch
```

The Makefile wraps the release build:

```sh
make build
```

## Building Packages

Build Debian and RPM packages:

```sh
make package VERSION=0.1.0
make check-packages VERSION=0.1.0
```

Package artifacts are written to `dist/` by default:

- `diskwatch_0.1.0_amd64.deb`
- `diskwatch-0.1.0-1.x86_64.rpm`

Both packages install `diskwatch` to `/usr/bin/diskwatch`, keep the binary
executable, and run this during package installation:

```sh
setcap cap_dac_read_search+ep /usr/bin/diskwatch
```

Required package build tools:

- `dpkg-deb`, usually provided by the Debian or Ubuntu `dpkg` package.
- `rpmbuild`, usually provided by the Fedora, RHEL, or Debian `rpm` package.

## Development Checks

Run the full local check suite:

```sh
make check
```

That runs:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

You can also run individual targets:

```sh
make fmt
make test
make clippy
```

## Installing And Capabilities

The recommended install path is through the Makefile:

```sh
make install
```

By default this:

1. Builds `target/release/diskwatch` if needed.
2. Installs it to `/usr/local/bin/diskwatch`.
3. Applies the `cap_dac_read_search+ep` file capability set.
4. Prints the resulting capability with `getcap`.

Verify the installed command:

```sh
command -v diskwatch
diskwatch --once
```

If you prefer to run the privileged install step explicitly, build first and
then run install under `sudo`:

```sh
make build
sudo make install
```

The prebuild matters because `sudo make install` runs as root and the Makefile
expects the release binary to already exist in that case.

### Custom Install Paths

Install under a different prefix:

```sh
PREFIX="$HOME/.local" make install
```

Install to a specific binary directory:

```sh
BINDIR="$HOME/.local/bin" make install
```

Install to an exact path:

```sh
INSTALL_PATH="$HOME/.local/bin/diskwatch" make install
```

### Installing Without Capabilities

To install only the binary:

```sh
make install-binary
```

Without capabilities, `diskwatch` still runs, but protected storage metadata and
SMART details may be unavailable on some hosts.

You can apply or reapply capabilities later:

```sh
make capability
```

Check the installed capabilities:

```sh
make show-capability
getcap "$(command -v diskwatch)"
```

Remove the installed binary:

```sh
make uninstall
```

### Cargo Install

You can also install with Cargo:

```sh
cargo install --path .
```

Cargo does not apply Linux file capabilities. If you need protected storage
metadata reads, apply the capabilities manually or use `make install`.

## Runtime Setup For Optional Commands

### Basic Activity, Space, And Device Inventory

The core activity, filesystem, and block-device sections rely on `/proc`,
`/sys`, and mounted filesystems:

```sh
test -r /proc/diskstats
test -r /proc/mounts
ls /sys/block
```

These data sources normally work as an unprivileged user on Linux. Some
containers or hardened hosts may hide devices or expose only virtualized
storage.

### ZFS

ZFS details use read-only `zpool` commands:

```sh
zpool list
zpool status
```

If ZFS is not installed or no pools are present, the ZFS section reports `N/A`
and includes a diagnostic when useful.

### mdraid

mdraid state is read from:

```sh
/proc/mdstat
```

Additional details use:

```sh
mdadm --detail --scan
```

If `mdadm` is missing, `/proc/mdstat` can still provide array state on systems
that use mdraid.

### LVM

LVM details use:

```sh
pvs --readonly
vgs --readonly
lvs --readonly
```

Install LVM tools if you want physical volume, volume group, and logical volume
details.

### SMART

SMART health uses `smartctl`:

```sh
smartctl -n standby -A -H /dev/sda
```

Device names vary by host. `diskwatch` probes common physical disk names such as
`sd*`, `hd*`, `nvme*`, and `mmcblk*`; it skips logical and virtual devices such
as `dm-*`, `vda`, `xvda`, `nbd*`, `rbd*`, and `zd*` so they do not consume the
optional command budget. The `-n standby` guard reduces the chance of waking
sleeping disks, but `smartctl` device autodetection can still wake some
hardware. Some drives, USB adapters, NVMe devices, and RAID controllers require
different `smartctl` options or elevated privileges. The monitor reports
missing, asleep, or unreadable SMART data as `N/A` rather than failing.

### Capabilities

Some filesystems, package managers, or copy operations do not preserve Linux file
capabilities. If the installed binary is replaced after install, run:

```sh
make capability
```

The default capability is:

```sh
cap_dac_read_search+ep
```

It can help read protected metadata, but it does not grant write access and does
not bypass all kernel, device, container, or command-level restrictions.

## Usage

Start the interactive TUI:

```sh
diskwatch
```

Exit the TUI with any of:

- `q`
- `Esc`
- `Ctrl-C`

Use a custom update interval:

```sh
diskwatch --interval 500ms
diskwatch --interval 2s
```

Print one text report and exit:

```sh
diskwatch --once
```

Use a custom sampling interval for the one-shot report:

```sh
diskwatch --once --interval 250ms
```

In `--once` mode, `diskwatch` takes an initial sample, waits for the interval,
then takes a second sample so activity rates can be computed from counter
deltas.

Loop devices and loop-backed filesystem rows are hidden by default. Show them
when needed:

```sh
diskwatch --loop
```

Tmpfs filesystem rows are hidden by default. Show them when needed:

```sh
diskwatch --tmpfs
```

Show CLI help:

```sh
diskwatch --help
```

Current options:

```text
Usage: diskwatch [OPTIONS]

Options:
      --interval <INTERVAL>  [default: 1s]
      --once
      --loop
      --tmpfs
  -h, --help                 Print help
```

## Read-Only Safety

`diskwatch` is designed as a read-only monitor. It reads Linux procfs/sysfs
files, mounted filesystem statistics, and optional command output. It does not
write to block devices, run repairs, start scrubs, alter mounts, modify ZFS
pools, change mdraid arrays, change LVM metadata, start SMART tests, or tune
kernel storage settings.

## Repository Layout

- `src/main.rs`: binary entry point.
- `src/lib.rs`: mode selection, TUI loop, and terminal lifecycle.
- `src/cli.rs`: command-line options.
- `src/diskstats.rs`: `/proc/diskstats` parsing and activity calculations.
- `src/block.rs`: `/sys/block` inventory and block-device metadata.
- `src/filesystems.rs`: mount parsing and filesystem capacity.
- `src/raid.rs`: `/proc/mdstat` and optional `mdadm` parsing.
- `src/zfs.rs`: optional `zpool` parsing.
- `src/lvm.rs`: optional LVM command parsing.
- `src/smart.rs`: optional `smartctl` parsing.
- `src/commands.rs`: timeout-aware helper for optional read-only commands.
- `src/snapshot.rs`: combined sampling state.
- `src/render.rs`: TUI rendering and one-shot text reports.
- `Makefile`: build, install, capability, package, and check targets.
