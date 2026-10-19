//! `hush ui`: a terminal UI for browsing and editing the vault.

mod actions;
mod app;
mod dialog;
mod input;
mod ops;
mod session;
mod theme;
mod view;

#[cfg(test)]
mod tests;

use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;

use crate::paths::Paths;
use crate::project;
use app::App;
use session::Session;

const TICK: Duration = Duration::from_millis(250);
const IDLE_VAR: &str = "HUSH_UI_TIMEOUT";
const DEFAULT_IDLE_MINUTES: u64 = 5;

pub fn run(paths: Paths) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("`hush ui` needs an interactive terminal");
    }
    let idle = idle_timeout()?;
    let linked = std::env::current_dir().ok().and_then(|cwd| project::find(&cwd).ok().flatten()).map(|(name, _)| name);
    // Unlock first, with the normal prompt, before the TUI takes the screen.
    let session = Session::start(paths)?;
    let mut app = App::new(session, linked, idle);

    let mut terminal = ratatui::init();
    // ratatui's hook restores the screen on panic. This one runs first and
    // turns mouse reporting off too, so the shell isn't left full of escapes.
    let restore_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
        restore_hook(info);
    }));
    let result = execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)
        .map_err(anyhow::Error::from)
        .and_then(|()| event_loop(&mut terminal, &mut app));
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|frame| view::draw(frame, app))?;
        // Slow work runs after a frame that already says "Saving…" or similar.
        if let Some(job) = app.pending.take() {
            app.run_job(job);
            continue;
        }
        if app.quit {
            return Ok(());
        }
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => input::key(app, key),
                Event::Mouse(mouse) => input::mouse(app, mouse),
                Event::Paste(text) => input::paste(app, &text),
                _ => {}
            }
        }
        // Ticks run on time even while events keep arriving (mouse movement).
        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }
    }
}

/// Minutes of no input before the TUI locks. `0` turns the idle lock off.
fn idle_timeout() -> Result<Option<Duration>> {
    let minutes = match std::env::var(IDLE_VAR) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("{IDLE_VAR} should be a number of minutes, like 5 (0 turns the idle lock off)"))?,
        Err(_) => DEFAULT_IDLE_MINUTES,
    };
    Ok((minutes > 0).then(|| Duration::from_secs(minutes * 60)))
}
