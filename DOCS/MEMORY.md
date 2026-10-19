# Decisions

## 2026-10-19: Local-first Rust CLI, no server

**Decided:** hush is a single Rust binary with a local encrypted vault. There are no accounts, no sync, and no server.
**Why:** The point is getting secrets out of plaintext `.env` files without adding a service to run or trust. Rust gives a single fast binary.
**Rejected:** Go with a Charm TUI (also a good fit, but the preference was Rust). Node/TypeScript (native keychain bindings are messier, and users would need Node installed). Infisical-style server sync (Infisical is MIT and self-hostable, but it needs a server, which is exactly what hush avoids).

## 2026-10-19: age for encryption, keychain for the key

**Decided:** The vault is one age-encrypted JSON file. The key is an age x25519 identity in the OS keychain through `keyring` 4.x, with an age scrypt passphrase mode as the fallback.
**Why:** age is a well-reviewed format, so there's no hand-rolled crypto. The keychain unlocks with the login session. Passphrase mode covers SSH, WSL, and CI where there's no Secret Service.
**Rejected:** Storing the key in a file next to the vault (that just moves the plaintext problem). Raw ChaCha20-Poly1305 with a custom format (more to get wrong for no benefit).

## 2026-10-19: `.hush` marker file for project lookup

**Decided:** A folder is linked to a project by a `.hush` file holding just the project name. hush walks up from the cwd to find it.
**Why:** It survives moving or re-cloning the folder, and it's safe to commit.
**Rejected:** A path-to-project map in the global config, which breaks whenever a folder moves.

## 2026-10-19: Linux first, dev/prod environments in v1

**Decided:** v1 targets Linux. Environments are free-form names that default to `dev`.
**Why:** That's the main machine hush gets used on. Environments were wanted from the start, and making them free-form costs nothing extra.

## 2026-10-19: Values never come from arguments

**Decided:** `hush set` reads from a hidden prompt or stdin only.
**Why:** Arguments end up in shell history and are visible in `ps`.

## 2026-10-19: Vault values override inherited env vars in `run`

**Decided:** If a variable is already set, the vault value wins for the child process.
**Why:** It's predictable. You get what's in the vault. This is the opposite of dotenv's usual "existing env wins" behavior, so the README calls it out.
