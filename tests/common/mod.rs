//! Shared helpers for the end-to-end tests. Each test binary uses a different
//! subset, hence the dead_code allowance.
#![allow(dead_code)]

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::TempDir;

pub const PASSPHRASE: &str = "correct horse battery staple";
/// A valid age identity that doesn't unlock any test vault.
pub const WRONG_RECOVERY_KEY: &str = "AGE-SECRET-KEY-1GQ9778VQXMMJVE8SK7J6VT8UJ4HDQAJUVSFCWCM02D8GEWQ72PVQ2Y5J33";

pub struct Sandbox {
    pub data: TempDir,
    pub project: TempDir,
}

impl Sandbox {
    /// A fresh passphrase vault plus a project folder linked to `my-app`.
    pub fn new() -> Self {
        let sandbox = Self { data: TempDir::new().unwrap(), project: TempDir::new().unwrap() };
        sandbox.hush(&["init", "--passphrase"]).assert().success();
        sandbox.hush(&["link", "my-app"]).assert().success();
        sandbox
    }

    pub fn hush(&self, args: &[&str]) -> Command {
        let mut cmd = cargo_bin_cmd!("hush");
        cmd.args(args)
            .current_dir(self.project.path())
            .env("HUSH_DIR", self.data.path())
            .env("HUSH_PASSPHRASE", PASSPHRASE);
        cmd
    }

    pub fn set(&self, args: &[&str], value: &str) {
        self.hush(args).write_stdin(value).assert().success();
    }
}

/// stdout of a finished command as a string.
pub fn stdout(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

/// A vault file encrypted only to `recovery`, holding `app/dev/A=1`, the way a
/// keychain vault with a recovery key looks to someone without the keychain.
pub fn recovery_only_vault(recovery: &age::x25519::Identity) -> Vec<u8> {
    let json = br#"{"version":1,"projects":{"app":{"dev":{"A":"1"}}}}"#;
    age::encrypt(&recovery.to_public(), json).unwrap()
}
