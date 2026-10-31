#!/usr/bin/env python3
"""把 iCopy 的剪贴板历史 / 快捷回复提取成 ClipBase 可导入的 JSON。

iCopy 的数据布局（逆向结论，见 docs/REA-iCopy-Filee.md §1.3）：

- `data.sqlite` 表 `t_data(id, pastetype, sourcetype, text, filepath, createtime, data, favorite)`
  - `pastetype`：0 = 纯文本，1 = 富文本，2 = 文件/图片
  - `sourcetype`：粘贴板 UTI（public.utf8-plain-text / public.html / public.png …）
  - `data`：iCopy 自己的 `PasteModel`（NSKeyedArchiver），比表列多出
    `sourceAppName`（来源 App 名）与 `richText`（NSAttributedString），
    富文本的 HTML 原文藏在 `data` 字段里：它是 **Apple LZFSE 压缩流**
    （Compression 框架，`bvx` 开头，详见 docs/HANDOFF-2026-10-26.md §3），
    需经 libcompression 解压后再解析
  - `filepath`：iCopy 容器内的图片/文件绝对路径

本脚本只负责「读 iCopy + 解归档 + 复制图片文件」，写库由 ClipBase 侧
`clipbase_import` 二进制完成（保证 hash / preview / 序列化与 App 内部完全一致）。
快捷回复功能已下线，iCopy 的 `reply.sqlite` 不再迁移。

用法：
    python3 scripts/import_icopy.py            # 生成 ~/.clipbase/data/import/icopy.json
    python3 scripts/import_icopy.py --dry-run  # 只统计不写文件
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import plistlib
import shutil
import sqlite3
import struct
import sys
from pathlib import Path

ICOPY_CONTAINER = Path(
    "~/Library/Containers/cn.better365.iCopy/Data/Documents"
).expanduser()
CLIPBASE_HOME = Path("~/.clipbase").expanduser()
CLIPBASE_FILES = CLIPBASE_HOME / "data" / "files"
CLIPBASE_IMPORT_DIR = CLIPBASE_HOME / "data" / "import"

# 判定为「图片」的 UTI 前缀；其余 pastetype=2 视为文件列表。
IMAGE_UTIS = {"public.png", "public.tiff", "public.jpeg", "com.compuserve.gif"}


def resolve(objs, value, depth=0):
    """解析 NSKeyedArchiver 的 UID 引用链。"""
    while isinstance(value, plistlib.UID) and depth < 8:
        value = objs[value.data]
        depth += 1
    return value


def decode_model(blob: bytes) -> dict:
    """解 iCopy 的 PasteModel 归档；失败时返回空 dict。"""
    if not blob or blob[:8] != b"bplist00":
        return {}
    try:
        plist = plistlib.loads(blob)
    except Exception:
        return {}
    objs = plist.get("$objects") or []
    if len(objs) < 2 or not isinstance(objs[1], dict):
        return {}
    root = objs[1]
    out = {}
    for key in (
        "type",
        "text",
        "filePath",
        "sourceType",
        "sourceAppName",
        "favorite",
        "createTime",
        "size",
        "data",
        "richText",
    ):
        if key in root:
            out[key] = resolve(objs, root[key])
    return out


def extract_html(data) -> str | None:
    """从 iCopy 的富文本数据里取出 HTML 原文（向后兼容的薄封装）。

    `data` 是 Apple LZFSE 压缩流（Compression 框架，`bvx` 开头的分块格式，
    见 docs/HANDOFF-2026-10-26.md §3），先经 libcompression 解压再解析。
    解压结果可能是 HTML 或 RTF（`{\\rtf` 开头，本脚本回落纯文本）。

    仅当 HTML 含有实际正文（去标签后非空、非空壳）时才返回，否则交由
    调用方降级为纯文本导入（内容不丢）。
    """
    return extract_html_from_payload(decode_payload(data))


def decode_payload(data) -> bytes | None:
    """解压 iCopy 富文本载荷；非 `bvx` 数据（裸 HTML/RTF）原样返回。"""
    if not isinstance(data, (bytes, bytearray)):
        return None
    raw = bytes(data)
    if raw.startswith(b"bvx"):
        return lzfse_decompress(raw)
    return raw


def extract_html_from_payload(payload: bytes | None) -> str | None:
    """从解压后的载荷里提取 HTML；RTF / 非 HTML / 空壳一律返回 None。"""
    if not payload:
        return None
    if payload.lstrip().startswith(b"{"):
        # RTF 等非 HTML 载荷：回落纯文本（P1 可用 textutil 转 HTML）。
        return None
    idx = payload.find(b"<")
    if idx < 0:
        return None
    try:
        text = payload[idx:].decode("utf-8", errors="replace")
    except UnicodeDecodeError:
        return None
    # 结尾若带二进制噪声，按最后一个 '>' 截断。
    if ">" in text:
        text = text[: text.rindex(">") + 1]
    if not _is_usable_html(text):
        return None
    return text


# LZFSE 解压：直接调 macOS libcompression，零第三方依赖。
COMPRESSION_LZFSE = 0x801
_LZFSE_LIB: ctypes.CDLL | None = None
_LZFSE_READY = False


def _lzfse_lib() -> ctypes.CDLL | None:
    global _LZFSE_LIB, _LZFSE_READY
    if _LZFSE_READY:
        return _LZFSE_LIB
    _LZFSE_READY = True
    if sys.platform != "darwin":
        return None
    try:
        lib = ctypes.CDLL("/usr/lib/libcompression.dylib")
        lib.compression_decode_buffer.restype = ctypes.c_size_t
        lib.compression_decode_buffer.argtypes = [
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_int,
        ]
        _LZFSE_LIB = lib
    except OSError:
        _LZFSE_LIB = None
    return _LZFSE_LIB


def lzfse_decompress(raw: bytes) -> bytes | None:
    """解 Apple LZFSE 流；失败返回 None。"""
    if not raw.startswith(b"bvx") or len(raw) < 8:
        return None
    lib = _lzfse_lib()
    if lib is None:
        return None
    hint = struct.unpack("<I", raw[4:8])[0]  # 首块头携带原始长度
    for cap in (hint + 8, max(hint * 4 + 4096, 1 << 16), len(raw) * 32 + (1 << 16)):
        src = ctypes.create_string_buffer(raw, len(raw))
        dst = ctypes.create_string_buffer(cap)
        n = lib.compression_decode_buffer(dst, cap, src, len(raw), None, COMPRESSION_LZFSE)
        if n:
            return dst.raw[:n]
    return None


def _is_usable_html(text: str) -> bool:
    """HTML 必须有实际正文：非空壳（如仅 `<meta charset='utf-8'>`）、
    无控制字符、无替换字符。"""
    if not text or len(text) <= 32 or "�" in text:
        return False
    for ch in text:
        code = ord(ch)
        if code < 0x20 and ch not in "\t\n\r":
            return False
    return bool(_strip_tags(text).strip())


def _strip_tags(html: str) -> str:
    """极简 HTML 去标签（只用于校验正文非空，不做真实渲染）。"""
    out = []
    in_tag = False
    for c in html:
        if c == "<":
            in_tag = True
        elif c == ">":
            in_tag = False
            out.append(" ")
        elif not in_tag:
            out.append(c)
    return "".join(out)


def png_size(path: Path) -> tuple[int, int]:
    """读 PNG 宽高（IHDR）；失败返回 (0, 0)。"""
    try:
        with path.open("rb") as f:
            head = f.read(24)
        if len(head) < 24 or head[12:16] != b"IHDR":
            return (0, 0)
        w, h = struct.unpack(">II", head[16:24])
        return (int(w), int(h))
    except Exception:
        return (0, 0)


def copy_asset(src: Path, tag: str) -> Path | None:
    """把 iCopy 容器里的图片/文件复制到 ClipBase 的 files 目录。"""
    if not src or not src.is_file():
        return None
    CLIPBASE_FILES.mkdir(parents=True, exist_ok=True)
    suffix = src.suffix or ".bin"
    dest = CLIPBASE_FILES / f"icopy_{tag}{suffix}"
    shutil.copy2(src, dest)
    return dest


def build_clips(dry_run: bool) -> tuple[list[dict], dict]:
    db = ICOPY_CONTAINER / "data.sqlite"
    if not db.is_file():
        sys.exit(f"找不到 iCopy 数据库：{db}")
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    items: list[dict] = []
    stats = {"text": 0, "html": 0, "rtf": 0, "image": 0, "files": 0, "skipped": 0}

    rows = con.execute(
        "select id, pastetype, sourcetype, text, filepath, createtime, data, favorite "
        "from t_data order by id"
    )
    for (rid, pastetype, uti, text, filepath, createtime, blob, favorite) in rows:
        model = decode_model(bytes(blob) if blob else b"")
        source_app = model.get("sourceAppName")
        if not isinstance(source_app, str) or not source_app:
            source_app = None
        created_at = model.get("createTime")
        if not isinstance(created_at, (int, float)):
            created_at = createtime or 0
        created_ms = int(float(created_at) * 1000)

        content = None
        if pastetype == 0:
            body = model.get("text")
            if not isinstance(body, str):
                body = text or ""
            content = {"kind": "text", "value": body}
            stats["text"] += 1
        elif pastetype == 1:
            payload = decode_payload(model.get("data"))
            html = extract_html_from_payload(payload)
            body = model.get("text")
            if not isinstance(body, str):
                body = text or ""
            if html:
                content = {"kind": "html", "value": html}
                stats["html"] += 1
            elif payload and payload.lstrip().startswith(b"{"):
                # RTF 载荷：回落纯文本（转 HTML 留待 P1 的 textutil 方案）。
                stats["rtf"] += 1
                if body:
                    content = {"kind": "text", "value": body}
                    stats["text"] += 1
            elif body:
                content = {"kind": "text", "value": body}
                stats["text"] += 1
        elif pastetype == 2:
            src = Path(filepath) if filepath else None
            if src and src.is_file():
                if not dry_run:
                    dest = copy_asset(src, f"{rid}")
                else:
                    dest = CLIPBASE_FILES / f"icopy_{rid}{src.suffix or '.bin'}"
                if dest is None:
                    stats["skipped"] += 1
                    continue
                if uti in IMAGE_UTIS:
                    w, h = png_size(dest) if not dry_run else (0, 0)
                    content = {
                        "kind": "image",
                        "value": {"path": str(dest), "width": w, "height": h},
                    }
                    stats["image"] += 1
                else:
                    content = {"kind": "files", "value": [str(dest)]}
                    stats["files"] += 1
        if content is None:
            stats["skipped"] += 1
            continue

        items.append(
            {
                "content": content,
                "source_app": source_app,
                "created_at": created_ms,
                "favorite": bool(favorite or model.get("favorite") or False),
                "tag": None,
            }
        )
    con.close()
    return items, stats


def main() -> int:
    parser = argparse.ArgumentParser(description="从 iCopy 提取历史到 ClipBase 导入文件")
    parser.add_argument("--dry-run", action="store_true", help="只统计，不写文件")
    args = parser.parse_args()

    clips, stats = build_clips(args.dry_run)

    print("iCopy → ClipBase 提取结果")
    print(f"  纯文本 : {stats['text']}")
    print(f"  富文本 : {stats['html']}")
    print(f"  RTF回落: {stats['rtf']}")
    print(f"  图片   : {stats['image']}")
    print(f"  文件   : {stats['files']}")
    print(f"  跳过   : {stats['skipped']}")
    print(f"  合计   : {len(clips)}")

    if args.dry_run:
        return 0

    CLIPBASE_IMPORT_DIR.mkdir(parents=True, exist_ok=True)
    out = CLIPBASE_IMPORT_DIR / "icopy.json"
    with out.open("w", encoding="utf-8") as f:
        json.dump({"clips": clips}, f, ensure_ascii=False)
    print(f"\n已写入 {out}")
    print("下一步执行：cargo run --bin clipbase_import -- "
          f"{CLIPBASE_IMPORT_DIR / 'icopy.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
