//! ★ **실행 계획 검토**(사용자 10-23 "실행 계획을 검토하는 기능") — 방언별 EXPLAIN 관용으로 받은 결과를 **공통 트리**([`PlanNode`])로
//! 풀고, 자주 보는 문제(전체 스캔 · 카티전 조인 · 정렬/임시 테이블 · 비용 집중 · 추정치 불일치 · 인덱스 미사용 · 컬럼 함수)를
//! [`Finding`]으로 짚는다. 순수 함수뿐(의존 0 · 호스트가 결과 행을 문자열로 넘긴다) — GUI(실행 ▸ 실행 계획 / 실제 실행 계획)와
//! CLI(`nsql explain [--analyze] [--raw]`)가 같은 길을 쓴다. 문서 = docs/122.
//!
//! - 추정 계획(`analyze = false`) = 실행하지 않는다 · 실제 계획(`analyze = true`) = **문장을 실제로 실행**한다(DML이면 바뀐다 → 호스트가 경고).
//! - 방언별 원문: PostgreSQL `EXPLAIN (VERBOSE, COSTS[, ANALYZE, BUFFERS])` 텍스트 · Oracle `DBMS_XPLAN.DISPLAY` / `DISPLAY_CURSOR('ALLSTATS LAST')` 표 ·
//!   SQL Server `SET SHOWPLAN_ALL ON` / `SET STATISTICS PROFILE ON` 결과 집합 · MySQL/MariaDB `EXPLAIN` 표(/ `EXPLAIN ANALYZE` 트리) ·
//!   SQLite `EXPLAIN QUERY PLAN` · DuckDB `EXPLAIN [ANALYZE]` 상자 그림 · ODBC = 뒤 DBMS를 모르니 `EXPLAIN` 원문만.
use nsql_core::Dialect;

/// 계획 요청 옵션.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanOptions {
    /// 실제 실행 계획(문장을 실행하고 실측 행 수·시간을 받는다).
    pub analyze: bool,
}

/// 공통 계획 노드(트리는 `depth`로 표현 · 부모 = 앞쪽에서 가장 가까운 depth-1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanNode {
    pub depth: usize,
    /// 연산(예: `TABLE ACCESS FULL` · `Seq Scan` · `Clustered Index Seek` · `SCAN`).
    pub op: String,
    /// 대상 객체(테이블·인덱스) — 모르면 빈 문자열.
    pub object: String,
    /// 추정 행 수.
    pub rows: Option<f64>,
    /// 실측 행 수(실제 계획일 때).
    pub actual_rows: Option<f64>,
    /// 비용(방언 단위 · PG = 총 비용 상한 · Oracle = Cost · MSSQL = TotalSubtreeCost · MySQL = rows × filtered).
    pub cost: Option<f64>,
    /// 실측 시간(ms · 실제 계획일 때).
    pub time_ms: Option<f64>,
    /// 부가 정보(필터·조건·정렬 방식·경고 …).
    pub detail: String,
}

/// 검토 결과의 무게.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    High,
}

impl Severity {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Warn => "WARN",
            Severity::High => "HIGH",
        }
    }
}

/// 검토 한 건(어느 노드에 대한 것인지 · 사람이 읽는 메시지 · 두 언어는 `tr!`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// `nodes` 인덱스(전체 계획에 대한 것이면 None).
    pub node: Option<usize>,
    pub message: String,
}

/// 호스트에 돌려주는 보고서.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanReport {
    pub dialect: Option<Dialect>,
    pub analyzed: bool,
    pub nodes: Vec<PlanNode>,
    /// 원문 줄(방언 결과를 한 줄씩 · 파싱이 안 돼도 보여 준다).
    pub raw: Vec<String>,
    pub findings: Vec<Finding>,
}

/// 방언별 EXPLAIN 스크립트(옵션판). `stmt`는 한 문장(끝 `;` 없어도 됨).
#[must_use]
pub fn explain_script_opts(dialect: Dialect, stmt: &str, opts: PlanOptions) -> String {
    let s = stmt.trim().trim_end_matches(';').trim();
    match (dialect, opts.analyze) {
        // ClickHouse(10-24): `EXPLAIN`(PLAN) 하나 — 실제 실행 통계는 없다(`EXPLAIN PIPELINE`·`ESTIMATE`는 후속).
        (Dialect::Clickhouse, _) => format!("EXPLAIN {s};\n"),
        // ★ PL/SQL `EXECUTE IMMEDIATE`로 감싼다(10-23 실측): Thin 드라이버(oracledb)는 `EXPLAIN`을 질의·DML·DDL 어디에도 넣지 않아 **파싱만**
        //   하고 실행하지 않는다(PLAN_TABLE 0행 → "cannot fetch last explain plan"). 블록 안에서는 두 드라이버 모두 실행된다. `'`는 `''`로.
        (Dialect::Oracle, false) => format!(
            "BEGIN EXECUTE IMMEDIATE 'EXPLAIN PLAN FOR {}'; END;\n/\nSELECT * FROM TABLE(DBMS_XPLAN.DISPLAY(NULL, NULL, 'TYPICAL'));\n",
            s.replace('\'', "''")
        ),
        // 실제 계획 = GATHER_PLAN_STATISTICS 힌트로 실행한 뒤 같은 세션의 마지막 커서를 DISPLAY_CURSOR로(V$SQL_PLAN 읽기 권한 필요).
        (Dialect::Oracle, true) => format!(
            "{};\nSELECT * FROM TABLE(DBMS_XPLAN.DISPLAY_CURSOR(NULL, NULL, 'ALLSTATS LAST'));\n",
            with_oracle_hint(s, "GATHER_PLAN_STATISTICS")
        ),
        // DECLARE로 시작해야 어댑터가 sp_executesql(RPC)이 아닌 배치로 보낸다 — SHOWPLAN 결과 집합은 배치에서만 온다(09-15 실측).
        (Dialect::Mssql, false) => format!(
            "SET SHOWPLAN_ALL ON;\nGO\nDECLARE @nsql_plan BIT;\n{s};\nGO\nSET SHOWPLAN_ALL OFF;\nGO\n"
        ),
        (Dialect::Mssql, true) => format!(
            "SET STATISTICS PROFILE ON;\nGO\nDECLARE @nsql_plan BIT;\n{s};\nGO\nSET STATISTICS PROFILE OFF;\nGO\n"
        ),
        (Dialect::Postgres, false) => format!("EXPLAIN (VERBOSE, COSTS) {s};\n"),
        (Dialect::Postgres, true) => {
            format!("EXPLAIN (ANALYZE, BUFFERS, VERBOSE, COSTS) {s};\n")
        }
        (Dialect::Sqlite, _) => format!("EXPLAIN QUERY PLAN {s};\n"),
        (Dialect::Mysql, false) => format!("EXPLAIN {s};\n"),
        (Dialect::Mysql, true) => format!("EXPLAIN ANALYZE {s};\n"),
        (Dialect::Duckdb, false) => format!("EXPLAIN {s};\n"),
        (Dialect::Duckdb, true) => format!("EXPLAIN ANALYZE {s};\n"),
        (Dialect::Odbc, _) => format!("EXPLAIN {s};\n"),
    }
}

/// `SELECT …` 첫 낱말 뒤에 힌트를 끼운다(이미 힌트 블록이 있으면 그 안에 덧붙인다 · SELECT가 아니면 그대로).
fn with_oracle_hint(stmt: &str, hint: &str) -> String {
    let trimmed = stmt.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("select") else {
        return stmt.to_string();
    };
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return stmt.to_string();
    }
    let after = &trimmed[6..];
    let after_trim = after.trim_start();
    if let Some(body) = after_trim.strip_prefix("/*+") {
        // 기존 힌트 블록 안에 추가.
        return format!("SELECT /*+ {hint} {}", body.trim_start());
    }
    format!("SELECT /*+ {hint} */ {after_trim}")
}

/// 숫자 낱말(`1,234` · `12.5` · `1.2e3` · `1K`/`1M`(DuckDB)) → f64.
fn num(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches(|c| c == '|' || c == '*').trim();
    if t.is_empty() || t == "-" {
        return None;
    }
    let cleaned: String = t.chars().filter(|c| *c != ',').collect();
    if let Ok(v) = cleaned.parse::<f64>() {
        return Some(v);
    }
    let (body, mul) = match cleaned.chars().last() {
        Some('K') => (&cleaned[..cleaned.len() - 1], 1e3),
        Some('M') => (&cleaned[..cleaned.len() - 1], 1e6),
        Some('G') => (&cleaned[..cleaned.len() - 1], 1e9),
        _ => return None,
    };
    body.parse::<f64>().ok().map(|v| v * mul)
}

/// `key=value` 꼴 토큰에서 값을 찾는다(`rows=14` · `cost=0.00..1.14`).
fn kv(text: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=");
    let i = text.find(&pat)?;
    let rest = &text[i + pat.len()..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ')')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// 결과 행(문자열)을 방언에 맞게 트리로. `columns` = 결과 열 이름(소문자 비교) · `rows` = 셀 문자열.
#[must_use]
pub fn parse_plan(dialect: Dialect, columns: &[String], rows: &[Vec<String>]) -> Vec<PlanNode> {
    match dialect {
        Dialect::Postgres => parse_pg(rows),
        Dialect::Oracle => parse_oracle(rows),
        Dialect::Mssql => parse_mssql(columns, rows),
        Dialect::Mysql => parse_mysql(columns, rows),
        Dialect::Sqlite => parse_sqlite(columns, rows),
        Dialect::Duckdb => parse_duckdb(rows),
        Dialect::Odbc | Dialect::Clickhouse => parse_generic(columns, rows),
    }
}

fn first_cell(r: &[String]) -> &str {
    r.first().map(String::as_str).unwrap_or("")
}

/// PostgreSQL / MySQL TREE / EXPLAIN ANALYZE 공통: `->` 들여쓰기 트리 + `(cost=… rows=…)` + `(actual time=… rows=… loops=…)`.
fn parse_arrow_tree(lines: impl Iterator<Item = String>) -> Vec<PlanNode> {
    let mut nodes: Vec<PlanNode> = Vec::new();
    for line in lines {
        let raw = line.trim_end();
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let body = raw.trim_start();
        let is_node = body.starts_with("->") || (nodes.is_empty() && !body.contains(':'));
        if is_node {
            let text = body.trim_start_matches("->").trim_start();
            let depth = if body.starts_with("->") {
                indent / 6 + 1
            } else {
                0
            };
            let head = text.split("  (").next().unwrap_or(text).trim();
            let (op, object) = split_op_object(head);
            let mut n = PlanNode {
                depth,
                op,
                object,
                ..Default::default()
            };
            if let Some(c) = kv(text, "cost") {
                n.cost = c.split("..").nth(1).and_then(num);
            }
            // 첫 rows= 는 추정 · "actual" 뒤의 rows= 는 실측.
            if let Some(est) = kv(text, "rows") {
                n.rows = num(&est);
            }
            if let Some(i) = text.find("(actual") {
                let act = &text[i..];
                n.actual_rows = kv(act, "rows").and_then(|v| num(&v));
                n.time_ms = kv(act, "time")
                    .and_then(|v| v.split("..").nth(1).map(str::to_string))
                    .and_then(|v| num(&v));
                if let Some(l) = kv(act, "loops").and_then(|v| num(&v)) {
                    if l > 1.0 {
                        n.actual_rows = n.actual_rows.map(|r| r * l);
                    }
                }
            }
            nodes.push(n);
        } else if let Some(last) = nodes.last_mut() {
            if !last.detail.is_empty() {
                last.detail.push_str(" · ");
            }
            last.detail.push_str(body);
        }
    }
    nodes
}

/// `Seq Scan on public.emp e` → (`Seq Scan`, `public.emp e`) · `Index Scan using idx on t` → (`Index Scan`, `idx on t`).
fn split_op_object(head: &str) -> (String, String) {
    for sep in [" using ", " on "] {
        if let Some(i) = head.find(sep) {
            let obj = head[i + sep.len()..].trim();
            return (head[..i].trim().to_string(), obj.to_string());
        }
    }
    (head.to_string(), String::new())
}

fn parse_pg(rows: &[Vec<String>]) -> Vec<PlanNode> {
    parse_arrow_tree(rows.iter().map(|r| first_cell(r).to_string()))
}

/// Oracle DBMS_XPLAN 표: 머리 행에서 열 위치를 읽고 `| id | Operation | Name | …` 행을 노드로 · "Predicate Information"의
/// `   1 - filter(…)` 줄은 해당 id 노드의 detail로.
fn parse_oracle(rows: &[Vec<String>]) -> Vec<PlanNode> {
    let lines: Vec<&str> = rows.iter().map(|r| first_cell(r)).collect();
    let mut nodes: Vec<PlanNode> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut col: Vec<String> = Vec::new();
    let mut in_pred = false;
    for line in lines {
        let t = line.trim();
        if t.starts_with("Predicate Information") {
            in_pred = true;
            continue;
        }
        if in_pred {
            // `   1 - filter("DEPTNO"=10)` / `   2 - access(…)`
            if let Some((id, rest)) = t.split_once(" - ") {
                if let Some(pos) = ids.iter().position(|x| x == id.trim()) {
                    let n = &mut nodes[pos];
                    if !n.detail.is_empty() {
                        n.detail.push_str(" · ");
                    }
                    n.detail.push_str(rest.trim());
                }
            }
            continue;
        }
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').collect();
        let names: Vec<String> = cells
            .iter()
            .map(|c| c.trim().to_ascii_lowercase())
            .collect();
        if names.iter().any(|n| n == "operation") {
            col = names;
            continue;
        }
        if col.is_empty() || cells.len() < 2 {
            continue;
        }
        let get = |name: &str| -> Option<&str> {
            col.iter()
                .position(|c| c == name)
                .and_then(|i| cells.get(i).copied())
        };
        let Some(op_raw) = get("operation") else {
            continue;
        };
        let id = get("id")
            .unwrap_or("")
            .trim()
            .trim_matches('*')
            .trim()
            .to_string();
        if id.is_empty() {
            continue;
        }
        let depth = (op_raw.len() - op_raw.trim_start().len()).saturating_sub(1);
        let rows_est = get("rows").or_else(|| get("e-rows")).and_then(num);
        let actual = get("a-rows").and_then(num);
        let cost = get("cost (%cpu)")
            .or_else(|| get("cost"))
            .map(|c| c.split('(').next().unwrap_or(c).trim().to_string())
            .and_then(|c| num(&c));
        let time_ms = get("a-time").and_then(|t| {
            // 00:00:00.01 → ms
            let parts: Vec<&str> = t.trim().split(':').collect();
            if parts.len() == 3 {
                let h: f64 = parts[0].parse().ok()?;
                let m: f64 = parts[1].parse().ok()?;
                let s: f64 = parts[2].parse().ok()?;
                Some((h * 3600.0 + m * 60.0 + s) * 1000.0)
            } else {
                None
            }
        });
        ids.push(id);
        nodes.push(PlanNode {
            depth,
            op: op_raw.trim().to_string(),
            object: get("name").unwrap_or("").trim().to_string(),
            rows: rows_est,
            actual_rows: actual,
            cost,
            time_ms,
            detail: String::new(),
        });
    }
    nodes
}

/// SQL Server SHOWPLAN_ALL / STATISTICS PROFILE 결과 집합: `StmtText`의 `|--` 들여쓰기 = 깊이 · `PhysicalOp` · `Argument`의 OBJECT · `EstimateRows` ·
/// `TotalSubtreeCost` · (PROFILE) `Rows`/`Executes` = 실측.
fn parse_mssql(columns: &[String], rows: &[Vec<String>]) -> Vec<PlanNode> {
    let idx = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (Some(stmt_i), phys_i) = (idx("StmtText"), idx("PhysicalOp")) else {
        return parse_generic(columns, rows);
    };
    let arg_i = idx("Argument");
    let est_i = idx("EstimateRows");
    let cost_i = idx("TotalSubtreeCost");
    let rows_i = idx("Rows");
    let exec_i = idx("Executes");
    let warn_i = idx("Warnings");
    let mut nodes = Vec::new();
    for r in rows {
        let text = r.get(stmt_i).map(String::as_str).unwrap_or("");
        let phys = phys_i
            .and_then(|i| r.get(i))
            .map(String::as_str)
            .unwrap_or("");
        let trimmed = text.trim_start();
        let is_node = trimmed.starts_with("|--") || !phys.trim().is_empty();
        if !is_node {
            // 첫 행 = 문장 자체(루트).
            if nodes.is_empty() && !trimmed.is_empty() {
                nodes.push(PlanNode {
                    depth: 0,
                    op: "Statement".into(),
                    object: String::new(),
                    detail: trimmed.chars().take(120).collect(),
                    cost: cost_i.and_then(|i| r.get(i)).and_then(|c| num(c)),
                    rows: est_i.and_then(|i| r.get(i)).and_then(|c| num(c)),
                    actual_rows: rows_i.and_then(|i| r.get(i)).and_then(|c| num(c)),
                    ..Default::default()
                });
            }
            continue;
        }
        let depth = (text.len() - trimmed.len()) / 7 + 1;
        let op = if phys.trim().is_empty() {
            trimmed
                .trim_start_matches("|--")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        } else {
            phys.trim().to_string()
        };
        let argument = arg_i
            .and_then(|i| r.get(i))
            .map(String::as_str)
            .unwrap_or("");
        let object = argument
            .find("OBJECT:(")
            .map(|i| {
                let rest = &argument[i + 8..];
                rest.split(')').next().unwrap_or("").to_string()
            })
            .unwrap_or_default();
        let mut detail = argument.chars().take(200).collect::<String>();
        if let Some(w) = warn_i
            .and_then(|i| r.get(i))
            .filter(|w| !w.trim().is_empty())
        {
            if !detail.is_empty() {
                detail.push_str(" · ");
            }
            detail.push_str(w.trim());
        }
        let executes = exec_i.and_then(|i| r.get(i)).and_then(|c| num(c));
        let actual = rows_i.and_then(|i| r.get(i)).and_then(|c| num(c));
        nodes.push(PlanNode {
            depth,
            op,
            object,
            rows: est_i
                .and_then(|i| r.get(i))
                .and_then(|c| num(c))
                .map(|v| v * executes.unwrap_or(1.0).max(1.0)),
            actual_rows: actual,
            cost: cost_i.and_then(|i| r.get(i)).and_then(|c| num(c)),
            time_ms: None,
            detail,
        });
    }
    nodes
}

/// MySQL/MariaDB: 표(`id · select_type · table · type · possible_keys · key · rows · filtered · Extra`) 또는 트리 텍스트(`EXPLAIN ANALYZE` · FORMAT=TREE).
fn parse_mysql(columns: &[String], rows: &[Vec<String>]) -> Vec<PlanNode> {
    let idx = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let Some(type_i) = idx("type") else {
        // 한 열짜리 텍스트 = 트리.
        return parse_arrow_tree(rows.iter().map(|r| first_cell(r).to_string()));
    };
    let table_i = idx("table");
    let key_i = idx("key");
    let pk_i = idx("possible_keys");
    let rows_i = idx("rows");
    let filt_i = idx("filtered");
    let extra_i = idx("Extra");
    let mut nodes = Vec::new();
    for r in rows {
        let cell = |i: Option<usize>| i.and_then(|i| r.get(i)).map(String::as_str).unwrap_or("");
        let access = cell(Some(type_i));
        let key = cell(key_i);
        let pk = cell(pk_i);
        let extra = cell(extra_i);
        let est = num(cell(rows_i));
        let mut detail = String::new();
        if !key.trim().is_empty() && key != "NULL" {
            detail.push_str(&format!("key={key}"));
        } else if !pk.trim().is_empty() && pk != "NULL" {
            detail.push_str(&format!("possible_keys={pk} key=NULL"));
        }
        if !extra.trim().is_empty() && extra != "NULL" {
            if !detail.is_empty() {
                detail.push_str(" · ");
            }
            detail.push_str(extra);
        }
        nodes.push(PlanNode {
            depth: 0,
            op: format!("type={}", if access.is_empty() { "?" } else { access }),
            object: cell(table_i).to_string(),
            rows: est,
            actual_rows: None,
            cost: est.map(|e| e * num(cell(filt_i)).unwrap_or(100.0) / 100.0),
            time_ms: None,
            detail,
        });
    }
    nodes
}

/// SQLite `EXPLAIN QUERY PLAN`: `id · parent · notused · detail` — 부모 사슬로 깊이 · detail 첫 낱말이 연산(SCAN/SEARCH …).
fn parse_sqlite(columns: &[String], rows: &[Vec<String>]) -> Vec<PlanNode> {
    let idx = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (Some(id_i), Some(parent_i), Some(detail_i)) = (idx("id"), idx("parent"), idx("detail"))
    else {
        return parse_generic(columns, rows);
    };
    let mut depth_of: Vec<(String, usize)> = Vec::new();
    let mut nodes = Vec::new();
    for r in rows {
        let id = r.get(id_i).cloned().unwrap_or_default();
        let parent = r.get(parent_i).cloned().unwrap_or_default();
        let detail = r.get(detail_i).cloned().unwrap_or_default();
        let depth = depth_of
            .iter()
            .find(|(i, _)| *i == parent)
            .map_or(0, |(_, d)| d + 1);
        depth_of.push((id, depth));
        let mut words = detail.split_whitespace();
        let first = words.next().unwrap_or("").to_string();
        // `USE TEMP B-TREE FOR ORDER BY` · `MULTI-INDEX OR` · `LIST SUBQUERY n` 같은 줄은 첫 낱말만으로는 뜻이 없다 → 줄 전체가 연산(10-23 협업 지적).
        let op = if first == "SCAN" || first == "SEARCH" {
            first.clone()
        } else {
            detail.trim().to_string()
        };
        let object = if first == "SCAN" || first == "SEARCH" {
            words
                .next()
                .filter(|w| !w.eq_ignore_ascii_case("TABLE"))
                .or_else(|| detail.split_whitespace().nth(2))
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        nodes.push(PlanNode {
            depth,
            op,
            object,
            detail,
            ..Default::default()
        });
    }
    nodes
}

/// DuckDB 상자 그림: `│   SEQ_SCAN   │` 같은 대문자 낱말 줄을 노드로(깊이는 상자 열 위치 · 같은 행에 여러 상자 = 형제).
fn parse_duckdb(rows: &[Vec<String>]) -> Vec<PlanNode> {
    let mut nodes: Vec<PlanNode> = Vec::new();
    let mut last_cols: Vec<usize> = Vec::new();
    for r in rows {
        let line = first_cell(r);
        if !line.contains('│') {
            continue;
        }
        let mut col = 0usize;
        let cells: Vec<&str> = line.split('│').collect();
        for (k, cell) in cells.iter().enumerate() {
            let t = cell.trim();
            if k > 0 && k + 1 < cells.len() && !t.is_empty() {
                let is_op = t.len() >= 3
                    && t.chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
                if is_op {
                    let depth = last_cols.iter().filter(|c| **c < col).count();
                    nodes.push(PlanNode {
                        depth,
                        op: t.to_string(),
                        ..Default::default()
                    });
                    last_cols.push(col);
                } else if let Some(n) = nodes.last_mut() {
                    // 상자 안 다음 줄(테이블 이름 · 추정 행 수 · 시간)
                    let rule = t.chars().all(|c| c == '\u{2500}');
                    if rule {
                        // 상자 안 구분선은 정보가 아니다.
                    } else if n.object.is_empty()
                        && !t.contains(' ')
                        && !t.contains(':')
                        && !t.starts_with('~')
                    {
                        n.object = t.to_string();
                    } else if let Some(v) = t.strip_prefix('~') {
                        n.rows = num(v.split_whitespace().next().unwrap_or(""));
                    } else if let Some(v) = t.strip_prefix("EC:") {
                        n.cost = num(v.trim());
                    } else {
                        if !n.detail.is_empty() {
                            n.detail.push_str(" · ");
                        }
                        n.detail.push_str(t);
                    }
                }
            }
            col += cell.chars().count() + 1;
        }
    }
    nodes
}

/// 모르는 꼴(ODBC 등): 행마다 노드 하나(첫 열 = 연산 · 나머지 = detail).
fn parse_generic(columns: &[String], rows: &[Vec<String>]) -> Vec<PlanNode> {
    rows.iter()
        .map(|r| PlanNode {
            depth: 0,
            op: first_cell(r).trim().to_string(),
            detail: r
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(i, v)| {
                    format!(
                        "{}={}",
                        columns.get(i).map(String::as_str).unwrap_or("?"),
                        v
                    )
                })
                .collect::<Vec<_>>()
                .join(" · "),
            ..Default::default()
        })
        .collect()
}

/// 검토 규칙(순수): 전체 스캔 · 카티전 조인 · 정렬/임시 · 비용 집중 · 추정치 불일치 · 인덱스 후보 미사용 · 컬럼 함수.
#[must_use]
pub fn review(dialect: Dialect, nodes: &[PlanNode], analyzed: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let max_cost = nodes.iter().filter_map(|n| n.cost).fold(0.0_f64, f64::max);
    for (i, n) in nodes.iter().enumerate() {
        let op = n.op.to_ascii_uppercase();
        let detail = n.detail.to_ascii_uppercase();
        let big = n.rows.unwrap_or(0.0) >= 10_000.0 || n.actual_rows.unwrap_or(0.0) >= 10_000.0;
        // 1) 전체 스캔
        let full = match dialect {
            Dialect::Oracle => {
                op.contains("TABLE ACCESS FULL")
                    || op.contains("INDEX FULL SCAN")
                    || op.contains("INDEX FAST FULL SCAN")
            }
            Dialect::Postgres => op.starts_with("SEQ SCAN"),
            Dialect::Mssql => op.contains("TABLE SCAN") || op.contains("INDEX SCAN"),
            Dialect::Mysql => {
                op == "TYPE=ALL" || op == "TYPE=INDEX" || op.starts_with("TABLE SCAN")
            }
            Dialect::Sqlite => op == "SCAN",
            Dialect::Duckdb => op == "SEQ_SCAN",
            // ClickHouse `ReadFromMergeTree`는 키 범위를 쓰는지 한 줄로는 모른다 → 판정 없음.
            Dialect::Clickhouse => false,
            Dialect::Odbc => op.contains("FULL") || op.contains("SEQ"),
        };
        if full {
            out.push(Finding {
                severity: if big { Severity::High } else { Severity::Warn },
                node: Some(i),
                message: tr!(
                    "Full scan of {} - consider an index on the filter/join columns (or confirm the table is small)",
                    "{} 전체 스캔 - 필터/조인 컬럼 인덱스를 검토하세요(작은 테이블이면 무시해도 됩니다)"
                , if n.object.is_empty() { &n.op } else { &n.object }),
            });
        }
        // 2) 카티전 조인
        if op.contains("CARTESIAN")
            || detail.contains("NO JOIN PREDICATE")
            || detail.contains("BLOCK NESTED LOOP")
            || (op.contains("NESTED LOOP") && n.rows.unwrap_or(0.0) >= 1_000_000.0)
        {
            out.push(Finding {
                severity: Severity::High,
                node: Some(i),
                message: tr!(
                    "Cartesian product / join without a predicate - check the join condition",
                    "카티전 곱 / 조인 조건 없는 조인 - 조인 조건을 확인하세요"
                ),
            });
        }
        // 3) 정렬 · 임시 테이블 · 디스크 사용
        if detail.contains("EXTERNAL")
            || detail.contains("DISK:")
            || detail.contains("USING FILESORT")
            || detail.contains("USING TEMPORARY")
            || (op.contains("SORT") && big)
        {
            out.push(Finding {
                severity: Severity::Warn,
                node: Some(i),
                message: tr!(
                    "Sort or temporary table on a large row set - an index matching ORDER BY/GROUP BY avoids it",
                    "큰 행 집합의 정렬/임시 테이블 - ORDER BY/GROUP BY에 맞는 인덱스가 있으면 피할 수 있습니다"
                ),
            });
        }
        // 4) 비용 집중: 최대 비용(보통 루트)의 절반 이상인 노드 가운데 **가장 깊은 것**만(루트·조상은 자식 비용을 포함해 늘 크다 ·
        //   Oracle 실측 10-23 "4 of max 4"가 루트·NESTED LOOPS 둘 다 찍히던 것) · 노드 3개 이상일 때.
        if nodes.len() >= 3 && max_cost > 0.0 && i != 0 && !op.starts_with("STATEMENT") {
            if let Some(c) = n.cost {
                let threshold = max_cost * 0.5;
                let child_heavy = nodes[i + 1..]
                    .iter()
                    .take_while(|m| m.depth > n.depth)
                    .any(|m| m.cost.is_some_and(|mc| mc >= threshold));
                if c >= threshold && !child_heavy {
                    out.push(Finding {
                        severity: Severity::Info,
                        node: Some(i),
                        message: tr!(
                            "Most of the cost is here ({} of max {})",
                            "비용이 여기에 집중됩니다({} / 최대 {})",
                            fmt_num(c),
                            fmt_num(max_cost)
                        ),
                    });
                }
            }
        }
        // 5) 추정 vs 실측(실제 계획)
        if analyzed {
            if let (Some(e), Some(a)) = (n.rows, n.actual_rows) {
                if a >= 1_000.0 && (a >= e * 10.0 || (e >= 1_000.0 && a * 10.0 <= e)) {
                    out.push(Finding {
                        severity: Severity::Warn,
                        node: Some(i),
                        message: tr!(
                            "Estimated {} rows but actually {} - statistics may be stale (gather statistics / ANALYZE)",
                            "추정 {}행인데 실측 {}행 - 통계가 오래됐을 수 있습니다(통계 수집 / ANALYZE)"
                        , &fmt_num(e), &fmt_num(a)),
                    });
                }
            }
        }
        // 6) 인덱스 후보가 있는데 안 씀(MySQL) · 필터로 버린 행이 많음(PG 실측)
        if detail.contains("POSSIBLE_KEYS=") && detail.contains("KEY=NULL") {
            out.push(Finding {
                severity: Severity::Warn,
                node: Some(i),
                message: tr!(
                    "An index exists (possible_keys) but was not used - check the predicate shape or index selectivity",
                    "인덱스 후보(possible_keys)가 있는데 쓰지 않았습니다 - 조건식 모양이나 선택도를 확인하세요"
                ),
            });
        }
        if let Some(removed) = kv(&n.detail, "Rows Removed by Filter").and_then(|v| num(&v)) {
            if removed >= 10_000.0 && removed >= n.actual_rows.unwrap_or(0.0) * 10.0 {
                out.push(Finding {
                    severity: Severity::Info,
                    node: Some(i),
                    message: tr!(
                        "The filter discards most rows ({}) - an index on the filter column would read less",
                        "필터가 대부분의 행({})을 버립니다 - 필터 컬럼 인덱스가 있으면 덜 읽습니다"
                    , &fmt_num(removed)),
                });
            }
        }
        // 7) 컬럼에 함수(Oracle/PG 필터 식)
        if full {
            for f in [
                "TO_CHAR(", "UPPER(", "LOWER(", "TRUNC(", "SUBSTR(", "NVL(", "CAST(",
            ] {
                if detail.contains(f) {
                    out.push(Finding {
                        severity: Severity::Info,
                        node: Some(i),
                        message: tr!(
                            "A function is applied to a column in the filter ({}) - a plain index cannot be used; consider a function-based index or rewriting the predicate",
                            "필터에서 컬럼에 함수가 적용됐습니다({}) - 일반 인덱스를 못 씁니다 · 함수 기반 인덱스나 조건식 재작성을 검토하세요"
                        , f.trim_end_matches('(')),
                    });
                    break;
                }
            }
        }
    }
    if nodes.is_empty() {
        out.push(Finding {
            severity: Severity::Info,
            node: None,
            message: tr!(
                "The plan could not be parsed into a tree - see the raw plan output",
                "계획을 트리로 풀지 못했습니다 - 원문 계획을 보세요"
            ),
        });
    } else if out.is_empty() {
        out.push(Finding {
            severity: Severity::Info,
            node: None,
            message: tr!(
                "No issues found by the automatic review",
                "자동 검토에서 짚을 것이 없습니다"
            ),
        });
    }
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.node.cmp(&b.node)));
    out
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        let s = (v as i64).to_string();
        // 천 단위 구분
        let mut out = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 && c != '-' {
                out.push(',');
            }
            out.push(c);
        }
        out.chars().rev().collect()
    } else {
        format!("{v:.2}")
    }
}

/// 보고서 조립(파싱 + 검토).
#[must_use]
pub fn build_report(
    dialect: Dialect,
    analyzed: bool,
    columns: &[String],
    rows: &[Vec<String>],
) -> PlanReport {
    let nodes = parse_plan(dialect, columns, rows);
    let findings = review(dialect, &nodes, analyzed);
    let raw = rows
        .iter()
        .map(|r| {
            if r.len() == 1 {
                r[0].clone()
            } else {
                r.join(" | ")
            }
        })
        .collect();
    PlanReport {
        dialect: Some(dialect),
        analyzed,
        nodes,
        raw,
        findings,
    }
}

/// CLI/로그용 텍스트: 트리(들여쓰기) + 검토 목록.
#[must_use]
pub fn render_text(rep: &PlanReport) -> String {
    let mut s = String::new();
    s.push_str(&tr!("Execution plan", "실행 계획"));
    if rep.analyzed {
        s.push_str(&tr!(" (actual)", " (실제)"));
    }
    s.push('\n');
    for (i, n) in rep.nodes.iter().enumerate() {
        let mut line = format!("{:>3}  {}{}", i + 1, "  ".repeat(n.depth), n.op);
        if !n.object.is_empty() {
            line.push_str(&format!(" [{}]", n.object));
        }
        let mut tail = Vec::new();
        if let Some(r) = n.rows {
            tail.push(format!("rows={}", fmt_num(r)));
        }
        if let Some(a) = n.actual_rows {
            tail.push(format!("actual={}", fmt_num(a)));
        }
        if let Some(c) = n.cost {
            tail.push(format!("cost={}", fmt_num(c)));
        }
        if let Some(t) = n.time_ms {
            tail.push(format!("time={t:.2}ms"));
        }
        if !tail.is_empty() {
            line.push_str(&format!("  ({})", tail.join(" ")));
        }
        if !n.detail.is_empty() {
            line.push_str(&format!("\n     {}", n.detail));
        }
        s.push_str(&line);
        s.push('\n');
    }
    s.push_str(&tr!("Review", "검토"));
    s.push('\n');
    for f in &rep.findings {
        match f.node {
            Some(i) => s.push_str(&format!(
                "  [{}] #{}: {}\n",
                f.severity.label(),
                i + 1,
                f.message
            )),
            None => s.push_str(&format!("  [{}] {}\n", f.severity.label(), f.message)),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows1(text: &str) -> Vec<Vec<String>> {
        text.lines().map(|l| vec![l.to_string()]).collect()
    }

    #[test]
    fn scripts_per_dialect_and_oracle_hint() {
        let o = PlanOptions { analyze: false };
        assert!(explain_script_opts(Dialect::Postgres, "select 1", o)
            .starts_with("EXPLAIN (VERBOSE, COSTS) select 1;"));
        assert!(
            explain_script_opts(Dialect::Postgres, "select 1", PlanOptions { analyze: true })
                .contains("ANALYZE, BUFFERS")
        );
        assert!(explain_script_opts(Dialect::Mssql, "select 1", o).contains("SHOWPLAN_ALL ON"));
        assert!(
            explain_script_opts(Dialect::Mssql, "select 1", PlanOptions { analyze: true })
                .contains("STATISTICS PROFILE ON")
        );
        let est = explain_script_opts(Dialect::Oracle, "SELECT * FROM emp WHERE job = 'CLERK'", o);
        assert!(est.starts_with("BEGIN EXECUTE IMMEDIATE 'EXPLAIN PLAN FOR SELECT * FROM emp WHERE job = ''CLERK'''; END;\n/\n"), "{est}");
        assert!(est.contains("DBMS_XPLAN.DISPLAY(NULL, NULL, 'TYPICAL')"));
        let ora = explain_script_opts(
            Dialect::Oracle,
            "SELECT * FROM emp",
            PlanOptions { analyze: true },
        );
        assert!(
            ora.starts_with("SELECT /*+ GATHER_PLAN_STATISTICS */ * FROM emp;"),
            "{ora}"
        );
        assert!(ora.contains("DISPLAY_CURSOR(NULL, NULL, 'ALLSTATS LAST')"));
        assert_eq!(
            with_oracle_hint("select /*+ FULL(e) */ * from emp e", "X"),
            "SELECT /*+ X FULL(e) */ * from emp e"
        );
        assert_eq!(
            with_oracle_hint("update t set a=1", "X"),
            "update t set a=1"
        );
        assert_eq!(with_oracle_hint("selectx", "X"), "selectx");
        assert_eq!(
            explain_script_opts(Dialect::Sqlite, "select 1;", o),
            "EXPLAIN QUERY PLAN select 1;\n"
        );
        assert_eq!(
            explain_script_opts(Dialect::Duckdb, "select 1", PlanOptions { analyze: true }),
            "EXPLAIN ANALYZE select 1;\n"
        );
        assert_eq!(
            explain_script_opts(Dialect::Mysql, "select 1", o),
            "EXPLAIN select 1;\n"
        );
    }

    #[test]
    fn pg_text_plan_parses_tree_and_actuals() {
        let text = "Hash Join  (cost=1.09..2.30 rows=4 width=40) (actual time=0.040..0.050 rows=4 loops=1)\n  Hash Cond: (e.deptno = d.deptno)\n  ->  Seq Scan on public.emp e  (cost=0.00..1.14 rows=14 width=24) (actual time=0.010..0.012 rows=14 loops=1)\n        Filter: (e.sal > 1000)\n        Rows Removed by Filter: 2\n  ->  Hash  (cost=1.04..1.04 rows=4 width=24) (actual time=0.010..0.010 rows=4 loops=1)\n        ->  Index Scan using dept_pk on public.dept d  (cost=0.00..1.04 rows=4 width=24) (actual time=0.005..0.006 rows=4 loops=1)\nPlanning Time: 0.1 ms\nExecution Time: 0.2 ms";
        let nodes = parse_plan(Dialect::Postgres, &[], &rows1(text));
        assert_eq!(nodes.len(), 4, "{nodes:#?}");
        assert_eq!((nodes[0].depth, nodes[0].op.as_str()), (0, "Hash Join"));
        assert_eq!(nodes[0].detail, "Hash Cond: (e.deptno = d.deptno)");
        assert_eq!(
            (
                nodes[1].depth,
                nodes[1].op.as_str(),
                nodes[1].object.as_str()
            ),
            (1, "Seq Scan", "public.emp e")
        );
        assert_eq!(
            (nodes[1].rows, nodes[1].actual_rows, nodes[1].cost),
            (Some(14.0), Some(14.0), Some(1.14))
        );
        assert!(
            nodes[1].detail.contains("Filter: (e.sal > 1000)")
                && nodes[1].detail.contains("Rows Removed by Filter: 2")
        );
        assert_eq!(
            (
                nodes[3].depth,
                nodes[3].op.as_str(),
                nodes[3].object.as_str()
            ),
            (2, "Index Scan", "dept_pk on public.dept d")
        );
        let f = review(Dialect::Postgres, &nodes, true);
        assert!(
            f.iter().any(|x| x.node == Some(1)
                && x.severity == Severity::Warn
                && x.message.contains("emp")),
            "{f:#?}"
        );
    }

    #[test]
    fn oracle_xplan_table_parses_depth_cost_and_predicates() {
        let text = "Plan hash value: 3956160932\n\n------------------------------------------------------------------------------------\n| Id  | Operation                   | Name   | Rows  | Bytes | Cost (%CPU)| Time     |\n------------------------------------------------------------------------------------\n|   0 | SELECT STATEMENT            |        |     3 |   261 |     4   (0)| 00:00:01 |\n|   1 |  NESTED LOOPS               |        |     3 |   261 |     4   (0)| 00:00:01 |\n|*  2 |   TABLE ACCESS FULL         | EMP    |     3 |   114 |     3   (0)| 00:00:01 |\n|   3 |   TABLE ACCESS BY INDEX ROWID| DEPT   |     1 |    49 |     1   (0)| 00:00:01 |\n|*  4 |    INDEX UNIQUE SCAN        | PK_DEPT|     1 |       |     0   (0)| 00:00:01 |\n------------------------------------------------------------------------------------\n\nPredicate Information (identified by operation id):\n---------------------------------------------------\n\n   2 - filter(UPPER(\"E\".\"ENAME\")='KING')\n   4 - access(\"E\".\"DEPTNO\"=\"D\".\"DEPTNO\")\n";
        let nodes = parse_plan(Dialect::Oracle, &[], &rows1(text));
        assert_eq!(nodes.len(), 5, "{nodes:#?}");
        assert_eq!(
            (
                nodes[2].depth,
                nodes[2].op.as_str(),
                nodes[2].object.as_str()
            ),
            (2, "TABLE ACCESS FULL", "EMP")
        );
        assert_eq!((nodes[2].rows, nodes[2].cost), (Some(3.0), Some(3.0)));
        assert!(nodes[2].detail.contains("filter(UPPER("));
        assert_eq!(nodes[4].depth, 3);
        let f = review(Dialect::Oracle, &nodes, false);
        assert!(f
            .iter()
            .any(|x| x.node == Some(2) && x.message.contains("EMP")));
        assert!(
            f.iter()
                .any(|x| x.node == Some(2) && x.message.contains("UPPER")),
            "{f:#?}"
        );
        let cost_hits: Vec<_> = f
            .iter()
            .filter(|x| x.message.contains("max") || x.message.contains("최대"))
            .collect();
        assert_eq!(
            cost_hits.len(),
            1,
            "비용 집중은 가장 깊은 노드 하나: {cost_hits:#?}"
        );
        assert_eq!(cost_hits[0].node, Some(2));
    }

    #[test]
    fn oracle_allstats_uses_actual_rows() {
        let text = "| Id  | Operation         | Name | Starts | E-Rows | A-Rows |   A-Time   |\n|   0 | SELECT STATEMENT  |      |      1 |        |  20000 |00:00:00.05 |\n|*  1 |  TABLE ACCESS FULL| BIG  |      1 |     10 |  20000 |00:00:00.05 |";
        let nodes = parse_plan(Dialect::Oracle, &[], &rows1(text));
        assert_eq!(nodes.len(), 2);
        assert_eq!(
            (nodes[1].rows, nodes[1].actual_rows),
            (Some(10.0), Some(20000.0))
        );
        assert_eq!(nodes[1].time_ms, Some(50.0));
        let f = review(Dialect::Oracle, &nodes, true);
        assert!(f.iter().any(|x| x.message.contains("20,000")), "{f:#?}");
        assert!(
            f.iter().any(|x| x.severity == Severity::High),
            "큰 전체 스캔 = High"
        );
    }

    #[test]
    fn mssql_showplan_all_rows_parse_depth_object_cost() {
        let cols: Vec<String> = [
            "StmtText",
            "StmtId",
            "NodeId",
            "Parent",
            "PhysicalOp",
            "LogicalOp",
            "Argument",
            "DefinedValues",
            "EstimateRows",
            "EstimateIO",
            "EstimateCPU",
            "AvgRowSize",
            "TotalSubtreeCost",
            "OutputList",
            "Warnings",
            "Type",
            "Parallel",
            "EstimateExecutions",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let r =
            |stmt: &str, phys: &str, arg: &str, est: &str, cost: &str, warn: &str| -> Vec<String> {
                let mut v = vec![String::new(); cols.len()];
                v[0] = stmt.into();
                v[4] = phys.into();
                v[6] = arg.into();
                v[8] = est.into();
                v[12] = cost.into();
                v[14] = warn.into();
                v
            };
        let rows = vec![
            r(
                "SELECT * FROM dbo.t1 a, dbo.t2 b",
                "",
                "",
                "100",
                "0.03",
                "",
            ),
            r(
                "  |--Nested Loops(Inner Join)",
                "Nested Loops",
                "",
                "100",
                "0.03",
                "NO JOIN PREDICATE",
            ),
            r(
                "       |--Clustered Index Scan(OBJECT:([db].[dbo].[t1].[PK_t1] AS [a]))",
                "Clustered Index Scan",
                "OBJECT:([db].[dbo].[t1].[PK_t1] AS [a])",
                "10",
                "0.01",
                "",
            ),
            r(
                "       |--Table Scan(OBJECT:([db].[dbo].[t2] AS [b]))",
                "Table Scan",
                "OBJECT:([db].[dbo].[t2] AS [b])",
                "10",
                "0.01",
                "",
            ),
        ];
        let nodes = parse_plan(Dialect::Mssql, &cols, &rows);
        assert_eq!(nodes.len(), 4, "{nodes:#?}");
        assert_eq!(nodes[0].op, "Statement");
        assert_eq!((nodes[1].depth, nodes[1].op.as_str()), (1, "Nested Loops"));
        assert_eq!(
            (nodes[2].depth, nodes[2].object.as_str()),
            (2, "[db].[dbo].[t1].[PK_t1] AS [a]")
        );
        assert_eq!(nodes[3].cost, Some(0.01));
        let f = review(Dialect::Mssql, &nodes, false);
        assert!(
            f.iter()
                .any(|x| x.node == Some(1) && x.severity == Severity::High),
            "{f:#?}"
        );
        assert_eq!(
            f.iter()
                .filter(|x| x.message.contains("scan") || x.message.contains("스캔"))
                .count(),
            2
        );
    }

    #[test]
    fn mysql_table_and_sqlite_duckdb_shapes() {
        let cols: Vec<String> = [
            "id",
            "select_type",
            "table",
            "partitions",
            "type",
            "possible_keys",
            "key",
            "key_len",
            "ref",
            "rows",
            "filtered",
            "Extra",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let rows = vec![[
            "1",
            "SIMPLE",
            "emp",
            "NULL",
            "ALL",
            "idx_dept",
            "NULL",
            "NULL",
            "NULL",
            "20000",
            "10.00",
            "Using where; Using filesort",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()];
        let nodes = parse_plan(Dialect::Mysql, &cols, &rows);
        assert_eq!(
            (
                nodes[0].op.as_str(),
                nodes[0].object.as_str(),
                nodes[0].rows
            ),
            ("type=ALL", "emp", Some(20000.0))
        );
        let f = review(Dialect::Mysql, &nodes, false);
        assert!(
            f.iter().any(|x| x.severity == Severity::High),
            "ALL 20000행 = High: {f:#?}"
        );
        assert!(f.iter().any(|x| x.message.contains("possible_keys")));
        assert!(
            f.iter()
                .any(|x| x.message.to_ascii_lowercase().contains("sort")
                    || x.message.contains("정렬"))
        );
        // SQLite
        let scols: Vec<String> = ["id", "parent", "notused", "detail"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let srows = vec![
            vec!["3".into(), "0".into(), "0".into(), "SCAN emp".into()],
            vec![
                "8".into(),
                "3".into(),
                "0".into(),
                "SEARCH dept USING INTEGER PRIMARY KEY (rowid=?)".into(),
            ],
        ];
        let s = parse_plan(Dialect::Sqlite, &scols, &srows);
        assert_eq!(
            (s[0].depth, s[0].op.as_str(), s[0].object.as_str()),
            (0, "SCAN", "emp")
        );
        assert_eq!(
            (s[1].depth, s[1].op.as_str(), s[1].object.as_str()),
            (1, "SEARCH", "dept")
        );
        assert!(review(Dialect::Sqlite, &s, false)
            .iter()
            .any(|x| x.node == Some(0)));
        let srows2 = vec![vec![
            "9".into(),
            "0".into(),
            "0".into(),
            "USE TEMP B-TREE FOR ORDER BY".into(),
        ]];
        let s2 = parse_plan(Dialect::Sqlite, &scols, &srows2);
        assert_eq!(
            (s2[0].op.as_str(), s2[0].object.as_str()),
            ("USE TEMP B-TREE FOR ORDER BY", "")
        );
        // DuckDB
        let d = "┌───────────────────────────┐\n│         PROJECTION        │\n│    ────────────────────   │\n│             a             │\n└─────────────┬─────────────┘\n┌─────────────┴─────────────┐\n│         SEQ_SCAN          │\n│    ────────────────────   │\n│             t             │\n│          ~5 Rows          │\n└───────────────────────────┘";
        let dn = parse_plan(Dialect::Duckdb, &[], &rows1(d));
        assert_eq!(
            dn.iter().map(|n| n.op.as_str()).collect::<Vec<_>>(),
            ["PROJECTION", "SEQ_SCAN"]
        );
        assert_eq!((dn[1].object.as_str(), dn[1].rows), ("t", Some(5.0)));
    }

    #[test]
    fn report_and_text_render() {
        let rep = build_report(
            Dialect::Postgres,
            false,
            &[],
            &rows1("Seq Scan on t  (cost=0.00..1.00 rows=5 width=4)"),
        );
        assert_eq!(rep.nodes.len(), 1);
        assert_eq!(rep.raw.len(), 1);
        let text = render_text(&rep);
        assert!(
            text.contains("Seq Scan [t]") && text.contains("rows=5") && text.contains("[WARN] #1"),
            "{text}"
        );
        let empty = build_report(Dialect::Odbc, false, &[], &[]);
        assert_eq!(empty.findings.len(), 1);
        assert_eq!(empty.findings[0].node, None);
        assert_eq!(fmt_num(1234567.0), "1,234,567");
        assert_eq!(fmt_num(1.5), "1.50");
    }
}
