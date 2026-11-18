#!/bin/bash
# 构建 Finder Sync .appex。产物由 tauri.conf.json 的 bundle.macOS.files
# 拷进 Halo.app/Contents/PlugIns/。
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/build/HaloFinderExtension.appex"
BIN="$OUT/Contents/MacOS/HaloFinderExtension"
BUNDLE_ID="${FINDER_EXTENSION_BUNDLE_ID:-com.halo.desktop.FinderExtension}"
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
    -module-name HaloFinderExtension \
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

# Finder 会按平台元数据匹配扩展；缺这些键时插件可能注册不上。
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
SDK_BUILD="$(xcrun --sdk macosx --show-sdk-build-version)"
/usr/libexec/PlistBuddy \
    -c "Set :DTPlatformVersion $SDK_VERSION" \
    -c "Add :DTSDKName string macosx$SDK_VERSION" \
    -c "Add :DTSDKBuild string $SDK_BUILD" \
    -c "Add :BuildMachineOSBuild string $(sw_vers -buildVersion)" \
    "$OUT/Contents/Info.plist"

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
