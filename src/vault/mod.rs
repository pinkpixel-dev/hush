pub mod crypto;
pub mod store;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Secret name -> value for one project environment.
pub type Secrets = BTreeMap<String, String>;

/// The decrypted vault: project -> environment -> secrets.
#[derive(Serialize, Deserialize)]
pub struct Vault {
    pub version: u32,
    pub projects: BTreeMap<String, BTreeMap<String, Secrets>>,
}

impl Vault {
    pub fn new() -> Self {
        Self { version: 1, projects: BTreeMap::new() }
    }

    pub fn secrets(&self, project: &str, env: &str) -> Option<&Secrets> {
        self.projects.get(project)?.get(env)
    }

    pub fn secrets_mut(&mut self, project: &str, env: &str) -> &mut Secrets {
        self.projects
            .entry(project.to_string())
            .or_default()
            .entry(env.to_string())
            .or_default()
    }

    /// Removes a secret and prunes any environment or project left empty.
    pub fn remove(&mut self, project: &str, env: &str, key: &str) -> bool {
        let Some(envs) = self.projects.get_mut(project) else { return false };
        let Some(secrets) = envs.get_mut(env) else { return false };
        let Some(mut value) = secrets.remove(key) else { return false };
        value.zeroize();
        if secrets.is_empty() {
            envs.remove(env);
        }
        if envs.is_empty() {
            self.projects.remove(project);
        }
        true
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        for envs in self.projects.values_mut() {
            for secrets in envs.values_mut() {
                for value in secrets.values_mut() {
                    value.zeroize();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_prunes_empty_scopes() {
        let mut vault = Vault::new();
        vault.secrets_mut("app", "dev").insert("A".into(), "1".into());
        assert!(vault.remove("app", "dev", "A"));
        assert!(vault.projects.is_empty());
        assert!(!vault.remove("app", "dev", "A"));
    }
}
