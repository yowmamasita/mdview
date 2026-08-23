#!/bin/sh
# Install mdview on macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/yowmamasita/mdview/main/scripts/install-macos.sh | sh
#
# Downloading through curl rather than a browser is the point: macOS attaches
# the quarantine flag to browser downloads, and an application signed ad-hoc
# rather than with a paid Developer ID cannot clear Gatekeeper once it carries
# that flag. Nothing fetched here is quarantined, so the application just opens.

set -eu

REPO="yowmamasita/mdview"
ASSET="mdview-macos-universal-app.tar.gz"
APP_DIR="/Applications"
BIN_DIR="${HOME}/.local/bin"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

[ "$(uname -s)" = "Darwin" ] || { echo "this installer is for macOS" >&2; exit 1; }

version="${MDVIEW_VERSION:-}"
if [ -z "$version" ]; then
  echo "==> finding the latest release"
  version=$(
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
      sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1
  )
fi
[ -n "$version" ] || { echo "could not determine a version to install" >&2; exit 1; }

url="https://github.com/$REPO/releases/download/$version/$ASSET"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "==> downloading mdview $version"
curl -fsSL "$url" -o "$tmp/$ASSET"

echo "==> verifying checksum"
if curl -fsSL "https://github.com/$REPO/releases/download/$version/SHA256SUMS" -o "$tmp/SHA256SUMS"; then
  expected=$(grep " $ASSET\$" "$tmp/SHA256SUMS" | awk '{print $1}')
  actual=$(shasum -a 256 "$tmp/$ASSET" | awk '{print $1}')
  if [ -n "$expected" ] && [ "$expected" != "$actual" ]; then
    echo "checksum mismatch for $ASSET" >&2
    echo "  expected $expected" >&2
    echo "  actual   $actual" >&2
    exit 1
  fi
  echo "  ok"
else
  echo "  no SHA256SUMS published; skipping"
fi

echo "==> installing to $APP_DIR/mdview.app"
tar -xzf "$tmp/$ASSET" -C "$tmp"
rm -rf "$APP_DIR/mdview.app"
mv "$tmp/mdview.app" "$APP_DIR/"
# Belt and braces: if this script was itself downloaded by a browser and run
# from a quarantined directory, the flag can still be inherited.
xattr -dr com.apple.quarantine "$APP_DIR/mdview.app" 2>/dev/null || true

echo "==> linking the command line binary into $BIN_DIR"
mkdir -p "$BIN_DIR"
ln -sf "$APP_DIR/mdview.app/Contents/MacOS/mdview" "$BIN_DIR/mdview"

echo "==> registering with Launch Services"
"$LSREGISTER" -f "$APP_DIR/mdview.app"

echo
echo "Installed. Open a file with:  mdview file.md"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Add $BIN_DIR to your PATH to use it from anywhere." ;;
esac
echo
echo "To make mdview the default Markdown viewer, right-click any .md file →"
echo "Get Info → Open with → mdview → Change All."
