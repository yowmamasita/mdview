#!/usr/bin/env bash
# Build mdview.app, the bundle macOS needs before it will let mdview be the
# default handler for Markdown files.
#
#   scripts/bundle-macos.sh              build into target/
#   scripts/bundle-macos.sh --install    also install and make it the default
#   scripts/bundle-macos.sh --no-build   bundle target/release/mdview as it is
#
# Finder only offers applications, not bare executables, in "Open With"; and it
# passes a double-clicked file as an Apple Event rather than an argument, which
# the binary handles through tao's `Event::Opened`.

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT="$PWD"
APP_NAME="mdview"
BUNDLE_ID="io.github.yowmamasita.mdview"
BUILD_DIR="$ROOT/target"
APP="$BUILD_DIR/$APP_NAME.app"
INSTALL_DIR="/Applications"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

install=false
build=true
for arg in "$@"; do
  case "$arg" in
    --install) install=true ;;
    # Release builds join two architectures into one binary before getting
    # here; rebuilding would replace it with a single-architecture one.
    --no-build) build=false ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
if $build; then
  echo "==> building mdview $version"
  cargo build --release
elif [[ ! -x "$BUILD_DIR/release/$APP_NAME" ]]; then
  echo "no binary at $BUILD_DIR/release/$APP_NAME; drop --no-build" >&2
  exit 1
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BUILD_DIR/release/$APP_NAME" "$APP/Contents/MacOS/$APP_NAME"
sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

echo "==> rendering icon"
iconset="$BUILD_DIR/$APP_NAME.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" packaging/macos/icon-1024.png \
    --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" packaging/macos/icon-1024.png \
    --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns "$iconset" --output "$APP/Contents/Resources/$APP_NAME.icns"
rm -rf "$iconset"

# arm64 macOS refuses to run a binary with no signature at all, so this always
# happens. `MDVIEW_SIGN_IDENTITY` upgrades it from an ad-hoc signature to a real
# Developer ID one, which is what release builds use when a certificate is
# available. Ad-hoc is fine for a locally built application; it is only
# downloads that Gatekeeper treats differently.
identity="${MDVIEW_SIGN_IDENTITY:--}"
if [[ "$identity" == "-" ]]; then
  echo "==> signing (ad-hoc)"
  codesign --force --sign - "$APP"
else
  echo "==> signing as $identity"
  codesign --force --options runtime --timestamp \
    --sign "$identity" "$APP/Contents/MacOS/$APP_NAME"
  codesign --force --options runtime --timestamp --sign "$identity" "$APP"
fi
codesign --verify --strict "$APP"

echo "built $APP"

if ! $install; then
  echo
  echo "To install and make it the default Markdown viewer:"
  echo "    $0 --install"
  exit 0
fi

echo "==> installing to $INSTALL_DIR"
rm -rf "${INSTALL_DIR:?}/$APP_NAME.app"
cp -R "$APP" "$INSTALL_DIR/"

echo "==> registering with Launch Services"
"$LSREGISTER" -f "$INSTALL_DIR/$APP_NAME.app"

echo "==> setting as the default handler for Markdown"
scripts/set-default-handler.swift "$BUNDLE_ID"

echo
echo "Done. Double-click a .md file, or run: open -a mdview file.md"
