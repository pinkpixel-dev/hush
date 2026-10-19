use anyhow::Result;

use super::scope;
use crate::cli::Target;
use crate::exec;
use crate::paths::Paths;
use crate::vault::{Secrets, store};

pub fn run(paths: &Paths, target: Target, command: &[String]) -> Result<()> {
    let (project, env) = scope(target)?;
    let (vault, _) = store::open(paths)?;
    let empty = Secrets::new();
    let secrets = vault.secrets(&project, &env).unwrap_or(&empty);
    if secrets.is_empty() {
        eprintln!("hush: no secrets in {project}/{env}, running without any.");
    }
    exec::run(command, secrets)
}
