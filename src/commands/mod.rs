mod import;
mod init;
mod link;
mod recovery;
mod run;
mod scan;
mod secrets;

use anyhow::Result;
use clap::CommandFactory;

use crate::cli::{Cli, Command, Target};
use crate::paths::Paths;
use crate::{names, project};

pub fn dispatch(cli: Cli) -> Result<()> {
    // Completions don't touch the vault, so they work before `hush init` or without a home folder.
    if let Command::Completions { shell } = cli.command {
        clap_complete::generate(shell, &mut Cli::command(), "hush", &mut std::io::stdout());
        return Ok(());
    }
    let paths = Paths::resolve()?;
    match cli.command {
        Command::Init { passphrase } => init::run(&paths, passphrase),
        Command::Link { name, force } => link::run(&name, force),
        Command::Set { key, target } => secrets::set(&paths, &key, target),
        Command::Get { key, target } => secrets::get(&paths, &key, target),
        Command::List { target, all } => secrets::list(&paths, target, all),
        Command::Rm { key, target } => secrets::rm(&paths, &key, target),
        Command::Run { target, command } => run::run(&paths, target, &command),
        Command::Import { file, target } => import::run(&paths, &file, target),
        Command::Scan { dir } => scan::run(dir),
        Command::RecoveryKey { replace } => recovery::create(&paths, replace),
        Command::Recover { from } => recovery::recover(&paths, from.as_deref()),
        Command::Completions { .. } => unreachable!("handled above"),
    }
}

/// Resolves a target to a validated (project, environment) pair.
fn scope(target: Target) -> Result<(String, String)> {
    names::validate_name("environment", &target.env)?;
    Ok((project::resolve(target.project)?, target.env))
}
