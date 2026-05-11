pub mod cli;
pub mod diskstats;
pub mod render;
pub mod snapshot;

use crate::cli::Cli;
use crate::snapshot::Sampler;
use anyhow::Context;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

pub fn run() -> anyhow::Result<()> {
    run_with_cli(Cli::parse())
}

pub fn run_with_cli(cli: Cli) -> anyhow::Result<()> {
    if cli.once {
        run_once(cli.interval)
    } else {
        run_tui(cli.interval)
    }
}

fn run_once(interval: Duration) -> anyhow::Result<()> {
    let mut sampler = Sampler::default();
    let _ = sampler.sample();
    thread::sleep(interval);
    let snapshot = sampler.sample();
    print!("{}", render::format_text_report(&snapshot));
    Ok(())
}

fn run_tui(interval: Duration) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("enable raw mode")?;
    enter_alternate_screen_or_restore_raw_mode(&mut stdout, disable_raw_mode)
        .context("enter alternate screen")?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("initialize terminal")?;
    terminal.clear().context("clear terminal")?;

    let mut sampler = Sampler::default();
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
}
