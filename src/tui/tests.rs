//! Drives the TUI through its real input handlers and checks what's drawn.
//! Each test works on a real encrypted vault in a temp folder.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tempfile::TempDir;

use super::app::{App, Pane, Screen};
use super::session::Session;
use super::{input, ops, view};
use crate::vault::Vault;

const VALUES: [&str; 4] = ["postgres://secret-db", "sk_live_secret", "postgres://prod-db", "blog-token"];

fn app_with_idle(idle: Option<Duration>) -> (App, TempDir) {
    let dir = TempDir::new().unwrap();
    let mut vault = Vault::new();
    ops::set_secret(&mut vault, "api", "dev", "DATABASE_URL", VALUES[0].into()).unwrap();
    ops::set_secret(&mut vault, "api", "dev", "STRIPE_KEY", VALUES[1].into()).unwrap();
    ops::set_secret(&mut vault, "api", "prod", "DATABASE_URL", VALUES[2].into()).unwrap();
    ops::set_secret(&mut vault, "blog", "dev", "TOKEN", VALUES[3].into()).unwrap();
    let (session, _) = Session::for_test(dir.path(), &vault);
    let mut app = App::new(session, Some("api".into()), idle);
    app.screen = Screen::Main;
    (app, dir)
}

fn app() -> (App, TempDir) {
    app_with_idle(None)
}

fn screen(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 20)).unwrap();
    terminal.draw(|frame| view::draw(frame, app)).unwrap();
    let cells = terminal.backend().buffer().content();
    let rows = cells.chunks(width as usize).map(|row| row.iter().map(|c| c.symbol()).collect::<String>());
    rows.collect::<Vec<_>>().join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    input::key(app, KeyEvent::new(code, KeyModifiers::NONE));
}

fn typed(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, KeyCode::Char(c)));
}

fn click(app: &mut App, column: u16, row: u16) {
    let kind = MouseEventKind::Down(MouseButton::Left);
    input::mouse(app, MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
}

fn run_pending(app: &mut App) {
    let job = app.pending.take().expect("a job should be pending");
    app.run_job(job);
}

fn secrets(app: &App) -> Vec<String> {
    app.secrets()
}

#[test]
fn values_are_masked_by_default() {
    let (mut app, _dir) = app();
    let shown = screen(&mut app, 100);
    assert!(shown.contains("DATABASE_URL") && shown.contains("STRIPE_KEY"), "{shown}");
    for value in VALUES {
        assert!(!shown.contains(value), "{value} leaked:\n{shown}");
    }
}

#[test]
fn reveal_shows_only_the_selected_value_until_the_selection_moves() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app, 100);
    assert!(shown.contains(VALUES[0]), "{shown}");
    assert!(!shown.contains(VALUES[1]));

    press(&mut app, KeyCode::Down);
    let shown = screen(&mut app, 100);
    for value in VALUES {
        assert!(!shown.contains(value), "{value} still shown:\n{shown}");
    }
}

#[test]
fn narrow_terminals_show_one_pane_at_a_time() {
    let (mut app, _dir) = app();
    let shown = screen(&mut app, 60);
    assert!(shown.contains("Secrets") && !shown.contains("Projects"), "{shown}");
    assert!(shown.contains("? help") && shown.contains("q quit"), "{shown}");
    press(&mut app, KeyCode::Left);
    let shown = screen(&mut app, 60);
    assert!(shown.contains("Envs") && !shown.contains("Secrets"), "{shown}");
}

#[test]
fn the_lock_screen_shows_no_names_or_values() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Enter);
    app.lock();
    let shown = screen(&mut app, 100);
    assert!(shown.contains("locked"), "{shown}");
    assert!(!shown.contains("DATABASE_URL") && !shown.contains("api"));
    for value in VALUES {
        assert!(!shown.contains(value));
    }
    assert!(app.session.vault().is_none());
}

#[test]
fn idle_time_locks_the_session() {
    let (mut app, _dir) = app_with_idle(Some(Duration::ZERO));
    app.tick();
    assert!(matches!(app.screen, Screen::Locked));
    assert!(app.session.vault().is_none());
}

#[test]
fn splash_shows_the_logo_and_version() {
    let (mut app, _dir) = app();
    app.screen = Screen::Splash(Instant::now());
    let shown = screen(&mut app, 100);
    assert!(shown.contains("█▀▀▄ █  █ ▄▀▀▀ █▀▀▄"), "{shown}");
    assert!(shown.contains(env!("CARGO_PKG_VERSION")));
    press(&mut app, KeyCode::Char('x'));
    assert!(matches!(app.screen, Screen::Main));
}

#[test]
fn adding_a_secret_masks_the_value_and_saves_it() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('a'));
    typed(&mut app, "NEW_KEY");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "hunter2-value");
    let shown = screen(&mut app, 100);
    assert!(!shown.contains("hunter2-value") && shown.contains("●●●●●●●●●●●●●"), "{shown}");
    assert!(shown.contains("[ Save ]") && shown.contains("[ Cancel ]"), "{shown}");

    press(&mut app, KeyCode::Enter);
    run_pending(&mut app);
    assert!(app.dialog.is_none());
    assert_eq!(app.sel.secret.as_deref(), Some("NEW_KEY"));
    assert_eq!(app.session.vault().unwrap().secrets("api", "dev").unwrap()["NEW_KEY"], "hunter2-value");
    assert!(!app.session.changed_on_disk());
}

#[test]
fn a_bad_name_keeps_the_dialog_open_with_an_error() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('a'));
    typed(&mut app, "bad-name");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "v");
    press(&mut app, KeyCode::Enter);
    run_pending(&mut app);
    let error = app.dialog.as_ref().and_then(|d| d.error.clone()).expect("dialog with an error");
    assert!(error.contains("bad-name"), "{error}");
}

#[test]
fn copying_an_env_selects_the_copy() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Left);
    assert_eq!(app.pane, Pane::Envs);
    press(&mut app, KeyCode::Char('c'));
    typed(&mut app, "staging");
    press(&mut app, KeyCode::Enter);
    run_pending(&mut app);
    assert_eq!(app.sel.env.as_deref(), Some("staging"));
    assert_eq!(secrets(&app), ["DATABASE_URL", "STRIPE_KEY"]);
}

#[test]
fn deleting_a_project_needs_its_name_typed() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Char('d'));
    typed(&mut app, "nope");
    press(&mut app, KeyCode::Enter);
    assert!(app.pending.is_none());
    assert!(app.dialog.as_ref().and_then(|d| d.error.as_deref()).is_some_and(|e| e.contains("type api")));

    (0..4).for_each(|_| press(&mut app, KeyCode::Backspace));
    typed(&mut app, "api");
    press(&mut app, KeyCode::Enter);
    run_pending(&mut app);
    assert_eq!(app.projects(), ["blog"]);
    assert_eq!(app.sel.project.as_deref(), Some("blog"));
}

#[test]
fn deleting_a_secret_asks_first_and_selects_the_next_one() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('y'));
    run_pending(&mut app);
    assert_eq!(secrets(&app), ["STRIPE_KEY"]);
    assert_eq!(app.sel.secret.as_deref(), Some("STRIPE_KEY"));
}

#[test]
fn find_filters_the_secrets() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('/'));
    typed(&mut app, "stri");
    assert_eq!(secrets(&app), ["STRIPE_KEY"]);
    press(&mut app, KeyCode::Esc);
    assert_eq!(secrets(&app).len(), 2);
}

#[test]
fn clicking_selects_and_clicking_again_reveals() {
    let (mut app, _dir) = app();
    screen(&mut app, 100);
    let (_, _, inner) = *app.hits.panes.iter().find(|(pane, _, _)| *pane == Pane::Secrets).unwrap();
    click(&mut app, inner.x + 2, inner.y + 1);
    assert_eq!(app.sel.secret.as_deref(), Some("STRIPE_KEY"));
    assert!(!screen(&mut app, 100).contains(VALUES[1]));
    click(&mut app, inner.x + 2, inner.y + 1);
    assert!(screen(&mut app, 100).contains(VALUES[1]));
}

#[test]
fn help_fits_its_dialog() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('?'));
    let shown = screen(&mut app, 100);
    assert!(shown.contains("this help") && shown.contains("Mouse:") && shown.contains("[ Close ]"), "{shown}");
}

#[test]
fn clicking_a_footer_hint_runs_it() {
    let (mut app, _dir) = app();
    screen(&mut app, 100);
    let (rect, _) = app.hits.footer[1];
    click(&mut app, rect.x, rect.y);
    assert!(app.dialog.is_some(), "the second secrets hint is `a add`");
}
