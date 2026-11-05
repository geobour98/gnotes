# Updating dependencies

gnotes is built from three kinds of dependencies:

| What           | Where it's declared     | Exact versions in   |
| -------------- | ----------------------- | ------------------- |
| Rust toolchain | installed with `rustup` | (your machine)      |
| Rust crates    | `Cargo.toml` files      | `Cargo.lock`        |
| npm packages   | `package.json`          | `package-lock.json` |

The lock files pin every version, so a build today and a build in a year use the same code until
you update on purpose.

## Routine updates (bug fixes, security fixes)

```sh
scripts/update.sh --check   # see what's outdated or vulnerable; changes nothing
scripts/update.sh           # update, then run every check
```

`scripts/update.sh` updates the Rust toolchain, then every crate and npm package **within the
version ranges already declared**: bug-fix and minor releases (Svelte 5.56 → 5.60, Tauri 2.12 →
2.13), never a new major version. Then it runs formatting, lint, type-check, tests, a build and
both security audits (`npm audit`, `cargo audit`).

- Everything passes: commit `Cargo.lock` and `package-lock.json`, then `scripts/install.sh`.
- Something fails: go back with `git checkout Cargo.lock package-lock.json && npm ci`, and
  try again later or update the failing package on its own (below).

**One package only**, e.g. because a specific bug was just fixed:

```sh
cargo update -p tauri          # a Rust crate
npm update svelte              # an npm package
scripts/update.sh --verify     # run all checks
```

Run `scripts/update.sh --check` now and then (e.g. monthly), and whenever you hear about a bug or
security fix in one of the dependencies.

## Major upgrades

A new major version (Svelte 6, Tauri 3, Vite 9, markdown-it 16, …) can change APIs, so it is done
one package at a time:

1. Read the package's release notes or migration guide.
2. Change the version:
   - npm: `npm install svelte@6` (dev tools: `npm install -D vite@9`)
   - Rust: edit the version in the `Cargo.toml` that lists the crate, then `cargo update -p <crate>`
3. Run `scripts/update.sh --verify` and fix what it reports.
4. Try the app: `npm run tauri dev`. Unlock, edit, search, attach an image, export a note, lock.
5. Commit that upgrade on its own, so it is easy to revert.

Packages that belong together must be upgraded together:

| Upgrade               | Together with                                                                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Tauri (`tauri` crate) | `@tauri-apps/cli`, `@tauri-apps/api`, `tauri-build`, the `tauri-plugin-*` crates and their `@tauri-apps/plugin-*` npm packages |
| Svelte                | `@sveltejs/kit`, `@sveltejs/vite-plugin-svelte`, `svelte-check`, `prettier-plugin-svelte`                                      |
| Vite                  | `@sveltejs/vite-plugin-svelte`                                                                                                 |
| CodeMirror            | all `@codemirror/*` packages and `codemirror`                                                                                  |

For Tauri, `npx tauri migrate` handles most of a major upgrade's config changes.

## Things that need care

- **`rusqlite` / SQLCipher:** after upgrading, run the tests (they create, reopen and re-key real
  vaults) and open a copy of a real vault before using it. Never test on your only copy.
- **Argon2 parameters** (`crates/gnotes-core/src/crypto.rs`): changing them would make existing
  vaults impossible to open. They need a migration, not an edit.
- **DOMPurify, markdown-it, KaTeX, Mermaid** render untrusted note content: keep them current, and
  check after upgrading that a note with HTML, math and a diagram still renders.
