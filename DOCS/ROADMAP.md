# Roadmap

Ideas, not promises. Ordered roughly by how useful they'd be.

## Next

- Test the keychain path on a few more Linux setups (KWallet, GNOME Keyring, no Secret Service at all)
- Pick a crates.io package name. `hush` is taken by an unrelated shell language, so publishing would need something like `hush-cli` with the binary still called `hush`
- Encrypted vault backup and restore, since losing the keychain entry currently means losing the vault
- Shell completions

## Later

- `hush export` for tools that insist on reading a `.env` file
- A way to move from keychain mode to passphrase mode (and back) without re-entering everything
- macOS and Windows testing
- A small TUI for browsing projects and environments
- Release binaries
