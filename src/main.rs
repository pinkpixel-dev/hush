mod cli;
mod commands;
mod exec;
mod keychain;
mod names;
mod paths;
mod project;
mod vault;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match commands::dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hush: {e:#}");
            ExitCode::FAILURE
        }
    }
}
