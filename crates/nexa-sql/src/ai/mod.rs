//! ★ AI 어시스턴트(docs/110 · 사용자 11-10/11-11 "Claude 탭 — API · 다중 API · CLI 직접 모두") — 핵심 모델.
//!
//! - 모드 = `Api`(Messages API 직접 · 프로필의 API 키 = 볼트 봉투) · `Cli`(설치된 `claude` 실행 파일을 자식 프로세스로 · stream-json).
//! - 프로필 = 다중 API 등록·전환(`NSQL_HOME/ai/profiles.conf` · 키는 절대 평문으로 두지 않고 볼트 `ai.<이름>` 봉투).
//! - 세션·메시지·사용량 · 호스트 → UI 이벤트([`AiEvent`]) · 가림 필터([`redact`] · [`contains_secret`]) · 이력(`NSQL_HOME/ai/history/<id>.jsonl`).
//! - 안전 경계(110 §7): 접속·실행·비밀은 호스트 · 플러그인/AI는 요청과 표시 · 쓰기 질의는 사용자 확인.

pub(crate) mod claude_api;
pub(crate) mod claude_cli;
pub(crate) mod codex;
pub(crate) mod host;
pub(crate) mod mcp;
pub(crate) mod panel;
pub(crate) mod picker;
pub(crate) mod search;
pub(crate) mod skills;
pub(crate) mod tools;
pub(crate) mod view;
pub(crate) mod wif;

use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};
use std::path::{Path, PathBuf};

/// 통합 방식.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Mode {
    #[default]
    Api,
    Cli,
}

impl Mode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Mode::Api => "api",
            Mode::Cli => "cli",
        }
    }
    pub(crate) fn parse(s: &str) -> Mode {
        if s.eq_ignore_ascii_case("cli") {
            Mode::Cli
        } else {
            Mode::Api
        }
    }
}

/// 상대(제공자) — Claude(API/CLI) · Codex(`codex app-server` + API 키 · docs/110 §15-3 ②).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Provider {
    #[default]
    Claude,
    Codex,
}

impl Provider {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
        }
    }
    pub(crate) fn parse(s: &str) -> Provider {
        if s.eq_ignore_ascii_case("codex") {
            Provider::Codex
        } else {
            Provider::Claude
        }
    }
    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Provider::Claude => DEFAULT_MODEL,
            Provider::Codex => DEFAULT_CODEX_MODEL,
        }
    }
    pub(crate) fn models(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => MODELS,
            Provider::Codex => CODEX_MODELS,
        }
    }
    /// 자동 모델 이름(가족).
    pub(crate) fn families(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => CLAUDE_FAMILIES,
            Provider::Codex => CODEX_FAMILIES,
        }
    }
    /// 탭·패널 제목(제품 이름 · 번역 없음).
    pub(crate) fn title(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        }
    }
    /// 활동 막대 항목 id(탭).
    pub(crate) fn act_id(self) -> &'static str {
        match self {
            Provider::Claude => "view.ai",
            Provider::Codex => "view.ai_codex",
        }
    }
    /// 채팅 뷰 탭 키.
    pub(crate) fn view_key(self) -> &'static str {
        match self {
            Provider::Claude => "ai.chat",
            Provider::Codex => "ai.codex",
        }
    }
}

/// 모델 선택 팝업의 설명 줄(내장 표 · 조회한 모델은 "제공자 목록에서").
pub(crate) fn model_desc(id: &str) -> nsql_i18n::Msg {
    use nsql_i18n::Msg;
    let m = id.to_ascii_lowercase();
    if m.contains("fable") || m.contains("mythos") {
        Msg::AiDescFable
    } else if m.contains("opus") {
        Msg::AiDescOpus
    } else if m.contains("sonnet") {
        Msg::AiDescSonnet
    } else if m.contains("haiku") {
        Msg::AiDescHaiku
    } else if m.contains("sol-mini") {
        Msg::AiDescSolMini
    } else if m.contains("gpt-6") {
        Msg::AiDescSol
    } else {
        Msg::AiDescFetched
    }
}

/// Codex 기본 모델(110 §13-2 표).
pub(crate) const DEFAULT_CODEX_MODEL: &str = "sol";
pub(crate) const CODEX_MODELS: &[&str] = &["gpt-6.1-sol", "gpt-6.1-sol-mini"];

/// 기본 모델(claude-api 스킬 기본 · docs/110 §10 ⑤).
pub(crate) const DEFAULT_MODEL: &str = "opus";
pub(crate) const DEFAULT_EFFORT: &str = "medium";
pub(crate) const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// 모델 선택지(설정 표 · Models API 조회는 2차).
pub(crate) const MODELS: &[&str] = &[
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5-1",
];
pub(crate) const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// ★ 자동 모델 이름(사용자 11-11 "바뀌지 않고 버전까지 자동 반영되는 이름"): 세션·프로필의 모델이 가족 이름이면 **보낼 때**
/// 프로필이 조회한 목록(없으면 내장 표)에서 가장 새 id로 푼다([`resolve_model`]).
pub(crate) const CLAUDE_FAMILIES: &[&str] = &["opus", "sonnet", "haiku", "fable"];
pub(crate) const CODEX_FAMILIES: &[&str] = &["sol", "sol-mini"];

pub(crate) fn is_family(p: Provider, name: &str) -> bool {
    p.families().contains(&name)
}

fn matches_family(id: &str, fam: &str) -> bool {
    let id = id.to_ascii_lowercase();
    match fam {
        "sol" => id.contains("sol") && !id.contains("mini"),
        "sol-mini" => id.contains("sol") && id.contains("mini"),
        f => id.contains(f),
    }
}

/// id 안의 숫자 묶음(버전 · 날짜) — 큰 쪽이 새 것. 같으면 짧은 id(날짜 없는 별칭) 우선.
fn version_key(id: &str) -> (Vec<u64>, std::cmp::Reverse<usize>) {
    let mut v = Vec::new();
    let mut cur: Option<u64> = None;
    for c in id.chars() {
        if let Some(d) = c.to_digit(10) {
            cur = Some(
                cur.unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(u64::from(d)),
            );
        } else if let Some(n) = cur.take() {
            v.push(n);
        }
    }
    if let Some(n) = cur {
        v.push(n);
    }
    (v, std::cmp::Reverse(id.len()))
}

/// 가족 이름 → 가장 새 id(조회 목록 우선 · 없으면 내장 표 · 그래도 없으면 이름 그대로). 가족 이름이 아니면 그대로.
pub(crate) fn resolve_model(p: Provider, name: &str, fetched: &[String]) -> String {
    if !is_family(p, name) {
        return name.to_string();
    }
    let pick = |ids: &[&str]| -> Option<String> {
        ids.iter()
            .filter(|id| matches_family(id, name))
            .max_by_key(|id| version_key(id))
            .map(|s| (*s).to_string())
    };
    let fetched: Vec<&str> = fetched.iter().map(String::as_str).collect();
    pick(&fetched)
        .or_else(|| pick(p.models()))
        .unwrap_or_else(|| name.to_string())
}

/// 모델 선택 항목(id · 설명): 자동 이름들(설명 = 지금 풀리는 id) → 조회/내장 id들 · 현재 값은 항상 포함.
pub(crate) fn model_items(p: Provider, fetched: &[String], current: &str) -> Vec<(String, String)> {
    use nsql_i18n::{t, tf, Msg};
    let mut out: Vec<(String, String)> = p
        .families()
        .iter()
        .map(|f| {
            let now = resolve_model(p, f, fetched);
            ((*f).to_string(), tf(Msg::AiDescAuto, &[f, &now]))
        })
        .collect();
    let ids: Vec<String> = if fetched.is_empty() {
        p.models().iter().map(|m| (*m).to_string()).collect()
    } else {
        fetched.to_vec()
    };
    let builtin = p.models();
    for id in ids {
        let d = if builtin.contains(&id.as_str()) {
            t(model_desc(&id)).to_string()
        } else {
            t(Msg::AiDescFetched).to_string()
        };
        out.push((id, d));
    }
    if !current.is_empty() && !out.iter().any(|(id, _)| id == current) {
        out.insert(0, (current.to_string(), String::new()));
    }
    out
}

/// 노력도 선택 항목(값 · 설명).
pub(crate) fn effort_items() -> Vec<(String, String)> {
    use nsql_i18n::{t, Msg};
    EFFORTS
        .iter()
        .map(|e| {
            let d = match *e {
                "low" => Msg::AiDescEffortLow,
                "medium" => Msg::AiDescEffortMedium,
                "high" => Msg::AiDescEffortHigh,
                "xhigh" => Msg::AiDescEffortXhigh,
                _ => Msg::AiDescEffortMax,
            };
            ((*e).to_string(), t(d).to_string())
        })
        .collect()
}

/// API 프로필(다중 등록 · 110 §14-3) — 키는 여기 없다(볼트 `ai.<이름>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    pub name: String,
    /// 제공자(프로필은 제공자별 · 이름은 전체에서 유일 · 볼트 비밀 이름 `ai.<이름>`).
    pub provider: Provider,
    /// Anthropic 호환 엔드포인트(기본 api.anthropic.com · 게이트웨이/프록시 가능).
    pub base_url: String,
    /// 워크스페이스 미지정 키면 필요(`anthropic-workspace-id`).
    pub workspace: String,
    pub model: String,
    pub effort: String,
    /// 조회한 모델 목록([검증] · Claude `/v1/models` · Codex `model/list` · 110 §15-5) — 비면 제공자 내장 표.
    pub models: Vec<String>,
    /// ★ 인증 = ID 페더레이션(WIF · 110 §14-5 · §15-5) — `Some`이면 API 키 대신 IdP 로그인 토큰(볼트 봉투 `ai.<이름>` = 토큰 JSON).
    pub wif: Option<wif::Wif>,
}

impl Profile {
    pub(crate) fn new(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            provider: Provider::Claude,
            base_url: DEFAULT_BASE_URL.to_string(),
            workspace: String::new(),
            model: DEFAULT_MODEL.to_string(),
            effort: DEFAULT_EFFORT.to_string(),
            models: Vec::new(),
            wif: None,
        }
    }
    /// 볼트 비밀 이름.
    pub(crate) fn secret_name(&self) -> String {
        format!("ai.{}", self.name)
    }
}

/// 프로필 이름 규칙 = 볼트 비밀 이름 규칙(`ai.<이름>` ≤ 64 → 이름 ≤ 61 · 영문·숫자·`_ - .` · `.`로 시작 금지) — 저장 전 폼에서 점검(사용자 11-11).
pub(crate) fn profile_name_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 61
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !s.starts_with('.')
}

/// 프로필 저장소 — `NSQL_HOME/ai/profiles.conf`(key=value 블록 · 빈 줄로 구분 · `current=`는 머리).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProfileStore {
    pub profiles: Vec<Profile>,
    pub current: String,
}

impl ProfileStore {
    pub(crate) fn dir() -> Option<PathBuf> {
        nsql_settings::config_dir().map(|d| d.join("ai"))
    }

    fn path(dir: &Path) -> PathBuf {
        dir.join("profiles.conf")
    }

    pub(crate) fn load(dir: &Path) -> ProfileStore {
        let Ok(text) = std::fs::read_to_string(Self::path(dir)) else {
            return ProfileStore::default();
        };
        Self::parse(&text)
    }

    pub(crate) fn parse(text: &str) -> ProfileStore {
        let mut st = ProfileStore::default();
        let mut cur: Option<Profile> = None;
        let flush = |cur: &mut Option<Profile>, st: &mut ProfileStore| {
            if let Some(p) = cur.take() {
                if !p.name.is_empty() {
                    st.profiles.push(p);
                }
            }
        };
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                flush(&mut cur, &mut st);
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "current" if cur.is_none() => st.current = v.to_string(),
                "name" => {
                    flush(&mut cur, &mut st);
                    cur = Some(Profile::new(v));
                }
                "base_url" => {
                    if let Some(p) = cur.as_mut() {
                        p.base_url = v.to_string();
                    }
                }
                "workspace" => {
                    if let Some(p) = cur.as_mut() {
                        p.workspace = v.to_string();
                    }
                }
                "model" => {
                    if let Some(p) = cur.as_mut() {
                        p.model = v.to_string();
                    }
                }
                "effort" => {
                    if let Some(p) = cur.as_mut() {
                        p.effort = v.to_string();
                    }
                }
                "provider" => {
                    if let Some(p) = cur.as_mut() {
                        p.provider = Provider::parse(v);
                    }
                }
                "models" => {
                    if let Some(p) = cur.as_mut() {
                        p.models = v
                            .split(',')
                            .map(str::trim)
                            .filter(|m| !m.is_empty())
                            .map(String::from)
                            .collect();
                    }
                }
                "auth" => {
                    if let Some(p) = cur.as_mut() {
                        if v.eq_ignore_ascii_case("wif") && p.wif.is_none() {
                            p.wif = Some(wif::Wif::default());
                        }
                    }
                }
                "wif_issuer" | "wif_client_id" | "wif_rule" | "wif_org" | "wif_sa"
                | "wif_scope" => {
                    if let Some(p) = cur.as_mut() {
                        let w = p.wif.get_or_insert_with(wif::Wif::default);
                        let slot = match k {
                            "wif_issuer" => &mut w.issuer,
                            "wif_client_id" => &mut w.client_id,
                            "wif_rule" => &mut w.rule_id,
                            "wif_org" => &mut w.org_id,
                            "wif_sa" => &mut w.service_account,
                            _ => &mut w.scope,
                        };
                        *slot = v.to_string();
                    }
                }
                _ => {}
            }
        }
        flush(&mut cur, &mut st);
        if st.current.is_empty() {
            if let Some(p) = st.profiles.first() {
                st.current = p.name.clone();
            }
        }
        st
    }

    pub(crate) fn dump(&self) -> String {
        let mut out = format!("current={}\n", self.current);
        for p in &self.profiles {
            out.push('\n');
            out.push_str(&format!(
                "name={}\nprovider={}\nbase_url={}\nworkspace={}\nmodel={}\neffort={}\nmodels={}\n",
                p.name,
                p.provider.as_str(),
                p.base_url,
                p.workspace,
                p.model,
                p.effort,
                p.models.join(",")
            ));
            if let Some(w) = &p.wif {
                out.push_str(&format!(
                    "auth=wif\nwif_issuer={}\nwif_client_id={}\nwif_rule={}\nwif_org={}\nwif_sa={}\nwif_scope={}\n",
                    w.issuer, w.client_id, w.rule_id, w.org_id, w.service_account, w.scope
                ));
            }
        }
        out
    }

    pub(crate) fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = Self::path(dir);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.dump())?;
        std::fs::rename(tmp, path)
    }

    pub(crate) fn current(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == self.current)
    }

    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.name == name)
    }

    /// 넣거나 바꾼다(이름 기준).
    pub(crate) fn upsert(&mut self, p: Profile) {
        match self.profiles.iter_mut().find(|q| q.name == p.name) {
            Some(q) => *q = p,
            None => self.profiles.push(p),
        }
        if self.current.is_empty() {
            self.current = self.profiles[0].name.clone();
        }
    }

    /// 이름 바꾸기(프로필 · 현재 표시) — 볼트 비밀·세션 참조는 호출자가 옮긴다.
    pub(crate) fn rename(&mut self, old: &str, new: &str) {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.name == old) {
            p.name = new.to_string();
        }
        if self.current == old {
            self.current = new.to_string();
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.current == name {
            self.current = self
                .profiles
                .first()
                .map(|p| p.name.clone())
                .unwrap_or_default();
        }
    }
}

/// 대화 한 조각의 종류.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    /// 사용자 글.
    User,
    /// 어시스턴트 글(스트리밍 중 덧붙는다).
    Assistant,
    /// 도구 호출(이름 · 입력 요약).
    ToolCall { name: String, input: String },
    /// 도구 결과(이름 · 출력 요약 · 성공).
    ToolResult { name: String, ok: bool },
    /// 승인 요청(도구 · 설명) — 카드로 그린다.
    Approval { tool: String },
    /// 오류.
    Error,
    /// 안내(시스템 · 모드 바뀜 · 비용 등).
    Note,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChatMsg {
    pub kind: Kind,
    pub text: String,
    /// 도구 호출·승인의 id(CLI tool_use id · API tool_use id).
    pub id: String,
    /// 첨부 요약(사용자 글 아래 칩).
    pub attachments: Vec<String>,
    /// ★ 코드 블록 [실행] 결과(블록 번호(메시지 안) · 글 · 성공) — 화면에만(이력 저장 안 함 · 사용자 11-11).
    pub block_results: Vec<(usize, String, bool)>,
}

impl ChatMsg {
    pub(crate) fn new(kind: Kind, text: impl Into<String>) -> ChatMsg {
        ChatMsg {
            kind,
            text: text.into(),
            id: String::new(),
            attachments: Vec::new(),
            block_results: Vec::new(),
        }
    }
}

/// 토큰 사용량 누계(비용은 추정치 · 110 §5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost_usd: f64,
}

impl Usage {
    pub(crate) fn add(&mut self, o: Usage) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.cost_usd += o.cost_usd;
    }
    /// ★ 비용 추정 — 표 = 설정 `ai.prices`(기본 `nsql_settings::AI_PRICES_DEFAULT` · 사용자 편집 가능 · 110 §15-5) · 캐시 읽기 = 입력의 1/20 근사.
    pub(crate) fn estimate_cost_in(
        table: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
    ) -> f64 {
        let (i, o) = price_per_m_in(table, model);
        (input as f64 * i + output as f64 * o + cache_read as f64 * i * 0.05) / 1_000_000.0
    }
}

/// 비용 표 한 줄 해석 — `<모델 부분 문자열>=<입력>/<출력>`(USD/1M) · `;` 또는 줄 바꿈 구분 · 앞에서부터 첫 일치 ·
/// `*` = 그 밖(없으면 opus-5-5 요율) · 모르는 항목은 건너뛴다(설정 오타에 관대).
pub(crate) fn price_per_m_in(table: &str, model: &str) -> (f64, f64) {
    let m = model.to_ascii_lowercase();
    let mut fallback = None;
    for item in table.split([';', '\n']) {
        let Some((pat, io)) = item.split_once('=') else {
            continue;
        };
        let Some((i, o)) = io.split_once('/') else {
            continue;
        };
        let (Ok(i), Ok(o)) = (i.trim().parse::<f64>(), o.trim().parse::<f64>()) else {
            continue;
        };
        let pat = pat.trim().to_ascii_lowercase();
        if pat == "*" {
            if fallback.is_none() {
                fallback = Some((i, o));
            }
        } else if !pat.is_empty() && m.contains(&pat) {
            return (i, o);
        }
    }
    fallback.unwrap_or((4.0, 20.0))
}

/// 첨부(입력 데이터 · 110 §14-4).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Attachment {
    /// 글(파일 · 클립보드 · 테이블 조회 결과) — 라벨 · 본문.
    Text { label: String, body: String },
    /// 이미지(PNG/JPEG 바이트 · 미디어 타입).
    Image {
        label: String,
        media: String,
        bytes: Vec<u8>,
    },
    /// PDF 바이트.
    Pdf { label: String, bytes: Vec<u8> },
}

impl Attachment {
    pub(crate) fn label(&self) -> &str {
        match self {
            Attachment::Text { label, .. }
            | Attachment::Image { label, .. }
            | Attachment::Pdf { label, .. } => label,
        }
    }
    /// 칩 글(사용자 11-11 "이미지처럼 상단에 표시"): 이미지 = 이름 W×H · PDF = 이름 (KB) · 글 = 이름(줄 수는 라벨에).
    pub(crate) fn chip_text(&self) -> String {
        match self {
            Attachment::Text { label, .. } => label.clone(),
            Attachment::Image {
                label,
                media,
                bytes,
            } => match image_dims(media, bytes) {
                Some((w, h)) => format!("{label} {w}×{h}"),
                // 용량은 칩에 안 보인다(미리보기 아래 줄에 · 사용자 11-11).
                None => label.clone(),
            },
            Attachment::Pdf { label, bytes } => format!("{label} ({} KB)", bytes.len() / 1024),
        }
    }
}

/// 이미지 머리에서 치수(PNG IHDR · GIF 논리 화면 · JPEG SOF) — 미리보기 칩·툴팁용 · 모르면 None.
pub(crate) fn image_dims(media: &str, b: &[u8]) -> Option<(u32, u32)> {
    let be32 = |o: usize| {
        b.get(o..o + 4)
            .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
    };
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some((be32(16)?, be32(20)?));
    }
    if b.starts_with(b"GIF8") {
        return Some((
            u32::from(u16::from_le_bytes([*b.get(6)?, *b.get(7)?])),
            u32::from(u16::from_le_bytes([*b.get(8)?, *b.get(9)?])),
        ));
    }
    if media.contains("jpeg") || b.starts_with(&[0xFF, 0xD8]) {
        let mut p = 2usize;
        while p + 9 < b.len() {
            if b[p] != 0xFF {
                p += 1;
                continue;
            }
            let m = b[p + 1];
            if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
                p += 2;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([b[p + 2], b[p + 3]]));
            if matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([b[p + 5], b[p + 6]]);
                let w = u16::from_be_bytes([b[p + 7], b[p + 8]]);
                return Some((u32::from(w), u32::from(h)));
            }
            p += 2 + len;
        }
    }
    None
}

/// 세션 하나.
#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub id: String,
    pub title: String,
    pub provider: Provider,
    pub mode: Mode,
    pub profile: String,
    pub model: String,
    pub effort: String,
    pub auto: bool,
    pub messages: Vec<ChatMsg>,
    pub usage: Usage,
    /// CLI 세션 id(`--resume`) · API는 없음.
    pub cli_session: Option<String>,
    /// 지금 보내려고 모아 둔 첨부.
    pub attachments: Vec<Attachment>,
    /// 이 세션 동안 행 데이터 전송 허용(110 §7).
    pub rows_allowed: bool,
}

impl Session {
    pub(crate) fn new(mode: Mode, profile: &str, model: &str, effort: &str) -> Session {
        Session {
            id: stamp_id(),
            title: String::new(),
            provider: Provider::Claude,
            mode,
            profile: profile.to_string(),
            model: model.to_string(),
            effort: effort.to_string(),
            auto: false,
            messages: Vec::new(),
            usage: Usage::default(),
            cli_session: None,
            attachments: Vec::new(),
            rows_allowed: false,
        }
    }

    /// 제목 = 첫 사용자 글 앞 40자.
    pub(crate) fn title_or_default(&self) -> String {
        if !self.title.is_empty() {
            return redact(&self.title);
        }
        let t = self
            .messages
            .iter()
            .find(|m| m.kind == Kind::User)
            .map(|m| {
                // 여러 줄 첫 메시지는 한 줄로(줄 바꿈 → 공백 · 협업 V1 관찰 11-11).
                let flat: String = m.text.split_whitespace().collect::<Vec<_>>().join(" ");
                let t: String = flat.chars().take(40).collect();
                if flat.chars().count() > 40 {
                    format!("{t}…")
                } else {
                    t
                }
            })
            .unwrap_or_else(|| "(new)".to_string());
        redact(&t)
    }

    /// 이력 파일 한 줄씩(가림 뒤).
    pub(crate) fn to_jsonl(&self) -> String {
        let mut out = String::new();
        let head = Json::Obj(vec![
            ("id".into(), Json::Str(self.id.clone())),
            ("title".into(), Json::Str(self.title_or_default())),
            ("provider".into(), Json::Str(self.provider.as_str().into())),
            ("mode".into(), Json::Str(self.mode.as_str().into())),
            ("profile".into(), Json::Str(self.profile.clone())),
            ("model".into(), Json::Str(self.model.clone())),
            ("effort".into(), Json::Str(self.effort.clone())),
            (
                "cli_session".into(),
                Json::Str(self.cli_session.clone().unwrap_or_default()),
            ),
            ("input".into(), Json::Num(self.usage.input as f64)),
            ("output".into(), Json::Num(self.usage.output as f64)),
            ("cache_read".into(), Json::Num(self.usage.cache_read as f64)),
            ("cost_usd".into(), Json::Num(self.usage.cost_usd)),
        ]);
        out.push_str(&json_dump(&head));
        out.push('\n');
        for m in &self.messages {
            let (kind, name, ok) = match &m.kind {
                Kind::User => ("user", String::new(), true),
                Kind::Assistant => ("assistant", String::new(), true),
                Kind::ToolCall { name, .. } => ("tool_call", name.clone(), true),
                Kind::ToolResult { name, ok } => ("tool_result", name.clone(), *ok),
                Kind::Approval { tool } => ("approval", tool.clone(), true),
                Kind::Error => ("error", String::new(), false),
                Kind::Note => ("note", String::new(), true),
            };
            let row = Json::Obj(vec![
                ("kind".into(), Json::Str(kind.into())),
                ("name".into(), Json::Str(name)),
                ("ok".into(), Json::Bool(ok)),
                ("text".into(), Json::Str(redact(&m.text))),
                (
                    "attachments".into(),
                    Json::Arr(m.attachments.iter().map(|a| Json::Str(a.clone())).collect()),
                ),
            ]);
            out.push_str(&json_dump(&row));
            out.push('\n');
        }
        out
    }

    pub(crate) fn from_jsonl(text: &str) -> Option<Session> {
        let mut lines = text.lines();
        let head = json_parse(lines.next()?).ok()?;
        let s = |k: &str| {
            jget(&head, k)
                .and_then(|v| v.scalar_text())
                .unwrap_or_default()
        };
        let n = |k: &str| match jget(&head, k) {
            Some(Json::Num(n)) => *n,
            _ => 0.0,
        };
        let mut sess = Session::new(
            Mode::parse(&s("mode")),
            &s("profile"),
            &s("model"),
            &s("effort"),
        );
        sess.id = s("id");
        sess.provider = Provider::parse(&s("provider"));
        sess.title = s("title");
        let cli = s("cli_session");
        sess.cli_session = (!cli.is_empty()).then_some(cli);
        sess.usage = Usage {
            input: n("input") as u64,
            output: n("output") as u64,
            cache_read: n("cache_read") as u64,
            cache_write: 0,
            cost_usd: n("cost_usd"),
        };
        for line in lines {
            let Ok(row) = json_parse(line) else { continue };
            let g = |k: &str| {
                jget(&row, k)
                    .and_then(|v| v.scalar_text())
                    .unwrap_or_default()
            };
            let name = g("name");
            let ok = matches!(jget(&row, "ok"), Some(Json::Bool(true)));
            let kind = match g("kind").as_str() {
                "user" => Kind::User,
                "assistant" => Kind::Assistant,
                "tool_call" => Kind::ToolCall {
                    name,
                    input: String::new(),
                },
                "tool_result" => Kind::ToolResult { name, ok },
                "approval" => Kind::Approval { tool: name },
                "error" => Kind::Error,
                _ => Kind::Note,
            };
            let mut m = ChatMsg::new(kind, g("text"));
            if let Some(Json::Arr(a)) = jget(&row, "attachments") {
                m.attachments = a.iter().filter_map(Json::scalar_text).collect();
            }
            sess.messages.push(m);
        }
        Some(sess)
    }
}

/// JSON 객체에서 키 하나.
pub(crate) fn jget<'a>(v: &'a Json, key: &str) -> Option<&'a Json> {
    match v {
        Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

pub(crate) fn jstr(v: &Json, key: &str) -> String {
    jget(v, key)
        .and_then(|v| match v {
            Json::Str(s) => Some(s.clone()),
            other => other.scalar_text(),
        })
        .unwrap_or_default()
}

/// 세션 id = 시각(초 · 16진) + 프로세스 id 꼬리.
pub(crate) fn stamp_id() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{t:x}-{:x}", std::process::id() & 0xffff)
}

/// 호스트(자식 CLI · API 스레드) → UI 이벤트.
#[derive(Debug)]
pub(crate) enum AiEvent {
    /// 세션 시작(CLI 세션 id · 도구 수 · MCP 서버 수 요약).
    Started {
        cli_session: Option<String>,
        note: String,
    },
    /// 어시스턴트 글 조각.
    TextDelta(String),
    /// 도구 호출 시작(id · 이름 · 입력 JSON).
    ToolCall {
        id: String,
        name: String,
        input: String,
    },
    /// 도구 결과(id · 이름 · 출력 요약 · 성공).
    ToolResult {
        id: String,
        name: String,
        output: String,
        ok: bool,
    },
    /// ★ 호스트 스레드가 **앱 기능 도구**의 실행을 UI 스레드에 부탁(API 모드) — 결과를 `reply`로 돌려준다.
    ToolRequest {
        id: String,
        name: String,
        input: String,
        reply: std::sync::mpsc::Sender<ToolReply>,
    },
    /// 승인 요청(쓰기 도구 · CLI 권한 프롬프트) — UI가 카드로 묻고 `reply`로 답한다.
    Approval {
        id: String,
        tool: String,
        summary: String,
        reply: std::sync::mpsc::Sender<bool>,
    },
    /// 한 턴의 사용량(델타).
    Usage(Usage),
    /// 턴 끝(이유 · 소요 ms).
    Done {
        stop: String,
        ms: u64,
    },
    Error(String),
    Note(String),
}

/// 도구 실행 결과(UI → 호스트).
#[derive(Clone, Debug)]
pub(crate) struct ToolReply {
    pub output: String,
    pub ok: bool,
}

/// ★ 가림 필터(110 §5 · 보내기·표시·저장 전 모두): API 키 접두 · `user/pw@host` · `password=…` · `DEFINE *PASS* = '…'`.
pub(crate) fn redact(text: &str) -> String {
    use std::sync::OnceLock;
    static RX: OnceLock<Vec<(regex::Regex, &'static str)>> = OnceLock::new();
    let rx = RX.get_or_init(|| {
        vec![
            (
                regex::Regex::new(r"(?i)\bsk-[A-Za-z0-9_-]{8,}").expect("rx"),
                "sk-******",
            ),
            (
                regex::Regex::new(r"(?i)\b(password|passwd|pwd|secret|token)\s*[=:]\s*\S+")
                    .expect("rx"),
                "$1=******",
            ),
            (
                regex::Regex::new(r"(?i)\b([A-Za-z0-9_$#.]+)/([^@\s/]+)@").expect("rx"),
                "$1/******@",
            ),
        ]
    });
    let mut out = text.to_string();
    for (r, rep) in rx {
        out = r.replace_all(&out, *rep).into_owned();
    }
    out
}

/// 비밀이 든 글인가(전송 차단 판정 · API 키 접두 또는 접속 문자열 비밀번호).
pub(crate) fn contains_secret(text: &str) -> bool {
    redact(text) != text
}

/// ★ 첨부용 가림(사용자 11-11 "파일을 골랐더니 오류만 뜸"): 거부하지 않고 비밀만 `******`로 바꾼 글과 **가린 곳 수**를 돌려준다.
pub(crate) fn redact_count(text: &str) -> (String, usize) {
    let masked = redact(text);
    let n = masked.matches("******").count() - text.matches("******").count();
    (masked, n)
}

/// 이력 폴더.
pub(crate) fn history_dir() -> Option<PathBuf> {
    ProfileStore::dir().map(|d| d.join("history"))
}

pub(crate) fn history_save(sess: &Session) {
    let Some(dir) = history_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.jsonl", sess.id));
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, sess.to_jsonl()).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

pub(crate) fn history_delete(id: &str) {
    if let Some(dir) = history_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{id}.jsonl")));
    }
}

/// 저장된 세션 목록(최근 수정 순 · 상한).
pub(crate) fn history_list(limit: usize) -> Vec<Session> {
    let Some(dir) = history_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            Some((m, e.path()))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files
        .into_iter()
        .take(limit)
        .filter_map(|(_, p)| std::fs::read_to_string(p).ok())
        .filter_map(|t| Session::from_jsonl(&t))
        .collect()
}

/// 보존 기한 지난 이력 삭제(설정 `ai.history_keep_days` · 0 = 무제한).
pub(crate) fn history_prune(keep_days: u64) {
    if keep_days == 0 {
        return;
    }
    let Some(dir) = history_dir() else { return };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let limit = std::time::Duration::from_secs(keep_days * 86_400);
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        let Ok(m) = md.modified() else { continue };
        if m.elapsed().map(|d| d > limit).unwrap_or(false) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_table_first_match_and_fallback() {
        let d = nsql_settings::AI_PRICES_DEFAULT;
        assert_eq!(price_per_m_in(d, "claude-sonnet-5-5"), (2.0, 10.0));
        assert_eq!(price_per_m_in(d, "gpt-6.1-sol-mini"), (0.5, 2.5));
        assert_eq!(price_per_m_in(d, "claude-fable-5-1"), (10.0, 50.0));
        // 사용자 표: 앞 항목 우선 · 오타 항목 무시 · `*` 폴백 · 줄 바꿈 구분.
        let t = "opus=1/2\nbad\nx=y/z\n*=7/8";
        assert_eq!(price_per_m_in(t, "claude-opus-5-5"), (1.0, 2.0));
        assert_eq!(price_per_m_in(t, "unknown"), (7.0, 8.0));
        assert_eq!(price_per_m_in("", "unknown"), (4.0, 20.0));
        let c = Usage::estimate_cost_in(t, "claude-opus-5-5", 1_000_000, 1_000_000, 0);
        assert!((c - 3.0).abs() < 1e-9);
    }

    #[test]
    fn image_dims_png_gif_jpeg() {
        let png = crate::png::encode(3, 2, &[0; 6]);
        assert_eq!(image_dims("image/png", &png), Some((3, 2)));
        let gif = b"GIF89a\x10\x00\x08\x00";
        assert_eq!(image_dims("image/gif", gif), Some((16, 8)));
        // JPEG: SOI · APP0(길이 2) · SOF0(길이 11: 정밀도 8 · 높이 5 · 폭 7 · 성분 1 …)
        let jpg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x05, 0x00,
            0x07, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xD9,
        ];
        assert_eq!(image_dims("image/jpeg", &jpg), Some((7, 5)));
        assert_eq!(image_dims("image/webp", b"RIFF"), None);
        let a = Attachment::Image {
            label: "shot.png".into(),
            media: "image/png".into(),
            bytes: png,
        };
        assert_eq!(a.chip_text(), "shot.png 3×2");
    }

    #[test]
    fn family_names_resolve_to_newest() {
        let none: Vec<String> = Vec::new();
        assert_eq!(
            resolve_model(Provider::Claude, "opus", &none),
            "claude-opus-5-5"
        );
        assert_eq!(resolve_model(Provider::Codex, "sol", &none), "gpt-6.1-sol");
        assert_eq!(
            resolve_model(Provider::Codex, "sol-mini", &none),
            "gpt-6.1-sol-mini"
        );
        assert_eq!(
            resolve_model(Provider::Claude, "claude-haiku-5-5", &none),
            "claude-haiku-5-5"
        );
        let fetched: Vec<String> = [
            "claude-opus-4-1-20250805",
            "claude-opus-5-5-20260401",
            "claude-opus-5-5",
            "claude-opus-5-6",
            "claude-sonnet-5-5",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        assert_eq!(
            resolve_model(Provider::Claude, "opus", &fetched),
            "claude-opus-5-6"
        );
        assert_eq!(
            resolve_model(Provider::Claude, "sonnet", &fetched),
            "claude-sonnet-5-5"
        );
        // 조회 목록에 없는 가족은 내장 표로.
        assert_eq!(
            resolve_model(Provider::Claude, "haiku", &fetched),
            "claude-haiku-5-5"
        );
        let items = model_items(Provider::Claude, &fetched, "opus");
        assert_eq!(items[0].0, "opus");
        assert!(items[0].1.contains("claude-opus-5-6"));
        assert_eq!(items.len(), CLAUDE_FAMILIES.len() + fetched.len());
        assert_eq!(effort_items().len(), EFFORTS.len());
    }

    #[test]
    fn profile_name_rule_and_rename() {
        assert!(profile_name_ok("api1") && profile_name_ok("team-key.v2"));
        assert!(!profile_name_ok("api#12") && !profile_name_ok("") && !profile_name_ok(".x"));
        assert!(!profile_name_ok("한글") && !profile_name_ok(&"a".repeat(62)));
        let mut st = ProfileStore::default();
        st.upsert(Profile::new("a"));
        st.upsert(Profile::new("b"));
        st.rename("a", "a2");
        assert_eq!(st.current, "a2");
        assert_eq!(st.profiles[0].name, "a2");
        assert_eq!(st.profiles.len(), 2);
    }

    #[test]
    fn profile_models_roundtrip_and_choices() {
        let mut p = Profile::new("w");
        p.model = "claude-x".into();
        let items = model_items(Provider::Claude, &p.models, &p.model);
        assert_eq!(items[0].0, "claude-x");
        assert_eq!(items.len(), CLAUDE_FAMILIES.len() + MODELS.len() + 1);
        p.models = vec!["claude-x".into(), "claude-y".into()];
        let mut st = ProfileStore::default();
        st.upsert(p.clone());
        let back = ProfileStore::parse(&st.dump());
        assert_eq!(back.profiles[0].models, p.models);
        // WIF 프로필 왕복(토큰은 저장 파일에 없다).
        let mut q = Profile::new("corp");
        q.wif = Some(wif::Wif {
            issuer: "https://login.example/t".into(),
            client_id: "cid".into(),
            rule_id: "fdrl_1".into(),
            org_id: "org".into(),
            service_account: "svac_1".into(),
            scope: String::new(),
        });
        st.upsert(q.clone());
        let back = ProfileStore::parse(&st.dump());
        assert_eq!(back.profiles[1], q);
        assert!(back.profiles[0].wif.is_none());
    }

    #[test]
    fn profiles_roundtrip() {
        let mut st = ProfileStore::default();
        st.upsert(Profile::new("work"));
        let mut home = Profile::new("home");
        home.workspace = "wrkspc_1".into();
        home.model = "claude-sonnet-5-5".into();
        st.upsert(home);
        st.current = "home".into();
        let back = ProfileStore::parse(&st.dump());
        assert_eq!(back, st);
        assert_eq!(back.current().map(|p| p.name.as_str()), Some("home"));
        let mut st2 = back;
        st2.remove("home");
        assert_eq!(st2.current, "work");
        assert_eq!(Profile::new("x").secret_name(), "ai.x");
    }

    #[test]
    fn session_jsonl_roundtrip_redacts() {
        let mut s = Session::new(Mode::Cli, "work", DEFAULT_MODEL, "high");
        s.messages.push(ChatMsg::new(
            Kind::User,
            "connect scott/tiger@db1 and sk-ant-abcdefgh123",
        ));
        s.messages.push(ChatMsg::new(Kind::Assistant, "ok"));
        s.cli_session = Some("abc".into());
        s.usage.input = 10;
        let text = s.to_jsonl();
        assert!(
            !text.contains("tiger") && !text.contains("abcdefgh123"),
            "{text}"
        );
        let back = Session::from_jsonl(&text).expect("parse");
        assert_eq!(back.mode, Mode::Cli);
        assert_eq!(back.cli_session.as_deref(), Some("abc"));
        assert_eq!(back.messages.len(), 2);
        assert_eq!(back.usage.input, 10);
        assert!(back.title_or_default().starts_with("connect"));
    }

    #[test]
    fn redact_patterns() {
        assert_eq!(redact("key sk-ant-api03-ABCDEFGH"), "key sk-******");
        assert_eq!(redact("password=hunter2 x"), "password=****** x");
        assert_eq!(redact("scott/tiger@orcl"), "scott/******@orcl");
        assert!(!contains_secret("SELECT 1 FROM dual"));
        assert!(contains_secret("CONNECT app/pw@host"));
        assert_eq!(Mode::parse("CLI"), Mode::Cli);
        assert!(
            Usage::estimate_cost_in(
                nsql_settings::AI_PRICES_DEFAULT,
                "claude-opus-5-5",
                1_000_000,
                0,
                0
            ) > 3.9
        );
    }
}
