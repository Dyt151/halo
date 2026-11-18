//! 盘面规划：格式探测、Profile 选择、扇区构建与可用性判定。

use super::*;

/// 由路径 + 配置计算对外快照（类别、数量、选中 Profile、**只含可用的**扇区）。
pub(super) fn build_snapshot(
    generation: u64,
    state: &State,
    paths: &[String],
    config: &Config,
) -> RingSnapshot {
    // 第一步：每个文件探格式（不认得的计入 skip，不能静默丢）。
    let per_file = detect_per_file(paths);
    let recognized = per_file.iter().filter(|f| f.is_some()).count();
    let dirs = paths.iter().filter(|p| Path::new(p).is_dir()).count();
    let image = per_file.iter().flatten().filter(|f| f.is_image()).count();
    let docs = per_file
        .iter()
        .flatten()
        .filter(|f| f.is_document())
        .count();
    let text = recognized - image - docs;
    let formats: Vec<ConvertFormat> = per_file.iter().flatten().copied().collect();

    // 中心提示：同一源格式显示「数量 + 格式名」，同类混合显示类别名，
    // 跨类才叫「混合文件」——避免选了 20 张 PNG 还只写「图片」。
    // 纯文件夹批次没有格式可探，单独标「文件夹」。
    let category = if recognized == 0 && dirs > 0 {
        Some("文件夹".to_string())
    } else if recognized == 0 {
        None
    } else if formats.iter().all(|f| Some(*f) == formats.first().copied()) {
        formats.first().map(|f| f.label().to_string())
    } else if text == 0 && docs == 0 {
        Some("图片".to_string())
    } else if image == 0 && docs == 0 {
        Some("文本".to_string())
    } else if image == 0 && text == 0 {
        Some("文档".to_string())
    } else {
        Some("混合文件".to_string())
    };

    // 第二步：整批扩展名 → 唯一一个 Profile（Filee 行为）。
    // 选盘先拿到 preset_ids 的固定顺序，再逐个算能不能执行；不可用的扇区
    // 在快照下发前统一过滤（`actions.retain`），盘面只呈现真正能点的动作。
    let extensions = profile::extensions_of(paths);
    let selected = profile::select_profile(&config.ring.profiles, &extensions);
    let (profile_info, ids) = match selected {
        Some(p) => (
            Some(ProfileInfo {
                id: p.id.clone(),
                name: if p.name.trim().is_empty() {
                    p.id.clone()
                } else {
                    p.name.clone()
                },
                extensions: p.extensions.clone(),
                fallback: p.kind == RingProfileKind::MixedFallback,
            }),
            slice_ids(p, config),
        ),
        None => (None, Vec::new()),
    };
    let mut actions = build_actions(&config.conversion.presets, &per_file, paths, &ids);

    // 记住的最后动作只用于预高亮，不自动执行；只能高亮真正可用的扇区。
    let suggested = if image > 0 && text == 0 && docs == 0 {
        Some("图片")
    } else if text > 0 && image == 0 && docs == 0 {
        Some("文本")
    } else if docs > 0 && image == 0 && text == 0 {
        Some("文档")
    } else {
        None
    }
    .and_then(|kind| {
        config
            .ring
            .last_action
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, v)| v.clone())
    })
    .filter(|id| actions.iter().any(|a| &a.id == id && a.enabled));

    // 空状态的原因不止一种，提示不能一刀切：「没选中」曾被错用在「选中了但没有
    // 可转目标」上（比如只选了一个 .txt），用户会以为自己的选中丢了。
    // 文件夹没有可探格式，不能算进「不是支持的格式」。
    let hint = if recognized == 0 && dirs == 0 {
        if paths.is_empty() {
            Some("没有读到拖动的文件，按住 Option 从文件图标上开始拖动。".to_string())
        } else {
            Some(format!(
                "选中的 {} 个文件都不是 Halo 支持的格式（图片 / 文档 / Markdown / HTML）。",
                paths.len()
            ))
        }
    } else if actions.is_empty() {
        Some(match profile_info.as_ref() {
            Some(p) => format!(
                "「{}」还没有圆环动作：到 设置 → 环形转换 里给它添加预设。",
                p.name
            ),
            None => "还没有可用的文件类型 Profile：到 设置 → 环形转换 里新建一个。".to_string(),
        })
    } else if !actions.iter().any(|a| a.enabled) {
        // 原因直接取可用性计算的那一条：比如「已经是 TXT，这个预设不需要重新编码」
        // 或「文本 → 图片 需要 OCR 引擎」，比笼统的「没有可用动作」有用。
        let reason = actions
            .iter()
            .find_map(|a| a.disabled_reason.clone())
            .unwrap_or_else(|| "这些预设都不适用于当前文件".to_string());
        Some(format!("选中的文件没有可执行的动作：{reason}。"))
    } else {
        None
    };

    // 只下发真正可用的扇区：灰的（禁用的）不再占位置，前端按剩余数量重新均分。
    // 执行端的第二道闸（action_slice）仍按完整列表校验，禁用项的拦截不受影响。
    actions.retain(|a| a.enabled);

    RingSnapshot {
        generation,
        state: state.as_str().to_string(),
        category,
        count: recognized + dirs,
        hint,
        profile: profile_info,
        actions,
        suggested,
    }
}

/// 一张盘的完整扇区 id：Profile 顺序 + （开关打开时）工具动作固定顺序。
///
/// 工具动作按 id 去重：开关关着时用户可能手动把某条工具预设放进过这张盘，
/// 再打开开关不能出现两个「打包 ZIP」扇区。
fn slice_ids(profile: &RingProfile, config: &Config) -> Vec<String> {
    let mut ids = profile.preset_ids.clone();
    if config.ring.utility_actions {
        for id in preset::ACTION_BUILTIN_IDS {
            if !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// 逐个文件探测格式（与 `Job` 同一份映射）。
fn detect_per_file(paths: &[String]) -> Vec<Option<ConvertFormat>> {
    paths
        .iter()
        .map(|path| convert::detect_format(path))
        .collect()
}

/// 按 Profile 的固定顺序构建完整扇区列表（含禁用项与原因）。
///
/// 格式预设的可用性走的是和 Job 同一套 `preset_availability`；打包 / 解压 /
/// PDF 分页等工具动作按载荷本身判定（`action::availability`）。两条路径都和
/// 执行端共用，不会再出现「扇区能点、执行时却无路由」的偏差。
///
/// 这里的禁用项仅供执行端闸门与内部判断；下发给前端前由 `build_snapshot`
/// 过滤掉，盘面不呈现灰扇区。
pub(super) fn build_actions(
    presets: &[ConversionPreset],
    per_file: &[Option<ConvertFormat>],
    paths: &[String],
    ids: &[String],
) -> Vec<RingAction> {
    ids.iter()
        .filter_map(|id| {
            // 预设被删掉了：这一条无法渲染标签，只能不显示；
            // 迁移（`Config::migrate`）会保证配置里没有悬空 id。
            let preset = presets.iter().find(|p| &p.id == id)?;
            let (convert, skip, reason) = if preset.kind.is_convert() {
                job::preset_availability(preset, per_file)
            } else {
                action::availability(preset, paths)
            };
            Some(RingAction {
                id: preset.id.clone(),
                label: preset.ring_label(),
                target: match preset.target {
                    crate::core::convert::preset::TargetSpec::Explicit(f) => {
                        Some(f.as_str().to_string())
                    }
                    crate::core::convert::preset::TargetSpec::SameAsSource => None,
                },
                convert,
                skip,
                enabled: convert > 0,
                disabled_reason: if convert > 0 {
                    None
                } else {
                    Some(
                        reason
                            .filter(|r| !r.trim().is_empty())
                            .unwrap_or_else(|| "这批文件里没有能用到它的输入".to_string()),
                    )
                },
            })
        })
        .collect()
}

/// 当前批次里某个动作的切片（`execute` 的第二道闸）。
///
/// 重新按当前配置与当前批次算一次：旧窗口事件、并发状态都不能把禁用项
/// 当成可用项。工具动作也在 `slice_ids` 里，因此同样受这道闸约束。
pub(super) fn action_slice(config: &Config, paths: &[String], action_id: &str) -> Option<RingAction> {
    let per_file = detect_per_file(paths);
    let extensions = profile::extensions_of(paths);
    let profile = profile::select_profile(&config.ring.profiles, &extensions)?;
    let ids = slice_ids(profile, config);
    build_actions(&config.conversion.presets, &per_file, paths, &ids)
        .into_iter()
        .find(|a| a.id == action_id)
}
