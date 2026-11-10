//! ★ Workload Identity Federation(docs/110 §14-5 · §15-5 · 11-11): 조직 IdP(OIDC)에 **브라우저 로그인**(PKCE S256 · localhost
//! 콜백) → ID 토큰(JWT) → `POST {base_url}/v1/oauth/token`(RFC 7523 jwt-bearer · **JSON 본문** · WIF 레퍼런스 확인 11-11) →
//! 단명 Anthropic 토큰(`sk-ant-oat01-…` · `Authorization: Bearer`). 토큰(refresh · id · access · 만료)은 볼트 봉투 `ai.<프로필>`에만
//! (JSON) · 로그·화면·덤프 금지. `jti` 1회용 규칙 때문에 만료마다 **IdP refresh_token으로 새 ID 토큰**을 받아 다시 교환한다(저장된
//! id_token 재사용 금지). 외부 crate 0 — HTTP = nsql-net · SHA-256 = nsql-drivers ext · 난수 = nsql-vault(getrandom) · JSON = nsql-settings.

use super::{jget, jstr};
use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const GRANT_JWT_BEARER: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
/// IdP 로그인 범위 기본값 — `offline_access`가 있어야 refresh_token이 온다(Entra ID · Okta).
pub(crate) const DEFAULT_SCOPE: &str = "openid offline_access";

/// 프로필의 WIF 설정(비밀 아님 · `profiles.conf`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Wif {
    /// OIDC 발급자 URL(`/.well-known/openid-configuration` 디스커버리).
    pub issuer: String,
    /// 공개 클라이언트 id(PKCE · 비밀 없음).
    pub client_id: String,
    /// `fdrl_…`.
    pub rule_id: String,
    /// 조직 UUID.
    pub org_id: String,
    /// `svac_…`.
    pub service_account: String,
    /// 비면 [`DEFAULT_SCOPE`].
    pub scope: String,
}

impl Wif {
    pub(crate) fn scope_or_default(&self) -> &str {
        if self.scope.trim().is_empty() {
            DEFAULT_SCOPE
        } else {
            self.scope.trim()
        }
    }
    pub(crate) fn complete(&self) -> bool {
        !(self.issuer.trim().is_empty()
            || self.client_id.trim().is_empty()
            || self.rule_id.trim().is_empty()
            || self.org_id.trim().is_empty()
            || self.service_account.trim().is_empty())
    }
}

/// 볼트에 봉인되는 토큰 묶음.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tokens {
    pub refresh_token: String,
    pub id_token: String,
    pub access_token: String,
    /// access_token 만료(unix 초).
    pub expires: u64,
}

impl Tokens {
    pub(crate) fn to_json(&self) -> String {
        json_dump(&Json::Obj(vec![
            ("kind".into(), Json::Str("wif".into())),
            (
                "refresh_token".into(),
                Json::Str(self.refresh_token.clone()),
            ),
            ("id_token".into(), Json::Str(self.id_token.clone())),
            ("access_token".into(), Json::Str(self.access_token.clone())),
            ("expires".into(), Json::Num(self.expires as f64)),
        ]))
    }
    pub(crate) fn from_json(text: &str) -> Option<Tokens> {
        let v = json_parse(text.trim()).ok()?;
        if jstr(&v, "kind") != "wif" {
            return None;
        }
        Some(Tokens {
            refresh_token: jstr(&v, "refresh_token"),
            id_token: jstr(&v, "id_token"),
            access_token: jstr(&v, "access_token"),
            expires: match jget(&v, "expires") {
                Some(Json::Num(n)) => *n as u64,
                _ => 0,
            },
        })
    }
    /// 60초 여유를 두고 아직 쓸 수 있나.
    pub(crate) fn access_valid(&self, now: u64) -> bool {
        !self.access_token.is_empty() && self.expires > now + 60
    }
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 볼트 봉투 `ai.<프로필>`에서 토큰 읽기(없거나 API 키 형식이면 `None`).
pub(crate) fn tokens_load(profile: &str) -> Option<Tokens> {
    let v = nsql_vault::Vault::open_default().ok()?;
    let b = v.get_secret(&format!("ai.{profile}"))?;
    Tokens::from_json(&String::from_utf8(b).ok()?)
}

pub(crate) fn tokens_save(profile: &str, t: &Tokens) -> Result<(), String> {
    nsql_vault::Vault::open_default()
        .and_then(|v| v.put_secret(&format!("ai.{profile}"), t.to_json().as_bytes()))
        .map_err(|e| e.to_string())
}

/// base64url(패딩 없음 · RFC 4648 §5).
pub(crate) fn base64url(bytes: &[u8]) -> String {
    super::claude_cli::base64(bytes)
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string()
}

/// PKCE S256 challenge.
pub(crate) fn challenge_of(verifier: &str) -> String {
    base64url(&nsql_drivers::ext::sha256::sha256(verifier.as_bytes()))
}

/// (verifier, challenge) — verifier = OS 난수 32B의 base64url(43자).
pub(crate) fn pkce() -> Result<(String, String), String> {
    let seed = nsql_vault::random_bytes(32).map_err(|e| e.to_string())?;
    let verifier = base64url(&seed);
    let challenge = challenge_of(&verifier);
    Ok((verifier, challenge))
}

pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub(crate) fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            match std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                Some(v) => {
                    out.push(v);
                    i += 3;
                }
                None => {
                    out.push(b'%');
                    i += 1;
                }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn query_params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

pub(crate) struct Discovery {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
}

fn http_opts(timeout_secs: u64) -> nsql_net::Options {
    nsql_net::Options {
        timeout: Duration::from_secs(timeout_secs),
        ..nsql_net::Options::default()
    }
}

/// 오류 본문 → 짧은 글(OAuth `error`/`error_description` · Anthropic `error.message`) · 토큰은 절대 안 들어간다(응답 본문이 아니라 오류 본문).
fn http_err(status: u16, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body).into_owned();
    let msg = json_parse(text.trim())
        .ok()
        .and_then(|v| {
            let e = jget(&v, "error")?;
            Some(match e {
                Json::Str(s) => format!("{s} {}", jstr(&v, "error_description")),
                _ => jstr(e, "message"),
            })
        })
        .filter(|m| !m.trim().is_empty())
        .unwrap_or(text);
    format!(
        "HTTP {status}: {}",
        msg.trim().chars().take(300).collect::<String>()
    )
}

/// OIDC 디스커버리.
pub(crate) fn discover(issuer: &str) -> Result<Discovery, String> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim().trim_end_matches('/')
    );
    let (resp, body) = nsql_net::get_bytes(&url, &http_opts(20)).map_err(|e| e.to_string())?;
    if !(200..300).contains(&resp.status) {
        return Err(http_err(resp.status, &body));
    }
    let v = json_parse(String::from_utf8_lossy(&body).trim())
        .map_err(|_| "discovery: invalid JSON".to_string())?;
    let d = Discovery {
        authorization_endpoint: jstr(&v, "authorization_endpoint"),
        token_endpoint: jstr(&v, "token_endpoint"),
    };
    if d.authorization_endpoint.is_empty() || d.token_endpoint.is_empty() {
        return Err("discovery: authorization_endpoint/token_endpoint missing".into());
    }
    Ok(d)
}

/// 인가 URL(response_type=code · PKCE S256).
pub(crate) fn auth_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect: &str,
    state: &str,
    challenge: &str,
    scope: &str,
) -> String {
    format!(
        "{authorization_endpoint}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        if authorization_endpoint.contains('?') { "&" } else { "?" },
        url_encode(client_id),
        url_encode(redirect),
        url_encode(scope),
        url_encode(state),
        url_encode(challenge)
    )
}

/// 콜백 요청 첫 줄 → `Ok(Some((code, state)))` · 질의 없는 경로(`/favicon.ico`) = `Ok(None)` · IdP 오류 = `Err`.
pub(crate) fn parse_callback(request_line: &str) -> Result<Option<(String, String)>, String> {
    let Some(path) = request_line.split_whitespace().nth(1) else {
        return Ok(None);
    };
    let Some((_, query)) = path.split_once('?') else {
        return Ok(None);
    };
    let ps = query_params(query);
    let get = |k: &str| ps.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    if let Some(e) = get("error") {
        return Err(format!(
            "{e}: {}",
            get("error_description").unwrap_or_default()
        ));
    }
    match (get("code"), get("state")) {
        (Some(c), Some(s)) if !c.is_empty() => Ok(Some((c, s))),
        _ => Ok(None),
    }
}

const DONE_HTML: &str = "<!doctype html><meta charset=\"utf-8\"><title>Nexa SQL</title><body style=\"font-family:sans-serif;padding:2em\"><h2>Signed in</h2><p>You can close this window and return to Nexa SQL.</p><p>로그인됐습니다. 이 창을 닫고 Nexa SQL로 돌아가세요.</p></body>";

fn respond(s: &mut TcpStream, status: u16, html: &str) {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let _ = write!(
        s,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = s.flush();
}

/// 콜백 한 번 기다리기(마감까지 · 100 ms 폴링 · 다른 경로 요청은 404로 보내고 계속).
fn wait_callback(listener: &TcpListener, deadline: Instant) -> Result<(String, String), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    loop {
        match listener.accept() {
            Ok((mut s, _)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                let first = text.lines().next().unwrap_or("");
                match parse_callback(first) {
                    Ok(Some(cs)) => {
                        respond(&mut s, 200, DONE_HTML);
                        return Ok(cs);
                    }
                    Ok(None) => respond(&mut s, 404, ""),
                    Err(e) => {
                        respond(&mut s, 200, "<!doctype html><meta charset=\"utf-8\"><body>Sign-in was not completed. Return to Nexa SQL.</body>");
                        return Err(e);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("sign-in timed out (no callback from the browser)".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// IdP 토큰 끝점에 form POST.
fn token_post(token_endpoint: &str, form: &[(&str, &str)]) -> Result<Json, String> {
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut o = http_opts(30);
    o.headers.push((
        "content-type".into(),
        "application/x-www-form-urlencoded".into(),
    ));
    o.headers.push(("accept".into(), "application/json".into()));
    let (resp, out) =
        nsql_net::post_bytes(token_endpoint, body.as_bytes(), &o).map_err(|e| e.to_string())?;
    if !(200..300).contains(&resp.status) {
        return Err(http_err(resp.status, &out));
    }
    json_parse(String::from_utf8_lossy(&out).trim()).map_err(|_| "token: invalid JSON".to_string())
}

pub(crate) fn exchange_code(
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect: &str,
) -> Result<Json, String> {
    token_post(
        token_endpoint,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ],
    )
}

pub(crate) fn refresh(
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
    scope: &str,
) -> Result<Json, String> {
    token_post(
        token_endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("scope", scope),
        ],
    )
}

/// Anthropic 교환 본문(WIF 레퍼런스 "Token exchange request" · JSON).
pub(crate) fn federate_body(cfg: &Wif, workspace: &str, id_token: &str) -> Json {
    let mut items = vec![
        ("grant_type".into(), Json::Str(GRANT_JWT_BEARER.into())),
        ("assertion".into(), Json::Str(id_token.into())),
        (
            "federation_rule_id".into(),
            Json::Str(cfg.rule_id.trim().into()),
        ),
        (
            "organization_id".into(),
            Json::Str(cfg.org_id.trim().into()),
        ),
        (
            "service_account_id".into(),
            Json::Str(cfg.service_account.trim().into()),
        ),
    ];
    if !workspace.trim().is_empty() {
        items.push(("workspace_id".into(), Json::Str(workspace.trim().into())));
    }
    Json::Obj(items)
}

/// `POST {base_url}/v1/oauth/token` → (access_token, expires_in 초).
pub(crate) fn federate(
    base_url: &str,
    cfg: &Wif,
    workspace: &str,
    id_token: &str,
) -> Result<(String, u64), String> {
    let url = format!("{}/v1/oauth/token", base_url.trim_end_matches('/'));
    let mut o = http_opts(30);
    o.headers
        .push(("content-type".into(), "application/json".into()));
    o.headers.push(("accept".into(), "application/json".into()));
    let payload = json_dump(&federate_body(cfg, workspace, id_token)).into_bytes();
    let (resp, out) = nsql_net::post_bytes(&url, &payload, &o).map_err(|e| e.to_string())?;
    if !(200..300).contains(&resp.status) {
        return Err(http_err(resp.status, &out));
    }
    let v = json_parse(String::from_utf8_lossy(&out).trim())
        .map_err(|_| "oauth/token: invalid JSON".to_string())?;
    let at = jstr(&v, "access_token");
    if at.is_empty() {
        return Err("oauth/token: no access_token".into());
    }
    let exp = match jget(&v, "expires_in") {
        Some(Json::Num(n)) if *n > 0.0 => *n as u64,
        _ => 3600,
    };
    Ok((at, exp))
}

/// ★ 유효한 Anthropic 토큰 확보(턴마다 · 스레드) — 만료면 IdP refresh → 새 ID 토큰 → 교환 → 볼트 저장. 로그인 전이면 오류.
pub(crate) fn ensure_access(
    cfg: &Wif,
    base_url: &str,
    workspace: &str,
    profile: &str,
) -> Result<String, String> {
    let mut t = tokens_load(profile)
        .ok_or_else(|| "not signed in — open Profiles and click Sign in".to_string())?;
    if t.access_valid(now_secs()) {
        return Ok(t.access_token);
    }
    if t.refresh_token.is_empty() {
        return Err("session expired — sign in again".into());
    }
    let d = discover(&cfg.issuer)?;
    let v = refresh(
        &d.token_endpoint,
        &cfg.client_id,
        &t.refresh_token,
        cfg.scope_or_default(),
    )?;
    let id = jstr(&v, "id_token");
    if id.is_empty() {
        return Err("IdP returned no id_token on refresh".into());
    }
    let rt = jstr(&v, "refresh_token");
    if !rt.is_empty() {
        t.refresh_token = rt;
    }
    t.id_token = id;
    let (at, exp) = federate(base_url, cfg, workspace, &t.id_token)?;
    t.access_token = at;
    t.expires = now_secs() + exp;
    tokens_save(profile, &t)?;
    Ok(t.access_token)
}

/// ★ 전체 로그인(스레드) — 디스커버리 → localhost 리스너 → PKCE → 브라우저(`open`) → 콜백 → 코드 교환 → Anthropic 교환 → 볼트.
pub(crate) fn login(
    cfg: &Wif,
    base_url: &str,
    workspace: &str,
    profile: &str,
    timeout_secs: u64,
    open: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    if !cfg.complete() {
        return Err(
            "WIF profile incomplete (issuer, client id, rule id, organization id, service account id)"
                .into(),
        );
    }
    let d = discover(&cfg.issuer)?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    // 공개 클라이언트의 루프백 리다이렉트는 `http://localhost:<아무 포트>`가 관례(Entra ID · Okta) — 브라우저는 ::1 거부 시 127.0.0.1로.
    let redirect = format!("http://localhost:{port}/callback");
    let (verifier, challenge) = pkce()?;
    let state = base64url(&nsql_vault::random_bytes(16).map_err(|e| e.to_string())?);
    let url = auth_url(
        &d.authorization_endpoint,
        &cfg.client_id,
        &redirect,
        &state,
        &challenge,
        cfg.scope_or_default(),
    );
    open(&url)?;
    let (code, got_state) = wait_callback(
        &listener,
        Instant::now() + Duration::from_secs(timeout_secs.max(30)),
    )?;
    if got_state != state {
        return Err("state mismatch (callback was not for this sign-in)".into());
    }
    let v = exchange_code(
        &d.token_endpoint,
        &cfg.client_id,
        &code,
        &verifier,
        &redirect,
    )?;
    let id = jstr(&v, "id_token");
    if id.is_empty() {
        return Err("IdP returned no id_token (is the 'openid' scope granted?)".into());
    }
    let (at, exp) = federate(base_url, cfg, workspace, &id)?;
    let t = Tokens {
        refresh_token: jstr(&v, "refresh_token"),
        id_token: id,
        access_token: at,
        expires: now_secs() + exp,
    };
    tokens_save(profile, &t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_rfc7636_vector() {
        // RFC 7636 부록 B.
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        let (v, c) = pkce().expect("os random");
        assert_eq!(v.len(), 43);
        assert_eq!(challenge_of(&v), c);
    }

    #[test]
    fn url_codec_and_query() {
        assert_eq!(url_encode("a b/ü~"), "a%20b%2F%C3%BC~");
        assert_eq!(url_decode("a%20b%2F%C3%BC+x%zz"), "a b/ü x%zz");
        let ps = query_params("code=ab%2Bc&state=s1&empty=");
        assert_eq!(ps[0], ("code".into(), "ab+c".into()));
        assert_eq!(ps[2], ("empty".into(), String::new()));
    }

    #[test]
    fn callback_and_auth_url() {
        assert_eq!(
            parse_callback("GET /callback?code=xyz&state=st HTTP/1.1"),
            Ok(Some(("xyz".into(), "st".into())))
        );
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1"), Ok(None));
        assert!(
            parse_callback("GET /callback?error=access_denied&error_description=no HTTP/1.1")
                .is_err()
        );
        let u = auth_url(
            "https://idp.example/authorize",
            "cid 1",
            "http://localhost:4321/callback",
            "st",
            "ch",
            DEFAULT_SCOPE,
        );
        assert!(u.starts_with("https://idp.example/authorize?response_type=code&client_id=cid%201&redirect_uri=http%3A%2F%2Flocalhost%3A4321%2Fcallback&scope=openid%20offline_access"));
        assert!(u.ends_with("&state=st&code_challenge=ch&code_challenge_method=S256"));
    }

    #[test]
    fn tokens_json_and_federate_body() {
        let t = Tokens {
            refresh_token: "r".into(),
            id_token: "i".into(),
            access_token: "sk-ant-oat01-x".into(),
            expires: 1_700_000_000,
        };
        let back = Tokens::from_json(&t.to_json()).expect("wif json");
        assert_eq!(back, t);
        assert!(Tokens::from_json("sk-ant-api03-plainkey").is_none());
        assert!(t.access_valid(1_700_000_000 - 61));
        assert!(!t.access_valid(1_700_000_000 - 60));
        let cfg = Wif {
            issuer: "https://login.example/x".into(),
            client_id: "c".into(),
            rule_id: "fdrl_1".into(),
            org_id: "org".into(),
            service_account: "svac_1".into(),
            scope: String::new(),
        };
        assert!(cfg.complete());
        assert_eq!(cfg.scope_or_default(), DEFAULT_SCOPE);
        let b = json_dump(&federate_body(&cfg, "wrkspc_1", "jwt"));
        assert!(b.contains("\"grant_type\":\"urn:ietf:params:oauth:grant-type:jwt-bearer\""));
        assert!(b.contains("\"assertion\":\"jwt\"") && b.contains("\"workspace_id\":\"wrkspc_1\""));
        assert!(!json_dump(&federate_body(&cfg, "", "jwt")).contains("workspace_id"));
    }
}
