#!/bin/bash
# Baut CraftCenter.app (Apple Silicon, ad-hoc signiert) und packt sie in dist/CraftCenter.dmg.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="$(awk -F'"' '/^version/ {print $2; exit}' Cargo.toml)"
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin

APP="dist/CraftCenter.app"
rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/aarch64-apple-darwin/release/craft-center "$APP/Contents/MacOS/CraftCenter"
cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>CraftCenter</string>
  <key>CFBundleDisplayName</key><string>Craft-Zentrale</string>
  <key>CFBundleExecutable</key><string>CraftCenter</string>
  <key>CFBundleIdentifier</key><string>com.76344243.craftcenter</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

# Symbol: ein abgerundetes Quadrat mit einem Pfeil im Kreis, aus dem Code gezeichnet.
# Fails softly: without an icon the app still builds (macOS shows a generic one).
make_icon() {
  local iconset="dist/AppIcon.iconset"
  mkdir -p "$iconset"
  swift packaging/make-icon.swift "dist/icon-1024.png" || return 1
  for s in 16 32 128 256 512; do
    sips -z $s $s dist/icon-1024.png --out "$iconset/icon_${s}x${s}.png" >/dev/null || return 1
    d=$((s * 2)); sips -z $d $d dist/icon-1024.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null || return 1
  done
  iconutil -c icns "$iconset" -o "$APP/Contents/Resources/AppIcon.icns"
}
if command -v swift >/dev/null; then
  make_icon || echo "warning: no app icon"
fi

codesign --force --deep --sign - "$APP"
codesign --verify --strict "$APP"
hdiutil create -volname "Craft-Zentrale" -srcfolder "$APP" -ov -format UDZO "dist/CraftCenter-${VERSION}-macos-aarch64.dmg"
