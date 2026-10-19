use anyhow::Result;

use crate::paths::Paths;
use crate::vault::store::{self, Mode};

pub fn run(paths: &Paths, passphrase: bool) -> Result<()> {
    let mode = if passphrase { Mode::Passphrase } else { Mode::Keychain };
    store::create(paths, mode)?;
    eprintln!("Created vault at {}", paths.vault().display());
    match mode {
        Mode::Keychain => {
            eprintln!("Its key is stored in your system keychain.");
            eprintln!("Run `hush recovery-key` so losing the keychain entry doesn't mean losing the vault.");
        }
        Mode::Passphrase => eprintln!("Unlock it with your passphrase (or HUSH_PASSPHRASE)."),
    }
    eprintln!("Next: run `hush link <project>` inside a project folder.");
    Ok(())
}
