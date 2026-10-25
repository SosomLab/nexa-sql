//! [`nsql_core::Session`] 구현 — 문장 실행 흐름(pydrda `Connection._execute/_query/_parse_response`와 같은 순서).
//!
//! - 즉시 실행(DML/DDL · 바인드 없음) = `EXCSQLIMM(PKGNAMCSN · RDBCMTOK)` + `SQLSTT` → `SQLCARD`(영향 행 수 = SQLERRD[2]).
//! - 조회(바인드 없음) = `PRPSQLSTT(RTNSQLDA)` + `SQLSTT` + `OPNQRY(QRYBLKSZ · MAXBLKEXT · QRYCLSIMP)` 한 체인 → `SQLDARD`(열 이름·타입) · `OPNQRYRM`(QRYINSID) ·
//!   `QRYDSC` · `QRYDTA`… · 블록이 끝나면 `CNTQRY`로 다음 블록 · `ENDQRYRM`이면 끝. 상한(`max_rows`)에 닿으면 `CLSQRY`.
//! - 바인드 = `PRPSQLSTT` + `SQLSTT` + `DSCSQLSTT(TYPSQLDA=1)` → SQLDARD 둘(0x00 = 결과 열 · 0xFF = 입력 매개변수) → `EXCSQLSTT`/`OPNQRY(DYNDTAFMT)` + `SQLDTA`.
//! - 자동 커밋 = 수동 트랜잭션(`BEGIN TRANSACTION` · Caps `tx_begin`)이 아닐 때 문장마다 뒤에 `RDBCMM` 체인(ODBC 어댑터와 같은 모델) ·
//!   스크립트의 `COMMIT`/`ROLLBACK` 문 = `RDBCMM`/`RDBRLLBCK`.
//! - 상관 번호 = 요청 체인마다 1부터(pydrda) · 명령 뒤에 딸린 객체(SQLSTT · SQLDTA)는 같은 번호(포맷 0x10).
//! - 취소 = 1차 없음(DRDA에 중단 없음 → 2차 = 별도 접속 `FORCE APPLICATION`/`WLM_CANCEL_ACTIVITY`) · LOB/CALL 결과 집합 = 2차.

use crate::codepoint as cp;
use crate::conn::{self, Login, ServerInfo, Wire};
use crate::dss::{Ddm, Dss, DssKind};
use crate::fdoca::{self, ColDesc, SqlDaCol};
use crate::sqlca::{self, Sqlca};
use nsql_core::{
    Caps, Column, CursorId, DbError, Dialect, DriverLimit, ExecRequest, ExecResult, ResultSet,
    Session, Value,
};
use nsql_script::ConnectSpec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// ★ 취소(T-426 2차 ②): DRDA에는 문장 중단이 없다 → **소켓을 끊는다**(SQL Server 어댑터와 같은 모델 · `drops_session` = 참 · 서버가 작업
///   단위를 롤백 · 호스트는 다음 실행 때 재접속). 실행 중(`active`)일 때만 끊는다(유휴 세션을 죽이지 않게).
#[derive(Debug)]
struct Db2Cancel {
    tcp: std::net::TcpStream,
    active: Arc<AtomicBool>,
}

impl nsql_core::CancelHandle for Db2Cancel {
    fn cancel(&self) -> Result<(), DbError> {
        if !self.active.load(Ordering::Acquire) {
            return Ok(());
        }
        self.tcp
            .shutdown(std::net::Shutdown::Both)
            .map_err(|e| err(format!("Db2: cancel (socket shutdown): {e}")))
    }
    fn drops_session(&self) -> bool {
        true
    }
}

const PKGID: &str = "SYSSH200";
const PKGCNSTKN: &str = "SYSLVL01";
const PKGSN: u16 = 65;
const QRYBLKSZ: u32 = 65_535;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

fn err(msg: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: msg.into(),
        position: None,
    }
}

/// 접속 하나.
#[derive(Debug)]
pub struct Db2Session {
    wire: Option<Wire>,
    info: ServerInfo,
    /// RDBNAM(18자 공백 채움 · PKGNAMCSN에도 쓴다).
    database: String,
    description: String,
    max_rows: usize,
    /// 수동 트랜잭션 열림(`BEGIN TRANSACTION` 뒤 · 커밋/롤백까지 자동 커밋 안 함).
    in_tx: bool,
    /// 실행 중 표식(취소 핸들이 본다).
    active: Arc<AtomicBool>,
    /// 취소용 소켓 복제(접속 때 한 번).
    cancel_tcp: Option<std::net::TcpStream>,
}

/// 결과 집합 하나(조회 1개 · CALL = 여럿 · 121b ⑥).
#[derive(Default)]
struct SetAcc {
    /// SQLCINRD(CALL 집합의 열 정보 · 없으면 PRPSQLSTT SQLDARD 또는 COLn).
    cols: Vec<SqlDaCol>,
    desc: Vec<ColDesc>,
    rows: Vec<Vec<Value>>,
    /// QRYDTA 블록 끝에서 잘린 행의 앞부분(다음 블록 앞에 붙인다).
    carry: Vec<u8>,
    /// LOB 값(EXTDTA · 행·열 순서 · T-426 2차 ①).
    extdta: Vec<Vec<u8>>,
    qryinsid: Option<[u8; 8]>,
    /// OPNQRYRM이 온 상관 번호(= OPNQRY/EXCSQLSTT의 번호 · CNTQRY/CLSQRY는 같은 번호로 · pydrda).
    corr: u16,
    open: bool,
}

/// 응답 체인 하나를 해석한 결과.
#[derive(Default)]
struct Reply {
    /// PRPSQLSTT의 SQLDARD(결과 열 · 첫 집합용).
    cols: Vec<SqlDaCol>,
    params: Vec<SqlDaCol>,
    sets: Vec<SetAcc>,
    /// 지금 QRYDSC/QRYDTA가 향하는 집합(OPNQRYRM 때 새로 · CNTQRY 때 호출자가 지정).
    cur: Option<usize>,
    /// RSLSETRM의 PKGSNLST(CALL 결과 집합마다 PKGNAMCSN 64B · CNTQRY/CLSQRY에 쓴다).
    pkgsnlst: Vec<Vec<u8>>,
    /// SQLDTARD(OUT 매개변수 값 · FDODSC+FDODTA 객체).
    out_dta: Option<Vec<u8>>,
    /// 첫 집합 전에 온 EXTDTA(OUT LOB · jb 주석).
    out_extdta: Vec<Vec<u8>>,
    sqlca: Option<Sqlca>,
    error: Option<DbError>,
    /// `error`가 SQLERRRM의 일반 문구인가(뒤에 오는 SQLCARD가 덮어쓴다).
    error_generic: bool,
}

impl Reply {
    fn set_mut(&mut self) -> &mut SetAcc {
        if self.sets.is_empty() {
            self.sets.push(SetAcc::default());
            self.cur = Some(0);
        }
        let i = self
            .cur
            .unwrap_or(self.sets.len() - 1)
            .min(self.sets.len() - 1);
        &mut self.sets[i]
    }
}

/// 값의 글자 표현(BLOB 리터럴 재작성에 문자열/숫자 값이 왔을 때).
fn fdoca_text(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::Decimal(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => {
            if *b {
                "1".into()
            } else {
                "0".into()
            }
        }
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Null | Value::Cursor(_) => String::new(),
    }
}

/// `CALL …`(프로시저 · 결과 집합·OUT 매개변수 = EXCSQLSTT 경로 · 121b ⑥).
fn is_call(sql: &str) -> bool {
    let s = sql.trim_start();
    s.len() >= 4
        && s.as_bytes()[..4].eq_ignore_ascii_case(b"CALL")
        && s[4..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '(')
}

fn is_query(sql: &str) -> bool {
    let mut s = sql.trim_start();
    loop {
        if let Some(r) = s.strip_prefix("--") {
            s = r.split_once('\n').map_or("", |(_, t)| t).trim_start();
        } else if let Some(r) = s.strip_prefix("/*") {
            s = r.split_once("*/").map_or("", |(_, t)| t).trim_start();
        } else {
            break;
        }
    }
    let s = s.trim_start_matches('(').trim_start();
    let w: String = s
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    matches!(w.as_str(), "SELECT" | "WITH" | "VALUES" | "XQUERY")
}

/// `COMMIT [WORK]` / `ROLLBACK [WORK]` 단독 문 → Some(커밋?) · `BEGIN [TRANSACTION|WORK]`·`START TRANSACTION` → None이지만 [`is_tx_begin`].
fn tx_control(sql: &str) -> Option<bool> {
    let w: Vec<String> = sql
        .trim()
        .trim_end_matches(';')
        .split_whitespace()
        .map(str::to_ascii_uppercase)
        .collect();
    match w.as_slice() {
        [a] | [a, _] if a == "COMMIT" => (w.len() == 1 || w[1] == "WORK").then_some(true),
        [a] | [a, _] if a == "ROLLBACK" => (w.len() == 1 || w[1] == "WORK").then_some(false),
        _ => None,
    }
}

fn is_tx_begin(sql: &str) -> bool {
    let w: Vec<String> = sql
        .trim()
        .trim_end_matches(';')
        .split_whitespace()
        .map(str::to_ascii_uppercase)
        .collect();
    match w.as_slice() {
        [a] => a == "BEGIN",
        [a, b] => {
            (a == "BEGIN" && (b == "TRANSACTION" || b == "TRAN" || b == "WORK"))
                || (a == "START" && b == "TRANSACTION")
        }
        _ => false,
    }
}

impl Db2Session {
    /// `db2://user:password@host:50000/DB?schema=X&tls=required` → 접속(핸드셰이크 포함).
    pub fn open(spec: &ConnectSpec) -> Result<Self, DbError> {
        let host = spec
            .host
            .as_deref()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| err("Db2: a host is required"))?;
        let database = spec
            .database
            .as_deref()
            .filter(|d| !d.is_empty())
            .ok_or_else(|| err("Db2: a database name is required"))?;
        let user = spec
            .user
            .as_deref()
            .ok_or_else(|| err("Db2: a user is required"))?;
        let password = spec.password.as_deref().unwrap_or("");
        let tls = spec.tls.as_deref().is_some_and(|t| {
            matches!(
                t.to_ascii_lowercase().as_str(),
                "required" | "require" | "on" | "true" | "yes" | "1"
            )
        });
        let port = spec.port.unwrap_or(if tls { 50_001 } else { 50_000 });
        let mut wire = Wire::connect(
            host,
            port,
            CONNECT_TIMEOUT,
            Duration::from_secs(60),
            tls.then_some(host),
        )?;
        let db_upper = database.to_ascii_uppercase();
        let login = Login {
            database: &db_upper,
            user,
            password,
            ext_name: "nsql",
        };
        let info = conn::handshake(&mut wire, &login)?;
        // 질의는 길 수 있다 → 핸드셰이크 뒤 읽기 상한 해제(강제 중지는 호스트 몫 · 2차 = 취소 접속).
        wire.set_read_timeout(None)?;
        let cancel_tcp = wire.tcp_clone().ok();
        let mut s = Db2Session {
            wire: Some(wire),
            info,
            // Db2 데이터베이스 이름은 대문자(서버가 RDBNAM을 대문자 EBCDIC로 돌려준다).
            database: format!("{:<18}", database.to_ascii_uppercase()),
            description: spec.redacted(),
            max_rows: 0,
            in_tx: false,
            active: Arc::new(AtomicBool::new(false)),
            cancel_tcp,
        };
        if let Some(schema) = spec.schema.as_deref().filter(|x| !x.is_empty()) {
            s.set_schema(schema)?;
        }
        Ok(s)
    }

    fn wire(&mut self) -> Result<&mut Wire, DbError> {
        self.wire.as_mut().ok_or_else(|| err("Db2: not connected"))
    }

    fn le(&self) -> bool {
        self.info.little_endian()
    }

    fn pkgnamcsn(&self) -> Ddm {
        // 64바이트 = RDBNAM 18 · RDBCOLID 18 · PKGID 18(셋 다 **EBCDIC cp500 · 대문자 · 0x40 채움** — CI 2차 40115496236: ASCII로 보내자
        //   SQL0805N 패키지 없음 · 서버는 RDBNAM을 EBCDIC 대문자로 돌려준다) · PKGCNSTKN 8(ASCII `SYSLVL01` · pydrda/go-db2) · PKGSN 2(큰 끝).
        let mut v = Vec::with_capacity(64);
        v.extend_from_slice(&crate::ebcdic::padded(self.database.trim(), 18));
        v.extend_from_slice(&crate::ebcdic::padded("NULLID", 18));
        v.extend_from_slice(&crate::ebcdic::padded(PKGID, 18));
        v.extend_from_slice(format!("{PKGCNSTKN:>8}").as_bytes());
        v.extend_from_slice(&PKGSN.to_be_bytes());
        Ddm::new(cp::PKGNAMCSN, v)
    }

    fn sqlstt(sql: &str) -> Ddm {
        // 널 허용 문자열 둘: `00 + 길이(4 · 큰 끝) + UTF-8` · `FF`(두 번째 = 없음).
        let b = sql.as_bytes();
        let mut v = Vec::with_capacity(b.len() + 6);
        v.push(0x00);
        v.extend_from_slice(&(b.len() as u32).to_be_bytes());
        v.extend_from_slice(b);
        v.push(0xFF);
        Ddm::new(cp::SQLSTT, v)
    }

    /// 요청 체인 — `cmds` = (명령, 딸린 객체들). 상관 번호 `first`부터 1씩 · 객체는 명령과 같은 번호.
    fn send_chain(&mut self, cmds: &[(Ddm, Vec<Ddm>)]) -> Result<(), DbError> {
        self.send_chain_from(cmds, 1)
    }

    fn send_chain_from(&mut self, cmds: &[(Ddm, Vec<Ddm>)], first: u16) -> Result<(), DbError> {
        let mut out = Vec::new();
        let n = cmds.len();
        for (i, (cmd, objs)) in cmds.iter().enumerate() {
            let corr = first.wrapping_add(i as u16);
            let last_cmd = i + 1 == n;
            let chained = !last_cmd || !objs.is_empty();
            crate::dss::encode_dss(
                DssKind::Request,
                chained,
                !objs.is_empty(),
                corr,
                cmd,
                &mut out,
            );
            let m = objs.len();
            for (j, o) in objs.iter().enumerate() {
                let last_obj = j + 1 == m;
                let chained = !last_cmd || !last_obj;
                crate::dss::encode_dss(DssKind::Object, chained, !last_obj, corr, o, &mut out);
            }
        }
        self.wire()?.send_raw(&out)
    }

    /// 응답 체인 하나를 `Reply`에 누적(코드포인트별 · 순서 비의존 · 121b ⑥-2).
    fn absorb(&mut self, chain: &[Dss], r: &mut Reply) -> Result<(), DbError> {
        let le = self.le();
        for d in chain {
            match d.obj.cp {
                cp::SQLCARD => {
                    let (ca, _) = sqlca::parse_sqlca(&d.obj.value, le)?;
                    if let Some(ca) = ca {
                        // SQLERRRM의 일반 문구보다 SQLCARD(SQLCODE·토큰)가 먼저(협업 10-25 "SQLCARD 내용을 안 보여 줌").
                        if ca.is_error() && (r.error.is_none() || r.error_generic) {
                            r.error = Some(ca.to_db_error());
                            r.error_generic = false;
                        }
                        if r.sqlca.is_none() || ca.is_error() {
                            r.sqlca = Some(ca);
                        }
                    }
                }
                cp::SQLDARD => {
                    let (ca, cols) = fdoca::parse_sqldard(&d.obj.value, le)?;
                    if let Some(ca) = ca {
                        if ca.is_error() && r.error.is_none() {
                            r.error = Some(ca.to_db_error());
                        }
                    }
                    if d.obj.value.first() == Some(&0xFF) {
                        r.params = cols;
                    } else if r.cols.is_empty() {
                        r.cols = cols;
                    }
                }
                cp::RSLSETRM => {
                    if let Some(l) = d.obj.child(cp::PKGSNLST) {
                        r.pkgsnlst = l
                            .children()
                            .into_iter()
                            .filter(|c| c.cp == cp::PKGNAMCSN)
                            .map(|c| c.value)
                            .collect();
                    }
                }
                cp::SQLDTARD => r.out_dta = Some(d.obj.value.clone()),
                cp::SQLRSLRD => {}
                cp::SQLCINRD => {
                    let cols = fdoca::parse_sqlcinrd(&d.obj.value, le)?;
                    r.set_mut().cols = cols;
                }
                cp::OPNQRYRM => {
                    let mut set = SetAcc {
                        corr: d.corr,
                        open: true,
                        ..SetAcc::default()
                    };
                    if let Some(id) = d.obj.child(cp::QRYINSID) {
                        if id.value.len() == 8 {
                            let mut a = [0u8; 8];
                            a.copy_from_slice(&id.value);
                            set.qryinsid = Some(a);
                        }
                    }
                    r.sets.push(set);
                    r.cur = Some(r.sets.len() - 1);
                }
                cp::QRYDSC => {
                    let desc = fdoca::parse_qrydsc(&d.obj.value)?;
                    r.set_mut().desc = desc;
                }
                cp::QRYDTA => {
                    let set = r.set_mut();
                    let mut data = std::mem::take(&mut set.carry);
                    data.extend_from_slice(&d.obj.value);
                    let (rows, used, sqlca_follows) = fdoca::read_rows(&data, &set.desc, le)?;
                    set.rows.extend(rows);
                    if sqlca_follows {
                        // 행 자리에 SQLCA = 이 집합의 끝(+100 · CI 10-25 CALL = ENDQRYRM 없이 이것만 옴) 또는 오류 · 경고는 ENDQRYRM이 뜻을 준다.
                        let (ca, _) = sqlca::parse_sqlca(&data[used..], le).unwrap_or((None, 0));
                        let err_ca = match ca {
                            Some(ca) => {
                                set.open = false;
                                ca.is_error().then_some(ca)
                            }
                            None => None,
                        };
                        if let Some(ca) = err_ca {
                            if r.error.is_none() {
                                r.error = Some(ca.to_db_error());
                            }
                        }
                    } else if used < data.len() {
                        set.carry = data[used..].to_vec();
                    }
                }
                cp::ENDQRYRM => {
                    r.set_mut().open = false;
                }
                cp::ENDUOWRM | cp::RDBUPDRM | cp::EXCSATRD => {}
                cp::SQLERRRM => {
                    if r.error.is_none() {
                        r.error = Some(err("DRDA SQLERRRM: SQL error (see SQLCARD)"));
                        r.error_generic = true;
                    }
                }
                cp::EXTDTA => {
                    // 첫 집합의 첫 QRYDTA 전에 온 EXTDTA = OUT 매개변수 LOB(jb · Db2 11.5.8).
                    if r.sets.is_empty()
                        || r.sets.iter().all(|s| s.rows.is_empty()) && r.out_dta.is_some()
                    {
                        r.out_extdta.push(d.obj.value.clone());
                    } else {
                        r.set_mut().extdta.push(d.obj.value.clone());
                    }
                }
                _ if d.kind == DssKind::Reply => {
                    if let Err(e) = conn::check_reply(std::slice::from_ref(d), &[]) {
                        if r.error.is_none() {
                            r.error = Some(e);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// 집합 `i`의 PKGNAMCSN(CALL = RSLSETRM의 PKGSNLST[i] · 조회 = 기본 섹션).
    fn set_pkgnamcsn(&self, r: &Reply, i: usize) -> Ddm {
        match r.pkgsnlst.get(i) {
            Some(v) if v.len() >= 64 => Ddm::new(cp::PKGNAMCSN, v.clone()),
            _ => self.pkgnamcsn(),
        }
    }

    /// 체인 보내고 응답을 끝까지 받아 해석 · 열린 집합은 차례로 상한까지 `CNTQRY`로 이어 받는다.
    fn round_trip(&mut self, cmds: &[(Ddm, Vec<Ddm>)]) -> Result<Reply, DbError> {
        self.send_chain(cmds)?;
        let mut r = Reply::default();
        let chain = self.wire()?.recv_chain()?;
        self.absorb(&chain, &mut r)?;
        let mut i = 0;
        let mut stalls = 0u32;
        while r.error.is_none() && i < r.sets.len() {
            if !r.sets[i].open {
                i += 1;
                continue;
            }
            if self.max_rows > 0 && r.sets[i].rows.len() >= self.max_rows {
                r.sets[i].rows.truncate(self.max_rows);
                self.close_set(&r, i)?;
                r.sets[i].open = false;
                i += 1;
                continue;
            }
            let cnt = Ddm::group(
                cp::CNTQRY,
                &[
                    self.set_pkgnamcsn(&r, i),
                    Ddm::u32(cp::QRYBLKSZ, QRYBLKSZ),
                    Ddm::new(cp::QRYINSID, r.sets[i].qryinsid.unwrap_or([0; 8]).to_vec()),
                    // LOB 값을 EXTDTA로 함께(pydrda `packCNTQRY` rtnextdta=0x02).
                    Ddm::new(cp::RTNEXTDTA, vec![0x02]),
                ],
            );
            let corr = r.sets[i].corr.max(1);
            r.cur = Some(i);
            let before = r.sets[i].rows.len();
            self.send_chain_from(&[(cnt, Vec::new())], corr)?;
            let chain = self.wire()?.recv_chain()?;
            self.absorb(&chain, &mut r)?;
            // 안전장치: 진전 없는 CNTQRY가 이어지면 끊는다(끝 표식을 놓친 경우 무한 반복 방지 · CI 40150244681).
            if r.sets[i].open && r.sets[i].rows.len() == before {
                stalls += 1;
                if stalls >= 2 {
                    r.error = Some(err(
                        "DRDA: CNTQRY returned no rows twice without end-of-query - giving up",
                    ));
                }
            } else {
                stalls = 0;
            }
        }
        if r.error.is_some() {
            for i in 0..r.sets.len() {
                if r.sets[i].open {
                    let _ = self.close_set(&r, i);
                }
            }
        }
        for set in &mut r.sets {
            if !set.extdta.is_empty() {
                fdoca::attach_lobs(&mut set.rows, &set.desc, &set.extdta);
            }
        }
        Ok(r)
    }

    fn close_set(&mut self, r: &Reply, i: usize) -> Result<(), DbError> {
        let cls = Ddm::group(
            cp::CLSQRY,
            &[
                self.set_pkgnamcsn(r, i),
                Ddm::new(cp::QRYINSID, r.sets[i].qryinsid.unwrap_or([0; 8]).to_vec()),
            ],
        );
        self.send_chain_from(&[(cls, Vec::new())], r.sets[i].corr.max(1))?;
        let chain = self.wire()?.recv_chain()?;
        let mut tmp = Reply::default();
        self.absorb(&chain, &mut tmp)?;
        Ok(())
    }

    fn result_set(set: &SetAcc, fallback: &[SqlDaCol]) -> ResultSet {
        let named = if set.cols.is_empty() {
            fallback
        } else {
            &set.cols
        };
        let columns = if named.len() == set.desc.len() && !named.is_empty() {
            named
                .iter()
                .enumerate()
                .map(|(i, c)| Column {
                    name: if c.name.is_empty() {
                        format!("COL{}", i + 1)
                    } else {
                        c.name.clone()
                    },
                    type_name: c.type_name(),
                })
                .collect()
        } else {
            set.desc
                .iter()
                .enumerate()
                .map(|(i, _)| Column {
                    name: format!("COL{}", i + 1),
                    type_name: String::new(),
                })
                .collect()
        };
        ResultSet {
            columns,
            rows: set.rows.clone(),
        }
    }

    fn opnqry(&self, with_params: bool) -> Ddm {
        let mut kids = vec![
            self.pkgnamcsn(),
            Ddm::u32(cp::QRYBLKSZ, QRYBLKSZ),
            Ddm::u16(cp::MAXBLKEXT, 0xFFFF),
            Ddm::new(cp::QRYCLSIMP, vec![0x01]),
        ];
        if with_params {
            kids.push(Ddm::new(cp::DYNDTAFMT, vec![0xF1]));
        }
        Ddm::group(cp::OPNQRY, &kids)
    }

    /// CALL 실행 명령(121b ⑥-1 · derby `writeExecuteCall`): OUTEXP(매개변수 있을 때) · QRYBLKSZ · MAXBLKEXT · MAXRSLCNT 무제한 · RSLSETFLG 확장 SQLDA.
    fn excsqlstt_call(&self, has_params: bool) -> Ddm {
        let mut kids = vec![self.pkgnamcsn(), Ddm::new(cp::RDBCMTOK, vec![0xF1])];
        if has_params {
            kids.push(Ddm::new(cp::OUTEXP, vec![0xF1]));
        }
        kids.push(Ddm::u32(cp::QRYBLKSZ, QRYBLKSZ));
        kids.push(Ddm::u16(cp::MAXBLKEXT, 0xFFFF));
        kids.push(Ddm::u16(cp::MAXRSLCNT, 0xFFFF));
        kids.push(Ddm::new(cp::RSLSETFLG, vec![0x04]));
        Ddm::group(cp::EXCSQLSTT, &kids)
    }

    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let sql = req.sql.trim().trim_end_matches(';').trim();
        let mut out = ExecResult::default();
        if let Some(commit) = tx_control(sql) {
            if commit {
                self.commit()?;
            } else {
                self.rollback()?;
            }
            return Ok(out);
        }
        if is_tx_begin(sql) {
            self.in_tx = true;
            return Ok(out);
        }
        let query = is_query(sql);
        let call = is_call(sql);
        let values: Vec<Value> = req.params.iter().map(|p| p.value.clone()).collect();
        let reply = if values.is_empty() && !call {
            if query {
                self.round_trip(&[
                    (
                        Ddm::group(
                            cp::PRPSQLSTT,
                            &[self.pkgnamcsn(), Ddm::new(cp::RTNSQLDA, vec![0xF1])],
                        ),
                        vec![Self::sqlstt(sql)],
                    ),
                    (self.opnqry(false), Vec::new()),
                ])?
            } else {
                self.round_trip(&[(
                    Ddm::group(
                        cp::EXCSQLIMM,
                        &[self.pkgnamcsn(), Ddm::new(cp::RDBCMTOK, vec![0xF1])],
                    ),
                    vec![Self::sqlstt(sql)],
                )])?
            }
        } else {
            // 1. 준비 + 기술 → 결과 열·매개변수 타입(CALL은 매개변수가 없어도 이 길 = 결과 집합을 받으려면 EXCSQLSTT).
            let mut prep_cmds = vec![(
                Ddm::group(
                    cp::PRPSQLSTT,
                    &[self.pkgnamcsn(), Ddm::new(cp::RTNSQLDA, vec![0xF1])],
                ),
                vec![Self::sqlstt(sql)],
            )];
            if !values.is_empty() {
                prep_cmds.push((
                    Ddm::group(
                        cp::DSCSQLSTT,
                        &[self.pkgnamcsn(), Ddm::new(cp::TYPSQLDA, vec![0x01])],
                    ),
                    Vec::new(),
                ));
            }
            let prep = self.round_trip(&prep_cmds)?;
            if let Some(e) = prep.error {
                return Err(e);
            }
            // ★ LOB 매개변수(BLOB 404/405 · CLOB 408/409 · DBCLOB 412/413 · XML 988/989)는 SQLDTA로 보낼 수 없어(VARGRAPHIC으로 보내면 DTAMCHRM −30020 ·
            //   CI 40149862498) 리터럴로 문장을 고쳐 다시 준비한다(pydrda BLOB 방식 · 문자 리터럴 한계 32 672 B = 2차 ⑥ EXTDTA 입력 전 제한).
            if let Some(k) = prep
                .params
                .iter()
                .position(|p| matches!(p.sqltype | 1, 405 | 409 | 413 | 989))
                .filter(|&k| k < values.len())
            {
                let is_blob = prep.params[k].sqltype | 1 == 405;
                let lit = if matches!(values[k], Value::Null) {
                    if is_blob {
                        "CAST(NULL AS BLOB)"
                    } else {
                        "CAST(NULL AS CLOB)"
                    }
                    .to_string()
                } else if is_blob {
                    let bytes = match &values[k] {
                        Value::Bytes(b) => b.clone(),
                        other => fdoca_text(other).into_bytes(),
                    };
                    let hex: String = bytes.iter().map(|x| format!("{x:02X}")).collect();
                    format!("BLOB(X'{hex}')")
                } else {
                    let text = match &values[k] {
                        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
                        other => fdoca_text(other),
                    };
                    if text.len() > 32_000 {
                        return Err(err(format!(
                            "Db2: character LOB bind parameters over 32 000 bytes are not supported yet ({} bytes) - phase 2 (EXTDTA input)",
                            text.len()
                        )));
                    }
                    format!("CLOB('{}')", text.replace('\'', "''"))
                };
                let sql2 = fdoca::replace_placeholder(sql, k, &lit);
                let mut params2 = req.params.clone();
                params2.remove(k);
                let req2 = ExecRequest {
                    sql: sql2,
                    params: params2,
                };
                return self.execute_inner(&req2);
            }
            let le = self.le();
            let objs = if values.is_empty() {
                Vec::new()
            } else {
                vec![Ddm::new(
                    cp::SQLDTA,
                    fdoca::encode_sqldta(&prep.params, &values, le)?,
                )]
            };
            // 2. 실행(DML/CALL) 또는 열기(조회) + 데이터.
            let mut r = if call {
                self.round_trip(&[(self.excsqlstt_call(!values.is_empty()), objs)])?
            } else if query {
                self.round_trip(&[(self.opnqry(true), objs)])?
            } else {
                self.round_trip(&[(
                    Ddm::group(
                        cp::EXCSQLSTT,
                        &[self.pkgnamcsn(), Ddm::new(cp::RDBCMTOK, vec![0xF1])],
                    ),
                    objs,
                )])?
            };
            if r.cols.is_empty() {
                r.cols = prep.cols;
            }
            r
        };
        if let Some(e) = reply.error {
            if !self.in_tx {
                let _ = self.rollback_quiet();
            }
            return Err(e);
        }
        // 결과 집합(조회 1 · CALL N).
        for (i, set) in reply.sets.iter().enumerate() {
            if set.desc.is_empty() && set.rows.is_empty() && !query {
                continue;
            }
            let fallback: &[SqlDaCol] = if i == 0 { &reply.cols } else { &[] };
            out.result_sets.push(Self::result_set(set, fallback));
            out.result_labels.push(None);
        }
        // OUT 매개변수(SQLDTARD = FDODSC + FDODTA · 121b ⑥-4) → 이름 = 요청의 OUT/INOUT 매개변수 순서.
        if let Some(dta) = &reply.out_dta {
            let le = self.le();
            let objs = crate::dss::parse_objects(dta).unwrap_or_default();
            let dsc = objs
                .iter()
                .find(|o| o.cp == cp::FDODSC)
                .map(|o| o.value.clone());
            let dat = objs
                .iter()
                .find(|o| o.cp == cp::FDODTA)
                .map(|o| o.value.clone());
            if let (Some(dsc), Some(dat)) = (dsc, dat) {
                let cols = fdoca::parse_qrydsc(&dsc)?;
                let mut row = fdoca::read_out_row(&dat, &cols, le)?;
                if !reply.out_extdta.is_empty() {
                    let mut rows = vec![row];
                    fdoca::attach_lobs(&mut rows, &cols, &reply.out_extdta);
                    row = rows.remove(0);
                }
                // FDODTA = 매개변수 전부 한 행(IN은 지시자 0x80 = NULL · CI 10-25) → OUT/INOUT만 순번으로 · 길이가 다르면 OUT 순서대로.
                if row.len() == req.params.len() {
                    for (p, v) in req.params.iter().zip(row) {
                        if !matches!(p.direction, nsql_core::Direction::In) {
                            out.out_params.push((p.name.clone(), v));
                        }
                    }
                } else {
                    let outs = req
                        .params
                        .iter()
                        .filter(|p| !matches!(p.direction, nsql_core::Direction::In));
                    for (p, v) in outs.zip(row) {
                        out.out_params.push((p.name.clone(), v));
                    }
                }
            }
        }
        if out.result_sets.is_empty() {
            if let Some(ca) = &reply.sqlca {
                let n = ca.errd[2];
                out.rows_affected = (n >= 0).then_some(n as u64);
            }
        }
        if !self.in_tx {
            self.commit()?;
        }
        Ok(out)
    }

    fn uow(&mut self, commit: bool) -> Result<(), DbError> {
        let cpnt = if commit { cp::RDBCMM } else { cp::RDBRLLBCK };
        let r = self.round_trip(&[(Ddm::new(cpnt, Vec::new()), Vec::new())])?;
        if let Some(e) = r.error {
            return Err(e);
        }
        Ok(())
    }

    fn rollback_quiet(&mut self) -> Result<(), DbError> {
        self.uow(false)
    }

    fn set_schema(&mut self, schema: &str) -> Result<(), DbError> {
        let q = format!("SET SCHEMA \"{}\"", schema.replace('"', "\"\""));
        let r = self.round_trip(&[(
            Ddm::group(
                cp::EXCSQLIMM,
                &[self.pkgnamcsn(), Ddm::new(cp::RDBCMTOK, vec![0xF1])],
            ),
            vec![Self::sqlstt(&q)],
        )])?;
        if let Some(e) = r.error {
            return Err(e);
        }
        if !self.in_tx {
            self.commit()?;
        }
        Ok(())
    }
}

impl Session for Db2Session {
    fn dialect(&self) -> Dialect {
        Dialect::Db2
    }

    fn caps(&self) -> Caps {
        Caps::of(Dialect::Db2)
    }

    fn driver_name(&self) -> String {
        let mut s = format!("drda ({})", conn::PRDID);
        if !self.info.prdid.is_empty() {
            s.push_str(&format!(" · server {}", self.info.prdid));
        }
        if !self.info.class.is_empty() {
            s.push_str(&format!(" {}", self.info.class));
        }
        s
    }

    fn limits(&self) -> &'static [DriverLimit] {
        &[]
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        self.active.store(true, Ordering::Release);
        let r = self.execute_inner(req);
        self.active.store(false, Ordering::Release);
        if let Err(e) = &r {
            // 전송 오류(취소로 끊김 · 서버 종료)면 세션을 죽은 것으로(호스트 = 다음 실행 때 재접속).
            if e.code.is_none()
                && (e.message.starts_with("DRDA recv") || e.message.starts_with("DRDA send"))
            {
                self.wire = None;
            }
        }
        r
    }

    fn cancel_handle(&self) -> Option<Arc<dyn nsql_core::CancelHandle>> {
        self.cancel_tcp
            .as_ref()
            .and_then(|t| t.try_clone().ok())
            .map(|tcp| {
                Arc::new(Db2Cancel {
                    tcp,
                    active: Arc::clone(&self.active),
                }) as Arc<dyn nsql_core::CancelHandle>
            })
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(err("Db2: server cursors are not supported"))
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        match name {
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            "schema" if !value.trim().is_empty() => self.set_schema(value.trim())?,
            _ => {}
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DbError> {
        self.in_tx = false;
        self.uow(true)
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.in_tx = false;
        self.uow(false)
    }

    fn is_alive(&self) -> bool {
        self.wire.is_some()
    }
}

impl Drop for Db2Session {
    fn drop(&mut self) {
        // 되감기 뒤 소켓 닫기(best-effort · 서버가 작업 단위를 정리한다).
        if self.wire.is_some() {
            let _ = self.rollback_quiet();
        }
        self.wire = None;
    }
}

// ★ 확장 드라이버 내보내기(D-350 · ABI v2): cdylib로 빌드하면 `nsql_driver_vtable`이 나간다(id `db2` · 방언 `db2`).
fn nsql_abi_open_from_spec(spec: &ConnectSpec) -> Result<Db2Session, DbError> {
    Db2Session::open(spec)
}
nsql_abi::export_driver!("db2", "db2", nsql_abi_open_from_spec);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_detection_and_tx_statements() {
        assert!(is_query("SELECT 1 FROM SYSIBM.SYSDUMMY1"));
        assert!(is_query(
            "  -- c\n /* x */ with t as (values 1) select * from t"
        ));
        assert!(is_query("(SELECT 1) UNION (SELECT 2)"));
        assert!(is_query("VALUES CURRENT SCHEMA"));
        assert!(!is_query("INSERT INTO t VALUES (1)"));
        assert!(!is_query("CALL p(?)"));
        assert!(is_call("call NSQLP(?, ?)") && is_call("CALL(x)") && !is_call("CALLER_X"));
        assert_eq!(tx_control("commit;"), Some(true));
        assert_eq!(tx_control("ROLLBACK WORK"), Some(false));
        assert_eq!(tx_control("ROLLBACK TO SAVEPOINT a"), None);
        assert!(is_tx_begin("BEGIN TRANSACTION") && is_tx_begin("start transaction"));
        assert!(!is_tx_begin("BEGIN ATOMIC"));
    }

    #[test]
    fn pkgnamcsn_is_64_bytes_ascii() {
        let s = Db2Session {
            wire: None,
            info: ServerInfo::default(),
            database: format!("{:<18}", "SAMPLE"),
            description: String::new(),
            max_rows: 0,
            in_tx: false,
            active: Arc::new(AtomicBool::new(false)),
            cancel_tcp: None,
        };
        assert!(s.cancel_handle().is_none(), "소켓 없음 = 취소 핸들 없음");
        let p = s.pkgnamcsn();
        assert_eq!(p.value.len(), 64);
        // EBCDIC: S=0xE2 A=0xC1 M=0xD4 P=0xD7 L=0xD3 E=0xC5 · 공백 0x40 · 토큰은 ASCII.
        assert_eq!(&p.value[..7], &[0xE2, 0xC1, 0xD4, 0xD7, 0xD3, 0xC5, 0x40]);
        assert_eq!(&p.value[18..24], &crate::ebcdic::from_utf8("NULLID")[..]);
        assert_eq!(&p.value[36..44], &crate::ebcdic::from_utf8("SYSSH200")[..]);
        assert_eq!(&p.value[54..62], b"SYSLVL01");
        assert_eq!(&p.value[62..], &[0x00, 0x41]);
        let st = Db2Session::sqlstt("VALUES 1");
        assert_eq!(&st.value[..5], &[0x00, 0x00, 0x00, 0x00, 0x08]);
        assert_eq!(*st.value.last().unwrap(), 0xFF);
    }
}
