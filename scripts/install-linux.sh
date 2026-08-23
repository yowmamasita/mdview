#!/usr/bin/env bash
# Install mdview for the current user and make it the default Markdown viewer.
#
#   scripts/install-linux.sh
#
# Everything goes under ~/.local, so no root is needed. `xdg-mime` is what
# actually sets the association; the desktop entry is what makes mdview appear
# in a file manager's "Open With" list at all.

set -euo pipefail

cd "$(dirname "$0")/.."

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/512x512/apps"

echo "==> building"
cargo build --release

echo "==> installing binary to $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 755 target/release/mdview "$BIN_DIR/mdview"

echo "==> installing desktop entry and icon"
mkdir -p "$APPS_DIR" "$ICON_DIR"
install -m 644 packaging/linux/mdview.desktop "$APPS_DIR/mdview.desktop"
install -m 644 packaging/macos/icon-1024.png "$ICON_DIR/mdview.png"

if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$APPS_DIR"
fi
if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true
fi

echo "==> setting as the default handler for Markdown"
for type in text/markdown text/x-markdown; do
  xdg-mime default mdview.desktop "$type"
  echo "  $type -> $(xdg-mime query default "$type")"
done

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo; echo "note: $BIN_DIR is not on your PATH." ;;
esac

echo
echo "Done. Double-click a .md file, or run: mdview file.md"
