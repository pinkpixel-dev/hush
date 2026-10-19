use std::io::{IsTerminal, Read};

use anyhow::{Result, bail};
use zeroize::Zeroizing;

use super::scope;
use crate::cli::Target;
use crate::names;
use crate::paths::Paths;
use crate::vault::store;

pub fn set(paths: &Paths, key: &str, target: Target) -> Result<()> {
    names::validate_key(key)?;
    let (project, env) = scope(target)?;
    let _lock = store::lock(paths)?;
    let (mut vault, unlock) = store::open(paths)?;
    let value = read_value(key)?;
    let existed = vault.secrets_mut(&project, &env).insert(key.to_string(), value.to_string()).is_some();
    store::save(paths, &vault, &unlock)?;
    let verb = if existed { "Updated" } else { "Saved" };
    eprintln!("{verb} {key} in {project}/{env}.");
    Ok(())
}

pub fn get(paths: &Paths, key: &str, target: Target) -> Result<()> {
    let (project, env) = scope(target)?;
    let (vault, _) = store::open(paths)?;
    match vault.secrets(&project, &env).and_then(|s| s.get(key)) {
        Some(value) => println!("{value}"),
        None => bail!("{key} isn't set in {project}/{env}"),
    }
    Ok(())
}

pub fn list(paths: &Paths, target: Target, all: bool) -> Result<()> {
    if all {
        let (vault, _) = store::open(paths)?;
        if vault.projects.is_empty() {
            eprintln!("The vault is empty.");
        }
        for (project, envs) in &vault.projects {
            println!("{project}");
            for (env, secrets) in envs {
                println!("  {env}");
                for key in secrets.keys() {
                    println!("    {key}");
                }
            }
        }
        return Ok(());
    }

    let (project, env) = scope(target)?;
    let (vault, _) = store::open(paths)?;
    match vault.secrets(&project, &env) {
        Some(secrets) => secrets.keys().for_each(|key| println!("{key}")),
        None => eprintln!("No secrets in {project}/{env} yet."),
    }
    Ok(())
}

pub fn rm(paths: &Paths, key: &str, target: Target) -> Result<()> {
    let (project, env) = scope(target)?;
    let _lock = store::lock(paths)?;
    let (mut vault, unlock) = store::open(paths)?;
    if !vault.remove(&project, &env, key) {
        bail!("{key} isn't set in {project}/{env}");
    }
    store::save(paths, &vault, &unlock)?;
    eprintln!("Removed {key} from {project}/{env}.");
    Ok(())
}

/// Hidden prompt when interactive, otherwise stdin (minus one trailing newline).
/// Values are never taken as arguments so they stay out of shell history.
fn read_value(key: &str) -> Result<Zeroizing<String>> {
    let mut value = Zeroizing::new(String::new());
    if std::io::stdin().is_terminal() {
        *value = rpassword::prompt_password(format!("{key}: "))?;
    } else {
        std::io::stdin().read_to_string(&mut value)?;
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
    }
    if value.is_empty() {
        bail!("empty value, nothing saved");
    }
    Ok(value)
}
