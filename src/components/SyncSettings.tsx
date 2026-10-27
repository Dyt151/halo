/**
 * 同步设置。
 *
 * 清单 20：保存前校验地址 / 文件名 / 间隔；「立即同步」「测试连接」都带防重入。
 * 清单 8：密码与加密口令只进钥匙串，界面上永远不回填明文；
 *         输入框留空 = 不修改，显式「清除」才删除钥匙串条目。
 *
 * 与其它设置页不同，同步设置不与 `config` 自动保存合并：
 * 它要走 `set_sync_settings` 才能同时写钥匙串并重启定时器。
 */
import { useEffect, useRef, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import type { SyncConfig } from "../lib/types";
import { Btn, CardGroup, Divider, Field, Input, Notice, ToggleRow, cx } from "./ui";

/**
 * 自动同步间隔下限（与后端 `MIN_INTERVAL_SECS` 一致）。
 *
 * 不设上限：后端对 `interval_secs` 只做下限钳制，前端若额外拦一个上限，
 * 一份手工写成 7 天的配置会变成「怎么改都存不进去」的死锁 ——
 * 用户只是想改密码，却被告知间隔非法。
 */
const MIN_INTERVAL_SECS = 60;

/** 同步状态文案（与状态栏保持一致）。 */
function syncStateLabel(state?: string): string {
  switch (state) {
    case "syncing":
      return "同步中";
    case "success":
      return "已同步";
    case "error":
      return "失败";
    case "idle":
      return "待命";
    default:
      return state ?? "—";
  }
}

/**
 * 凭据输入：`null` = 不改动钥匙串，`""` = 清除，其它 = 设为新值。
 * 用三态表示，避免「留空到底是清空还是不改」的歧义。
 */
type SecretInput = string | null;

interface FormState {
  enabled: boolean;
  encryption: boolean;
  url: string;
  username: string;
  remote_file: string;
  /** 字符串形态：允许暂时为空，保存时再校验。 */
  interval: string;
}

function toForm(sync: SyncConfig): FormState {
  return {
    enabled: sync.enabled,
    encryption: sync.encryption,
    url: sync.credentials.url,
    username: sync.credentials.username,
    remote_file: sync.remote_file,
    interval: String(sync.interval_secs),
  };
}

/** 清单 20：地址 / 文件名 / 间隔三项校验，错误直接挂在对应字段下方。 */
function validate(form: FormState): Record<string, string> {
  const errors: Record<string, string> = {};
  const url = form.url.trim();

  if (form.enabled && !url) {
    errors.url = "启用同步后必须填写 WebDAV 地址";
  } else if (url && !/^https?:\/\/[^\s]+$/i.test(url)) {
    errors.url = "需以 http:// 或 https:// 开头，且不含空格";
  }

  const file = form.remote_file.trim();
  if (file && /[/\\]/.test(file)) {
    errors.remote_file = "只填文件名，不要带路径分隔符";
  }
  if (form.enabled && !file) {
    errors.remote_file = "请填写远端文件名（如 clipbase-sync.json）";
  }

  const interval = form.interval.trim();
  if (interval === "" || !/^\d+$/.test(interval)) {
    errors.interval = "请填写整数秒数，0 表示关闭自动同步";
  } else if (Number(interval) !== 0 && Number(interval) < MIN_INTERVAL_SECS) {
    errors.interval = `自动同步间隔需为 0，或不小于 ${MIN_INTERVAL_SECS} 秒`;
  }

  return errors;
}

export default function SyncSettings() {
  const config = useClipStore((s) => s.config);
  const syncStatus = useClipStore((s) => s.syncStatus);
  const syncBusy = useClipStore((s) => s.syncBusy);
  const hasPassword = useClipStore((s) => s.syncHasPassword);
  const hasPassphrase = useClipStore((s) => s.syncHasPassphrase);
  const syncNow = useClipStore((s) => s.syncNow);
  const testSync = useClipStore((s) => s.testSync);
  const setSyncSettings = useClipStore((s) => s.setSyncSettings);

  const [form, setForm] = useState<FormState | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [password, setPassword] = useState<SecretInput>(null);
  const [passphrase, setPassphrase] = useState<SecretInput>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  // 已编辑未保存时，不被外部 config 刷新覆盖（与 Settings 页同一策略）。
  // ref 供 effect 读，state 供渲染读：只留 ref 的话「有未保存的改动」不会随着
  // 改密码这种不动 form 的操作出现。
  const [dirty, setDirty] = useState(false);
  const dirtyRef = useRef(false);
  const markDirty = (value: boolean) => {
    dirtyRef.current = value;
    setDirty(value);
  };

  const sync = config?.sync;

  useEffect(() => {
    if (!dirtyRef.current && sync) setForm(toForm(sync));
  }, [sync]);

  if (!sync || !form) return null;

  const update = (patch: Partial<FormState>) => {
    markDirty(true);
    setForm({ ...form, ...patch });
    setErrors({});
  };

  const busy = saving || testing;

  const save = async () => {
    if (busy) return;
    const next = validate(form);
    // 加密只在本机派生密钥；口令为空时要到第一次同步才报错（那时云端已经是
    // 半同步状态了），宁可现在拦下。后端 `set_sync_settings` 也会再守一道。
    if (form.encryption && !hasPassphrase && !(passphrase ?? "").trim()) {
      next.passphrase = "启用端到端加密前请设置加密口令";
    }
    setErrors(next);
    if (Object.keys(next).length > 0) return;

    setSaving(true);
    try {
      await setSyncSettings({
        enabled: form.enabled,
        provider: sync.provider,
        url: form.url.trim(),
        username: form.username,
        // undefined = 沿用钥匙串里的旧密码；"" = 清除；其它 = 新密码。
        password: password === null ? undefined : password,
        encryption: form.encryption,
        passphrase: passphrase === null ? undefined : passphrase,
        remote_file: form.remote_file.trim() || "clipbase-sync.json",
        interval_secs: Number(form.interval),
      });
      markDirty(false);
      setPassword(null);
      setPassphrase(null);
      // 后端会补默认文件名、钳制间隔（如留空→clipbase-sync.json）：
      // 成功后以落库值为准回填表单，否则界面会与真实配置对不上。
      const saved = useClipStore.getState().config?.sync;
      if (saved) setForm(toForm(saved));
    } catch {
      // 错误已由 store 弹 toast；这里保留 dirty，用户可直接重试。
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    if (busy) return;
    const next = validate(form);
    // 测试连接只关心可达性：间隔不影响连通，不必拦住。
    delete next.interval;
    setErrors(next);
    if (Object.keys(next).length > 0) return;

    setTesting(true);
    try {
      await testSync(
        {
          url: form.url.trim(),
          username: form.username,
          // 空串让后端回退到钥匙串里的密码（界面从不回填明文）。
          password: password ?? "",
        },
        sync.provider,
      );
    } catch {
      // 错误已由 store 弹 toast。
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-[16px]">
      <CardGroup title="多端同步" subtitle="默认关闭，数据本地优先">
        <ToggleRow
          label="启用同步"
          checked={form.enabled}
          onChange={(v) => update({ enabled: v })}
        />
        <Divider />
        <ToggleRow
          label="端到端加密"
          hint="云端只存密文"
          checked={form.encryption}
          onChange={(v) => update({ encryption: v })}
        />
        <Divider />
        {/* 说清楚「同步什么、不同步什么」：否则用户会以为丢数据。 */}
        <div className="space-y-[4px] text-[12px]/[18px] text-white/45">
          <div>
            同步的是 <span className="text-white/70">文本 / 富文本 / 文件路径</span>{" "}
            这三类条目；删除也会同步传播（靠墓碑记录）。
          </div>
          <div>
            不传的有两类：<span className="text-white/70">敏感内容</span>
            （密码、验证码等，识别后一律不上云）、
            <span className="text-white/70">图片二进制</span>
            （只有两台机器各自有同一张图时，那条记录才会落地）。
          </div>
        </div>
      </CardGroup>

      <CardGroup title="WebDAV">
        <div className="space-y-[12px]">
          <Field label="地址">
            <Input
              value={form.url}
              onChange={(e) => update({ url: e.target.value })}
              placeholder="https://dav.example.com/remote.php/dav/files/me/clipbase"
              aria-invalid={Boolean(errors.url)}
            />
            {errors.url && <p className="mt-[4px] text-[12px] text-danger">{errors.url}</p>}
          </Field>

          <div className="grid grid-cols-2 gap-[12px]">
            <Field label="用户名">
              <Input
                value={form.username}
                onChange={(e) => update({ username: e.target.value })}
              />
            </Field>
            <Field label="密码">
              <Input
                type="password"
                value={password ?? ""}
                onChange={(e) => {
                  markDirty(true);
                  setPassword(e.target.value);
                }}
                placeholder={
                  hasPassword ? "已存入钥匙串 · 留空表示不修改" : "保存时写入系统钥匙串"
                }
                autoComplete="off"
              />
              {hasPassword && (
                <button
                  type="button"
                  onClick={() => {
                    markDirty(true);
                    setPassword("");
                  }}
                  className={cx(
                    "mt-[4px] text-[11px] hover:underline",
                    password === "" ? "text-danger" : "text-white/45",
                  )}
                >
                  {password === "" ? "保存后清除钥匙串密码" : "清除钥匙串密码"}
                </button>
              )}
            </Field>
          </div>

          <div className="grid grid-cols-2 gap-[12px]">
            <Field label="远端文件名">
              <Input
                value={form.remote_file}
                onChange={(e) => update({ remote_file: e.target.value })}
                placeholder="clipbase-sync.json"
                aria-invalid={Boolean(errors.remote_file)}
              />
              {errors.remote_file && (
                <p className="mt-[4px] text-[12px] text-danger">{errors.remote_file}</p>
              )}
            </Field>
            <Field label="自动同步间隔（秒，0 关闭）">
              <Input
                type="number"
                min={0}
                step={60}
                value={form.interval}
                onChange={(e) => update({ interval: e.target.value })}
                aria-invalid={Boolean(errors.interval)}
              />
              {errors.interval && (
                <p className="mt-[4px] text-[12px] text-danger">{errors.interval}</p>
              )}
            </Field>
          </div>

          {form.encryption && (
            <Field label="加密口令">
              <Input
                type="password"
                value={passphrase ?? ""}
                onChange={(e) => {
                  markDirty(true);
                  setPassphrase(e.target.value);
                }}
                aria-invalid={Boolean(errors.passphrase)}
                placeholder={
                  hasPassphrase
                    ? "已存入钥匙串 · 留空表示不修改"
                    : "用于派生加密密钥，保存时写入钥匙串"
                }
                autoComplete="off"
              />
              {errors.passphrase && (
                <p className="mt-[4px] text-[12px] text-danger">{errors.passphrase}</p>
              )}
              <p className="mt-[4px] text-[12px]/[16px] text-white/45">
                口令丢失将无法解密云端数据，请自行备份；更改口令后需重新同步一次。
              </p>
              {hasPassphrase && (
                <button
                  type="button"
                  onClick={() => {
                    markDirty(true);
                    setPassphrase("");
                  }}
                  className={cx(
                    "mt-[4px] text-[11px] hover:underline",
                    passphrase === "" ? "text-danger" : "text-white/45",
                  )}
                >
                  {passphrase === "" ? "保存后清除钥匙串口令" : "清除钥匙串口令"}
                </button>
              )}
            </Field>
          )}
        </div>
      </CardGroup>

      <div className="flex items-center justify-end gap-[12px]">
        {syncStatus?.last_error ? (
          <Notice tone="danger">{syncStatus.last_error}</Notice>
        ) : (
          <span className="text-[12px] text-white/45">
            {syncStatus ? `状态：${syncStateLabel(syncStatus.state)}` : "状态：—"}
            {dirty && " · 有未保存的改动"}
          </span>
        )}
        <Btn onClick={() => void test()} disabled={busy}>
          {testing ? "测试中…" : "测试连接"}
        </Btn>
        <Btn
          onClick={() => void syncNow()}
          disabled={!form.enabled || syncBusy || syncStatus?.state === "syncing"}
        >
          {syncBusy ? "同步中…" : "立即同步"}
        </Btn>
        <Btn variant="primary" onClick={() => void save()} disabled={busy}>
          {saving ? "保存中…" : "保存"}
        </Btn>
      </div>
    </div>
  );
}
