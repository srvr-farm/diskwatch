pub mod block;
pub mod cli;
pub mod commands;
pub mod diskstats;
pub mod filesystems;
pub mod lvm;
pub mod raid;
pub mod render;
pub mod smart;
pub mod snapshot;
pub mod zfs;

use crate::cli::Cli;
use crate::snapshot::{DisplayOptions, Sampler};
use anyhow::Context;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

pub fn run() -> anyhow::Result<()> {
    run_with_cli(Cli::parse())
}

pub fn run_with_cli(cli: Cli) -> anyhow::Result<()> {
    let display_options = DisplayOptions {
        show_loop: cli.show_loop,
        show_tmpfs: cli.show_tmpfs,
        zfs_deep: cli.zfs_deep,
    };
    if cli.once {
        run_once(cli.interval, display_options)
    } else {
        run_tui(cli.interval, display_options)
    }
}

fn run_once(interval: Duration, display_options: DisplayOptions) -> anyhow::Result<()> {
    let mut sampler = Sampler::default().with_display_options(display_options);
    let _ = sampler.sample_at_with_optional(Instant::now(), false);
    thread::sleep(interval);
    let snapshot = sampler.sample_at_with_optional(Instant::now(), true);
    write_once_report(&mut io::stdout(), &snapshot).context("write one-shot report")?;
    Ok(())
}

fn write_once_report<W>(writer: &mut W, snapshot: &crate::snapshot::Snapshot) -> io::Result<()>
where
    W: Write,
{
    match writer.write_all(render::format_text_report(snapshot).as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error),
    }
}

fn run_tui(interval: Duration, display_options: DisplayOptions) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("enable raw mode")?;
    enter_alternate_screen_or_restore_raw_mode(&mut stdout, disable_raw_mode)
        .context("enter alternate screen")?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("initialize terminal")?;
    terminal.clear().context("clear terminal")?;

    let mut sampler = Sampler::default().with_display_options(display_options);
    let mut snapshot = sampler.sample();
    let mut last_tick = Instant::now();

    loop {
        terminal
            .draw(|frame| render::draw(frame, &snapshot))
            .context("draw terminal frame")?;

        let timeout = interval
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        let should_exit = if event::poll(timeout).context("poll terminal events")? {
            match event::read().context("read terminal event")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    should_quit(key.code, key.modifiers)
                }
                _ => false,
            }
        } else {
            false
        };

        if should_exit {
            break;
        }

        if last_tick.elapsed() >= interval {
            snapshot = sampler.sample();
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn enter_alternate_screen_or_restore_raw_mode<W, F>(
    stdout: &mut W,
    mut disable_raw_mode: F,
) -> io::Result<()>
where
    W: io::Write,
    F: FnMut() -> io::Result<()>,
{
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error);
    }
    Ok(())
}

fn should_quit(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Esc)
        || matches!(code, KeyCode::Char('q'))
        || (matches!(code, KeyCode::Char('c')) && modifiers.contains(KeyModifiers::CONTROL))
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::io::{self, Write};

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn should_quit_accepts_q_escape_and_ctrl_c() {
        assert!(should_quit(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(should_quit(KeyCode::Esc, KeyModifiers::NONE));
        assert!(should_quit(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!should_quit(KeyCode::Char('c'), KeyModifiers::NONE));
    }

    #[test]
    fn enter_alternate_screen_failure_restores_raw_mode() {
        let mut disable_raw_mode_called = false;
        let error = enter_alternate_screen_or_restore_raw_mode(&mut FailingWriter, || {
            disable_raw_mode_called = true;
            Ok(())
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(disable_raw_mode_called);
    }

    #[test]
    fn once_report_treats_broken_pipe_as_clean_exit() {
        let mut writer = BrokenPipeWriter;

        let result = write_once_report(&mut writer, &crate::snapshot::Snapshot::default());

        assert!(result.is_ok());
    }
}
