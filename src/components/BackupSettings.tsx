/**
 * 备份与迁移（设置 → 备份与迁移）。
 *
 * 导出：把整库（历史 + 配置 + 可选图片）打包成一个目录，可拷到外接盘或新 Mac。
 * 导入：从包目录恢复历史（按 hash 幂等去重），可选一并恢复配置。
 *
 * 与同步不同，这里**不联网**、不做端到端加密，纯本机文件操作；
 * 备份含敏感内容时，请放在已加密的磁盘上。
 */
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { toast, toastError } from "../lib/toast";
import { confirm } from "../lib/dialog";
import type { ExportResult, ImportResult } from "../lib/types";
import { Btn, CardGroup, Divider, Notice, Row, ToggleRow } from "./ui";

function formatBytes(n: number): string {
  if (n >= 1024 * 1024 * 1024) return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

export default function BackupSettings() {
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [exportBusy, setExportBusy] = useState(false);
  const [importBusy, setImportBusy] = useState(false);
  const [includeFiles, setIncludeFiles] = useState(true);
  const [restoreConfig, setRestoreConfig] = useState(false);
  const [lastExport, setLastExport] = useState<ExportResult | null>(null);
  const [lastImport, setLastImport] = useState<ImportResult | null>(null);

  useEffect(() => {
    void api.appInfo().then((i) => setDataDir(i.data_dir)).catch(() => setDataDir(null));
  }, []);

  const onExport = async () => {
    setExportBusy(true);
    try {
      const dirs = await api.pickDirectory();
      const dest = dirs[0];
      if (!dest) return;
      const result = await api.exportData(dest, includeFiles);
      setLastExport(result);
      setLastImport(null);
      toast(`已导出 ${result.clips} 条历史到 ${result.path}`, { tone: "success" });
    } catch (e) {
      toastError(e, "导出失败");
    } finally {
      setExportBusy(false);
    }
  };

  const onImport = async () => {
    setImportBusy(true);
    try {
      const dirs = await api.pickDirectory();
      const path = dirs[0];
      if (!path) return;
      const ok = await confirm(
        "导入会按内容去重合并进现有历史，不会删除已有记录。确定从所选包目录导入？",
        { title: "导入备份", okLabel: "导入" },
      );
      if (!ok) return;
      const result = await api.importData(path, restoreConfig);
      setLastImport(result);
      setLastExport(null);
      const errHint = result.errors.length > 0 ? `（${result.errors.length} 条失败）` : "";
      toast(`已导入 ${result.clips} 条历史${errHint}`, { tone: result.errors.length ? "info" : "success" });
    } catch (e) {
      toastError(e, "导入失败");
    } finally {
      setImportBusy(false);
    }
  };

  return (
    <div className="space-y-[16px]">
      <CardGroup title="数据位置">
        <Row label="数据目录">
          <div className="flex items-center gap-[6px]">
            <Btn
              size="sm"
              disabled={!dataDir}
              onClick={() => {
                if (!dataDir) return;
                void api
                  .revealPath(dataDir)
                  .catch((e) => toastError(e, "打开失败"));
              }}
            >
              在 Finder 中显示
            </Btn>
          </div>
        </Row>
        {dataDir && (
          <p className="mt-[8px] break-all font-mono text-[12px]/[16px] text-white/45">{dataDir}</p>
        )}
        <Divider className="my-[8px]" />
        <Notice tone="info">备份内含敏感剪贴内容，请存到安全位置。</Notice>
      </CardGroup>

      <CardGroup title="导出备份">
        <div className="space-y-[2px]">
          <ToggleRow
            label="包含图片"
            checked={includeFiles}
            onChange={setIncludeFiles}
          />
        </div>
        <div className="mt-[12px]">
          <Btn variant="primary" disabled={exportBusy} onClick={() => void onExport()}>
            {exportBusy ? "导出中…" : "选择目录并导出"}
          </Btn>
        </div>
        {lastExport && (
          <p className="mt-[10px] text-[12px]/[16px] text-white/55">
            已导出 {lastExport.clips} 条历史
            {lastExport.files_copied > 0
              ? `，随包图片 ${lastExport.files_copied} 个（${formatBytes(lastExport.bytes)}）`
              : ""}
            。
            <br />
            包位置：<span className="font-mono text-white/45">{lastExport.path}</span>
          </p>
        )}
      </CardGroup>

      <CardGroup title="导入备份">
        <div className="space-y-[2px]">
          <ToggleRow
            label="同时恢复配置"
            checked={restoreConfig}
            onChange={setRestoreConfig}
          />
        </div>
        <div className="mt-[12px]">
          <Btn variant="primary" disabled={importBusy} onClick={() => void onImport()}>
            {importBusy ? "导入中…" : "选择包目录并导入"}
          </Btn>
        </div>
        {lastImport && (
          <p className="mt-[10px] text-[12px]/[16px] text-white/55">
            已导入 {lastImport.clips} 条
            {lastImport.skipped > 0 ? `，跳过/失败 ${lastImport.skipped} 条` : ""}。
            {lastImport.errors.length > 0 && (
              <span className="block text-white/40">明细：{lastImport.errors.join("；")}</span>
            )}
          </p>
        )}
      </CardGroup>
    </div>
  );
}
