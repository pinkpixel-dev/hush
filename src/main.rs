mod cli;
mod commands;
mod dotenv;
mod exec;
mod harden;
mod keychain;
mod names;
mod paths;
mod project;
mod tui;
mod vault;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    harden::no_dumps();
    let cli = cli::Cli::parse();
    match commands::dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hush: {e:#}");
            ExitCode::FAILURE
        }
    }
}
