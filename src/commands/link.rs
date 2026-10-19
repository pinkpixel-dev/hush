use std::fs;

use anyhow::{Context, Result, bail};

use crate::names;
use crate::project::MARKER;

pub fn run(name: &str, force: bool) -> Result<()> {
    names::validate_name("project", name)?;
    let cwd = std::env::current_dir()?;
    let marker = cwd.join(MARKER);
    if marker.exists() && !force {
        let current = fs::read_to_string(&marker).unwrap_or_default();
        bail!("this folder is already linked to `{}`. Use --force to change it", current.trim());
    }
    fs::write(&marker, format!("{name}\n")).with_context(|| format!("couldn't write {}", marker.display()))?;
    eprintln!("Linked {} to project `{name}`.", cwd.display());
    Ok(())
}
