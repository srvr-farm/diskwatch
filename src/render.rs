use crate::snapshot::Snapshot;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::fmt::Write;

pub fn format_text_report(snapshot: &Snapshot) -> String {
    let mut report = String::new();

    writeln!(report, "activity:").unwrap();
    write_activity_lines(&mut report, snapshot, "  ");

    writeln!(report, "filesystems:").unwrap();
    write_filesystem_lines(&mut report, snapshot, "  ");

    writeln!(report, "devices:").unwrap();
    write_device_lines(&mut report, snapshot, "  ");

    writeln!(report, "zfs:").unwrap();
    write_zfs_lines(&mut report, snapshot, "  ");

    writeln!(report, "mdraid:").unwrap();
    write_mdraid_lines(&mut report, snapshot, "  ");

    writeln!(report, "lvm:").unwrap();
    write_lvm_lines(&mut report, snapshot, "  ");

    writeln!(report, "smart:").unwrap();
    write_smart_lines(&mut report, snapshot, "  ");

    writeln!(report, "diagnostics:").unwrap();
    write_diagnostic_lines(&mut report, snapshot, "  ");

    report
}

pub fn draw(frame: &mut Frame<'_>, snapshot: &Snapshot) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(if snapshot.diagnostics.is_empty() {
                0
            } else {
                4
            }),
        ])
        .split(frame.area());

    let title = Paragraph::new("diskwatch  q/Esc/Ctrl-C to quit")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, root[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Min(6),
        ])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(columns[1]);

    frame.render_widget(
        Paragraph::new(activity_text(snapshot))
            .block(Block::default().title("Activity").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[0],
    );
    frame.render_widget(
        Paragraph::new(space_text(snapshot))
            .block(Block::default().title("Space").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[1],
    );
    frame.render_widget(
        Paragraph::new(devices_text(snapshot))
            .block(Block::default().title("Devices").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[2],
    );
    frame.render_widget(
        Paragraph::new(stacks_text(snapshot))
            .block(Block::default().title("Stacks").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[0],
    );
    frame.render_widget(
        Paragraph::new(health_text(snapshot))
            .block(Block::default().title("Health").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[1],
    );

    if !snapshot.diagnostics.is_empty() {
        frame.render_widget(
            Paragraph::new(snapshot.diagnostics.join("\n"))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().title("Diagnostics").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            root[2],
        );
    }
}

fn activity_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    write_activity_lines(&mut text, snapshot, "");
    text
}

fn space_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    write_filesystem_lines(&mut text, snapshot, "");
    text
}

fn devices_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    write_device_lines(&mut text, snapshot, "");
    text
}

fn stacks_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();

    writeln!(text, "zfs:").unwrap();
    write_zfs_lines(&mut text, snapshot, "  ");
    writeln!(text, "mdraid:").unwrap();
    write_mdraid_lines(&mut text, snapshot, "  ");
    writeln!(text, "lvm:").unwrap();
    write_lvm_lines(&mut text, snapshot, "  ");

    text
}

fn health_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    write_smart_lines(&mut text, snapshot, "");
    text
}

fn write_activity_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.activity.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for activity in &snapshot.activity {
        writeln!(
            output,
            "{indent}{:<12} read={} write={} riops={} wiops={} busy={}",
            truncate(&activity.name, 12),
            format_rate_bytes(activity.read_bytes_per_sec),
            format_rate_bytes(activity.write_bytes_per_sec),
            format_iops(activity.read_iops),
            format_iops(activity.write_iops),
            format_percent(activity.busy_percent)
        )
        .unwrap();
    }
}

fn write_filesystem_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.filesystems.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for filesystem in &snapshot.filesystems {
        writeln!(
            output,
            "{indent}{} on {} ({}) used={} total={} avail={} use={}",
            truncate(&filesystem.source, 18),
            truncate(&filesystem.mountpoint, 28),
            filesystem.fs_type,
            format_bytes(filesystem.used_bytes),
            format_bytes(filesystem.total_bytes),
            format_bytes(filesystem.available_bytes),
            format_percent(filesystem.used_percent)
        )
        .unwrap();
    }
}

fn write_device_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.devices.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for device in &snapshot.devices {
        let rotational = device
            .rotational
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("N/A");
        writeln!(
            output,
            "{indent}{:<12} type={} size={} rotational={} logical={} physical={} vendor={} model={} serial={}",
            truncate(&device.name, 12),
            device.device_type,
            format_bytes(device.size_bytes),
            rotational,
            device
                .logical_block_size
                .map(format_bytes)
                .unwrap_or_else(|| "N/A".to_string()),
            device
                .physical_block_size
                .map(format_bytes)
                .unwrap_or_else(|| "N/A".to_string()),
            format_optional(device.vendor.as_deref()),
            format_optional(device.model.as_deref()),
            format_optional(device.serial.as_deref())
        )
        .unwrap();
    }
}

fn write_zfs_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.zfs.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for pool in &snapshot.zfs {
        writeln!(
            output,
            "{indent}{} size={} allocated={} free={} health={} status={}",
            pool.name,
            pool.size,
            pool.allocated,
            pool.free,
            pool.health,
            format_optional(pool.status.as_deref())
        )
        .unwrap();
    }
}

fn write_mdraid_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.mdraid.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for array in &snapshot.mdraid {
        writeln!(
            output,
            "{indent}{} level={} blocks={} status={} devices={}",
            array.name,
            format_optional(array.level.as_deref()),
            array
                .blocks
                .map(|blocks| blocks.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            format_optional(array.status.as_deref()),
            if array.devices.is_empty() {
                "N/A".to_string()
            } else {
                array.devices.join(",")
            }
        )
        .unwrap();
    }
}

fn write_lvm_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.lvm.volume_groups.is_empty()
        && snapshot.lvm.physical_volumes.is_empty()
        && snapshot.lvm.logical_volumes.is_empty()
    {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for group in &snapshot.lvm.volume_groups {
        writeln!(
            output,
            "{indent}vg {} size={} free={}",
            group.name, group.size, group.free
        )
        .unwrap();
    }
    for volume in &snapshot.lvm.physical_volumes {
        writeln!(
            output,
            "{indent}pv {} vg={} size={} free={}",
            volume.name, volume.vg_name, volume.size, volume.free
        )
        .unwrap();
    }
    for volume in &snapshot.lvm.logical_volumes {
        writeln!(
            output,
            "{indent}lv {} vg={} size={} attr={}",
            volume.name, volume.vg_name, volume.size, volume.attr
        )
        .unwrap();
    }
}

fn write_smart_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.smart.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for smart in &snapshot.smart {
        writeln!(
            output,
            "{indent}{} health={} temp={} hours={} wear={}",
            smart.device,
            format_optional(smart.health.as_deref()),
            smart
                .temperature_celsius
                .map(|value| format!("{value} C"))
                .unwrap_or_else(|| "N/A".to_string()),
            smart
                .power_on_hours
                .map(|value| value.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            smart
                .wearout_percent
                .map(|value| format!("{value}%"))
                .unwrap_or_else(|| "N/A".to_string())
        )
        .unwrap();
    }
}

fn write_diagnostic_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.diagnostics.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for diagnostic in &snapshot.diagnostics {
        writeln!(output, "{indent}{diagnostic}").unwrap();
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn format_rate_bytes(value: Option<f64>) -> String {
    value
        .map(|bytes_per_sec| format!("{}/s", format_bytes(bytes_per_sec.max(0.0) as u64)))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_iops(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}/s"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_optional(value: Option<&str>) -> &str {
    value.unwrap_or("N/A")
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn text_report_has_stable_section_order_and_empty_values() {
        let snapshot = Snapshot::default();

        let report = format_text_report(&snapshot);

        assert_eq!(
            report,
            "\
activity:
  N/A
filesystems:
  N/A
devices:
  N/A
zfs:
  N/A
mdraid:
  N/A
lvm:
  N/A
smart:
  N/A
diagnostics:
  N/A
"
        );
    }

    #[test]
    fn draw_renders_empty_snapshot_labels_and_values() {
        let output = render_snapshot(80, 24, &Snapshot::default());

        assert!(output.contains("diskwatch  q/Esc/Ctrl-C to quit"));
        assert!(output.contains("Activity"));
        assert!(output.contains("Space"));
        assert!(output.contains("Devices"));
        assert!(output.contains("Stacks"));
        assert!(output.contains("Health"));
        assert!(output.contains("N/A"));
    }

    #[test]
    fn draw_renders_diagnostics_band_when_diagnostics_exist() {
        let snapshot = Snapshot {
            diagnostics: vec!["zpool not found; ZFS pool data unavailable".to_string()],
            ..Snapshot::default()
        };

        let output = render_snapshot(80, 24, &snapshot);

        assert!(output.contains("Diagnostics"));
        assert!(output.contains("zpool not found; ZFS pool data unavailable"));
    }

    #[test]
    fn draw_does_not_panic_on_small_terminal() {
        let _output = render_snapshot(32, 8, &Snapshot::default());
    }

    fn render_snapshot(width: u16, height: u16, snapshot: &Snapshot) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, snapshot)).unwrap();

        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
