<p align="center">
  <img src="https://raw.githubusercontent.com/pinkpixel-dev/hush/main/icon.png" alt="hush icon" width="300" height="300">
</p>

# hush

hush is a CLI that replaces `.env` files. Your secrets live in one encrypted vault on your machine, and `hush run` hands them to your app as environment variables when it starts.

API keys sit in plaintext `.env` files across dozens of project folders. They end up in backups and synced folders, and these days AI coding agents happily read them while poking around a repo. I wanted those keys in one encrypted place instead, without changing any app code.

hush is local-first and free. There are no accounts or servers, and it doesn't sync anything. It's just a binary and a file.

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

### Prebuilt binary

Every [release](https://github.com/pinkpixel-dev/hush/releases) has static Linux binaries for x86_64 and arm64. They don't depend on any system libraries, so they should run on any distro. Download one, check it, and put `hush` somewhere on your `PATH`:

```bash
curl -LO https://github.com/pinkpixel-dev/hush/releases/download/v0.4.0/hush-v0.4.0-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/pinkpixel-dev/hush/releases/download/v0.4.0/hush-v0.4.0-x86_64-unknown-linux-musl.tar.gz.sha256
sha256sum -c hush-v0.4.0-x86_64-unknown-linux-musl.tar.gz.sha256
tar xzf hush-v0.4.0-x86_64-unknown-linux-musl.tar.gz
install -m 755 hush-v0.4.0-x86_64-unknown-linux-musl/hush ~/.local/bin/hush
```

On arm64, swap `x86_64` for `aarch64` in those names.

### With cargo

The crate is called `hush-env` (plain `hush` was taken), but the command it installs is still `hush`. You need Rust 1.89 or newer:

```bash
cargo install hush-env
```

### From source

```bash
git clone https://github.com/pinkpixel-dev/hush.git
cd hush
cargo install --path .
```

hush is Linux-first. I've only tested it on Linux so far. The keychain code should work on macOS and Windows too, but I haven't tried it there yet.

## Quick start

```bash
hush init                        # create the vault, key goes in your keychain
hush recovery-key                # print a recovery key, save it somewhere safe
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
| `hush export` | Print the project's secrets in `.env` format, for tools that insist on reading a file |
| `hush scan [dir]` | Find `.env` files on disk (defaults to your home folder) |
| `hush recovery-key` | Make a recovery key for a keychain vault. `--replace` swaps it for a new one |
| `hush recover` | Unlock the vault with its recovery key and put a new key in your keychain. `--from <file>` rebuilds from a copy of `vault.age` |
| `hush doctor` | Check the vault folder, its files, the config, and the key, and say what's wrong |
| `hush completions <shell>` | Print a completion script for `bash`, `zsh`, `fish`, `elvish`, or `powershell` |

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

### When a tool needs a real .env file

Some tools only read a `.env` file and won't take environment variables. `hush export` prints the current project's secrets in `.env` format, and you decide where they go:

```bash
hush export > .env
hush export --env prod > .env.production
```

hush never writes that file itself, since the whole point is not having plaintext secrets lying around. If you do write one, delete it when you're done, and keep it out of git.

Values with spaces, quotes, `$`, or line breaks get quoted so `hush import` and other dotenv parsers read them back exactly. Tools that take a file literally (like `docker run --env-file`, as far as I know) will see those quotes as part of the value, so `hush run` is the better fit there.

### Finding old .env files

```bash
hush scan ~/projects
```

This prints every `.env` and `.env.*` file it finds, along with how many variables each one holds. It skips templates like `.env.example` and folders like `node_modules`, `.git`, and `target`.

### Shell completions

hush can print a completion script for your shell, so Tab fills in commands and flags. Pick the one you use:

```bash
# bash
mkdir -p ~/.local/share/bash-completion/completions
hush completions bash > ~/.local/share/bash-completion/completions/hush

# zsh (make sure ~/.zfunc is in your fpath before compinit runs)
mkdir -p ~/.zfunc
hush completions zsh > ~/.zfunc/_hush

# fish
hush completions fish > ~/.config/fish/completions/hush.fish
```

Open a new shell afterward. It completes command and flag names, not project or secret names, since those would mean unlocking the vault every time you press Tab.

## Recovery

In keychain mode, the only key that opens the vault normally lives in your keychain. If that entry gets wiped (a reinstall, a reset keyring, a new machine), the vault is just random bytes. A recovery key fixes that:

```bash
hush recovery-key
```

This prints a key starting with `AGE-SECRET-KEY-1`. It's shown once and hush doesn't keep a copy, so put it somewhere that isn't this machine. A password manager is a good spot. From then on, every save encrypts the vault to both your keychain key and the recovery key.

If the keychain entry goes missing, run this and paste the key at the prompt:

```bash
hush recover
```

hush unlocks the vault with the recovery key, stores a fresh key in your keychain, and re-encrypts. Your recovery key keeps working afterward.

### Backing up the vault file

The recovery key only helps if you still have the vault. `vault.age` (in `~/.local/share/hush` by default) is encrypted, so it's safe to copy into whatever backup or sync tool you already use. On a new machine, or after the folder is gone, rebuild from that copy:

```bash
hush recover --from ~/backups/vault.age
```

This sets up the whole vault folder from the copy. It refuses to run if a vault already exists, so it can't overwrite one by accident.

A couple of things to know:

- `hush recovery-key --replace` makes a new key and the old one stops working for future saves. Older copies of `vault.age` in your backups can still be opened with the old key, though, so treat the old one as sensitive until those copies are gone.
- Anyone with your recovery key and a copy of `vault.age` can read every secret in it.
- Passphrase vaults don't have recovery keys (age can't encrypt to a passphrase and a key at the same time). Your passphrase already unlocks the vault anywhere, so back up `vault.age` and `config.json` and remember the passphrase.

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

## When something's off

Run this first:

```bash
hush doctor
```

It prints one line per check: the vault folder and its permissions, `vault.age` and `config.json`, the config itself, whether your key actually opens the vault, any temp files left over from an interrupted write, and which project the current folder is linked to. It never changes anything, and it exits with an error if it finds a real problem. For a passphrase vault it asks for the passphrase (or uses `HUSH_PASSPHRASE`) so it can test the unlock.

hush is strict about the vault folder, and it refuses to work in these cases:

- The folder can be read or written by other users. hush creates it as `0700`, so this usually means something changed it. The error includes the `chmod 700` command that fixes it.
- The folder, `vault.age`, `config.json`, or `vault.lock` is a symlink. If you keep the vault somewhere else (a synced folder, say), point `HUSH_DIR` at the real location instead of symlinking it.

## What it protects (and what it doesn't)

hush keeps secrets out of plaintext files. That covers the usual ways `.env` files leak: getting committed to git, landing in backups or synced folders, showing up in a screen share, or getting read by an AI agent browsing your project.

It doesn't protect you from malware running as your own user while you're logged in. That code could ask the keychain for the key the same way hush does. Once your app is running, its environment is also readable by your user (through `/proc/<pid>/environ` on Linux). So it's a big step up from `.env` files, but it isn't a sandbox.

Two other behaviors are worth knowing:

- If a variable is already set in your shell, the vault value wins for the command `hush run` starts.
- Losing the keychain entry without a recovery key means losing the vault. Run `hush recovery-key` once after `hush init`.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

The tests run every command end to end against real passphrase vaults in temp folders. That includes parallel writers, symlinks, folder permissions, and recoveries that fail partway.

The keychain tests need a running Secret Service, so they're opt-in. Each one uses its own temp vault and deletes its keychain entry afterward, so your real vault isn't touched:

```bash
cargo test --test keychain -- --ignored
```

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)
