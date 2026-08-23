#!/usr/bin/env bash
# Point the Homebrew cask at a released version.
#
#   scripts/update-tap.sh            # the latest release
#   scripts/update-tap.sh v0.1.2     # a specific one
#
# Reads the checksum from the release's own SHA256SUMS rather than recomputing
# it, so the cask can only ever agree with what was published.

set -euo pipefail

cd "$(dirname "$0")/.."

REPO="yowmamasita/mdview"
TAP="yowmamasita/homebrew-tap"
ASSET="mdview-macos-universal-app.tar.gz"
CASK="Casks/mdview.rb"

command -v gh >/dev/null || { echo "the GitHub CLI (gh) is required" >&2; exit 1; }

tag=${1:-$(gh release view --repo "$REPO" --json tagName -q .tagName)}
version=${tag#v}

echo "==> $REPO $tag"
sha=$(
  gh release download "$tag" --repo "$REPO" --pattern SHA256SUMS --output - |
    awk -v a="$ASSET" '$2 == a { print $1 }'
)
[[ -n "$sha" ]] || { echo "no checksum for $ASSET in $tag" >&2; exit 1; }
echo "    $sha"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
gh repo clone "$TAP" "$tmp/tap" -- --quiet

cask="$tmp/tap/$CASK"
[[ -f "$cask" ]] || { echo "$CASK not found in $TAP" >&2; exit 1; }

# Only the two lines that describe the release ever change.
sed -i.bak \
  -e "s/^  version \".*\"$/  version \"$version\"/" \
  -e "s/^  sha256 \".*\"$/  sha256 \"$sha\"/" \
  "$cask"
rm -f "$cask.bak"

cd "$tmp/tap"
if git diff --quiet; then
  echo "==> already at $version"
  exit 0
fi
git diff --stat | sed 's/^/    /'
git commit -qam "mdview $version"
git push -q origin HEAD
echo "==> pushed"
