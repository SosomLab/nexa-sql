//! DRDA 접속(와이어 + 핸드셰이크) — `EXCSAT → ACCSEC → SECCHK → ACCRDB`(Derby `NetConnection`·pydrda와 같은 순서).
//!
//! - [`Wire`] = `nsql_net::Socket` + 수신 버퍼 + 상관 번호. `send`는 DSS 체인 하나를 한 번에 쓰고, `recv_chain`은 체인 비트가 꺼질 때까지 DSS를 모은다.
//! - 매니저 문자 파라미터(EXTNAM · SRVNAM · RDBNAM · USRID · PASSWORD …) = EBCDIC CP500 · SQL 문자 = TYPDEFOVR(CCSID 1208 UTF-8 · 1200 UTF-16).
//! - 서버가 거부하면 응답 코드포인트(RDBNFNRM · SECCHKRM(SECCHKCD≠0) · RDBAFLRM · MGRLVLRM …) + SVRCOD를 오류 메시지에 담는다.
//! - 보안 = 1차 USRIDPWD(3 · 평문 · TLS 권장) · EUSRIDPWD(9 · DH + DES)는 2차(docs/121b ②).

use crate::codepoint as cp;
use crate::dss::{decode_dss, encode_dss, Ddm, Dss, DssKind};
use crate::ebcdic;
use nsql_core::DbError;
use std::io::{Read, Write};
use std::time::Duration;

/// 서버가 알려 준 접속 사실(ACCRDBRM · EXCSATRD).
#[derive(Clone, Debug, Default)]
pub struct ServerInfo {
    /// 서버 클래스 이름(`QDB2/LINUXX8664` 등 · EXCSATRD SRVCLSNM).
    pub class: String,
    /// 서버 제품 식별자(`SQL11059` 등 · ACCRDBRM PRDID).
    pub prdid: String,
    /// 서버 TYPDEFNAM(`QTDSQLX86` = SQL 데이터 정수·실수가 **작은 끝** · `QTDSQLASC`/`QTDSQL370` = 큰 끝).
    pub typdef: String,
    /// 서버 SQL 문자 CCSID(TYPDEFOVR · 보통 1208).
    pub ccsid_sbc: u16,
    pub ccsid_dbc: u16,
    pub ccsid_mbc: u16,
}

impl ServerInfo {
    /// SQL 데이터(SQLCARD·QRYDTA 안의 정수·실수)가 작은 끝인가.
    #[must_use]
    pub fn little_endian(&self) -> bool {
        self.typdef.eq_ignore_ascii_case("QTDSQLX86")
    }
}

pub struct Wire {
    sock: nsql_net::Socket,
    rbuf: Vec<u8>,
    corr: u16,
}

impl std::fmt::Debug for Wire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Wire(corr={})", self.corr)
    }
}

fn io_err(what: &str, e: impl std::fmt::Display) -> DbError {
    DbError {
        code: None,
        message: format!("DRDA {what}: {e}"),
        position: None,
    }
}

impl Wire {
    pub fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        io: Duration,
        tls: Option<&str>,
    ) -> Result<Wire, DbError> {
        let sock = nsql_net::Socket::connect(host, port, timeout, io, tls)
            .map_err(|e| io_err("connect", e))?;
        Ok(Wire {
            sock,
            rbuf: Vec::with_capacity(64 * 1024),
            corr: 0,
        })
    }

    pub fn set_read_timeout(&mut self, d: Option<Duration>) -> Result<(), DbError> {
        self.sock
            .set_read_timeout(d)
            .map_err(|e| io_err("set timeout", e))
    }

    fn next_corr(&mut self) -> u16 {
        self.corr = self.corr.wrapping_add(1).max(1);
        self.corr
    }

    /// 요청 DSS 체인 하나를 보낸다 — `parts` = (종류, 객체) · 마지막 전까지 체인 비트 · **요청 DSS마다 상관 번호 1씩 증가**(pydrda · 10-25
    /// CI 첫 실접속 = 같은 번호 + 0x10으로 보내자 서버가 머리에서 PROTOCOL ERROR) · 객체 DSS는 앞 요청과 같은 번호(앞 요청에 0x10).
    pub fn send(&mut self, parts: &[(DssKind, Ddm)]) -> Result<u16, DbError> {
        let mut out = Vec::new();
        let n = parts.len();
        let mut corr = 0u16;
        for (i, (kind, obj)) in parts.iter().enumerate() {
            let last = i + 1 == n;
            let next_is_obj = parts.get(i + 1).is_some_and(|(k, _)| *k == DssKind::Object);
            if *kind == DssKind::Request {
                corr = self.next_corr();
            }
            encode_dss(*kind, !last, next_is_obj, corr, obj, &mut out);
        }
        // ★ SECCHK(USRID·PASSWORD·SECTKN)는 추적에 찍지 않는다(협업 10-25: 공개 CI 로그에 비밀번호 바이트가 남았다).
        let secret = parts.iter().any(|(_, o)| o.cp == cp::SECCHK);
        if secret {
            Self::trace_note("send: SECCHK chain (credentials masked)");
            self.send_raw_quiet(&out)?;
        } else {
            self.send_raw(&out)?;
        }
        Ok(corr)
    }

    fn trace_note(s: &str) {
        if std::env::var_os("NSQL_TRACE_DRDA").is_some() {
            eprintln!("[drda {s}]");
        }
    }

    fn send_raw_quiet(&mut self, bytes: &[u8]) -> Result<(), DbError> {
        self.sock
            .write_all(bytes)
            .and_then(|()| self.sock.flush())
            .map_err(|e| io_err("send", e))
    }

    /// `NSQL_TRACE_DRDA=1`이면 송수신 바이트 앞부분을 stderr로(CI 첫 실접속 진단 · 서버 db2diag RECEIVE BUFFER와 대조).
    fn trace(dir: &str, b: &[u8]) {
        if std::env::var_os("NSQL_TRACE_DRDA").is_none() {
            return;
        }
        let n = b.len().min(512);
        let hex: Vec<String> = b[..n].iter().map(|x| format!("{x:02X}")).collect();
        eprintln!(
            "[drda {dir} {} B] {}{}",
            b.len(),
            hex.join(" "),
            if b.len() > n { " …" } else { "" }
        );
    }

    /// 이미 인코딩된 DSS 체인을 그대로 보낸다(세션이 상관 번호를 직접 매긴다).
    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<(), DbError> {
        Self::trace("send", bytes);
        self.sock
            .write_all(bytes)
            .and_then(|()| self.sock.flush())
            .map_err(|e| io_err("send", e))
    }

    /// 응답 체인을 끝(체인 비트 꺼짐)까지 읽는다.
    pub fn recv_chain(&mut self) -> Result<Vec<Dss>, DbError> {
        let mut out = Vec::new();
        loop {
            let d = self.recv_one()?;
            let more = d.chained;
            out.push(d);
            if !more {
                return Ok(out);
            }
        }
    }

    fn recv_one(&mut self) -> Result<Dss, DbError> {
        loop {
            match decode_dss(&self.rbuf).map_err(|e| io_err("decode", e))? {
                Some((d, used)) => {
                    self.rbuf.drain(..used);
                    return Ok(d);
                }
                None => {
                    let mut chunk = [0u8; 32 * 1024];
                    let n = self.sock.read(&mut chunk).map_err(|e| io_err("recv", e))?;
                    if n == 0 {
                        return Err(io_err("recv", "connection closed by server"));
                    }
                    Self::trace("recv", &chunk[..n]);
                    self.rbuf.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
}

/// 핸드셰이크 입력.
#[derive(Clone, Debug)]
pub struct Login<'a> {
    pub database: &'a str,
    pub user: &'a str,
    pub password: &'a str,
    /// EXTNAM(클라이언트 이름 · 서버 모니터에 보인다).
    pub ext_name: &'a str,
}

/// 클라이언트가 내는 제품 식별자(8자 · pydrda = `SQL12010` = Db2 12.1 클라이언트 흉내 · 서버가 수준 판정에 쓸 수 있어 같은 값 · docs/121b ①(a)).
pub const PRDID: &str = "SQL12010";
/// 클라이언트 TYPDEFNAM — 우리가 **보내는** SQL 데이터의 꼴(`QTDSQLX86` = 작은 끝 정수 · IEEE 실수 · pydrda와 같음 · 121b ①(a)).
pub const TYPDEF_CLIENT: &str = "QTDSQLX86";

fn ebc(cp: u16, s: &str) -> Ddm {
    Ddm::new(cp, ebcdic::from_utf8(s))
}

fn mgrlvlls() -> Ddm {
    let mut v = Vec::new();
    for (m, l) in [
        (cp::AGENT, cp::MGRLVL_AGENT),
        (cp::SQLAM, cp::MGRLVL_SQLAM),
        (cp::RDB, cp::MGRLVL_RDB),
        (cp::SECMGR, cp::MGRLVL_SECMGR),
        (cp::CMNTCPIP, cp::MGRLVL_CMNTCPIP),
        (cp::UNICODEMGR, cp::MGRLVL_UNICODEMGR),
    ] {
        v.extend_from_slice(&m.to_be_bytes());
        v.extend_from_slice(&l.to_be_bytes());
    }
    Ddm::new(cp::MGRLVLLS, v)
}

/// 응답 체인에서 오류 응답을 찾아 `DbError`로 — 없으면 `Ok`.
pub fn check_reply(chain: &[Dss], expect: &[u16]) -> Result<(), DbError> {
    for d in chain {
        if d.kind != DssKind::Reply {
            continue;
        }
        if expect.contains(&d.obj.cp) {
            continue;
        }
        let svrcod = d
            .obj
            .child(cp::SVRCOD)
            .and_then(|c| c.as_u16())
            .unwrap_or(0);
        let name = reply_name(d.obj.cp);
        let detail = d
            .obj
            .child(cp::SECCHKCD)
            .and_then(|c| c.as_u8())
            .map(|c| format!(" SECCHKCD={c} ({})", secchkcd_text(c)))
            .unwrap_or_default();
        let rdb = d
            .obj
            .child(cp::RDBNAM)
            .map(|c| format!(" RDBNAM={}", ebcdic::to_utf8(&c.value).trim()))
            .unwrap_or_default();
        return Err(DbError {
            code: None,
            message: format!(
                "DRDA {name} (0x{:04X}) SVRCOD={svrcod}{detail}{rdb}",
                d.obj.cp
            ),
            position: None,
        });
    }
    Ok(())
}

fn reply_name(c: u16) -> &'static str {
    match c {
        cp::RDBNFNRM => "RDBNFNRM: database not found",
        cp::RDBAFLRM => "RDBAFLRM: access to database failed",
        cp::RDBATHRM => "RDBATHRM: not authorized to database",
        cp::SECCHKRM => "SECCHKRM: security check failed",
        cp::MGRLVLRM => "MGRLVLRM: manager level not supported",
        cp::CMDCHKRM => "CMDCHKRM: command check",
        cp::PRCCNVRM => "PRCCNVRM: conversational protocol error",
        cp::SYNTAXRM => "SYNTAXRM: data stream syntax error",
        cp::AGNPRMRM => "AGNPRMRM: permanent agent error",
        cp::ABNUOWRM => "ABNUOWRM: unit of work aborted",
        cp::RSCLMTRM => "RSCLMTRM: resource limit reached",
        cp::SQLERRRM => "SQLERRRM: SQL error",
        cp::OPNQFLRM => "OPNQFLRM: open query failed",
        cp::QRYNOPRM => "QRYNOPRM: query not open",
        cp::DSCINVRM => "DSCINVRM: description invalid",
        _ => "unexpected reply",
    }
}

fn secchkcd_text(c: u8) -> &'static str {
    match c {
        0x00 => "ok",
        0x01 => "SECMEC not supported",
        0x02 => "DCE informational",
        0x03 => "DCE retry",
        0x04 => "DCE error",
        0x05 => "GSSAPI informational",
        0x06 => "GSSAPI retry",
        0x07 => "GSSAPI error",
        0x08 => "local security service info",
        0x09 => "local security service retry",
        0x0A => "local security service error",
        0x0B => "SECTKN missing or invalid",
        0x0E => "password expired",
        0x0F => "password invalid",
        0x10 => "password missing",
        0x12 => "user id missing",
        0x13 => "user id invalid",
        0x14 => "user id revoked",
        0x15 => "new password invalid",
        _ => "?",
    }
}

/// 핸드셰이크 — 성공하면 서버 정보. `secmec` = 시도할 메커니즘(1차 = USRIDPWD).
pub fn handshake(w: &mut Wire, l: &Login<'_>) -> Result<ServerInfo, DbError> {
    let mut info = ServerInfo::default();
    // 1. EXCSAT ─ 매니저 레벨 교환.
    let excsat = Ddm::group(
        cp::EXCSAT,
        &[
            ebc(cp::EXTNAM, l.ext_name),
            mgrlvlls(),
            ebc(cp::SRVCLSNM, "QDB2/JVM"),
            ebc(cp::SRVNAM, "nsql"),
            ebc(cp::SRVRLSLV, PRDID),
        ],
    );
    // 2. ACCSEC ─ 메커니즘 협상(같은 체인).
    let rdbnam = Ddm::new(cp::RDBNAM, ebcdic::padded(l.database, 18));
    let accsec = Ddm::group(
        cp::ACCSEC,
        &[Ddm::u16(cp::SECMEC, cp::SECMEC_USRIDPWD), rdbnam.clone()],
    );
    w.send(&[(DssKind::Request, excsat), (DssKind::Request, accsec)])?;
    let chain = w.recv_chain()?;
    check_reply(&chain, &[cp::EXCSATRD, cp::ACCSECRD])?;
    for d in &chain {
        if d.obj.cp == cp::EXCSATRD {
            if let Some(c) = d.obj.child(cp::SRVCLSNM) {
                info.class = ebcdic::to_utf8(&c.value).trim().to_string();
            }
        }
        if d.obj.cp == cp::ACCSECRD {
            // 서버가 다른 메커니즘을 요구하면(SECMEC ≠ 3) 1차는 중단 · 안내.
            if let Some(m) = d.obj.child(cp::SECMEC).and_then(|c| c.as_u16()) {
                if m != cp::SECMEC_USRIDPWD {
                    return Err(DbError {
                        code: None,
                        message: format!(
                            "DRDA: server requires security mechanism {m} (encrypted); only USRIDPWD (3) is supported yet - enable TLS or allow SERVER authentication"
                        ),
                        position: None,
                    });
                }
            }
        }
    }
    // 3. SECCHK ─ 자격 · 4. ACCRDB ─ 데이터베이스 접근(같은 체인).
    let secchk = Ddm::group(
        cp::SECCHK,
        &[
            Ddm::u16(cp::SECMEC, cp::SECMEC_USRIDPWD),
            rdbnam.clone(),
            ebc(cp::USRID, l.user),
            ebc(cp::PASSWORD, l.password),
        ],
    );
    let typdefovr = Ddm::group(
        cp::TYPDEFOVR,
        &[
            Ddm::u16(cp::CCSIDSBC, 1208),
            Ddm::u16(cp::CCSIDDBC, 1200),
            Ddm::u16(cp::CCSIDMBC, 1208),
        ],
    );
    let crrtkn = Ddm::new(cp::CRRTKN, correlation_token(l.ext_name));
    let accrdb = Ddm::group(
        cp::ACCRDB,
        &[
            Ddm::u16(cp::RDBACCCL, cp::SQLAM),
            crrtkn,
            rdbnam,
            ebc(cp::PRDID, PRDID),
            ebc(cp::TYPDEFNAM, TYPDEF_CLIENT),
            typdefovr,
        ],
    );
    w.send(&[(DssKind::Request, secchk), (DssKind::Request, accrdb)])?;
    let chain = w.recv_chain()?;
    // SECCHKRM은 기대 응답이지만 SECCHKCD ≠ 0이면 실패.
    for d in &chain {
        if d.obj.cp == cp::SECCHKRM {
            let code = d
                .obj
                .child(cp::SECCHKCD)
                .and_then(|c| c.as_u8())
                .unwrap_or(0);
            if code != 0 {
                return Err(DbError {
                    code: None,
                    message: format!("DRDA SECCHKRM: {} (SECCHKCD={code})", secchkcd_text(code)),
                    position: None,
                });
            }
        }
    }
    check_reply(&chain, &[cp::SECCHKRM, cp::ACCRDBRM])?;
    for d in &chain {
        if d.obj.cp == cp::ACCRDBRM {
            if let Some(c) = d.obj.child(cp::PRDID) {
                info.prdid = ebcdic::to_utf8(&c.value).trim().to_string();
            }
            if let Some(c) = d.obj.child(cp::TYPDEFNAM) {
                info.typdef = ebcdic::to_utf8(&c.value).trim().to_string();
            }
            if let Some(o) = d.obj.child(cp::TYPDEFOVR) {
                for k in o.children() {
                    match k.cp {
                        cp::CCSIDSBC => info.ccsid_sbc = k.as_u16().unwrap_or(0),
                        cp::CCSIDDBC => info.ccsid_dbc = k.as_u16().unwrap_or(0),
                        cp::CCSIDMBC => info.ccsid_mbc = k.as_u16().unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }
    }
    if info.typdef.is_empty() {
        info.typdef = "QTDSQLASC".into();
    }
    Ok(info)
}

/// 상관 토큰(CRRTKN · 접속마다 고유 · 서버 모니터의 CLIENT_... 표시) — `nsql.<pid>.<nanos>` EBCDIC.
fn correlation_token(ext: &str) -> Vec<u8> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let s = format!(
        "{}.{}.{:x}",
        ext.chars().take(8).collect::<String>(),
        std::process::id(),
        nanos
    );
    ebcdic::from_utf8(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mgrlvlls_and_reply_check() {
        let m = mgrlvlls();
        assert_eq!(m.value.len(), 6 * 4);
        assert_eq!(&m.value[..4], &[0x14, 0x03, 0x00, 0x0A], "AGENT 10");
        // RDBNFNRM(SVRCOD 8 · RDBNAM) = 오류 · ACCRDBRM = 통과.
        let bad = Dss {
            kind: DssKind::Reply,
            chained: false,
            same_corr: false,
            cont_on_err: false,
            corr: 1,
            obj: Ddm::group(
                cp::RDBNFNRM,
                &[
                    Ddm::u16(cp::SVRCOD, 8),
                    Ddm::new(cp::RDBNAM, ebcdic::padded("NOPE", 18)),
                ],
            ),
        };
        let e = check_reply(&[bad], &[cp::ACCRDBRM]).unwrap_err();
        assert!(
            e.message.contains("database not found") && e.message.contains("RDBNAM=NOPE"),
            "{e:?}"
        );
        let ok = Dss {
            kind: DssKind::Reply,
            chained: false,
            same_corr: false,
            cont_on_err: false,
            corr: 1,
            obj: Ddm::group(cp::ACCRDBRM, &[Ddm::u16(cp::SVRCOD, 0)]),
        };
        assert!(check_reply(&[ok], &[cp::ACCRDBRM]).is_ok());
        assert!(ServerInfo {
            typdef: "QTDSQLX86".into(),
            ..ServerInfo::default()
        }
        .little_endian());
    }
}
