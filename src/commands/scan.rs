use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

/// Folders that are huge or never hold project .env files worth migrating.
const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", ".cache", ".cargo", ".rustup", ".npm", ".pnpm-store",
    ".local", "vendor", ".venv", "venv", "__pycache__", ".next", "dist", "build",
];

/// Templates that are meant to be committed and hold no real secrets.
const TEMPLATE_SUFFIXES: &[&str] = &["example", "sample", "template", "dist", "defaults"];

pub fn run(dir: Option<PathBuf>) -> Result<()> {
    let root = match dir {
        Some(dir) => dir,
        None => directories::BaseDirs::new()
            .context("couldn't find your home folder")?
            .home_dir()
            .to_path_buf(),
    };

    let mut found = 0;
    let walker = WalkDir::new(&root).follow_links(false).into_iter().filter_entry(|entry| {
        !(entry.depth() > 0
            && entry.file_type().is_dir()
            && entry.file_name().to_str().is_some_and(|name| SKIP_DIRS.contains(&name)))
    });
    for entry in walker.filter_map(Result::ok) {
        let is_match = entry.file_type().is_file() && entry.file_name().to_str().is_some_and(is_env_file);
        if is_match {
            found += 1;
            println!("{}  ({})", entry.path().display(), describe(entry.path()));
        }
    }

    if found == 0 {
        eprintln!("No .env files found under {}.", root.display());
    } else {
        eprintln!("\nFound {found}. Move one into the vault with `hush import <path>` from its project folder.");
    }
    Ok(())
}

fn is_env_file(name: &str) -> bool {
    match name.strip_prefix(".env") {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix('.')
            .is_some_and(|suffix| !suffix.is_empty() && !TEMPLATE_SUFFIXES.contains(&suffix)),
        None => false,
    }
}

/// Counts variables without ever printing a value.
fn describe(path: &Path) -> String {
    match dotenvy::from_path_iter(path) {
        Ok(iter) => {
            let count = iter.filter(Result::is_ok).count();
            format!("{count} {}", if count == 1 { "variable" } else { "variables" })
        }
        Err(_) => "unreadable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::is_env_file;

    #[test]
    fn matches_real_env_files_only() {
        assert!(is_env_file(".env"));
        assert!(is_env_file(".env.local"));
        assert!(is_env_file(".env.production"));
        assert!(!is_env_file(".env.example"));
        assert!(!is_env_file(".envrc"));
        assert!(!is_env_file(".env."));
        assert!(!is_env_file("env"));
    }
}
