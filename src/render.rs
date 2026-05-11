use crate::snapshot::Snapshot;
use ratatui::Frame;

pub fn format_text_report(snapshot: &Snapshot) -> String {
    if snapshot.diagnostics.is_empty() {
        String::new()
    } else {
        format!("diagnostics:\n  {}\n", snapshot.diagnostics.join("\n  "))
    }
}

pub fn draw(_frame: &mut Frame<'_>, _snapshot: &Snapshot) {}
