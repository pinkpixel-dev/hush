use std::process::Command;

use anyhow::{Context, Result};

use crate::vault::Secrets;

/// Runs `command` with the secrets added to its environment. Vault values win
/// over variables that are already set. On Unix the hush process is replaced,
/// so signals and the exit code belong to the child.
pub fn run(command: &[String], secrets: &Secrets) -> Result<()> {
    let (program, args) = command.split_first().context("no command given")?;
    let mut cmd = Command::new(program);
    cmd.args(args).envs(secrets).env_remove("HUSH_PASSPHRASE");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        Err(anyhow::anyhow!("couldn't start `{program}` ({err})"))
    }

    #[cfg(not(unix))]
    {
        let status = cmd.status().with_context(|| format!("couldn't start `{program}`"))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
