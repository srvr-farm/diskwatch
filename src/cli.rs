use clap::Parser;
use std::time::Duration;

#[derive(Debug, Clone, Parser)]
pub struct Cli {
    #[arg(long, default_value = "1s", value_parser = parse_duration)]
    pub interval: Duration,
    #[arg(long)]
    pub once: bool,
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let duration = humantime::parse_duration(value).map_err(|error| error.to_string())?;
    if duration.is_zero() {
        return Err("interval must be greater than zero".to_string());
    }
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;
    use std::time::Duration;

    #[test]
    fn default_interval_is_one_second() {
        let cli = Cli::try_parse_from(["diskwatch"]).unwrap();
        assert_eq!(cli.interval, Duration::from_secs(1));
    }

    #[test]
    fn parses_human_interval_values() {
        let cli = Cli::try_parse_from(["diskwatch", "--interval", "500ms"]).unwrap();
        assert_eq!(cli.interval, Duration::from_millis(500));
    }

    #[test]
    fn parses_once_mode() {
        let cli = Cli::try_parse_from(["diskwatch", "--once"]).unwrap();
        assert!(cli.once);
    }

    #[test]
    fn rejects_zero_interval() {
        let error = Cli::try_parse_from(["diskwatch", "--interval", "0s"]).unwrap_err();
        assert!(error
            .to_string()
            .contains("interval must be greater than zero"));
    }
}
