//! Vault changes the TUI can make. Each one checks everything before it
//! touches the vault, so a failed change leaves it as it was. Removed values
//! are zeroized.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use zeroize::Zeroize;

use crate::names;
use crate::vault::{Secrets, Vault};

/// Adds or replaces a secret, creating the project and environment if needed.
/// The value is zeroized if it's refused.
pub fn set_secret(vault: &mut Vault, project: &str, env: &str, key: &str, mut value: String) -> Result<()> {
    let valid = names::validate_name("project", project)
        .and_then(|()| names::validate_name("environment", env))
        .and_then(|()| names::validate_key(key))
        .and_then(|()| if value.is_empty() { bail!("the value is empty, nothing saved") } else { Ok(()) });
    if let Err(e) = valid {
        value.zeroize();
        return Err(e);
    }
    if let Some(mut old) = vault.secrets_mut(project, env).insert(key.to_string(), value) {
        old.zeroize();
    }
    Ok(())
}

pub fn rename_secret(vault: &mut Vault, project: &str, env: &str, old: &str, new: &str) -> Result<()> {
    names::validate_key(new)?;
    let secrets = env_mut(vault, project, env)?;
    rename_entry(secrets, old, new)
}

pub fn delete_secret(vault: &mut Vault, project: &str, env: &str, key: &str) -> Result<()> {
    if !vault.remove(project, env, key) {
        bail!("{key} isn't in {project}/{env}");
    }
    Ok(())
}

pub fn rename_env(vault: &mut Vault, project: &str, old: &str, new: &str) -> Result<()> {
    names::validate_name("environment", new)?;
    let envs = vault.projects.get_mut(project).with_context(|| format!("there's no project {project}"))?;
    rename_entry(envs, old, new)
}

/// Copies every secret in one environment into a new one in the same project.
pub fn copy_env(vault: &mut Vault, project: &str, from: &str, to: &str) -> Result<()> {
    names::validate_name("environment", to)?;
    let envs = vault.projects.get_mut(project).with_context(|| format!("there's no project {project}"))?;
    if envs.contains_key(to) {
        bail!("{project}/{to} already exists");
    }
    let copy = envs.get(from).with_context(|| format!("there's no environment {project}/{from}"))?.clone();
    envs.insert(to.to_string(), copy);
    Ok(())
}

pub fn delete_env(vault: &mut Vault, project: &str, env: &str) -> Result<()> {
    let envs = vault.projects.get_mut(project).with_context(|| format!("there's no project {project}"))?;
    let mut secrets = envs.remove(env).with_context(|| format!("there's no environment {project}/{env}"))?;
    wipe(&mut secrets);
    if envs.is_empty() {
        vault.projects.remove(project);
    }
    Ok(())
}

pub fn rename_project(vault: &mut Vault, old: &str, new: &str) -> Result<()> {
    names::validate_name("project", new)?;
    rename_entry(&mut vault.projects, old, new)
}

pub fn delete_project(vault: &mut Vault, project: &str) -> Result<()> {
    let mut envs = vault.projects.remove(project).with_context(|| format!("there's no project {project}"))?;
    envs.values_mut().for_each(wipe);
    Ok(())
}

fn env_mut<'a>(vault: &'a mut Vault, project: &str, env: &str) -> Result<&'a mut Secrets> {
    vault
        .projects
        .get_mut(project)
        .and_then(|envs| envs.get_mut(env))
        .with_context(|| format!("there's no environment {project}/{env}"))
}

/// Moves a map entry to a new key, refusing to overwrite an existing one.
fn rename_entry<V>(map: &mut BTreeMap<String, V>, old: &str, new: &str) -> Result<()> {
    if old == new {
        return Ok(());
    }
    if map.contains_key(new) {
        bail!("{new} already exists");
    }
    let value = map.remove(old).with_context(|| format!("{old} doesn't exist anymore"))?;
    map.insert(new.to_string(), value);
    Ok(())
}

fn wipe(secrets: &mut Secrets) {
    secrets.values_mut().for_each(Zeroize::zeroize);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vault {
        let mut vault = Vault::new();
        set_secret(&mut vault, "api", "dev", "A", "1".into()).unwrap();
        set_secret(&mut vault, "api", "dev", "B", "2".into()).unwrap();
        set_secret(&mut vault, "api", "prod", "A", "p".into()).unwrap();
        vault
    }

    #[test]
    fn set_validates_names_and_creates_scopes() {
        let mut vault = Vault::new();
        assert!(set_secret(&mut vault, "new", "staging", "KEY", "v".into()).is_ok());
        assert_eq!(vault.secrets("new", "staging").unwrap()["KEY"], "v");
        assert!(set_secret(&mut vault, "new", "staging", "BAD-KEY", "v".into()).is_err());
        assert!(set_secret(&mut vault, "bad name", "dev", "KEY", "v".into()).is_err());
        assert!(set_secret(&mut vault, "new", "staging", "EMPTY", String::new()).is_err());
        assert!(vault.secrets("bad name", "dev").is_none());
    }

    #[test]
    fn rename_secret_refuses_to_overwrite() {
        let mut vault = sample();
        assert!(rename_secret(&mut vault, "api", "dev", "A", "B").is_err());
        assert_eq!(vault.secrets("api", "dev").unwrap()["A"], "1");
        rename_secret(&mut vault, "api", "dev", "A", "C").unwrap();
        let dev = vault.secrets("api", "dev").unwrap();
        assert_eq!((dev.get("A"), dev["C"].as_str()), (None, "1"));
    }

    #[test]
    fn copy_env_makes_an_independent_copy() {
        let mut vault = sample();
        assert!(copy_env(&mut vault, "api", "dev", "prod").is_err());
        copy_env(&mut vault, "api", "dev", "staging").unwrap();
        set_secret(&mut vault, "api", "staging", "A", "changed".into()).unwrap();
        assert_eq!(vault.secrets("api", "dev").unwrap()["A"], "1");
        assert_eq!(vault.secrets("api", "staging").unwrap()["B"], "2");
    }

    #[test]
    fn deleting_the_last_env_removes_the_project() {
        let mut vault = sample();
        delete_env(&mut vault, "api", "dev").unwrap();
        assert!(vault.projects.contains_key("api"));
        delete_env(&mut vault, "api", "prod").unwrap();
        assert!(vault.projects.is_empty());
    }

    #[test]
    fn rename_and_delete_projects() {
        let mut vault = sample();
        rename_env(&mut vault, "api", "prod", "live").unwrap();
        rename_project(&mut vault, "api", "backend").unwrap();
        assert_eq!(vault.secrets("backend", "live").unwrap()["A"], "p");
        assert!(rename_project(&mut vault, "backend", "no spaces").is_err());
        delete_project(&mut vault, "backend").unwrap();
        assert!(delete_project(&mut vault, "backend").is_err());
    }
}
