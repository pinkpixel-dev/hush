# hush overview

How hush works right now. For the decisions behind it, see `MEMORY.md`.

## What it is

hush is a Rust CLI that stores secrets in one encrypted vault and injects them into a child process as environment variables. Apps don't change at all. They read env vars the same way they always did.

## Layout

```text
src/
  main.rs          entry point, prints errors as `hush: ...`
  cli.rs           clap definitions (commands + the shared Target flags)
  paths.rs         vault folder: HUSH_DIR or ~/.local/share/hush
  names.rs         validation for project, environment, and secret names
  project.rs       finds the .hush marker by walking up from the cwd
  keychain.rs      age identity in the Secret Service, plus passphrase prompts
  exec.rs          builds the child command and exec()s it
  vault/
    mod.rs         Vault model (project -> env -> secrets), zeroized on drop
    crypto.rs      age encrypt/decrypt for both key types
    store.rs       create/open/save, config.json, atomic writes
  commands/        one file per command (set/get/list/rm share secrets.rs)
tests/cli.rs       end-to-end tests against passphrase vaults in temp dirs
```

## Storage

Everything lives in the vault folder (`HUSH_DIR`, or `ProjectDirs::data_dir()`, which is `~/.local/share/hush` on Linux):

| File | Contents |
|---|---|
| `vault.age` | age-encrypted JSON: `{ "version": 1, "projects": { project: { env: { KEY: value } } } }` |
| `config.json` | `{ "mode": "keychain" }` or `{ "mode": "passphrase" }`, not secret |

The folder is created with `0700` permissions. Both files are written through `write_atomic`, which writes a `.tmp` file with `0600` permissions, fsyncs it, and renames it over the original.

## Keys

- **Keychain mode:** `hush init` generates an `x25519::Identity` and stores it with `keyring::Entry::new("hush", "vault:<absolute vault dir>")`. Including the folder in the entry name keeps vaults under different `HUSH_DIR` paths from colliding. The vault is encrypted to that identity's public key.
- **Passphrase mode:** the vault uses age's scrypt recipient. The passphrase comes from `HUSH_PASSPHRASE` or a hidden `rpassword` prompt.

`keyring` 4.x is used with its default `v1` feature, which picks the Secret Service (through zbus) on Linux.

## Project resolution

`project::resolve` uses `--project` if it's given. Otherwise it walks up from the cwd looking for a `.hush` file whose content is the project name. The environment comes from `--env` and defaults to `dev`. Project and environment names allow `[A-Za-z0-9._-]`, up to 64 characters. Secret names have to be valid env var names.

## run

`commands/run.rs` opens the vault, picks the project/env secrets, and calls `exec::run`. That function adds the secrets with `Command::envs` (so vault values override inherited vars), removes `HUSH_PASSPHRASE`, and calls `exec()` on Unix. hush's process image is replaced, so the child owns signals and the exit code. On non-Unix targets it spawns the child and exits with the child's code instead.

## Memory hygiene

`Vault` zeroizes every value on drop. Decrypted plaintext and serialized JSON are wrapped in `Zeroizing<Vec<u8>>`, and prompted values in `Zeroizing<String>`. This only narrows how long secrets sit in memory. It doesn't make that time zero.

## scan

`commands/scan.rs` walks with `walkdir` (not `ignore`, which skips gitignored files, and `.env` files are almost always gitignored). It prunes a fixed list of heavy folders (`node_modules`, `.git`, `target`, and so on), matches `.env` and `.env.<suffix>` while skipping template suffixes, and prints variable counts only.

## Tests

`cargo test` runs unit tests (crypto round trips, name validation, vault pruning, env file matching) and `tests/cli.rs`. The CLI tests use `HUSH_DIR` and `HUSH_PASSPHRASE` to run real passphrase vaults headless. Each command does a full scrypt unlock, so the suite takes around 15 seconds. The keychain path isn't covered by automated tests.
