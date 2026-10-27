#!/bin/bash
# Build locally; no Apple account, notarization or publishing.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS is required to build and sign this package." >&2
  exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(node -p "require('./package.json').version")"
target="$(rustc -vV | awk '/^host:/ {print $2}')"
case "$target" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x64 ;;
  *) echo "Unsupported macOS host: $target" >&2; exit 1 ;;
esac

# Fix output/identity; do not import certificates or notarize even if credentials
# happen to be configured in the caller's environment.
env -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID \
  -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
  -u APPLE_CERTIFICATE -u APPLE_CERTIFICATE_PASSWORD \
  CARGO_TARGET_DIR="$root/target" APPLE_SIGNING_IDENTITY=- \
  npm run tauri -- build --bundles app --target "$target"
app="$root/target/$target/release/bundle/macos/Live Translator.app"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app"
bash "$root/scripts/verify-macos-signature.sh" "$app"

out="$root/target/release/distribute"
mkdir -p "$out"
stage="$(mktemp -d "$out/.package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/payload"
/usr/bin/ditto "$app" "$stage/payload/Live Translator.app"
cp "$root/INSTALL-macOS.txt" "$stage/payload/INSTALL-macOS.txt"
/usr/bin/codesign --verify --deep --strict "$stage/payload/Live Translator.app"
archive="Live-Translator_${version}_macOS_${arch}.zip"
/usr/bin/ditto -c -k --sequesterRsrc "$stage/payload" "$stage/$archive"
(cd "$stage" && /usr/bin/shasum -a 256 "$archive" > "$archive.sha256")
mv "$stage/$archive" "$stage/$archive.sha256" "$out/"
printf '\nAd-hoc signed (NOT notarized):\n%s\n%s\n' "$out/$archive" "$out/$archive.sha256"
