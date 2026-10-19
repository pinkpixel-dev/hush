//! End-to-end tests against a real passphrase vault in a temp directory.
//! Keychain vaults are covered by the opt-in tests in `keychain.rs`.

mod common;

use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use common::{Sandbox, WRONG_RECOVERY_KEY};
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn set_get_list_and_rm() {
    let s = Sandbox::new();
    s.set(&["set", "OPENAI_API_KEY"], "sk-test-123\n");

    s.hush(&["get", "OPENAI_API_KEY"]).assert().success().stdout("sk-test-123\n");
    s.hush(&["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OPENAI_API_KEY").and(predicate::str::contains("sk-test").not()));

    let vault = fs::read(s.data.path().join("vault.age")).unwrap();
    assert!(!vault.windows(11).any(|w| w == b"sk-test-123"), "vault holds plaintext");

    s.hush(&["rm", "OPENAI_API_KEY"]).assert().success();
    s.hush(&["get", "OPENAI_API_KEY"]).assert().failure().stderr(predicate::str::contains("isn't set"));
}

#[test]
fn concurrent_sets_keep_every_secret() {
    let s = Sandbox::new();
    let keys = ["KEY_A", "KEY_B", "KEY_C", "KEY_D"];
    std::thread::scope(|scope| {
        for key in keys {
            let s = &s;
            scope.spawn(move || s.set(&["set", key], "value"));
        }
    });

    let list = s.hush(&["list"]).assert().success();
    let stdout = String::from_utf8(list.get_output().stdout.clone()).unwrap();
    for key in keys {
        assert!(stdout.contains(key), "{key} was lost to a concurrent write");
    }
}

#[test]
fn run_injects_the_right_environment() {
    let s = Sandbox::new();
    s.set(&["set", "DB_URL"], "postgres://dev");
    s.set(&["set", "DB_URL", "--env", "prod"], "postgres://prod");

    let print = ["sh", "-c", "printf %s \"$DB_URL\"; test -z \"$HUSH_PASSPHRASE\""];
    s.hush(&[&["run", "--"][..], &print].concat()).assert().success().stdout("postgres://dev");
    s.hush(&[&["run", "--env", "prod", "--"][..], &print].concat())
        .assert()
        .success()
        .stdout("postgres://prod");
}

#[test]
fn run_passes_through_the_exit_code() {
    let s = Sandbox::new();
    s.hush(&["run", "--", "sh", "-c", "exit 7"]).assert().code(7);
}

#[test]
fn import_reads_a_dotenv_file() {
    let s = Sandbox::new();
    let file = s.project.path().join(".env");
    fs::write(&file, "# comment\nAPI_KEY=abc\nQUOTED=\"hello world\"\n").unwrap();

    s.hush(&["import", ".env"]).assert().success().stderr(predicate::str::contains("2 new"));
    s.hush(&["get", "QUOTED"]).assert().success().stdout("hello world\n");
    assert!(file.exists(), "import must not delete the original file");
}

#[test]
fn import_keeps_dollar_signs_as_written() {
    let s = Sandbox::new();
    fs::write(s.project.path().join(".env"), "PW=abc$HOME\nURL=\"${PW}/api\"\nPLAIN=1\n").unwrap();

    s.hush(&["import", ".env"])
        .env("HOME", "/should/not/appear")
        .assert()
        .success()
        .stderr(predicate::str::contains("Kept `$` as written in PW, URL."));
    s.hush(&["get", "PW"]).assert().success().stdout("abc$HOME\n");
    s.hush(&["get", "URL"]).assert().success().stdout("${PW}/api\n");
}

#[test]
fn import_rejects_a_bad_line_without_saving_anything() {
    let s = Sandbox::new();
    fs::write(s.project.path().join(".env"), "A=1\nB=two words\n").unwrap();

    s.hush(&["import", ".env"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("line 2").and(predicate::str::contains("two words").not()));
    s.hush(&["list"]).assert().success().stdout("");
}

#[test]
fn wrong_passphrase_is_rejected() {
    let s = Sandbox::new();
    s.hush(&["list"])
        .env("HUSH_PASSPHRASE", "nope")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wrong passphrase"));
}

#[test]
fn unlinked_folder_needs_a_project() {
    let s = Sandbox::new();
    let elsewhere = TempDir::new().unwrap();
    s.hush(&["list"])
        .current_dir(elsewhere.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("isn't linked"));
    s.hush(&["list", "--project", "my-app"]).current_dir(elsewhere.path()).assert().success();
}

#[test]
fn scan_finds_env_files_and_skips_noise() {
    let root = TempDir::new().unwrap();
    let app = root.path().join("app");
    fs::create_dir_all(app.join("node_modules/pkg")).unwrap();
    fs::write(app.join(".env"), "A=1\nB=2\n").unwrap();
    fs::write(app.join(".env.local"), "C=3\n").unwrap();
    fs::write(app.join(".env.example"), "A=\n").unwrap();
    fs::write(app.join("node_modules/pkg/.env"), "X=1\n").unwrap();

    cargo_bin_cmd!("hush")
        .args(["scan", root.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(".env  (2 variables)")
                .and(predicate::str::contains(".env.local  (1 variable)"))
                .and(predicate::str::contains("example").not())
                .and(predicate::str::contains("node_modules").not()),
        );
}

#[test]
fn passphrase_vaults_reject_recovery_keys() {
    let s = Sandbox::new();
    s.hush(&["recovery-key"]).assert().failure().stderr(predicate::str::contains("don't use recovery keys"));
    s.hush(&["recover"])
        .write_stdin(WRONG_RECOVERY_KEY)
        .assert()
        .failure()
        .stderr(predicate::str::contains("don't use recovery keys"));
}

#[test]
fn recover_from_never_overwrites_an_existing_vault() {
    let s = Sandbox::new();
    let before = fs::read(s.data.path().join("vault.age")).unwrap();
    s.hush(&["recover", "--from", "elsewhere.age"])
        .write_stdin(WRONG_RECOVERY_KEY)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    assert_eq!(fs::read(s.data.path().join("vault.age")).unwrap(), before);
}

#[test]
fn recover_with_the_wrong_key_writes_nothing() {
    let s = Sandbox::new();
    let empty = TempDir::new().unwrap();
    let copy = s.data.path().join("vault.age");
    s.hush(&["recover", "--from", copy.to_str().unwrap()])
        .env("HUSH_DIR", empty.path())
        .write_stdin(WRONG_RECOVERY_KEY)
        .assert()
        .failure()
        .stderr(predicate::str::contains("doesn't unlock"));
    assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0, "a failed recover left files behind");

    s.hush(&["recover", "--from", copy.to_str().unwrap()])
        .env("HUSH_DIR", empty.path())
        .write_stdin("not a key")
        .assert()
        .failure()
        .stderr(predicate::str::contains("isn't a valid recovery key"));
}

#[test]
fn completions_work_without_a_vault() {
    for shell in ["bash", "zsh", "fish"] {
        cargo_bin_cmd!("hush")
            .args(["completions", shell])
            .env("HUSH_DIR", "/nonexistent/hush")
            .assert()
            .success()
            .stdout(predicate::str::contains("recovery-key").and(predicate::str::contains("import")));
    }
}

#[test]
fn export_round_trips_through_import() {
    let s = Sandbox::new();
    let values = [("PLAIN", "sk-123"), ("SPACES", "a b # c"), ("DOLLAR", "$HOME 'x' \"y\""), ("MULTI", "one\ntwo")];
    for (key, value) in values {
        s.set(&["set", key], value);
    }

    let exported = s.hush(&["export"]).assert().success();
    fs::write(s.project.path().join("out.env"), &exported.get_output().stdout).unwrap();
    s.hush(&["import", "out.env", "--env", "copy"]).assert().success();
    for (key, value) in values {
        s.hush(&["get", key, "--env", "copy"]).assert().success().stdout(format!("{value}\n"));
    }

    s.hush(&["export", "--env", "empty"]).assert().failure().stderr(predicate::str::contains("no secrets"));
}
