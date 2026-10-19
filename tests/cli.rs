//! End-to-end tests against a real passphrase vault in a temp directory.
//! The keychain path can't run headless, so it's covered by manual testing.

use std::fs;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

const PASSPHRASE: &str = "correct horse battery staple";

struct Sandbox {
    data: TempDir,
    project: TempDir,
}

impl Sandbox {
    /// A fresh passphrase vault plus a project folder linked to `my-app`.
    fn new() -> Self {
        let sandbox = Self { data: TempDir::new().unwrap(), project: TempDir::new().unwrap() };
        sandbox.hush(&["init", "--passphrase"]).assert().success();
        sandbox.hush(&["link", "my-app"]).assert().success();
        sandbox
    }

    fn hush(&self, args: &[&str]) -> Command {
        let mut cmd = cargo_bin_cmd!("hush");
        cmd.args(args)
            .current_dir(self.project.path())
            .env("HUSH_DIR", self.data.path())
            .env("HUSH_PASSPHRASE", PASSPHRASE);
        cmd
    }

    fn set(&self, args: &[&str], value: &str) {
        self.hush(args).write_stdin(value).assert().success();
    }
}

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
