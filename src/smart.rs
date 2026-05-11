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
            health.power_on_hours = parse_power_on_hours(trimmed).or(health.power_on_hours);
        }

        if let Some(wearout_percent) = parse_wearout_percent(trimmed) {
            health.wearout_percent = Some(wearout_percent);
        }
    }

    health
}

pub fn collect(devices: &[BlockDevice], timeout: Duration) -> (Vec<SmartHealth>, Vec<String>) {
    collect_with_availability(devices, timeout, commands::program_available("smartctl"))
}

pub fn collect_budgeted(
    devices: &[BlockDevice],
    budget: &commands::OptionalCommandBudget,
) -> (Vec<SmartHealth>, Vec<String>) {
    let candidates = collectable_devices(devices);
    if candidates.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if !commands::program_available("smartctl") {
        return (Vec::new(), vec!["smartctl not found".to_string()]);
    }

    collect_candidates_with_runner(&candidates, |program, args| {
        commands::run_optional_budgeted(program, args, budget)
    })
}

fn collect_with_availability(
    devices: &[BlockDevice],
    timeout: Duration,
    smartctl_available: bool,
) -> (Vec<SmartHealth>, Vec<String>) {
    let candidates = collectable_devices(devices);
    if candidates.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if !smartctl_available {
        return (Vec::new(), vec!["smartctl not found".to_string()]);
    }

    collect_candidates_with_runner(&candidates, |program, args| {
        Some(commands::run_optional(program, args, timeout))
    })
}

fn collectable_devices(devices: &[BlockDevice]) -> Vec<&BlockDevice> {
    devices
        .iter()
        .filter(|device| should_collect_device(device))
        .collect()
}

fn collect_candidates_with_runner<F>(
    devices: &[&BlockDevice],
    mut run: F,
) -> (Vec<SmartHealth>, Vec<String>)
where
    F: FnMut(&str, &[&str]) -> Option<commands::OptionalCommandOutput>,
{
    let mut health = Vec::new();
    let mut diagnostics = Vec::new();

    for device in devices {
        let path = format!("/dev/{}", device.name);
        let Some(result) = run("smartctl", &["-A", "-H", &path]) else {
            break;
        };
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

fn parse_power_on_hours(line: &str) -> Option<u64> {
    if let Some(raw_value) = smart_attribute_raw_value(line) {
        if let Some(hours) = first_integer(raw_value) {
            return Some(hours);
        }
    }

    last_integer(line)
}

fn parse_wearout_percent(line: &str) -> Option<u64> {
    let lower = line.to_ascii_lowercase();

    if lower.contains("percentage used") || lower.contains("percentage_used") {
        return last_integer(line);
    }

    if lower.contains("percent_lifetime_used")
        || lower.contains("lifetime") && lower.contains("used")
    {
        return last_integer(line);
    }

    if lower.contains("percent_lifetime")
        || lower.contains("lifetime") && (lower.contains("remain") || lower.contains("left"))
    {
        return last_integer(line).map(remaining_to_used_percent);
    }

    if lower.contains("media_wearout_indicator") || lower.contains("wear_leveling_count") {
        return smart_attribute_value(line).map(remaining_to_used_percent);
    }

    None
}

fn should_collect_device(device: &BlockDevice) -> bool {
    !device.name.starts_with("loop")
        && matches!(device.device_type.as_str(), "disk" | "nvme" | "mmc" | "zbc")
}

fn last_integer(input: &str) -> Option<u64> {
    let normalized = normalize_grouped_digits(input);
    normalized
        .split(|character: char| !character.is_ascii_digit())
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse().ok())
        .next_back()
}

fn smart_attribute_value(line: &str) -> Option<u64> {
    line.split_whitespace().nth(3)?.parse().ok()
}

fn smart_attribute_raw_value(line: &str) -> Option<&str> {
    line.split_whitespace().nth(9)
}

fn remaining_to_used_percent(remaining: u64) -> u64 {
    100_u64.saturating_sub(remaining)
}

fn first_integer(input: &str) -> Option<u64> {
    let normalized = normalize_grouped_digits(input);
    normalized
        .split(|character: char| !character.is_ascii_digit())
        .filter(|value| !value.is_empty())
        .find_map(|value| value.parse().ok())
}

fn normalize_grouped_digits(input: &str) -> String {
    let characters: Vec<_> = input.chars().collect();
    let mut normalized = String::with_capacity(input.len());

    for (index, character) in characters.iter().enumerate() {
        if *character == ','
            && index > 0
            && index + 1 < characters.len()
            && characters[index - 1].is_ascii_digit()
            && characters[index + 1].is_ascii_digit()
        {
            continue;
        }

        normalized.push(*character);
    }

    normalized
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

    #[test]
    fn parses_comma_formatted_power_on_hours() {
        let input = "Power On Hours: 1,234\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.power_on_hours, Some(1234));
    }

    #[test]
    fn parses_compound_power_on_hours() {
        let input = "9 Power_On_Hours 0x0032 099 099 000 Old_age Always - 1234h+56m+00.000s\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.power_on_hours, Some(1234));
    }

    #[test]
    fn parses_smartctl_bitmask_stdout() {
        let input = "SMART overall-health self-assessment test result: PASSED\nPower_On_Hours          0x0032   100   100   000    Old_age   Always       -       1234\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.health.as_deref(), Some("PASSED"));
        assert_eq!(health.power_on_hours, Some(1234));
    }

    #[test]
    fn parses_media_wearout_indicator_as_used_percent() {
        let input = "233 Media_Wearout_Indicator 0x0032   091   091   000    Old_age   Always       -       12345\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(9));
    }

    #[test]
    fn parses_lifetime_remaining_as_used_percent() {
        let input = "Percent_Lifetime_Remain: 87%\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(13));
    }

    #[test]
    fn parses_lifetime_used_as_used_percent() {
        let input = "Percent_Lifetime_Used: 2%\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(2));
    }

    #[test]
    fn parses_lifetime_left_as_used_percent() {
        let input = "Percent_Lifetime_Left: 87%\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(13));
    }

    #[test]
    fn parses_wear_leveling_count_from_value_column_not_raw() {
        let input = "177 Wear_Leveling_Count     0x0013   080   080   010    Pre-fail  Always       -       999\n";

        let health = parse_smartctl("/dev/sda", input);

        assert_eq!(health.wearout_percent, Some(20));
    }

    #[test]
    fn missing_smartctl_reports_one_diagnostic_for_many_devices() {
        let devices = vec![
            BlockDevice {
                name: "sda".to_string(),
                device_type: "disk".to_string(),
                ..BlockDevice::default()
            },
            BlockDevice {
                name: "nvme0n1".to_string(),
                device_type: "nvme".to_string(),
                ..BlockDevice::default()
            },
        ];

        let (health, diagnostics) =
            collect_with_availability(&devices, Duration::from_secs(1), false);

        assert!(health.is_empty());
        assert_eq!(diagnostics, ["smartctl not found"]);
    }

    #[test]
    fn smart_candidates_skip_dm_logical_devices() {
        let devices = vec![
            BlockDevice {
                name: "dm-0".to_string(),
                device_type: "dm".to_string(),
                ..BlockDevice::default()
            },
            BlockDevice {
                name: "sda".to_string(),
                device_type: "disk".to_string(),
                ..BlockDevice::default()
            },
            BlockDevice {
                name: "nvme0n1".to_string(),
                device_type: "nvme".to_string(),
                ..BlockDevice::default()
            },
        ];

        let names: Vec<_> = collectable_devices(&devices)
            .into_iter()
            .map(|device| device.name.as_str())
            .collect();

        assert_eq!(names, ["sda", "nvme0n1"]);
    }
}
