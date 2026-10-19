use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::names;

/// The non-secret file that links a folder to a vault project.
pub const MARKER: &str = ".hush";

/// Walks up from `start` looking for a `.hush` marker.
pub fn find(start: &Path) -> Result<Option<(String, PathBuf)>> {
    for dir in start.ancestors() {
        let marker = dir.join(MARKER);
        if marker.is_file() {
            let name = fs::read_to_string(&marker)
                .with_context(|| format!("couldn't read {}", marker.display()))?
                .trim()
                .to_string();
            names::validate_name("project", &name)
                .with_context(|| format!("{} is damaged", marker.display()))?;
            return Ok(Some((name, marker)));
        }
    }
    Ok(None)
}

/// Uses the `--project` flag if given, otherwise the linked project.
pub fn resolve(flag: Option<String>) -> Result<String> {
    if let Some(name) = flag {
        names::validate_name("project", &name)?;
        return Ok(name);
    }
    let cwd = std::env::current_dir()?;
    match find(&cwd)? {
        Some((name, _)) => Ok(name),
        None => bail!("this folder isn't linked to a project. Run `hush link <name>` or pass --project"),
    }
}
