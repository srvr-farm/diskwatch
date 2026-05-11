use crate::commands;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MdArray {
    pub name: String,
    pub level: Option<String>,
    pub devices: Vec<String>,
    pub status: Option<String>,
    pub blocks: Option<u64>,
    pub detail: Option<String>,
}

pub fn read_mdstat(path: &Path) -> Vec<MdArray> {
    fs::read_to_string(path)
        .map(|input| parse_mdstat(&input))
        .unwrap_or_default()
}

pub fn parse_mdstat(input: &str) -> Vec<MdArray> {
    let mut arrays = Vec::new();
    let mut current: Option<MdArray> = None;

    for line in input.lines() {
        if let Some((name, rest)) = line.split_once(" : ") {
            let name = name.trim();
            if !name.starts_with("md") {
                continue;
            }

            if let Some(array) = current.take() {
                arrays.push(array);
            }

            let tokens: Vec<_> = rest.split_whitespace().collect();
            let level = tokens
                .iter()
                .find(|token| token.starts_with("raid"))
                .map(|token| (*token).to_string());
            let devices = tokens
                .iter()
                .filter(|token| token.contains('[') && !token.starts_with('['))
                .map(|token| token.trim_end_matches(',').to_string())
                .collect();

            current = Some(MdArray {
                name: name.to_string(),
                level,
                devices,
                status: None,
                blocks: None,
                detail: None,
            });
        } else if let Some(array) = current.as_mut() {
            apply_detail_line(array, line);
        }
    }

    if let Some(array) = current {
        arrays.push(array);
    }

    arrays
}

pub fn parse_mdadm_detail_scan(input: &str) -> Vec<String> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn collect_mdadm_detail_scan(timeout: Duration) -> (Vec<String>, Vec<String>) {
    let result = commands::run_optional("mdadm", &["--detail", "--scan"], timeout);
    let details = result
        .output
        .as_deref()
        .map(parse_mdadm_detail_scan)
        .unwrap_or_default();
    let diagnostics = result.diagnostic.into_iter().collect();

    (details, diagnostics)
}

pub fn apply_mdadm_detail_scan(arrays: &mut [MdArray], details: &[String]) {
    for detail in details {
        let Some(name) = mdadm_detail_array_name(detail) else {
            continue;
        };

        if let Some(array) = arrays.iter_mut().find(|array| array.name == name) {
            array.detail = Some(detail.clone());
        }
    }
}

fn mdadm_detail_array_name(detail: &str) -> Option<String> {
    let mut fields = detail.split_whitespace();
    if fields.next()? != "ARRAY" {
        return None;
    }

    let path = fields.next()?;
    path.rsplit('/').next().map(str::to_string)
}

fn apply_detail_line(array: &mut MdArray, line: &str) {
    let tokens: Vec<_> = line.split_whitespace().collect();
    if let Some(index) = tokens.iter().position(|token| *token == "blocks") {
        array.blocks = index
            .checked_sub(1)
            .and_then(|blocks_index| tokens.get(blocks_index))
            .and_then(|blocks| blocks.parse().ok());
    }

    array.status = tokens
        .iter()
        .rev()
        .find(|token| {
            token.starts_with('[')
                && token.ends_with(']')
                && token
                    .chars()
                    .any(|character| matches!(character, 'U' | '_'))
        })
        .map(|token| (*token).to_string())
        .or_else(|| array.status.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mdstat_arrays() {
        let input =
            "md0 : active raid1 sdb1[1] sda1[0]\n      1046528 blocks super 1.2 [2/2] [UU]\n";
        let arrays = parse_mdstat(input);
        assert_eq!(arrays[0].name, "md0");
        assert_eq!(arrays[0].level.as_deref(), Some("raid1"));
        assert_eq!(arrays[0].status.as_deref(), Some("[UU]"));
    }

    #[test]
    fn skips_mdstat_headers_and_unused_devices() {
        let input = "Personalities : [raid1] [raid6]\nmd0 : active raid1 sdb1[1] sda1[0]\n      1046528 blocks super 1.2 [2/2] [UU]\nmd1 : active raid5 sdc1[0] sdd1[1]\n      2093056 blocks super 1.2 [2/2] [UU]\nunused devices: <none>\n";

        let arrays = parse_mdstat(input);

        assert_eq!(arrays.len(), 2);
        assert_eq!(arrays[0].name, "md0");
        assert_eq!(arrays[1].name, "md1");
    }

    #[test]
    fn parses_mdadm_detail_scan_lines() {
        let input =
            "\nARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0\n\nARRAY /dev/md1 UUID=def\n";

        let lines = parse_mdadm_detail_scan(input);

        assert_eq!(
            lines,
            [
                "ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0",
                "ARRAY /dev/md1 UUID=def"
            ]
        );
    }

    #[test]
    fn applies_mdadm_detail_scan_to_matching_mdstat_arrays() {
        let mut arrays = parse_mdstat(
            "md0 : active raid1 sdb1[1] sda1[0]\n      1046528 blocks super 1.2 [2/2] [UU]\n",
        );
        let details = parse_mdadm_detail_scan("ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0\n");

        apply_mdadm_detail_scan(&mut arrays, &details);

        assert_eq!(
            arrays[0].detail.as_deref(),
            Some("ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0")
        );
    }
}
