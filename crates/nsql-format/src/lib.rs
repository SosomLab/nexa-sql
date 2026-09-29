//! **nsql-format** — SQL 포맷터 핵심(의존 0). 앱의 내장 **Basic formatter**와 WASM 포맷터 확장(예: SQL Formatter for
//! kiros33)이 같은 렉서·레이아웃 IR·공통 옵션을 쓴다(사용자 09-28 "공통 특성은 Basic을 준용").
//!
//! - [`lexer`]: 토큰(주석·문자열·힌트·바인드 포함 · 줄 바꿈 수 · 원문 위치).
//! - [`Options`]: **공통 옵션**(앱 설정 `format.*` ↔ 문자열 쌍으로 오간다 · [`Options::from_pairs`]).
//! - [`layout`]: 문장 → 줄 IR([`layout::Line`]/[`layout::Part`]) — 절·항목·조건·조인·서브쿼리·CASE 배치. 확장은 이 IR 위에
//!   정렬·구분행 같은 자기 규칙을 얹는다.
//! - [`format_basic`]: Basic 포맷터 = 레이아웃 + 공백 렌더.
//!
//! 불변식: **토큰 보존** — 공백만 바꾼다(옵션이 켜진 시드 `1=1`·`AS` 추가·대소문자 변환은 명시적 예외).

pub mod layout;
pub mod lexer;

pub use layout::{Line, Part, Role};
pub use lexer::{lex, Kind, Token};

/// 들여쓰기 단위.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    Tab,
    Spaces(u8),
}

/// 대소문자 규칙.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Keep,
    Upper,
    Lower,
}

impl Case {
    fn parse(s: &str) -> Case {
        match s.trim().to_ascii_lowercase().as_str() {
            "upper" => Case::Upper,
            "lower" => Case::Lower,
            _ => Case::Keep,
        }
    }
    pub(crate) fn apply(self, s: &str) -> String {
        match self {
            Case::Keep => s.to_string(),
            Case::Upper => s.to_ascii_uppercase(),
            Case::Lower => s.to_ascii_lowercase(),
        }
    }
}

/// 콤마 위치.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comma {
    /// 줄 앞(`,` + 간격 + 항목).
    Leading,
    /// 줄 끝(항목 + `,`).
    Trailing,
}

/// 콤마 뒤 간격 · 인라인 주석 앞 간격.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    Space,
    Tab,
}

/// 목록(SELECT 항목 · GROUP/ORDER BY …) 배치.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListStyle {
    /// 항목마다 한 줄.
    Multi,
    /// 한 줄에 이어 붙인다(콤마 + 공백).
    Single,
    /// `line_width` 안에 들면 한 줄, 넘치면 항목마다.
    Auto,
}

/// 개행.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newline {
    Lf,
    CrLf,
    /// 원문의 첫 개행을 따른다(없으면 LF).
    Keep,
}

/// AND/OR 자리.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalNewline {
    /// 줄 앞(`AND x = 1`).
    Before,
    /// 줄 끝(`x = 1 AND`).
    After,
}

/// 명시적 `AS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasAs {
    Keep,
    Add,
    Remove,
}

impl AliasAs {
    fn parse(s: &str) -> AliasAs {
        match s.trim().to_ascii_lowercase().as_str() {
            "add" => AliasAs::Add,
            "remove" => AliasAs::Remove,
            _ => AliasAs::Keep,
        }
    }
}

/// ★ 공통 옵션(앱 설정 `format.*` · Basic과 확장이 같이 읽는다).
/// 방언 치환 대상(스킬 §22 · T-255): 포맷은 그대로, **함수·문법 이름만** 치환(`None` = 안 함).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DialectTarget {
    #[default]
    None,
    Ansi,
    Oracle,
    Tsql,
}

impl DialectTarget {
    #[must_use]
    pub fn parse(s: &str) -> DialectTarget {
        match s.trim().to_ascii_lowercase().as_str() {
            "ansi" => DialectTarget::Ansi,
            "oracle" => DialectTarget::Oracle,
            "tsql" | "mssql" | "sqlserver" => DialectTarget::Tsql,
            _ => DialectTarget::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub indent: Indent,
    /// 탭 폭(정렬 계산 · 탭 스톱).
    pub tab_width: usize,
    pub keyword_case: Case,
    pub identifier_case: Case,
    pub function_case: Case,
    pub comma: Comma,
    pub comma_gap: Gap,
    /// ★ `WHERE`/`ON`/`HAVING`과 시드 `1=1` 사이(사용자 09-29): 공백 하나 또는 탭.
    pub seed_gap: Gap,
    /// ★ WHERE/HAVING의 조건 줄(AND/OR)을 절 키워드보다 한 단계 안에(true · 기본) / 같은 열에(false · 스킬 §3).
    pub cond_indent: bool,
    pub logical_newline: LogicalNewline,
    /// `WHERE 1=1` · `ON 1=1` · `HAVING 1=1` 시드(조건은 다음 줄부터 AND/OR).
    pub where_seed: bool,
    pub list_style: ListStyle,
    /// 한 줄 상한(Auto 목록 · CASE 한 줄 판정 · 긴 표현식).
    pub line_width: usize,
    /// 이항 연산자 양쪽 공백.
    pub operator_spaces: bool,
    /// 모든 SELECT 열에 별칭(단순 열 참조 `A.COL` → `A.COL AS COL` · 식은 그대로).
    pub column_alias_all: bool,
    /// 열 별칭의 `AS` 키워드.
    pub column_as: AliasAs,
    /// 테이블 별칭의 `AS` 키워드.
    pub table_as: AliasAs,
    /// JOIN 줄을 FROM 항목처럼 한 단계 들여쓴다(off = FROM과 같은 열).
    pub join_indent: bool,
    /// 문장 사이 빈 줄(최소).
    pub stmt_blank_lines: usize,
    /// 연속 빈 줄 상한.
    pub max_blank_lines: usize,
    /// 원문이 한 줄로 완결된 DML은 펼치지 않는다(§19).
    pub keep_oneliners: bool,
    /// CASE 한 줄 상한(글자 · 넘치면 WHEN마다 줄).
    pub case_inline_max: usize,
    pub newline: Newline,
    pub final_newline: bool,
    /// `;`를 문장 끝 줄이 아니라 다음 줄에.
    pub semicolon_newline: bool,
    /// ★ 괄호 AND/OR 그룹 시드(스킬 §3 · T-255): `(1=1` / `(1=0` + 하위 조건 한 단계 더 + `)` 줄.
    pub paren_seed: bool,
    /// ★ FROM/JOIN의 별칭 없는 테이블·인라인뷰에 `A`, `B`, … 자동 부여(스킬 2-1 · 문장 안 식별자와 안 겹치게).
    pub auto_alias: bool,
    /// ★ 방언 치환(스킬 §22).
    pub dialect_target: DialectTarget,
    /// ★ 테이블 설명 주석(스킬 2-2): (대문자 `SCHEMA.TABLE` 또는 `TABLE`, 설명) — 호스트가 메타에서 채운다 · FROM/JOIN 테이블 줄 끝에 `--\t설명`.
    pub table_comments: Vec<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            indent: Indent::Tab,
            tab_width: 4,
            keyword_case: Case::Upper,
            identifier_case: Case::Keep,
            function_case: Case::Keep,
            comma: Comma::Leading,
            comma_gap: Gap::Space,
            seed_gap: Gap::Space,
            cond_indent: true,
            logical_newline: LogicalNewline::Before,
            where_seed: false,
            list_style: ListStyle::Multi,
            line_width: 120,
            operator_spaces: true,
            column_alias_all: false,
            column_as: AliasAs::Keep,
            table_as: AliasAs::Keep,
            join_indent: false,
            stmt_blank_lines: 1,
            max_blank_lines: 2,
            keep_oneliners: true,
            case_inline_max: 120,
            newline: Newline::Keep,
            final_newline: true,
            semicolon_newline: false,
            paren_seed: false,
            auto_alias: false,
            dialect_target: DialectTarget::None,
            table_comments: Vec::new(),
        }
    }
}

/// 설정 키(접두 `format.` 없이) 목록 — 앱 레지스트리·SDK 문서·미리보기가 같은 표를 쓴다.
pub const OPTION_KEYS: &[&str] = &[
    "indent",
    "indent_width",
    "keyword_case",
    "identifier_case",
    "function_case",
    "comma",
    "comma_gap",
    "seed_gap",
    "cond_indent",
    "logical_newline",
    "where_seed",
    "list_style",
    "line_width",
    "operator_spaces",
    "column_alias_all",
    "column_as",
    "table_as",
    "join_indent",
    "stmt_blank_lines",
    "max_blank_lines",
    "keep_oneliners",
    "case_inline_max",
    "newline",
    "final_newline",
    "semicolon_newline",
    "paren_seed",
    "auto_alias",
    "dialect_target",
];

fn flag(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "on" | "true" | "1" | "yes"
    )
}

impl Options {
    /// 문자열 쌍(키는 `format.` 접두가 있어도 없어도 됨 · 값 = 설정 원문)에서 옵션을 만든다. 모르는 키는 무시 · 빠진 키 = 기본.
    #[must_use]
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Options {
        let mut o = Options::default();
        for (k, v) in pairs {
            let key = k.strip_prefix("format.").unwrap_or(k);
            let lv = v.trim().to_ascii_lowercase();
            match key {
                "indent" => {
                    o.indent = if lv == "space" || lv == "spaces" {
                        Indent::Spaces(o.tab_width.clamp(1, 16) as u8)
                    } else {
                        Indent::Tab
                    };
                }
                "indent_width" | "tab_width" => {
                    if let Ok(n) = lv.parse::<usize>() {
                        o.tab_width = n.clamp(1, 16);
                        if let Indent::Spaces(_) = o.indent {
                            o.indent = Indent::Spaces(o.tab_width as u8);
                        }
                    }
                }
                "keyword_case" => o.keyword_case = Case::parse(&lv),
                "identifier_case" => o.identifier_case = Case::parse(&lv),
                "function_case" => o.function_case = Case::parse(&lv),
                "comma" => {
                    o.comma = if lv == "trailing" || lv == "end" {
                        Comma::Trailing
                    } else {
                        Comma::Leading
                    };
                }
                "comma_gap" => o.comma_gap = if lv == "tab" { Gap::Tab } else { Gap::Space },
                "seed_gap" => o.seed_gap = if lv == "tab" { Gap::Tab } else { Gap::Space },
                "cond_indent" => o.cond_indent = !(lv == "same" || lv == "off" || lv == "0"),
                "logical_newline" => {
                    o.logical_newline = if lv == "after" {
                        LogicalNewline::After
                    } else {
                        LogicalNewline::Before
                    };
                }
                "where_seed" => o.where_seed = flag(v),
                "paren_seed" => o.paren_seed = flag(v),
                "auto_alias" => o.auto_alias = flag(v),
                "dialect_target" => o.dialect_target = DialectTarget::parse(&lv),
                "list_style" => {
                    o.list_style = match lv.as_str() {
                        "single" => ListStyle::Single,
                        "auto" => ListStyle::Auto,
                        _ => ListStyle::Multi,
                    };
                }
                "line_width" => {
                    if let Ok(n) = lv.parse::<usize>() {
                        o.line_width = n.clamp(40, 400);
                    }
                }
                "operator_spaces" => o.operator_spaces = flag(v),
                "column_alias_all" => o.column_alias_all = flag(v),
                "column_as" => o.column_as = AliasAs::parse(&lv),
                "table_as" => o.table_as = AliasAs::parse(&lv),
                "join_indent" => o.join_indent = flag(v),
                "stmt_blank_lines" => {
                    if let Ok(n) = lv.parse::<usize>() {
                        o.stmt_blank_lines = n.min(5);
                    }
                }
                "max_blank_lines" => {
                    if let Ok(n) = lv.parse::<usize>() {
                        o.max_blank_lines = n.min(10);
                    }
                }
                "keep_oneliners" => o.keep_oneliners = flag(v),
                "case_inline_max" => {
                    if let Ok(n) = lv.parse::<usize>() {
                        o.case_inline_max = n.clamp(20, 400);
                    }
                }
                "newline" => {
                    o.newline = match lv.as_str() {
                        "lf" => Newline::Lf,
                        "crlf" => Newline::CrLf,
                        _ => Newline::Keep,
                    };
                }
                "final_newline" => o.final_newline = flag(v),
                "semicolon_newline" => o.semicolon_newline = flag(v),
                _ => {}
            }
        }
        // 공백 들여쓰기 폭은 탭 폭을 따른다(설정 `format.indent_width` 하나).
        if let Indent::Spaces(_) = o.indent {
            o.indent = Indent::Spaces(o.tab_width as u8);
        }
        o
    }

    /// 들여쓰기 한 단계 문자열.
    #[must_use]
    pub fn indent_unit(&self) -> String {
        match self.indent {
            Indent::Tab => "\t".to_string(),
            Indent::Spaces(n) => " ".repeat(n as usize),
        }
    }

    /// 콤마 뒤 간격 문자열.
    #[must_use]
    pub fn gap(&self) -> &'static str {
        match self.comma_gap {
            Gap::Space => " ",
            Gap::Tab => "\t",
        }
    }
}

/// 절을 시작하거나 배치에 관여하는 예약어(대문자) — 대소문자 변환·별칭 판정의 기준.
pub const KEYWORDS: &[&str] = &[
    "SELECT",
    "DISTINCT",
    "ALL",
    "FROM",
    "WHERE",
    "GROUP",
    "BY",
    "HAVING",
    "ORDER",
    "LIMIT",
    "OFFSET",
    "FETCH",
    "FIRST",
    "NEXT",
    "ROWS",
    "ROW",
    "ONLY",
    "UNION",
    "INTERSECT",
    "MINUS",
    "EXCEPT",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "OUTER",
    "CROSS",
    "NATURAL",
    "JOIN",
    "ON",
    "USING",
    "AND",
    "OR",
    "NOT",
    "IN",
    "IS",
    "NULL",
    "LIKE",
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "AS",
    "ASC",
    "DESC",
    "NULLS",
    "LAST",
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "MERGE",
    "MATCHED",
    "WITH",
    "RECURSIVE",
    "OVER",
    "PARTITION",
    "ROLLUP",
    "CUBE",
    "GROUPING",
    "SETS",
    "START",
    "CONNECT",
    "PRIOR",
    "TOP",
    "APPLY",
    "PIVOT",
    "UNPIVOT",
    "FOR",
    "RETURNING",
    "QUALIFY",
    "WINDOW",
    "LATERAL",
    "TABLE",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "DECLARE",
    "BEGIN",
    "EXCEPTION",
    "COMMIT",
    "ROLLBACK",
    "GRANT",
    "REVOKE",
    "EXEC",
    "EXECUTE",
    "CALL",
    "REPLACE",
    "PROCEDURE",
    "FUNCTION",
    "PACKAGE",
    "TRIGGER",
    "TYPE",
    "VIEW",
    "INDEX",
    "SEQUENCE",
    "ANY",
    "SOME",
    "ESCAPE",
    "INTERVAL",
    "CAST",
    "DELETE",
    "DEFAULT",
    "IF",
    "LOOP",
    "WHILE",
    "RETURN",
    "BODY",
    "IS",
];

/// 예약어인가(대소문자 무시).
#[must_use]
pub fn is_keyword(w: &str) -> bool {
    let up = w.to_ascii_uppercase();
    KEYWORDS.contains(&up.as_str())
}

/// ★ Basic 포맷터 — 레이아웃 + 공백 렌더(옵션대로).
#[must_use]
pub fn format_basic(src: &str, opts: &Options) -> String {
    let lines = layout::layout(src, opts);
    layout::render(&lines, opts, src)
}

/// 미리보기용 예시 SQL(설정 창·미리보기 탭이 문서가 비었을 때 쓴다 · 확장은 메타 `formatter.sample`로 자기 예시를 줄 수 있다).
pub const SAMPLE_SQL: &str = "with base as (select a.plant_cd, a.item_cd, sum(a.qty) qty from tb_demand a where a.yymm between '202601' and '202612' and a.del_yn is null group by a.plant_cd, a.item_cd)\nselect b.plant_cd, b.item_cd, b.qty, case when b.qty > 100 then 'HIGH' when b.qty > 0 then 'LOW' else 'NONE' end as qty_grp, (select max(c.yymm) from tb_demand c where c.item_cd = b.item_cd) last_yymm\nfrom base b inner join tb_item i on i.item_cd = b.item_cd left outer join tb_plant p on p.plant_cd = b.plant_cd\nwhere b.qty > 0 and p.use_yn = 'Y' and i.item_type_cd in ('FG', 'SF')\norder by b.plant_cd, b.qty desc;\n\nupdate tb_plan_result a set a.qty = 0, a.modify_date = sysdate where a.plant_cd = '1200';\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse_pairs() {
        let o = Options::from_pairs([
            ("format.indent", "space"),
            ("format.indent_width", "2"),
            ("format.comma", "trailing"),
            ("comma_gap", "tab"),
            ("where_seed", "on"),
            ("list_style", "auto"),
            ("line_width", "80"),
            ("column_as", "add"),
            ("newline", "crlf"),
            ("nope", "x"),
        ]);
        assert_eq!(o.indent, Indent::Spaces(2));
        assert_eq!(o.tab_width, 2);
        assert_eq!(o.comma, Comma::Trailing);
        assert_eq!(o.comma_gap, Gap::Tab);
        assert!(o.where_seed);
        assert_eq!(o.list_style, ListStyle::Auto);
        assert_eq!(o.line_width, 80);
        assert_eq!(o.column_as, AliasAs::Add);
        assert_eq!(o.newline, Newline::CrLf);
        assert_eq!(o.indent_unit(), "  ");
    }

    #[test]
    fn keyword_table() {
        assert!(is_keyword("select") && is_keyword("Group") && !is_keyword("plant_cd"));
    }
}
