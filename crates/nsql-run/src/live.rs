//! ★ Oracle 라이브 모니터(T-71 · [docs/32 §2](../../../docs/32-server-messages-and-live-log.md)) — **GUI(탐색기 메타 세션)와 CLI(`nsql run --live`)가
//! 같은 폴링 문장·줄 형식·중복 제거를 쓴다**(11-04 · 종전 GUI 전용 `explorer::live_query`를 부품으로 올림 · [30 §2](../../../docs/30-architecture-patterns.md)).
//!
//! 소스(설정 `oracle.live.source` · CLI `--live`):
//! - `session` = `V$SESSION` client_info/action/module(프로시저가 `DBMS_APPLICATION_INFO`로 쓴 최신 값 하나) · 권한 `SELECT ON V_$SESSION`.
//! - `table` = 자율 트랜잭션 로그 표(`oracle.live.table/ts_col/text_col`) · 마지막 시각 이후 행만(`since` 워터마크 = 서버 시각).
//! - `pipe` = `DBMS_PIPE.RECEIVE_MESSAGE('<파이프>', 0)` 비차단 수신(프로시저가 `PACK_MESSAGE`+`SEND_MESSAGE('nsql_<sid>')`) · 권한 `EXECUTE ON DBMS_PIPE` ·
//!   한 틱에 메시지 ≤ 100 · OUT 버퍼 ≤ 3,800자(드라이버 OUT 스칼라 = VARCHAR2(4000)) · 남은 것은 다음 틱.
//! - `longops`(어느 소스와도 **병행** · `oracle.live.longops`) = `V$SESSION_LONGOPS` 진행률(`sofar/totalwork units · ~남은 초`) · 권한 `SELECT ON V_$SESSION_LONGOPS`.
//!
//! 부하 원칙(26 §8): 실행 중에만 · 간격 ≥ 250 ms(기본 1 s) · 프로필당 요청 1(진행 중이면 건너뜀) · 접속한 적 없는 서버에 트래픽 0 · 오류는 같은 글이면 한 번만.

use nsql_core::{BindParam, Direction, ExecRequest, ResultSet, Session, Value, VarType};

/// Oracle 세션의 SID(접속 직후 1회 · 워커 `session_id_sql`과 같은 문장 · CLI도 같은 문장).
pub const ORACLE_SID_SQL: &str = "SELECT SYS_CONTEXT('USERENV','SID') FROM dual";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveSource {
    Session,
    Table,
    Pipe,
}

impl LiveSource {
    /// 설정 값·CLI 낱말 → 소스(`off`·모르는 값 = None).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "session" => Some(Self::Session),
            "table" => Some(Self::Table),
            "pipe" => Some(Self::Pipe),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Table => "table",
            Self::Pipe => "pipe",
        }
    }
}

/// 설정에서 읽은 라이브 구성(소스 None = 끔). GUI는 `Settings`, CLI는 설정 파일 + `--live` 덮어쓰기로 만든다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveConfig {
    pub source: Option<LiveSource>,
    pub table: String,
    pub ts_col: String,
    pub text_col: String,
    /// 파이프 이름(비면 `nsql_<sid>`).
    pub pipe: String,
    pub longops: bool,
    pub interval_ms: u64,
}

impl LiveConfig {
    /// 설정 조회 함수 하나로 구성(키 = `oracle.live.*` · 없는 키 = 기본값).
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let s = |k: &str, d: &str| get(k).unwrap_or_else(|| d.to_string()).trim().to_string();
        let interval_ms = s("oracle.live.interval_ms", "1000")
            .parse::<u64>()
            .unwrap_or(1000)
            .clamp(250, 60_000);
        Self {
            source: LiveSource::parse(&s("oracle.live.source", "off")),
            table: s("oracle.live.table", ""),
            ts_col: s("oracle.live.ts_col", "LOG_TIME"),
            text_col: s("oracle.live.text_col", "LOG_TEXT"),
            pipe: s("oracle.live.pipe", ""),
            longops: matches!(
                s("oracle.live.longops", "off")
                    .to_ascii_lowercase()
                    .as_str(),
                "on" | "true" | "1" | "yes"
            ),
            interval_ms,
        }
    }

    /// CLI `--live[=<스펙>]` 덮어쓰기 — 스펙 = 쉼표 목록 `session` · `table:<표>[:<시각 열>[:<본문 열>]]` · `pipe[:<이름>]` · `longops` ·
    /// `off`. 빈 글 = 설정 그대로(소스가 끔이면 `session`으로 켠다 = 플래그를 준 뜻).
    pub fn apply_spec(mut self, spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.is_empty() {
            if self.source.is_none() {
                self.source = Some(LiveSource::Session);
            }
            return Ok(self);
        }
        let mut source_given = false;
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let mut parts = item.splitn(4, ':');
            let head = parts.next().unwrap_or("").to_ascii_lowercase();
            match head.as_str() {
                "off" => {
                    self.source = None;
                    self.longops = false;
                    source_given = true;
                }
                "longops" => self.longops = true,
                "session" => {
                    self.source = Some(LiveSource::Session);
                    source_given = true;
                }
                "pipe" => {
                    self.source = Some(LiveSource::Pipe);
                    source_given = true;
                    if let Some(n) = parts.next() {
                        self.pipe = n.trim().to_string();
                    }
                }
                "table" => {
                    self.source = Some(LiveSource::Table);
                    source_given = true;
                    let Some(t) = parts.next().map(str::trim).filter(|t| !t.is_empty()) else {
                        return Err("--live table:<table>[:<ts_col>[:<text_col>]]".into());
                    };
                    self.table = t.to_string();
                    if let Some(c) = parts.next() {
                        self.ts_col = c.trim().to_string();
                    }
                    if let Some(c) = parts.next() {
                        self.text_col = c.trim().to_string();
                    }
                }
                other => return Err(format!("--live: unknown source '{other}'")),
            }
        }
        // `--live longops`만 준 경우 = 진행률만(소스가 끔이면 끔 그대로 두되 longops는 켠다 → 요청은 만들어진다).
        if !source_given && self.source.is_none() && !self.longops {
            self.source = Some(LiveSource::Session);
        }
        Ok(self)
    }

    /// 폴링할 것이 있는가(소스 또는 longops).
    #[must_use]
    pub fn active(&self) -> bool {
        self.source.is_some() || self.longops
    }
}

/// 한 틱의 요청(구성 + SID + 워터마크).
#[derive(Clone, Debug)]
pub struct LiveReq {
    pub sid: String,
    pub cfg: LiveConfig,
    /// 로그 표 소스의 마지막 시각(`YYYY-MM-DD HH24:MI:SS.FF3`) — None이면 서버 현재 시각만 받아 기준점으로.
    pub since: Option<String>,
}

/// (줄들, 새 워터마크) 또는 오류 글.
pub type LiveResult = Result<(Vec<String>, Option<String>), String>;

/// 식별자 검사(표·컬럼·파이프 이름 설정값 — 문장에 그대로 들어간다) — 영숫자·`_`·`$`·`#`·`.`·`"`만.
fn ident_ok(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#' | '.' | '"'))
}

fn sid_num(sid: &str) -> Result<i64, String> {
    sid.trim().parse().map_err(|_| "live: SID".to_string())
}

/// 파이프 이름(비면 `nsql_<sid>`).
#[must_use]
pub fn pipe_name(cfg: &LiveConfig, sid: &str) -> String {
    if cfg.pipe.trim().is_empty() {
        format!("nsql_{}", sid.trim())
    } else {
        cfg.pipe.trim().to_string()
    }
}

#[must_use]
pub fn session_sql(sid: i64) -> String {
    format!(
        "SELECT NVL(client_info, ''), NVL(action, ''), NVL(module, ''), status FROM v$session WHERE sid = {sid}"
    )
}

/// 진행 중인 긴 작업(최근 5개 · `sofar < totalwork`).
#[must_use]
pub fn longops_sql(sid: i64) -> String {
    format!(
        "SELECT * FROM (SELECT NVL(opname, ''), NVL(target, ''), sofar, totalwork, NVL(units, ''), time_remaining, NVL(message, '') \
         FROM v$session_longops WHERE sid = {sid} AND totalwork > 0 AND sofar < totalwork ORDER BY start_time DESC) WHERE ROWNUM <= 5"
    )
}

/// `DBMS_PIPE` 비차단 수신 블록 — 한 틱에 메시지 ≤ 100 · 버퍼 ≤ 3,800자(OUT 스칼라 VARCHAR2(4000)) · 항목 형 = VARCHAR2(9)·NUMBER(6)·DATE(12) ·
/// 그 밖(ROWID 11 · RAW 23)은 그 메시지의 나머지를 버린다. 파이프 이름은 [`ident_ok`]를 지나 그대로 들어간다.
#[must_use]
pub fn pipe_block(pipe: &str) -> String {
    format!(
        "DECLARE\n  s PLS_INTEGER; t PLS_INTEGER; v VARCHAR2(4000); n NUMBER; d DATE; k PLS_INTEGER := 0; buf VARCHAR2(4000) := '';\n\
         BEGIN\n  LOOP\n    s := DBMS_PIPE.RECEIVE_MESSAGE('{pipe}', 0);\n    EXIT WHEN s <> 0;\n    LOOP\n      t := DBMS_PIPE.NEXT_ITEM_TYPE;\n      EXIT WHEN t = 0;\n\
         \x20     IF t = 9 THEN DBMS_PIPE.UNPACK_MESSAGE(v);\n      ELSIF t = 6 THEN DBMS_PIPE.UNPACK_MESSAGE(n); v := TO_CHAR(n);\n\
         \x20     ELSIF t = 12 THEN DBMS_PIPE.UNPACK_MESSAGE(d); v := TO_CHAR(d, 'YYYY-MM-DD HH24:MI:SS');\n      ELSE EXIT;\n      END IF;\n\
         \x20     IF LENGTH(buf) + NVL(LENGTH(v), 0) > 3800 THEN EXIT; END IF;\n      buf := buf || v || CHR(10);\n    END LOOP;\n    k := k + 1;\n\
         \x20   EXIT WHEN k >= 100 OR LENGTH(buf) > 3700;\n  END LOOP;\n  :OUT := buf;\nEND;"
    )
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        o => o.display(),
    }
}

/// V$SESSION 한 행 → `client_info · action · module`(빈 조각 생략 · 전부 비면 None).
#[must_use]
pub fn session_line(row: &[Value]) -> Option<String> {
    let parts: Vec<String> = row
        .iter()
        .take(3)
        .map(cell)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// V$SESSION_LONGOPS 한 행 → `⏳ message — sofar/totalwork units · ~Ns`(message 없으면 opname: target).
#[must_use]
pub fn longops_line(row: &[Value]) -> String {
    let g = |i: usize| row.get(i).map(cell).unwrap_or_default();
    let (opname, target, sofar, total, units, remain, message) =
        (g(0), g(1), g(2), g(3), g(4), g(5), g(6));
    let head = if message.is_empty() {
        if target.is_empty() {
            opname
        } else {
            format!("{opname}: {target}")
        }
    } else {
        message
    };
    let mut s = format!("⏳ {head} — {sofar}/{total}");
    if !units.is_empty() {
        s.push(' ');
        s.push_str(&units);
    }
    if !remain.is_empty() && remain != "0" {
        s.push_str(&format!(" · ~{remain}s"));
    }
    s
}

/// 파이프 OUT 버퍼(`CHR(10)` 구분) → 줄들(빈 줄 제외).
#[must_use]
pub fn pipe_lines(buf: &str) -> Vec<String> {
    buf.split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

fn run(s: &mut dyn Session, sql: &str) -> Result<ResultSet, String> {
    s.execute(&ExecRequest {
        sql: sql.to_string(),
        params: vec![],
    })
    .map(|r| r.result_sets.into_iter().next().unwrap_or_default())
    .map_err(|e| e.message)
}

/// 한 틱 폴링 — 소스 줄 + (켜져 있으면) 진행률 줄. 로그 표는 워터마크를 돌려준다(첫 틱 = 서버 현재 시각만).
pub fn live_query(s: &mut dyn Session, req: &LiveReq) -> LiveResult {
    let sid = sid_num(&req.sid)?;
    let cfg = &req.cfg;
    let mut lines: Vec<String> = Vec::new();
    let mut watermark: Option<String> = None;
    match cfg.source {
        None => {}
        Some(LiveSource::Session) => {
            let rs = run(s, &session_sql(sid))?;
            lines.extend(rs.rows.iter().filter_map(|r| session_line(r)));
        }
        Some(LiveSource::Table) => {
            if !ident_ok(&cfg.table) || !ident_ok(&cfg.ts_col) || !ident_ok(&cfg.text_col) {
                return Err("live: table/column name".into());
            }
            let fmt = "YYYY-MM-DD HH24:MI:SS.FF3";
            match &req.since {
                None => {
                    let rs = run(
                        s,
                        &format!("SELECT TO_CHAR(SYSTIMESTAMP, '{fmt}') FROM dual"),
                    )?;
                    watermark = rs.rows.first().and_then(|r| r.first()).map(cell);
                }
                Some(since) => {
                    let sql = format!(
                        "SELECT TO_CHAR({ts}, '{fmt}'), {tx} FROM {tb} WHERE {ts} > TO_TIMESTAMP('{since}', '{fmt}') ORDER BY {ts}",
                        ts = cfg.ts_col,
                        tx = cfg.text_col,
                        tb = cfg.table,
                        since = since.replace('\'', "")
                    );
                    let rs = run(s, &sql)?;
                    for r in &rs.rows {
                        let t = r.first().map(cell).unwrap_or_default();
                        let x = r.get(1).map(cell).unwrap_or_default();
                        watermark = Some(t.clone());
                        lines.push(format!("{t}  {x}"));
                    }
                }
            }
        }
        Some(LiveSource::Pipe) => {
            let pipe = pipe_name(cfg, &req.sid);
            if !ident_ok(&pipe) || pipe.contains('"') || pipe.contains('.') {
                return Err("live: pipe name".into());
            }
            let r = s
                .execute(&ExecRequest {
                    sql: pipe_block(&pipe),
                    params: vec![BindParam {
                        name: "OUT".into(),
                        value: Value::Null,
                        ty: VarType::Varchar2(4000),
                        direction: Direction::Out,
                    }],
                })
                .map_err(|e| e.message)?;
            let buf = r
                .out_params
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("OUT"))
                .map(|(_, v)| cell(v))
                .unwrap_or_default();
            lines.extend(pipe_lines(&buf));
        }
    }
    if cfg.longops {
        let rs = run(s, &longops_sql(sid))?;
        lines.extend(rs.rows.iter().map(|r| longops_line(r)));
    }
    Ok((lines, watermark))
}

/// 한 줄의 결과(호스트가 로그 창·stderr에 쓴다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveLine {
    Text(String),
    Error(String),
}

/// 틱 사이 상태 — 워터마크 · 직전 틱과 같은 줄 제거(세션·진행률은 매 틱 같은 값이 온다) · 같은 오류는 한 번만.
#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub since: Option<String>,
    prev: Vec<String>,
    last_err: String,
}

impl LiveState {
    /// 실행 시작 — 워터마크·직전 줄·오류 기억을 비운다.
    pub fn reset(&mut self) {
        self.since = None;
        self.prev.clear();
        self.last_err.clear();
    }

    /// 틱 결과를 흡수해 새로 보일 줄만 돌려준다.
    pub fn absorb(&mut self, r: LiveResult) -> Vec<LiveLine> {
        match r {
            Ok((lines, wm)) => {
                if wm.is_some() {
                    self.since = wm;
                }
                self.last_err.clear();
                let out: Vec<LiveLine> = lines
                    .iter()
                    .filter(|l| !self.prev.contains(l))
                    .map(|l| LiveLine::Text(l.clone()))
                    .collect();
                self.prev = lines;
                out
            }
            Err(e) => {
                if e == self.last_err {
                    Vec::new()
                } else {
                    self.last_err = e.clone();
                    vec![LiveLine::Error(e)]
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LiveConfig {
        LiveConfig::from_lookup(&|_k| None)
    }

    /// 설정 기본값 = 끔 · 1000 ms · 로그 표 열 이름 기본 · 조회 함수로 덮임.
    #[test]
    fn config_from_lookup_defaults_and_overrides() {
        let c = cfg();
        assert_eq!(c.source, None);
        assert!(!c.longops && c.interval_ms == 1000 && c.ts_col == "LOG_TIME");
        let c = LiveConfig::from_lookup(&|k| match k {
            "oracle.live.source" => Some("table".into()),
            "oracle.live.table" => Some("APP_LOG".into()),
            "oracle.live.longops" => Some("on".into()),
            "oracle.live.interval_ms" => Some("100".into()),
            _ => None,
        });
        assert_eq!(c.source, Some(LiveSource::Table));
        assert!(
            c.longops && c.table == "APP_LOG" && c.interval_ms == 250,
            "하한 250"
        );
    }

    /// CLI `--live` 스펙: 빈 글 = 켬(session) · table:표:시각:본문 · pipe:이름 · longops 병행 · off · 모르는 낱말 = 오류.
    #[test]
    fn apply_spec_grammar() {
        let c = cfg().apply_spec("").unwrap();
        assert_eq!(c.source, Some(LiveSource::Session));
        let c = cfg().apply_spec("table:LOG_T:TS:TXT,longops").unwrap();
        assert_eq!(c.source, Some(LiveSource::Table));
        assert_eq!(
            (c.table.as_str(), c.ts_col.as_str(), c.text_col.as_str()),
            ("LOG_T", "TS", "TXT")
        );
        assert!(c.longops);
        let c = cfg().apply_spec("pipe:MY_PIPE").unwrap();
        assert_eq!(
            (c.source, c.pipe.as_str()),
            (Some(LiveSource::Pipe), "MY_PIPE")
        );
        let c = cfg().apply_spec("longops").unwrap();
        assert_eq!(c.source, None, "진행률만");
        assert!(c.longops && c.active());
        assert!(!cfg().apply_spec("off").unwrap().active());
        assert!(cfg().apply_spec("table").is_err());
        assert!(cfg().apply_spec("foo").is_err());
    }

    /// 문장 생성: SID는 숫자만 · 파이프 블록은 이름 그대로 + OUT 바인드 자리 · 진행률은 ROWNUM ≤ 5.
    #[test]
    fn sql_builders() {
        assert!(session_sql(42).contains("WHERE sid = 42"));
        assert!(
            longops_sql(7).contains("sofar < totalwork") && longops_sql(7).contains("ROWNUM <= 5")
        );
        let b = pipe_block("nsql_42");
        assert!(b.contains("RECEIVE_MESSAGE('nsql_42', 0)") && b.contains(":OUT := buf"));
        assert!(
            ident_ok("APP.LOG_T")
                && ident_ok("\"MixedCase\"")
                && !ident_ok("x; DROP")
                && !ident_ok("")
        );
        assert_eq!(pipe_name(&cfg(), " 42 "), "nsql_42");
        assert!(sid_num("abc").is_err());
    }

    /// 줄 형식: 세션 = 빈 조각 생략 · 진행률 = message 우선 · 파이프 버퍼 = 빈 줄 제외.
    #[test]
    fn line_formats() {
        let s = |t: &str| Value::Str(t.to_string());
        assert_eq!(
            session_line(&[s("step 3/10"), Value::Null, s("SQL*Plus"), s("ACTIVE")]).as_deref(),
            Some("step 3/10 · SQL*Plus")
        );
        assert_eq!(
            session_line(&[Value::Null, Value::Null, Value::Null, s("ACTIVE")]),
            None
        );
        assert_eq!(
            longops_line(&[
                s("Table Scan"),
                s("APP.T"),
                s("120"),
                s("1000"),
                s("Blocks"),
                s("8"),
                s("")
            ]),
            "⏳ Table Scan: APP.T — 120/1000 Blocks · ~8s"
        );
        assert_eq!(
            longops_line(&[
                s("op"),
                s(""),
                s("1"),
                s("2"),
                s(""),
                s("0"),
                s("loading part 1")
            ]),
            "⏳ loading part 1 — 1/2"
        );
        assert_eq!(
            pipe_lines("a\n\nb\r\n"),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    /// 상태: 같은 줄은 다음 틱에 다시 안 나옴 · 바뀌면 나옴 · 워터마크 보존 · 같은 오류 한 번 · 성공 뒤 같은 오류 다시 한 번.
    #[test]
    fn state_dedupes_lines_and_errors() {
        let mut st = LiveState::default();
        let a = st.absorb(Ok((vec!["x".into(), "⏳ p — 1/2".into()], None)));
        assert_eq!(a.len(), 2);
        let b = st.absorb(Ok((
            vec!["x".into(), "⏳ p — 2/2".into()],
            Some("t1".into()),
        )));
        assert_eq!(b, vec![LiveLine::Text("⏳ p — 2/2".into())]);
        assert_eq!(st.since.as_deref(), Some("t1"));
        assert_eq!(st.absorb(Err("e".into())).len(), 1);
        assert!(st.absorb(Err("e".into())).is_empty(), "같은 오류는 한 번");
        st.absorb(Ok((vec![], None)));
        assert_eq!(
            st.absorb(Err("e".into())).len(),
            1,
            "성공 뒤 재발은 다시 알림"
        );
        st.reset();
        assert!(st.since.is_none());
    }
}
