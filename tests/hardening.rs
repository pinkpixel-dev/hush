//! Hardening tests: permissions, symlinks, concurrent processes, failure paths
//! during recovery, and `hush doctor`.

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use age::secrecy::ExposeSecret;
use assert_cmd::cargo::cargo_bin_cmd;
use common::{PASSPHRASE, Sandbox, recovery_only_vault, stdout};
use predicates::prelude::*;
use tempfile::TempDir;

/// A session bus address that goes nowhere, so the keychain is unreachable.
const NO_KEYCHAIN: &str = "unix:path=/nonexistent/hush-test-bus";

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn leftover_temp_files(dir: &std::path::Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

#[test]
fn vault_folder_and_files_are_owner_only() {
    let s = Sandbox::new();
    s.set(&["set", "A"], "1");
    s.set(&["rm", "A"], "");
    let dir = s.data.path();
    assert_eq!(mode(dir), 0o700);
    for file in ["vault.age", "config.json", "vault.lock"] {
        assert_eq!(mode(&dir.join(file)), 0o600, "{file}");
    }
    assert!(leftover_temp_files(dir).is_empty(), "writes left temp files behind");
}

#[test]
fn loose_folder_permissions_are_refused() {
    let s = Sandbox::new();
    fs::set_permissions(s.data.path(), fs::Permissions::from_mode(0o755)).unwrap();
    s.hush(&["list"]).assert().failure().stderr(predicate::str::contains("chmod 700"));
    s.hush(&["set", "A"]).write_stdin("1").assert().failure().stderr(predicate::str::contains("chmod 700"));

    fs::set_permissions(s.data.path(), fs::Permissions::from_mode(0o700)).unwrap();
    s.hush(&["list"]).assert().success();
}

#[test]
fn symlinked_vault_folder_is_refused() {
    let s = Sandbox::new();
    let links = TempDir::new().unwrap();
    let link = links.path().join("hush");
    symlink(s.data.path(), &link).unwrap();
    s.hush(&["list"])
        .env("HUSH_DIR", &link)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is a symlink"));

    // init through a symlink must not create anything at the target either.
    let target = TempDir::new().unwrap();
    let fresh = links.path().join("fresh");
    symlink(target.path(), &fresh).unwrap();
    s.hush(&["init", "--passphrase"])
        .env("HUSH_DIR", &fresh)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is a symlink"));
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn symlinked_vault_files_are_refused() {
    for file in ["vault.age", "config.json"] {
        let s = Sandbox::new();
        let real = TempDir::new().unwrap();
        let moved = real.path().join(file);
        fs::rename(s.data.path().join(file), &moved).unwrap();
        symlink(&moved, s.data.path().join(file)).unwrap();

        s.hush(&["list"]).assert().failure().stderr(predicate::str::contains("is a symlink"));
        s.hush(&["set", "A"]).write_stdin("1").assert().failure().stderr(predicate::str::contains("is a symlink"));
    }
}

#[test]
fn symlinked_lock_file_is_refused_and_its_target_untouched() {
    let s = Sandbox::new();
    let elsewhere = TempDir::new().unwrap();
    let victim = elsewhere.path().join("victim");
    fs::write(&victim, "untouched").unwrap();
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
    let lock = s.data.path().join("vault.lock");
    let _ = fs::remove_file(&lock);
    symlink(&victim, &lock).unwrap();

    s.hush(&["set", "A"]).write_stdin("1").assert().failure().stderr(predicate::str::contains("is a symlink"));
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
    assert_eq!(mode(&victim), 0o644);
}

#[test]
fn mixed_concurrent_writers_keep_every_change() {
    let s = Sandbox::new();
    let doomed = ["OLD_0", "OLD_1", "OLD_2"];
    for key in doomed {
        s.set(&["set", key], "x");
    }
    let env_file = s.project.path().join("extra.env");
    fs::write(&env_file, "IMPORTED_A=a\nIMPORTED_B=b\n").unwrap();
    let added = ["NEW_0", "NEW_1", "NEW_2", "NEW_3"];

    std::thread::scope(|scope| {
        let s = &s;
        for key in added {
            scope.spawn(move || s.set(&["set", key], "value"));
        }
        for key in doomed {
            scope.spawn(move || s.hush(&["rm", key]).assert().success());
        }
        scope.spawn(move || s.hush(&["import", "extra.env"]).assert().success());
    });

    let listed = stdout(&s.hush(&["list"]).assert().success());
    let mut names: Vec<&str> = listed.split_whitespace().filter(|w| w.contains('_')).collect();
    names.sort();
    assert_eq!(names, ["IMPORTED_A", "IMPORTED_B", "NEW_0", "NEW_1", "NEW_2", "NEW_3"], "{listed}");
    assert!(leftover_temp_files(s.data.path()).is_empty());
}

#[test]
fn readers_never_see_a_partial_vault() {
    let s = Sandbox::new();
    s.set(&["set", "STABLE"], "still-here");
    let writing = std::sync::atomic::AtomicBool::new(true);

    std::thread::scope(|scope| {
        let (s, writing) = (&s, &writing);
        scope.spawn(move || {
            for i in 0..5 {
                s.set(&["set", &format!("KEY_{i}")], "value");
            }
            writing.store(false, std::sync::atomic::Ordering::SeqCst);
        });
        for _ in 0..2 {
            scope.spawn(move || {
                while writing.load(std::sync::atomic::Ordering::SeqCst) {
                    s.hush(&["get", "STABLE"]).assert().success().stdout("still-here\n");
                }
            });
        }
    });
}

#[test]
fn concurrent_inits_create_exactly_one_vault() {
    let data = TempDir::new().unwrap();
    let passphrases = ["first passphrase", "second passphrase"];
    let results: Vec<bool> = std::thread::scope(|scope| {
        let handles: Vec<_> = passphrases
            .iter()
            .map(|p| {
                let dir = data.path();
                scope.spawn(move || {
                    cargo_bin_cmd!("hush")
                        .args(["init", "--passphrase"])
                        .env("HUSH_DIR", dir)
                        .env("HUSH_PASSPHRASE", p)
                        .output()
                        .unwrap()
                        .status
                        .success()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|ok| **ok).count(), 1, "{results:?}");

    let winner = passphrases[results.iter().position(|ok| *ok).unwrap()];
    cargo_bin_cmd!("hush")
        .args(["list", "--all"])
        .env("HUSH_DIR", data.path())
        .env("HUSH_PASSPHRASE", winner)
        .assert()
        .success();
}

#[test]
fn init_refuses_a_short_passphrase() {
    let data = TempDir::new().unwrap();
    cargo_bin_cmd!("hush")
        .args(["init", "--passphrase"])
        .env("HUSH_DIR", data.path())
        .env("HUSH_PASSPHRASE", "eleven-char")
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least 12 characters"));
    assert!(!data.path().join("vault.age").exists());
}

#[test]
fn init_without_a_keychain_points_to_passphrase_mode() {
    let data = TempDir::new().unwrap();
    let hush = |args: &[&str]| {
        let mut cmd = cargo_bin_cmd!("hush");
        cmd.args(args)
            .env("HUSH_DIR", data.path())
            .env("HUSH_PASSPHRASE", PASSPHRASE)
            .env("DBUS_SESSION_BUS_ADDRESS", NO_KEYCHAIN);
        cmd
    };
    hush(&["init"]).assert().failure().stderr(predicate::str::contains("hush init --passphrase"));
    assert!(!data.path().join("vault.age").exists());
    assert!(!data.path().join("config.json").exists());

    hush(&["init", "--passphrase"]).assert().success();
    hush(&["list", "--all"]).assert().success();
}

#[test]
fn recover_from_a_damaged_copy_writes_nothing() {
    let recovery = age::x25519::Identity::generate();
    let source = TempDir::new().unwrap();
    let good = recovery_only_vault(&recovery);
    let key = recovery.to_string();

    for (name, bytes) in [("truncated.age", &good[..good.len() / 2]), ("garbage.age", &b"not age at all"[..])] {
        let copy = source.path().join(name);
        fs::write(&copy, bytes).unwrap();
        let data = TempDir::new().unwrap();
        cargo_bin_cmd!("hush")
            .args(["recover", "--from", copy.to_str().unwrap()])
            .env("HUSH_DIR", data.path())
            .write_stdin(key.expose_secret())
            .assert()
            .failure()
            .stderr(predicate::str::contains("doesn't unlock"));
        assert_eq!(fs::read_dir(data.path()).unwrap().count(), 0, "{name} left files behind");
    }
}

#[test]
fn recover_without_a_keychain_leaves_no_vault_and_can_be_retried() {
    let recovery = age::x25519::Identity::generate();
    let source = TempDir::new().unwrap();
    let copy = source.path().join("vault.age");
    fs::write(&copy, recovery_only_vault(&recovery)).unwrap();
    let before = fs::read(&copy).unwrap();
    let data = TempDir::new().unwrap();
    let key = recovery.to_string();

    for _ in 0..2 {
        cargo_bin_cmd!("hush")
            .args(["recover", "--from", copy.to_str().unwrap()])
            .env("HUSH_DIR", data.path())
            .env("DBUS_SESSION_BUS_ADDRESS", NO_KEYCHAIN)
            .write_stdin(key.expose_secret())
            .assert()
            .failure()
            .stderr(predicate::str::contains("couldn't reach your system keychain"));
        assert!(!data.path().join("vault.age").exists());
    }
    assert_eq!(fs::read(&copy).unwrap(), before, "the source copy changed");
}

#[test]
fn doctor_reports_a_healthy_vault() {
    let s = Sandbox::new();
    let out = stdout(&s.hush(&["doctor"]).assert().success());
    for expected in ["✓ folder", "✓ vault.age", "✓ config.json", "✓ config      passphrase mode", "✓ unlock", "my-app"] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }
}

#[test]
fn doctor_flags_problems() {
    let s = Sandbox::new();
    fs::write(s.data.path().join("vault.4242.tmp"), "stale").unwrap();
    let out = stdout(&s.hush(&["doctor"]).assert().success());
    assert!(out.contains("! leftovers") && out.contains("vault.4242.tmp"), "{out}");

    fs::set_permissions(s.data.path(), fs::Permissions::from_mode(0o750)).unwrap();
    let failed = s.hush(&["doctor"]).assert().failure().stderr(predicate::str::contains("found 1 problem"));
    let out = stdout(&failed);
    assert!(out.contains("✗ folder") && out.contains("chmod 700") && out.contains("· unlock"), "{out}");

    s.hush(&["doctor"])
        .env("HUSH_DIR", s.data.path().join("missing"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("hush init"));
}

#[test]
fn doctor_reports_a_wrong_passphrase() {
    let s = Sandbox::new();
    s.hush(&["doctor"])
        .env("HUSH_PASSPHRASE", "nope")
        .assert()
        .failure()
        .stdout(predicate::str::contains("✗ unlock").and(predicate::str::contains("Wrong passphrase")));
}
