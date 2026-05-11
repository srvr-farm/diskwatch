use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const SKIPPED_FS_TYPES: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "cgroup",
    "cgroup2",
    "securityfs",
    "pstore",
    "bpf",
    "tracefs",
    "debugfs",
    "configfs",
    "fusectl",
    "mqueue",
    "hugetlbfs",
    "autofs",
];

const BLOCKING_PRONE_FS_TYPES: &[&str] = &[
    "9p",
    "afpfs",
    "ceph",
    "cifs",
    "davfs",
    "glusterfs",
    "lustre",
    "nfs",
    "nfs4",
    "smb3",
    "smbfs",
    "sshfs",
    "virtiofs",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mount {
    pub source: String,
    pub mountpoint: String,
    pub fs_type: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FsStats {
    pub block_size: u64,
    pub blocks: u64,
    pub blocks_available: u64,
    pub blocks_free: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilesystemUsage {
    pub source: String,
    pub mountpoint: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: Option<f64>,
}

pub fn read_mounts(path: &Path) -> Vec<Mount> {
    fs::read_to_string(path)
        .map(|input| parse_mounts(&input))
        .unwrap_or_default()
}

pub fn parse_mounts(input: &str) -> Vec<Mount> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 3 {
                return None;
            }

            Some(Mount {
                source: unescape_mount_field(fields[0]),
                mountpoint: unescape_mount_field(fields[1]),
                fs_type: unescape_mount_field(fields[2]),
            })
        })
        .collect()
}

pub fn stat_mount(path: &Path) -> Option<FsStats> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();

    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let stats = unsafe { stats.assume_init() };
    let fragment_size = stats.f_frsize;
    Some(FsStats {
        block_size: if fragment_size == 0 {
            stats.f_bsize
        } else {
            fragment_size
        },
        blocks: stats.f_blocks,
        blocks_available: stats.f_bavail,
        blocks_free: stats.f_bfree,
    })
}

pub fn summarize_mount(mount: &Mount, stats: FsStats) -> FilesystemUsage {
    let total_bytes = stats.blocks.saturating_mul(stats.block_size);
    let available_bytes = stats.blocks_available.saturating_mul(stats.block_size);
    let used_blocks = stats.blocks.saturating_sub(stats.blocks_free);
    let used_bytes = used_blocks.saturating_mul(stats.block_size);

    FilesystemUsage {
        source: mount.source.clone(),
        mountpoint: mount.mountpoint.clone(),
        fs_type: mount.fs_type.clone(),
        total_bytes,
        available_bytes,
        used_bytes,
        used_percent: if stats.blocks == 0 {
            None
        } else {
            Some(used_blocks as f64 / stats.blocks as f64 * 100.0)
        },
    }
}

pub fn collect(mounts_path: &Path) -> Vec<FilesystemUsage> {
    read_mounts(mounts_path)
        .into_iter()
        .filter(|mount| !should_skip_mount(mount))
        .filter_map(|mount| {
            let stats = stat_mount(Path::new(&mount.mountpoint))?;
            Some(summarize_mount(&mount, stats))
        })
        .collect()
}

fn should_skip_mount(mount: &Mount) -> bool {
    SKIPPED_FS_TYPES.contains(&mount.fs_type.as_str())
        || is_blocking_prone_filesystem(&mount.fs_type)
}

fn is_blocking_prone_filesystem(fs_type: &str) -> bool {
    BLOCKING_PRONE_FS_TYPES.contains(&fs_type) || fs_type == "fuse" || fs_type.starts_with("fuse.")
}

fn unescape_mount_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if octal.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
                if let Ok(byte) = u8::from_str_radix(octal, 8) {
                    output.push(byte as char);
                    index += 4;
                    continue;
                }
            }
        }

        let ch = value[index..].chars().next().expect("index is in bounds");
        output.push(ch);
        index += ch.len_utf8();
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_proc_mounts_with_escaped_spaces() {
        let input =
            "/dev/sda1 / ext4 rw,relatime 0 0\nserver:/share /mnt/My\\040Share nfs rw 0 0\n";
        let mounts = parse_mounts(input);
        assert_eq!(mounts[0].source, "/dev/sda1");
        assert_eq!(mounts[0].mountpoint, "/");
        assert_eq!(mounts[1].mountpoint, "/mnt/My Share");
    }

    #[test]
    fn summarizes_filesystem_capacity() {
        let mount = Mount {
            source: "/dev/sda1".to_string(),
            mountpoint: "/".to_string(),
            fs_type: "ext4".to_string(),
        };
        let summary = summarize_mount(
            &mount,
            FsStats {
                block_size: 4096,
                blocks: 1000,
                blocks_available: 250,
                blocks_free: 300,
            },
        );
        assert_eq!(summary.total_bytes, 4_096_000);
        assert_eq!(summary.available_bytes, 1_024_000);
        assert_eq!(summary.used_bytes, 2_867_200);
        assert_eq!(summary.used_percent, Some(70.0));
    }

    #[test]
    fn ignores_malformed_mount_lines() {
        let input = "not-enough-fields\n/dev/sda1 / ext4 rw,relatime 0 0\nmissing fs-type\n";

        let mounts = parse_mounts(input);

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, "/dev/sda1");
    }

    #[test]
    fn zero_block_filesystem_has_no_used_percent() {
        let mount = Mount {
            source: "/dev/empty".to_string(),
            mountpoint: "/empty".to_string(),
            fs_type: "ext4".to_string(),
        };

        let summary = summarize_mount(
            &mount,
            FsStats {
                block_size: 4096,
                blocks: 0,
                blocks_available: 0,
                blocks_free: 0,
            },
        );

        assert_eq!(summary.total_bytes, 0);
        assert_eq!(summary.used_percent, None);
    }

    #[test]
    fn collect_skips_kernel_pseudo_filesystems_but_keeps_capacity_mounts() {
        let mountpoint = TempDir::new().unwrap();
        let mounts = TempDir::new().unwrap();
        let mounts_path = mounts.path().join("mounts");
        std::fs::write(
            &mounts_path,
            format!(
                "proc {} proc rw 0 0\ntmpfs {} tmpfs rw 0 0\noverlay {} overlay rw 0 0\n",
                mountpoint.path().display(),
                mountpoint.path().display(),
                mountpoint.path().display()
            ),
        )
        .unwrap();

        let filesystems = collect(&mounts_path);

        let fs_types: Vec<_> = filesystems
            .iter()
            .map(|filesystem| filesystem.fs_type.as_str())
            .collect();
        assert_eq!(fs_types, ["tmpfs", "overlay"]);
    }

    #[test]
    fn collect_skips_remote_and_fuse_mounts_to_avoid_blocking_statvfs() {
        let mounts = TempDir::new().unwrap();
        let mounts_path = mounts.path().join("mounts");
        std::fs::write(
            &mounts_path,
            "server:/share /mnt/nfs nfs rw 0 0\n//server/share /mnt/cifs cifs rw 0 0\nsshfs#host:/data /mnt/sshfs fuse.sshfs rw 0 0\n",
        )
        .unwrap();

        let filesystems = collect(&mounts_path);

        assert!(filesystems.is_empty());
    }

    #[test]
    fn collect_skips_unavailable_mountpoints() {
        let mounts = TempDir::new().unwrap();
        let mounts_path = mounts.path().join("mounts");
        let unavailable = mounts.path().join("does-not-exist");
        std::fs::write(
            &mounts_path,
            format!("/dev/sda1 {} ext4 rw 0 0\n", unavailable.display()),
        )
        .unwrap();

        let filesystems = collect(&mounts_path);

        assert!(filesystems.is_empty());
    }
}
