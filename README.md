# gnotes

Encrypted Markdown notes for Linux. Obsidian-style folders and editing, Joplin-style encryption:
all your notes, images and attachments live in **one encrypted file**, unlocked with a master
password.

- **Private by design:** nothing is stored unencrypted, nothing is sent over the internet.
- **Simple to back up and move:** it's one file. Copy it to another machine and open it there.
- **Full Markdown:** tables, task lists, footnotes, math (KaTeX), Mermaid diagrams, callouts,
  syntax-highlighted code, front matter.

## Features

- Folders and notes, with a live preview (edit / split / preview) and synchronized scrolling
- Tabs (reopened after unlocking), an outline of the note's headings, zoom
- Full-text search and `#tags` (including nested `#ai/llm` and front-matter `tags:`)
- Images and files stored inside the vault: paste, drag & drop, or attach
- Import an Obsidian vault, a Joplin export or any folder of `.md` files, including shared image
  folders
- Export the vault as plain Markdown, or one note as HTML, Markdown or PDF to share it
- Auto-lock after inactivity, when the screen locks, and before the computer sleeps
- One-click encrypted backups

## Install (Linux)

**1. System libraries**, plus [Node.js](https://nodejs.org) 22+ and [Rust](https://rustup.rs):

```sh
# Arch
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl librsvg xdotool nodejs npm rustup
rustup default stable

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev nodejs npm
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Fedora
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libxdo-devel librsvg2-devel nodejs npm
sudo dnf group install development-tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**2. Build and install** for your user (no sudo needed):

```sh
git clone https://github.com/geobour98/gnotes.git
cd gnotes
scripts/install.sh
```

gnotes now appears in your app launcher (GNOME, KDE, Hyprland launchers such as rofi / wofi /
fuzzel, …). The install script checks the system libraries first and prints the right command if
something is missing.

| Command                          | What it does                                 |
| -------------------------------- | -------------------------------------------- |
| `scripts/install.sh`             | build a release and install it to `~/.local` |
| `scripts/install.sh --no-build`  | reinstall the last build                     |
| `scripts/install.sh --uninstall` | remove the app (your vaults are not touched) |

**Updating gnotes:** `git pull && scripts/install.sh`.

## Using gnotes

**First start:** choose where the vault file goes (default
`~/.local/share/com.gnotes.app/notes.gnotes`) and a master password of at least 8 characters.
**There is no password recovery.** If you forget it, the notes cannot be decrypted.

**Backups:** _Vault ▴ → Backup now_ writes an encrypted, integrity-checked copy to
`gnotes-backups/` next to the vault. To move to another machine, copy the `.gnotes` file and open
it there.

**Import** (_Vault ▴_ menu):

- _Import Markdown folder…_: any folder of `.md` files, including Obsidian vaults (`![[image.png]]`
  embeds, `.obsidian/` skipped) and Joplin exports (`_resources/`). Referenced images and files are
  copied into the vault. You choose the destination folder, and whether to wrap the import in a new
  folder or merge it.
- _Import images from folder…_: for notes whose images live in a shared folder (e.g. `img/`). Every
  referenced file found there is copied into the vault once and all links to it are updated.

**Export:** _Vault ▴ → Export as Markdown…_ writes every note plus an `attachments/` folder. For a
single note, use _⇪ Export ▾_ in the note's header: an HTML file with images embedded (opens in any
browser), Markdown with its attachments, or Print / Save as PDF. **Exports are not encrypted**, as
they are meant for sharing.

**Tags:** write `#tag` anywhere in a note (or `tags: [a, b]` in front matter). `#ai/llm` is nested
under `#ai`. Headings (`# Title`), numbers (`#123`) and code don't count. Click a tag in the sidebar
or the preview, or search for `#tag` / `tag:name`.

### Keyboard shortcuts

| Keys                        | Action                                   |
| --------------------------- | ---------------------------------------- |
| Ctrl+N                      | New note                                 |
| Ctrl+Shift+F                | Search (`#tag` for tags)                 |
| Ctrl+Shift+O                | Outline (the note's headings)            |
| Ctrl+E                      | Switch view: edit / split / preview      |
| Ctrl+Tab / Ctrl+Shift+Tab   | Next / previous tab                      |
| Ctrl+W                      | Close tab                                |
| Ctrl+1 … Ctrl+9             | Go to tab (9 = last)                     |
| Ctrl+S                      | Save now (notes also save automatically) |
| Ctrl+Shift+S                | Scroll sync between editor and preview   |
| Ctrl+Shift+[ / Ctrl+Shift+] | Fold / unfold the section at the cursor  |
| Ctrl+L                      | Lock                                     |
| Ctrl + / − / 0, Ctrl+wheel  | Zoom                                     |

Ctrl+click or middle-click a note in the sidebar to open it in a new tab.

## Security

**What protects your notes**

- The vault is a single [SQLCipher](https://www.zetetic.net/sqlcipher/) database (AES-256, with an
  HMAC on every page). Everything inside is encrypted: titles, folders, note text, attachments,
  tags, even which tabs were open.
- The key is derived from your password with Argon2id (64 MiB, 3 passes), which makes guessing
  passwords slow even on GPUs. The random salt is stored in the file's header, so the vault is
  self-contained.
- Decrypted content exists only in memory. Locking (manual, after inactivity, on screen lock, before
  sleep, or on closing the window) drops it and wipes the key. Temporary database files are kept in
  memory, never on disk.
- Images are shown from memory through an internal `gnotes://` address; decrypted files never touch
  the disk unless you export or save them.
- gnotes makes no network requests: remote images in notes are not loaded (a placeholder is shown),
  links open in your normal browser only when you click them, and the app window can't be navigated
  to any web page.
- Notes are rendered with sanitized HTML (DOMPurify) under a strict Content Security Policy, so a
  malicious note (e.g. an imported one) can't run scripts.

**What it does not protect against**

- Someone who knows or guesses your password, or malware running as your user while the vault is
  unlocked (it could read memory or your keystrokes). Use a strong password.
- Exports and saved attachments are plain files: protect or delete them yourself.
- Backups made before a password change still open with the old password.
- The settings file (`~/.config/com.gnotes.app/config.json`) is not encrypted. It holds only the
  vault's path and the lock settings.

## Development

```
crates/gnotes-core/   the vault: crypto, storage, search, tags, import/export (pure Rust, no UI)
src-tauri/            the desktop app (Tauri): exposes the vault to the UI as commands
src/                  the UI (Svelte 5 + TypeScript): CodeMirror editor, Markdown preview
scripts/              install.sh, update.sh
```

```sh
npm install
npm run tauri dev    # run the app with live reload
npm test             # Rust tests
npm run check        # type-check the UI
npm run lint         # formatting (Prettier, rustfmt) and clippy
npm run format       # fix formatting
```

The UI can only reach the vault through the commands in `src-tauri/src/lib.rs`, and those are
allowed in `src-tauri/capabilities/default.json`. Keep both lists small.

### Updating dependencies

```sh
scripts/update.sh --check   # what's outdated or vulnerable (changes nothing)
scripts/update.sh           # update everything within safe version ranges, then test it all
```

Major upgrades (e.g. Svelte 6, Tauri 3) are covered in [docs/UPDATING.md](docs/UPDATING.md).

Windows is not tested yet. Builds for it should be made on Windows, not cross-compiled.

## License

[MIT](LICENSE)
