# ClipBase 模块开发规范（module-spec）

> 面向模块开发者：如何编写一个可被 ClipBase 安装、启用、调用的功能模块。

## 1. 模块包结构

一个模块 = 一个目录包，必须包含 `module.json`：

```
com.example.mymodule/
├── module.json          # 清单（必须）
├── bin/                 # sidecar 可执行文件（runtime=sidecar 时必须）
├── index.html           # UI 入口（可选）
├── assets/              # 静态资源
└── data/                # 私有数据目录（运行时隔离，仅模块可写）
```

## 2. module.json 契约（schema 1）

```jsonc
{
  "schema": 1,                              // 契约版本，破坏性变更需 bump
  "id": "com.example.mymodule",             // 全局唯一，反域名格式
  "name": "我的模块",
  "version": "0.1.0",                       // semver
  "min_core": "0.1.0",                      // 最低核心版本
  "runtime": "sidecar",                     // "iframe" | "sidecar" | "wasm"
  "entry": {
    "backend": "bin/mymodule",              // sidecar 可执行文件（相对模块目录）
    "ui": "index.html"                      // 可选 UI 入口
  },
  "permissions": [                          // 能力白名单
    "clipboard:read",
    "clipboard:write",
    "fs:read:module",
    "fs:read:user",
    "fs:write:user",
    "network:webdav"
  ],
  "contributions": [                        // 统一入口贡献点（tray 已无渲染位置，保留解析）
    { "type": "tray", "label": "我的工具", "icon": "icon.png" },
    { "type": "clipboard_action", "label": "转换此文件", "mime": ["image/*", "application/pdf"] },
    { "type": "shortcut", "label": "打开我的工具", "default": "Cmd+Shift+F" },
    { "type": "settings", "label": "我的工具设置" },
    { "type": "command", "id": "mymod.open", "label": "打开我的工具" }
  ],
  "lifecycle": { "run_on": "demand" }       // "demand"（按需）| "background"（常驻）
}
```

## 3. 运行时三级能力模型

| 级别 | 运行方式 | 隔离 | 适用场景 |
| --- | --- | --- | --- |
| L0 | sandboxed iframe + postMessage | 浏览器沙箱 | 纯前端逻辑、设置面板 |
| L1 | WASM 沙箱 | 内存沙箱（P1 预留） | 计算密集 |
| L2 | 独立 sidecar 进程 + JSON-RPC over stdio | 进程隔离 | 文件转换、右键工具等真实系统调用 |

## 4. sidecar 通信协议（JSON-RPC 2.0 over stdio）

核心以子进程启动 sidecar，stdin/stdout 每行一个 JSON 消息。

**核心 → 模块（请求）：**

```json
{ "jsonrpc": "2.0", "id": 1, "method": "module.clipboard_action", "params": { "clip": { ... } } }
```

**模块 → 核心（响应）：**

```json
{ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }
```

**模块 → 核心（反向请求，经 `CapabilityGate` 校验）：**

```json
{ "jsonrpc": "2.0", "id": 2, "method": "clipboard.write", "params": { "kind": "text", "value": "..." } }
```

支持的核心方法：

| 方法 | 所需能力 | 说明 |
| --- | --- | --- |
| `clipboard.read` | `clipboard:read` | 读取剪贴板 |
| `clipboard.write` | `clipboard:write` | 写入剪贴板 |
| `fs.read` | `fs:read:module` | 读取模块自身目录 |
| `fs.read_user` | `fs:read:user` | 读取用户目录（运行时授权） |
| `fs.write_user` | `fs:write:user` | 写入用户目录 |
| `network.request` | `network:webdav` | 网络访问 |

## 5. 权限与安全默认值

1. 安装后默认 **Disabled**，不加载任何代码。
2. 启用时只授予「manifest 声明 ∩ 用户勾选」的能力；`Network`/`FsReadUser` 需显式确认。
3. sidecar 进程崩溃只影响自身，核心不受影响（心跳监测 + 自动 `Crashed`）。
4. 模块只能读写 `~/.clipbase/modules/<id>/`。
5. 模块 UI 在 `sandbox="allow-scripts"` iframe 中渲染（无 `allow-same-origin`）。

## 6. 示例

见仓库 `modules.example/filee/`，包含 `module.json`、`bin/filee-cli`（shell 脚本）、`index.html`。
