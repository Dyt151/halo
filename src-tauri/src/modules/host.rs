//! 模块宿主：发现/安装/卸载/启用/禁用/列表/调用，及贡献点聚合。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

use super::bus::MessageBus;
use super::capabilities::{Capability, CapabilityGate};
use super::instance::{InstanceHandle, ModuleContext, ModuleInstance, ModuleSender, ModuleState};
use super::manifest::{load_manifest, Contribution, ModuleId, ModuleManifest, ModuleRuntime};
use super::sidecar::SidecarHandle;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;

/// 模块注册记录（列表展示用）。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRecord {
    pub id: ModuleId,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub grants: Vec<Capability>,
    pub installed_at: i64,
    pub state: ModuleState,
    pub manifest: ModuleManifest,
}

/// 贡献点条目（供前端/托盘聚合）。
#[derive(Debug, Clone, Serialize)]
pub struct ContributionEntry {
    pub module_id: ModuleId,
    pub module_name: String,
    pub contribution: Contribution,
}

/// 模块界面文件（`entry.ui`）的体积上限：1 MB。
///
/// 界面是整段读成字符串再交给 iframe 的，几十 MB 的「界面」只会把 webview 卡死。
const MAX_UI_BYTES: u64 = 1024 * 1024;

/// 模块宿主实现（对应架构中的 `ModuleHostApi`）。
#[derive(Clone)]
pub struct ModuleHost {
    db: Arc<Mutex<rusqlite::Connection>>,
    modules_dir: PathBuf,
    gate: Arc<CapabilityGate>,
    bus: Arc<MessageBus>,
    instances: Arc<Mutex<HashMap<ModuleId, Arc<ModuleInstance>>>>,
}

impl ModuleHost {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        modules_dir: PathBuf,
        gate: Arc<CapabilityGate>,
        bus: Arc<MessageBus>,
    ) -> Self {
        let host = Self {
            db,
            modules_dir,
            gate,
            bus,
            instances: Arc::new(Mutex::new(HashMap::new())),
        };
        // 用户可能直接把模块目录拷进 `modules/`（不走「安装」按钮选目录）。
        // 把目录里已有、库里没登记的补登记为「已安装、未启用」，否则
        // `list_modules` 永远看不到它（`discover_modules` 又没有 UI 消费，
        // 手放的模块就成了“看得见文件、界面上却没有”）。
        host.adopt_discovered();
        host
    }

    /// 把模块目录里已存在的模块补登记进数据库（未启用，等用户显式授权）。
    ///
    /// 失败一律忽略：启动路径上的 IO / 解析问题不应阻断 App 启动。
    fn adopt_discovered(&self) {
        let Ok(manifests) = self.discover() else {
            return; // 目录还没建：首次启动的常态，不是错误。
        };
        let Ok(conn) = self.db.lock() else {
            return;
        };
        for manifest in manifests {
            let known = matches!(
                conn.query_row("SELECT 1 FROM modules WHERE id = ?1", [&manifest.id], |_| Ok(()))
                    .optional(),
                Ok(Some(()))
            );
            if known {
                continue;
            }
            let Ok(json) = serde_json::to_string(&manifest) else {
                continue;
            };
            let _ = conn.execute(
                "INSERT INTO modules (id, name, version, manifest, enabled, grants, installed_at)
                 VALUES (?1, ?2, ?3, ?4, 0, '[]', ?5)",
                params![
                    manifest.id,
                    manifest.name,
                    manifest.version.to_string(),
                    json,
                    now_ms()
                ],
            );
        }
    }

    pub fn gate(&self) -> Arc<CapabilityGate> {
        self.gate.clone()
    }

    /// 模块 ID → 模块目录。
    ///
    /// 安装 / 卸载 / 启动都会拿 `id` 拼路径，而 `id` 来自清单文本、数据库、
    /// 命令参数三处，任一处被改坏都可能跳出 `modules/`（`../..`、绝对路径）。
    /// 清单解析已经拦一次，这里再拦一次：命令层不依赖「上游已经校验过」。
    fn module_dir(&self, id: &ModuleId) -> CommandResult<PathBuf> {
        if !super::manifest::valid_module_id(id) {
            return Err(AppError::with_details(4001, "模块 ID 不合法", id));
        }
        Ok(self.modules_dir.join(id))
    }

    /// 模块目录内的入口文件（sidecar 可执行文件 / iframe 页面）。
    fn entry_path(&self, id: &ModuleId, entry: &str) -> CommandResult<PathBuf> {
        if !super::manifest::safe_relative_path(entry) {
            return Err(AppError::with_details(
                4001,
                "模块入口路径不合法",
                entry,
            ));
        }
        Ok(self.module_dir(id)?.join(entry))
    }

    /// 解析模块内文件，并按**真实路径**确认它没被符号链接带出模块目录。
    ///
    /// `safe_relative_path` 只做词法校验（拦 `../` 与绝对路径）；模块包里放一个
    /// `ui.html -> ../../../../etc/passwd` 的符号链接，词法上完全合法，
    /// 读出来却是任意文件（sidecar 入口同理，那就是执行任意可执行文件）。
    fn resolve_in_module(&self, id: &ModuleId, relative: &str) -> CommandResult<PathBuf> {
        let path = self.entry_path(id, relative)?;
        let Ok(dir_real) = std::fs::canonicalize(self.module_dir(id)?) else {
            // 模块目录不存在：保持原路径，由上层报「读取失败」更贴切。
            return Ok(path);
        };
        let Ok(real) = std::fs::canonicalize(&path) else {
            // 文件不存在：同上，留给读取环节报错。
            return Ok(path);
        };
        if !real.starts_with(&dir_real) {
            return Err(AppError::with_details(
                4001,
                "模块入口路径不合法",
                format!("{relative} 指向模块目录之外"),
            ));
        }
        Ok(real)
    }

    /// 扫描模块目录，发现并解析所有 module.json。
    pub fn discover(&self) -> CommandResult<Vec<ModuleManifest>> {
        let mut manifests = Vec::new();
        let entries = std::fs::read_dir(&self.modules_dir)
            .map_err(|e| AppError::with_details(4002, "读取模块目录失败", e.to_string()))?;
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let manifest_path = entry.path().join("module.json");
            if manifest_path.exists() {
                if let Ok(manifest) = load_manifest(&manifest_path) {
                    manifests.push(manifest);
                }
            }
        }
        Ok(manifests)
    }

    /// 安装模块：校验 manifest → 复制目录 → 入库（默认禁用）。
    pub fn install(&self, src_path: &Path) -> CommandResult<ModuleManifest> {
        if !src_path.is_dir() {
            return Err(AppError::with_details(
                4001,
                "选择的不是模块目录",
                src_path.display().to_string(),
            ));
        }
        let manifest_path = src_path.join("module.json");
        if !manifest_path.exists() {
            return Err(AppError::with_details(
                4001,
                "模块包里没有 module.json",
                src_path.display().to_string(),
            ));
        }
        let manifest = load_manifest(&manifest_path)?;

        // 已安装检查。
        let conn = self.db.lock_ok();
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM modules WHERE id = ?1",
                [&manifest.id],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(AppError::with_details(
                4001,
                "该模块已经安装过了",
                &manifest.id,
            ));
        }

        let dest = self.module_dir(&manifest.id)?;
        // 源目录就是安装目标时（用户把模块直接放进 modules/ 再点「安装」）
        // 不能「先删目标再复制」：那等于把源一起删掉，后续复制必定失败，
        // 模块目录也就永久没了。这种情形直接入库。
        let src_is_dest = match (std::fs::canonicalize(src_path), std::fs::canonicalize(&dest)) {
            (Ok(a), Ok(b)) => a == b,
            _ => src_path == dest.as_path(),
        };
        if !src_is_dest {
            if dest.exists() {
                std::fs::remove_dir_all(&dest).map_err(AppError::internal)?;
            }
            if let Err(e) = copy_dir_all(src_path, &dest) {
                // 复制到一半失败（磁盘满 / 没权限）会留下半个包：清掉，
                // 否则模块目录里会多出一个「装了一半」的残留目录，
                // 用户重装时还要手动去删。
                let _ = std::fs::remove_dir_all(&dest);
                return Err(AppError::with_details(4001, "复制模块文件失败", e.to_string()));
            }
        }

        let manifest_json = serde_json::to_string(&manifest)?;
        let inserted = conn.execute(
            "INSERT INTO modules (id, name, version, manifest, enabled, grants, installed_at)
             VALUES (?1, ?2, ?3, ?4, 0, '[]', ?5)",
            params![manifest.id, manifest.name, manifest.version.to_string(), manifest_json, now_ms()],
        );
        if let Err(e) = inserted {
            // 入库失败就把刚复制过去的目录删掉：否则模块目录里会留下一个
            // 「列表里看不到、也无法卸载」的孤儿包，下次安装还会被覆盖报错。
            if !src_is_dest {
                let _ = std::fs::remove_dir_all(&dest);
            }
            return Err(AppError::internal(e));
        }

        Ok(manifest)
    }

    /// 卸载模块：停止实例 → 删除目录 → 删除记录 → 撤销授权。
    pub fn uninstall(&self, id: &ModuleId) -> CommandResult<()> {
        self.disable(id)?;
        let dir = self.module_dir(id)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(AppError::internal)?;
        }
        let conn = self.db.lock_ok();
        conn.execute("DELETE FROM modules WHERE id = ?1", [id])?;
        self.gate.revoke(id);
        Ok(())
    }

    /// 启用模块：校验授权子集 → 落库 → 门控授权 → 启动实例。
    pub async fn enable(&self, id: &ModuleId, grants: Vec<Capability>) -> CommandResult<()> {
        let manifest = self.load_installed_manifest(id)?;

        // 授权必须是 manifest 声明权限的子集。
        for g in &grants {
            if !manifest.permissions.contains(g) {
                return Err(AppError::with_details(
                    4003,
                    "模块没有声明该权限",
                    format!("模块 '{}' 未声明 '{}'", id, g.id()),
                ));
            }
        }

        let grants_json = serde_json::to_string(&grants)?;
        {
            let conn = self.db.lock_ok();
            conn.execute(
                "UPDATE modules SET enabled = 1, grants = ?1 WHERE id = ?2",
                params![grants_json, id],
            )?;
        }
        self.gate.grant(id, &grants);

        let instance = Arc::new(ModuleInstance::new(id.clone(), grants.clone()));

        match manifest.runtime {
            ModuleRuntime::Sidecar => {
                let backend = manifest.entry.backend.clone().ok_or_else(|| {
                    AppError::new(4001, "sidecar 模块缺少 entry.backend")
                })?;
                let program = self.resolve_in_module(id, &backend)?;
                let sidecar = SidecarHandle::spawn(&program, &[])
                    .await
                    .map_err(|e| AppError::with_details(4002, "模块进程启动失败", e))?;
                instance.set_handle(InstanceHandle::Sidecar(Arc::new(sidecar)));
            }
            ModuleRuntime::Iframe => instance.set_handle(InstanceHandle::Iframe),
            ModuleRuntime::Wasm => instance.set_handle(InstanceHandle::Wasm),
        }

        let ctx = ModuleContext {
            module_dir: self.module_dir(id)?,
            bus: self.bus.clone(),
            sender: ModuleSender::new(id.clone(), self.bus.clone(), self.gate.clone()),
            grants: Arc::from(grants.clone()),
        };
        instance.start(ctx)?;

        // 心跳监测：进程退出 → 标记 Crashed 并广播（不阻断核心）。
        if manifest.runtime == ModuleRuntime::Sidecar {
            let inst = instance.clone();
            let bus = self.bus.clone();
            let mid = id.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    if inst.state() == ModuleState::Stopped {
                        break;
                    }
                    let alive = inst.with_sidecar(|s| s.is_alive()).unwrap_or(false);
                    if inst.state() == ModuleState::Running && !alive {
                        inst.mark_crashed();
                        bus.publish(crate::events::MODULE_CRASHED, json!({ "id": mid }));
                        break;
                    }
                }
            });
        }

        self.instances.lock_ok().insert(id.clone(), instance);
        Ok(())
    }

    /// 禁用模块：停止实例 → 落库 → 撤销授权。
    pub fn disable(&self, id: &ModuleId) -> CommandResult<()> {
        if let Some(instance) = self.instances.lock_ok().remove(id) {
            let _ = instance.stop();
        }
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE modules SET enabled = 0 WHERE id = ?1",
            [id],
        )?;
        // 0 行 = 这个 id 根本没装：无提示地「成功」会让用户以为已经禁用/卸载了，
        // 实际库里本来就没有这条记录。
        if updated == 0 {
            return Err(AppError::with_details(4002, "模块未安装", id));
        }
        self.gate.revoke(id);
        Ok(())
    }

    /// 列出已安装模块。
    pub fn list(&self) -> CommandResult<Vec<ModuleRecord>> {
        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare(
            "SELECT id, name, version, manifest, enabled, grants, installed_at FROM modules ORDER BY installed_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let version: String = row.get(2)?;
            let manifest_json: String = row.get(3)?;
            let enabled: i64 = row.get(4)?;
            let grants_json: String = row.get(5)?;
            let installed_at: i64 = row.get(6)?;
            Ok((id, name, version, manifest_json, enabled != 0, grants_json, installed_at))
        })?;

        let instances = self.instances.lock_ok();
        let mut records = Vec::new();
        for row in rows {
            let (id, name, version, manifest_json, enabled, grants_json, installed_at) = row?;
            let manifest: ModuleManifest = serde_json::from_str(&manifest_json)
                .map_err(|e| AppError::with_details(4001, "模块清单损坏", e.to_string()))?;
            let grants: Vec<Capability> = serde_json::from_str(&grants_json).unwrap_or_default();
            // 未加载的模块统一呈现为已停止；实际运行态由实例表决定。
            let state = instances.get(&id).map(|i| i.state()).unwrap_or(ModuleState::Stopped);
            records.push(ModuleRecord {
                id,
                name,
                version,
                enabled,
                grants,
                installed_at,
                state,
                manifest,
            });
        }
        Ok(records)
    }

    /// 调用模块（sidecar 经 JSON-RPC，iframe/wasm 返回说明）。
    pub async fn invoke_method(
        &self,
        id: &ModuleId,
        method: &str,
        params: Value,
    ) -> CommandResult<Value> {
        // 方法名会原样发给 sidecar：空串/超长的名字只会在对面换来一句
        // 看不懂的解析失败，不如在边界上直接拒掉。
        if method.is_empty()
            || method.len() > 64
            || !method
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
        {
            return Err(AppError::with_details(4001, "模块方法名不合法", method));
        }
        let instance = self
            .instances
            .lock_ok()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::with_details(4002, "模块未运行", id))?;

        if instance.state() == ModuleState::Crashed {
            return Err(AppError::with_details(4002, "模块已崩溃", id));
        }

        // 能力门控：方法 → 能力。
        if let Some(cap) = Capability::required_for_method(method) {
            if !self.gate.check(id, cap) {
                return Err(AppError::with_details(
                    4003,
                    "模块没有获得该权限",
                    format!("模块 '{}' 缺少 '{}'", id, cap.id()),
                ));
            }
        }

        match instance.sidecar_handle() {
            Some(sidecar) => sidecar
                .request(method, params)
                .await
                .map_err(|e| AppError::with_details(4002, "模块请求失败", e)),
            None => Err(AppError::with_details(
                4004,
                "该模块不支持后端调用",
                "iframe 模块由前端沙箱直接调用，没有后端进程",
            )),
        }
    }

    /// 向运行中的 sidecar 模块广播事件通知（JSON-RPC notification）。
    pub fn broadcast(&self, topic: &str, payload: Value) {
        let handles: Vec<Arc<SidecarHandle>> = {
            let instances = self.instances.lock_ok();
            instances
                .values()
                .filter(|i| i.state() == ModuleState::Running)
                .filter_map(|i| i.sidecar_handle())
                .collect()
        };
        for handle in handles {
            let topic = topic.to_string();
            let payload = payload.clone();
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let params = json!({ "topic": topic, "data": payload });
                let _ = handle.request("module.event", params).await;
            });
        }
    }

    /// 读取模块 UI 入口内容（供前端 iframe 沙箱渲染）。
    pub fn read_module_ui(&self, id: &ModuleId) -> CommandResult<String> {
        let manifest = self.load_installed_manifest(id)?;
        let ui = manifest
            .entry
            .ui
            .ok_or_else(|| AppError::new(4004, "该模块没有界面入口（entry.ui）"))?;
        let path = self.resolve_in_module(id, &ui)?;
        // 体积闸门：界面是整段 HTML 字符串，几十 MB 的「界面」只会把 webview 卡死。
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > MAX_UI_BYTES {
                return Err(AppError::with_details(
                    4004,
                    "模块界面文件太大",
                    format!(
                        "{} KB，上限 {} KB",
                        meta.len() / 1024,
                        MAX_UI_BYTES / 1024
                    ),
                ));
            }
        }
        std::fs::read_to_string(&path)
            .map_err(|e| AppError::with_details(4004, "读取模块界面失败", e.to_string()))
    }

    /// 聚合所有已启用模块的贡献点。
    pub fn list_contributions(&self) -> CommandResult<Vec<ContributionEntry>> {
        let records = self.list()?;
        let mut out = Vec::new();
        for record in records {
            if !record.enabled {
                continue;
            }
            for contribution in &record.manifest.contributions {
                out.push(ContributionEntry {
                    module_id: record.id.clone(),
                    module_name: record.name.clone(),
                    contribution: contribution.clone(),
                });
            }
        }
        Ok(out)
    }

    /// 从 DB 加载已安装模块的 manifest。
    fn load_installed_manifest(&self, id: &ModuleId) -> CommandResult<ModuleManifest> {
        let conn = self.db.lock_ok();
        let json: Option<String> = conn
            .query_row("SELECT manifest FROM modules WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        let json = json.ok_or_else(|| AppError::with_details(4002, "模块未安装", id))?;
        serde_json::from_str(&json)
            .map_err(|e| AppError::with_details(4001, "模块清单损坏", e.to_string()))
    }
}

/// 递归复制目录（安装模块包用）。
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-modhost-{}-{name}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest_json(id: &str) -> String {
        // 用 json! 构造：手写 format! 的 `{{` / `}}` 转义极易数错。
        serde_json::json!({
            "schema": 1,
            "id": id,
            "name": "测试模块",
            "version": "0.1.0",
            "min_core": "0.1.0",
            "runtime": "sidecar",
            "entry": { "backend": "bin/x" },
            "permissions": [],
            "contributions": [],
            "lifecycle": { "run_on": "demand" }
        })
        .to_string()
    }

    fn host_at(root: &Path) -> (ModuleHost, PathBuf) {
        let modules_dir = root.join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();
        let db = crate::storage::db::open(&root.join("data").join("clipbase.db")).unwrap();
        let host = ModuleHost::new(
            Arc::new(Mutex::new(db)),
            modules_dir.clone(),
            Arc::new(CapabilityGate::new()),
            Arc::new(MessageBus::new()),
        );
        (host, modules_dir)
    }

    /// 手放进 `modules/` 的模块要在下次启动时自动登记，否则界面永远看不到它。
    #[test]
    fn modules_dropped_into_the_dir_are_adopted() {
        let root = scratch("adopt");
        let pkg = root.join("modules").join("com.clipbase.demo");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("module.json"), manifest_json("com.clipbase.demo")).unwrap();

        let (host, _) = host_at(&root);
        let list = host.list().unwrap();
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].id, "com.clipbase.demo");
        // 自动登记必须是「未启用」：不能绕过授权直接跑起来。
        assert!(!list[0].enabled);
        assert!(list[0].grants.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 源目录就是安装目标时，不能「先删目标再复制」——那等于把唯一的一份源删了。
    #[test]
    fn install_from_the_target_dir_does_not_delete_it() {
        let root = scratch("in-place");
        let (host, modules_dir) = host_at(&root);
        // host 起来之后才放模块，模拟运行中手放（启动时 adopt 看不到）。
        let pkg = modules_dir.join("com.clipbase.late");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("module.json"), manifest_json("com.clipbase.late")).unwrap();

        host.install(&pkg).expect("原地安装应当成功");
        assert!(pkg.join("module.json").exists(), "源目录被删掉了");
        assert_eq!(host.list().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }
}
