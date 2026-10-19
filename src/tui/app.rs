//! TUI state and everything that changes it. Input handling (`input.rs`)
//! turns events into calls here, and `view.rs` draws whatever this holds.

use std::time::{Duration, Instant};

use age::secrecy::SecretString;
use anyhow::Result;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use zeroize::Zeroizing;

use super::dialog::Dialog;
use super::session::Session;
use crate::vault::Vault;

/// How long the splash screen stays up unless a key skips it.
const SPLASH: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Projects,
    Envs,
    Secrets,
}

impl Pane {
    pub const ALL: [Pane; 3] = [Pane::Projects, Pane::Envs, Pane::Secrets];

    pub fn index(self) -> usize {
        self as usize
    }
}

pub enum Screen {
    Splash(Instant),
    Main,
    Locked,
}

pub enum Tone {
    Ok,
    Error,
    Busy,
}

pub struct Status {
    pub text: String,
    pub tone: Tone,
}

/// A change to apply to the vault inside `Session::edit`.
pub type Change = Box<dyn FnOnce(&mut Vault) -> Result<()>>;

/// Things that hit the disk (or scrypt), run right after a frame that says so.
pub enum Job {
    Edit { change: Change, done: String, then: Selection },
    Unlock(Option<SecretString>),
    Reload,
}

/// What's selected in each pane, by name, so it survives reloads.
#[derive(Default, Clone)]
pub struct Selection {
    pub project: Option<String>,
    pub env: Option<String>,
    pub secret: Option<String>,
}

#[derive(Clone, Copy)]
pub enum Command {
    Add,
    Edit,
    Rename,
    Delete,
    Copy,
    Find,
    Help,
    Reveal,
    Quit,
}

/// Where panes and footer hints were drawn last frame, for mouse clicks.
#[derive(Default)]
pub struct Hits {
    pub panes: Vec<(Pane, Rect, Rect)>,
    pub footer: Vec<(Rect, Command)>,
}

pub struct App {
    pub session: Session,
    pub screen: Screen,
    pub pane: Pane,
    pub sel: Selection,
    pub revealed: bool,
    pub dialog: Option<Dialog>,
    pub find: Option<String>,
    pub finding: bool,
    pub status: Option<Status>,
    pub pending: Option<Job>,
    pub passphrase: Zeroizing<String>,
    pub lists: [ListState; 3],
    pub hits: Hits,
    pub quit: bool,
    pub(super) linked: Option<String>,
    idle: Option<Duration>,
    last_input: Instant,
}

impl App {
    pub fn new(session: Session, linked: Option<String>, idle: Option<Duration>) -> Self {
        let sel = Selection { project: linked.clone(), env: Some("dev".into()), secret: None };
        let mut app = Self {
            session,
            screen: Screen::Splash(Instant::now()),
            pane: Pane::Secrets,
            sel,
            revealed: false,
            dialog: None,
            find: None,
            finding: false,
            status: None,
            pending: None,
            passphrase: Zeroizing::new(String::new()),
            lists: Default::default(),
            hits: Hits::default(),
            quit: false,
            linked,
            idle,
            last_input: Instant::now(),
        };
        app.normalize();
        app
    }

    pub fn projects(&self) -> Vec<String> {
        self.session.vault().map(|v| v.projects.keys().cloned().collect()).unwrap_or_default()
    }

    pub fn envs(&self) -> Vec<String> {
        let vault = self.session.vault();
        let envs = self.sel.project.as_ref().and_then(|p| vault?.projects.get(p));
        envs.map(|e| e.keys().cloned().collect()).unwrap_or_default()
    }

    /// Secret names in the selected environment, narrowed by the find filter.
    pub fn secrets(&self) -> Vec<String> {
        let filter = self.find.as_deref().unwrap_or("").to_lowercase();
        let (Some(project), Some(env)) = (&self.sel.project, &self.sel.env) else { return Vec::new() };
        let secrets = self.session.vault().and_then(|v| v.secrets(project, env));
        let keys = secrets.into_iter().flat_map(|s| s.keys());
        keys.filter(|k| k.to_lowercase().contains(&filter)).cloned().collect()
    }

    pub fn names(&self, pane: Pane) -> Vec<String> {
        match pane {
            Pane::Projects => self.projects(),
            Pane::Envs => self.envs(),
            Pane::Secrets => self.secrets(),
        }
    }

    pub fn selected(&self, pane: Pane) -> Option<&String> {
        match pane {
            Pane::Projects => self.sel.project.as_ref(),
            Pane::Envs => self.sel.env.as_ref(),
            Pane::Secrets => self.sel.secret.as_ref(),
        }
    }

    pub fn selected_index(&self, pane: Pane) -> Option<usize> {
        let name = self.selected(pane)?;
        self.names(pane).iter().position(|n| n == name)
    }

    /// The selected secret's value, only while it's revealed.
    pub fn revealed_value(&self) -> Option<&str> {
        if !self.revealed {
            return None;
        }
        let (project, env, key) = (self.sel.project.as_ref()?, self.sel.env.as_ref()?, self.sel.secret.as_ref()?);
        self.session.vault()?.secrets(project, env)?.get(key).map(String::as_str)
    }

    /// Points each selection at something that exists. Environments fall back
    /// to `dev` when there is one.
    pub fn normalize(&mut self) {
        let projects = self.projects();
        if !self.sel.project.as_ref().is_some_and(|p| projects.contains(p)) {
            self.sel.project = projects.first().cloned();
        }
        let envs = self.envs();
        if !self.sel.env.as_ref().is_some_and(|e| envs.contains(e)) {
            let dev = envs.iter().find(|e| *e == "dev");
            self.sel.env = dev.or(envs.first()).cloned();
        }
        let secrets = self.secrets();
        if !self.sel.secret.as_ref().is_some_and(|s| secrets.contains(s)) {
            self.sel.secret = secrets.first().cloned();
        }
    }

    fn set_selected(&mut self, pane: Pane, name: Option<String>) {
        match pane {
            Pane::Projects => self.sel = Selection { project: name, ..Selection::default() },
            Pane::Envs => {
                self.sel.env = name;
                self.sel.secret = None;
            }
            Pane::Secrets => self.sel.secret = name,
        }
        self.revealed = false;
        self.normalize();
    }

    pub fn move_by(&mut self, pane: Pane, delta: isize) {
        let names = self.names(pane);
        if names.is_empty() {
            return;
        }
        let current = self.selected_index(pane).unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, names.len() as isize - 1) as usize;
        self.set_selected(pane, Some(names[next].clone()));
    }

    pub fn select_index(&mut self, pane: Pane, index: usize) {
        if let Some(name) = self.names(pane).get(index).cloned() {
            self.set_selected(pane, Some(name));
        }
    }

    pub fn focus(&mut self, pane: Pane) {
        if self.pane != pane {
            self.pane = pane;
            self.revealed = false;
        }
    }

    pub fn focus_by(&mut self, delta: isize) {
        let next = (self.pane.index() as isize + delta).rem_euclid(3) as usize;
        self.focus(Pane::ALL[next]);
    }

    /// Any key, click, or scroll. Resets the idle timer.
    pub fn touch(&mut self) {
        self.last_input = Instant::now();
    }

    pub fn tick(&mut self) {
        match self.screen {
            Screen::Splash(started) if started.elapsed() >= SPLASH => self.screen = Screen::Main,
            Screen::Main if self.idle.is_some_and(|idle| self.last_input.elapsed() >= idle) => self.lock(),
            Screen::Main if self.pending.is_none() && self.session.changed_on_disk() => {
                self.busy("Reloading…");
                self.pending = Some(Job::Reload);
            }
            _ => {}
        }
    }

    pub fn lock(&mut self) {
        self.session.lock();
        self.dialog = None;
        self.revealed = false;
        self.status = None;
        self.passphrase.clear();
        self.screen = Screen::Locked;
    }

    pub fn busy(&mut self, text: &str) {
        self.status = Some(Status { text: text.into(), tone: Tone::Busy });
    }

    fn report(&mut self, result: Result<()>, done: String) {
        self.status = Some(match result {
            Ok(()) => Status { text: done, tone: Tone::Ok },
            Err(e) => Status { text: format!("{e:#}"), tone: Tone::Error },
        });
    }

    pub fn run_job(&mut self, job: Job) {
        match job {
            Job::Edit { change, done, then } => match self.session.edit(change) {
                Ok(()) => {
                    self.dialog = None;
                    self.sel = then;
                    self.revealed = false;
                    self.normalize();
                    self.report(Ok(()), done);
                }
                Err(e) => match self.dialog.as_mut() {
                    Some(dialog) => {
                        dialog.error = Some(format!("{e:#}"));
                        self.status = None;
                    }
                    None => self.report(Err(e), String::new()),
                },
            },
            Job::Unlock(passphrase) => {
                let result = self.session.unlock(passphrase);
                self.passphrase.clear();
                match result {
                    Ok(()) => {
                        self.screen = Screen::Main;
                        self.status = None;
                        self.touch();
                        self.normalize();
                    }
                    Err(e) => self.report(Err(e), String::new()),
                }
            }
            Job::Reload => {
                let result = self.session.reload();
                self.normalize();
                self.report(result, "Reloaded changes saved elsewhere".into());
            }
        }
    }

    pub fn unlock(&mut self) {
        let passphrase = match self.session.mode {
            crate::vault::store::Mode::Keychain => None,
            crate::vault::store::Mode::Passphrase => Some(SecretString::from(self.passphrase.to_string())),
        };
        self.passphrase.clear();
        self.busy("Unlocking…");
        self.pending = Some(Job::Unlock(passphrase));
    }
}
