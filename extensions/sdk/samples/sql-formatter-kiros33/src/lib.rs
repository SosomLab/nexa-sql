//! **SQL Formatter for kiros33** — Nexa SQL 포맷터 확장(ABI v1.1 `nx_ext_format`).
//!
//! 규칙 원천 = 스킬 `ansi-sql-format`(2026-07). 공통 특성(들여쓰기 단위·폭 · 콤마 위치·간격 · 대소문자 · 한 줄/다중 줄 ·
//! `1=1` 시드 · AND/OR 자리)은 앱의 **Basic 공통 옵션**(`format.*`)을 그대로 쓰고, 이 확장은 그 위에 스킬 고유 규칙을 얹는다:
//! - `AS` · 비교 연산자 · ORDER BY 방향의 **탭 수직 정렬**(블록마다 가장 긴 항목 다음 탭 스톱 · 아웃라이어는 탭 1개).
//! - WHERE/HAVING의 AND/OR = 절 키워드와 **같은 열**(§3).
//! - 집합 연산자 앞뒤 **대시 구분행**(§12 · 키워드 글자 수만큼).
//! - `sqlfmt.strict`(기본 켬) = 스킬 규정값(탭 · 폭 4 · 콤마 앞 · `,\t` · `1=1` · `AND\t` · 연산자 탭 · 괄호 그룹 시드 · 대문자 · AND 앞)을
//!   강제 · 끄면 `format.*`를 따른다.
//! **1.1.0(09-29)**: Basic 확장 API(`nsql_format::format_with` = prepare → 손질 → render)로 얹는다 — 별칭 자동 부여 · 테이블 설명 주석 ·
//! 방언 치환 · 괄호 AND/OR 그룹 시드 · 시드 간격 · 조건 줄 자리 · AND/OR 뒤 간격 · 연산자 간격(4자 이상 = 공백 1개)은 전부 Basic
//! 공통 옵션이 하고, 이 확장은 정렬·구분행만 더한다(같은 열 AND = `cond_indent` · `AND\t` = `logical_gap`).

use nexa_ext_sdk::{Editor, Effect, Extension, FormatRequest, Formatter, Label, Meta, Settings};
use nsql_format::layout::{blocks, display_width, line_prefix, tabs_to, Part, Role};
use nsql_format::{Case, Comma, Gap, Indent, Line, ListStyle, LogicalNewline, Options};

pub struct Kiros33;

/// 확장 고유 설정(`sqlfmt.*`).
#[derive(Clone, Debug)]
pub struct Cfg {
    /// 스킬 규정값 강제.
    pub strict: bool,
    /// `AS` 수직 정렬.
    pub align_as: bool,
    /// 비교 연산자 수직 정렬(WHERE/ON/HAVING · SET).
    pub align_ops: bool,
    /// ORDER BY 방향(ASC/DESC) 정렬.
    pub align_order: bool,
    /// 이 글자 수를 넘는 항목은 정렬 기준에서 제외(그 줄만 탭 1개).
    pub outlier_chars: usize,
    /// WHERE/HAVING의 AND/OR를 절 키워드와 같은 열에.
    pub and_same_level: bool,
    /// 집합 연산자 앞뒤 대시 구분행.
    pub set_op_dashes: bool,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            strict: true,
            align_as: true,
            align_ops: true,
            align_order: true,
            outlier_chars: 48,
            and_same_level: true,
            set_op_dashes: true,
        }
    }
}

impl Cfg {
    pub fn from_settings(s: &Settings) -> Cfg {
        let d = Cfg::default();
        let flag = |k: &str, dflt: bool| match s.get(k) {
            Some(v) => matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "on" | "true" | "1" | "yes"
            ),
            None => dflt,
        };
        Cfg {
            strict: flag("sqlfmt.strict", d.strict),
            align_as: flag("sqlfmt.align_as", d.align_as),
            align_ops: flag("sqlfmt.align_ops", d.align_ops),
            align_order: flag("sqlfmt.align_order", d.align_order),
            outlier_chars: match s.get("sqlfmt.outlier_chars") {
                Some(v) => v
                    .trim()
                    .parse::<usize>()
                    .unwrap_or(d.outlier_chars)
                    .clamp(16, 400),
                None => d.outlier_chars,
            },
            and_same_level: flag("sqlfmt.and_same_level", d.and_same_level),
            set_op_dashes: flag("sqlfmt.set_op_dashes", d.set_op_dashes),
        }
    }
}

/// 스킬 규정값(strict) — Basic 공통 옵션 위에 덮어쓰는 값만(나머지 = 앱 설정 그대로 · 테이블 설명·별칭·방언 등 포함).
fn strict_options(base: &Options) -> Options {
    Options {
        indent: Indent::Tab,
        tab_width: 4,
        keyword_case: Case::Upper,
        comma: Comma::Leading,
        comma_gap: Gap::Tab,
        as_gap: Gap::Tab,
        logical_newline: LogicalNewline::Before,
        logical_gap: Gap::Tab,
        where_seed: true,
        paren_seed: true,
        list_style: ListStyle::Multi,
        operator_spaces: true,
        operator_gap: Gap::Tab,
        operator_long_space: true,
        case_inline_max: 120,
        ..base.clone()
    }
}

/// ★ 포맷 본체 = Basic 확장 API: 공통 옵션(+strict 덮어쓰기 · 같은 열 AND) → `format_with`(Basic 준비 → 구분행 손질 → 렌더 + 탭 정렬 채움).
pub fn format(src: &str, common: &Options, cfg: &Cfg) -> String {
    let mut opts = if cfg.strict {
        strict_options(common)
    } else {
        common.clone()
    };
    // 스킬 §3 같은 열 AND/OR = Basic의 `cond_indent`(확장 설정이 우선).
    if cfg.and_same_level {
        opts.cond_indent = false;
    }
    // ① 준비(Basic 전부) → ② 손질(구분행) — 정렬 계획은 손질 뒤 줄 번호 기준이므로 여기서 세운다.
    let mut lines = nsql_format::prepare(src, &opts);
    if cfg.set_op_dashes {
        lines = with_set_op_dashes(lines, &opts);
    }
    let plan = AlignPlan::build(&lines, &opts, cfg);
    let tw = opts.tab_width;
    let pad = |li: usize, pi: usize, sofar: &str| -> String {
        let Some(stop) = plan.stop_for(li, pi) else {
            return String::new();
        };
        let line = &lines[li];
        let leading =
            opts.comma == Comma::Leading && matches!(line.parts.first(), Some(Part::Comma));
        let prefix = line_prefix(line.indent, leading, &opts);
        let cur = display_width(&format!("{prefix}{sofar}"), tw);
        if stop == usize::MAX {
            "\t".to_string()
        } else {
            tabs_to(cur, stop, tw)
        }
    };
    // ③ 렌더(Basic + 채움).
    nsql_format::render(&lines, &opts, src, Some(&pad))
}

/// 집합 연산자 앞뒤 대시 구분행(§12 · 키워드 글자 수 · 같은 들여쓰기).
fn with_set_op_dashes(lines: Vec<Line>, opts: &Options) -> Vec<Line> {
    let mut out = Vec::with_capacity(lines.len() + 8);
    for l in lines {
        if l.role == Role::SetOp {
            let text = l.plain(opts);
            let dash = "-".repeat(display_width(&text, opts.tab_width).max(3));
            let indent = l.indent;
            let mk = || Line {
                indent,
                parts: vec![Part::Text(dash.clone())],
                role: Role::Other,
                blank_before: 0,
            };
            out.push(mk());
            out.push(l);
            out.push(mk());
        } else {
            out.push(l);
        }
    }
    out
}

/// 정렬 계획: 줄·조각 → 탭 스톱(usize::MAX = 아웃라이어 = 탭 1개).
struct AlignPlan {
    stops: Vec<(usize, usize, usize)>,
}

impl AlignPlan {
    fn build(lines: &[Line], opts: &Options, cfg: &Cfg) -> AlignPlan {
        let tw = opts.tab_width;
        let mut stops = Vec::new();
        for (s, e) in blocks(lines) {
            let role = lines[s].role;
            let kinds: &[AlignKind] = match role {
                Role::Item => &[AlignKind::As, AlignKind::Dir],
                Role::Cond => &[AlignKind::Cmp],
                _ => &[],
            };
            for k in kinds {
                let want = match k {
                    AlignKind::As => cfg.align_as,
                    AlignKind::Dir => cfg.align_order,
                    AlignKind::Cmp => cfg.align_ops,
                };
                if !want {
                    continue;
                }
                // 블록 안 각 줄의 (조각 index, 그 앞 글 폭).
                let mut widths: Vec<(usize, usize, usize)> = Vec::new();
                for li in s..e {
                    let line = &lines[li];
                    let Some(pi) = line.parts.iter().position(|p| k.matches(p)) else {
                        continue;
                    };
                    let leading = opts.comma == Comma::Leading
                        && matches!(line.parts.first(), Some(Part::Comma));
                    let prefix = line_prefix(line.indent, leading, opts);
                    let head = nsql_format::layout::render_parts(&line.parts[..pi], opts, None);
                    let w = display_width(&format!("{prefix}{head}"), tw);
                    widths.push((li, pi, w));
                }
                if widths.is_empty() {
                    continue;
                }
                let base_indent_w = {
                    let line = &lines[widths[0].0];
                    display_width(&line_prefix(line.indent, false, opts), tw)
                };
                let normal: Vec<usize> = widths
                    .iter()
                    .map(|(_, _, w)| *w)
                    .filter(|w| w.saturating_sub(base_indent_w) <= cfg.outlier_chars)
                    .collect();
                let max = normal.iter().copied().max().unwrap_or(0);
                let stop = (max / tw + 1) * tw;
                for (li, pi, w) in widths {
                    let outlier = w.saturating_sub(base_indent_w) > cfg.outlier_chars;
                    stops.push((li, pi, if outlier { usize::MAX } else { stop }));
                }
            }
        }
        AlignPlan { stops }
    }
    fn stop_for(&self, li: usize, pi: usize) -> Option<usize> {
        self.stops
            .iter()
            .find(|(l, p, _)| *l == li && *p == pi)
            .map(|(_, _, s)| *s)
    }
}

/// 정렬 대상 조각 종류.
#[derive(Clone, Copy)]
enum AlignKind {
    As,
    Dir,
    Cmp,
}

impl AlignKind {
    fn matches(self, p: &Part) -> bool {
        match self {
            AlignKind::As => matches!(p, Part::As),
            AlignKind::Dir => matches!(p, Part::OrderDir(_)),
            AlignKind::Cmp => matches!(p, Part::CmpOp(_)),
        }
    }
}

impl Extension for Kiros33 {
    fn meta() -> Meta {
        Meta {
            id: "sql-formatter-kiros33".into(),
            name: "SQL Formatter for kiros33".into(),
            settings_prefix: "sqlfmt.".into(),
            commands: vec![],
            menus: vec![],
            formatter: Some(Formatter {
                label: Label::new("SQL Formatter for kiros33", "SQL Formatter for kiros33"),
                sample: nsql_format::SAMPLE_SQL.to_string(),
            }),
        }
    }

    fn on_settings(_s: &Settings) -> Effect {
        Effect::default()
    }

    fn run(_cmd: &str, _ed: &mut Editor) -> bool {
        false
    }

    fn format(req: &FormatRequest) -> Result<String, String> {
        let common =
            Options::from_pairs(req.options.0.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let cfg = Cfg::from_settings(&req.settings);
        Ok(format(&req.text, &common, &cfg))
    }
}

nexa_ext_sdk::export_extension!(Kiros33);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_as_and_ops_with_tabs() {
        let out = format(
            "select a.ord_no, a.item_cd, a.ord_qty as qty, a.ord_amt as amt from tb_order a where a.plant_cd = '1200' and a.stock_qty > 0 and a.status_cd <> 'X'",
            &Options::default(),
            &Cfg::default(),
        );
        let want = concat!(
            "SELECT\n\ta.ord_no\n,\ta.item_cd\n,\ta.ord_qty\tAS\tqty\n,\ta.ord_amt\tAS\tamt\n",
            "FROM\n\ttb_order a\nWHERE 1=1\nAND\ta.plant_cd\t=\t'1200'\nAND\ta.stock_qty\t>\t0\nAND\ta.status_cd\t<>\t'X'\n"
        );
        assert_eq!(out, want);
    }

    #[test]
    fn set_op_dashes_and_same_level() {
        let out = format(
            "select a.k from t1 a where a.x = 1 union all select b.k from t2 b",
            &Options::default(),
            &Cfg::default(),
        );
        assert!(
            out.contains("WHERE 1=1\nAND\ta.x\t=\t1\n---------\nUNION ALL\n---------\nSELECT\n"),
            "{out}"
        );
    }

    /// 1.1.0: Basic 공통 옵션이 그대로 얹힌다 — 테이블 설명(pair) · 괄호 그룹 시드 · 단어 연산자(4자 이상 = 공백 1개) · 멱등.
    #[test]
    fn basic_options_layer_through() {
        let list = vec![("TB_ORDER".to_string(), "주문".to_string())];
        let pair = nsql_format::table_comments_pair(&list);
        let common = Options::from_pairs([("table_comments", pair.as_str())]);
        let out = format(
            "select a.k from tb_order a where a.x = 1 and (a.y = 2 or a.z like 'q%')",
            &common,
            &Cfg::default(),
        );
        assert!(out.contains("tb_order a\t--\t주문\n"), "{out}");
        assert!(out.contains("AND\t(1=0\n"), "{out}");
        assert!(
            out.contains("a.z LIKE 'q%'") || out.contains("a.z\tLIKE 'q%'"),
            "{out}"
        );
        assert_eq!(format(&out, &common, &Cfg::default()), out, "idempotent");
    }

    #[test]
    fn non_strict_follows_common_options() {
        let common = Options {
            comma: Comma::Trailing,
            indent: Indent::Spaces(2),
            tab_width: 2,
            ..Options::default()
        };
        let cfg = Cfg {
            strict: false,
            ..Cfg::default()
        };
        let out = format("select a, b from t", &common, &cfg);
        assert_eq!(out, "SELECT\n  a,\n  b\nFROM\n  t\n");
    }
}
