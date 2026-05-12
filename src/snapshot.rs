use crate::block::{collect as collect_block_devices, BlockDevice};
use crate::commands::OptionalCommandBudget;
use crate::diskstats::{activities_between, read_diskstats, DiskActivity, DiskStat};
use crate::filesystems::{collect as collect_filesystems, FilesystemUsage};
use crate::lvm::{self, LvmSnapshot};
use crate::raid::{self, read_mdstat, MdArray};
use crate::smart::{self, SmartHealth};
use crate::zfs::{self, Zpool};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_millis(750);
const OPTIONAL_COMMAND_TOTAL_BUDGET: Duration = Duration::from_millis(750);
const OPTIONAL_COMMAND_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const OPTIONAL_COMMAND_BUDGET_EXHAUSTED_DIAGNOSTIC: &str =
    "optional command budget exhausted; remaining optional data deferred";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub activity: Vec<DiskActivity>,
    pub filesystems: Vec<FilesystemUsage>,
    pub devices: Vec<BlockDevice>,
    pub mdraid: Vec<MdArray>,
    pub zfs: Vec<Zpool>,
    pub lvm: LvmSnapshot,
    pub smart: Vec<SmartHealth>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayOptions {
    pub show_loop: bool,
    pub show_tmpfs: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct OptionalCommandCache {
    zfs: Vec<Zpool>,
    mdadm_scan: Vec<String>,
    lvm: LvmSnapshot,
    smart: Vec<SmartHealth>,
    diagnostics: Vec<String>,
    device_names: Vec<String>,
    collected_at: Option<Instant>,
}

#[derive(Debug)]
pub struct Sampler {
    diskstats_path: PathBuf,
    sys_block_root: PathBuf,
    mounts_path: PathBuf,
    mdstat_path: PathBuf,
    display_options: DisplayOptions,
    optional_commands_enabled: bool,
    optional_cache: OptionalCommandCache,
    previous_diskstats: Vec<DiskStat>,
    previous_at: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            diskstats_path: PathBuf::from("/proc/diskstats"),
            sys_block_root: PathBuf::from("/sys/block"),
            mounts_path: PathBuf::from("/proc/mounts"),
            mdstat_path: PathBuf::from("/proc/mdstat"),
            display_options: DisplayOptions::default(),
            optional_commands_enabled: true,
            optional_cache: OptionalCommandCache::default(),
            previous_diskstats: Vec::new(),
            previous_at: None,
        }
    }
}

impl Sampler {
    pub fn new_for_tests(diskstats_path: PathBuf) -> Self {
        let hermetic_root = hermetic_test_root(&diskstats_path);
        Self::new_for_tests_with_paths(
            diskstats_path,
            hermetic_root.join("sys/block"),
            hermetic_root.join("mounts"),
            hermetic_root.join("mdstat"),
        )
    }

    pub fn new_for_tests_with_paths(
        diskstats_path: PathBuf,
        sys_block_root: PathBuf,
        mounts_path: PathBuf,
        mdstat_path: PathBuf,
    ) -> Self {
        Self {
            diskstats_path,
            sys_block_root,
            mounts_path,
            mdstat_path,
            display_options: DisplayOptions::default(),
            optional_commands_enabled: false,
            optional_cache: OptionalCommandCache::default(),
            previous_diskstats: Vec::new(),
            previous_at: None,
        }
    }

    pub fn new_for_tests_with_roots(diskstats_path: PathBuf, sys_block_root: PathBuf) -> Self {
        Self::new_for_tests_with_roots_and_mounts(
            diskstats_path,
            sys_block_root,
            PathBuf::from("/nonexistent-diskwatch-test-mounts"),
        )
    }

    pub fn new_for_tests_with_roots_and_mounts(
        diskstats_path: PathBuf,
        sys_block_root: PathBuf,
        mounts_path: PathBuf,
    ) -> Self {
        Self::new_for_tests_with_paths(
            diskstats_path,
            sys_block_root,
            mounts_path,
            PathBuf::from("/nonexistent-diskwatch-test-mdstat"),
        )
    }

    pub fn sample(&mut self) -> Snapshot {
        self.sample_at(Instant::now())
    }

    pub fn with_display_options(mut self, display_options: DisplayOptions) -> Self {
        self.display_options = display_options;
        self
    }

    pub fn sample_at(&mut self, now: Instant) -> Snapshot {
        let current_diskstats = read_diskstats(&self.diskstats_path);
        let mut activity = self
            .previous_at
            .map(|previous_at| {
                activities_between(
                    &self.previous_diskstats,
                    &current_diskstats,
                    now.duration_since(previous_at),
                )
            })
            .unwrap_or_default();

        let mut devices = collect_block_devices(&self.sys_block_root);
        let duplicate_mappers = duplicate_mappers(&devices);
        devices = filter_devices(devices, &duplicate_mappers, self.display_options);
        activity = filter_activity(activity, &devices);
        let filesystems =
            filter_filesystems(collect_filesystems(&self.mounts_path), self.display_options);
        let mut mdraid = read_mdstat(&self.mdstat_path);
        let optional_commands = self.optional_commands_snapshot(&devices, now);
        raid::apply_mdadm_detail_scan(&mut mdraid, &optional_commands.mdadm_scan);

        self.previous_diskstats = current_diskstats;
        self.previous_at = Some(now);

        Snapshot {
            activity,
            filesystems,
            devices,
            mdraid,
            zfs: optional_commands.zfs,
            lvm: optional_commands.lvm,
            smart: optional_commands.smart,
            diagnostics: optional_commands.diagnostics,
        }
    }

    fn optional_commands_snapshot(
        &mut self,
        devices: &[BlockDevice],
        now: Instant,
    ) -> OptionalCommandCache {
        if !self.optional_commands_enabled {
            return OptionalCommandCache::default();
        }

        let device_names = optional_device_names(devices);
        if self.optional_cache_is_fresh(&device_names, now) {
            return self.optional_cache.clone();
        }

        let mut cache = self.collect_optional_commands(devices);
        cache.device_names = device_names;
        cache.collected_at = Some(now);
        self.optional_cache = cache;
        self.optional_cache.clone()
    }

    fn optional_cache_is_fresh(&self, device_names: &[String], now: Instant) -> bool {
        if self.optional_cache.device_names != device_names {
            return false;
        }

        let Some(collected_at) = self.optional_cache.collected_at else {
            return false;
        };

        now.checked_duration_since(collected_at)
            .is_some_and(|elapsed| elapsed < OPTIONAL_COMMAND_REFRESH_INTERVAL)
    }

    fn collect_optional_commands(&self, devices: &[BlockDevice]) -> OptionalCommandCache {
        let budget =
            OptionalCommandBudget::new(OPTIONAL_COMMAND_TOTAL_BUDGET, DEFAULT_COMMAND_TIMEOUT);
        let mut diagnostics = Vec::new();

        let (zfs, zfs_diagnostics) = zfs::collect_budgeted(&budget);
        diagnostics.extend(zfs_diagnostics);
        if append_budget_exhausted_if_needed(&budget, &mut diagnostics) {
            return optional_cache(
                zfs,
                Vec::new(),
                LvmSnapshot::default(),
                Vec::new(),
                diagnostics,
            );
        }

        let (mdadm_scan, mdadm_diagnostics) = raid::collect_mdadm_detail_scan_budgeted(&budget);
        diagnostics.extend(mdadm_diagnostics);
        if append_budget_exhausted_if_needed(&budget, &mut diagnostics) {
            return optional_cache(
                zfs,
                mdadm_scan,
                LvmSnapshot::default(),
                Vec::new(),
                diagnostics,
            );
        }

        let (lvm, lvm_diagnostics) = lvm::collect_budgeted(&budget);
        diagnostics.extend(lvm_diagnostics);
        if append_budget_exhausted_if_needed(&budget, &mut diagnostics) {
            return optional_cache(zfs, mdadm_scan, lvm, Vec::new(), diagnostics);
        }

        let (smart, smart_diagnostics) = smart::collect_budgeted(devices, &budget);
        diagnostics.extend(smart_diagnostics);
        append_budget_exhausted_if_needed(&budget, &mut diagnostics);

        optional_cache(zfs, mdadm_scan, lvm, smart, diagnostics)
    }
}

fn duplicate_mappers(devices: &[BlockDevice]) -> HashSet<String> {
    devices
        .iter()
        .filter(|device| device.device_type == "dm" && device.slaves.len() == 1)
        .map(|device| device.name.clone())
        .collect()
}

fn filter_activity(
    activity: Vec<DiskActivity>,
    displayed_devices: &[BlockDevice],
) -> Vec<DiskActivity> {
    let displayed_device_names: HashSet<&str> = displayed_devices
        .iter()
        .map(|device| device.name.as_str())
        .collect();
    activity
        .into_iter()
        .filter(|activity| displayed_device_names.contains(activity.name.as_str()))
        .collect()
}

fn filter_devices(
    devices: Vec<BlockDevice>,
    duplicate_mappers: &HashSet<String>,
    display_options: DisplayOptions,
) -> Vec<BlockDevice> {
    devices
        .into_iter()
        .filter(|device| display_options.show_loop || !is_loop_device(device))
        .filter(|device| !duplicate_mappers.contains(&device.name))
        .collect()
}

fn filter_filesystems(
    filesystems: Vec<FilesystemUsage>,
    display_options: DisplayOptions,
) -> Vec<FilesystemUsage> {
    filesystems
        .into_iter()
        .filter(|filesystem| display_options.show_loop || !is_loop_filesystem(filesystem))
        .filter(|filesystem| display_options.show_tmpfs || filesystem.fs_type != "tmpfs")
        .collect()
}

fn is_loop_device(device: &BlockDevice) -> bool {
    device.device_type == "loop" || is_loop_name(&device.name)
}

fn is_loop_name(name: &str) -> bool {
    name.starts_with("loop")
}

fn is_loop_filesystem(filesystem: &FilesystemUsage) -> bool {
    filesystem.source.starts_with("/dev/loop")
}

fn optional_cache(
    zfs: Vec<Zpool>,
    mdadm_scan: Vec<String>,
    lvm: LvmSnapshot,
    smart: Vec<SmartHealth>,
    diagnostics: Vec<String>,
) -> OptionalCommandCache {
    OptionalCommandCache {
        zfs,
        mdadm_scan,
        lvm,
        smart,
        diagnostics,
        device_names: Vec::new(),
        collected_at: None,
    }
}

fn append_budget_exhausted_if_needed(
    budget: &OptionalCommandBudget,
    diagnostics: &mut Vec<String>,
) -> bool {
    if budget.exhausted() {
        if !diagnostics
            .iter()
            .any(|diagnostic| diagnostic == OPTIONAL_COMMAND_BUDGET_EXHAUSTED_DIAGNOSTIC)
        {
            diagnostics.push(OPTIONAL_COMMAND_BUDGET_EXHAUSTED_DIAGNOSTIC.to_string());
        }
        true
    } else {
        false
    }
}

fn optional_device_names(devices: &[BlockDevice]) -> Vec<String> {
    devices
        .iter()
        .filter(|device| matches!(device.device_type.as_str(), "disk" | "nvme" | "mmc" | "zbc"))
        .map(|device| device.name.clone())
        .collect()
}

fn hermetic_test_root(diskstats_path: &std::path::Path) -> PathBuf {
    let parent = diskstats_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = diskstats_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "diskstats".into());

    parent.join(format!(".{name}-diskwatch-test"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    fn write(path: &Path, value: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    }

    #[test]
    fn sampler_combines_storage_snapshots() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");
        let mountpoint = temp.path().join("mnt/data");

        write(
            &diskstats,
            "8 0 sda 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0\n",
        );
        write(&sys_block.join("sda/size"), "2097152\n");
        write(&sys_block.join("sda/queue/rotational"), "1\n");
        fs::create_dir_all(&mountpoint).unwrap();
        write(
            &mounts,
            &format!("/dev/sda1 {} ext4 rw 0 0\n", mountpoint.display()),
        );
        write(
            &mdstat,
            "Personalities : [raid1]\nmd0 : active raid1 sdb1[1] sda1[0]\n      1046528 blocks super 1.2 [2/2] [UU]\n",
        );

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        let started = Instant::now();
        let first = sampler.sample_at(started);
        assert_eq!(first.devices.len(), 1);
        assert_eq!(first.filesystems.len(), 1);
        assert_eq!(first.filesystems[0].source, "/dev/sda1");
        assert_eq!(
            first.filesystems[0].mountpoint,
            mountpoint.display().to_string()
        );
        assert_eq!(first.filesystems[0].fs_type, "ext4");
        assert_eq!(first.mdraid.len(), 1);
        assert_eq!(first.mdraid[0].name, "md0");
        assert_eq!(first.mdraid[0].level.as_deref(), Some("raid1"));
        assert_eq!(first.mdraid[0].status.as_deref(), Some("[UU]"));
        assert!(first.zfs.is_empty());
        assert_eq!(first.lvm, LvmSnapshot::default());
        assert!(first.smart.is_empty());
        assert!(first.diagnostics.is_empty());
        assert!(first.activity.is_empty());

        write(
            &sampler.diskstats_path,
            "8 0 sda 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0\n",
        );
        let second = sampler.sample_at(started + Duration::from_secs(1));
        assert_eq!(second.activity[0].name, "sda");
        assert_eq!(second.activity[0].read_bytes_per_sec, Some(262_144.0));
        assert_eq!(second.activity[0].write_bytes_per_sec, Some(262_144.0));
        assert_eq!(second.devices[0].name, "sda");
        assert_eq!(second.filesystems[0].source, "/dev/sda1");
        assert_eq!(second.mdraid[0].name, "md0");
        assert!(second.zfs.is_empty());
        assert_eq!(second.lvm, LvmSnapshot::default());
        assert!(second.smart.is_empty());
        assert!(second.diagnostics.is_empty());
    }

    #[test]
    fn one_path_test_constructor_keeps_optional_commands_disabled() {
        let diskstats = NamedTempFile::new().unwrap();
        let mut sampler = Sampler::new_for_tests(diskstats.path().to_path_buf());

        let snapshot = sampler.sample();

        assert!(snapshot.devices.is_empty());
        assert!(snapshot.filesystems.is_empty());
        assert!(snapshot.mdraid.is_empty());
        assert!(snapshot.zfs.is_empty());
        assert_eq!(snapshot.lvm, LvmSnapshot::default());
        assert!(snapshot.smart.is_empty());
        assert!(snapshot.diagnostics.is_empty());
    }

    #[test]
    fn sampler_reports_activity_from_diskstats_deltas() {
        let file = NamedTempFile::new().unwrap();
        let sys_block = TempDir::new().unwrap();
        let mounts = NamedTempFile::new().unwrap();
        write(&sys_block.path().join("sda/size"), "2097152\n");
        fs::write(
            file.path(),
            "   8       0 sda 10 0 200 30 5 0 80 20 0 40 50 0 0 0 0 0 0\n",
        )
        .unwrap();

        let mut sampler = Sampler::new_for_tests_with_roots_and_mounts(
            file.path().to_path_buf(),
            sys_block.path().to_path_buf(),
            mounts.path().to_path_buf(),
        );
        assert!(sampler.sample().activity.is_empty());

        fs::write(
            file.path(),
            "   8       0 sda 16 0 1224 30 9 0 592 20 0 240 50 0 0 0 0 0 0\n",
        )
        .unwrap();

        sampler.previous_at = sampler
            .previous_at
            .map(|previous_at| previous_at - std::time::Duration::from_secs(100));

        let snapshot = sampler.sample();
        assert_eq!(snapshot.activity.len(), 1);
        assert_eq!(snapshot.activity[0].name, "sda");
        assert_close(snapshot.activity[0].read_bytes_per_sec.unwrap(), 5_242.88);
        assert_close(snapshot.activity[0].write_bytes_per_sec.unwrap(), 2_621.44);
    }

    #[test]
    fn sampler_hides_partition_activity_rows() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");

        write(
            &diskstats,
            "\
259 0 nvme0n1 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
259 3 nvme0n1p3 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
8 0 sda 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
8 1 sda1 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
",
        );
        write(&sys_block.join("nvme0n1/size"), "2097152\n");
        write(&sys_block.join("sda/size"), "2097152\n");
        write(&mounts, "");
        write(&mdstat, "");

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        let started = Instant::now();
        let _ = sampler.sample_at(started);
        write(
            &sampler.diskstats_path,
            "\
259 0 nvme0n1 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
259 3 nvme0n1p3 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
8 0 sda 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
8 1 sda1 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
",
        );

        let snapshot = sampler.sample_at(started + Duration::from_secs(1));

        assert_eq!(names(&snapshot.activity), ["nvme0n1", "sda"]);
    }

    #[test]
    fn sampler_reports_block_device_inventory() {
        let diskstats = NamedTempFile::new().unwrap();
        let sys_block = TempDir::new().unwrap();
        let mounts = NamedTempFile::new().unwrap();
        let sda = sys_block.path().join("sda");
        fs::create_dir_all(&sda).unwrap();
        fs::write(sda.join("size"), "2097152\n").unwrap();

        let mut sampler = Sampler::new_for_tests_with_roots_and_mounts(
            diskstats.path().to_path_buf(),
            sys_block.path().to_path_buf(),
            mounts.path().to_path_buf(),
        );

        let snapshot = sampler.sample();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].name, "sda");
        assert_eq!(snapshot.devices[0].size_bytes, 1_073_741_824);
        assert!(snapshot.filesystems.is_empty());
    }

    #[test]
    fn sampler_hides_loop_devices_and_tmpfs_by_default() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");
        let root = temp.path().join("root");
        let snap = temp.path().join("snap/tool");
        let shm = temp.path().join("dev/shm");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&snap).unwrap();
        fs::create_dir_all(&shm).unwrap();

        write(
            &diskstats,
            "\
8 0 sda 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
7 0 loop0 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
",
        );
        write(&sys_block.join("sda/size"), "2097152\n");
        write(&sys_block.join("loop0/size"), "1024\n");
        write(
            &mounts,
            &format!(
                "/dev/sda1 {} ext4 rw 0 0\n/dev/loop0 {} squashfs ro 0 0\ntmpfs {} tmpfs rw 0 0\n",
                root.display(),
                snap.display(),
                shm.display()
            ),
        );
        write(&mdstat, "");

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        let started = Instant::now();
        let _ = sampler.sample_at(started);
        write(
            &sampler.diskstats_path,
            "\
8 0 sda 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
7 0 loop0 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
",
        );

        let snapshot = sampler.sample_at(started + Duration::from_secs(1));

        assert_eq!(names(&snapshot.activity), ["sda"]);
        assert_eq!(device_names(&snapshot.devices), ["sda"]);
        assert_eq!(filesystem_sources(&snapshot.filesystems), ["/dev/sda1"]);
    }

    #[test]
    fn sampler_can_show_loop_devices_and_tmpfs_when_enabled() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");
        let root = temp.path().join("root");
        let snap = temp.path().join("snap/tool");
        let shm = temp.path().join("dev/shm");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&snap).unwrap();
        fs::create_dir_all(&shm).unwrap();

        write(
            &diskstats,
            "\
8 0 sda 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
7 0 loop0 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
",
        );
        write(&sys_block.join("sda/size"), "2097152\n");
        write(&sys_block.join("loop0/size"), "1024\n");
        write(
            &mounts,
            &format!(
                "/dev/sda1 {} ext4 rw 0 0\n/dev/loop0 {} squashfs ro 0 0\ntmpfs {} tmpfs rw 0 0\n",
                root.display(),
                snap.display(),
                shm.display()
            ),
        );
        write(&mdstat, "");

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        sampler.display_options = DisplayOptions {
            show_loop: true,
            show_tmpfs: true,
        };
        let started = Instant::now();
        let _ = sampler.sample_at(started);
        write(
            &sampler.diskstats_path,
            "\
8 0 sda 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
7 0 loop0 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
",
        );

        let snapshot = sampler.sample_at(started + Duration::from_secs(1));

        assert_eq!(names(&snapshot.activity), ["sda", "loop0"]);
        assert_eq!(device_names(&snapshot.devices), ["loop0", "sda"]);
        assert_eq!(
            filesystem_sources(&snapshot.filesystems),
            ["/dev/sda1", "/dev/loop0", "tmpfs"]
        );
    }

    #[test]
    fn sampler_hides_single_slave_dm_activity_and_device_duplicates() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");

        write(
            &diskstats,
            "\
259 3 nvme0n1p3 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
253 0 dm-0 10 0 200 0 5 0 80 0 0 40 50 0 0 0 0 0 0
",
        );
        write(&sys_block.join("nvme0n1p3/size"), "2097152\n");
        write(&sys_block.join("dm-0/size"), "2097152\n");
        fs::create_dir_all(sys_block.join("dm-0/slaves/nvme0n1p3")).unwrap();
        write(&mounts, "");
        write(&mdstat, "");

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        let started = Instant::now();
        let _ = sampler.sample_at(started);
        write(
            &sampler.diskstats_path,
            "\
259 3 nvme0n1p3 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
253 0 dm-0 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0
",
        );

        let snapshot = sampler.sample_at(started + Duration::from_secs(1));

        assert_eq!(names(&snapshot.activity), ["nvme0n1p3"]);
        assert_eq!(device_names(&snapshot.devices), ["nvme0n1p3"]);
    }

    #[test]
    fn sampler_reuses_cached_optional_commands_between_refreshes() {
        let temp = TempDir::new().unwrap();
        let diskstats = temp.path().join("diskstats");
        let sys_block = temp.path().join("sys/block");
        let mounts = temp.path().join("mounts");
        let mdstat = temp.path().join("mdstat");
        write(&diskstats, "");
        write(&sys_block.join("sda/size"), "2097152\n");
        write(&mounts, "");
        write(
            &mdstat,
            "md0 : active raid1 sdb1[1] sda1[0]\n      1046528 blocks super 1.2 [2/2] [UU]\n",
        );

        let mut sampler = Sampler::new_for_tests_with_paths(diskstats, sys_block, mounts, mdstat);
        sampler.optional_commands_enabled = true;
        let started = Instant::now();
        sampler.optional_cache = OptionalCommandCache {
            zfs: vec![Zpool {
                name: "tank".to_string(),
                size: "1T".to_string(),
                allocated: "100G".to_string(),
                free: "900G".to_string(),
                health: "ONLINE".to_string(),
                status: None,
            }],
            mdadm_scan: vec!["ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0".to_string()],
            diagnostics: vec!["cached optional data".to_string()],
            device_names: vec!["sda".to_string()],
            collected_at: Some(started),
            ..OptionalCommandCache::default()
        };

        let snapshot = sampler.sample_at(started + Duration::from_secs(1));

        assert_eq!(snapshot.zfs[0].name, "tank");
        assert_eq!(
            snapshot.mdraid[0].detail.as_deref(),
            Some("ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0")
        );
        assert_eq!(snapshot.diagnostics, ["cached optional data"]);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 64.0,
            "expected {actual} to be close to {expected}"
        );
    }

    fn names(activity: &[DiskActivity]) -> Vec<&str> {
        activity
            .iter()
            .map(|activity| activity.name.as_str())
            .collect()
    }

    fn device_names(devices: &[BlockDevice]) -> Vec<&str> {
        devices.iter().map(|device| device.name.as_str()).collect()
    }

    fn filesystem_sources(filesystems: &[FilesystemUsage]) -> Vec<&str> {
        filesystems
            .iter()
            .map(|filesystem| filesystem.source.as_str())
            .collect()
    }
}
