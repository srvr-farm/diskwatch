use std::fs;
use std::path::Path;

const SECTOR_BYTES: u64 = 512;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockDevice {
    pub name: String,
    pub device_type: String,
    pub size_bytes: u64,
    pub slaves: Vec<String>,
    pub rotational: Option<bool>,
    pub logical_block_size: Option<u64>,
    pub physical_block_size: Option<u64>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}

pub fn collect(sys_block_root: &Path) -> Vec<BlockDevice> {
    let mut devices: Vec<_> = fs::read_dir(sys_block_root)
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| collect_device(&entry.path()))
        .collect();

    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
}

fn collect_device(path: &Path) -> Option<BlockDevice> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let size_sectors = read_u64(&path.join("size")).unwrap_or_default();

    Some(BlockDevice {
        device_type: device_type(path, &name),
        size_bytes: size_sectors.saturating_mul(SECTOR_BYTES),
        slaves: read_slave_names(&path.join("slaves")),
        rotational: read_bool(&path.join("queue/rotational")),
        logical_block_size: read_u64(&path.join("queue/logical_block_size")),
        physical_block_size: read_u64(&path.join("queue/physical_block_size")),
        vendor: read_string(&path.join("device/vendor")),
        model: read_string(&path.join("device/model")),
        serial: read_string(&path.join("device/serial")),
        name,
    })
}

fn device_type(path: &Path, name: &str) -> String {
    read_string(&path.join("device/type"))
        .and_then(|value| scsi_device_type(&value).map(str::to_string))
        .unwrap_or_else(|| infer_device_type(name).to_string())
}

fn scsi_device_type(value: &str) -> Option<&'static str> {
    match value {
        "0" => Some("disk"),
        "5" => Some("cdrom"),
        "7" => Some("optical"),
        "14" => Some("zbc"),
        _ => None,
    }
}

fn infer_device_type(name: &str) -> &'static str {
    if name.starts_with("loop") {
        "loop"
    } else if name.starts_with("ram") {
        "ram"
    } else if name.starts_with("md") {
        "md"
    } else if name == "dm" || name.starts_with("dm-") {
        "dm"
    } else if name.starts_with("nvme") {
        "nvme"
    } else if name.starts_with("mmc") {
        "mmc"
    } else {
        "disk"
    }
}

fn read_slave_names(path: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

fn read_bool(path: &Path) -> Option<bool> {
    match read_string(path)?.as_str() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn read_u64(path: &Path) -> Option<u64> {
    read_string(path)?.parse().ok()
}

fn read_string(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, value: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    }

    #[test]
    fn collects_block_devices_from_sysfs() {
        let temp = TempDir::new().unwrap();
        let sda = temp.path().join("sda");
        write(&sda.join("size"), "2097152\n");
        write(&sda.join("queue/rotational"), "0\n");
        write(&sda.join("queue/logical_block_size"), "512\n");
        write(&sda.join("queue/physical_block_size"), "4096\n");
        write(&sda.join("device/model"), "FastDisk\n");
        write(&sda.join("device/vendor"), "ACME\n");
        write(&sda.join("device/serial"), "XYZ123\n");

        let devices = collect(temp.path());
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "sda");
        assert_eq!(devices[0].device_type, "disk");
        assert_eq!(devices[0].size_bytes, 1_073_741_824);
        assert_eq!(devices[0].rotational, Some(false));
        assert_eq!(devices[0].logical_block_size, Some(512));
        assert_eq!(devices[0].physical_block_size, Some(4096));
        assert_eq!(devices[0].model.as_deref(), Some("FastDisk"));
    }

    #[test]
    fn sorts_multiple_devices_by_name() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join("sdb")).unwrap();
        std::fs::create_dir(temp.path().join("nvme0n1")).unwrap();
        std::fs::create_dir(temp.path().join("sda")).unwrap();

        let names: Vec<_> = collect(temp.path())
            .into_iter()
            .map(|device| device.name)
            .collect();

        assert_eq!(names, ["nvme0n1", "sda", "sdb"]);
    }

    #[test]
    fn ignores_non_directory_entries() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir(temp.path().join("sda")).unwrap();
        write(&temp.path().join("not-a-device"), "ignored\n");

        let devices = collect(temp.path());

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "sda");
    }

    #[test]
    fn missing_and_malformed_fields_use_defaults_without_panicking() {
        let temp = TempDir::new().unwrap();
        let sda = temp.path().join("sda");
        write(&sda.join("size"), "not-a-number\n");
        write(&sda.join("queue/rotational"), "maybe\n");
        write(&sda.join("queue/logical_block_size"), "large\n");

        let devices = collect(temp.path());

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "sda");
        assert_eq!(devices[0].size_bytes, 0);
        assert_eq!(devices[0].rotational, None);
        assert_eq!(devices[0].logical_block_size, None);
        assert_eq!(devices[0].physical_block_size, None);
        assert_eq!(devices[0].vendor, None);
        assert_eq!(devices[0].model, None);
        assert_eq!(devices[0].serial, None);
    }

    #[test]
    fn empty_trimmed_metadata_becomes_none() {
        let temp = TempDir::new().unwrap();
        let sda = temp.path().join("sda");
        write(&sda.join("device/vendor"), " \n\t");
        write(&sda.join("device/model"), "\n");
        write(&sda.join("device/serial"), "\t\n");

        let devices = collect(temp.path());

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].vendor, None);
        assert_eq!(devices[0].model, None);
        assert_eq!(devices[0].serial, None);
    }

    #[test]
    fn maps_common_scsi_device_types() {
        let cases = [
            ("sda", "0", "disk"),
            ("sr0", "5", "cdrom"),
            ("opt0", "7", "optical"),
            ("zbc0", "14", "zbc"),
        ];

        for (name, scsi_type, expected_type) in cases {
            let temp = TempDir::new().unwrap();
            write(&temp.path().join(name).join("device/type"), scsi_type);

            let devices = collect(temp.path());

            assert_eq!(devices.len(), 1);
            assert_eq!(devices[0].device_type, expected_type);
        }
    }

    #[test]
    fn collects_device_slave_names() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("dm-0/slaves/nvme0n1p3")).unwrap();
        std::fs::create_dir_all(temp.path().join("dm-0/slaves/nvme0n1p2")).unwrap();

        let devices = collect(temp.path());

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "dm-0");
        assert_eq!(devices[0].slaves, ["nvme0n1p2", "nvme0n1p3"]);
    }

    #[test]
    fn infers_device_types_from_names() {
        let cases = [
            ("loop0", "loop"),
            ("ram0", "ram"),
            ("md0", "md"),
            ("dm-0", "dm"),
            ("dm", "dm"),
            ("nvme0n1", "nvme"),
            ("mmcblk0", "mmc"),
            ("sda", "disk"),
        ];

        for (name, expected_type) in cases {
            let temp = TempDir::new().unwrap();
            std::fs::create_dir(temp.path().join(name)).unwrap();

            let devices = collect(temp.path());

            assert_eq!(devices.len(), 1);
            assert_eq!(devices[0].device_type, expected_type);
        }
    }
}
