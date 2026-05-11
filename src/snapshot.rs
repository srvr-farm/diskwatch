use crate::block::{collect as collect_block_devices, BlockDevice};
use crate::diskstats::{activities_between, read_diskstats, DiskActivity, DiskStat};
use crate::filesystems::{collect as collect_filesystems, FilesystemUsage};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub activity: Vec<DiskActivity>,
    pub devices: Vec<BlockDevice>,
    pub filesystems: Vec<FilesystemUsage>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug)]
pub struct Sampler {
    diskstats_path: PathBuf,
    sys_block_root: PathBuf,
    mounts_path: PathBuf,
    previous_diskstats: Vec<DiskStat>,
    previous_at: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            diskstats_path: PathBuf::from("/proc/diskstats"),
            sys_block_root: PathBuf::from("/sys/block"),
            mounts_path: PathBuf::from("/proc/mounts"),
            previous_diskstats: Vec::new(),
            previous_at: None,
        }
    }
}

impl Sampler {
    pub fn new_for_tests(diskstats_path: PathBuf) -> Self {
        Self::new_for_tests_with_roots(diskstats_path, PathBuf::from("/sys/block"))
    }

    pub fn new_for_tests_with_roots(diskstats_path: PathBuf, sys_block_root: PathBuf) -> Self {
        Self {
            diskstats_path,
            sys_block_root,
            mounts_path: PathBuf::from("/proc/mounts"),
            previous_diskstats: Vec::new(),
            previous_at: None,
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
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

        self.previous_diskstats = current_diskstats;
        self.previous_at = Some(now);

        Snapshot {
            activity,
            devices: collect_block_devices(&self.sys_block_root),
            filesystems: collect_filesystems(&self.mounts_path),
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    #[test]
    fn sampler_reports_activity_from_diskstats_deltas() {
        let file = NamedTempFile::new().unwrap();
        let sys_block = TempDir::new().unwrap();
        fs::write(
            file.path(),
            "   8       0 sda 10 0 200 30 5 0 80 20 0 40 50 0 0 0 0 0 0\n",
        )
        .unwrap();

        let mut sampler = Sampler::new_for_tests_with_roots(
            file.path().to_path_buf(),
            sys_block.path().to_path_buf(),
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
        let sda = sys_block.path().join("sda");
        fs::create_dir_all(&sda).unwrap();
        fs::write(sda.join("size"), "2097152\n").unwrap();

        let mut sampler = Sampler::new_for_tests_with_roots(
            diskstats.path().to_path_buf(),
            sys_block.path().to_path_buf(),
        );

        let snapshot = sampler.sample();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].name, "sda");
        assert_eq!(snapshot.devices[0].size_bytes, 1_073_741_824);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 64.0,
            "expected {actual} to be close to {expected}"
        );
    }
}
