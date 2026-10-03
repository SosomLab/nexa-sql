//! **SQL Formatter for kiros33** — Nexa SQL 포맷터 확장(ABI v1.1 `nx_ext_format`).
//!
//! 규칙 원천 = 스킬 `ansi-sql-format`(2026-07). 공통 특성(들여쓰기 단위·폭 · 콤마 위치·간격 · 대소문자 · 한 줄/다중 줄 ·
//! `1=1` 시드 · AND/OR 자리)은 앱의 **Basic 공통 옵션**(`format.*`)을 그대로 쓰고, 이 확장은 그 위에 스킬 고유 규칙을 얹는다:
//! - `AS` · 비교 연산자 · ORDER BY 방향의 **탭 수직 정렬**(블록마다 가장 긴 항목 다음 탭 스톱 · 아웃라이어는 탭 1개).
//! - WHERE/HAVING의 AND/OR = 절 키워드와 **같은 열**(§3).
//! - 집합 연산자 앞뒤 **대시 구분행**(§12 · 키워드 글자 수만큼).
//! - `ext.sqlfmt_kiros33.strict`(기본 켬) = 스킬 규정값(탭 · 폭 4 · 콤마 앞 · `,\t` · `1=1` · `AND\t` · 연산자 탭 · 괄호 그룹 시드 · 대문자 · AND 앞)을
//!   강제 · 끄면 `format.*`를 따른다.
//! **1.1.0(09-29)**: Basic 확장 API(`nsql_format::format_with` = prepare → 손질 → render)로 얹는다 — 별칭 자동 부여 · 테이블 설명 주석 ·
//! 방언 치환 · 괄호 AND/OR 그룹 시드 · 시드 간격 · 조건 줄 자리 · AND/OR 뒤 간격 · 연산자 간격(4자 이상 = 공백 1개)은 전부 Basic
//! 공통 옵션이 하고, 이 확장은 정렬·구분행만 더한다(같은 열 AND = `cond_indent` · `AND\t` = `logical_gap`).

use nexa_ext_sdk::{Editor, Effect, Extension, FormatRequest, Formatter, Label, Meta, Settings};
use nsql_format::layout::{blocks, display_width, line_prefix, tabs_to, Part, Role};
use nsql_format::{AliasAs, Case, Comma, Gap, Indent, Line, ListStyle, LogicalNewline, Options};

pub struct Kiros33;

/// 중복 별칭 처리(`ext.sqlfmt_kiros33.dup_alias` · 사용자 09-30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DupAlias {
    /// `plant_cd1`, `plant_cd2` …(기본).
    Numbered,
    /// `plant_cd_p`, `plant_cd_t`(테이블 별칭 · 없으면 순번).
    Qualified,
    /// 그대로 둔다.
    Keep,
}

/// ★ 확장 설정별 확인 예제(사용자 09-30 "켜고 끌 때마다 바뀌는 예제"): Basic 샘플 뒤에 붙는다 — 한 문장에 설정 하나씩.
///   ① `align_as`/`force_as` = 길이가 다른 별칭(AS 없는 것 포함) ② `outlier_chars` = 긴 CASE 열 ③ `align_ops` = `=` `>=` `<>` `LIKE`
///   ④ `and_same_level`/`strict` = WHERE의 AND/OR ⑤ `set_op_dashes` = UNION ALL ⑥ `align_order` = ORDER BY ASC/DESC.
pub const KIROS33_SAMPLE: &str = "-- SQL Formatter for kiros33: align_as / force_as / outlier_chars\nselect p.plant_cd, t.plant_cd, p.item_cd item, nvl(p.plan_qty, 0) as qty, p.status_cd status_code_of_plan, trim(p.remark_txt) remark, sum(p.plan_qty) over (partition by p.plant_cd order by p.item_cd rows between 2 preceding and current row) mov_sum_3, case when p.plan_qty >= 1000 then 'BULK' when p.plan_qty >= 100 then 'NORMAL' else 'SMALL' end plan_size_grade\nfrom tb_plan p\n-- force_as / dup_alias(plant_cd가 둘)\ninner join tb_plant t on t.plant_cd = p.plant_cd\n-- align_ops / and_same_level / strict(1=1)\nwhere p.plant_cd = '1200' and nvl(p.plan_qty, 0) >= 10 and p.status_cd <> 'X' or p.item_cd like 'PCM%'\n-- set_op_dashes\nunion all\nselect h.plant_cd, h.item_cd, h.plan_qty, h.status_cd, 'HIST'\nfrom tb_plan_hist h\nwhere h.yymm between '202601' and '202612'\n-- align_order\norder by 1 asc, 3 desc, 2;\n";

/// 확장 고유 설정(`ext.sqlfmt_kiros33.*`).
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
    /// 이 글자 수(들여쓰기·앞 공백을 뺀 실제 글자부터)를 넘는 항목은 정렬 기준에서 제외 — 그 줄은 AS/연산자 **앞뒤 탭 1개씩**
    /// (공백 없음 · 4자 이상 연산자 뒤는 공백 1개 규칙 유지 · 사용자 09-30 · 기본 40).
    pub outlier_chars: usize,
    /// ★ 열 별칭에 `AS` 강제(`sum(a.qty) qty` → `sum(a.qty) AS qty` · Basic `column_as = add`) + 순수 컬럼도 `AS 컬럼명`
    ///   (`a.plant_cd` → `a.plant_cd AS plant_cd` · Basic `column_alias_all`) · 기본 켬 · 사용자 09-30.
    pub force_as: bool,
    /// ★ JOIN으로 같은 이름 별칭이 겹칠 때(사용자 09-30 3택): 컬럼명1·2 순번 / 컬럼명_별칭(별칭 없으면 순번) / 그대로.
    pub dup_alias: DupAlias,
    /// ★ 윈도우 `OVER (…)` 줄바꿈 문턱(스킬 §8 · 기본 40 · 0 = 늘 한 줄) — Basic `window_break`에 강제.
    pub window_break: usize,
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
            outlier_chars: 40,
            force_as: true,
            dup_alias: DupAlias::Numbered,
            window_break: 40,
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
            strict: flag("ext.sqlfmt_kiros33.strict", d.strict),
            align_as: flag("ext.sqlfmt_kiros33.align_as", d.align_as),
            align_ops: flag("ext.sqlfmt_kiros33.align_ops", d.align_ops),
            align_order: flag("ext.sqlfmt_kiros33.align_order", d.align_order),
            outlier_chars: match s.get("ext.sqlfmt_kiros33.outlier_chars") {
                Some(v) => v
                    .trim()
                    .parse::<usize>()
                    .unwrap_or(d.outlier_chars)
                    .clamp(16, 400),
                None => d.outlier_chars,
            },
            force_as: flag("ext.sqlfmt_kiros33.force_as", d.force_as),
            dup_alias: match s
                .get("ext.sqlfmt_kiros33.dup_alias")
                .map(|v| v.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("qualified") => DupAlias::Qualified,
                Some("keep") => DupAlias::Keep,
                Some("numbered") => DupAlias::Numbered,
                _ => d.dup_alias,
            },
            window_break: match s.get("ext.sqlfmt_kiros33.window_break") {
                Some(v) => v.trim().parse::<usize>().unwrap_or(d.window_break).min(400),
                None => d.window_break,
            },
            and_same_level: flag("ext.sqlfmt_kiros33.and_same_level", d.and_same_level),
            set_op_dashes: flag("ext.sqlfmt_kiros33.set_op_dashes", d.set_op_dashes),
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
        comment_space: true,
        comment_gap: Gap::Tab,
        logical_newline: LogicalNewline::Before,
        logical_gap: Gap::Tab,
        where_seed: true,
        paren_seed: true,
        list_style: ListStyle::Multi,
        operator_spaces: true,
        operator_gap: Gap::Tab,
        operator_long_space: true,
        case_inline_max: 120,
        // 문장 끝 `;`은 다음 줄 1칸(사용자 09-30).
        semicolon_newline: true,
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
    // 윈도우 함수 줄바꿈 문턱(스킬 §8) = 확장 설정이 Basic 값을 덮는다(사용자 09-30).
    opts.window_break = cfg.window_break;
    // 열 별칭 AS 강제 = Basic의 `column_as = add` + 순수 컬럼 `column_alias_all`(사용자 09-30).
    if cfg.force_as {
        opts.column_as = AliasAs::Add;
        opts.column_alias_all = true;
    }
    // ① 준비(Basic 전부) → ② 손질(중복 별칭 · 구분행) — 정렬 계획은 손질 뒤 줄 번호 기준이므로 여기서 세운다.
    let mut lines = nsql_format::prepare(src, &opts);
    if cfg.force_as {
        dedup_aliases(&mut lines, cfg.dup_alias);
    }
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
        // (윈도우 절을 닫는 `)` 줄도 같은 블록의 정렬 열로 — 사용자 09-30 "다중 행 항목도 AS 정렬 대상".)
        if stop == usize::MAX {
            // 아웃라이어(기준보다 긴 항목) = 정렬에서 빼고 **앞뒤 탭 1개씩**(공백 없음 · 사용자 09-30) — 뒤 탭은 as_gap/operator_gap.
            "\t".to_string()
        } else {
            tabs_to(cur, stop, tw)
        }
    };
    // ③ 렌더(Basic + 채움).
    nsql_format::render(&lines, &opts, src, Some(&pad))
}

/// SELECT 목록 안에서 겹치는 별칭을 `dup_alias` 규칙으로 바꾼다(사용자 09-30 · JOIN으로 같은 컬럼 이름이 둘 이상일 때).
/// 목록 = `SELECT` 절 줄 다음의 같은 깊이(+1) Item 줄들(서브쿼리 줄은 더 깊어 건너뜀).
fn dedup_aliases(lines: &mut [Line], mode: DupAlias) {
    if mode == DupAlias::Keep {
        return;
    }
    let n = lines.len();
    let mut i = 0;
    while i < n {
        let is_select = lines[i].role == Role::Clause
            && matches!(lines[i].parts.first(), Some(Part::Text(t)) if t.to_ascii_uppercase().starts_with("SELECT"));
        if !is_select {
            i += 1;
            continue;
        }
        let ind = lines[i].indent;
        let mut items: Vec<usize> = Vec::new();
        let mut j = i + 1;
        while j < n {
            let l = &lines[j];
            if l.role != Role::Item && l.role != Role::Comment && l.indent <= ind {
                break;
            }
            if l.role == Role::Item && l.indent == ind + 1 {
                items.push(j);
            }
            j += 1;
        }
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for &li in &items {
            let Some(a) = alias_of(&lines[li]) else {
                continue;
            };
            let key = a.to_ascii_uppercase();
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push(li),
                None => groups.push((key, vec![li])),
            }
        }
        for (_, idxs) in groups.into_iter().filter(|(_, v)| v.len() > 1) {
            for (k, &li) in idxs.iter().enumerate() {
                let Some(a) = alias_of(&lines[li]) else {
                    continue;
                };
                let new = match (mode, qualifier_of(&lines[li])) {
                    (DupAlias::Qualified, Some(q)) => format!("{a}_{q}"),
                    _ => format!("{a}{}", k + 1),
                };
                set_alias(&mut lines[li], new);
            }
        }
        i = j.max(i + 1);
    }
}

fn alias_of(l: &Line) -> Option<String> {
    l.parts.iter().find_map(|p| match p {
        Part::Alias(a) => Some(a.clone()),
        _ => None,
    })
}

/// 항목 식이 `별칭.컬럼` 꼴이면 그 별칭(테이블 별칭 또는 테이블 이름).
fn qualifier_of(l: &Line) -> Option<String> {
    let text = l.parts.iter().find_map(|p| match p {
        Part::Text(t) => Some(t.trim()),
        _ => None,
    })?;
    let (q, c) = text.split_once('.')?;
    let ident = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '$' | '#'))
            && !s.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    };
    (ident(q) && ident(c)).then(|| q.to_string())
}

fn set_alias(l: &mut Line, new: String) {
    if let Some(p) = l.parts.iter_mut().find(|p| matches!(p, Part::Alias(_))) {
        *p = Part::Alias(new);
    }
}

/// 집합 연산자 앞뒤 대시 구분행(§12 · 키워드 글자 수 · 같은 들여쓰기). 원문에 이미 있던 대시 줄(주석으로 읽힘)은 지우고 새로
/// 넣는다 — 다시 포맷해도 겹치지 않는다(멱등 · 09-30).
fn with_set_op_dashes(lines: Vec<Line>, opts: &Options) -> Vec<Line> {
    let is_dash_line = |l: &Line| {
        l.parts.len() == 1
            && matches!(&l.parts[0], Part::Text(t) if t.len() >= 3 && t.bytes().all(|b| b == b'-'))
    };
    let mut out = Vec::with_capacity(lines.len() + 8);
    for l in lines {
        if is_dash_line(&l) {
            continue;
        }
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

/// ★ 정렬 블록(사용자 09-30 "다중 행 항목도 AS 정렬"): Basic `blocks`(같은 들여쓰기·같은 역할의 연속 줄)에 더해, 같은
/// 들여쓰기·역할의 두 블록 사이에 **더 깊은 들여쓰기 줄만** 있으면(윈도우 `OVER (` … `)` · CASE 여러 줄) 하나로 잇는다 —
/// 그래야 `)` 줄의 AS가 위아래 항목과 같은 열에 선다. 순수 함수.
fn merged_blocks(lines: &[Line]) -> Vec<(usize, usize)> {
    let raw = blocks(lines);
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = out.last_mut() {
            let (ls, le) = *last;
            let same = lines[ls].indent == lines[s].indent && lines[ls].role == lines[s].role;
            let deeper_between = (le..s).all(|i| lines[i].indent > lines[ls].indent);
            if same && s > le && deeper_between {
                *last = (ls, e);
                continue;
            }
            if same && s == le {
                *last = (ls, e);
                continue;
            }
        }
        out.push((s, e));
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
        for (s, e) in merged_blocks(lines) {
            let role = lines[s].role;
            let depth = lines[s].indent;
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
                // (줄, 조각, 조각 앞 화면 폭, 앞 TRIM 글자 폭) · 조각이 없는 항목(별칭·방향 없음)은 폭만 기준에 넣는다
                //   (사용자 09-30 "ORDER BY 방향은 가장 긴 항목 다음 탭 스톱" · 길이 판정은 들여쓰기를 뺀 실제 글자부터).
                let mut widths: Vec<(usize, usize, usize, usize)> = Vec::new();
                let mut others: Vec<usize> = Vec::new();
                for li in s..e {
                    let line = &lines[li];
                    // 블록 안의 더 깊은 줄(윈도우 절 안 PARTITION BY/ORDER BY/프레임)은 정렬 기준이 아니다.
                    if line.indent != depth {
                        continue;
                    }
                    let leading = opts.comma == Comma::Leading
                        && matches!(line.parts.first(), Some(Part::Comma));
                    let prefix = line_prefix(line.indent, leading, opts);
                    match line.parts.iter().position(|p| k.matches(p)) {
                        Some(pi) => {
                            let head =
                                nsql_format::layout::render_parts(&line.parts[..pi], opts, None);
                            let w = display_width(&format!("{prefix}{head}"), tw);
                            let text_w = display_width(head.trim_start(), tw);
                            widths.push((li, pi, w, text_w));
                        }
                        None if matches!(k, AlignKind::As | AlignKind::Dir) => {
                            let all = nsql_format::layout::render_parts(&line.parts, opts, None);
                            let w = display_width(&format!("{prefix}{all}"), tw);
                            // 윈도우 절을 여는 `… OVER (` 줄은 AS가 `)` 줄에 있으므로 기준에서 뺀다(스킬 §8).
                            if display_width(all.trim_start(), tw) <= cfg.outlier_chars
                                && !all.trim_end().ends_with('(')
                            {
                                others.push(w);
                            }
                        }
                        None => {}
                    }
                }
                if widths.is_empty() {
                    continue;
                }
                let normal = widths
                    .iter()
                    .filter(|(_, _, _, tw_)| *tw_ <= cfg.outlier_chars)
                    .map(|(_, _, w, _)| *w)
                    .chain(others.iter().copied());
                let max = normal.max().unwrap_or(0);
                let stop = (max / tw + 1) * tw;
                for (li, pi, _, text_w) in widths {
                    let outlier = text_w > cfg.outlier_chars;
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
            AlignKind::As => matches!(p, Part::As | Part::AsRaw(_)),
            AlignKind::Dir => matches!(p, Part::OrderDir(_)),
            AlignKind::Cmp => matches!(p, Part::CmpOp(_)),
        }
    }
}

/// 설정 키(접두 뒤) → 미리보기에서 처음 드러나는 줄의 조각(`KIROS33_SAMPLE` 기준).
const PREVIEW_MARKS: &[(&str, &str)] = &[
    ("strict", "WHERE1=1"),
    ("align_as", "P.ITEM_CD"),
    ("align_ops", "P.PLANT_CD='1200'"),
    ("align_order", "1ASC"),
    ("outlier_chars", "PLAN_SIZE_GRADE"),
    ("force_as", "P.PLANT_CDAS"),
    ("dup_alias", "T.PLANT_CDAS"),
    ("window_break", "P.PLAN_QTY)OVER("),
    ("and_same_level", "P.PLANT_CD='1200'"),
    ("set_op_dashes", "UNIONALL"),
];

impl Extension for Kiros33 {
    fn meta() -> Meta {
        Meta {
            id: "sql-formatter-kiros33".into(),
            name: "SQL Formatter for kiros33".into(),
            settings_prefix: "ext.sqlfmt_kiros33.".into(),
            commands: vec![],
            menus: vec![],
            formatter: Some(Formatter {
                label: Label::new("SQL Formatter for kiros33", "SQL Formatter for kiros33"),
                // 기본 샘플 + 확장 설정별 확인 예제(사용자 09-30).
                sample: format!("{}\n{}", nsql_format::SAMPLE_SQL, KIROS33_SAMPLE),
                // ★ 설정 카드의 "▶ 미리보기 n행" 조각(09-30 "확장 것은 확장에") — 미리보기 결과에서 공백을 지우고 대문자로
                //   바꾼 뒤 처음 포함되는 줄. 호스트 내장 표에는 Basic(`format.*`)만 남았다.
                marks: PREVIEW_MARKS
                    .iter()
                    .map(|(k, m)| (format!("ext.sqlfmt_kiros33.{k}"), (*m).to_string()))
                    .collect(),
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
        // force_as(기본) = 순수 컬럼도 AS 컬럼명 · 4항목 모두 다음 탭 스톱(16)에 AS.
        let want = concat!(
            "SELECT\n\ta.ord_no\tAS\tord_no\n,\ta.item_cd\tAS\titem_cd\n,\ta.ord_qty\tAS\tqty\n,\ta.ord_amt\tAS\tamt\n",
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

    /// 윈도우 함수(스킬 §8 · 사용자 09-30): 길면 `OVER (` 줄바꿈 · 절마다 한 줄(한 단계 안) · `)` 뒤 탭 1개 + AS · 짧으면 한 줄 · 정렬 계획은 `)` 줄을 건너뛴다.
    #[test]
    fn window_over_multiline_per_skill() {
        let out = format(
            "select a.k, sum(a.q) over (partition by a.k order by a.d rows between 2 preceding and current row) mov3, row_number() over (order by a.d) rn from t a",
            &Options::default(),
            &Cfg::default(),
        );
        assert!(
            out.contains(",\tsum(a.q) OVER (\n\t\tPARTITION BY a.k\n\t\tORDER BY a.d\n\t\tROWS BETWEEN 2 PRECEDING AND CURRENT ROW\n\t)"),
            "{out}"
        );
        // ★ `)` 줄의 AS도 같은 블록 정렬 열(사용자 09-30 "다중 행 항목도 AS 정렬 대상") — 열 = 탭 4 기준 화면 폭.
        let col_of = |needle: &str| -> usize {
            let line = out
                .lines()
                .find(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle}\n{out}"));
            let head = &line[..line.find("AS").expect("AS")];
            display_width(head, 4)
        };
        assert_eq!(col_of("AS\tmov3"), col_of("AS\trn"), "{out}");
        assert!(out.contains("\t)\t"), "{out}");
        assert!(
            out.contains("row_number() OVER (ORDER BY a.d)"),
            "짧은 것은 한 줄: {out}"
        );
        assert_eq!(
            format(&out, &Options::default(), &Cfg::default()),
            out,
            "idempotent"
        );
        let cfg = Cfg {
            window_break: 0,
            ..Cfg::default()
        };
        let out = format("select sum(a.q) over (partition by a.k order by a.d rows between 2 preceding and current row) mov3 from t a", &Options::default(), &cfg);
        assert!(!out.contains("OVER (\n"), "0 = 한 줄: {out}");
    }

    /// ORDER BY 방향 정렬(사용자 09-30): 방향 없는 항목도 기준에 들어 가장 긴 항목 다음 탭 스톱에 DESC · strict = `;` 다음 줄 1칸.
    #[test]
    fn order_dir_aligns_past_longest_item_and_semicolon_newline() {
        let out = format(
            "select b.qty from t b order by b.plant_cd, b.qty desc;",
            &Options::default(),
            &Cfg::default(),
        );
        assert!(
            out.contains("ORDER BY\n\tb.plant_cd\n,\tb.qty\t\tDESC\n;\n"),
            "{out}"
        );
    }

    /// 중복 별칭 3택(사용자 09-30): 순번 / 별칭 붙임 / 그대로 · 별칭 없는 테이블은 순번으로.
    #[test]
    fn duplicate_aliases_three_modes() {
        let src = "select p.plant_cd, t.plant_cd, p.item_cd from tb_plan p join tb_plant t on t.plant_cd = p.plant_cd";
        let run = |mode: DupAlias| {
            format(
                src,
                &Options::default(),
                &Cfg {
                    dup_alias: mode,
                    ..Cfg::default()
                },
            )
        };
        let n = run(DupAlias::Numbered);
        assert!(
            n.contains("AS\tplant_cd1\n") && n.contains("AS\tplant_cd2\n"),
            "{n}"
        );
        assert!(
            n.contains("p.item_cd\tAS\titem_cd\n"),
            "겹치지 않으면 그대로: {n}"
        );
        let q = run(DupAlias::Qualified);
        assert!(
            q.contains("AS\tplant_cd_p\n") && q.contains("AS\tplant_cd_t\n"),
            "{q}"
        );
        let k = run(DupAlias::Keep);
        assert_eq!(k.matches("AS\tplant_cd\n").count(), 2, "{k}");
        assert_eq!(
            run(DupAlias::Numbered),
            format(&n, &Options::default(), &Cfg::default()),
            "idempotent"
        );
    }

    /// 확장 예제(사용자 09-30): 설정마다 드러나는 자리가 있고 · 토큰 보존 · 멱등 · 켜고 끄면 결과가 바뀐다.
    #[test]
    fn kiros33_sample_exercises_every_setting() {
        let on = format(KIROS33_SAMPLE, &Options::default(), &Cfg::default());
        // AS 정렬: 짧은 식은 탭 3개 · 가장 긴 식(nvl…)은 탭 1개 → AS가 같은 열(다음 탭 스톱 24).
        // 순수 컬럼도 AS 컬럼명 · JOIN 중복 = 순번(기본).
        assert!(on.contains("\tp.plant_cd\t\t\tAS\tplant_cd1\n"), "{on}");
        assert!(on.contains(",\tt.plant_cd\t\t\tAS\tplant_cd2\n"), "{on}");
        assert!(
            on.contains(",\tp.item_cd\t\t\tAS\titem\n"),
            "force_as + align_as: {on}"
        );
        assert!(on.contains(",\tnvl(p.plan_qty, 0)\tAS\tqty\n"), "{on}");
        assert!(on.contains(",\ttrim(p.remark_txt)\tAS\tremark\n"), "{on}");
        assert!(
            on.contains("END\tAS\tplan_size_grade\n"),
            "outlier = 앞뒤 탭 1개씩: {on}"
        );
        // 연산자 정렬: 짧은 왼쪽 식은 탭 3개 · 가장 긴 식(nvl…)은 탭 1개 → 연산자가 같은 열.
        assert!(
            on.contains("AND\tp.plant_cd\t\t\t=\t'1200'\n"),
            "align_ops: {on}"
        );
        assert!(on.contains("AND\tnvl(p.plan_qty, 0)\t>=\t10\n"), "{on}");
        assert!(
            on.contains("---------\nUNION ALL\n---------\n"),
            "set_op_dashes: {on}"
        );
        assert!(
            on.contains("\t1\tASC\n") && on.contains("\t3\tDESC\n"),
            "align_order: {on}"
        );
        assert_eq!(
            format(&on, &Options::default(), &Cfg::default()),
            on,
            "idempotent"
        );
        let off = Cfg {
            align_as: false,
            align_ops: false,
            align_order: false,
            force_as: false,
            set_op_dashes: false,
            ..Cfg::default()
        };
        let off = format(KIROS33_SAMPLE, &Options::default(), &off);
        assert!(off.contains("p.item_cd item\n"), "{off}");
        assert!(
            off.contains("\tp.plant_cd\n"),
            "force_as 끔 = 순수 컬럼 그대로: {off}"
        );
        assert!(
            off.contains("nvl(p.plan_qty, 0)\tAS\tqty\n"),
            "정렬 끔 = 탭 1개: {off}"
        );
        assert!(
            off.contains("AND\tp.plant_cd\t=\t'1200'\n"),
            "정렬 끔 = 간격만(strict 탭): {off}"
        );
        assert!(!off.contains("---------"), "{off}");
    }

    /// 1.2.0(사용자 09-30): AS 강제(기본) · 아웃라이어 = 정렬 제외 + AS 앞뒤 공백 하나 · 짧은 것끼리는 여전히 탭 정렬.
    #[test]
    fn force_as_and_outlier_plain_spaces() {
        let out = format(
            "select a.k, sum(a.q) total, round(sum(a.q) over (partition by a.k) / nullif(count(a.k) over (partition by a.k), 0), 2) avg_q from t a",
            &Options::default(),
            &Cfg::default(),
        );
        // force_as(기본) = 순수 컬럼도 AS 컬럼명(가장 긴 항목 sum(a.q) 다음 탭 스톱 16에 맞춰 탭 3개).
        assert!(out.contains("\ta.k\t\t\tAS\tk\n"), "{out}");
        assert!(
            out.contains(",\tsum(a.q)\tAS\ttotal\n"),
            "AS 강제 + 탭 정렬: {out}"
        );
        assert!(
            out.contains(", 2)\tAS\tavg_q\n"),
            "아웃라이어 = 앞뒤 탭 1개씩: {out}"
        );
        let cfg = Cfg {
            force_as: false,
            ..Cfg::default()
        };
        let out = format("select sum(a.q) total from t a", &Options::default(), &cfg);
        assert!(out.contains("sum(a.q) total"), "{out}");
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
            force_as: false,
            ..Cfg::default()
        };
        let out = format("select a, b from t", &common, &cfg);
        assert_eq!(out, "SELECT\n  a,\n  b\nFROM\n  t\n");
    }

    /// ★ 미리보기 표식(09-30): 모든 설정 조각이 기본 설정의 kiros33 미리보기 결과에 실제로 있다(공백 제거 · 대문자).
    #[test]
    fn preview_marks_all_present() {
        let sample = format!("{}\n{}", nsql_format::SAMPLE_SQL, KIROS33_SAMPLE);
        let out = format(&sample, &Options::default(), &Cfg::default());
        let squash = |s: &str| -> String {
            s.chars()
                .filter(|c| !c.is_whitespace())
                .flat_map(char::to_uppercase)
                .collect()
        };
        for (key, mark) in PREVIEW_MARKS {
            let found = out.lines().any(|l| squash(l).contains(&squash(mark)));
            assert!(found, "{key} ({mark})\n{out}");
        }
        let meta = Kiros33::meta();
        let f = meta.formatter.expect("formatter");
        assert_eq!(f.marks.len(), PREVIEW_MARKS.len());
        assert!(f
            .marks
            .iter()
            .all(|(k, _)| k.starts_with("ext.sqlfmt_kiros33.")));
    }
}
