use crate::block::{collect as collect_block_devices, BlockDevice};
use crate::diskstats::{activities_between, read_diskstats, DiskActivity, DiskStat};
use crate::filesystems::{collect as collect_filesystems, FilesystemUsage};
use crate::lvm::{self, LvmSnapshot};
use crate::raid::{read_mdstat, MdArray};
use crate::smart::{self, SmartHealth};
use crate::zfs::{self, Zpool};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_millis(750);

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

#[derive(Debug)]
pub struct Sampler {
    diskstats_path: PathBuf,
    sys_block_root: PathBuf,
    mounts_path: PathBuf,
    mdstat_path: PathBuf,
    optional_commands_enabled: bool,
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
            optional_commands_enabled: true,
            previous_diskstats: Vec::new(),
            previous_at: None,
        }
    }
}

impl Sampler {
    pub fn new_for_tests(diskstats_path: PathBuf) -> Self {
        Self::new_for_tests_with_roots(diskstats_path, PathBuf::from("/sys/block"))
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
            optional_commands_enabled: false,
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

    pub fn sample_at(&mut self, now: Instant) -> Snapshot {
        let current_diskstats = read_diskstats(&self.diskstats_path);
        let activity = self
            .previous_at
            .map(|previous_at| {
                activities_between(
                    &self.previous_diskstats,
                    &current_diskstats,
                    now.duration_since(previous_at),
                )
            })
            .unwrap_or_default();

        let devices = collect_block_devices(&self.sys_block_root);
        let filesystems = collect_filesystems(&self.mounts_path);
        let mdraid = read_mdstat(&self.mdstat_path);
        let (zfs, lvm, smart, diagnostics) = self.collect_optional_commands(&devices);

        self.previous_diskstats = current_diskstats;
        self.previous_at = Some(now);

        Snapshot {
            activity,
            filesystems,
            devices,
            mdraid,
            zfs,
            lvm,
            smart,
            diagnostics,
        }
    }

    fn collect_optional_commands(
        &self,
        devices: &[BlockDevice],
    ) -> (Vec<Zpool>, LvmSnapshot, Vec<SmartHealth>, Vec<String>) {
        if !self.optional_commands_enabled {
            return (Vec::new(), LvmSnapshot::default(), Vec::new(), Vec::new());
        }

        let mut diagnostics = Vec::new();

        let (zfs, zfs_diagnostics) = zfs::collect(DEFAULT_COMMAND_TIMEOUT);
        diagnostics.extend(zfs_diagnostics);

        let (lvm, lvm_diagnostics) = lvm::collect(DEFAULT_COMMAND_TIMEOUT);
        diagnostics.extend(lvm_diagnostics);

        let (smart, smart_diagnostics) = smart::collect(devices, DEFAULT_COMMAND_TIMEOUT);
        diagnostics.extend(smart_diagnostics);

        (zfs, lvm, smart, diagnostics)
    }
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
        let first = sampler.sample_at(Instant::now());
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
        assert!(first
            .activity
            .iter()
            .all(|device| device.read_bytes_per_sec.is_none()));

        write(
            &sampler.diskstats_path,
            "8 0 sda 12 0 712 0 9 0 592 0 0 140 150 0 0 0 0 0 0\n",
        );
        let second = sampler.sample_at(Instant::now() + Duration::from_secs(1));
        assert_eq!(second.activity[0].name, "sda");
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

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 64.0,
            "expected {actual} to be close to {expected}"
        );
    }
}
