//! 配置服务：从 SQLite `config` 表读取/写入，缺省回退默认值。

pub mod settings;

use std::sync::{Arc, Mutex, RwLock};

use crate::error::{AppError, CommandResult};
use crate::lock_ext::{LockExt, RwLockExt};
use rusqlite::OptionalExtension;
use settings::Config;

const CONFIG_KEY: &str = "app_config";
const MIGRATION_MARKER_KEY: &str = "migrated_v6_defaults";
/// v7：格式裁剪后刷新出厂盘的扩展名（补 heic，去掉 md/doc/odt/rtf/mhtml）。
const RING_FORMATS_MARKER_KEY: &str = "migrated_v7_ring_formats";
/// v8：状态栏面板新增网络/蓝牙，旧配置的模块列表里补齐这两项（只跑一次，
/// 用户之后自己取消勾选不会被加回来）。
const STATUSBAR_NETWORK_MARKER_KEY: &str = "migrated_v8_statusbar_network";
/// v9：状态栏面板新增 VPN（代理），补齐到网络后面。
const STATUSBAR_VPN_MARKER_KEY: &str = "migrated_v9_statusbar_vpn";

/// 丢掉指向已删除格式的预设（ico / qoi / md / odt …）。
///
/// `TargetSpec` 对未知格式直接反序列化失败，整份 `Config` 就会解析失败，
/// `ConfigService::new` 随即回落到默认值——用户的快捷键、容量、盘序全被重置。
/// 所以在 `from_value` 之前先按 JSON 过滤：只放弃那条失效预设，其余配置保住。
fn sanitize_removed_presets(value: &mut serde_json::Value) {
    use crate::core::convert::ConvertFormat;

    let Some(presets) = value
        .get_mut("conversion")
        .and_then(|c| c.get_mut("presets"))
        .and_then(|p| p.as_array_mut())
    else {
        return;
    };
    presets.retain(|preset| {
        let target_ok = match preset.get("target").and_then(|t| t.as_str()) {
            Some("*") | None => true,
            Some(raw) => ConvertFormat::from_ext(raw).is_some(),
        };
        let pages_ok = preset
            .get("kind")
            .and_then(|k| k.get("pdf_pages"))
            .and_then(|p| p.get("to"))
            .and_then(|t| t.as_str())
            .map(|to| ConvertFormat::from_ext(to).is_some())
            .unwrap_or(true);
        target_ok && pages_ok
    });
}

/// v6 一次性默认值迁移：旧默认「唤起面板 Control+C」「敏感策略 ignore」改成新默认
/// （Control+Option+C / mask）。只在标记键不存在时执行一次——`Config::migrate()`
/// 每次加载都跑，这种会覆盖用户有意选择的重写不能放进去。迁移过就写标记键，
/// 用户之后自己改回 ⌃C / ignore 也原样保留。
///
/// 敏感策略迁移到 `Mask`（脱敏入库）而不是 `Flag`：老用户若沿用旧的 `ignore`，
/// 静默丢条目的代价大于多打一个标记，但明文落盘的代价又比两者都大——
/// 脱敏入库既不丢条目，也不留明文。
///
/// v7 一次性刷写出厂盘的扩展名表：旧配置的「图片」盘可能是在 heic 加入之前
/// 建的，扩展名里没有 `heic`，拖 HEIC 会掉进兜底盘、找不到 HEIC 动作；同时
/// 清掉已删除格式（md / doc / odt / rtf / mhtml）的残留。只动四个出厂 id，
/// 自建盘保持原样。
fn migrate_user_defaults(
    conn: &rusqlite::Connection,
    config: &mut Config,
) -> Result<bool, rusqlite::Error> {
    let mut changed = false;

    if !marker_set(conn, MIGRATION_MARKER_KEY)? {
        if config.hotkeys.toggle == "Control+C" {
            config.hotkeys.toggle = settings::default_toggle_hotkey();
            changed = true;
        }
        if config.sensitivity == settings::SensitivityPolicy::Ignore {
            config.sensitivity = settings::SensitivityPolicy::Mask;
            changed = true;
        }
        if changed {
            persist_config(conn, config)?;
        }
        set_marker(conn, MIGRATION_MARKER_KEY)?;
    }

    if !marker_set(conn, RING_FORMATS_MARKER_KEY)? {
        let mut refreshed = false;
        for profile in &mut config.ring.profiles {
            if let Some(stock) = settings::stock_extensions(&profile.id) {
                if profile.extensions != stock {
                    profile.extensions = stock;
                    refreshed = true;
                }
            }
        }
        if refreshed {
            persist_config(conn, config)?;
            changed = true;
        }
        set_marker(conn, RING_FORMATS_MARKER_KEY)?;
    }

    if !marker_set(conn, STATUSBAR_NETWORK_MARKER_KEY)? {
        // 空列表视为用户主动清空，不硬塞新模块。
        if !config.statusbar.sections.is_empty() {
            let mut sections = Vec::with_capacity(config.statusbar.sections.len() + 2);
            let mut inserted = false;
            for id in &config.statusbar.sections {
                sections.push(id.clone());
                if !inserted && id == "battery" {
                    sections.push("network".to_string());
                    sections.push("bluetooth".to_string());
                    inserted = true;
                }
            }
            if !inserted {
                sections.splice(0..0, ["network".to_string(), "bluetooth".to_string()]);
            }
            let mut seen = std::collections::HashSet::new();
            sections.retain(|id| seen.insert(id.clone()));
            if sections != config.statusbar.sections {
                config.statusbar.sections = sections;
                config.statusbar.normalize();
                persist_config(conn, config)?;
                changed = true;
            }
        }
        set_marker(conn, STATUSBAR_NETWORK_MARKER_KEY)?;
    }

    if !marker_set(conn, STATUSBAR_VPN_MARKER_KEY)? {
        // 只补给「还留着网络模块」的旧配置：列表为空或网络被用户取消勾选，
        // 都当成主动选择，不硬塞新模块。
        let network_at = config
            .statusbar
            .sections
            .iter()
            .position(|id| id == "network");
        let has_vpn = config.statusbar.sections.iter().any(|id| id == "vpn");
        if let Some(index) = network_at.filter(|_| !has_vpn) {
            config
                .statusbar
                .sections
                .insert(index + 1, "vpn".to_string());
            config.statusbar.normalize();
            persist_config(conn, config)?;
            changed = true;
        }
        set_marker(conn, STATUSBAR_VPN_MARKER_KEY)?;
    }

    Ok(changed)
}

/// 一次性迁移标记键是否存在。
fn marker_set(conn: &rusqlite::Connection, key: &str) -> Result<bool, rusqlite::Error> {
    Ok(conn
        .query_row("SELECT value FROM config WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .is_some())
}

fn set_marker(conn: &rusqlite::Connection, key: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, "1"],
    )?;
    Ok(())
}

fn persist_config(conn: &rusqlite::Connection, config: &Config) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_string(config).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "serialize migrated config",
            )),
        )
    })?;
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![CONFIG_KEY, json],
    )?;
    Ok(())
}

/// 配置服务。内部以读写锁缓存当前配置，写操作同时落库。
#[derive(Clone)]
pub struct ConfigService {
    db: Arc<Mutex<rusqlite::Connection>>,
    cache: Arc<RwLock<Config>>,
    /// 写串行化锁：`patch` 是「读-改-写」，两个并发 patch（例如托盘切监控与
    /// 设置页改字号同时发生）不加锁会互相丢改动（评审 F3）。
    write_lock: Arc<Mutex<()>>,
}

impl ConfigService {
    /// 使用共享数据库连接构建（与其他服务共享同一连接）。
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        // 从库中加载配置，失败则使用默认值。
        //
        // 不能默默吞掉失败：默认值会让快捷键/主题/容量全部回到出厂状态，
        // 用户只会看到「我的设置被重置了」而不知道原因。这里是唯一的诊断入口。
        let loaded = match Self::load_from_db(&db) {
            Ok(config) => config,
            Err(e) => {
                tracing::error!(
                    "读取配置失败，本次启动使用默认配置（库里的原值未被覆盖，改任意设置后会重写）：{e}"
                );
                Config::default()
            }
        };
        let cache = Arc::new(RwLock::new(loaded));
        Self {
            db,
            cache,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    fn load_from_db(db: &Arc<Mutex<rusqlite::Connection>>) -> Result<Config, rusqlite::Error> {
        let conn = db.lock_ok();
        let row: Option<String> = conn
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                [CONFIG_KEY],
                |r| r.get(0),
            )
            .optional()?;
        match row {
            Some(json) => {
                let mut value: serde_json::Value = serde_json::from_str(&json).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "invalid config json",
                        )),
                    )
                })?;
                sanitize_removed_presets(&mut value);
                let mut config: Config = serde_json::from_value(value).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "invalid config json",
                        )),
                    )
                })?;
                // 旧版配置（环形动作存的是格式名、JPG 质量是全局字段）在这里
                // 一次性升级到预设模型，保证后面的代码只需认识 preset id。
                config.migrate();
                migrate_user_defaults(&conn, &mut config)?;
                Ok(config)
            }
            // 首次启动没有配置文件：默认值也要过一遍迁移，否则 `ring.profiles`
            // 是空的（默认 Profile 要按预设类别生成，不能写死在 `Default` 里）。
            None => {
                let mut config = Config::default();
                config.migrate();
                Ok(config)
            }
        }
    }

    /// 读取当前配置（克隆副本）。
    pub fn get(&self) -> Config {
        self.cache.read_ok().clone()
    }

    /// 写入整份配置（落库 + 更新缓存）。
    pub fn save(&self, config: Config) -> CommandResult<Config> {
        let _guard = self.write_lock.lock_ok();
        self.save_inner(config)
    }

    fn save_inner(&self, config: Config) -> CommandResult<Config> {
        // 前端 patch 的 `ring` 子块可能来自旧缓存（格式 id / 旧的 jpeg_quality），
        // 落库前统一过一遍迁移，避免老字段又写回库里。
        let mut config = config;
        config.migrate();
        let json = serde_json::to_string(&config)?;
        {
            let conn = self.db.lock_ok();
            conn.execute(
                "INSERT INTO config (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![CONFIG_KEY, json],
            )?;
        }
        *self.cache.write_ok() = config.clone();
        Ok(config)
    }

    /// 部分更新：合并字段（前端只传要改的字段）。
    ///
    /// 「读-合并-写」全程持写锁，避免两处同时 patch 时后写覆盖先写（评审 F3）。
    ///
    /// 注意：合并只发生在**顶层**，嵌套子块是整块替换的。
    /// 传 `{ "ring": { "option_tap": true } }` 会把 `ring.profiles` 等字段一并
    /// 重置成默认值；调用方必须把整个子块传全（前端一律 `{...ring, 字段}`）。
    pub fn patch(&self, patch: serde_json::Value) -> CommandResult<Config> {
        let _guard = self.write_lock.lock_ok();
        let current = self.get();
        let mut merged = serde_json::to_value(&current)?;
        if let serde_json::Value::Object(map) = &mut merged {
            if let serde_json::Value::Object(patch_map) = patch {
                for (k, v) in patch_map {
                    map.insert(k.clone(), v);
                }
            }
        }
        let config: Config = {
            sanitize_removed_presets(&mut merged);
            serde_json::from_value(merged)
                .map_err(|e| AppError::with_details(5001, "配置项不合法", e.to_string()))?
        };
        self.save_inner(config)
    }

    /// 更新同步配置子块。
    pub fn set_sync_config(&self, sync: settings::SyncConfig) -> CommandResult<Config> {
        let _guard = self.write_lock.lock_ok();
        let mut cfg = self.get();
        cfg.sync = sync;
        self.save_inner(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("memory db");
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .expect("create config table");
        conn
    }

    fn profile<'a>(config: &'a mut Config, id: &str) -> &'a mut settings::RingProfile {
        config
            .ring
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("缺少 {id} 盘"))
    }

    /// v7 只刷出厂盘、只跑一次：第二次不会把用户后来改的扩展名覆盖回去。
    #[test]
    fn v7_refreshes_stock_extensions_once() {
        let conn = memory_db();
        let mut config = Config::default();
        config.migrate();
        // 旧盘：图片缺 heic，文本残留 md。
        profile(&mut config, "images").extensions = vec!["png".into(), "jpg".into()];
        profile(&mut config, "text").extensions = vec!["md".into(), "txt".into()];
        // 自建盘不动。
        config.ring.profiles.push(settings::RingProfile {
            id: "profile-mine".into(),
            name: "自建".into(),
            extensions: vec!["sketch".into()],
            kind: settings::RingProfileKind::Normal,
            preset_ids: vec!["img.jpg".into()],
            back_ids: Vec::new(),
        });

        assert!(migrate_user_defaults(&conn, &mut config).expect("v7 迁移"));
        assert_eq!(
            profile(&mut config, "images").extensions,
            settings::stock_extensions("images").expect("出厂表")
        );
        assert_eq!(
            profile(&mut config, "text").extensions,
            settings::stock_extensions("text").expect("出厂表")
        );
        assert_eq!(
            profile(&mut config, "profile-mine").extensions,
            vec!["sketch".to_string()]
        );

        // 用户之后自己改：标记已写，迁移不再覆盖。
        profile(&mut config, "images").extensions = vec!["png".into()];
        assert!(!migrate_user_defaults(&conn, &mut config).expect("第二次迁移"));
        assert_eq!(
            profile(&mut config, "images").extensions,
            vec!["png".to_string()]
        );
    }

    /// v8 只把网络/蓝牙并进旧列表一次；用户之后取消勾选不会被加回来。
    ///
    /// v9（VPN）在同一趟里跑，所以期望值里带 vpn：它排在 network 后面。
    #[test]
    fn v8_adds_network_and_bluetooth_once() {
        let conn = memory_db();
        let mut config = Config::default();
        config.migrate();
        config.statusbar.sections = vec![
            "battery".into(),
            "volume".into(),
            "audio_input".into(),
            "input_source".into(),
        ];

        assert!(migrate_user_defaults(&conn, &mut config).expect("v8 迁移"));
        assert_eq!(
            config.statusbar.sections,
            vec![
                "battery".to_string(),
                "network".to_string(),
                "vpn".to_string(),
                "bluetooth".to_string(),
                "volume".to_string(),
                "audio_input".to_string(),
                "input_source".to_string(),
            ]
        );

        config.statusbar.sections.retain(|id| id != "bluetooth");
        assert!(!migrate_user_defaults(&conn, &mut config).expect("第二次迁移"));
        assert!(!config.statusbar.sections.iter().any(|id| id == "bluetooth"));
    }

    /// v9 只在 network 后面补一格 vpn；标记只写一次，用户之后取消勾选不回来。
    #[test]
    fn v9_adds_vpn_only_next_to_network() {
        // 前面的迁移标记先补上，这一趟只测 v9 自己。
        fn only_v9() -> (rusqlite::Connection, Config) {
            let conn = memory_db();
            for key in [
                MIGRATION_MARKER_KEY,
                RING_FORMATS_MARKER_KEY,
                STATUSBAR_NETWORK_MARKER_KEY,
            ] {
                set_marker(&conn, key).expect("标记");
            }
            let mut config = Config::default();
            config.migrate();
            (conn, config)
        }

        // 网络模块被用户取消勾选：不硬塞新模块。
        let (conn, mut config) = only_v9();
        config.statusbar.sections = vec!["volume".into(), "input_source".into()];
        assert!(!migrate_user_defaults(&conn, &mut config).expect("没有网络模块，不该改"));
        assert_eq!(
            config.statusbar.sections,
            vec!["volume".to_string(), "input_source".to_string()]
        );

        let (conn, mut config) = only_v9();
        config.statusbar.sections = vec![
            "battery".into(),
            "network".into(),
            "volume".into(),
            "input_source".into(),
        ];
        assert!(migrate_user_defaults(&conn, &mut config).expect("补 vpn"));
        assert_eq!(
            config.statusbar.sections,
            vec![
                "battery".to_string(),
                "network".to_string(),
                "vpn".to_string(),
                "volume".to_string(),
                "input_source".to_string(),
            ]
        );

        // 标记已写：之后用户自己取消勾选 vpn，不会被加回来。
        config.statusbar.sections.retain(|id| id != "vpn");
        assert!(!migrate_user_defaults(&conn, &mut config).expect("第二次迁移"));
        assert!(!config.statusbar.sections.iter().any(|id| id == "vpn"));
    }
}
