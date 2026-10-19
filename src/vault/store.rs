use std::fs;
use std::path::Path;

use age::x25519;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{VERSION, Vault};
use super::crypto::{self, Key};
use super::files::{self, create_private_dir, write_atomic};
use crate::keychain;
use crate::paths::Paths;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Keychain,
    Passphrase,
}

#[derive(Serialize, Deserialize)]
struct Config {
    mode: Mode,
    /// Public half of the recovery key. Keychain vaults are also encrypted to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery: Option<String>,
}

impl Config {
    fn read(paths: &Paths) -> Result<Self> {
        let bytes = fs::read(paths.config()).with_context(|| format!("couldn't read {}", paths.config().display()))?;
        serde_json::from_slice(&bytes).context("the hush config file is damaged")
    }

    fn write(&self, paths: &Paths) -> Result<()> {
        write_atomic(&paths.config(), &serde_json::to_vec_pretty(self)?)
    }

    fn recovery(&self) -> Result<Option<x25519::Recipient>> {
        self.recovery
            .as_deref()
            .map(|r| r.parse().map_err(|e| anyhow!("the recovery key in the hush config is damaged ({e})")))
            .transpose()
    }
}

pub fn create(paths: &Paths, mode: Mode) -> Result<()> {
    if paths.vault().exists() {
        bail!("a vault already exists at {}", paths.vault().display());
    }
    create_private_dir(&paths.dir)?;
    check(paths)?;
    let _lock = acquire(paths)?;
    // Another `hush init` may have finished while this one waited for the lock.
    if paths.vault().exists() {
        bail!("a vault already exists at {}", paths.vault().display());
    }
    let key = match mode {
        Mode::Keychain => keychain::create_identity(paths)?,
        Mode::Passphrase => Key::Passphrase(keychain::new_passphrase()?),
    };
    Config { mode, recovery: None }.write(paths)?;
    save(paths, &Vault::new(), &key)
}

pub fn open(paths: &Paths) -> Result<(Vault, Key)> {
    check(paths)?;
    if !paths.vault().exists() {
        bail!("no vault yet. Run `hush init` first");
    }
    let config = Config::read(paths)?;
    let key = match config.mode {
        Mode::Keychain => keychain::load_identity(paths)?,
        Mode::Passphrase => Key::Passphrase(keychain::passphrase()?),
    };
    let ciphertext = fs::read(paths.vault()).context("couldn't read the vault")?;
    let plaintext = crypto::decrypt(&key, &ciphertext)?;
    Ok((parse(&plaintext)?, key))
}

/// Decrypts the vault with a key that's already unlocked, without prompting.
/// The TUI keeps its key for the session and reopens with this before edits.
pub fn open_with(paths: &Paths, key: &Key) -> Result<Vault> {
    check(paths)?;
    let ciphertext = fs::read(paths.vault()).context("couldn't read the vault")?;
    parse(&crypto::decrypt(key, &ciphertext)?)
}

/// Reads decrypted vault JSON. A newer format is refused, since saving it
/// with this build would silently drop whatever the newer one added.
fn parse(plaintext: &[u8]) -> Result<Vault> {
    let vault: Vault = serde_json::from_slice(plaintext).context("the vault decrypted but its contents are damaged")?;
    if vault.version > VERSION {
        bail!("this vault was saved by a newer version of hush (format {}). Upgrade hush to open it", vault.version);
    }
    if vault.version != VERSION {
        bail!("the vault decrypted but has an unknown format ({})", vault.version);
    }
    Ok(vault)
}

/// An exclusive lock on the vault, released when dropped. Take it before
/// `open` in any command that saves, so two hush processes can't read the
/// same vault and then overwrite each other's changes.
pub struct VaultLock(#[allow(dead_code)] fs::File);

pub fn lock(paths: &Paths) -> Result<VaultLock> {
    check(paths)?;
    if !paths.vault().exists() {
        bail!("no vault yet. Run `hush init` first");
    }
    acquire(paths)
}

/// Takes the lock without requiring a vault, for commands that create one.
/// The caller has already run `check`.
fn acquire(paths: &Paths) -> Result<VaultLock> {
    let path = paths.lock();
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(&path).with_context(|| format!("couldn't open {}", path.display()))?;
    file.lock().context("couldn't lock the vault")?;
    Ok(VaultLock(file))
}

/// Refuses a vault folder that's a symlink or open to other users, and any
/// symlink where hush's own files should be.
pub fn check(paths: &Paths) -> Result<()> {
    files::check_private_dir(&paths.dir)?;
    for file in [paths.vault(), paths.config(), paths.lock()] {
        files::reject_symlink(&file)?;
    }
    Ok(())
}

/// The vault's mode and whether it has a recovery key, without unlocking it.
pub fn describe(paths: &Paths) -> Result<(Mode, bool)> {
    let config = Config::read(paths)?;
    config.recovery()?;
    Ok((config.mode, config.recovery.is_some()))
}

pub fn save(paths: &Paths, vault: &Vault, key: &Key) -> Result<()> {
    let plaintext = Zeroizing::new(serde_json::to_vec(vault)?);
    let ciphertext = crypto::encrypt(key, Config::read(paths)?.recovery()?.as_ref(), &plaintext)?;
    write_atomic(&paths.vault(), &ciphertext)
}

/// Gives a keychain vault a new recovery key and returns it. The caller holds
/// the vault lock. Refuses to replace an existing key unless `replace` is set,
/// since that makes the old key useless for future saves.
pub fn add_recovery(paths: &Paths, replace: bool) -> Result<x25519::Identity> {
    let mut config = Config::read(paths)?;
    if config.mode == Mode::Passphrase {
        bail!("passphrase vaults don't use recovery keys. Your passphrase already unlocks the vault anywhere");
    }
    if config.recovery.is_some() && !replace {
        bail!("this vault already has a recovery key. Use `--replace` to make a new one (the old one stops working)");
    }
    let (vault, key) = open(paths)?;
    let recovery = x25519::Identity::generate();
    config.recovery = Some(recovery.to_public().to_string());
    config.write(paths)?;
    save(paths, &vault, &key)?;
    Ok(recovery)
}

/// Unlocks a vault with its recovery key, puts a fresh key in the keychain, and
/// re-encrypts. With `from`, rebuilds the vault folder from a copied `vault.age`.
/// The vault is written last, so if anything fails partway the old vault still
/// opens with the recovery key and you can run this again.
pub fn recover(paths: &Paths, from: Option<&Path>, recovery: x25519::Identity) -> Result<usize> {
    let source = match from {
        Some(file) => {
            if paths.vault().exists() {
                bail!(
                    "a vault already exists at {}. Move it aside first, hush won't overwrite it",
                    paths.vault().display()
                );
            }
            file.to_path_buf()
        }
        None => {
            if Config::read(paths)?.mode == Mode::Passphrase {
                bail!("passphrase vaults don't use recovery keys. Unlock it with your passphrase");
            }
            paths.vault()
        }
    };
    let vault = unlock_with_recovery(&source, &recovery)?;

    // With `--from` the folder may not exist until now, so it's created and
    // locked here. In place, the caller already checked and locked it.
    let _lock = match from {
        Some(_) => {
            create_private_dir(&paths.dir)?;
            check(paths)?;
            let lock = acquire(paths)?;
            if paths.vault().exists() {
                bail!("a vault appeared at {} while recovering. hush won't overwrite it", paths.vault().display());
            }
            Some(lock)
        }
        None => None,
    };
    let key = keychain::create_identity(paths)?;
    Config { mode: Mode::Keychain, recovery: Some(recovery.to_public().to_string()) }.write(paths)?;
    save(paths, &vault, &key)?;
    Ok(vault.projects.len())
}

fn unlock_with_recovery(file: &Path, recovery: &x25519::Identity) -> Result<Vault> {
    let ciphertext = fs::read(file).with_context(|| format!("couldn't read {}", file.display()))?;
    let plaintext = crypto::decrypt(&Key::Identity(recovery.clone()), &ciphertext)
        .map_err(|_| anyhow!("that recovery key doesn't unlock {}", file.display()))?;
    parse(&plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_configs_without_recovery_still_load() {
        let config: Config = serde_json::from_slice(br#"{ "mode": "keychain" }"#).unwrap();
        assert!(config.recovery().unwrap().is_none());
    }

    #[test]
    fn other_vault_formats_are_refused() {
        assert!(parse(br#"{"version":1,"projects":{}}"#).is_ok());
        let newer = parse(br#"{"version":2,"projects":{},"extra":true}"#).err().unwrap().to_string();
        assert!(newer.contains("newer version of hush"), "{newer}");
        assert!(parse(br#"{"version":0,"projects":{}}"#).err().unwrap().to_string().contains("unknown format"));
    }

    #[test]
    fn recovery_key_unlocks_a_vault_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("vault.age");
        let recovery = x25519::Identity::generate();
        let mut vault = Vault::new();
        vault.secrets_mut("app", "dev").insert("A".into(), "1".into());
        let plaintext = serde_json::to_vec(&vault).unwrap();
        let main = Key::Identity(x25519::Identity::generate());
        fs::write(&file, crypto::encrypt(&main, Some(&recovery.to_public()), &plaintext).unwrap()).unwrap();

        let restored = unlock_with_recovery(&file, &recovery).unwrap();
        assert_eq!(restored.secrets("app", "dev").unwrap()["A"], "1");
        assert!(unlock_with_recovery(&file, &x25519::Identity::generate()).is_err());
    }
}
