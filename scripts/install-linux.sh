#!/usr/bin/env bash
# Install mdview for the current user and make it the default Markdown viewer.
#
#   scripts/install-linux.sh
#
# Works both from a source checkout, where it builds first, and from an
# unpacked release tarball, where the binary is already sitting next to it.
# Everything goes under ~/.local, so no root is needed. `xdg-mime` is what
# actually sets the association; the desktop entry is what makes mdview appear
# in a file manager's "Open With" list at all.

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/512x512/apps"

# Release tarball layout, or source checkout.
if [[ -x "$here/mdview" ]]; then
  binary="$here/mdview"
  desktop="$here/mdview.desktop"
  icon="$here/mdview.png"
else
  cd "$here/.."
  echo "==> building"
  cargo build --release
  binary="$PWD/target/release/mdview"
  desktop="$PWD/packaging/linux/mdview.desktop"
  icon="$PWD/packaging/macos/icon-1024.png"
fi

echo "==> installing binary to $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 755 "$binary" "$BIN_DIR/mdview"

echo "==> installing desktop entry and icon"
mkdir -p "$APPS_DIR" "$ICON_DIR"
install -m 644 "$desktop" "$APPS_DIR/mdview.desktop"
install -m 644 "$icon" "$ICON_DIR/mdview.png"

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
