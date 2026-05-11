use crate::commands;
use std::time::Duration;

const PVS_ARGS: &[&str] = &[
    "--readonly",
    "--noheadings",
    "--separator",
    "\t",
    "-o",
    "pv_name,vg_name,pv_size,pv_free",
];
const VGS_ARGS: &[&str] = &[
    "--readonly",
    "--noheadings",
    "--separator",
    "\t",
    "-o",
    "vg_name,vg_size,vg_free",
];
const LVS_ARGS: &[&str] = &[
    "--readonly",
    "--noheadings",
    "--separator",
    "\t",
    "-o",
    "lv_name,vg_name,lv_size,lv_attr",
];

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
    collect_with_runner(|program, args| Some(commands::run_optional(program, args, timeout)))
}

pub fn collect_budgeted(budget: &commands::OptionalCommandBudget) -> (LvmSnapshot, Vec<String>) {
    collect_with_runner(|program, args| commands::run_optional_budgeted(program, args, budget))
}

fn collect_with_runner<F>(mut run: F) -> (LvmSnapshot, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    let mut snapshot = LvmSnapshot::default();
    let mut diagnostics = Vec::new();

    if let Some(pvs) = run("pvs", pvs_args()) {
        if let Some(output) = pvs.output {
            snapshot.physical_volumes = parse_pvs(&output);
        }
        if let Some(diagnostic) = pvs.diagnostic {
            diagnostics.push(diagnostic);
        }
    } else {
        return (snapshot, diagnostics);
    }

    if let Some(vgs) = run("vgs", vgs_args()) {
        if let Some(output) = vgs.output {
            snapshot.volume_groups = parse_vgs(&output);
        }
        if let Some(diagnostic) = vgs.diagnostic {
            diagnostics.push(diagnostic);
        }
    } else {
        return (snapshot, diagnostics);
    }

    if let Some(lvs) = run("lvs", lvs_args()) {
        if let Some(output) = lvs.output {
            snapshot.logical_volumes = parse_lvs(&output);
        }
        if let Some(diagnostic) = lvs.diagnostic {
            diagnostics.push(diagnostic);
        }
    }

    (snapshot, diagnostics)
}

fn pvs_args() -> &'static [&'static str] {
    PVS_ARGS
}

fn vgs_args() -> &'static [&'static str] {
    VGS_ARGS
}

fn lvs_args() -> &'static [&'static str] {
    LVS_ARGS
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

    #[test]
    fn lvm_command_args_are_read_only() {
        assert!(pvs_args().contains(&"--readonly"));
        assert!(vgs_args().contains(&"--readonly"));
        assert!(lvs_args().contains(&"--readonly"));
    }
}
