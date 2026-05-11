use crate::commands;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeGroup {
    pub name: String,
    pub size: String,
    pub free: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicalVolume {
    pub name: String,
    pub vg_name: String,
    pub size: String,
    pub free: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogicalVolume {
    pub name: String,
    pub vg_name: String,
    pub size: String,
    pub attr: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LvmSnapshot {
    pub volume_groups: Vec<VolumeGroup>,
    pub physical_volumes: Vec<PhysicalVolume>,
    pub logical_volumes: Vec<LogicalVolume>,
}

pub fn parse_vgs(input: &str) -> Vec<VolumeGroup> {
    parse_rows(input)
        .into_iter()
        .filter_map(|fields| {
            Some(VolumeGroup {
                name: fields.first()?.to_string(),
                size: fields.get(1)?.to_string(),
                free: fields.get(2)?.to_string(),
            })
        })
        .collect()
}

pub fn parse_pvs(input: &str) -> Vec<PhysicalVolume> {
    parse_rows(input)
        .into_iter()
        .filter_map(|fields| {
            Some(PhysicalVolume {
                name: fields.first()?.to_string(),
                vg_name: fields.get(1)?.to_string(),
                size: fields.get(2)?.to_string(),
                free: fields.get(3)?.to_string(),
            })
        })
        .collect()
}

pub fn parse_lvs(input: &str) -> Vec<LogicalVolume> {
    parse_rows(input)
        .into_iter()
        .filter_map(|fields| {
            Some(LogicalVolume {
                name: fields.first()?.to_string(),
                vg_name: fields.get(1)?.to_string(),
                size: fields.get(2)?.to_string(),
                attr: fields.get(3)?.to_string(),
            })
        })
        .collect()
}

pub fn collect(timeout: Duration) -> (LvmSnapshot, Vec<String>) {
    let mut snapshot = LvmSnapshot::default();
    let mut diagnostics = Vec::new();

    let pvs = commands::run_optional(
        "pvs",
        &[
            "--noheadings",
            "--separator",
            "\t",
            "-o",
            "pv_name,vg_name,pv_size,pv_free",
        ],
        timeout,
    );
    if let Some(output) = pvs.output {
        snapshot.physical_volumes = parse_pvs(&output);
    }
    if let Some(diagnostic) = pvs.diagnostic {
        diagnostics.push(diagnostic);
    }

    let vgs = commands::run_optional(
        "vgs",
        &[
            "--noheadings",
            "--separator",
            "\t",
            "-o",
            "vg_name,vg_size,vg_free",
        ],
        timeout,
    );
    if let Some(output) = vgs.output {
        snapshot.volume_groups = parse_vgs(&output);
    }
    if let Some(diagnostic) = vgs.diagnostic {
        diagnostics.push(diagnostic);
    }

    let lvs = commands::run_optional(
        "lvs",
        &[
            "--noheadings",
            "--separator",
            "\t",
            "-o",
            "lv_name,vg_name,lv_size,lv_attr",
        ],
        timeout,
    );
    if let Some(output) = lvs.output {
        snapshot.logical_volumes = parse_lvs(&output);
    }
    if let Some(diagnostic) = lvs.diagnostic {
        diagnostics.push(diagnostic);
    }

    (snapshot, diagnostics)
}

fn parse_rows(input: &str) -> Vec<Vec<&str>> {
    input
        .lines()
        .map(|line| line.split('\t').map(str::trim).collect::<Vec<_>>())
        .filter(|fields| fields.iter().any(|field| !field.is_empty()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lvm_rows() {
        let input = "vg0\t500.00g\t100.00g\n";
        let groups = parse_vgs(input);
        assert_eq!(groups[0].name, "vg0");
        assert_eq!(groups[0].size, "500.00g");
    }

    #[test]
    fn parses_lvm_physical_and_logical_volume_rows() {
        let pvs = parse_pvs("/dev/sda2\tvg0\t500.00g\t100.00g\n");
        assert_eq!(pvs[0].name, "/dev/sda2");
        assert_eq!(pvs[0].vg_name, "vg0");

        let lvs = parse_lvs("root\tvg0\t100.00g\t-wi-ao----\n");
        assert_eq!(lvs[0].name, "root");
        assert_eq!(lvs[0].vg_name, "vg0");
    }
}
