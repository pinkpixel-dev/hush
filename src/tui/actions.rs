//! Commands (add, edit, rename, delete, copy, ...) and the dialogs behind
//! them. Submitting a dialog queues a `Job::Edit` that `App::run_job` saves.

use super::app::{App, Change, Command, Job, Pane, Selection};
use super::dialog::{Dialog, Field, Kind, Outcome};
use super::ops;

impl App {
    pub fn run(&mut self, command: Command) {
        let (project, env) = (self.sel.project.clone().unwrap_or_default(), self.sel.env.clone().unwrap_or_default());
        let key = self.sel.secret.clone();
        let has_project = self.sel.project.is_some();
        let dialog = |kind, fields, focus| Some(Dialog::new(kind, &project, &env, fields, focus));
        let was_revealed = self.revealed;
        self.revealed = false;
        self.dialog = match (command, self.pane) {
            (Command::Quit, _) => {
                self.quit = true;
                None
            }
            (Command::Help, _) => dialog(Kind::Help, Vec::new(), 0),
            (Command::Reveal, Pane::Secrets) => {
                self.revealed = key.is_some() && !was_revealed;
                None
            }
            (Command::Reveal, pane) => {
                self.focus(Pane::ALL[pane.index() + 1]);
                None
            }
            (Command::Find, _) => {
                self.focus(Pane::Secrets);
                self.finding = true;
                self.find.get_or_insert_with(String::new);
                None
            }
            (Command::Add, pane) => {
                let new_project = pane == Pane::Projects || !has_project;
                let project_text = if new_project { self.linked_if_new() } else { project.clone() };
                let env_text = if pane == Pane::Envs || new_project { "dev" } else { env.as_str() };
                let fields = vec![
                    Field::plain("Project", &project_text),
                    Field::plain("Environment", if pane == Pane::Envs { "" } else { env_text }),
                    Field::plain("Name", ""),
                    Field::secret("Value"),
                ];
                let focus = match pane {
                    Pane::Projects => 0,
                    Pane::Envs => 1,
                    Pane::Secrets if !has_project => 0,
                    Pane::Secrets => 2,
                };
                dialog(Kind::AddSecret, fields, focus)
            }
            (Command::Edit, Pane::Secrets) => match key {
                Some(key) => dialog(Kind::EditSecret { key }, vec![Field::secret("New value")], 0),
                None => None,
            },
            (Command::Rename, Pane::Secrets) => match key {
                Some(key) => dialog(Kind::RenameSecret { key: key.clone() }, vec![Field::plain("Name", &key)], 0),
                None => None,
            },
            (Command::Delete, Pane::Secrets) => match key {
                Some(key) => dialog(Kind::DeleteSecret { key }, Vec::new(), 0),
                None => None,
            },
            _ if !has_project => None,
            (Command::Rename, Pane::Envs) => dialog(Kind::RenameEnv, vec![Field::plain("Name", &env)], 0),
            (Command::Copy, Pane::Envs) => dialog(Kind::CopyEnv, vec![Field::plain("New name", "")], 0),
            (Command::Delete, Pane::Envs) => dialog(Kind::DeleteEnv, vec![Field::plain("Type to confirm", "")], 0),
            (Command::Rename, Pane::Projects) => dialog(Kind::RenameProject, vec![Field::plain("Name", &project)], 0),
            (Command::Delete, Pane::Projects) => {
                dialog(Kind::DeleteProject, vec![Field::plain("Type to confirm", "")], 0)
            }
            _ => None,
        };
    }

    /// A new project starts with the folder's linked name if the vault doesn't
    /// have it yet.
    fn linked_if_new(&self) -> String {
        let linked = self.linked.clone().unwrap_or_default();
        if self.projects().contains(&linked) { String::new() } else { linked }
    }

    pub fn dialog_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Continue => {}
            Outcome::Cancel => self.dialog = None,
            Outcome::Submit => self.submit(),
        }
    }

    fn submit(&mut self) {
        let Some(dialog) = self.dialog.as_ref() else { return };
        let kind = dialog.kind.clone();
        let (project, env) = (dialog.project.clone(), dialog.env.clone());
        // Names are trimmed. The secret value is kept exactly, in a Zeroizing copy.
        let texts: Vec<String> = dialog.fields.iter().map(|f| if f.is_secret() { String::new() } else { f.text.trim().to_string() }).collect();
        let value = dialog.fields.iter().find(|f| f.is_secret()).map(|f| f.text.clone()).unwrap_or_default();
        let text = |i: usize| texts.get(i).cloned().unwrap_or_default();
        let neighbor = |names: Vec<String>, name: &str| {
            let at = names.iter().position(|n| n == name).unwrap_or(0);
            names.get(at + 1).or(at.checked_sub(1).and_then(|i| names.get(i))).cloned()
        };
        let keep = self.sel.clone();
        let (change, done, then): (Change, String, Selection) = match kind {
            Kind::Help => {
                self.dialog = None;
                return;
            }
            Kind::AddSecret => {
                let (p, e, k) = (text(0), text(1), text(2));
                let done = format!("Saved {k} in {p}/{e}");
                let then = Selection { project: Some(p.clone()), env: Some(e.clone()), secret: Some(k.clone()) };
                (Box::new(move |v| ops::set_secret(v, &p, &e, &k, value.to_string())), done, then)
            }
            Kind::EditSecret { key } => {
                let done = format!("Updated {key}");
                (Box::new(move |v| ops::set_secret(v, &project, &env, &key, value.to_string())), done, keep)
            }
            Kind::RenameSecret { key } => {
                let new = text(0);
                let then = Selection { secret: Some(new.clone()), ..keep };
                (Box::new(move |v| ops::rename_secret(v, &project, &env, &key, &new)), format!("Renamed to {}", text(0)), then)
            }
            Kind::DeleteSecret { key } => {
                let done = format!("Deleted {key}");
                let then = Selection { secret: neighbor(self.secrets(), &key), ..keep };
                (Box::new(move |v| ops::delete_secret(v, &project, &env, &key)), done, then)
            }
            Kind::RenameEnv => {
                let new = text(0);
                let then = Selection { env: Some(new.clone()), secret: None, ..keep };
                (Box::new(move |v| ops::rename_env(v, &project, &env, &new)), format!("Renamed to {}", text(0)), then)
            }
            Kind::CopyEnv => {
                let new = text(0);
                let done = format!("Copied {project}/{env} to {project}/{new}");
                let then = Selection { env: Some(new.clone()), secret: None, ..keep };
                (Box::new(move |v| ops::copy_env(v, &project, &env, &new)), done, then)
            }
            Kind::DeleteEnv | Kind::DeleteProject => {
                let whole_project = matches!(kind, Kind::DeleteProject);
                let target = if whole_project { project.clone() } else { env.clone() };
                if text(0) != target {
                    if let Some(dialog) = self.dialog.as_mut() {
                        dialog.error = Some(format!("type {target} to confirm"));
                    }
                    return;
                }
                if whole_project {
                    let then = Selection { project: neighbor(self.projects(), &project), ..Selection::default() };
                    (Box::new(move |v| ops::delete_project(v, &project)), format!("Deleted {target}"), then)
                } else {
                    let done = format!("Deleted {project}/{env}");
                    let then = Selection { env: neighbor(self.envs(), &env), secret: None, ..keep };
                    (Box::new(move |v| ops::delete_env(v, &project, &env)), done, then)
                }
            }
            Kind::RenameProject => {
                let new = text(0);
                let then = Selection { project: Some(new.clone()), ..keep };
                (Box::new(move |v| ops::rename_project(v, &project, &new)), format!("Renamed to {}", text(0)), then)
            }
        };
        self.busy("Saving…");
        self.pending = Some(Job::Edit { change, done, then });
    }
}
