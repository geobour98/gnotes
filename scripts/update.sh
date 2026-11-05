#!/usr/bin/env bash
# Updates gnotes' dependencies and toolchain, then verifies everything.
#
#   scripts/update.sh           update within safe (semver-compatible) ranges, then check
#   scripts/update.sh --check   only report what is outdated / vulnerable; change nothing
#   scripts/update.sh --verify  only run the checks (e.g. after a manual upgrade)
#
# "Safe" means what the version ranges in Cargo.toml / package.json allow:
# bug-fix and minor releases (e.g. svelte 5.56 -> 5.60), never a new major
# version (svelte 6). Major upgrades are manual: see docs/UPDATING.md.
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-update}"
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

verify() {
  step "Formatting and lint"
  npm run --silent lint
  step "Type-check the UI"
  npm run --silent check
  step "Rust tests"
  cargo test --workspace --quiet
  step "Build the UI"
  npm run --silent build >/dev/null
  step "Security audit: npm packages that ship in the app"
  npm audit --omit=dev
  step "Security audit: Rust crates"
  if command -v cargo-audit >/dev/null; then
    cargo audit
  else
    echo "cargo-audit is not installed (cargo install cargo-audit --locked); skipped"
  fi
  step "All checks passed"
}

report() {
  step "Rust toolchain"
  if command -v rustup >/dev/null; then rustup check || true; else rustc --version; fi
  step "Rust crates with newer compatible versions"
  cargo update --dry-run --verbose 2>&1 | grep -E "Updating|Unchanged|behind" || echo "none"
  step "npm packages (Wanted = safe update, Latest = may need a major upgrade)"
  npm outdated || true
  step "Known vulnerabilities"
  npm audit --omit=dev || true
  if command -v cargo-audit >/dev/null; then cargo audit || true; fi
}

case "$mode" in
  --check)
    report
    ;;
  --verify)
    verify
    ;;
  update)
    if ! git diff --quiet -- Cargo.lock package-lock.json 2>/dev/null; then
      echo "Cargo.lock / package-lock.json have uncommitted changes; commit or revert them first." >&2
      exit 1
    fi
    if command -v rustup >/dev/null; then
      step "Updating the Rust toolchain"
      rustup update stable
    fi
    step "Updating Rust crates (Cargo.lock)"
    cargo update
    step "Updating npm packages (package-lock.json)"
    npm update --no-fund
    if ! verify; then
      echo
      echo "Something failed after updating. To go back to the previous versions:" >&2
      echo "  git checkout Cargo.lock package-lock.json && npm ci" >&2
      exit 1
    fi
    echo
    echo "Updated and verified. Review with 'git diff --stat', then commit Cargo.lock and package-lock.json."
    echo "Run scripts/install.sh to rebuild the installed app."
    ;;
  *)
    sed -n '2,10p' "$0"
    exit 1
    ;;
esac
