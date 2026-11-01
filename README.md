# gnotes

Encrypted Markdown notes for Linux (and Windows). Obsidian-style folders and
editing, Joplin-style encryption: the whole vault is one encrypted file,
unlocked with a master password.

## Layout

```
crates/gnotes-core/   vault: crypto + storage (pure Rust, unit-tested, no UI deps)
src-tauri/            desktop app: exposes the vault to the UI via commands
src/                  Svelte UI (CodeMirror editor, markdown preview)
```

## Security model

- Vault = one SQLCipher database (AES-256, HMAC per page). Everything inside
  is encrypted: titles, folders, bodies, indexes.
- Key = Argon2id(password, salt), 64 MiB / 3 passes. The salt is the SQLCipher
  header salt, so the vault is a single self-contained file.
- Decrypted content lives only in memory; the vault auto-locks after
  inactivity (default 10 min) and the UI drops all note content on lock.
- Rendered Markdown is sanitized (DOMPurify) and the webview runs under a strict
  CSP: no remote scripts, no remote images, no navigation away from the app.
- Attachments (images, PDFs, ...) are stored inside the vault too. The
  preview loads images through an in-memory `gnotes://` protocol, so decrypted
  files never touch the disk; other files can only leave via "save a copy".
- Remote images are not loaded (a placeholder shows instead).
- No password recovery: forget it and the data is gone.

## Backups and moving machines

- **Backup** button writes `gnotes-backups/<name>-<UTC timestamp>.gnotes` next
  to the vault, verified by an integrity check. It is encrypted with the same password.
- To move to another machine, copy the `.gnotes` file and open it there.

## Import / export

Vault menu (bottom of the sidebar):

- **Import Markdown folder…**: any folder of `.md` files, including Obsidian
  vaults (`![[image.png]]` embeds resolved by name, `.obsidian/` skipped) and
  Joplin Markdown exports (`_resources/`). Referenced images and files become
  encrypted attachments. Imported into a new folder named after the source.
- **Export as Markdown…**: writes `gnotes-export-<time>/` with one `.md` per
  note and an `attachments/` folder; links are rewritten to relative paths.
  The export is NOT encrypted.

## Markdown support

CommonMark + GFM tables, task lists, strikethrough, footnotes, `==highlight==`,
`$math$` / `$$math$$` (KaTeX), ```` ```mermaid ```` diagrams, callouts
(`> [!note] Title`), YAML front matter, syntax-highlighted code, heading
anchors, raw HTML (sanitized).

## Develop

Linux prerequisites (Debian/Ubuntu):

    sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
      libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

Then:

    npm install
    npm run tauri dev          # run the app
    cargo test -p gnotes-core  # vault tests
    npm run check              # typecheck UI
    npm run tauri build        # release bundles (.deb, .rpm, AppImage)

Install for your user (no sudo; shows up in GNOME/KDE/Hyprland launchers):

    scripts/install.sh              # build release + install to ~/.local
    scripts/install.sh --no-build   # reinstall existing build
    scripts/install.sh --uninstall  # remove app (vaults untouched)

Windows builds should be produced natively (e.g. a GitHub Actions matrix), not
cross-compiled.

## Shortcuts

Ctrl+N new note · Ctrl+E cycle edit/split/preview · Ctrl+S save now ·
Ctrl+Shift+S toggle scroll sync (split view) · Ctrl+L lock ·
Ctrl + / Ctrl − / Ctrl+0 and Ctrl+wheel zoom

## Roadmap

1. ~~Core: encrypted vault, unlock, folders, editor + preview, backups~~
2. ~~Images/attachments, math (KaTeX), Mermaid, export/import plain .md folder~~
3. Tags, full-text search, `[[wikilinks]]` + backlinks
4. Read-later: save a URL → clean Markdown with images stored offline
