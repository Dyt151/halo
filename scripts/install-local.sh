#!/usr/bin/env bash
# 本机安装：把构建产物装进 /Applications/开源软件，并按正确顺序重签。
#
# 为什么需要这个脚本：
# Finder 扩展（.appex）必须带着 com.apple.security.app-sandbox 才能被
# pluginkit 注册。任何一次对宿主 App 的 `codesign --deep --force` 就会
# 用不带 entitlements 的签名覆盖掉扩展，表现为「Finder 扩展未启用」但
# 界面上没有任何错误。这里先签 appex、再签宿主，并在结尾校验。
#
# 机器上只允许有一份 Halo.app：装在 /Applications/开源软件，每次安装先把
# 旧的那份退掉、注销、删掉。构建产物（target/release/bundle/macos）原地
# 留着不动，只从 LaunchServices 注销，否则启动台里会数出第二个、第三个 Halo。
#
# 用法：
#   npm run build:app && npx tauri build
#   scripts/install-local.sh
# 可选环境变量：
#   CODESIGN_IDENTITY  签名身份，默认 ad-hoc（"-"）
#   HALO_INSTALL_DIR   安装目录，默认 /Applications/开源软件
#   RESTART_FINDER=1   安装后自动重启 Finder
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_APP="$ROOT/src-tauri/target/release/bundle/macos/Halo.app"
DEST_DIR="${HALO_INSTALL_DIR:-/Applications/开源软件}"
DEST_APP="$DEST_DIR/Halo.app"
EXT_REL="Contents/PlugIns/HaloFinderExtension.appex"
IDENTITY="${CODESIGN_IDENTITY:--}"
APP_BUNDLE_ID="com.halo.desktop"
EXT_BUNDLE_ID="com.halo.desktop.FinderExtension"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

[ -d "$SRC_APP" ] || {
    echo "找不到 $SRC_APP，先运行 npx tauri build" >&2
    exit 1
}

echo "==> 清掉别处的旧 Halo.app（只留 $DEST_APP）"
# 构建产物别让 Spotlight 收进去：没有这个标记，聚焦搜索会把 target 里那份
# 当成第三个 Halo 列出来（LaunchServices 注销挡不住它，它读的是索引）。
touch "$ROOT/src-tauri/target/.metadata_never_index"
if pgrep -x halo >/dev/null; then
    osascript -e 'tell application "Halo" to quit' 2>/dev/null || true
    sleep 2
    pkill -x halo 2>/dev/null || true
fi
/usr/bin/mdfind "kMDItemCFBundleIdentifier == '$APP_BUNDLE_ID'" 2>/dev/null | while IFS= read -r stale; do
    [ "$stale" = "$DEST_APP" ] && continue
    "$LSREGISTER" -u "$stale" 2>/dev/null || true
    if [ "$stale" = "$SRC_APP" ]; then
        continue # 构建产物不删，只注销
    fi
    case "$stale" in
        /Applications/Halo.app | /Applications/*/Halo.app)
            echo "    删除 $stale"
            rm -rf "$stale"
            ;;
        *)
            echo "    $stale 不是安装位置，只注销不删"
            ;;
    esac
done

mkdir -p "$DEST_DIR"
echo "==> 安装到 $DEST_APP"
rm -rf "$DEST_APP"
ditto "$SRC_APP" "$DEST_APP"

sign_args=(--force --sign "$IDENTITY")
if [ "$IDENTITY" != "-" ]; then
    sign_args+=(--options runtime --timestamp)
fi

echo "==> 重签 Finder 扩展（带沙盒 entitlement）"
codesign "${sign_args[@]}" \
    --entitlements "$ROOT/src-tauri/finder-extension/entitlements.plist" \
    "$DEST_APP/$EXT_REL"

echo "==> 重签宿主 App（带宿主 entitlement）"
codesign "${sign_args[@]}" \
    --entitlements "$ROOT/src-tauri/entitlements.plist" \
    "$DEST_APP"

codesign --verify --deep --strict "$DEST_APP"

echo "==> 校验 entitlement"
if ! codesign -d --entitlements - "$DEST_APP/$EXT_REL" 2>/dev/null | grep -q "com.apple.security.app-sandbox"; then
    echo "Finder 扩展缺少 app-sandbox entitlement" >&2
    exit 1
fi

echo "==> 注册扩展"
"$LSREGISTER" -f "$DEST_APP"
/usr/bin/pluginkit -a "$DEST_APP/$EXT_REL"
/usr/bin/pluginkit -e use -i "$EXT_BUNDLE_ID"

if ! /usr/bin/pluginkit -m -i "$EXT_BUNDLE_ID" | grep -q '^+'; then
    echo "pluginkit 未把扩展标记为启用：" >&2
    /usr/bin/pluginkit -mAvvv -i "$EXT_BUNDLE_ID" >&2 || true
    exit 1
fi

# 装好了才删构建产物：聚焦搜索和启动台都会把 target 里那份 Halo.app 数成第二个，
# 光注销挡不住。下次 npx tauri build 会重新打出来（cargo 的编译缓存没动，只是重新打包）。
"$LSREGISTER" -u "$SRC_APP" 2>/dev/null || true
rm -rf "$SRC_APP"

echo "==> 完成"
echo "退出并重新打开 Halo，Finder 右键菜单即可生效。"
if [ "${RESTART_FINDER:-0}" = "1" ]; then
    killall Finder || true
    echo "Finder 已重启。"
fi
