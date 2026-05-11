use crate::block::BlockDevice;
use crate::commands;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartHealth {
    pub device: String,
    pub health: Option<String>,
    pub temperature_celsius: Option<u64>,
    pub power_on_hours: Option<u64>,
    pub wearout_percent: Option<u64>,
}

pub fn parse_smartctl(device: &str, input: &str) -> SmartHealth {
    let mut health = SmartHealth {
        device: device.to_string(),
        ..SmartHealth::default()
    };

    for line in input.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.contains("overall-health") || lower.contains("smart health status") {
            health.health = parse_health_status(trimmed);
        }

        if is_temperature_line(&lower) {
            health.temperature_celsius =
                parse_temperature_celsius(trimmed).or(health.temperature_celsius);
        }

        if lower.contains("power_on_hours") || lower.contains("power on hours") {
            health.power_on_hours = last_integer(trimmed).or(health.power_on_hours);
        }

        if is_wearout_line(trimmed) {
            health.wearout_percent = last_integer(trimmed).or(health.wearout_percent);
        }
    }

    health
}

pub fn collect(devices: &[BlockDevice], timeout: Duration) -> (Vec<SmartHealth>, Vec<String>) {
    let mut health = Vec::new();
    let mut diagnostics = Vec::new();

    for device in devices
        .iter()
        .filter(|device| should_collect_device(device))
    {
        let path = format!("/dev/{}", device.name);
        let result = commands::run_optional("smartctl", &["-A", "-H", &path], timeout);
        if let Some(output) = result.output {
            health.push(parse_smartctl(&path, &output));
        }
        if let Some(diagnostic) = result.diagnostic {
            diagnostics.push(format!("{path}: {diagnostic}"));
        }
    }

    (health, diagnostics)
}

fn parse_health_status(line: &str) -> Option<String> {
    let (_, status) = line.split_once(':')?;
    let status = status.trim();
    if status.is_empty() {
        None
    } else {
        Some(status.to_string())
    }
}

fn is_temperature_line(lower: &str) -> bool {
    lower.contains("temperature_celsius")
        || lower.contains("airflow_temperature")
        || lower.contains("temperature_internal")
        || lower.contains("current drive temperature")
        || lower.starts_with("temperature:")
}

fn parse_temperature_celsius(line: &str) -> Option<u64> {
    let current_value = line.split_once('(').map_or(line, |(value, _)| value);
    last_integer(current_value)
}

fn is_wearout_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("wear")
        || lower.contains("media_wearout")
        || lower.contains("percent_lifetime")
        || lower.contains("percentage_used")
        || lower.contains("percentage used")
}

fn should_collect_device(device: &BlockDevice) -> bool {
    !device.name.starts_with("loop")
        && matches!(
            device.device_type.as_str(),
            "disk" | "nvme" | "mmc" | "zbc" | "dm"
        )
}

fn last_integer(input: &str) -> Option<u64> {
    input
        .split(|character: char| !character.is_ascii_digit())
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse().ok())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smartctl_health() {
        let input = "SMART overall-health self-assessment test result: PASSED\nTemperature_Celsius     0x0022   30\nPower_On_Hours          0x0032   1234\n";
        let health = parse_smartctl("/dev/sda", input);
        assert_eq!(health.device, "/dev/sda");
        assert_eq!(health.health.as_deref(), Some("PASSED"));
        assert_eq!(health.temperature_celsius, Some(30));
        assert_eq!(health.power_on_hours, Some(1234));
    }

    #[test]
    fn parses_current_temperature_before_min_max_values() {
        let input = "194 Temperature_Celsius     0x0022   064   052   000    Old_age   Always       -       36 (Min/Max 18/49)\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.temperature_celsius, Some(36));
    }

    #[test]
    fn parses_nvme_percentage_used() {
        let input = "Percentage Used:                    2%\n";

        let health = parse_smartctl("/dev/nvme0n1", input);

        assert_eq!(health.wearout_percent, Some(2));
    }

    #[test]
    fn parses_ata_wear_leveling_count() {
        let input = "177 Wear_Leveling_Count     0x0013   096   096   010    Pre-fail  Always       -       4\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(4));
    }
}
