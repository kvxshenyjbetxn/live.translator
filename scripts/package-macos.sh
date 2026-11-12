#!/bin/bash
# Build locally; no Apple account, notarization or publishing.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS is required to build and sign this package." >&2
  exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(cargo metadata --no-deps --format-version 1   | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "live-translator"))')"
target="$(rustc -vV | awk '/^host:/ {print $2}')"
case "$target" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x64 ;;
  *) echo "Unsupported macOS host: $target" >&2; exit 1 ;;
esac

cargo build --release --target "$target" -p live-translator

app="$root/target/$target/release/bundle/macos/Live Translator.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/$target/release/live-translator" "$app/Contents/MacOS/live-translator"
cp "$root/crates/app/assets/icons/icon.icns" "$app/Contents/Resources/icon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Live Translator</string>
<key>CFBundleDisplayName</key><string>Live Translator</string>
<key>CFBundleIdentifier</key><string>dev.live-translator.desktop</string>
<key>CFBundleExecutable</key><string>live-translator</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>14.2</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSAudioCaptureUsageDescription</key><string>Live Translator captures only the selected application's audio and sends it to Google for live translation. No microphone or audio recording files.</string>
</dict></plist>
PLIST
/usr/bin/codesign --force --deep --sign - "$app"
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
