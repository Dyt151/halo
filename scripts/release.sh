#!/usr/bin/env bash
# 发布新版本：构建签名更新包并上传到 GitHub Releases（自动更新源）。
#
# 前置（一次性）：
#   1. `gh auth login`（本机已有 gh）
#   2. Xcode 生成代码签名证书，tauri.conf.json 的 signingIdentity 指向它
#      （macOS 更新器拒绝 ad-hoc 签名的包，详见 docs/ 或 tauri updater 文档）
#   3. 私钥在 ~/.tauri/halo-updater.key（`npx tauri signer generate` 生成，
#      对应的 pubkey 已写入 tauri.conf.json plugins.updater.pubkey）
#
# 用法：scripts/release.sh <版本号> [发布说明]
# 之后把三处版本号改动 commit 并 push 即可。
set -euo pipefail

VERSION="${1:?用法: scripts/release.sh 0.2.0 [发布说明]}"
NOTES="${2:-v$VERSION}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# 三处版本号保持一致（updater 以 tauri.conf.json 为准）。
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" src-tauri/Cargo.toml
sed -i '' "s/\"version\": \"[0-9][^\"]*\"/\"version\": \"$VERSION\"/" \
    src-tauri/tauri.conf.json package.json

# 构建（含 updater 签名产物：Halo.app.tar.gz + .sig）。
TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/halo-updater.key" npx tauri build

BUNDLE="src-tauri/target/release/bundle/macos"
APP_TGZ="$BUNDLE/Halo.app.tar.gz"
SIG="$BUNDLE/Halo.app.tar.gz.sig"
[ -f "$APP_TGZ" ] && [ -f "$SIG" ] || { echo "更新产物缺失：$BUNDLE"; exit 1; }

# 生成 latest.json：GitHub Releases 的静态 manifest，
# endpoint `releases/latest/download/latest.json` 永远指向最新稳定版。
python3 - "$VERSION" "$SIG" <<'PY'
import datetime, json, sys
version, sig_path = sys.argv[1], sys.argv[2]
manifest = {
    "version": version,
    "notes": f"v{version}",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {
        # 目标三元组 key 与 tauri-plugin-updater 的 target() 一致（macOS arm64）。
        "darwin-aarch64": {
            "signature": open(sig_path).read().strip(),
            "url": f"https://github.com/Dyt151/halo/releases/download/v{version}/Halo.app.tar.gz",
        }
    },
}
open("latest.json", "w").write(json.dumps(manifest, indent=2) + "\n")
print(f"latest.json 已生成（v{version}）")
PY

gh release create "v$VERSION" \
    --title "v$VERSION" \
    --notes "$NOTES" \
    "$APP_TGZ" "latest.json"

echo "发布完成。已安装的 App 下次启动（或「设置 → 关于 → 检查更新」）会发现 v$VERSION。"
echo "别忘了 commit 版本号改动（Cargo.toml / tauri.conf.json / package.json / Cargo.lock）并 push。"
