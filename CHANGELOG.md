# Changelog

## Unreleased

### 🩺 Commands

- New `hush export` command that prints a project's secrets in `.env` format on stdout, for tools that only read a file. hush never writes the file itself. Values are quoted so `hush import` reads them back exactly, including spaces, quotes, `$`, and line breaks
- New `hush doctor` command. It checks the vault folder, its files, the config, and whether your key opens the vault, then prints one line per check. It's read-only and exits with an error when something's actually broken

### 🔒 Hardening

- hush now refuses to use a vault folder that other users can read or write, and tells you the `chmod 700` command that fixes it
- hush refuses symlinks for the vault folder, `vault.age`, `config.json`, and `vault.lock`, so it can't be pointed at files somewhere else
- Temp files are created fresh every time and never follow a symlink, and the folder is synced after each rename so a save survives a power cut

### 🐛 Fixes

- Running `hush init` twice at the same time could create two vaults, with the second one silently replacing the first. `init` now takes the vault lock, so the second run fails with "a vault already exists"
- `hush recover --from` now takes the vault lock too

## 0.3.0 - October 19, 2026

### ⌨️ Commands

- New `hush completions <shell>` command that prints a completion script for bash, zsh, fish, elvish, or PowerShell. It works before `hush init` too, since it never touches the vault

## 0.2.0 - October 19, 2026

### 🔑 Recovery

- New `hush recovery-key` command. It prints a recovery key once, and from then on every save also encrypts the vault to that key
- New `hush recover` command. If your keychain entry is lost, it unlocks the vault with the recovery key and stores a fresh key in your keychain
- `hush recover --from <file>` rebuilds the vault folder from a copy of `vault.age`, for a new machine or a wiped folder. It won't overwrite an existing vault
- `hush init` now suggests making a recovery key, and the "key is missing from your keychain" error points to `hush recover`

### 🐛 Fixes

- Running `hush set`, `hush rm`, or `hush import` at the same time no longer loses changes. Writers now take a lock on the vault, so the second one waits for the first to finish
- Each write uses its own temp file, so two overlapping writes can't mix their bytes together and leave a vault that won't decrypt

## 0.1.0 - October 19, 2026

First version.

### 🔐 Vault

- One age-encrypted vault file at `~/.local/share/hush/vault.age`, written atomically with `0600` permissions
- The vault key is stored in the system keychain through the Secret Service, or you can use a passphrase vault with `hush init --passphrase`
- Secrets are grouped by project and environment, with `dev` as the default environment

### ⌨️ Commands

- `init`, `link`, `set`, `get`, `list`, `rm`, `run`, `import`, and `scan`
- `run` replaces itself with your command, so signals and exit codes pass straight through
- `set` reads values from a hidden prompt or stdin, never from arguments
- `scan` finds `.env` files on disk without printing any values
