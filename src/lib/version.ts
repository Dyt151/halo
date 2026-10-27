/**
 * 版本号唯一来源：Tauri 运行时版本（来自 tauri.conf.json / Cargo.toml 打包元数据）。
 *
 * UI 不再硬编码版本字符串，避免 package.json / tauri.conf.json / 侧边栏 / 关于页
 * 四处不同步（评审 B6）。
 */
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

let cached: Promise<string> | null = null;

/** 读取 App 版本（进程内缓存，失败时返回空字符串由 UI 降级显示）。 */
export function appVersion(): Promise<string> {
  if (!cached) {
    cached = getVersion().catch(() => "");
  }
  return cached;
}

/** React 侧读取版本号；未就绪时返回空字符串。 */
export function useAppVersion(): string {
  const [version, setVersion] = useState("");

  useEffect(() => {
    let disposed = false;
    void appVersion().then((v) => {
      if (!disposed) setVersion(v);
    });
    return () => {
      disposed = true;
    };
  }, []);

  return version;
}
