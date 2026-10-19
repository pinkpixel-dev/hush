use age::secrecy::{ExposeSecret, SecretString};
use age::x25519;
use anyhow::{Result, anyhow, bail};
use zeroize::Zeroizing;

use crate::paths::Paths;
use crate::vault::crypto::Key;

const SERVICE: &str = "hush";
const PASSPHRASE_VAR: &str = "HUSH_PASSPHRASE";
/// Minimum length for new passphrases. Existing vaults aren't affected.
const MIN_PASSPHRASE: usize = 12;

/// One keychain entry per vault directory, so `HUSH_DIR` vaults don't collide.
fn entry(paths: &Paths) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("vault:{}", paths.dir.display())).map_err(keychain_error)
}

fn keychain_error(e: keyring::Error) -> anyhow::Error {
    anyhow!(
        "couldn't reach your system keychain ({e}). Make sure GNOME Keyring or KWallet is running, \
         or create a passphrase vault with `hush init --passphrase`"
    )
}

pub fn create_identity(paths: &Paths) -> Result<Key> {
    let identity = x25519::Identity::generate();
    entry(paths)?
        .set_password(identity.to_string().expose_secret())
        .map_err(keychain_error)?;
    Ok(Key::Identity(identity))
}

pub fn load_identity(paths: &Paths) -> Result<Key> {
    let stored = entry(paths)?.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => anyhow!(
            "the key for this vault is missing from your keychain. If you made a recovery key, run `hush recover`"
        ),
        e => keychain_error(e),
    })?;
    let stored = Zeroizing::new(stored);
    let identity = stored
        .parse::<x25519::Identity>()
        .map_err(|e| anyhow!("the key in your keychain isn't a valid age identity ({e})"))?;
    Ok(Key::Identity(identity))
}

/// Reads the passphrase from `HUSH_PASSPHRASE` (for scripts and CI) or a hidden prompt.
pub fn passphrase() -> Result<SecretString> {
    if let Ok(value) = std::env::var(PASSPHRASE_VAR) {
        return Ok(value.into());
    }
    Ok(rpassword::prompt_password("Vault passphrase: ")?.into())
}

pub fn new_passphrase() -> Result<SecretString> {
    if let Ok(value) = std::env::var(PASSPHRASE_VAR) {
        if value.chars().count() < MIN_PASSPHRASE {
            bail!("{PASSPHRASE_VAR} is too short. New passphrases need at least {MIN_PASSPHRASE} characters");
        }
        return Ok(value.into());
    }
    let first = Zeroizing::new(rpassword::prompt_password("New vault passphrase: ")?);
    if first.chars().count() < MIN_PASSPHRASE {
        bail!("the passphrase is too short. Use at least {MIN_PASSPHRASE} characters");
    }
    let second = Zeroizing::new(rpassword::prompt_password("Confirm passphrase: ")?);
    if *first != *second {
        bail!("the passphrases didn't match");
    }
    Ok(first.to_string().into())
}
