use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hush", version, about = "Encrypted, local-first replacement for .env files")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Which project and environment a command works on.
#[derive(Args)]
pub struct Target {
    /// Project name (defaults to the folder's linked project)
    #[arg(short, long)]
    pub project: Option<String>,

    /// Environment name
    #[arg(short, long, default_value = "dev")]
    pub env: String,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new vault
    Init {
        /// Use a passphrase instead of the system keychain
        #[arg(long)]
        passphrase: bool,
    },
    /// Link the current folder to a project
    Link {
        name: String,
        /// Replace an existing link
        #[arg(long)]
        force: bool,
    },
    /// Add or update a secret (value comes from a hidden prompt or stdin)
    Set {
        key: String,
        #[command(flatten)]
        target: Target,
    },
    /// Print a secret's value
    Get {
        key: String,
        #[command(flatten)]
        target: Target,
    },
    /// List secret names (never values)
    List {
        #[command(flatten)]
        target: Target,
        /// Show every project and environment
        #[arg(long)]
        all: bool,
    },
    /// Remove a secret
    Rm {
        key: String,
        #[command(flatten)]
        target: Target,
    },
    /// Run a command with secrets injected as environment variables
    Run {
        #[command(flatten)]
        target: Target,
        /// The command to run, after `--`
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Import secrets from a .env file
    Import {
        file: PathBuf,
        #[command(flatten)]
        target: Target,
    },
    /// Find .env files on disk
    Scan {
        /// Folder to search (defaults to your home folder)
        dir: Option<PathBuf>,
    },
    /// Make a recovery key that can unlock the vault if the keychain entry is lost
    RecoveryKey {
        /// Replace the existing recovery key (the old one stops working)
        #[arg(long)]
        replace: bool,
    },
    /// Unlock the vault with its recovery key and store a new key in the keychain
    Recover {
        /// Rebuild from a copy of vault.age (for a new machine or a wiped vault folder)
        #[arg(long)]
        from: Option<PathBuf>,
    },
}
