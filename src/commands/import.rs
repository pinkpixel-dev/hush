use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use zeroize::Zeroizing;

use super::scope;
use crate::cli::Target;
use crate::paths::Paths;
use crate::vault::store;
use crate::{dotenv, names};

pub fn run(paths: &Paths, file: &Path, target: Target) -> Result<()> {
    let (project, env) = scope(target)?;
    let text = Zeroizing::new(fs::read_to_string(file).with_context(|| format!("couldn't read {}", file.display()))?);
    let pairs = dotenv::parse(&text).with_context(|| format!("couldn't parse {}", file.display()))?;
    for (key, _) in &pairs {
        names::validate_key(key)?;
    }
    if pairs.is_empty() {
        bail!("no variables found in {}", file.display());
    }
    // hush stores `$` as written. Point it out in case one was meant as a reference.
    let dollars: Vec<String> = pairs.iter().filter(|(_, v)| v.contains('$')).map(|(k, _)| k.clone()).collect();

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
    if !dollars.is_empty() {
        eprintln!("Kept `$` as written in {}. hush doesn't expand variable references.", dollars.join(", "));
    }
    eprintln!("{} is untouched. Delete it once `hush run` works for you.", file.display());
    Ok(())
}
