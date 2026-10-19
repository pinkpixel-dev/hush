use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::paths::Paths;
use crate::project;
use crate::vault::store::{self, Mode};

/// Checks the vault folder, its files, the config, and the key, one line each.
/// Read-only: it never takes the lock or changes anything.
pub fn run(paths: &Paths) -> Result<()> {
    let mut report = Report::default();
    if check_layout(paths, &mut report) {
        match store::open(paths) {
            Ok(_) => report.ok("unlock", "the key opens the vault"),
            Err(e) => report.fail("unlock", &format!("{e:#}")),
        }
    } else {
        report.info("unlock", "skipped until the problems above are fixed");
    }
    report_leftovers(paths, &mut report);
    report_project(&mut report);

    match report.problems {
        0 => Ok(()),
        1 => bail!("found 1 problem"),
        n => bail!("found {n} problems"),
    }
}

/// Reports on the folder, files, and config. Returns whether unlocking is worth trying.
fn check_layout(paths: &Paths, report: &mut Report) -> bool {
    let dir = &paths.dir;
    if fs::symlink_metadata(dir).is_err() {
        report.fail("folder", &format!("{} doesn't exist. Run `hush init`", dir.display()));
        return false;
    }
    let before = report.problems;
    match store::check(paths) {
        Ok(()) => report.ok("folder", &format!("{}{}", dir.display(), mode_suffix(dir))),
        Err(e) => report.fail("folder", &format!("{e:#}")),
    }
    for file in [paths.vault(), paths.config()] {
        let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match fs::symlink_metadata(&file) {
            Err(_) => report.fail(&name, "missing. Run `hush init`, or `hush recover --from <copy>`"),
            Ok(meta) if !meta.is_file() => report.fail(&name, "isn't a regular file"),
            Ok(_) if loose(&file) => report.warn(&name, &format!("readable by other users{}", mode_suffix(&file))),
            Ok(_) => report.ok(&name, mode_suffix(&file).trim()),
        }
    }
    if report.problems > before {
        return false;
    }
    match store::describe(paths) {
        Ok((Mode::Passphrase, _)) => report.ok("config", "passphrase mode"),
        Ok((Mode::Keychain, true)) => report.ok("config", "keychain mode, recovery key set"),
        Ok((Mode::Keychain, false)) => {
            report.warn("config", "keychain mode, no recovery key. Run `hush recovery-key`")
        }
        Err(e) => {
            report.fail("config", &format!("{e:#}"));
            return false;
        }
    }
    true
}

/// Temp files left by a write that was interrupted before its rename.
fn report_leftovers(paths: &Paths, report: &mut Report) {
    let Ok(entries) = fs::read_dir(&paths.dir) else { return };
    let mut leftovers: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    if !leftovers.is_empty() {
        leftovers.sort();
        report.warn("leftovers", &format!("{} from an interrupted write, safe to delete while no hush command is running", leftovers.join(", ")));
    }
}

fn report_project(report: &mut Report) {
    let found = std::env::current_dir().ok().map(|cwd| project::find(&cwd));
    match found {
        Some(Ok(Some((name, marker)))) => report.info("project", &format!("{name} (from {})", marker.display())),
        Some(Ok(None)) | None => report.info("project", "this folder isn't linked"),
        Some(Err(e)) => report.fail("project", &format!("{e:#}")),
    }
}

#[cfg(unix)]
fn permissions(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).ok().map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn permissions(_: &Path) -> Option<u32> {
    None
}

fn loose(path: &Path) -> bool {
    permissions(path).is_some_and(|mode| mode & 0o077 != 0)
}

fn mode_suffix(path: &Path) -> String {
    permissions(path).map(|mode| format!(" ({mode:o})")).unwrap_or_default()
}

#[derive(Default)]
struct Report {
    problems: usize,
}

impl Report {
    fn line(&self, mark: char, label: &str, message: &str) {
        println!("{mark} {label:<11} {message}");
    }

    fn ok(&self, label: &str, message: &str) {
        self.line('✓', label, message);
    }

    fn info(&self, label: &str, message: &str) {
        self.line('·', label, message);
    }

    fn warn(&self, label: &str, message: &str) {
        self.line('!', label, message);
    }

    fn fail(&mut self, label: &str, message: &str) {
        self.problems += 1;
        self.line('✗', label, message);
    }
}
