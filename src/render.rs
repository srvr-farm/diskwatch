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

    for activity in ordered_activities(snapshot) {
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

    for filesystem in ordered_filesystems(snapshot) {
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

    for device in ordered_devices(snapshot) {
        let rotational = device
            .rotational
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("N/A");
        let logical = device
            .logical_block_size
            .map(format_bytes)
            .unwrap_or_else(|| "N/A".to_string());
        let physical = device
            .physical_block_size
            .map(format_bytes)
            .unwrap_or_else(|| "N/A".to_string());
        writeln!(output, "{indent}{}", device.name).unwrap();
        write_device_field(output, indent, "type:", &device.device_type);
        write_device_field(output, indent, "size:", &format_bytes(device.size_bytes));
        write_device_field(output, indent, "rotational:", rotational);
        write_device_field(output, indent, "logical:", &logical);
        write_device_field(output, indent, "physical:", &physical);
        write_device_field(
            output,
            indent,
            "vendor:",
            format_optional(device.vendor.as_deref()),
        );
        write_device_field(
            output,
            indent,
            "model:",
            format_optional(device.model.as_deref()),
        );
        write_device_field(
            output,
            indent,
            "serial:",
            format_optional(device.serial.as_deref()),
        );
    }
}

fn write_device_field(output: &mut String, indent: &str, label: &str, value: &str) {
    writeln!(output, "{indent}  {label:<11} {value}").unwrap();
}

fn ordered_activities(snapshot: &Snapshot) -> Vec<&crate::diskstats::DiskActivity> {
    let mut activity = snapshot.activity.iter().collect::<Vec<_>>();
    activity.sort_by_key(|activity| {
        (
            storage_name_priority(&activity.name),
            activity.name.as_str(),
        )
    });
    activity
}

fn ordered_filesystems(snapshot: &Snapshot) -> Vec<&crate::filesystems::FilesystemUsage> {
    let mut filesystems = snapshot.filesystems.iter().collect::<Vec<_>>();
    filesystems.sort_by_key(|filesystem| {
        (
            filesystem_priority(filesystem),
            filesystem.source.as_str(),
            filesystem.mountpoint.as_str(),
        )
    });
    filesystems
}

fn ordered_devices(snapshot: &Snapshot) -> Vec<&crate::block::BlockDevice> {
    let mut devices = snapshot.devices.iter().collect::<Vec<_>>();
    devices.sort_by_key(|device| (device_priority(device), device.name.as_str()));
    devices
}

fn device_priority(device: &crate::block::BlockDevice) -> u8 {
    match device.device_type.as_str() {
        "disk" | "nvme" | "mmc" | "zbc" => 0,
        "md" | "dm" => 1,
        "loop" | "ram" => 30,
        _ => 10,
    }
}

fn filesystem_priority(filesystem: &crate::filesystems::FilesystemUsage) -> u8 {
    if filesystem.source.starts_with("/dev/loop") || filesystem.fs_type == "squashfs" {
        30
    } else if matches!(filesystem.fs_type.as_str(), "tmpfs" | "overlay") {
        20
    } else {
        0
    }
}

fn storage_name_priority(name: &str) -> u8 {
    if name.starts_with("loop") || name.starts_with("ram") {
        30
    } else {
        0
    }
}

fn write_zfs_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.zfs.deep {
        write_zfs_deep_lines(output, snapshot, indent);
    } else {
        write_zfs_compact_lines(output, snapshot, indent);
    }
}

fn write_zfs_compact_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.zfs.pools.is_empty() && snapshot.zfs.arc.is_none() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for pool in &snapshot.zfs.pools {
        writeln!(output, "{indent}{}", pool.name).unwrap();
        write_zfs_field(output, indent, "health:", &pool.health);
        write_zfs_field(
            output,
            indent,
            "size:",
            &format_optional_bytes(pool.size_bytes),
        );
        write_zfs_field(
            output,
            indent,
            "allocated:",
            &format_optional_bytes(pool.allocated_bytes),
        );
        write_zfs_field(
            output,
            indent,
            "free:",
            &format_optional_bytes(pool.free_bytes),
        );
        write_zfs_field(
            output,
            indent,
            "capacity:",
            &format_percent(pool.capacity_percent),
        );
        write_zfs_field(
            output,
            indent,
            "fragmentation:",
            &format_percent(pool.fragmentation_percent),
        );
        write_zfs_field(output, indent, "dedup:", &format_ratio(pool.dedup_ratio));
        if let Some(status) = pool.status.as_deref() {
            write_zfs_field(output, indent, "status:", status);
        }
        if let Some(action) = pool.action.as_deref() {
            write_zfs_field(output, indent, "action:", action);
        }
        if let Some(scan) = pool.scan.as_deref() {
            write_zfs_field(output, indent, "scan:", scan);
        }
        if let Some(errors) = pool.errors.as_deref() {
            write_zfs_field(output, indent, "errors:", errors);
        }
    }
    if let Some(arc) = snapshot.zfs.arc.as_ref() {
        writeln!(
            output,
            "{indent}arc: hit={} size={} l2_hit={} l2_size={}",
            format_percent(arc.hit_ratio_percent),
            format_optional_bytes(arc.size_bytes),
            format_percent(arc.l2_hit_ratio_percent),
            format_optional_bytes(arc.l2_size_bytes)
        )
        .unwrap();
    }
}

fn write_zfs_deep_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    writeln!(output, "{indent}pools:").unwrap();
    if snapshot.zfs.pools.is_empty() {
        writeln!(output, "{indent}  N/A").unwrap();
    } else {
        for pool in &snapshot.zfs.pools {
            write_zfs_pool_lines(output, pool, indent);
        }
    }

    write_zfs_arc_lines(output, snapshot.zfs.arc.as_ref(), indent);
    write_zfs_dataset_lines(output, &snapshot.zfs.datasets, indent);
    write_zfs_kernel_lines(output, &snapshot.zfs.kernel, indent);
}

fn write_zfs_pool_lines(output: &mut String, pool: &crate::zfs::ZfsPool, indent: &str) {
    writeln!(output, "{indent}  {}", pool.name).unwrap();
    write_zfs_nested_field(output, indent, "health:", &pool.health);
    write_zfs_nested_field(
        output,
        indent,
        "size:",
        &format_optional_bytes(pool.size_bytes),
    );
    write_zfs_nested_field(
        output,
        indent,
        "allocated:",
        &format_optional_bytes(pool.allocated_bytes),
    );
    write_zfs_nested_field(
        output,
        indent,
        "free:",
        &format_optional_bytes(pool.free_bytes),
    );
    write_zfs_nested_field(
        output,
        indent,
        "capacity:",
        &format_percent(pool.capacity_percent),
    );
    write_zfs_nested_field(
        output,
        indent,
        "fragmentation:",
        &format_percent(pool.fragmentation_percent),
    );
    write_zfs_nested_field(output, indent, "dedup:", &format_ratio(pool.dedup_ratio));
    if let Some(status) = pool.status.as_deref() {
        write_zfs_nested_field(output, indent, "status:", status);
    }
    if let Some(action) = pool.action.as_deref() {
        write_zfs_nested_field(output, indent, "action:", action);
    }
    if let Some(scan) = pool.scan.as_deref() {
        write_zfs_nested_field(output, indent, "scan:", scan);
    }
    if let Some(errors) = pool.errors.as_deref() {
        write_zfs_nested_field(output, indent, "errors:", errors);
    }

    if !pool.topology.is_empty() {
        writeln!(output, "{indent}    topology:").unwrap();
        for node in &pool.topology {
            writeln!(
                output,
                "{indent}      {}{} state={} read={} write={} cksum={}",
                "  ".repeat(node.depth),
                node.name,
                format_optional(node.state.as_deref()),
                node.read_errors
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                node.write_errors
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                node.checksum_errors
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            )
            .unwrap();
        }
    }

    if !pool.vdev_io.is_empty() {
        writeln!(output, "{indent}    vdev io:").unwrap();
        for row in &pool.vdev_io {
            writeln!(
                output,
                "{indent}      {} read={} write={} rops={} wops={} total_wait_w={} asyncq_wait_w={} syncq_r={} rebuildq_w={}",
                row.name,
                format_rate_bytes(row.read_bytes_per_sec),
                format_rate_bytes(row.write_bytes_per_sec),
                format_iops(row.read_ops_per_sec),
                format_iops(row.write_ops_per_sec),
                format_duration_ns(row.total_wait_write_ns),
                format_duration_ns(row.async_queue_wait_write_ns),
                format_queue_pair(row.sync_read_queue_pending, row.sync_read_queue_active),
                format_queue_pair(row.rebuild_write_queue_pending, row.rebuild_write_queue_active)
            )
            .unwrap();
        }
    }
}

fn write_zfs_arc_lines(output: &mut String, arc: Option<&crate::zfs::ArcStats>, indent: &str) {
    writeln!(output, "{indent}arc:").unwrap();
    let Some(arc) = arc else {
        writeln!(output, "{indent}  N/A").unwrap();
        return;
    };

    writeln!(
        output,
        "{indent}  hit={} miss={} size={} target={} min={} max={}",
        format_percent(arc.hit_ratio_percent),
        format_percent(arc.miss_ratio_percent),
        format_optional_bytes(arc.size_bytes),
        format_optional_bytes(arc.target_bytes),
        format_optional_bytes(arc.min_bytes),
        format_optional_bytes(arc.max_bytes)
    )
    .unwrap();
    writeln!(
        output,
        "{indent}  data={} metadata={} dbuf={} dnode={} mru={} mfu={}",
        format_optional_bytes(arc.data_size_bytes),
        format_optional_bytes(arc.metadata_size_bytes),
        format_optional_bytes(arc.dbuf_size_bytes),
        format_optional_bytes(arc.dnode_size_bytes),
        format_optional_bytes(arc.mru_size_bytes),
        format_optional_bytes(arc.mfu_size_bytes)
    )
    .unwrap();
    writeln!(
        output,
        "{indent}  l2 hit={} size={} asize={} read={} write={} writes={}/{} errors={} cksum_bad={} io_error={}",
        format_percent(arc.l2_hit_ratio_percent),
        format_optional_bytes(arc.l2_size_bytes),
        format_optional_bytes(arc.l2_asize_bytes),
        format_optional_bytes(arc.l2_read_bytes),
        format_optional_bytes(arc.l2_write_bytes),
        format_optional_u64(arc.l2_writes_done),
        format_optional_u64(arc.l2_writes_sent),
        format_optional_u64(arc.l2_writes_error),
        format_optional_u64(arc.l2_cksum_bad),
        format_optional_u64(arc.l2_io_error)
    )
    .unwrap();
}

fn write_zfs_dataset_lines(output: &mut String, datasets: &[crate::zfs::ZfsDataset], indent: &str) {
    writeln!(output, "{indent}datasets:").unwrap();
    if datasets.is_empty() {
        writeln!(output, "{indent}  N/A").unwrap();
        return;
    }

    for dataset in datasets {
        writeln!(
            output,
            "{indent}  {} used={} avail={} ref={} mount={} compress={} ratio={}",
            dataset.name,
            format_optional_bytes(dataset.used_bytes),
            format_optional_bytes(dataset.available_bytes),
            format_optional_bytes(dataset.referenced_bytes),
            format_optional(dataset.mountpoint.as_deref()),
            format_optional(dataset.compression.as_deref()),
            format_ratio(dataset.compressratio)
        )
        .unwrap();

        let mut properties = dataset.properties.iter().collect::<Vec<_>>();
        properties.sort_by_key(|(name, _)| name.as_str());
        for (name, property) in properties {
            writeln!(
                output,
                "{indent}    {name}: {}",
                format_zfs_property_value(&property.value)
            )
            .unwrap();
        }
    }
}

fn write_zfs_kernel_lines(output: &mut String, kernel: &crate::zfs::ZfsKernelStats, indent: &str) {
    writeln!(output, "{indent}kernel:").unwrap();
    if kernel.dbuf.is_none()
        && kernel.dnode.is_none()
        && kernel.zil.is_none()
        && kernel.zfetch.is_none()
        && kernel.abd.is_none()
        && kernel.txg.is_none()
    {
        writeln!(output, "{indent}  N/A").unwrap();
        return;
    }

    if let Some(dbuf) = kernel.dbuf.as_ref() {
        writeln!(
            output,
            "{indent}  dbuf cache={} target={} hash_hits={} hash_misses={} evicts={}",
            format_optional_bytes(dbuf.cache_size_bytes),
            format_optional_bytes(dbuf.cache_target_bytes),
            format_optional_u64(dbuf.hash_hits),
            format_optional_u64(dbuf.hash_misses),
            format_optional_u64(dbuf.cache_total_evicts)
        )
        .unwrap();
    }
    if let Some(dnode) = kernel.dnode.as_ref() {
        writeln!(
            output,
            "{indent}  dnode hold_hits={} hold_misses={} allocate={} buf_evict={}",
            format_optional_u64(dnode.hold_alloc_hits),
            format_optional_u64(dnode.hold_alloc_misses),
            format_optional_u64(dnode.allocate),
            format_optional_u64(dnode.buf_evict)
        )
        .unwrap();
    }
    if let Some(zil) = kernel.zil.as_ref() {
        writeln!(
            output,
            "{indent}  zil commits={} itx={} normal_bytes={}",
            format_optional_u64(zil.commit_count),
            format_optional_u64(zil.itx_count),
            format_optional_bytes(zil.itx_metaslab_normal_bytes)
        )
        .unwrap();
    }
    if let Some(zfetch) = kernel.zfetch.as_ref() {
        writeln!(
            output,
            "{indent}  zfetch hits={} misses={} io_issued={} io_active={}",
            format_optional_u64(zfetch.hits),
            format_optional_u64(zfetch.misses),
            format_optional_u64(zfetch.io_issued),
            format_optional_u64(zfetch.io_active)
        )
        .unwrap();
    }
    if let Some(abd) = kernel.abd.as_ref() {
        writeln!(
            output,
            "{indent}  abd linear={} linear_bytes={} scatter={} scatter_bytes={} retry={}",
            format_optional_u64(abd.linear_count),
            format_optional_bytes(abd.linear_data_size_bytes),
            format_optional_u64(abd.scatter_count),
            format_optional_bytes(abd.scatter_data_size_bytes),
            format_optional_u64(abd.scatter_page_alloc_retry)
        )
        .unwrap();
    }
    if let Some(txg) = kernel.txg.as_ref() {
        writeln!(
            output,
            "{indent}  txg latest={} dirty={} written={} writes={}",
            format_optional_u64(txg.latest_txg),
            format_optional_bytes(txg.latest_dirty_bytes),
            format_optional_bytes(txg.latest_written_bytes),
            format_optional_u64(txg.latest_writes)
        )
        .unwrap();
    }
}

fn write_zfs_field(output: &mut String, indent: &str, label: &str, value: &str) {
    writeln!(output, "{indent}  {label:<14} {value}").unwrap();
}

fn write_zfs_nested_field(output: &mut String, indent: &str, label: &str, value: &str) {
    writeln!(output, "{indent}    {label:<14} {value}").unwrap();
}

fn write_mdraid_lines(output: &mut String, snapshot: &Snapshot, indent: &str) {
    if snapshot.mdraid.is_empty() {
        writeln!(output, "{indent}N/A").unwrap();
        return;
    }

    for array in &snapshot.mdraid {
        writeln!(
            output,
            "{indent}{} level={} blocks={} status={} devices={} detail={}",
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
            },
            format_optional(array.detail.as_deref())
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
    const TIB: f64 = GIB * 1024.0;
    const PIB: f64 = TIB * 1024.0;
    const EIB: f64 = PIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= EIB {
        format!("{:.1} EiB", bytes / EIB)
    } else if bytes >= PIB {
        format!("{:.1} PiB", bytes / PIB)
    } else if bytes >= TIB {
        format!("{:.1} TiB", bytes / TIB)
    } else if bytes >= GIB {
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

fn format_ratio(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}x"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_optional_bytes(value: Option<u64>) -> String {
    value.map(format_bytes).unwrap_or_else(|| "N/A".to_string())
}

fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_duration_ns(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "N/A".to_string();
    };
    if value >= 1_000_000 {
        format!("{:.1} ms", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1} us", value as f64 / 1_000.0)
    } else {
        format!("{value} ns")
    }
}

fn format_queue_pair(pending: Option<u64>, active: Option<u64>) -> String {
    format!(
        "{}/{}",
        format_optional_u64(pending),
        format_optional_u64(active)
    )
}

fn format_zfs_property_value(value: &str) -> String {
    value
        .parse::<u64>()
        .map(format_bytes)
        .unwrap_or_else(|_| value.to_string())
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
    use crate::block::BlockDevice;
    use crate::diskstats::DiskActivity;
    use crate::filesystems::FilesystemUsage;
    use crate::raid::MdArray;
    use crate::zfs::{
        AbdStats, ArcStats, DbufStats, DnodeStats, TxgSummary, ZfetchStats, ZfsDataset,
        ZfsKernelStats, ZfsPool, ZfsProperty, ZfsSnapshot, ZfsTopologyNode, ZfsTopologyRole,
        ZfsVdevIo,
    };
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::collections::HashMap;

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
    fn text_report_includes_mdadm_detail_when_present() {
        let snapshot = Snapshot {
            mdraid: vec![MdArray {
                name: "md0".to_string(),
                level: Some("raid1".to_string()),
                devices: vec!["sda1[0]".to_string(), "sdb1[1]".to_string()],
                status: Some("[UU]".to_string()),
                blocks: Some(1046528),
                detail: Some("ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0".to_string()),
            }],
            ..Snapshot::default()
        };

        let report = format_text_report(&snapshot);

        assert!(report.contains("detail=ARRAY /dev/md0 metadata=1.2 UUID=abc name=host:0"));
    }

    #[test]
    fn text_report_formats_device_fields_on_separate_lines() {
        let snapshot = Snapshot {
            devices: vec![BlockDevice {
                name: "nvme0n1".to_string(),
                device_type: "nvme".to_string(),
                size_bytes: 1_000_000_000_000,
                rotational: Some(false),
                logical_block_size: Some(512),
                physical_block_size: Some(4096),
                vendor: Some("ACME".to_string()),
                model: Some("FastDisk".to_string()),
                serial: Some("XYZ123".to_string()),
                ..BlockDevice::default()
            }],
            ..Snapshot::default()
        };

        let report = format_text_report(&snapshot);

        assert!(report.contains(
            "\
devices:
  nvme0n1
    type:       nvme
    size:       931.3 GiB
    rotational: no
    logical:    512 B
    physical:   4.0 KiB
    vendor:     ACME
    model:      FastDisk
    serial:     XYZ123
"
        ));
    }

    #[test]
    fn draw_does_not_panic_on_small_terminal() {
        let _output = render_snapshot(32, 8, &Snapshot::default());
    }

    #[test]
    fn formats_large_storage_units() {
        assert_eq!(format_bytes(2 * 1024_u64.pow(4)), "2.0 TiB");
        assert_eq!(format_bytes(3 * 1024_u64.pow(5)), "3.0 PiB");
    }

    #[test]
    fn text_report_prioritizes_real_storage_over_loop_and_snap_noise() {
        let snapshot = Snapshot {
            activity: vec![
                DiskActivity {
                    name: "loop0".to_string(),
                    ..DiskActivity::default()
                },
                DiskActivity {
                    name: "sda".to_string(),
                    ..DiskActivity::default()
                },
            ],
            filesystems: vec![
                FilesystemUsage {
                    source: "/dev/loop0".to_string(),
                    mountpoint: "/snap/tool".to_string(),
                    fs_type: "squashfs".to_string(),
                    ..FilesystemUsage::default()
                },
                FilesystemUsage {
                    source: "/dev/sda1".to_string(),
                    mountpoint: "/".to_string(),
                    fs_type: "ext4".to_string(),
                    ..FilesystemUsage::default()
                },
            ],
            devices: vec![
                BlockDevice {
                    name: "loop0".to_string(),
                    device_type: "loop".to_string(),
                    ..BlockDevice::default()
                },
                BlockDevice {
                    name: "sda".to_string(),
                    device_type: "disk".to_string(),
                    ..BlockDevice::default()
                },
            ],
            ..Snapshot::default()
        };

        let report = format_text_report(&snapshot);

        assert!(report.find("sda").unwrap() < report.find("loop0").unwrap());
        assert!(report.find("/dev/sda1").unwrap() < report.find("/dev/loop0").unwrap());
    }

    #[test]
    fn text_report_renders_deep_zfs_sections() {
        let snapshot = full_zfs_snapshot_fixture();
        let report = format_text_report(&snapshot);

        assert!(report.contains("pools:"));
        assert!(report.contains("data"));
        assert!(report.contains("health:"));
        assert!(report.contains("status:"));
        assert!(report.contains("action:"));
        assert!(report.contains("raidz2-0"));
        assert!(report.contains("vdev io:"));
        assert!(report.contains("write="));
        assert!(report.contains("total_wait_w="));
        assert!(report.contains("syncq_r="));
        assert!(report.contains("arc:"));
        assert!(report.contains("hit="));
        assert!(report.contains("l2"));
        assert!(report.contains("datasets:"));
        assert!(report.contains("recordsize:"));
        assert!(report.contains("kernel:"));
        assert!(report.contains("dbuf"));
        assert!(report.contains("dnode"));
        assert!(report.contains("abd"));
    }

    #[test]
    fn text_report_renders_deep_zfs_partial_data_with_diagnostics() {
        let snapshot = Snapshot {
            zfs: ZfsSnapshot {
                deep: true,
                pools: vec![ZfsPool {
                    name: "data".to_string(),
                    health: "ONLINE".to_string(),
                    ..ZfsPool::default()
                }],
                ..ZfsSnapshot::default()
            },
            diagnostics: vec!["zfs kstat arcstats unreadable: permission denied".to_string()],
            ..Snapshot::default()
        };
        let report = format_text_report(&snapshot);

        assert!(report.contains("arc:"));
        assert!(report.contains("N/A"));
        assert!(report.contains("zfs kstat arcstats unreadable: permission denied"));
    }

    fn full_zfs_snapshot_fixture() -> Snapshot {
        let mut properties = HashMap::new();
        properties.insert(
            "recordsize".to_string(),
            ZfsProperty {
                value: "131072".to_string(),
                source: Some("default".to_string()),
            },
        );
        properties.insert(
            "primarycache".to_string(),
            ZfsProperty {
                value: "all".to_string(),
                source: Some("default".to_string()),
            },
        );

        Snapshot {
            zfs: ZfsSnapshot {
                deep: true,
                pools: vec![ZfsPool {
                    name: "data".to_string(),
                    health: "ONLINE".to_string(),
                    size_bytes: Some(29_961_691_856_896),
                    allocated_bytes: Some(10_665_749_323_776),
                    free_bytes: Some(19_295_942_533_120),
                    capacity_percent: Some(35.0),
                    fragmentation_percent: Some(1.0),
                    dedup_ratio: Some(1.0),
                    status: Some(
                        "One or more devices has experienced an unrecoverable error.".to_string(),
                    ),
                    action: Some("Replace the faulted device, or use 'zpool clear'.".to_string()),
                    scan: Some("resilvered 97.1M in 00:23:33 with 0 errors".to_string()),
                    errors: Some("No known data errors".to_string()),
                    topology: vec![
                        ZfsTopologyNode {
                            name: "raidz2-0".to_string(),
                            role: ZfsTopologyRole::Vdev,
                            depth: 1,
                            state: Some("ONLINE".to_string()),
                            ..ZfsTopologyNode::default()
                        },
                        ZfsTopologyNode {
                            name: "/dev/sdb".to_string(),
                            role: ZfsTopologyRole::Disk,
                            depth: 2,
                            state: Some("ONLINE".to_string()),
                            ..ZfsTopologyNode::default()
                        },
                    ],
                    vdev_io: vec![ZfsVdevIo {
                        name: "data".to_string(),
                        write_ops_per_sec: Some(382.0),
                        read_bytes_per_sec: Some(0.0),
                        write_bytes_per_sec: Some(4_010_886.0),
                        total_wait_write_ns: Some(3_094_394),
                        async_queue_wait_write_ns: Some(2_376_632),
                        sync_read_queue_pending: Some(0),
                        sync_read_queue_active: Some(0),
                        rebuild_write_queue_pending: Some(0),
                        rebuild_write_queue_active: Some(0),
                        ..ZfsVdevIo::default()
                    }],
                    ..ZfsPool::default()
                }],
                arc: Some(ArcStats {
                    hit_ratio_percent: Some(90.0),
                    size_bytes: Some(1536),
                    l2_hit_ratio_percent: Some(25.0),
                    l2_size_bytes: Some(8192),
                    ..ArcStats::default()
                }),
                datasets: vec![ZfsDataset {
                    name: "data".to_string(),
                    used_bytes: Some(6_311_953_548_792),
                    available_bytes: Some(11_187_890_698_760),
                    referenced_bytes: Some(6_311_795_099_184),
                    mountpoint: Some("/data".to_string()),
                    compression: Some("on".to_string()),
                    compressratio: Some(1.08),
                    properties,
                    ..ZfsDataset::default()
                }],
                kernel: ZfsKernelStats {
                    dbuf: Some(DbufStats {
                        cache_size_bytes: Some(278_614_528),
                        hash_hits: Some(46_158_602),
                        ..DbufStats::default()
                    }),
                    dnode: Some(DnodeStats {
                        allocate: Some(222_738),
                        buf_evict: Some(37_777),
                        ..DnodeStats::default()
                    }),
                    zfetch: Some(ZfetchStats {
                        io_issued: Some(16_434),
                        ..ZfetchStats::default()
                    }),
                    abd: Some(AbdStats {
                        scatter_data_size_bytes: Some(8_236_362_240),
                        ..AbdStats::default()
                    }),
                    txg: Some(TxgSummary {
                        latest_txg: Some(7_628_332),
                        latest_written_bytes: Some(3_854_336),
                        ..TxgSummary::default()
                    }),
                    ..ZfsKernelStats::default()
                },
            },
            ..Snapshot::default()
        }
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
