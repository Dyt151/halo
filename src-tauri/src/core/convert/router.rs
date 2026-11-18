//! 路由规划器：以格式为节点、引擎边为有向加权边，用 Dijkstra 选最便宜路径。
//!
//! 对照 README「三、环形文件转换」：
//! - 普通路由最多 3 步（`MAX_STEPS`），中间产物不进最终输出；
//! - 源格式 == 目标格式时**只认显式 self edge**，既不默认复制，也不默认非法；
//! - `Animated` / `Ocr` 这类特殊边不参与普通选路，必须由专门模式规划。
//!
//! 路由只回答「怎么走」，不回答「能不能写盘」（那是 `output.rs`）。

use std::collections::{HashMap, HashSet};

use super::edge::{ConversionEdge, EdgeMode, EngineId};
use super::engines;
use super::format::{ConvertFormat, ALL_FORMATS};

/// 普通路由的最大步数（含首尾）。
pub const MAX_STEPS: usize = 3;

/// 路由里的一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteStep {
    pub from: ConvertFormat,
    pub to: ConvertFormat,
    pub engine: EngineId,
}

/// 一条完整路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub steps: Vec<RouteStep>,
}

impl Route {
    /// 途经的格式序列（首尾都含），用于日志与结果展示。
    pub fn formats(&self) -> Vec<ConvertFormat> {
        let mut out = Vec::with_capacity(self.steps.len() + 1);
        if let Some(first) = self.steps.first() {
            out.push(first.from);
        }
        out.extend(self.steps.iter().map(|s| s.to));
        out
    }

    /// `png → webp` 这种可读描述。
    pub fn describe(&self) -> String {
        self.formats()
            .iter()
            .map(|f| f.label())
            .collect::<Vec<_>>()
            .join(" → ")
    }
}

/// 规划失败的原因（都不是「静默忽略」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoRoute {
    /// 同格式，但图上没有声明这条路（或 preset 没要求重编码）。
    SameFormat,
    /// 图上不可达。
    NoEdge,
    /// 可达但步数超过上限。
    TooManySteps(usize),
}

impl NoRoute {
    pub fn describe(&self, from: ConvertFormat, to: ConvertFormat) -> String {
        match self {
            Self::SameFormat => format!("已经是 {} 格式，且该预设不要求重新编码", from.label()),
            Self::NoEdge => format!(
                "没有从 {} 到 {} 的转换路径（该方向需要 OCR / 排版引擎，当前版本未接入）",
                from.label(),
                to.label()
            ),
            Self::TooManySteps(n) => format!(
                "{} → {} 需要 {n} 步，超过上限 {MAX_STEPS} 步",
                from.label(),
                to.label()
            ),
        }
    }
}

/// 规划 `from → to`。
///
/// `allow_self`：是否允许「同格式重编码」（由 preset 的 `reencode_same_format`
/// 决定）。为 false 时同格式直接返回 `SameFormat`。
pub fn plan(from: ConvertFormat, to: ConvertFormat, allow_self: bool) -> Result<Route, NoRoute> {
    // 普通模式可以站中间；`Terminal` 只能收尾（不能作为中途停靠点）。
    let edges: Vec<ConversionEdge> = engines::edges()
        .into_iter()
        .filter(|e| e.mode.is_chainable() || (e.mode == EdgeMode::Terminal && e.to == to))
        .collect();

    // 同格式：只认显式 self edge（Filee 行为）。
    if from == to {
        if !allow_self {
            return Err(NoRoute::SameFormat);
        }
        return edges
            .iter()
            .find(|e| e.is_self() && e.from == from)
            .map(|e| Route {
                steps: vec![RouteStep {
                    from,
                    to,
                    engine: e.engine,
                }],
            })
            .ok_or(NoRoute::SameFormat);
    }

    // Dijkstra：self edge 不作为中间步（原地重编码没有意义）。
    let mut best: HashMap<ConvertFormat, u32> = HashMap::new();
    let mut prev: HashMap<ConvertFormat, (ConvertFormat, EngineId)> = HashMap::new();
    let mut settled: HashSet<ConvertFormat> = HashSet::new();
    best.insert(from, 0);

    // 同代价时按 `ALL_FORMATS` 的顺序取，避免 HashMap 随机迭代导致
    // 同一对格式每次选出不同中间格式（测试与用户看到的路径都要稳定）。
    while let Some((node, cost)) = ALL_FORMATS
        .iter()
        .enumerate()
        .filter(|(_, node)| !settled.contains(*node))
        .filter_map(|(index, node)| best.get(node).map(|cost| (*node, *cost, index)))
        .min_by_key(|(_, cost, index)| (*cost, *index))
        .map(|(node, cost, _)| (node, cost))
    {
        if node == to {
            break;
        }
        settled.insert(node);

        for edge in edges
            .iter()
            .filter(|e| e.from == node && !e.is_self())
        {
            let next_cost = cost + edge.cost;
            let improved = match best.get(&edge.to) {
                Some(existing) => next_cost < *existing,
                None => true,
            };
            if improved {
                best.insert(edge.to, next_cost);
                prev.insert(edge.to, (node, edge.engine));
            }
        }
    }

    if !best.contains_key(&to) {
        return Err(NoRoute::NoEdge);
    }

    // 回溯。
    let mut steps: Vec<RouteStep> = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let Some(&(before, engine)) = prev.get(&cursor) else {
            return Err(NoRoute::NoEdge);
        };
        steps.push(RouteStep {
            from: before,
            to: cursor,
            engine,
        });
        cursor = before;
    }
    steps.reverse();

    if steps.len() > MAX_STEPS {
        return Err(NoRoute::TooManySteps(steps.len()));
    }
    Ok(Route { steps })
}

/// 是否存在普通路由（同格式时要求显式 self edge）。
pub fn has_route(from: ConvertFormat, to: ConvertFormat) -> bool {
    plan(from, to, true).is_ok()
}

/// 该格式能去往的所有格式（含自身——当且仅当声明了 self edge）。
pub fn reachable_targets(from: ConvertFormat) -> Vec<ConvertFormat> {
    ALL_FORMATS
        .into_iter()
        .filter(|to| has_route(from, *to))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::format::SIPS_FORMATS;
    use ConvertFormat::*;

    #[test]
    fn direct_edges_are_one_step() {
        let route = plan(Png, Jpg, false).expect("png -> jpg");
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].engine, EngineId::Image);

        let route = plan(Txt, Html, false).expect("txt -> html");
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].engine, EngineId::Text);
    }

    /// 图片 → HTML：直连比「先转 PNG 再内联」便宜，应当一步到位。
    #[test]
    fn cheapest_route_wins() {
        let route = plan(Png, Html, false).expect("png -> html");
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].engine, EngineId::Document);
        assert_eq!(route.describe(), "PNG → HTML");
    }

    /// TIFF 不能内联进 HTML，路由器应该先转成可内嵌的图片（两步），而不是报错。
    #[test]
    fn tiff_to_html_goes_through_embeddable_image() {
        let route = plan(Tiff, Html, false).expect("tiff -> html");
        assert_eq!(route.steps.len(), 2);
        let formats = route.formats();
        assert_eq!((formats[0], formats[2]), (Tiff, Html));
        // 中间格式由边权决定（PNG / JPG / WebP / GIF 都行），因此只断言
        // 「可以直接内联进 HTML」，不钉死具体是哪一个。
        assert!(
            [Png, Jpg, Webp, Gif].contains(&formats[1]),
            "中间格式不能内联进 HTML：{:?}",
            formats
        );
        assert_eq!(route.steps[1].engine, EngineId::Document);
    }

    /// 同格式：只有显式 self edge + 明确要求重编码才成立。
    #[test]
    fn same_format_needs_explicit_self_edge() {
        assert_eq!(plan(Png, Png, false), Err(NoRoute::SameFormat));
        let route = plan(Jpg, Jpg, true).expect("jpg -> jpg 重编码");
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].engine, EngineId::Image);
        // 文本引擎没有 self edge，即使允许也不成立。
        assert_eq!(plan(Txt, Txt, true), Err(NoRoute::SameFormat));
    }

    /// 缺的两条边必须明确不可达，而不是被悄悄绕过去。
    #[test]
    fn missing_edges_are_explicit() {
        assert_eq!(plan(Txt, Png, false), Err(NoRoute::NoEdge));
        assert_eq!(plan(Html, Jpg, false), Err(NoRoute::NoEdge));
        assert!(!has_route(Png, Txt), "图片 → TXT 需要 OCR");
        assert_eq!(plan(Png, Txt, false), Err(NoRoute::NoEdge));
    }

    #[test]
    fn reachable_targets_cover_same_quadrant_and_documents() {
        let image_targets = reachable_targets(Png);
        // 其余 5 张编解码图片 + HEIC + html + PDF + 自己（self edge）。
        assert_eq!(image_targets.len(), 9);
        assert!(image_targets.contains(&Png));
        assert!(image_targets.contains(&Pdf), "图片可以嵌入单页 PDF");
        assert!(!image_targets.contains(&Txt), "图片 → TXT 需要 OCR");

        let text_targets = reachable_targets(Txt);
        // 文本互转 + PDF 排版 + DOCX（textutil）；PDF 是终点，
        // 不会再被 pdf→图片 接走。
        assert_eq!(text_targets, vec![Html, Pdf, Docx]);
    }

    /// HEIC 借 PNG 桥可到全部图片目标（≤2 步），到不了文本。
    #[test]
    fn sips_formats_route_via_png_bridge() {
        for from in SIPS_FORMATS {
            for to in [Png, Jpg, Webp, Gif, Bmp, Tiff, Heic] {
                let route = plan(from, to, true)
                    .unwrap_or_else(|e| panic!("{from:?} → {to:?} 应有路由：{e:?}"));
                assert!(
                    route.steps.len() <= MAX_STEPS,
                    "{from:?} → {to:?} 用了 {} 步",
                    route.steps.len()
                );
            }
            assert_eq!(plan(from, Txt, false), Err(NoRoute::NoEdge));
        }
        // 常规格式之间由 image 引擎直连（单步）。
        let route = plan(Bmp, Tiff, false).expect("bmp -> tiff");
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].engine, EngineId::Image);
        let route = plan(Webp, Gif, false).expect("webp -> gif");
        assert_eq!(route.steps.len(), 1);
    }

    /// 任何格式都不应该需要 4 步以上（预算足够时路由链很短）。
    #[test]
    fn routes_never_exceed_step_budget() {
        for from in ALL_FORMATS {
            for to in ALL_FORMATS {
                if let Ok(route) = plan(from, to, true) {
                    assert!(
                        route.steps.len() <= MAX_STEPS,
                        "{from:?} → {to:?} 超过 {MAX_STEPS} 步"
                    );
                }
            }
        }
    }
}
