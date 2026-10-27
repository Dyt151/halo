//! 快捷回复 Excel 导入导出（`rust_xlsxwriter` 写 / `calamine` 读）。
//!
//! 设计参考开源项目 filee 的 importFromExcel/exportToExcel，用 Rust 重新实现：
//! - 导出列：分组、标题、内容；第一行表头（加粗），列宽固定；
//! - 导入：表头需含「标题」「内容」列，「分组」列可选（有则按分组名归组，
//!   不存在则创建；无分组列或分组为空则导入「默认」分组，不存在则创建）。
//!
//! 纯函数部分（`export_rows` / `export_file_path` / `map_header` / `parse_row` /
//! `parse_rows` / `resolve_group_target` / `normalize_group_name`）无副作用，可单测；
//! I/O 集中在 `write_export_file` / `read_sheet_rows`。命令层见 `crate::commands::reply`。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::reply::{Reply, ReplyGroup};

/// 导出表头（与导入解析对齐）。
pub const EXPORT_HEADERS: [&str; 3] = ["分组", "标题", "内容"];

/// 导入时无分组信息使用的默认分组名。
pub const DEFAULT_GROUP_NAME: &str = "默认";

/// 导出文件名前缀（完整名：`clipbase-replies-<毫秒时间戳>.xlsx`）。
pub const EXPORT_FILE_PREFIX: &str = "clipbase-replies";

/// 导入的 Excel 体积上限（20 MB）。
///
/// `calamine` 会把整个工作表读进内存：一个误选的几 GB 表格会直接把应用吃穿。
/// 快捷回复都是几十字的文本，20 MB 已经能装几十万行。
const MAX_XLSX_BYTES: u64 = 20 * 1024 * 1024;

/// 导出的一行（纯数据）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRow {
    pub group: String,
    pub title: String,
    pub content: String,
}

/// 由分组 + 回复生成导出行（保持传入顺序；分组名查不到的归入「未分组」）。
pub fn export_rows(groups: &[ReplyGroup], replies: &[Reply]) -> Vec<ExportRow> {
    replies
        .iter()
        .map(|r| {
            let group = groups
                .iter()
                .find(|g| g.id == r.group_id)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| "未分组".to_string());
            ExportRow {
                group,
                title: r.title.clone(),
                content: r.content.clone(),
            }
        })
        .collect()
}

/// 导出文件路径：`<dir>/clipbase-replies-<now_ms>.xlsx`。
pub fn export_file_path(dir: &Path, now_ms: u64) -> PathBuf {
    dir.join(format!("{EXPORT_FILE_PREFIX}-{now_ms}.xlsx"))
}

/// 写入 xlsx 文件（表头加粗；分组列 16、标题列 24、内容列 80 宽）。
pub fn write_export_file(path: &Path, rows: &[ExportRow]) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};

    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_column_width(0, 16).map_err(io_err)?;
    sheet.set_column_width(1, 24).map_err(io_err)?;
    sheet.set_column_width(2, 80).map_err(io_err)?;

    let header = Format::new().set_bold();
    let plain = Format::new();
    for (col, name) in EXPORT_HEADERS.iter().enumerate() {
        sheet
            .write_string(0, col as u16, name, &header)
            .map_err(io_err)?;
    }

    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write_string(r, 0, &row.group, &plain).map_err(io_err)?;
        sheet.write_string(r, 1, &row.title, &plain).map_err(io_err)?;
        sheet.write_string(r, 2, &row.content, &plain).map_err(io_err)?;
    }

    let path_str = path
        .to_str()
        .ok_or_else(|| "导出路径含非 UTF-8 字符".to_string())?;
    // 先写同目录临时文件再 rename：写到一半磁盘满 / 没权限时不会留下一个
    // 打不开的半成品 xlsx（用户双击只会得到「文件已损坏」）。
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("export.xlsx");
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| "导出路径含非 UTF-8 字符".to_string())?;
    book.save(tmp_str).map_err(io_err)?;
    if let Err(e) = std::fs::rename(&tmp, path_str) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("保存 Excel 失败：{e}"));
    }
    Ok(())
}

/// 表头列映射（标题/内容必需，分组可选；均按表头文字精确匹配）。
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderMap {
    pub group: Option<usize>,
    pub title: usize,
    pub content: usize,
}

/// 识别表头：缺「标题」或「内容」列时报错（错误信息列出缺失列名）。
pub fn map_header(cells: &[String]) -> Result<HeaderMap, String> {
    let mut group = None;
    let mut title = None;
    let mut content = None;
    for (i, cell) in cells.iter().enumerate() {
        match cell.trim() {
            "分组" => group = Some(i),
            "标题" => title = Some(i),
            "内容" => content = Some(i),
            _ => {}
        }
    }
    let mut missing = Vec::new();
    if title.is_none() {
        missing.push("标题");
    }
    if content.is_none() {
        missing.push("内容");
    }
    match (title, content) {
        (Some(t), Some(c)) => Ok(HeaderMap {
            group,
            title: t,
            content: c,
        }),
        _ => Err(format!("表头缺少必需列：{}", missing.join("、"))),
    }
}

/// 解析后的一行（`group` 为 None 表示未指定分组，导入时落入「默认」分组）。
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRow {
    pub group: Option<String>,
    pub title: String,
    pub content: String,
}

/// 解析单行（`cells` 为整行单元格，索引越界视为空串）：
/// 整行空白返回 `Ok(None)`（跳过）；标题为空返回 `Err`。
pub fn parse_row(cells: &[String], map: &HeaderMap) -> Result<Option<ImportRow>, String> {
    if cells.iter().all(|c| c.trim().is_empty()) {
        return Ok(None);
    }
    let cell = |i: usize| cells.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
    let title = cell(map.title);
    if title.is_empty() {
        return Err("标题为空".to_string());
    }
    let group = map.group.map(cell).filter(|g| !g.is_empty());
    let content = cell(map.content);
    Ok(Some(ImportRow {
        group,
        title,
        content,
    }))
}

/// 解析全部行（`rows[0]` 为表头）：返回 `(有效行, 错误列表)`；
/// 表头缺失或文件为空时错误列表含对应说明，有效行为空。
pub fn parse_rows(rows: &[Vec<String>]) -> (Vec<ImportRow>, Vec<String>) {
    let mut errors = Vec::new();
    if rows.is_empty() {
        errors.push("文件中没有数据".to_string());
        return (Vec::new(), errors);
    }
    let map = match map_header(&rows[0]) {
        Ok(m) => m,
        Err(e) => {
            errors.push(e);
            return (Vec::new(), errors);
        }
    };
    let mut out = Vec::new();
    for (idx, cells) in rows.iter().enumerate().skip(1) {
        match parse_row(cells, &map) {
            Ok(Some(row)) => out.push(row),
            Ok(None) => {}
            Err(e) => errors.push(format!("第 {} 行：{}", idx + 1, e)),
        }
    }
    (out, errors)
}

/// 归一化分组名：None / 空白 → 「默认」。
pub fn normalize_group_name(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => DEFAULT_GROUP_NAME.to_string(),
    }
}

/// 分组归组决策：按名称命中已有分组则沿用，否则新建。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GroupTarget {
    Existing(i64),
    Create,
}

/// 按分组名解析目标分组（名称精确匹配，大小写敏感）。
pub fn resolve_group_target(name: &str, groups: &[(i64, String)]) -> GroupTarget {
    match groups.iter().find(|(_, n)| n == name) {
        Some((id, _)) => GroupTarget::Existing(*id),
        None => GroupTarget::Create,
    }
}

/// 导入汇总（单行/单条失败不中断，错误收集到 `errors`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub groups_created: usize,
    pub errors: Vec<String>,
}

/// 读取 xlsx 第一个工作表的全部单元格（统一转为字符串；空单元格为空串）。
pub fn read_sheet_rows(path: &Path) -> Result<Vec<Vec<String>>, String> {
    use calamine::{open_workbook_auto, Data, Reader};

    let meta = std::fs::metadata(path).map_err(|e| format!("无法读取 Excel 文件：{e}"))?;
    if !meta.is_file() {
        return Err("选择的不是文件".to_string());
    }
    if meta.len() > MAX_XLSX_BYTES {
        return Err(format!(
            "Excel 文件太大（{:.1} MB），上限 {} MB",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_XLSX_BYTES / (1024 * 1024)
        ));
    }

    let mut book: calamine::Sheets<std::io::BufReader<std::fs::File>> =
        open_workbook_auto(path).map_err(|e| format!("无法打开 Excel 文件：{e}"))?;
    let first = book
        .sheet_names()
        .into_iter()
        .next()
        .ok_or_else(|| "工作簿中没有工作表".to_string())?;
    let range = book
        .worksheet_range(&first)
        .map_err(|e| format!("无法读取工作表「{first}」：{e}"))?;
    Ok(range
        .rows()
        .map(|row| row.iter().map(data_to_string).collect())
        .collect())
}

fn data_to_string(d: &calamine::Data) -> String {
    use calamine::Data;
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => fmt_float(*f),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(_) => String::new(),
    }
}

/// 浮点格式化：整数值去掉小数部分（避免 `1` 显示成 `1.0`）。
fn fmt_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

fn io_err(e: impl std::fmt::Display) -> String {
    format!("写入 Excel 失败：{e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: i64, name: &str) -> ReplyGroup {
        ReplyGroup {
            id,
            name: name.to_string(),
            sort: 0,
        }
    }

    fn reply(group_id: i64, title: &str, content: &str) -> Reply {
        Reply {
            id: 0,
            group_id,
            title: title.to_string(),
            content: content.to_string(),
            files: vec![],
            sort: 0,
            created_at: 0.0,
        }
    }

    fn cells(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ---------- 导出 ----------

    #[test]
    fn export_rows_maps_group_names_and_falls_back() {
        let groups = vec![group(1, "工作"), group(2, "生活")];
        let replies = vec![
            reply(1, "周报", "请查收"),
            reply(2, "问候", "早上好"),
            reply(99, "孤儿", "分组已被删除"),
        ];
        let rows = export_rows(&groups, &replies);
        assert_eq!(
            rows,
            vec![
                ExportRow {
                    group: "工作".into(),
                    title: "周报".into(),
                    content: "请查收".into()
                },
                ExportRow {
                    group: "生活".into(),
                    title: "问候".into(),
                    content: "早上好".into()
                },
                ExportRow {
                    group: "未分组".into(),
                    title: "孤儿".into(),
                    content: "分组已被删除".into()
                },
            ]
        );
    }

    #[test]
    fn export_file_path_uses_prefix_and_timestamp() {
        let p = export_file_path(Path::new("/tmp"), 1_700_000_000_000);
        assert_eq!(
            p,
            PathBuf::from("/tmp/clipbase-replies-1700000000000.xlsx")
        );
    }

    #[test]
    fn export_write_and_read_roundtrip() {
        let rows = vec![
            ExportRow {
                group: "工作".into(),
                title: "周报".into(),
                content: "请查收附件".into()
            },
            ExportRow {
                group: "生活".into(),
                title: "问候".into(),
                content: "早上好呀".into()
            },
        ];
        let dir = std::env::temp_dir().join(format!("clipbase-xlsx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = export_file_path(&dir, 42);
        write_export_file(&path, &rows).unwrap();

        let sheet = read_sheet_rows(&path).unwrap();
        assert_eq!(sheet[0], cells(&["分组", "标题", "内容"]));
        assert_eq!(sheet[1], cells(&["工作", "周报", "请查收附件"]));
        assert_eq!(sheet[2], cells(&["生活", "问候", "早上好呀"]));
        assert_eq!(sheet.len(), 3, "只有表头 + 两行数据");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- 导入 ----------

    #[test]
    fn map_header_accepts_optional_group_column() {
        let with_group = map_header(&cells(&["分组", "标题", "内容"])).unwrap();
        assert_eq!(
            with_group,
            HeaderMap {
                group: Some(0),
                title: 1,
                content: 2
            }
        );

        let without_group = map_header(&cells(&["标题", "内容"])).unwrap();
        assert_eq!(
            without_group,
            HeaderMap {
                group: None,
                title: 0,
                content: 1
            }
        );
    }

    #[test]
    fn map_header_errors_on_missing_required_columns() {
        let err = map_header(&cells(&["分组"])).unwrap_err();
        assert!(err.contains("标题"), "{err}");
        assert!(err.contains("内容"), "{err}");
        assert!(map_header(&cells(&["标题"])).is_err());
        assert!(map_header(&cells(&["内容"])).is_err());
    }

    #[test]
    fn parse_row_skips_blank_and_flags_empty_title() {
        let map = map_header(&cells(&["分组", "标题", "内容"])).unwrap();
        assert_eq!(parse_row(&cells(&["", "", ""]), &map).unwrap(), None);
        assert_eq!(
            parse_row(&cells(&["", "   ", "  "]), &map).unwrap(),
            None,
            "整行空白应跳过"
        );
        let err = parse_row(&cells(&["工作", "", "内容"]), &map).unwrap_err();
        assert_eq!(err, "标题为空");
        let ok = parse_row(&cells(&[" 工作 ", " 周报 ", " 请查收 "]), &map).unwrap();
        assert_eq!(
            ok,
            Some(ImportRow {
                group: Some("工作".into()),
                title: "周报".into(),
                content: "请查收".into()
            }),
            "单元格首尾空白应被修剪"
        );
    }

    #[test]
    fn parse_row_without_group_column_yields_none() {
        let map = map_header(&cells(&["标题", "内容"])).unwrap();
        assert_eq!(
            parse_row(&cells(&["周报", "请查收"]), &map).unwrap(),
            Some(ImportRow {
                group: None,
                title: "周报".into(),
                content: "请查收".into()
            })
        );
        // 分组列存在但为空 → 同样视为未指定。
        let map2 = map_header(&cells(&["分组", "标题", "内容"])).unwrap();
        assert_eq!(
            parse_row(&cells(&["", "周报", "请查收"]), &map2).unwrap(),
            Some(ImportRow {
                group: None,
                title: "周报".into(),
                content: "请查收".into()
            })
        );
    }

    #[test]
    fn parse_rows_collects_errors_and_keeps_valid_rows() {
        let rows = vec![
            cells(&["分组", "标题", "内容"]),
            cells(&["工作", "周报", "请查收"]),
            cells(&["", "", ""]),          // 空行：跳过
            cells(&["工作", "", "缺标题"]), // 错误：收集
            cells(&["生活", "问候", "早上好"]),
        ];
        let (out, errors) = parse_rows(&rows);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].title, "周报");
        assert_eq!(out[1].group.as_deref(), Some("生活"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("第 4 行"), "{}", errors[0]);
        assert!(errors[0].contains("标题为空"), "{}", errors[0]);
    }

    #[test]
    fn parse_rows_empty_file_and_bad_header() {
        let (out, errors) = parse_rows(&[]);
        assert!(out.is_empty());
        assert_eq!(errors, vec!["文件中没有数据".to_string()]);

        let (out2, errors2) = parse_rows(&[cells(&["分组", "名称", "正文"])]);
        assert!(out2.is_empty());
        assert_eq!(errors2.len(), 1);
        assert!(errors2[0].contains("标题"), "{}", errors2[0]);
    }

    // ---------- 分组归组 ----------

    #[test]
    fn resolve_group_target_reuses_existing_by_name() {
        let groups = vec![(1, "工作".to_string()), (2, "生活".to_string())];
        assert_eq!(resolve_group_target("工作", &groups), GroupTarget::Existing(1));
        assert_eq!(resolve_group_target("生活", &groups), GroupTarget::Existing(2));
        assert_eq!(resolve_group_target("新分组", &groups), GroupTarget::Create);
        assert_eq!(resolve_group_target("", &groups), GroupTarget::Create);
    }

    #[test]
    fn normalize_group_name_defaults_when_blank() {
        assert_eq!(normalize_group_name(None), "默认");
        assert_eq!(normalize_group_name(Some("  ")), "默认");
        assert_eq!(normalize_group_name(Some(" 工作 ")), "工作");
    }

    // ---------- 服务级导入（归组 / 建组） ----------

    #[test]
    fn import_replies_groups_by_name_and_creates_missing() {
        use crate::core::reply::ReplyService;
        use crate::qa_tests::test_db;

        let svc = ReplyService::new(test_db());
        let existing = svc.add_group("工作".to_string()).unwrap();
        svc.add_reply(existing.id, "旧回复".into(), "旧内容".into(), vec![])
            .unwrap();

        let rows = vec![
            ImportRow {
                group: Some("工作".into()),
                title: "周报".into(),
                content: "请查收".into(),
            },
            ImportRow {
                group: Some("生活".into()),
                title: "问候".into(),
                content: "早上好".into(),
            },
            ImportRow {
                group: None,
                title: "无分组".into(),
                content: "落入默认".into(),
            },
        ];
        let summary = svc.import_replies(&rows).unwrap();
        assert_eq!(summary.imported, 3);
        assert_eq!(summary.groups_created, 2, "「生活」与「默认」应被新建");
        assert!(summary.errors.is_empty());

        let groups = svc.list_groups().unwrap();
        assert_eq!(groups.len(), 3);
        let life = groups.iter().find(|g| g.name == "生活").unwrap();
        let default = groups.iter().find(|g| g.name == "默认").unwrap();

        let all = svc.list_replies(None, None).unwrap();
        assert_eq!(all.len(), 4, "原有 1 条 + 导入 3 条");
        let work: Vec<_> = all.iter().filter(|r| r.group_id == existing.id).collect();
        assert_eq!(work.len(), 2, "「工作」组应复用不新建");
        assert_eq!(
            all.iter().filter(|r| r.group_id == life.id).count(),
            1
        );
        assert_eq!(
            all.iter().filter(|r| r.group_id == default.id).count(),
            1
        );
    }

    #[test]
    fn import_replies_reuses_group_created_within_same_batch() {
        use crate::core::reply::ReplyService;
        use crate::qa_tests::test_db;

        let svc = ReplyService::new(test_db());
        let rows = vec![
            ImportRow {
                group: Some("临时".into()),
                title: "a".into(),
                content: "".into(),
            },
            ImportRow {
                group: Some("临时".into()),
                title: "b".into(),
                content: "".into(),
            },
        ];
        let summary = svc.import_replies(&rows).unwrap();
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.groups_created, 1, "同一批次内重复分组名只建一次");
        assert_eq!(svc.list_groups().unwrap().len(), 1);
    }
}
