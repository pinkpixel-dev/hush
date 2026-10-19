# Changelog

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
