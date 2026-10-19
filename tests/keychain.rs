//! Keychain vault tests. They need a running Secret Service (GNOME Keyring or
//! KWallet), so they're opt-in:
//!
//!     cargo test --test keychain -- --ignored
//!
//! Each test uses an entry named after its own temp folder and deletes it when
//! it finishes, so your real hush vault is never touched.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use common::stdout;
use predicates::prelude::*;
use tempfile::TempDir;

/// A keychain vault in a temp folder. Dropping it removes the keychain entry.
struct KeychainVault {
    data: TempDir,
}

impl KeychainVault {
    fn new() -> Self {
        let vault = Self { data: TempDir::new().unwrap() };
        vault.hush(&["init"]).assert().success();
        vault
    }

    fn hush(&self, args: &[&str]) -> Command {
        hush_in(self.data.path(), args)
    }

    fn entry(&self) -> keyring::Entry {
        entry_for(self.data.path())
    }

    /// Makes a recovery key, adds a secret, then loses the keychain entry.
    fn with_lost_key(&self) -> String {
        let key = stdout(&self.hush(&["recovery-key"]).assert().success()).trim().to_string();
        self.hush(&["set", "API_KEY", "-p", "app"]).write_stdin("secret").assert().success();
        self.entry().delete_credential().unwrap();
        self.hush(&["get", "API_KEY", "-p", "app"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("missing from your keychain"));
        key
    }
}

impl Drop for KeychainVault {
    fn drop(&mut self) {
        let _ = self.entry().delete_credential();
    }
}

fn hush_in(dir: &std::path::Path, args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("hush");
    cmd.args(args).env("HUSH_DIR", dir).env_remove("HUSH_PASSPHRASE");
    cmd
}

fn entry_for(dir: &std::path::Path) -> keyring::Entry {
    keyring::Entry::new("hush", &format!("vault:{}", dir.display())).unwrap()
}

#[test]
#[ignore = "needs a running Secret Service"]
fn recover_in_place_after_losing_the_keychain_entry() {
    let v = KeychainVault::new();
    let key = v.with_lost_key();

    v.hush(&["recover"]).write_stdin(key.as_str()).assert().success();
    v.hush(&["get", "API_KEY", "-p", "app"]).assert().success().stdout("secret\n");
    v.hush(&["doctor"]).assert().success().stdout(predicate::str::contains("recovery key set"));
}

#[test]
#[ignore = "needs a running Secret Service"]
fn recover_from_a_copy_on_a_fresh_machine() {
    let v = KeychainVault::new();
    let key = v.with_lost_key();
    let copy = v.data.path().join("vault.age");
    let fresh = TempDir::new().unwrap();
    let _cleanup = scopeguard(fresh.path());

    hush_in(fresh.path(), &["recover", "--from", copy.to_str().unwrap()])
        .write_stdin(key.as_str())
        .assert()
        .success();
    hush_in(fresh.path(), &["get", "API_KEY", "-p", "app"]).assert().success().stdout("secret\n");
}

#[test]
#[ignore = "needs a running Secret Service"]
fn interrupted_recovery_can_be_run_again() {
    let v = KeychainVault::new();
    let key = v.with_lost_key();
    let before = fs::read(v.data.path().join("vault.age")).unwrap();

    // A read-only folder lets the keychain write succeed and the config write fail,
    // so the keychain now holds a key the vault was never encrypted to.
    fs::set_permissions(v.data.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let failed = v.hush(&["recover"]).write_stdin(key.as_str()).assert().failure();
    fs::set_permissions(v.data.path(), fs::Permissions::from_mode(0o700)).unwrap();
    failed.stderr(predicate::str::contains("couldn't write"));
    assert_eq!(fs::read(v.data.path().join("vault.age")).unwrap(), before, "a failed recover changed the vault");

    v.hush(&["get", "API_KEY", "-p", "app"]).assert().failure();
    v.hush(&["recover"]).write_stdin(key.as_str()).assert().success();
    v.hush(&["get", "API_KEY", "-p", "app"]).assert().success().stdout("secret\n");
}

#[test]
#[ignore = "needs a running Secret Service"]
fn replacing_the_recovery_key_needs_the_flag_and_retires_the_old_one() {
    let v = KeychainVault::new();
    let old = stdout(&v.hush(&["recovery-key"]).assert().success()).trim().to_string();
    v.hush(&["recovery-key"]).assert().failure().stderr(predicate::str::contains("--replace"));

    let new = stdout(&v.hush(&["recovery-key", "--replace"]).assert().success()).trim().to_string();
    assert_ne!(old, new);
    let copy = v.data.path().join("vault.age");
    let fresh = TempDir::new().unwrap();
    let _cleanup = scopeguard(fresh.path());
    hush_in(fresh.path(), &["recover", "--from", copy.to_str().unwrap()])
        .write_stdin(old.as_str())
        .assert()
        .failure()
        .stderr(predicate::str::contains("doesn't unlock"));
    hush_in(fresh.path(), &["recover", "--from", copy.to_str().unwrap()])
        .write_stdin(new.as_str())
        .assert()
        .success();
}

#[test]
#[ignore = "needs a running Secret Service"]
fn concurrent_keychain_writers_keep_every_secret() {
    let v = KeychainVault::new();
    v.hush(&["recovery-key"]).assert().success();
    let keys = ["K_0", "K_1", "K_2", "K_3", "K_4", "K_5"];
    std::thread::scope(|scope| {
        for key in keys {
            let v = &v;
            scope.spawn(move || v.hush(&["set", key, "-p", "app"]).write_stdin("v").assert().success());
        }
    });
    let listed = stdout(&v.hush(&["list", "-p", "app"]).assert().success());
    for key in keys {
        assert!(listed.contains(key), "{key} lost:\n{listed}");
    }
}

/// Deletes the keychain entry for a second vault folder when the test ends.
fn scopeguard(dir: &std::path::Path) -> impl Drop + use<> {
    struct Guard(keyring::Entry);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.delete_credential();
        }
    }
    Guard(entry_for(dir))
}
