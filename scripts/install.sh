#!/usr/bin/env bash
# Builds a release of gnotes and installs it for the current user only
# (no sudo). Works with any XDG desktop: GNOME, KDE, Hyprland launchers
# (rofi/wofi/fuzzel/walker), etc.
#
#   scripts/install.sh              build + install
#   scripts/install.sh --no-build   install the existing target/release/gnotes
#   scripts/install.sh --uninstall  remove it (your vaults are NOT touched)
set -euo pipefail
cd "$(dirname "$0")/.."

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor"
DESKTOP="$APPS_DIR/gnotes.desktop"

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS_DIR" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -f -t "$ICON_DIR" || true
  command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
  command -v kbuildsycoca5 >/dev/null && kbuildsycoca5 >/dev/null 2>&1 || true
}

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$BIN_DIR/gnotes" "$DESKTOP"
  for s in 32x32 128x128 256x256; do rm -f "$ICON_DIR/$s/apps/gnotes.png"; done
  refresh
  echo "gnotes uninstalled. Vaults and backups were left in place."
  exit 0
fi

# Fails early, with the right install command, if build tools are missing.
check_deps() {
  local missing=()
  command -v cargo >/dev/null || missing+=("Rust (cargo)")
  command -v npm >/dev/null || missing+=("Node.js (npm)")
  command -v pkg-config >/dev/null || missing+=("pkg-config")
  if command -v pkg-config >/dev/null && ! pkg-config --exists webkit2gtk-4.1; then
    missing+=("WebKitGTK 4.1")
  fi
  ((${#missing[@]} == 0)) && return 0
  echo "Missing build dependencies: ${missing[*]}" >&2
  echo "Install them with:" >&2
  if command -v pacman >/dev/null; then
    echo "  sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl librsvg xdotool nodejs npm rustup" >&2
    echo "  rustup default stable   # if Rust was just installed" >&2
  elif command -v apt >/dev/null; then
    echo "  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev nodejs npm" >&2
    echo "  and Rust from https://rustup.rs" >&2
  elif command -v dnf >/dev/null; then
    echo "  sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libxdo-devel librsvg2-devel nodejs npm" >&2
    echo "  sudo dnf group install development-tools   # and Rust from https://rustup.rs" >&2
  else
    echo "  WebKitGTK 4.1 development files, a C toolchain, Node.js and Rust" >&2
  fi
  exit 1
}

if [[ "${1:-}" != "--no-build" ]]; then
  check_deps
  [[ -d node_modules ]] || npm install --no-audit --no-fund
  npm run tauri build -- --no-bundle
fi

install -Dm755 target/release/gnotes "$BIN_DIR/gnotes"
install -Dm644 src-tauri/icons/32x32.png       "$ICON_DIR/32x32/apps/gnotes.png"
install -Dm644 src-tauri/icons/128x128.png     "$ICON_DIR/128x128/apps/gnotes.png"
install -Dm644 src-tauri/icons/128x128@2x.png  "$ICON_DIR/256x256/apps/gnotes.png"

mkdir -p "$APPS_DIR"
cat > "$DESKTOP" <<DESK
[Desktop Entry]
Type=Application
Name=gnotes
GenericName=Encrypted Notes
Comment=Encrypted Markdown notes
Exec=$BIN_DIR/gnotes
Icon=gnotes
Terminal=false
Categories=Office;
Keywords=notes;markdown;encrypted;obsidian;joplin;
StartupWMClass=gnotes
StartupNotify=true
DESK

refresh
echo "Installed: $BIN_DIR/gnotes"
echo "Launcher:  $DESKTOP  (search for \"gnotes\" in your app menu)"
