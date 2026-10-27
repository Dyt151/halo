//! 配置服务：从 SQLite `config` 表读取/写入，缺省回退默认值。

pub mod settings;

use std::sync::{Arc, Mutex, RwLock};

use crate::error::{AppError, CommandResult};
use crate::lock_ext::{LockExt, RwLockExt};
use rusqlite::OptionalExtension;
use settings::Config;

const CONFIG_KEY: &str = "app_config";

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
                let mut config: Config = serde_json::from_str(&json).map_err(|_| {
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
        let config: Config = serde_json::from_value(merged)
            .map_err(|e| AppError::with_details(5001, "配置项不合法", e.to_string()))?;
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
