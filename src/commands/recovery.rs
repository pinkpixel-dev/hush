use std::io::{IsTerminal, Read};
use std::path::Path;

use age::secrecy::ExposeSecret;
use age::x25519;
use anyhow::{Result, anyhow};
use zeroize::Zeroizing;

use crate::paths::Paths;
use crate::vault::store;

pub fn create(paths: &Paths, replace: bool) -> Result<()> {
    let _lock = store::lock(paths)?;
    let recovery = store::add_recovery(paths, replace)?;
    eprintln!("Your recovery key is below. It's shown once and never stored, so save it somewhere");
    eprintln!("safe that isn't this machine, like a password manager. Anyone with it and a copy");
    eprintln!("of vault.age can read every secret.");
    println!("{}", recovery.to_string().expose_secret());
    Ok(())
}

pub fn recover(paths: &Paths, from: Option<&Path>) -> Result<()> {
    // The vault folder may not exist yet when rebuilding from a copy.
    let _lock = if from.is_none() { Some(store::lock(paths)?) } else { None };
    let recovery = read_recovery_key()?;
    let projects = store::recover(paths, from, recovery)?;
    let plural = if projects == 1 { "" } else { "s" };
    eprintln!("Recovered the vault at {} ({projects} project{plural}).", paths.vault().display());
    eprintln!("A new key is in your keychain. Your recovery key still works.");
    Ok(())
}

/// Hidden prompt when interactive, otherwise stdin.
fn read_recovery_key() -> Result<x25519::Identity> {
    let mut input = Zeroizing::new(String::new());
    if std::io::stdin().is_terminal() {
        *input = rpassword::prompt_password("Recovery key: ")?;
    } else {
        std::io::stdin().read_to_string(&mut input)?;
    }
    input.trim().parse().map_err(|_| anyhow!("that isn't a valid recovery key (it starts with AGE-SECRET-KEY-1)"))
}
