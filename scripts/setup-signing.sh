#!/usr/bin/env bash
# Configure signed, notarised macOS releases.
#
#   scripts/setup-signing.sh ~/Downloads/AuthKey_ABCD1234.p8 ABCD1234 <issuer-uuid>
#
# Exports the Developer ID certificate from your keychain, encodes it and the
# App Store Connect API key, and stores both as repository secrets. Run it once;
# after that every tagged release is signed and notarised automatically.
#
# Nothing is written to disk outside a temporary directory that is removed on
# exit, and no secret is echoed.

set -euo pipefail

cd "$(dirname "$0")/.."

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-2}"
}

[[ $# -eq 3 ]] || usage
api_key_path=$1
api_key_id=$2
api_issuer=$3

[[ -f "$api_key_path" ]] || { echo "no API key at $api_key_path" >&2; exit 1; }
command -v gh >/dev/null || { echo "the GitHub CLI (gh) is required" >&2; exit 1; }

# --- the certificate -------------------------------------------------------

identity=$(
  security find-identity -v -p codesigning |
    sed -n 's/.*"\(Developer ID Application: .*\)"/\1/p' | head -1
)

if [[ -z "$identity" ]]; then
  cat >&2 <<'MSG'
No "Developer ID Application" certificate found in your keychain.

This is a different certificate from "Apple Development" (which signs builds for
your own machines) and "Apple Distribution" (which signs App Store submissions).
Neither can sign software distributed outside the App Store.

Creating one costs nothing on an existing paid membership:

  Xcode → Settings → Accounts → your team → Manage Certificates…
    → + → Developer ID Application

or https://developer.apple.com/account/resources/certificates → + → Developer ID.
Only the Account Holder can create it.
MSG
  exit 1
fi

echo "==> certificate: $identity"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
chmod 700 "$tmp"

# A throwaway password: the .p12 exists only long enough to be encoded, and the
# same value is stored alongside it so the workflow can open it.
p12_password=$(uuidgen)

echo "==> exporting the certificate (your keychain will ask for permission)"
security export -t identities -f pkcs12 -P "$p12_password" -o "$tmp/cert.p12" \
  -T /usr/bin/codesign 2>/dev/null ||
  security export -t identities -f pkcs12 -P "$p12_password" -o "$tmp/cert.p12"

[[ -s "$tmp/cert.p12" ]] || { echo "the export produced nothing" >&2; exit 1; }

# --- store everything ------------------------------------------------------

set_secret() {
  printf '%s' "$2" | gh secret set "$1" >/dev/null
  echo "  $1"
}

echo "==> storing repository secrets"
set_secret APPLE_CERTIFICATE_P12 "$(base64 < "$tmp/cert.p12" | tr -d '\n')"
set_secret APPLE_CERTIFICATE_PASSWORD "$p12_password"
set_secret APPLE_SIGNING_IDENTITY "$identity"
set_secret APPLE_API_KEY "$(base64 < "$api_key_path" | tr -d '\n')"
set_secret APPLE_API_KEY_ID "$api_key_id"
set_secret APPLE_API_ISSUER "$api_issuer"

cat <<MSG

Done. The next tagged release will be signed and notarised.

Check it locally first if you like:

    scripts/bundle-macos.sh
    MDVIEW_SIGN_IDENTITY="$identity" scripts/bundle-macos.sh --no-build
    spctl --assess --type execute --verbose=4 target/mdview.app

Signing alone still leaves Gatekeeper unhappy — it is notarisation, which only
happens in CI, that satisfies it.
MSG
