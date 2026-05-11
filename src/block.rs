use std::fs;
use std::path::Path;

const SECTOR_BYTES: u64 = 512;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockDevice {
    pub name: String,
    pub device_type: String,
    pub size_bytes: u64,
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
    } else if name.starts_with("dm-") {
        "dm"
    } else if name.starts_with("nvme") {
        "nvme"
    } else if name.starts_with("mmc") {
        "mmc"
    } else {
        "disk"
    }
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
}
