use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::Vault;
use super::crypto::{self, Key};
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
}

pub fn create(paths: &Paths, mode: Mode) -> Result<()> {
    if paths.vault().exists() {
        bail!("a vault already exists at {}", paths.vault().display());
    }
    create_private_dir(&paths.dir)?;
    let key = match mode {
        Mode::Keychain => keychain::create_identity(paths)?,
        Mode::Passphrase => Key::Passphrase(keychain::new_passphrase()?),
    };
    let config = serde_json::to_vec_pretty(&Config { mode })?;
    write_atomic(&paths.config(), &config)?;
    save(paths, &Vault::new(), &key)
}

pub fn open(paths: &Paths) -> Result<(Vault, Key)> {
    if !paths.vault().exists() {
        bail!("no vault yet. Run `hush init` first");
    }
    let config: Config = serde_json::from_slice(
        &fs::read(paths.config()).with_context(|| format!("couldn't read {}", paths.config().display()))?,
    )
    .context("the hush config file is damaged")?;
    let key = match config.mode {
        Mode::Keychain => keychain::load_identity(paths)?,
        Mode::Passphrase => Key::Passphrase(keychain::passphrase()?),
    };
    let ciphertext = fs::read(paths.vault()).context("couldn't read the vault")?;
    let plaintext = crypto::decrypt(&key, &ciphertext)?;
    let vault = serde_json::from_slice(&plaintext).context("the vault decrypted but its contents are damaged")?;
    Ok((vault, key))
}

/// An exclusive lock on the vault, released when dropped. Take it before
/// `open` in any command that saves, so two hush processes can't read the
/// same vault and then overwrite each other's changes.
pub struct VaultLock(#[allow(dead_code)] fs::File);

pub fn lock(paths: &Paths) -> Result<VaultLock> {
    if !paths.vault().exists() {
        bail!("no vault yet. Run `hush init` first");
    }
    let path = paths.dir.join("vault.lock");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(&path).with_context(|| format!("couldn't open {}", path.display()))?;
    file.lock().context("couldn't lock the vault")?;
    Ok(VaultLock(file))
}

pub fn save(paths: &Paths, vault: &Vault, key: &Key) -> Result<()> {
    let plaintext = Zeroizing::new(serde_json::to_vec(vault)?);
    let ciphertext = crypto::encrypt(key, &plaintext)?;
    write_atomic(&paths.vault(), &ciphertext)
}

/// Writes to a temp file and renames it over the target, so a crash mid-write
/// never leaves a half-written vault behind. The temp name includes the pid so
/// two processes never write into the same temp file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = write_and_rename(&tmp, path, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(tmp).with_context(|| format!("couldn't write {}", tmp.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(tmp, path).with_context(|| format!("couldn't replace {}", path.display()))
}

fn create_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
