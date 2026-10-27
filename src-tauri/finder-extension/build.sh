#!/bin/bash
# 构建 Finder Sync .appex。产物由 tauri.conf.json 的 bundle.macOS.files
# 拷进 ClipBase.app/Contents/PlugIns/。
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/build/ClipBaseFinderExtension.appex"
BIN="$OUT/Contents/MacOS/ClipBaseFinderExtension"
BUNDLE_ID="${FINDER_EXTENSION_BUNDLE_ID:-com.clipbase.app.FinderExtension}"
IDENTITY="${CODESIGN_IDENTITY:--}"
MIN_VERSION="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
ARCH="${ARCH:-$(uname -m)}"
SDK="$(xcrun --sdk macosx --show-sdk-path)"

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS"

xcrun swiftc \
    -O \
    -whole-module-optimization \
    -parse-as-library \
    -module-name ClipBaseFinderExtension \
    -target "$ARCH-apple-macos$MIN_VERSION" \
    -sdk "$SDK" \
    -framework Foundation \
    -framework AppKit \
    -framework FinderSync \
    -Xlinker -e -Xlinker _NSExtensionMain \
    -o "$BIN" \
    "$HERE/Sources/FinderSync.swift"

cp "$HERE/Info.plist" "$OUT/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$OUT/Contents/Info.plist"

if [ "$IDENTITY" = "-" ]; then
    codesign --force --sign - --entitlements "$HERE/entitlements.plist" "$OUT"
else
    codesign --force --options runtime --timestamp \
        --sign "$IDENTITY" \
        --entitlements "$HERE/entitlements.plist" \
        "$OUT"
fi

codesign --verify --deep --strict "$OUT"
/usr/libexec/PlistBuddy -c "Print :NSExtension:NSExtensionPointIdentifier" "$OUT/Contents/Info.plist"
echo "built: $OUT"
