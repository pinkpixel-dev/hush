use std::path::Path;

use anyhow::{Context, Result, bail};

use super::scope;
use crate::cli::Target;
use crate::names;
use crate::paths::Paths;
use crate::vault::store;

pub fn run(paths: &Paths, file: &Path, target: Target) -> Result<()> {
    let (project, env) = scope(target)?;
    let mut pairs = Vec::new();
    for item in dotenvy::from_path_iter(file).with_context(|| format!("couldn't read {}", file.display()))? {
        let (key, value) = item.with_context(|| format!("couldn't parse {}", file.display()))?;
        names::validate_key(&key)?;
        pairs.push((key, value));
    }
    if pairs.is_empty() {
        bail!("no variables found in {}", file.display());
    }

    let _lock = store::lock(paths)?;
    let (mut vault, unlock) = store::open(paths)?;
    let secrets = vault.secrets_mut(&project, &env);
    let (mut added, mut updated) = (0, 0);
    for (key, value) in pairs {
        match secrets.insert(key, value) {
            Some(_) => updated += 1,
            None => added += 1,
        }
    }
    store::save(paths, &vault, &unlock)?;

    eprintln!("Imported {} into {project}/{env} ({added} new, {updated} updated).", added + updated);
    eprintln!("{} is untouched. Delete it once `hush run` works for you.", file.display());
    Ok(())
}
