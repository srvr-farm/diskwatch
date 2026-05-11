use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

const SECTOR_BYTES: f64 = 512.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskStat {
    pub name: String,
    pub reads_completed: u64,
    pub sectors_read: u64,
    pub writes_completed: u64,
    pub sectors_written: u64,
    pub io_time_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskActivity {
    pub name: String,
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    pub read_iops: Option<f64>,
    pub write_iops: Option<f64>,
    pub busy_percent: Option<f64>,
}

pub fn read_diskstats(path: &Path) -> Vec<DiskStat> {
    fs::read_to_string(path)
        .map(|input| parse_diskstats(&input))
        .unwrap_or_default()
}

pub fn parse_diskstats(input: &str) -> Vec<DiskStat> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() <= 12 {
                return None;
            }

            Some(DiskStat {
                name: fields[2].to_string(),
                reads_completed: fields[3].parse().ok()?,
                sectors_read: fields[5].parse().ok()?,
                writes_completed: fields[7].parse().ok()?,
                sectors_written: fields[9].parse().ok()?,
                io_time_ms: fields[12].parse().ok()?,
            })
        })
        .collect()
}

pub fn activity_between(
    prev: &DiskStat,
    curr: &DiskStat,
    elapsed: Duration,
) -> Option<DiskActivity> {
    if prev.name != curr.name {
        return None;
    }

    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= 0.0 {
        return None;
    }

    let sectors_read = curr.sectors_read.wrapping_sub(prev.sectors_read);
    let sectors_written = curr.sectors_written.wrapping_sub(prev.sectors_written);
    let reads_completed = curr.reads_completed.wrapping_sub(prev.reads_completed);
    let writes_completed = curr.writes_completed.wrapping_sub(prev.writes_completed);
    let io_time_ms = curr.io_time_ms.wrapping_sub(prev.io_time_ms);

    Some(DiskActivity {
        name: curr.name.clone(),
        read_bytes_per_sec: Some(sectors_read as f64 * SECTOR_BYTES / elapsed_secs),
        write_bytes_per_sec: Some(sectors_written as f64 * SECTOR_BYTES / elapsed_secs),
        read_iops: Some(reads_completed as f64 / elapsed_secs),
        write_iops: Some(writes_completed as f64 / elapsed_secs),
        busy_percent: Some(io_time_ms as f64 / (elapsed_secs * 1_000.0) * 100.0),
    })
}

pub fn activities_between(
    prev: &[DiskStat],
    curr: &[DiskStat],
    elapsed: Duration,
) -> Vec<DiskActivity> {
    let prev_by_name: HashMap<&str, &DiskStat> =
        prev.iter().map(|stat| (stat.name.as_str(), stat)).collect();

    curr.iter()
        .filter_map(|current| {
            prev_by_name
                .get(current.name.as_str())
                .and_then(|previous| activity_between(previous, current, elapsed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diskstats_rows() {
        let input = "   8       0 sda 10 0 200 30 5 0 80 20 0 40 50 0 0 0 0 0 0\n";
        let stats = parse_diskstats(input);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "sda");
        assert_eq!(stats[0].reads_completed, 10);
        assert_eq!(stats[0].sectors_read, 200);
        assert_eq!(stats[0].writes_completed, 5);
        assert_eq!(stats[0].sectors_written, 80);
        assert_eq!(stats[0].io_time_ms, 40);
    }

    #[test]
    fn calculates_rates_from_counter_deltas() {
        let prev = DiskStat {
            name: "sda".to_string(),
            reads_completed: 10,
            sectors_read: 200,
            writes_completed: 5,
            sectors_written: 80,
            io_time_ms: 40,
        };
        let curr = DiskStat {
            name: "sda".to_string(),
            reads_completed: 16,
            sectors_read: 1224,
            writes_completed: 9,
            sectors_written: 592,
            io_time_ms: 240,
        };
        let activity = activity_between(&prev, &curr, Duration::from_secs(2)).unwrap();
        assert_eq!(activity.name, "sda");
        assert_eq!(activity.read_bytes_per_sec, Some(262_144.0));
        assert_eq!(activity.write_bytes_per_sec, Some(131_072.0));
        assert_eq!(activity.read_iops, Some(3.0));
        assert_eq!(activity.write_iops, Some(2.0));
        assert_eq!(activity.busy_percent, Some(10.0));
    }
}
