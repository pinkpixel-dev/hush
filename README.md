# hush

hush is a small CLI that replaces `.env` files. Your secrets live in one encrypted vault on your machine, and `hush run` hands them to your app as environment variables when it starts.

I made it because I had API keys sitting in plaintext `.env` files across dozens of project folders. They end up in backups and synced folders, and these days AI coding agents happily read them while poking around a repo. I wanted those keys in one encrypted place instead, without changing any app code.

It's local-first and free. There are no accounts or servers, and it doesn't sync anything. It's just a binary and a file.

## How it works

```text
my-app/
  .hush        <- just the project name, safe to commit
  (no .env)

hush run -- npm run dev
  1. finds .hush and reads the project name
  2. unlocks the vault with the key from your system keychain
  3. decrypts that project's secrets in memory
  4. replaces itself with `npm run dev`, secrets already in the environment
```

Your app keeps reading `process.env.OPENAI_API_KEY` (or `os.environ`, or `std::env::var`) exactly like before. It has no idea the `.env` file is gone.

The vault is a single [age](https://age-encryption.org)-encrypted file. Its key is an age identity stored in your system keychain (GNOME Keyring or KWallet through the Secret Service), so it unlocks with your login session.

## Install

Right now you install hush from source, which needs a Rust toolchain:

```bash
git clone <repo-url> hush
cd hush
cargo install --path .
```

hush is Linux-first. I've only tested it on Linux so far. The keychain code should work on macOS and Windows too, but I haven't tried it there yet.

## Quick start

```bash
hush init                        # create the vault, key goes in your keychain
cd ~/projects/my-app
hush link my-app                 # writes a .hush file in this folder
hush set OPENAI_API_KEY          # hidden prompt for the value
hush run -- npm run dev          # start the app with secrets injected
```

Already have a `.env`? Import it, check that everything works, then delete the file yourself:

```bash
hush import .env
hush run -- npm run dev
rm .env
```

hush never deletes your files for you.

## Commands

| Command | What it does |
|---|---|
| `hush init` | Create a vault. Add `--passphrase` to use a passphrase instead of the keychain |
| `hush link <name>` | Link the current folder to a project. `--force` changes an existing link |
| `hush set <KEY>` | Add or update a secret. The value comes from a hidden prompt or stdin |
| `hush get <KEY>` | Print one value |
| `hush list` | List secret names for the current project. `--all` shows every project |
| `hush rm <KEY>` | Remove a secret |
| `hush run -- <cmd>` | Run a command with the secrets in its environment |
| `hush import <file>` | Copy every variable from a `.env` file into the vault |
| `hush scan [dir]` | Find `.env` files on disk (defaults to your home folder) |

Values are never accepted as command arguments, so they don't end up in your shell history. To set one from a script, pipe it in:

```bash
printf %s "$TOKEN" | hush set GITHUB_TOKEN
```

`list` and `scan` only ever show names and counts, never values.

### Projects and environments

Every command that touches secrets works on a project and an environment. The project comes from the nearest `.hush` file (hush walks up from the current folder), and the environment defaults to `dev`. You can override either one:

```bash
hush set DATABASE_URL --env prod
hush run --env prod -- ./deploy.sh
hush list --project blog
```

Environment names are free-form, so `staging` or `test` work too.

### Finding old .env files

```bash
hush scan ~/projects
```

This prints every `.env` and `.env.*` file it finds, along with how many variables each one holds. It skips templates like `.env.example` and folders like `node_modules`, `.git`, and `target`.

## Passphrase mode

If there's no keychain available (SSH sessions, WSL, CI, a minimal window manager without a Secret Service), create a passphrase vault instead:

```bash
hush init --passphrase
```

hush asks for the passphrase each time it opens the vault. For scripts, you can set `HUSH_PASSPHRASE`. hush strips it from the environment before starting your command, so your app never sees it.

## Environment variables

| Variable | What it does |
|---|---|
| `HUSH_DIR` | Use a different vault folder. The default is `~/.local/share/hush` |
| `HUSH_PASSPHRASE` | Passphrase for a passphrase vault, so you don't get prompted |

## What it protects (and what it doesn't)

hush keeps secrets out of plaintext files. That covers the usual ways `.env` files leak: getting committed to git, landing in backups or synced folders, showing up in a screen share, or getting read by an AI agent browsing your project.

It doesn't protect you from malware running as your own user while you're logged in. That code could ask the keychain for the key the same way hush does. Once your app is running, its environment is also readable by your user (through `/proc/<pid>/environ` on Linux). So it's a big step up from `.env` files, but it isn't a sandbox.

Two other behaviors are worth knowing:

- If a variable is already set in your shell, the vault value wins for the command `hush run` starts.
- Losing the keychain entry means losing the vault. For now there's no backup or export command, so a passphrase vault is the safer choice if that worries you.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

The tests run every command end to end against real passphrase vaults in temp folders. The keychain path can't run headless, so I test it by hand.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)
