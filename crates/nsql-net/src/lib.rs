//! ★ 최소 HTTPS 클라이언트(T-29 · docs/22 §3 "네트워크" 행): **GET 하나**만 — HTTP/1.1 · rustls + webpki-roots · 리다이렉트 ≤ 5 ·
//! `Content-Length` / `chunked` / 닫힘 종료 · `HTTPS_PROXY`(`https_proxy`) CONNECT 터널 · 접속·읽기·쓰기 타임아웃.
//!
//! - 외부 crate = `rustls`(이미 트리 안 · DR-3 원장 D-348) · `webpki-roots`. HTTP 파서는 자체(요청 1종이라 작다).
//! - **스스로 트래픽을 만들지 않는다** — 호출자가 사용자 동작(설치·검색·갱신 확인)에서만 부른다(26 §8 · 재시도 없음 · 동시 1).
//! - `http://`도 받는다(로컬 시험 서버 · 프록시 없는 평문) — 배포 저장소 URL은 호출자가 `https`로 고정한다.
//! - 리다이렉트로 **호스트가 바뀌면 `Authorization`을 떼고** 따라간다(GitHub → objects.githubusercontent.com · 서명 URL과 충돌).
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// 오류 — 네트워크 층의 사실만(HTTP 상태 코드는 오류가 아니라 [`Response::status`]).
#[derive(Debug)]
pub enum NetError {
    Url(String),
    Io(io::Error),
    Tls(String),
    Proxy(String),
    Http(String),
    TooLarge(u64),
    TooManyRedirects,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Url(s) => write!(f, "bad URL: {s}"),
            NetError::Io(e) => write!(f, "{e}"),
            NetError::Tls(s) => write!(f, "TLS: {s}"),
            NetError::Proxy(s) => write!(f, "proxy: {s}"),
            NetError::Http(s) => write!(f, "HTTP: {s}"),
            NetError::TooLarge(n) => write!(f, "body larger than {n} bytes"),
            NetError::TooManyRedirects => write!(f, "too many redirects"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

/// 나눈 URL(`scheme://host[:port]/path?query` · 사용자 정보 없음).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    /// `/`로 시작(쿼리 포함).
    pub path: String,
}

impl Url {
    /// `Host:` 헤더 값(기본 포트면 포트 생략).
    #[must_use]
    pub fn host_header(&self) -> String {
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// 다시 문자열로.
    #[must_use]
    pub fn to_string_url(&self) -> String {
        format!(
            "{}://{}{}",
            if self.https { "https" } else { "http" },
            self.host_header(),
            self.path
        )
    }

    /// `Location` 값을 이 URL 기준으로 푼다(절대 · `/경로` · 상대 경로).
    pub fn join(&self, location: &str) -> Result<Url, NetError> {
        if location.starts_with("http://") || location.starts_with("https://") {
            return parse_url(location);
        }
        let mut u = self.clone();
        if let Some(rest) = location.strip_prefix("//") {
            return parse_url(&format!(
                "{}://{rest}",
                if self.https { "https" } else { "http" }
            ));
        }
        if location.starts_with('/') {
            u.path = location.to_string();
        } else {
            let base = self.path.split('?').next().unwrap_or("/");
            let dir = match base.rfind('/') {
                Some(i) => &base[..=i],
                None => "/",
            };
            u.path = format!("{dir}{location}");
        }
        Ok(u)
    }
}

/// URL 파싱(순수). `http`·`https`만 · IPv6 호스트는 `[...]` 표기.
pub fn parse_url(s: &str) -> Result<Url, NetError> {
    let (https, rest) = if let Some(r) = s.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = s.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(NetError::Url(format!("scheme must be http(s): {s}")));
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(NetError::Url(format!("bad authority: {s}")));
    }
    let (host, port) = if let Some(end) = authority.strip_prefix('[').and_then(|a| a.find(']')) {
        let host = &authority[1..=end];
        let port = authority[end + 2..].strip_prefix(':');
        (host.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (authority.to_string(), None),
        }
    };
    let port = match port {
        Some(p) => p
            .parse::<u16>()
            .map_err(|_| NetError::Url(format!("bad port: {s}")))?,
        None if https => 443,
        None => 80,
    };
    if host.is_empty() {
        return Err(NetError::Url(format!("empty host: {s}")));
    }
    Ok(Url {
        https,
        host,
        port,
        path: path.to_string(),
    })
}

/// 요청 옵션.
#[derive(Clone, Debug)]
pub struct Options {
    /// 접속·읽기·쓰기 각각의 상한.
    pub timeout: Duration,
    /// ★ 읽기·쓰기 상한을 따로(10-24 · 긴 질의를 기다리는 DB 드라이버 = ClickHouse HTTP) — `None` = `timeout`과 같다.
    pub io_timeout: Option<Duration>,
    /// 추가 헤더(`Authorization` · `If-None-Match` · `Accept` …).
    pub headers: Vec<(String, String)>,
    /// `http://host:port` — `None` = 환경 변수(`HTTPS_PROXY` > `https_proxy`) · 빈 문자열 = 프록시 없음(환경 무시).
    pub proxy: Option<String>,
    pub max_redirects: u32,
    /// 본문 상한(초과 = [`NetError::TooLarge`]).
    pub max_body: u64,
    pub user_agent: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            timeout: Duration::from_secs(30),
            io_timeout: None,
            headers: Vec::new(),
            proxy: None,
            max_redirects: 5,
            max_body: 1 << 30,
            user_agent: format!("nsql/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl Options {
    fn effective_proxy(&self) -> Option<String> {
        match &self.proxy {
            Some(p) if p.is_empty() => None,
            Some(p) => Some(p.clone()),
            None => ["HTTPS_PROXY", "https_proxy"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty())),
        }
    }
}

/// 응답 머리(본문은 호출자의 sink로 흘렀다).
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    /// 이름은 소문자.
    pub headers: Vec<(String, String)>,
    /// 리다이렉트를 따라간 뒤의 최종 URL.
    pub url: String,
}

impl Response {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn content_length(&self) -> Option<u64> {
        self.header("content-length")
            .and_then(|v| v.trim().parse().ok())
    }
}

enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap_or_else(|e| panic!("rustls protocol versions: {e}"))
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

fn tcp_connect(
    host: &str,
    port: u16,
    timeout: Duration,
    io: Duration,
) -> Result<TcpStream, NetError> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| NetError::Io(io::Error::new(e.kind(), format!("resolve {host}: {e}"))))?;
    let mut last = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, timeout) {
            Ok(s) => {
                s.set_read_timeout(Some(io))?;
                s.set_write_timeout(Some(io))?;
                s.set_nodelay(true)?;
                return Ok(s);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(NetError::Io(last.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("no address for {host}"))
    })))
}

/// ★ 공개 소켓(10-25 · D-361 · 네이티브 DB 드라이버용): 평문 TCP 또는 TLS(rustls · 시스템 루트) 위 `Read + Write` 하나 —
///   DRDA(Db2) 등 HTTP가 아닌 프로토콜이 쓴다. 접속·읽기·쓰기 타임아웃 · Nagle 끔. 스스로 재시도하지 않는다(26 §8).
pub struct Socket(Conn);

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            Conn::Plain(_) => "Socket(plain)",
            Conn::Tls(_) => "Socket(tls)",
        })
    }
}

impl Socket {
    /// `tls` = `Some(서버 이름)`이면 TLS 핸드셰이크까지(인증서 = 시스템 루트 · 이름 검증).
    pub fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        io: Duration,
        tls: Option<&str>,
    ) -> Result<Socket, NetError> {
        let tcp = tcp_connect(host, port, timeout, io)?;
        match tls {
            None => Ok(Socket(Conn::Plain(tcp))),
            Some(name) => {
                let sn = rustls::pki_types::ServerName::try_from(name.to_string())
                    .map_err(|e| NetError::Tls(format!("server name {name}: {e}")))?;
                let cc = rustls::ClientConnection::new(tls_config(), sn)
                    .map_err(|e| NetError::Tls(e.to_string()))?;
                Ok(Socket(Conn::Tls(Box::new(rustls::StreamOwned::new(
                    cc, tcp,
                )))))
            }
        }
    }

    /// 아래 TCP 소켓의 복제 핸들(취소용 · 다른 스레드에서 `shutdown`하면 막힌 읽기가 오류로 끝난다 · TLS여도 평문 소켓을 끊는다).
    pub fn tcp_clone(&self) -> io::Result<TcpStream> {
        match &self.0 {
            Conn::Plain(s) => s.try_clone(),
            Conn::Tls(s) => s.sock.try_clone(),
        }
    }

    /// 읽기 타임아웃을 바꾼다(긴 질의 대기 · `None` = 무한).
    pub fn set_read_timeout(&mut self, d: Option<Duration>) -> io::Result<()> {
        match &mut self.0 {
            Conn::Plain(s) => s.set_read_timeout(d),
            Conn::Tls(s) => s.sock.set_read_timeout(d),
        }
    }
}

impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// 머리(상태줄 + 헤더)를 `\r\n\r\n`까지 읽는다 — 남은 바이트(본문 앞부분)를 돌려준다.
/// (상태, 헤더, 머리 뒤에 이미 읽힌 본문 앞부분).
type Head = (u16, Vec<(String, String)>, Vec<u8>);

fn read_head(r: &mut dyn Read) -> Result<Head, NetError> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(i) = find_crlfcrlf(&buf) {
            break i;
        }
        if buf.len() > 64 * 1024 {
            return Err(NetError::Http("header block too large".into()));
        }
        let n = r.read(&mut chunk)?;
        if n == 0 {
            return Err(NetError::Http("connection closed before headers".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let rest = buf[end + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .strip_prefix("HTTP/1.")
        .and_then(|s| s.get(2..5))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| NetError::Http(format!("bad status line: {status_line:?}")))?;
    let mut headers = Vec::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Ok((status, headers, rest))
}

fn find_crlfcrlf(b: &[u8]) -> Option<usize> {
    b.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 머리를 읽은 뒤 남은 바이트 + 연결에서 본문을 잇는 읽기.
struct Body<'a> {
    conn: &'a mut Conn,
    pending: Vec<u8>,
    pos: usize,
}

impl Read for Body<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos < self.pending.len() {
            let n = (self.pending.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        self.conn.read(buf)
    }
}

impl Body<'_> {
    fn read_exact_or_eof(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut got = 0;
        while got < buf.len() {
            let n = self.read(&mut buf[got..])?;
            if n == 0 {
                break;
            }
            got += n;
        }
        Ok(got)
    }

    fn read_line(&mut self) -> Result<String, NetError> {
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        loop {
            if self.read(&mut b)? == 0 {
                return Err(NetError::Http(
                    "connection closed inside chunked body".into(),
                ));
            }
            if b[0] == b'\n' {
                break;
            }
            line.push(b[0]);
            if line.len() > 8192 {
                return Err(NetError::Http("chunk line too long".into()));
            }
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Ok(String::from_utf8_lossy(&line).into_owned())
    }
}

fn copy_limited(
    body: &mut Body<'_>,
    total: Option<u64>,
    sink: &mut dyn Write,
    max: u64,
    received: &mut u64,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), NetError> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut left = total;
    loop {
        let want = match left {
            Some(0) => break,
            Some(n) => (n.min(buf.len() as u64)) as usize,
            None => buf.len(),
        };
        let n = body.read(&mut buf[..want])?;
        if n == 0 {
            if left.is_some() {
                return Err(NetError::Http("body shorter than Content-Length".into()));
            }
            break;
        }
        *received += n as u64;
        if *received > max {
            return Err(NetError::TooLarge(max));
        }
        sink.write_all(&buf[..n])?;
        if let Some(l) = left.as_mut() {
            *l -= n as u64;
        }
        progress(*received, total);
    }
    Ok(())
}

fn connect(u: &Url, o: &Options) -> Result<Conn, NetError> {
    let proxy = o.effective_proxy();
    let tcp = match &proxy {
        Some(p) => {
            let pu = parse_url(p).map_err(|e| NetError::Proxy(e.to_string()))?;
            let mut s = tcp_connect(
                &pu.host,
                pu.port,
                o.timeout,
                o.io_timeout.unwrap_or(o.timeout),
            )?;
            if u.https {
                let req = format!(
                    "CONNECT {h}:{p} HTTP/1.1\r\nHost: {h}:{p}\r\nUser-Agent: {ua}\r\n\r\n",
                    h = u.host,
                    p = u.port,
                    ua = o.user_agent
                );
                s.write_all(req.as_bytes())?;
                let (status, _, _) = read_head(&mut s)?;
                if status != 200 {
                    return Err(NetError::Proxy(format!("CONNECT returned {status}")));
                }
            }
            s
        }
        None => tcp_connect(
            &u.host,
            u.port,
            o.timeout,
            o.io_timeout.unwrap_or(o.timeout),
        )?,
    };
    if !u.https {
        return Ok(Conn::Plain(tcp));
    }
    let name = rustls::pki_types::ServerName::try_from(u.host.clone())
        .map_err(|e| NetError::Tls(format!("server name {}: {e}", u.host)))?;
    let conn = rustls::ClientConnection::new(tls_config(), name)
        .map_err(|e| NetError::Tls(e.to_string()))?;
    Ok(Conn::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

/// GET 한 번(리다이렉트 포함) — 본문은 `sink`로, 진행은 `progress(받은 바이트, 전체(알면))`.
pub fn get(
    url: &str,
    o: &Options,
    sink: &mut dyn Write,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Response, NetError> {
    request("GET", url, None, o, sink, progress)
}

/// ★ POST 한 번(10-24 · ClickHouse HTTP 드라이버) — `body`를 `Content-Length`로 보낸다 · 그 밖은 [`get`]과 같다(리다이렉트 때 같은 본문을 다시 보냄).
///   `Content-Type`은 호출자가 `headers`로.
pub fn post(
    url: &str,
    body: &[u8],
    o: &Options,
    sink: &mut dyn Write,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Response, NetError> {
    request("POST", url, Some(body), o, sink, progress)
}

/// POST → 바이트(작은 응답용).
pub fn post_bytes(url: &str, body: &[u8], o: &Options) -> Result<(Response, Vec<u8>), NetError> {
    let mut out = Vec::new();
    let r = post(url, body, o, &mut out, &mut |_, _| {})?;
    Ok((r, out))
}

/// 한 요청(리다이렉트 포함) — `method` + 선택 본문.
pub fn request(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    o: &Options,
    sink: &mut dyn Write,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Response, NetError> {
    let mut u = parse_url(url)?;
    let mut headers = o.headers.clone();
    let via_proxy_plain = o.effective_proxy().is_some();
    for _ in 0..=o.max_redirects {
        let mut conn = connect(&u, o)?;
        let target = if via_proxy_plain && !u.https {
            u.to_string_url()
        } else {
            u.path.clone()
        };
        let mut req = format!(
            "{method} {target} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n",
            u.host_header(),
            o.user_agent
        );
        for (k, v) in &headers {
            req.push_str(k);
            req.push_str(": ");
            req.push_str(v);
            req.push_str("\r\n");
        }
        if let Some(b) = body {
            req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        }
        req.push_str("\r\n");
        conn.write_all(req.as_bytes())?;
        if let Some(b) = body {
            conn.write_all(b)?;
        }
        conn.flush()?;
        let (status, resp_headers, pending) = read_head(&mut conn)?;
        let resp = Response {
            status,
            headers: resp_headers,
            url: u.to_string_url(),
        };
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let Some(loc) = resp.header("location") else {
                return Err(NetError::Http(format!("{status} without Location")));
            };
            let next = u.join(loc)?;
            if next.host != u.host {
                headers.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization"));
            }
            u = next;
            continue;
        }
        let mut body = Body {
            conn: &mut conn,
            pending,
            pos: 0,
        };
        let mut received = 0u64;
        if matches!(status, 204 | 304) || (100..200).contains(&status) {
            return Ok(resp);
        }
        let chunked = resp
            .header("transfer-encoding")
            .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
        if chunked {
            loop {
                let line = body.read_line()?;
                let size_str = line.split(';').next().unwrap_or("").trim();
                let size = u64::from_str_radix(size_str, 16)
                    .map_err(|_| NetError::Http(format!("bad chunk size {size_str:?}")))?;
                if size == 0 {
                    // 트레일러(있으면) — 빈 줄까지.
                    loop {
                        if body.read_line()?.is_empty() {
                            break;
                        }
                    }
                    break;
                }
                copy_limited(
                    &mut body,
                    Some(size),
                    sink,
                    o.max_body,
                    &mut received,
                    progress,
                )?;
                let mut crlf = [0u8; 2];
                body.read_exact_or_eof(&mut crlf)?;
            }
        } else {
            let total = resp.content_length();
            copy_limited(&mut body, total, sink, o.max_body, &mut received, progress)?;
        }
        sink.flush()?;
        return Ok(resp);
    }
    Err(NetError::TooManyRedirects)
}

/// GET → 바이트(작은 응답용).
pub fn get_bytes(url: &str, o: &Options) -> Result<(Response, Vec<u8>), NetError> {
    let mut out = Vec::new();
    let r = get(url, o, &mut out, &mut |_, _| {})?;
    Ok((r, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn url_parse_and_join() {
        let u = parse_url("https://github.com/SosomLab/x/releases/latest/download/index.json")
            .expect("url");
        assert!(u.https);
        assert_eq!(u.port, 443);
        assert_eq!(u.host_header(), "github.com");
        assert_eq!(u.path, "/SosomLab/x/releases/latest/download/index.json");
        let v = parse_url("http://127.0.0.1:8080").expect("url");
        assert_eq!((v.https, v.port, v.path.as_str()), (false, 8080, "/"));
        assert_eq!(v.host_header(), "127.0.0.1:8080");
        assert!(parse_url("ftp://x").is_err());
        assert!(parse_url("https://user@x/").is_err());
        assert_eq!(u.join("/a/b").expect("join").path, "/a/b");
        assert_eq!(
            u.join("other.json").expect("join").path,
            "/SosomLab/x/releases/latest/download/other.json"
        );
        assert_eq!(
            u.join("https://objects.githubusercontent.com/q?x=1")
                .expect("join")
                .host,
            "objects.githubusercontent.com"
        );
        let six = parse_url("http://[::1]:9/p").expect("ipv6");
        assert_eq!((six.host.as_str(), six.port), ("::1", 9));
    }

    /// 로컬 평문 서버: 경로별 응답(content-length · chunked · 리다이렉트 · 304 · 닫힘 종료).
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = l.local_addr().expect("addr");
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut req = Vec::new();
                let mut b = [0u8; 1024];
                while find_crlfcrlf(&req).is_none() {
                    let n = s.read(&mut b).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    req.extend_from_slice(&b[..n]);
                }
                let line = String::from_utf8_lossy(&req);
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let auth = line.to_ascii_lowercase().contains("\r\nauthorization:");
                let resp = routes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, r)| r.clone())
                    .unwrap_or_else(|| {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                    });
                let resp = if path == "/auth" {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n{}",
                        if auth { "1" } else { "0" }
                    )
                    .into_bytes()
                } else {
                    resp
                };
                let _ = s.write_all(&resp);
                let _ = s.flush();
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn get_content_length_chunked_redirect_and_304() {
        let base = serve(vec![
            (
                "/len",
                b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nETag: \"e1\"\r\n\r\nhello".to_vec(),
            ),
            (
                "/chunk",
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\nX-Trailer: 1\r\n\r\n".to_vec(),
            ),
            (
                "/redir",
                b"HTTP/1.1 302 Found\r\nLocation: /len\r\nContent-Length: 0\r\n\r\n".to_vec(),
            ),
            (
                "/loop",
                b"HTTP/1.1 301 Moved\r\nLocation: loop\r\nContent-Length: 0\r\n\r\n".to_vec(),
            ),
            ("/nomod", b"HTTP/1.1 304 Not Modified\r\n\r\n".to_vec()),
            (
                "/eof",
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nuntil-close".to_vec(),
            ),
        ]);
        let o = Options {
            proxy: Some(String::new()),
            timeout: Duration::from_secs(5),
            ..Options::default()
        };
        let (r, b) = get_bytes(&format!("{base}/len"), &o).expect("len");
        assert_eq!((r.status, b.as_slice()), (200, &b"hello"[..]));
        assert_eq!(r.header("etag"), Some("\"e1\""));
        let (r, b) = get_bytes(&format!("{base}/chunk"), &o).expect("chunk");
        assert_eq!(
            (r.status, String::from_utf8_lossy(&b).as_ref()),
            (200, "Wikipedia")
        );
        let (r, b) = get_bytes(&format!("{base}/redir"), &o).expect("redir");
        assert_eq!((r.status, b.as_slice()), (200, &b"hello"[..]));
        assert!(r.url.ends_with("/len"));
        assert!(matches!(
            get_bytes(&format!("{base}/loop"), &o),
            Err(NetError::TooManyRedirects)
        ));
        let (r, b) = get_bytes(&format!("{base}/nomod"), &o).expect("304");
        assert_eq!((r.status, b.len()), (304, 0));
        let (r, b) = get_bytes(&format!("{base}/eof"), &o).expect("eof");
        assert_eq!((r.status, b.as_slice()), (200, &b"until-close"[..]));
        let (r, _) = get_bytes(&format!("{base}/missing"), &o).expect("404");
        assert_eq!(r.status, 404);
        // 본문 상한.
        let small = Options {
            max_body: 3,
            ..o.clone()
        };
        assert!(matches!(
            get_bytes(&format!("{base}/len"), &small),
            Err(NetError::TooLarge(3))
        ));
        // 같은 호스트 리다이렉트는 Authorization을 유지한다.
        let auth = Options {
            headers: vec![("Authorization".into(), "Bearer t".into())],
            ..o
        };
        let (_, b) = get_bytes(&format!("{base}/auth"), &auth).expect("auth");
        assert_eq!(b, b"1");
    }
}
