//! Turns keyboard, mouse, and paste events into `App` calls.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::app::{App, Command, Pane, Screen};

pub fn key(app: &mut App, key: KeyEvent) {
    app.touch();
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        app.quit = true;
        return;
    }
    match app.screen {
        Screen::Splash(_) => app.screen = Screen::Main,
        Screen::Locked => locked_key(app, key),
        Screen::Main => {
            if let Some(dialog) = app.dialog.as_mut() {
                let outcome = dialog.handle_key(key);
                app.dialog_outcome(outcome);
            } else if app.finding {
                find_key(app, key);
            } else {
                main_key(app, key);
            }
        }
    }
}

fn locked_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.quit = true,
        KeyCode::Enter => app.unlock(),
        KeyCode::Backspace => {
            app.passphrase.pop();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => app.passphrase.push(c),
        _ => {}
    }
}

fn find_key(app: &mut App, key: KeyEvent) {
    let find = app.find.get_or_insert_with(String::new);
    match key.code {
        KeyCode::Esc => {
            app.find = None;
            app.finding = false;
        }
        KeyCode::Enter | KeyCode::Down | KeyCode::Up => app.finding = false,
        KeyCode::Backspace => {
            if find.pop().is_none() {
                app.find = None;
                app.finding = false;
            }
        }
        KeyCode::Char(c) => find.push(c),
        _ => {}
    }
    app.revealed = false;
    app.normalize();
}

fn main_key(app: &mut App, key: KeyEvent) {
    let pane = app.pane;
    match key.code {
        KeyCode::Char('q') => app.run(Command::Quit),
        KeyCode::Esc if app.find.is_some() => {
            app.find = None;
            app.normalize();
        }
        KeyCode::Up | KeyCode::Char('k') => app.move_by(pane, -1),
        KeyCode::Down | KeyCode::Char('j') => app.move_by(pane, 1),
        KeyCode::Home | KeyCode::Char('g') => app.move_by(pane, isize::MIN / 2),
        KeyCode::End | KeyCode::Char('G') => app.move_by(pane, isize::MAX / 2),
        KeyCode::Left | KeyCode::Char('h') => app.focus_by(-1),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => app.focus_by(1),
        KeyCode::BackTab => app.focus_by(-1),
        KeyCode::Enter | KeyCode::Char(' ') => app.run(Command::Reveal),
        KeyCode::Char('a') => app.run(Command::Add),
        KeyCode::Char('e') => app.run(Command::Edit),
        KeyCode::Char('r') => app.run(Command::Rename),
        KeyCode::Char('d') => app.run(Command::Delete),
        KeyCode::Char('c') => app.run(Command::Copy),
        KeyCode::Char('/') => app.run(Command::Find),
        KeyCode::Char('?') => app.run(Command::Help),
        _ => {}
    }
}

pub fn paste(app: &mut App, text: &str) {
    app.touch();
    match app.screen {
        Screen::Locked => app.passphrase.push_str(text.trim_end_matches(['\n', '\r'])),
        Screen::Main => {
            if let Some(dialog) = app.dialog.as_mut() {
                dialog.paste(text);
            }
        }
        Screen::Splash(_) => {}
    }
}

pub fn mouse(app: &mut App, event: MouseEvent) {
    let at = Position::new(event.column, event.row);
    let scroll = match event.kind {
        MouseEventKind::Down(MouseButton::Left) => 0,
        MouseEventKind::ScrollUp => -1,
        MouseEventKind::ScrollDown => 1,
        // Movement and drags don't count as activity for the idle lock.
        _ => return,
    };
    app.touch();
    match app.screen {
        Screen::Splash(_) => app.screen = Screen::Main,
        Screen::Locked if scroll == 0 => app.unlock(),
        Screen::Locked => {}
        Screen::Main if app.dialog.is_some() => {
            if scroll == 0 {
                let outcome = app.dialog.as_mut().map(|d| d.click(at));
                if let Some(outcome) = outcome {
                    app.dialog_outcome(outcome);
                }
            }
        }
        Screen::Main => main_mouse(app, at, scroll),
    }
}

fn main_mouse(app: &mut App, at: Position, scroll: isize) {
    if scroll == 0 {
        let hint = app.hits.footer.iter().find(|(rect, _)| rect.contains(at)).map(|(_, command)| *command);
        if let Some(command) = hint {
            app.finding = false;
            app.run(command);
            return;
        }
    }
    let Some(&(pane, _, inner)) = app.hits.panes.iter().find(|(_, outer, _)| outer.contains(at)) else { return };
    app.finding = false;
    let was_focused = app.pane == pane;
    app.focus(pane);
    if scroll != 0 {
        app.move_by(pane, scroll);
        return;
    }
    if !inner.contains(at) {
        return;
    }
    let index = app.lists[pane.index()].offset() + usize::from(at.y - inner.y);
    // Clicking the already selected secret shows or hides its value.
    if pane == Pane::Secrets && was_focused && app.selected_index(pane) == Some(index) {
        app.run(Command::Reveal);
    } else {
        app.select_index(pane, index);
    }
}
