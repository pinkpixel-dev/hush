//! The unlocked vault for one TUI session. The key is kept in memory so edits
//! don't prompt again, and both are dropped (and zeroized) when the session
//! locks. `vault.lock` is only held for the length of a single edit.

use std::fs;
use std::time::SystemTime;

use age::secrecy::SecretString;
use anyhow::Result;

use crate::keychain;
use crate::paths::Paths;
use crate::vault::Vault;
use crate::vault::crypto::Key;
use crate::vault::store::{self, Mode};

/// Enough of `vault.age`'s metadata to notice another process saving it. A
/// save always renames a new file into place, so the inode changes too.
#[derive(PartialEq, Clone, Copy)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    inode: u64,
}

struct Unlocked {
    key: Key,
    vault: Vault,
    stamp: Option<Stamp>,
}

pub struct Session {
    paths: Paths,
    pub mode: Mode,
    unlocked: Option<Unlocked>,
}

impl Session {
    /// Unlocks with the normal prompt (or keychain), before the TUI takes over
    /// the terminal.
    pub fn start(paths: Paths) -> Result<Self> {
        let (mode, _) = store::describe(&paths)?;
        let stamp = stamp(&paths);
        let (vault, key) = store::open(&paths)?;
        Ok(Self { paths, mode, unlocked: Some(Unlocked { key, vault, stamp }) })
    }

    pub fn vault(&self) -> Option<&Vault> {
        self.unlocked.as_ref().map(|u| &u.vault)
    }

    /// Drops the key and the decrypted vault.
    pub fn lock(&mut self) {
        self.unlocked = None;
    }

    /// Unlocks again after an idle lock. Passphrase vaults need the passphrase.
    pub fn unlock(&mut self, passphrase: Option<SecretString>) -> Result<()> {
        let key = match (self.mode, passphrase) {
            (Mode::Keychain, _) => keychain::load_identity(&self.paths)?,
            (Mode::Passphrase, Some(passphrase)) => Key::Passphrase(passphrase),
            (Mode::Passphrase, None) => anyhow::bail!("enter the vault passphrase"),
        };
        let stamp = stamp(&self.paths);
        let vault = store::open_with(&self.paths, &key)?;
        self.unlocked = Some(Unlocked { key, vault, stamp });
        Ok(())
    }

    /// Whether another process saved the vault since it was last read.
    pub fn changed_on_disk(&self) -> bool {
        self.unlocked.as_ref().is_some_and(|u| u.stamp != stamp(&self.paths))
    }

    /// Reads the vault again with the session key.
    pub fn reload(&mut self) -> Result<()> {
        let Some(unlocked) = self.unlocked.as_mut() else { return Ok(()) };
        let stamp = stamp(&self.paths);
        unlocked.vault = store::open_with(&self.paths, &unlocked.key)?;
        unlocked.stamp = stamp;
        Ok(())
    }

    /// Applies one change the same way the CLI does: take the lock, reread the
    /// vault if someone else saved it, change it, save, release the lock.
    pub fn edit(&mut self, change: impl FnOnce(&mut Vault) -> Result<()>) -> Result<()> {
        let Some(unlocked) = self.unlocked.as_mut() else { anyhow::bail!("the vault is locked") };
        let _lock = store::lock(&self.paths)?;
        if unlocked.stamp != stamp(&self.paths) {
            unlocked.stamp = stamp(&self.paths);
            unlocked.vault = store::open_with(&self.paths, &unlocked.key)?;
        }
        change(&mut unlocked.vault)?;
        if let Err(e) = store::save(&self.paths, &unlocked.vault, &unlocked.key) {
            // Memory now has a change the disk doesn't, so force a reload.
            unlocked.stamp = None;
            return Err(e);
        }
        unlocked.stamp = stamp(&self.paths);
        Ok(())
    }
}

fn stamp(paths: &Paths) -> Option<Stamp> {
    let meta = fs::metadata(paths.vault()).ok()?;
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    Some(Stamp { modified: meta.modified().ok(), len: meta.len(), inode })
}

#[cfg(test)]
impl Session {
    /// A keychain-style session over a real vault in `dir`, without touching
    /// the keychain. Returns the key too, so tests can play another process.
    pub fn for_test(dir: &std::path::Path, vault: &Vault) -> (Self, age::x25519::Identity) {
        let paths = Paths { dir: dir.to_path_buf() };
        fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        fs::write(paths.config(), br#"{ "mode": "keychain" }"#).unwrap();
        let identity = age::x25519::Identity::generate();
        let key = Key::Identity(identity.clone());
        store::save(&paths, vault, &key).unwrap();
        let unlocked = Unlocked { vault: store::open_with(&paths, &key).unwrap(), key, stamp: stamp(&paths) };
        (Self { paths, mode: Mode::Keychain, unlocked: Some(unlocked) }, identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::ops;

    fn sample() -> Vault {
        let mut vault = Vault::new();
        ops::set_secret(&mut vault, "api", "dev", "A", "1".into()).unwrap();
        vault
    }

    /// What's actually on disk, read the way another hush process would.
    fn on_disk(dir: &std::path::Path, identity: &age::x25519::Identity) -> Vault {
        store::open_with(&Paths { dir: dir.to_path_buf() }, &Key::Identity(identity.clone())).unwrap()
    }

    #[test]
    fn edits_are_saved_to_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut session, identity) = Session::for_test(dir.path(), &sample());
        session.edit(|v| ops::set_secret(v, "api", "dev", "B", "2".into())).unwrap();
        assert_eq!(on_disk(dir.path(), &identity).secrets("api", "dev").unwrap()["B"], "2");
        assert!(!session.changed_on_disk());
    }

    #[test]
    fn an_edit_rereads_changes_saved_elsewhere_first() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut session, identity) = Session::for_test(dir.path(), &sample());
        let paths = Paths { dir: dir.path().to_path_buf() };
        let mut other = on_disk(dir.path(), &identity);
        ops::set_secret(&mut other, "api", "dev", "FROM_CLI", "x".into()).unwrap();
        store::save(&paths, &other, &Key::Identity(identity.clone())).unwrap();
        assert!(session.changed_on_disk());

        session.edit(|v| ops::set_secret(v, "api", "dev", "FROM_UI", "y".into())).unwrap();
        let saved = on_disk(dir.path(), &identity);
        let dev = saved.secrets("api", "dev").unwrap();
        assert_eq!((dev["FROM_CLI"].as_str(), dev["FROM_UI"].as_str()), ("x", "y"));
    }

    #[test]
    fn a_refused_change_saves_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut session, identity) = Session::for_test(dir.path(), &sample());
        assert!(session.edit(|v| ops::rename_secret(v, "api", "dev", "A", "bad-name")).is_err());
        assert!(on_disk(dir.path(), &identity).secrets("api", "dev").unwrap().contains_key("A"));
    }

    #[test]
    fn reload_and_lock() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut session, identity) = Session::for_test(dir.path(), &sample());
        let mut other = on_disk(dir.path(), &identity);
        ops::delete_secret(&mut other, "api", "dev", "A").unwrap();
        store::save(&Paths { dir: dir.path().to_path_buf() }, &other, &Key::Identity(identity)).unwrap();

        session.reload().unwrap();
        assert!(session.vault().unwrap().projects.is_empty());
        assert!(!session.changed_on_disk());
        session.lock();
        assert!(session.vault().is_none());
        assert!(session.edit(|_| Ok(())).is_err());
    }
}
