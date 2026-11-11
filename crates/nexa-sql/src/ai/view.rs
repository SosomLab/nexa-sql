//! ★ Claude 채팅 뷰 탭(docs/110 §14-2 · 사용자 11-11 "하단은 VS Code Claude Code 확장 화면처럼"): 편집기 자리에 그리는 대화 화면 —
//! 스트리밍 대본(글 · 코드 블록[복사·편집기에 넣기] · 도구 호출/결과 · 승인 카드[허용/거부] · 오류) · 아래 작성 상자(Enter = 보내기 ·
//! Shift+Enter = 줄 바꿈 · `/` 명령 팝업) · 첨부 칩(파일 · 클립보드 · 조회 결과) · [보내기]/[■]. 두 번째 페이지 = **프로필 관리**
//! (다중 API 프로필 · 키는 마스킹 입력 → 볼트 · [검증] = `/v1/models`).

use super::picker::{PickEvent, PickItem, Picker};
use super::wif::Wif;
use super::{ChatMsg, Kind, Mode, Profile, Provider, Session};
use nexa_ctl::controls::ctxmenu::MenuIcon;
use nexa_ctl::controls::{ContextMenu, CtxItem};
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{Color, IconImage, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Page {
    #[default]
    Chat,
    /// 프로필 **목록**(사용자 11-11 "목록과 편집 분리").
    Profiles,
    /// 프로필 **상세/편집**(목록에서 행·[편집]·[+]·[복제]로 들어오고 [←]로만 돌아간다 · 저장해도 머문다).
    ProfileEdit,
}

/// 호스트가 그릴 때 주는 상태(세션 본문은 따로 `&Session`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ViewInfo {
    pub busy: bool,
    /// 승인 대기(id · 도구 · 요약).
    pub approval: Option<(String, String, String)>,
    pub profiles: Vec<Profile>,
    /// 볼트에 키가 있는 프로필 이름.
    pub keyed: Vec<String>,
    pub current_profile: String,
    /// 아래 줄 칩·스위치(모델 · 노력도 · Auto).
    pub model: String,
    pub effort: String,
    pub auto: bool,
    /// 프로필 페이지 안내 줄(검증 결과 · 저장됨 …).
    pub note: String,
    /// 상태 줄(스트리밍 중 "…" · 마지막 턴 소요·토큰).
    pub status: String,
    /// 분할 창 머리(세션 제목 · 제공자) · 세션 목록(id · 제목 · 제공자).
    pub title: String,
    pub provider: Provider,
    pub sessions: Vec<(String, String, Provider)>,
    /// 이 세션이 쓸 수 있는 모델 선택지(id · 설명) — 하단 칩 팝업.
    pub models: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewAction {
    Send(String),
    Stop,
    AttachFile,
    AttachClipboard,
    RemoveAttachment(usize),
    Approve(String, bool),
    /// `/` 명령 한 줄(`/mode cli` …).
    Slash(String),
    CopyText(String),
    InsertEditor(String),
    /// 코드 블록 [실행](메시지 번호 · 블록 번호 · SQL) — 결과는 그 블록 아래에(사용자 11-11).
    RunCode {
        msg: usize,
        block: usize,
        sql: String,
    },
    /// 코드 블록 [그리드에서 결과 보기] — 보통 실행과 같이 하단 결과 그리드로(사용자 11-11).
    RunGrid(String),
    /// 분할 창 머리: 이전 세션 열기 · 새 세션(제공자).
    OpenSession(String),
    NewSession(Provider),
    ProfileUse(String),
    ProfileDelete(String),
    /// 저장(프로필 · 새 키 = Some · 비우면 기존 유지 · 편집 전 이름 = Some이면 이름이 바뀌었을 때 rename).
    ProfileSave(Profile, Option<String>, Option<String>),
    /// 복제(목록 인덱스).
    ProfileDup(String),
    /// WIF 프로필 브라우저 로그인(폼 내용으로).
    ProfileLogin(Profile),
    ShowPage(Page),
}

/// `/` 명령 표(이름 · 인자 유무 · 설명 키).
pub(crate) const SLASH: &[(&str, bool, Msg)] = &[
    ("/clear", false, Msg::AiSlashClear),
    ("/mode", true, Msg::AiSlashMode),
    ("/model", true, Msg::AiSlashModel),
    ("/effort", true, Msg::AiSlashEffort),
    ("/auto", true, Msg::AiSlashAuto),
    ("/query", true, Msg::AiSlashQuery),
    ("/attach", false, Msg::AiSlashAttach),
    ("/clip", false, Msg::AiSlashClip),
    ("/rows", true, Msg::AiSlashRows),
    ("/grid", false, Msg::AiSlashGrid),
    ("/profiles", false, Msg::AiSlashProfiles),
    ("/status", false, Msg::AiSlashStatus),
    ("/account", false, Msg::AiSlashAccount),
    ("/help", false, Msg::AiSlashHelp),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineKind {
    Text,
    /// 마크다운 제목(굵게 · 110 §6 2차 렌더 11-11).
    Head,
    Code,
    /// 코드 블록 첫 줄(언어 라벨 + 버튼 자리).
    CodeHead(usize),
    /// [실행] 결과 줄(코드 상자 안 · 흐리게) · 실패.
    Result,
    ResultErr,
    Dim,
    Error,
    Role,
    Blank,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    text: String,
    kind: LineKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Send,
    Profiles,
    RemoveAttachment(usize),
    Approve(bool),
    CopyCode(usize),
    InsertCode(usize),
    RunCode(usize),
    GridCode(usize),
    /// 첨부 칩 본문(미리보기) · [+] 첨부 메뉴.
    PreviewAttachment(usize),
    AttachMenu,
    /// [/] 명령 · Auto 스위치.
    Commands,
    Auto,
    /// 분할 창 머리: [≡ 세션] [+ 새 세션].
    SideSessions,
    SideNew,
    /// 하단 [모델 · 노력도] 칩(큰 팝업 = 모델 + 노력도 점 + Auto).
    ModelChip,
    ProfileUse(usize),
    ProfileEdit(usize),
    ProfileDelete(usize),
    ProfileDup(usize),
    ProfileNew,
    ProfileSave,
    /// 목록 행(클릭 = 편집).
    ProfileRow(usize),
    /// 고급(기본 URL · 워크스페이스) 접기/펼치기.
    ProfileAdvanced,
    /// 모델 · 노력도 칩(큰 선택 팝업).
    ProfileModel,
    ProfileEffort,
    /// 인증 방식 토글(API 키 ↔ WIF).
    ProfileAuth,
    ProfileLogin,
    /// 상세 → 목록([←]) · 목록 → 대화.
    Back,
    BackChat,
}

pub(crate) struct AiView {
    bounds: Rect,
    scale: f32,
    page: Page,
    composer: TextBox,
    actions: Vec<ViewAction>,
    hover: Option<usize>,
    pressed: Option<usize>,
    buttons: Vec<(Btn, Rect)>,
    /// 대본 줄 캐시(메시지별) + 그 메시지 글 길이·폭.
    lines: Vec<Vec<Line>>,
    lens: Vec<usize>,
    cache_w: i32,
    /// 코드 블록 본문(버튼 동작용) + 그 블록의 (메시지 번호 · 메시지 안 블록 번호).
    code_blocks: Vec<String>,
    code_owner: Vec<(usize, usize)>,
    /// [+] 첨부 메뉴 · 미리보기 중인 첨부 번호 · 미리보기 캐시(번호 · 이미지 · 안내 글).
    attach_menu: ContextMenu,
    preview: Option<usize>,
    preview_cache: Option<(usize, Option<Rc<IconImage>>, String)>,
    /// 마지막 그리기의 Auto 값(스위치 클릭 → 반대로).
    auto_now: bool,
    /// 분할 창 모드(머리 줄 = 세션 제목 + [≡][+]) · 마지막 그리기의 세션 목록(팝업 항목).
    side_mode: bool,
    side_sessions: Vec<(String, String, Provider)>,
    /// 프로필 목록 행의 전체 글(Alt 전체 보기 말풍선 · 11-11 "버튼에 가려 못 본다").
    p_rows: Vec<String>,
    /// 마지막 그리기의 모델 선택지 · 현재 모델 · 노력도(칩 팝업용).
    chip_models: Vec<(String, String)>,
    model_now: String,
    effort_now: String,
    /// ★ 대본 글자 단위 선택(드래그 · 사용자 11-11 "드래그로 선택·복사" · 11-11 저녁 결정 = 글자 단위): (평면 줄, 글자) 앵커·초점.
    ///   누른/끄는 점은 (줄, x px)로 두고 **다음 그리기**에서 글자 자리로 푼다(x → 글자는 글꼴 폭이 필요 · `resolve_col`).
    sel: Option<((usize, usize), (usize, usize))>,
    drag_from: Option<(usize, i32)>,
    drag_to: Option<(usize, i32)>,
    dragging: bool,
    line_pos: Vec<(usize, i32, i32)>,
    /// 코드 블록 아이콘 버튼 틴트 캐시(종류 · 색).
    icon_cache: RefCell<Vec<(u8, Color, Rc<IconImage>)>>,
    transcript: Rect,
    composer_rect: Rect,
    scroll_y: i32,
    stick_bottom: bool,
    /// 작성 상자 최대 줄(설정 `ai.composer_max_lines` · 사용자 11-11).
    max_lines: i32,
    /// 마지막 마우스 위치(휠을 작성 상자/대본 중 어디로 보낼지 · 사용자 11-11).
    mouse: Point,
    bars: ScrollBars,
    content_h: i32,
    /// `/` 팝업(후보 인덱스 · 선택).
    slash: Vec<usize>,
    slash_sel: usize,
    slash_rect: Rect,
    // 프로필 페이지
    p_name: TextBox,
    p_url: TextBox,
    p_ws: TextBox,
    /// 모델 · 노력도(칩 → 큰 팝업 · 사용자 11-11 "콤보로") · 편집 중 프로필이 조회한 모델 목록.
    p_model: String,
    p_effort: String,
    p_models: Vec<String>,
    /// 고급(기본 URL · 워크스페이스) 펼침.
    p_adv: bool,
    provider: Provider,
    picker: Option<(Btn, Picker)>,
    lh: i32,
    p_key: TextBox,
    /// 인증 = WIF(110 §14-5) · 아래 다섯 상자.
    p_wif: bool,
    p_issuer: TextBox,
    p_client: TextBox,
    p_rule: TextBox,
    p_org: TextBox,
    p_sa: TextBox,
    p_editing: Option<String>,
    p_has_key: bool,
    /// 필수값 점검 결과(비면 없음 · 버튼 아래 경고색).
    form_err: String,
}

impl Default for AiView {
    fn default() -> Self {
        let mut composer = TextBox::new(tf(Msg::AiComposerHint, &["Claude"])).with_multiline();
        composer.set_wrap(true);
        composer.set_line_numbers(false);
        composer.set_gutter_marks(false);
        composer.set_minimap(false);
        composer.set_popup_deferred(true);
        composer.set_idle_caret(true);
        let tb = |ph: &str| {
            let mut b = TextBox::new(ph);
            b.set_popup_deferred(true);
            b
        };
        let mut p_key = tb("sk-ant-…");
        p_key.set_masked(true);
        AiView {
            bounds: Rect::default(),
            scale: 1.0,
            page: Page::Chat,
            composer,
            actions: Vec::new(),
            hover: None,
            pressed: None,
            buttons: Vec::new(),
            lines: Vec::new(),
            lens: Vec::new(),
            cache_w: 0,
            code_blocks: Vec::new(),
            code_owner: Vec::new(),
            attach_menu: ContextMenu::new(),
            preview: None,
            preview_cache: None,
            auto_now: false,
            side_mode: false,
            side_sessions: Vec::new(),
            p_rows: Vec::new(),
            chip_models: Vec::new(),
            model_now: String::new(),
            effort_now: String::new(),
            sel: None,
            drag_from: None,
            drag_to: None,
            dragging: false,
            line_pos: Vec::new(),
            icon_cache: RefCell::new(Vec::new()),
            transcript: Rect::default(),
            composer_rect: Rect::default(),
            max_lines: 8,
            mouse: Point { x: 0, y: 0 },
            scroll_y: 0,
            stick_bottom: true,
            bars: ScrollBars::new(),
            content_h: 0,
            slash: Vec::new(),
            slash_sel: 0,
            slash_rect: Rect::default(),
            p_name: tb("name"),
            p_url: tb(super::DEFAULT_BASE_URL),
            p_ws: tb("wrkspc_… (optional)"),
            p_model: super::DEFAULT_MODEL.to_string(),
            p_effort: super::DEFAULT_EFFORT.to_string(),
            p_models: Vec::new(),
            p_adv: false,
            provider: Provider::Claude,
            picker: None,
            lh: 16,
            p_key,
            p_wif: false,
            p_issuer: tb("https://login.microsoftonline.com/<tenant>/v2.0"),
            p_client: tb("client id (public client, PKCE)"),
            p_rule: tb("fdrl_…"),
            p_org: tb("organization UUID"),
            p_sa: tb("svac_…"),
            p_editing: None,
            p_has_key: false,
            form_err: String::new(),
        }
    }
}

impl AiView {
    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        if (scale - self.scale).abs() > f32::EPSILON {
            for tb in self.boxes_mut() {
                tb.set_scale(scale);
            }
        }
        self.scale = scale;
    }

    fn boxes_mut(&mut self) -> Vec<&mut TextBox> {
        vec![
            &mut self.composer,
            &mut self.p_name,
            &mut self.p_url,
            &mut self.p_ws,
            &mut self.p_key,
            &mut self.p_issuer,
            &mut self.p_client,
            &mut self.p_rule,
            &mut self.p_org,
            &mut self.p_sa,
        ]
    }

    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn show_page(&mut self, p: Page) {
        self.page = p;
        self.hover = None;
        self.pressed = None;
        self.buttons.clear();
        if p == Page::Chat {
            self.composer.set_focused(true);
        }
    }

    /// 워크스페이스 id 칸에 포커스(조직 범위 키 안내 · 11-11).
    pub(crate) fn focus_workspace(&mut self) {
        for (i, tb) in self.boxes_mut().into_iter().enumerate() {
            tb.set_focused(i == 3);
        }
    }

    /// [검증] 뒤 조회한 모델 목록을 폼에 반영(모델 팝업 선택지).
    pub(crate) fn set_models(&mut self, models: Vec<String>) {
        self.p_models = models;
    }

    /// 워크스페이스 자동 보정 뒤 폼의 칸을 맞춘다.
    pub(crate) fn set_workspace(&mut self, ws: &str) {
        self.p_ws.set_text(ws);
        self.p_adv = true;
    }

    /// 편집 중인 기존 프로필 이름(새 프로필이면 None).
    pub(crate) fn editing_name(&self) -> Option<&str> {
        self.p_editing.as_deref()
    }

    /// 프로필 편집 폼 채우기(None = 새 프로필).
    pub(crate) fn edit_profile(&mut self, p: Option<&Profile>, has_key: bool) {
        if self.page != Page::ProfileEdit {
            self.show_page(Page::ProfileEdit);
        }
        let p = p.cloned().unwrap_or_else(|| Profile::new(""));
        self.p_editing = (!p.name.is_empty()).then(|| p.name.clone());
        self.p_name.set_text(&p.name);
        self.p_url.set_text(&p.base_url);
        self.p_ws.set_text(&p.workspace);
        self.p_model = p.model.clone();
        self.p_effort = p.effort.clone();
        self.p_models = p.models.clone();
        // 고급은 기본값과 다를 때만 펼친다(보통 손댈 일 없음).
        self.p_adv = p.base_url != super::DEFAULT_BASE_URL || !p.workspace.is_empty();
        self.p_key.set_text("");
        // ★ 저장된 키가 있으면 칸에 ●●●●(저장됨)을 보여 "채워져 있음"을 알린다(사용자 11-11) — 비워 두면 그대로 유지.
        self.p_key.set_placeholder(if has_key {
            t(Msg::AiKeyStoredMask).to_string()
        } else {
            "sk-ant-…".to_string()
        });
        self.p_wif = p.wif.is_some();
        let w = p.wif.clone().unwrap_or_default();
        self.p_issuer.set_text(&w.issuer);
        self.p_client.set_text(&w.client_id);
        self.p_rule.set_text(&w.rule_id);
        self.p_org.set_text(&w.org_id);
        self.p_sa.set_text(&w.service_account);
        self.p_has_key = has_key;
        self.form_err.clear();
        self.p_name.set_focused(true);
    }

    /// ★ 필수값·형식 점검(사용자 11-11) → 첫 문제 (상자 번호 = `boxes_mut` 순서 · 안내 글). 이름 규칙 = 볼트 비밀 이름 규칙.
    fn form_check(&self) -> Option<(usize, String)> {
        let name = self.p_name.text().trim().to_string();
        if name.is_empty() {
            return Some((1, tf(Msg::AiRequired, &[t(Msg::AiFieldName)])));
        }
        if !super::profile_name_ok(&name) {
            return Some((1, t(Msg::AiNameRule).to_string()));
        }
        let url = self.p_url.text().trim().to_string();
        if !(url.is_empty() || url.starts_with("https://") || url.starts_with("http://")) {
            return Some((2, t(Msg::AiUrlRule).to_string()));
        }
        let ws = self.p_ws.text().trim().to_string();
        if !(ws.is_empty() || ws == "default" || ws.starts_with("wrkspc_")) {
            return Some((3, t(Msg::AiWorkspaceRule).to_string()));
        }
        if self.p_wif {
            for (i, (tb, label)) in [
                (&self.p_issuer, Msg::AiFieldIssuer),
                (&self.p_client, Msg::AiFieldClientId),
                (&self.p_rule, Msg::AiFieldRule),
                (&self.p_org, Msg::AiFieldOrg),
                (&self.p_sa, Msg::AiFieldSa),
            ]
            .into_iter()
            .enumerate()
            {
                if tb.text().trim().is_empty() {
                    return Some((5 + i, tf(Msg::AiRequired, &[t(label)])));
                }
            }
        } else if !self.p_has_key && self.p_key.text().trim().is_empty() {
            return Some((4, tf(Msg::AiRequired, &[t(Msg::AiFieldKey)])));
        }
        None
    }

    /// 점검 통과면 `true` · 아니면 안내 글 + 문제 상자에 포커스(고급 칸이면 펼친다).
    fn form_ok(&mut self) -> bool {
        match self.form_check() {
            None => {
                self.form_err.clear();
                true
            }
            Some((i, msg)) => {
                self.form_err = msg;
                if i == 2 || i == 3 {
                    self.p_adv = true;
                }
                for (j, tb) in self.boxes_mut().into_iter().enumerate() {
                    tb.set_focused(j == i);
                }
                false
            }
        }
    }

    fn form_profile(&self) -> (Profile, Option<String>) {
        let mut p = Profile::new(self.p_name.text().trim());
        let url = self.p_url.text().trim().to_string();
        if !url.is_empty() {
            p.base_url = url;
        }
        p.workspace = self.p_ws.text().trim().to_string();
        if !self.p_model.trim().is_empty() {
            p.model = self.p_model.trim().to_string();
        }
        if !self.p_effort.trim().is_empty() {
            p.effort = self.p_effort.trim().to_string();
        }
        p.models = self.p_models.clone();
        if self.p_wif {
            p.wif = Some(Wif {
                issuer: self.p_issuer.text().trim().to_string(),
                client_id: self.p_client.text().trim().to_string(),
                rule_id: self.p_rule.text().trim().to_string(),
                org_id: self.p_org.text().trim().to_string(),
                service_account: self.p_sa.text().trim().to_string(),
                scope: String::new(),
            });
            return (p, None);
        }
        let key = self.p_key.text().trim().to_string();
        (p, (!key.is_empty()).then_some(key))
    }

    /// 작성 상자 안내 글을 탭 제공자 이름으로(협업 V1 관찰 · 11-11).
    pub(crate) fn set_side_mode(&mut self, on: bool) {
        self.side_mode = on;
    }

    /// 선택한 대본 글(복사용 · 첫/끝 줄은 글자 경계까지 · 중간 줄은 통째) — 없으면 None.
    pub(crate) fn selection_text(&self) -> Option<String> {
        let (a, b) = self.sel_ordered()?;
        let flat: Vec<&Line> = self.lines.iter().flatten().collect();
        let last = flat.len().checked_sub(1)?;
        let (la, lb) = (a.0.min(last), b.0.min(last));
        let mut out = Vec::new();
        for (f, l) in flat.iter().enumerate().take(lb + 1).skip(la) {
            let n = l.text.chars().count();
            let c0 = if f == la { a.1.min(n) } else { 0 };
            let c1 = if f == lb { b.1.min(n) } else { n };
            out.push(
                l.text
                    .chars()
                    .skip(c0)
                    .take(c1.saturating_sub(c0))
                    .collect::<String>(),
            );
        }
        let s = out.join("\n");
        (!s.is_empty()).then_some(s)
    }

    /// 선택 범위를 (앞, 뒤) 순서로.
    fn sel_ordered(&self) -> Option<((usize, usize), (usize, usize))> {
        let (a, b) = self.sel?;
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// 줄 글 · 줄 x → 점 x에 가장 가까운 글자 경계(접두 폭 이진 탐색 · 글꼴은 호출자가 고른다).
    fn col_at(dc: &mut dyn DrawCtx, text: &str, lx: i32, px: i32) -> usize {
        let chars: Vec<char> = text.chars().collect();
        if px <= lx || chars.is_empty() {
            return 0;
        }
        let width =
            |dc: &mut dyn DrawCtx, k: usize| dc.text_width(&chars[..k].iter().collect::<String>());
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if lx + width(dc, mid) <= px {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        if lo < chars.len() {
            let (w0, w1) = (width(dc, lo), width(dc, lo + 1));
            if px - (lx + w0) > (w1 - w0) / 2 {
                return lo + 1;
            }
        }
        lo
    }

    /// 줄 종류 → 글 시작 x · 굵은 글꼴 여부(그릴 때와 같은 규칙).
    fn line_x(kind: &LineKind, x: i32, code_x: i32) -> (i32, bool) {
        match kind {
            LineKind::Code | LineKind::Result | LineKind::ResultErr | LineKind::CodeHead(_) => {
                (code_x, false)
            }
            LineKind::Head | LineKind::Role => (x, true),
            _ => (x, false),
        }
    }

    /// 평면 줄 번호 · 점 x → 글자 자리(그릴 때만 · 글꼴 폭).
    fn resolve_col(
        &self,
        dc: &mut dyn DrawCtx,
        flat: usize,
        px: i32,
        x: i32,
        code_x: i32,
    ) -> usize {
        let Some(l) = self.lines.iter().flatten().nth(flat) else {
            return 0;
        };
        let (lx, bold) = Self::line_x(&l.kind, x, code_x);
        if bold {
            dc.select_font(FontSlot::Base, true);
        }
        let c = Self::col_at(dc, &l.text, lx, px);
        if bold {
            dc.select_font(FontSlot::Base, false);
        }
        c
    }

    /// 대본 안 점 → 평면 줄 번호(그린 줄 기준 · 위/아래 밖이면 첫/끝 줄).
    fn line_at(&self, p: Point) -> Option<usize> {
        if self.line_pos.is_empty() || !self.transcript.contains(p) {
            return None;
        }
        if let Some((i, _, _)) = self
            .line_pos
            .iter()
            .find(|(_, y, h)| p.y >= *y && p.y < y + h)
        {
            return Some(*i);
        }
        if p.y < self.line_pos[0].1 {
            return Some(self.line_pos[0].0);
        }
        self.line_pos.last().map(|(i, _, _)| *i)
    }

    pub(crate) fn set_max_lines(&mut self, n: i32) {
        self.max_lines = n.clamp(1, 60);
    }

    pub(crate) fn set_provider(&mut self, p: Provider) {
        self.provider = p;
        self.composer
            .set_placeholder(tf(Msg::AiComposerHint, &[p.title()]));
    }

    pub(crate) fn reset(&mut self) {
        self.lines.clear();
        self.lens.clear();
        self.scroll_y = 0;
        self.stick_bottom = true;
        self.hover = None;
        self.pressed = None;
    }

    /// ★ 시계 틱(이벤트 루프 · 사용자 11-11 "작성 상자 스크롤바가 항상 표시"): 작성 상자·프로필 상자·대본 스크롤바의 자동 숨김은
    ///   이 틱이 있어야 진행된다(nexa-ctl `ScrollBars::tick` · 전역 `ui.scrollbars`를 따른다).
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let mut r = false;
        for b in self.boxes_mut() {
            r |= b.tick(now_ms);
        }
        r |= self.bars.tick(now_ms);
        r
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ViewAction> {
        std::mem::take(&mut self.actions)
    }

    pub(crate) fn reset_scroll_to_bottom(&mut self) {
        self.stick_bottom = true;
    }

    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        if self.page != Page::Chat {
            for tb in [
                &mut self.p_name,
                &mut self.p_url,
                &mut self.p_ws,
                &mut self.p_key,
                &mut self.p_issuer,
                &mut self.p_client,
                &mut self.p_rule,
                &mut self.p_org,
                &mut self.p_sa,
            ] {
                if tb.is_focused() {
                    return tb;
                }
            }
        }
        &mut self.composer
    }

    pub(crate) fn popup_open(&self) -> bool {
        self.composer.popup_open()
            || !self.slash.is_empty()
            || self.picker.is_some()
            || self.attach_menu.is_open()
            || self.preview.is_some()
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if self.page == Page::Chat {
            self.composer.set_focused(on);
        } else if !on {
            for tb in self.boxes_mut() {
                tb.set_focused(false);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn composer_text(&self) -> String {
        self.composer.text()
    }
    pub(crate) fn set_composer_text(&mut self, s: &str) {
        self.composer.set_text(s);
        self.update_slash();
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn update_slash(&mut self) {
        let text = self.composer.text();
        self.slash.clear();
        if text.starts_with('/') && !text.contains('\n') && !text.contains(' ') {
            let q = text.to_lowercase();
            self.slash = SLASH
                .iter()
                .enumerate()
                .filter(|(_, (n, _, _))| n.starts_with(&q))
                .map(|(i, _)| i)
                .collect();
            if self.slash.len() == 1 && SLASH[self.slash[0]].0 == text {
                // 정확히 일치 — 팝업은 두되 Enter가 명령을 보낸다.
            }
            self.slash_sel = self.slash_sel.min(self.slash.len().saturating_sub(1));
        }
    }

    fn submit(&mut self) {
        let text = self.composer.text();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if trimmed.starts_with('/') {
            self.actions.push(ViewAction::Slash(trimmed.to_string()));
        } else {
            self.actions.push(ViewAction::Send(trimmed.to_string()));
        }
        self.composer.set_text("");
        self.slash.clear();
        self.stick_bottom = true;
    }

    fn complete_slash(&mut self) -> bool {
        let Some(&i) = self.slash.get(self.slash_sel) else {
            return false;
        };
        let (name, has_arg, _) = SLASH[i];
        if has_arg {
            self.composer.set_text(&format!("{name} "));
            self.slash.clear();
            // 캐럿은 set_text 뒤 끝으로 간다(상자 규칙).
        } else {
            self.composer.set_text(name);
            self.submit();
        }
        true
    }

    fn after_box(&mut self, tb_is_composer: bool) {
        let act = if tb_is_composer {
            self.composer.take_edit_ctx()
        } else {
            self.textbox_mut().take_edit_ctx()
        };
        if let Some(a) = act {
            match a {
                nexa_ctl::EditCtxAction::Copy => {
                    let tb = self.textbox_mut();
                    if let Some(s) = tb.copy_selection() {
                        let _ = crate::clipboard::write_text(&s);
                    }
                }
                nexa_ctl::EditCtxAction::Paste => {
                    if let Some(s) = crate::clipboard::read_text() {
                        let mut inv = Invalidations::default();
                        self.textbox_mut().paste(&s, &mut inv);
                    }
                }
                nexa_ctl::EditCtxAction::Cut => {
                    let tb = self.textbox_mut();
                    if let Some(s) = tb.copy_selection() {
                        let _ = crate::clipboard::write_text(&s);
                        let mut inv = Invalidations::default();
                        tb.paste("", &mut inv);
                    }
                }
                _ => {}
            }
        }
    }

    /// 사건 — 다시 그릴 일이 있으면 true(호스트: 포인터는 뷰 영역 안일 때 · 키는 편집기 포커스일 때).
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
        if self.page != Page::Chat {
            return self.on_event_profiles(ev);
        }
        // 첨부 미리보기 오버레이 = 아무 클릭/Esc로 닫기.
        if self.preview.is_some() {
            if matches!(
                *ev,
                InputEvent::MouseDown { .. }
                    | InputEvent::RightDown { .. }
                    | InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
            ) {
                self.preview = None;
                self.preview_cache = None;
                return true;
            }
            return !matches!(ev, InputEvent::MouseMove { .. });
        }
        // 세션 선택 팝업(분할 창 머리 [≡]).
        if let Some((kind, p)) = &mut self.picker {
            let kind = *kind;
            match p.on_event(ev) {
                PickEvent::None | PickEvent::Redraw => return true,
                PickEvent::Effort(i) => {
                    if let Some(e) = super::EFFORTS.get(i) {
                        self.effort_now = (*e).to_string();
                        if let Some((_, p)) = &mut self.picker {
                            p.set_extra(
                                0,
                                super::picker::Extra::Effort {
                                    label: tf(Msg::AiEffortLabel, &[e]),
                                    idx: i,
                                    n: super::EFFORTS.len(),
                                },
                            );
                        }
                        self.actions.push(ViewAction::Slash(format!("/effort {e}")));
                    }
                    return true;
                }
                PickEvent::Toggle(on) => {
                    self.auto_now = on;
                    if let Some((_, p)) = &mut self.picker {
                        p.set_extra(
                            1,
                            super::picker::Extra::Toggle {
                                label: t(Msg::AiAuto).to_string(),
                                on,
                            },
                        );
                    }
                    let cmd = if on { "/auto on" } else { "/auto off" };
                    self.actions.push(ViewAction::Slash(cmd.into()));
                    return true;
                }
                PickEvent::Pick(id) => {
                    self.picker = None;
                    if kind == Btn::ModelChip {
                        self.actions.push(ViewAction::Slash(format!("/model {id}")));
                    } else if let Some(rest) = id.strip_prefix("new:") {
                        self.actions
                            .push(ViewAction::NewSession(Provider::parse(rest)));
                    } else if let Some(sid) = id.strip_prefix("s:") {
                        self.actions.push(ViewAction::OpenSession(sid.to_string()));
                    }
                    return true;
                }
                PickEvent::Close => {
                    self.picker = None;
                    if !matches!(ev, InputEvent::MouseDown { .. }) {
                        return true;
                    }
                }
            }
        }
        // [+] 첨부 메뉴.
        if self.attach_menu.is_open() {
            let outside = self.attach_menu.is_outside_click(ev);
            let consumed = self.attach_menu.on_event(ev) && !outside;
            if let Some(id) = self.attach_menu.take_picked() {
                match id.as_str() {
                    "file" => self.actions.push(ViewAction::AttachFile),
                    "clip" => self.actions.push(ViewAction::AttachClipboard),
                    "grid" => self.actions.push(ViewAction::Slash("/grid".into())),
                    _ => {}
                }
                return true;
            }
            if consumed {
                return true;
            }
            if matches!(ev, InputEvent::MouseMove { .. }) {
                return false;
            }
        }
        if self.composer.popup_open() {
            self.composer.on_event(ev, &mut inv);
            self.after_box(true);
            return true;
        }
        // `/` 팝업 키.
        if !self.slash.is_empty() {
            match *ev {
                InputEvent::Key {
                    key: CtlKey::Down, ..
                } => {
                    self.slash_sel = (self.slash_sel + 1).min(self.slash.len() - 1);
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Up, ..
                } => {
                    self.slash_sel = self.slash_sel.saturating_sub(1);
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                } => {
                    self.slash.clear();
                    return true;
                }
                InputEvent::Key {
                    key: CtlKey::Enter,
                    shift: false,
                    ..
                } => {
                    let text = self.composer.text();
                    if self.slash.len() == 1 && SLASH[self.slash[0]].0 == text.trim() {
                        self.submit();
                    } else {
                        self.complete_slash();
                    }
                    return true;
                }
                InputEvent::Char { c: '\t', .. } => {
                    self.complete_slash();
                    return true;
                }
                InputEvent::MouseDown { x, y, .. } if self.slash_rect.contains(Point { x, y }) => {
                    let row_h = self.s(24.0);
                    let i = ((y - self.slash_rect.y - self.s(4.0)) / row_h.max(1)) as usize;
                    if i < self.slash.len() {
                        self.slash_sel = i;
                        self.complete_slash();
                    }
                    return true;
                }
                _ => {}
            }
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                self.mouse = Point { x, y };
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                let mut changed = h != self.hover;
                self.hover = h;
                if self.dragging {
                    if let Some(to) = self.line_at(p) {
                        // 글자 자리는 다음 그리기에서(`resolve_col`).
                        self.drag_to = Some((to, x));
                        changed = true;
                    }
                }
                self.composer.on_event(ev, &mut inv);
                let (_, ny, c) = self.bars.on_event(
                    ev,
                    self.transcript,
                    self.transcript.w,
                    self.content_h.max(self.transcript.h),
                    0,
                    self.scroll_y,
                    self.scale,
                );
                if ny != self.scroll_y {
                    self.scroll_y = ny;
                    self.stick_bottom = false;
                }
                changed || c || !inv.is_empty()
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.pressed = self.buttons.iter().position(|(_, r)| r.contains(p));
                if self.pressed.is_none() {
                    let (_, ny, c) = self.bars.on_event(
                        ev,
                        self.transcript,
                        self.transcript.w,
                        self.content_h.max(self.transcript.h),
                        0,
                        self.scroll_y,
                        self.scale,
                    );
                    if c {
                        if ny != self.scroll_y {
                            self.scroll_y = ny;
                            self.stick_bottom = false;
                        }
                        return true;
                    }
                    let in_c = self.composer.bounds().contains(p);
                    self.composer.set_focused(in_c);
                    if in_c {
                        self.composer.on_event(ev, &mut inv);
                    }
                    // 대본 안 = 글자 선택 시작(끌면 범위 · 그냥 클릭 = 선택 해제).
                    self.sel = None;
                    self.drag_from = if in_c {
                        None
                    } else {
                        self.line_at(p).map(|l| (l, p.x))
                    };
                    self.drag_to = None;
                    self.dragging = self.drag_from.is_some();
                }
                true
            }
            InputEvent::RightDown { x, y } => {
                if self.composer.bounds().contains(Point { x, y }) {
                    self.composer.set_focused(true);
                    self.composer.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                let (_, ny, c) = self.bars.on_event(
                    ev,
                    self.transcript,
                    self.transcript.w,
                    self.content_h.max(self.transcript.h),
                    0,
                    self.scroll_y,
                    self.scale,
                );
                if ny != self.scroll_y {
                    self.scroll_y = ny;
                }
                if self.dragging {
                    self.dragging = false;
                    if let Some(to) = self.line_at(p) {
                        self.drag_to = Some((to, p.x));
                    }
                }
                if let Some(i) = self.pressed.take() {
                    if let Some((b, r)) = self.buttons.get(i).cloned() {
                        if r.contains(p) {
                            self.click(b);
                        }
                    }
                    return true;
                }
                if !c {
                    self.composer.on_event(ev, &mut inv);
                    self.after_box(true);
                }
                true
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                // 작성 상자 위면 상자 안 스크롤 · 아니면 대본(종전엔 늘 대본이 먼저 움직였다 · 사용자 11-11).
                if self.composer.bounds().contains(self.mouse) {
                    self.composer.on_event(ev, &mut inv);
                    return true;
                }
                let (_, ny, c) = self.bars.on_event(
                    ev,
                    self.transcript,
                    self.transcript.w,
                    self.content_h.max(self.transcript.h),
                    0,
                    self.scroll_y,
                    self.scale,
                );
                if c || ny != self.scroll_y {
                    self.scroll_y = ny;
                    self.stick_bottom = ny >= (self.content_h - self.transcript.h).max(0);
                    return true;
                }
                self.composer.on_event(ev, &mut inv);
                true
            }
            InputEvent::Key {
                key: CtlKey::Enter,
                shift: false,
                ..
            } if self.composer.is_focused() => {
                self.submit();
                true
            }
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } => {
                self.sel = None;
                if !self.composer.text().is_empty() {
                    self.composer.set_text("");
                    self.slash.clear();
                }
                true
            }
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo => {
                self.composer.set_focused(true);
                self.composer.on_event(ev, &mut inv);
                self.after_box(true);
                self.update_slash();
                true
            }
            _ => false,
        }
    }

    fn click(&mut self, b: Btn) {
        match b {
            Btn::Send => {
                // busy면 ■.
                self.actions.push(ViewAction::Stop);
            }
            Btn::PreviewAttachment(i) => {
                self.preview = Some(i);
                self.preview_cache = None;
            }
            Btn::Commands => {
                self.composer.set_text("/");
                self.composer.set_focused(true);
                self.update_slash();
            }
            Btn::Auto => {
                let cmd = if self.auto_now {
                    "/auto off"
                } else {
                    "/auto on"
                };
                self.actions.push(ViewAction::Slash(cmd.into()));
            }
            Btn::SideNew => {
                let p = self.provider;
                self.actions.push(ViewAction::NewSession(p));
            }
            Btn::ModelChip => {
                let anchor = self
                    .buttons
                    .iter()
                    .find(|(k, _)| *k == Btn::ModelChip)
                    .map(|(_, r)| *r)
                    .unwrap_or_default();
                let items: Vec<PickItem> = self
                    .chip_models
                    .iter()
                    .map(|(id, desc)| PickItem {
                        id: id.clone(),
                        title: id.clone(),
                        desc: desc.clone(),
                    })
                    .collect();
                let sel = items.iter().position(|i| i.id == self.model_now);
                let eidx = super::EFFORTS
                    .iter()
                    .position(|e| *e == self.effort_now)
                    .unwrap_or(1);
                let extras = vec![
                    super::picker::Extra::Effort {
                        label: tf(Msg::AiEffortLabel, &[&self.effort_now]),
                        idx: eidx,
                        n: super::EFFORTS.len(),
                    },
                    super::picker::Extra::Toggle {
                        label: t(Msg::AiAuto).to_string(),
                        on: self.auto_now,
                    },
                ];
                let host = self.bounds;
                let w = self.s(340.0).min(host.w);
                let anchor = Rect::new(anchor.right() - w, anchor.y, w, anchor.h);
                let p = Picker::new(
                    t(Msg::AiPickModel),
                    items,
                    sel,
                    extras,
                    anchor,
                    host,
                    self.lh,
                    self.scale,
                );
                self.picker = Some((Btn::ModelChip, p));
            }
            Btn::SideSessions => {
                let anchor = self
                    .buttons
                    .iter()
                    .find(|(k, _)| *k == Btn::SideSessions)
                    .map(|(_, r)| *r)
                    .unwrap_or_default();
                let mut items = vec![
                    PickItem {
                        id: "new:claude".into(),
                        title: tf(Msg::AiNewSessionFor, &["Claude"]),
                        desc: String::new(),
                    },
                    PickItem {
                        id: "new:codex".into(),
                        title: tf(Msg::AiNewSessionFor, &["Codex"]),
                        desc: String::new(),
                    },
                ];
                for (id, title, p) in &self.side_sessions {
                    items.push(PickItem {
                        id: format!("s:{id}"),
                        title: title.clone(),
                        desc: p.title().to_string(),
                    });
                }
                let host = self.bounds;
                let anchor = Rect::new(
                    anchor.right() - self.s(320.0).min(host.w),
                    anchor.y,
                    self.s(320.0).min(host.w),
                    anchor.h,
                );
                let p = Picker::new(
                    t(Msg::AiPickSession),
                    items,
                    None,
                    Vec::new(),
                    anchor,
                    host,
                    self.lh,
                    self.scale,
                );
                self.picker = Some((Btn::SideSessions, p));
            }
            Btn::AttachMenu => {
                let anchor = self
                    .buttons
                    .iter()
                    .find(|(k, _)| *k == Btn::AttachMenu)
                    .map(|(_, r)| *r)
                    .unwrap_or_default();
                let items = vec![
                    CtxItem::item("file", t(Msg::AiAttachFile)),
                    CtxItem::item("clip", t(Msg::AiAttachClipboard)),
                    CtxItem::item("grid", t(Msg::AiMenuGrid)),
                ];
                let host = Rect::new(0, 0, self.bounds.right(), i32::MAX / 4);
                let (ay, tw) = (anchor.y - self.s(4.0), self.s(200.0));
                crate::menu_icons::open_at(&mut self.attach_menu, anchor.x, ay, items, host, tw);
            }
            Btn::Profiles => self.actions.push(ViewAction::ShowPage(Page::Profiles)),
            Btn::RemoveAttachment(i) => self.actions.push(ViewAction::RemoveAttachment(i)),
            Btn::Approve(ok) => self.actions.push(ViewAction::Approve(String::new(), ok)),
            Btn::CopyCode(i) => {
                if let Some(c) = self.code_blocks.get(i) {
                    self.actions.push(ViewAction::CopyText(c.clone()));
                }
            }
            Btn::InsertCode(i) => {
                if let Some(c) = self.code_blocks.get(i) {
                    self.actions.push(ViewAction::InsertEditor(c.clone()));
                }
            }
            Btn::RunCode(i) => {
                if let (Some(c), Some(&(msg, block))) =
                    (self.code_blocks.get(i), self.code_owner.get(i))
                {
                    self.actions.push(ViewAction::RunCode {
                        msg,
                        block,
                        sql: c.clone(),
                    });
                }
            }
            Btn::GridCode(i) => {
                if let Some(c) = self.code_blocks.get(i) {
                    self.actions.push(ViewAction::RunGrid(c.clone()));
                }
            }
            Btn::ProfileUse(i) => self.actions.push(ViewAction::ProfileUse(i.to_string())),
            Btn::ProfileEdit(i) => self
                .actions
                .push(ViewAction::ProfileUse(format!("edit:{i}"))),
            Btn::ProfileDelete(i) => self.actions.push(ViewAction::ProfileDelete(i.to_string())),
            Btn::ProfileDup(i) => self.actions.push(ViewAction::ProfileDup(i.to_string())),
            Btn::ProfileNew => self.edit_profile(None, false),
            Btn::ProfileSave => {
                if self.form_ok() {
                    let (p, key) = self.form_profile();
                    let prev = self.p_editing.clone();
                    self.actions.push(ViewAction::ProfileSave(p, key, prev));
                }
            }
            Btn::ProfileRow(i) => self
                .actions
                .push(ViewAction::ProfileUse(format!("edit:{i}"))),
            Btn::ProfileAdvanced => self.p_adv = !self.p_adv,
            Btn::ProfileModel | Btn::ProfileEffort => {
                let anchor = self
                    .buttons
                    .iter()
                    .find(|(k, _)| *k == b)
                    .map(|(_, r)| *r)
                    .unwrap_or_default();
                let (title, items, cur) = if b == Btn::ProfileModel {
                    (
                        t(Msg::AiPickModel),
                        super::model_items(self.provider, &self.p_models, &self.p_model),
                        self.p_model.clone(),
                    )
                } else {
                    (
                        t(Msg::AiPickEffort),
                        super::effort_items(),
                        self.p_effort.clone(),
                    )
                };
                let items: Vec<PickItem> = items
                    .into_iter()
                    .map(|(id, desc)| PickItem {
                        id: id.clone(),
                        title: id,
                        desc,
                    })
                    .collect();
                let sel = items.iter().position(|i| i.id == cur);
                let host = self.bounds;
                let p = Picker::new(
                    title,
                    items,
                    sel,
                    Vec::new(),
                    anchor,
                    host,
                    self.lh,
                    self.scale,
                );
                self.picker = Some((b, p));
            }
            Btn::ProfileAuth => {
                self.p_wif = !self.p_wif;
                self.form_err.clear();
            }
            Btn::ProfileLogin => {
                if self.form_ok() {
                    let (p, _) = self.form_profile();
                    self.actions.push(ViewAction::ProfileLogin(p));
                }
            }
            Btn::Back => self.actions.push(ViewAction::ShowPage(Page::Profiles)),
            Btn::BackChat => self.actions.push(ViewAction::ShowPage(Page::Chat)),
        }
    }

    fn on_event_profiles(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
        // 모델/노력도 큰 팝업이 열려 있으면 그것이 먼저(바깥 클릭 = 닫고 그대로 진행).
        if let Some((kind, p)) = &mut self.picker {
            let kind = *kind;
            match p.on_event(ev) {
                PickEvent::None | PickEvent::Redraw => return true,
                PickEvent::Pick(id) => {
                    if kind == Btn::ProfileModel {
                        self.p_model = id;
                    } else {
                        self.p_effort = id;
                    }
                    self.picker = None;
                    return true;
                }
                PickEvent::Effort(_) | PickEvent::Toggle(_) => return true,
                PickEvent::Close => {
                    self.picker = None;
                    if !matches!(ev, InputEvent::MouseDown { .. }) {
                        return true;
                    }
                }
            }
        }
        // 팝업(편집 메뉴) 열린 상자가 먼저.
        for tb in self.boxes_mut() {
            if tb.popup_open() {
                tb.on_event(ev, &mut inv);
                break;
            }
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                let changed = h != self.hover;
                self.hover = h;
                for tb in self.boxes_mut() {
                    tb.on_event(ev, &mut inv);
                }
                changed || !inv.is_empty()
            }
            InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                self.pressed = self.buttons.iter().position(|(_, r)| r.contains(p));
                if self.pressed.is_none() {
                    // 숨긴 상자(영역 0)는 제외 · 첫 적중에서 멈춤(종전 "마지막 적중"이 숨은 WIF 상자를 고르던 결함 · 11-11).
                    let mut hit = None;
                    for (i, tb) in self.boxes_mut().into_iter().enumerate().skip(1) {
                        let b = tb.bounds();
                        if b.w > 0 && b.h > 0 && b.contains(p) {
                            hit = Some(i);
                            break;
                        }
                    }
                    for (i, tb) in self.boxes_mut().into_iter().enumerate().skip(1) {
                        tb.set_focused(hit == Some(i));
                        if hit == Some(i) {
                            tb.on_event(ev, &mut inv);
                        }
                    }
                }
                true
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                if let Some(i) = self.pressed.take() {
                    if let Some((b, r)) = self.buttons.get(i).cloned() {
                        if r.contains(p) {
                            self.click(b);
                        }
                    }
                    return true;
                }
                for tb in self.boxes_mut() {
                    if tb.is_focused() {
                        tb.on_event(ev, &mut inv);
                    }
                }
                self.after_box(false);
                true
            }
            InputEvent::Char { c: '\t', .. } => {
                // Tab = 다음 보이는 칸(숨긴 상자 건너뜀).
                let boxes = self.boxes_mut();
                let visible: Vec<usize> = boxes
                    .iter()
                    .enumerate()
                    .skip(1)
                    .filter(|(_, b)| b.bounds().w > 0)
                    .map(|(i, _)| i)
                    .collect();
                let cur = boxes.iter().position(|b| b.is_focused());
                let next = match (cur, visible.first()) {
                    (_, None) => return true,
                    (Some(c), Some(_)) => visible
                        .iter()
                        .position(|&i| i == c)
                        .map(|k| visible[(k + 1) % visible.len()])
                        .unwrap_or(visible[0]),
                    (None, Some(&f)) => f,
                };
                for (i, tb) in self.boxes_mut().into_iter().enumerate() {
                    tb.set_focused(i == next);
                }
                true
            }
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } => {
                let to = if self.page == Page::ProfileEdit {
                    Page::Profiles
                } else {
                    Page::Chat
                };
                self.actions.push(ViewAction::ShowPage(to));
                true
            }
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo => {
                for tb in self.boxes_mut() {
                    if tb.is_focused() {
                        tb.on_event(ev, &mut inv);
                    }
                }
                self.after_box(false);
                true
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => false,
            _ => false,
        }
    }

    // ───────────────────────── 그리기 ─────────────────────────

    fn rebuild_lines(&mut self, dc: &mut dyn DrawCtx, sess: &Session, w: i32) {
        if self.cache_w != w {
            self.lines.clear();
            self.lens.clear();
            self.cache_w = w;
        }
        self.lines.truncate(sess.messages.len());
        self.lens.truncate(sess.messages.len());
        self.code_blocks.clear();
        self.code_owner.clear();
        let mut code_i = 0usize;
        let who = match sess.provider {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        };
        for (i, m) in sess.messages.iter().enumerate() {
            let key = msg_key(m);
            let need = self.lines.get(i).is_none() || self.lens[i] != key;
            if need {
                let lines = layout_msg(dc, m, w, &mut code_i, &mut self.code_blocks, who);
                if i < self.lines.len() {
                    self.lines[i] = lines;
                    self.lens[i] = key;
                } else {
                    self.lines.push(lines);
                    self.lens.push(key);
                }
            } else {
                // 캐시된 메시지의 코드 블록 번호를 이어 붙인다.
                for l in &self.lines[i] {
                    if let LineKind::CodeHead(_) = l.kind {
                        code_i += 1;
                    }
                }
                collect_code(&m.text, &mut self.code_blocks);
            }
            let n = self.lines[i]
                .iter()
                .filter(|l| matches!(l.kind, LineKind::CodeHead(_)))
                .count();
            for k in 0..n {
                self.code_owner.push((i, k));
            }
        }
    }

    /// 코드 블록 아이콘(0 복사 · 1 편집기에 넣기 · 2 실행) — 색별 틴트 캐시.
    fn block_icon(&self, which: u8, fg: Color) -> Option<Rc<IconImage>> {
        if let Some((_, _, img)) = self
            .icon_cache
            .borrow()
            .iter()
            .find(|(k, c, _)| *k == which && *c == fg)
        {
            return Some(img.clone());
        }
        let ic: MenuIcon = match which {
            0 => crate::toolicons::mi_copy(),
            1 => crate::toolicons::mi_edit(),
            3 => crate::toolicons::mi_add(),
            4 => crate::toolicons::mi_key(),
            5 => crate::toolicons::mi_check(),
            8 => crate::toolicons::mi_table(),
            _ => crate::toolicons::mi_run()?,
        };
        let (r, g, b) = fg.rgb();
        let img = Rc::new(IconImage::from_alpha_tinted(
            ic.w,
            ic.h,
            &ic.alpha,
            (r, g, b),
        ));
        self.icon_cache.borrow_mut().push((which, fg, img.clone()));
        Some(img)
    }

    pub(crate) fn paint(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        sess: &Session,
        info: &ViewInfo,
    ) {
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        dc.fill_rect(b, th.field_bg);
        self.buttons.clear();
        if self.page != Page::Chat {
            self.paint_profiles(dc, th, info);
            return;
        }
        let pad = self.s(12.0);
        let x = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(80.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let line_h = lh + self.s(4.0);
        // ── 머리 줄: 분할 창에서만(세션 제목 + [≡ 세션][+ 새 세션] · 사용자 11-11) · 탭에서는 없음.
        let mut head_h = 0;
        if self.side_mode {
            self.side_sessions = info.sessions.clone();
            let row_h = lh + self.s(8.0);
            let hy = b.y + self.s(4.0);
            let side = row_h;
            let mut bx = b.right() - pad;
            for (btn, glyph) in [(Btn::SideNew, "+"), (Btn::SideSessions, "≡")] {
                bx -= side;
                let r = Rect::new(bx, hy, side, row_h);
                let idx = self.buttons.len();
                let hot = self.hover == Some(idx);
                if hot {
                    dc.fill_round_rect(r, self.s(4.0), th.sel_bg);
                }
                let gw = dc.text_width(glyph);
                let ty = dc.text_center_y(r.y, r.h);
                dc.text(r.x + (r.w - gw) / 2, ty, r, glyph, th.text);
                self.buttons.push((btn, r));
                bx -= self.s(2.0);
            }
            dc.select_font(FontSlot::Base, true);
            let title =
                nexa_ctl::draw::ellipsize_middle(dc, &info.title, (bx - x - self.s(8.0)).max(20));
            let ty = dc.text_center_y(hy, row_h);
            dc.text(x, ty, b, &title, th.text);
            dc.select_font(FontSlot::Base, false);
            head_h = row_h + self.s(8.0);
            dc.fill_rect(Rect::new(b.x, b.y + head_h - 1, b.w, 1), th.border);
        }
        let agent = format!(
            "{}  ·  {}{}",
            match sess.provider {
                super::Provider::Claude => t(Msg::AiProviderClaude),
                super::Provider::Codex => t(Msg::AiProviderCodex),
            },
            match (sess.mode, sess.provider) {
                (Mode::Api, _) => t(Msg::AiModeApi),
                (Mode::Cli, super::Provider::Claude) => t(Msg::AiModeCliClaude),
                (Mode::Cli, super::Provider::Codex) => t(Msg::AiModeCliCodex),
            },
            if (sess.mode == Mode::Api || sess.provider == super::Provider::Codex)
                && !sess.profile.is_empty()
            {
                format!("  ·  {}", sess.profile)
            } else {
                String::new()
            }
        );
        let top = b.y + self.s(4.0) + head_h;
        // ── 작성 영역 높이(아래에서 위로 잡는다)
        let text = self.composer.text();
        let n_lines = (text.split('\n').count() as i32).clamp(1, self.max_lines);
        let comp_h = n_lines * self.composer.line_h().max(lh) + self.s(14.0);
        let btn_h = lh + self.s(10.0);
        let chips_h = if sess.attachments.is_empty() {
            0
        } else {
            btn_h + self.s(6.0)
        };
        let hint_h = 0;
        let bottom_h = comp_h + btn_h + chips_h + hint_h + self.s(24.0);
        let comp_top = b.bottom() - bottom_h;
        self.transcript = Rect::new(b.x, top, b.w, (comp_top - top).max(0));
        // ── 대본
        self.rebuild_lines(dc, sess, w - self.s(16.0));
        // 대기 중 승인 카드의 버튼 줄도 높이에 넣는다(종전엔 빠져 버튼이 아래로 잘림 · 사용자 11-11).
        let total: i32 = self
            .lines
            .iter()
            .enumerate()
            .map(|(i, ls)| {
                let mut h = ls.len() as i32 * line_h + self.s(10.0);
                let pending = sess.messages.get(i).is_some_and(|m| {
                    matches!(m.kind, Kind::Approval { .. })
                        && info.approval.as_ref().is_some_and(|a| a.0 == m.id)
                });
                if pending {
                    h += btn_h + self.s(6.0);
                }
                h
            })
            .sum();
        self.content_h = total + self.s(8.0);
        let max_scroll = (self.content_h - self.transcript.h).max(0);
        if self.stick_bottom || (info.busy && self.scroll_y >= max_scroll - line_h * 2) {
            self.scroll_y = max_scroll;
        }
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);
        let tr = self.transcript;
        dc.push_clip(tr);
        let mut ly = tr.y + self.s(8.0) - self.scroll_y;
        let code_x = x + self.s(8.0);
        // 선택 점 → 글자 자리(끄는 중엔 매 그리기 · 놓으면 한 번 더 풀고 비운다).
        if let Some((la, xa)) = self.drag_from {
            let a = (la, self.resolve_col(dc, la, xa, x, code_x));
            if let Some((lf, xf)) = self.drag_to {
                let f = (lf, self.resolve_col(dc, lf, xf, x, code_x));
                self.sel = (a != f).then_some((a, f));
            }
            if !self.dragging {
                self.drag_from = None;
                self.drag_to = None;
            }
        }
        let lines = std::mem::take(&mut self.lines);
        self.line_pos.clear();
        let mut flat_base = 0usize;
        let sel = self.sel_ordered();
        for (mi, ls) in lines.iter().enumerate() {
            let block_h = ls.len() as i32 * line_h;
            let msg = &sess.messages[mi];
            let my_base = flat_base;
            flat_base += ls.len();
            if ly + block_h >= tr.y && ly <= tr.bottom() {
                for li in 0..ls.len() {
                    let yy = ly + li as i32 * line_h;
                    self.line_pos.push((my_base + li, yy, line_h));
                }
                let is_user = msg.kind == Kind::User;
                if is_user {
                    dc.fill_round_rect(
                        Rect::new(
                            x - self.s(4.0),
                            ly - self.s(3.0),
                            w + self.s(8.0),
                            block_h + self.s(6.0),
                        ),
                        self.s(6.0),
                        th.panel_bg_alt,
                    );
                }
                let mut in_code = false;
                for (li, l) in ls.iter().enumerate() {
                    let yy = ly + li as i32 * line_h;
                    match &l.kind {
                        LineKind::CodeHead(ci) => {
                            in_code = true;
                            // 코드 블록 배경은 끝 줄까지 — 끝을 찾아 미리 칠한다.
                            let n = ls[li + 1..]
                                .iter()
                                .take_while(|q| {
                                    matches!(
                                        q.kind,
                                        LineKind::Code | LineKind::Result | LineKind::ResultErr
                                    )
                                })
                                .count() as i32;
                            let cr = Rect::new(
                                code_x - self.s(6.0),
                                yy,
                                w - self.s(10.0),
                                (n + 1) * line_h + self.s(4.0),
                            );
                            dc.fill_round_rect(cr, self.s(4.0), th.panel_bg);
                            dc.stroke_round_rect(cr, self.s(4.0), th.border, 1.0);
                            dc.text(code_x, yy + self.s(2.0), tr, &l.text, th.text_dim);
                            // 아이콘 버튼 [실행][편집기에 넣기][복사](오른쪽부터 · 툴팁은 paint_popup · 사용자 11-11 "공간 절약").
                            let side = line_h.max(self.s(18.0));
                            let mut bx = cr.right() - self.s(4.0);
                            for (btn, which) in [
                                (Btn::GridCode(*ci), 8u8),
                                (Btn::RunCode(*ci), 2u8),
                                (Btn::InsertCode(*ci), 1u8),
                                (Btn::CopyCode(*ci), 0u8),
                            ] {
                                bx -= side;
                                let r = Rect::new(bx, yy, side, side);
                                let idx = self.buttons.len();
                                let hot = self.hover == Some(idx);
                                if hot {
                                    dc.fill_round_rect(r, self.s(3.0), th.sel_bg);
                                }
                                let fg = if which == 2 { th.accent } else { th.text_dim };
                                if let Some(img) = self.block_icon(which, fg) {
                                    let icon = self.s(14.0);
                                    let dst = Rect::new(
                                        r.x + (r.w - icon) / 2,
                                        r.y + (r.h - icon) / 2,
                                        icon,
                                        icon,
                                    );
                                    dc.image_scaled(dst, &img, tr);
                                }
                                self.buttons.push((btn, r));
                                bx -= self.s(2.0);
                            }
                        }
                        LineKind::Code => {
                            dc.text(code_x, yy + self.s(2.0), tr, &l.text, th.syn_string);
                        }
                        LineKind::Result => {
                            dc.text(code_x, yy + self.s(2.0), tr, &l.text, th.text_dim);
                        }
                        LineKind::ResultErr => {
                            dc.text(code_x, yy + self.s(2.0), tr, &l.text, th.warn);
                        }
                        LineKind::Text => {
                            in_code = false;
                            dc.text(x, yy, tr, &l.text, th.text);
                        }
                        LineKind::Head => {
                            dc.select_font(FontSlot::Base, true);
                            dc.text(x, yy, tr, &l.text, th.text);
                            dc.select_font(FontSlot::Base, false);
                        }
                        LineKind::Dim => dc.text(x, yy, tr, &l.text, th.text_dim),
                        LineKind::Error => dc.text(x, yy, tr, &l.text, th.warn),
                        LineKind::Role => {
                            dc.select_font(FontSlot::Base, true);
                            dc.text(
                                x,
                                yy,
                                tr,
                                &l.text,
                                if is_user { th.accent } else { th.text },
                            );
                            dc.select_font(FontSlot::Base, false);
                        }
                        LineKind::Blank => {}
                    }
                    let _ = in_code;
                }
                // 글자 단위 선택 강조(글 위에 반투명 · 첫/끝 줄은 글자 경계 · 중간 줄은 끝까지 + 줄 바꿈 표시 여백).
                if let Some((a, b2)) = sel {
                    for (li, l) in ls.iter().enumerate() {
                        let f = my_base + li;
                        if f < a.0 || f > b2.0 {
                            continue;
                        }
                        let (lx, bold) = Self::line_x(&l.kind, x, code_x);
                        let n = l.text.chars().count();
                        let c0 = if f == a.0 { a.1.min(n) } else { 0 };
                        let c1 = if f == b2.0 { b2.1.min(n) } else { n };
                        if bold {
                            dc.select_font(FontSlot::Base, true);
                        }
                        let x0 = lx + dc.text_width(&l.text.chars().take(c0).collect::<String>());
                        let mut x1 =
                            lx + dc.text_width(&l.text.chars().take(c1).collect::<String>());
                        if bold {
                            dc.select_font(FontSlot::Base, false);
                        }
                        if f < b2.0 {
                            x1 += self.s(6.0);
                        }
                        if x1 > x0 {
                            let yy = ly + li as i32 * line_h;
                            dc.fill_rect_alpha(Rect::new(x0, yy, x1 - x0, line_h), th.accent, 0.25);
                        }
                    }
                }
                // 승인 카드 버튼(대기 중인 것만).
                if let (Kind::Approval { .. }, Some((aid, _, _))) = (&msg.kind, &info.approval) {
                    if *aid == msg.id {
                        let yy = ly + block_h + self.s(2.0);
                        let mut bx = x + self.s(8.0);
                        for (btn, label, primary) in [
                            (Btn::Approve(true), t(Msg::AiAllow), true),
                            (Btn::Approve(false), t(Msg::AiDeny), false),
                        ] {
                            let bw = dc.text_width(label) + self.s(24.0);
                            let r = Rect::new(bx, yy, bw, btn_h);
                            self.paint_button(dc, th, r, label, primary, btn);
                            bx += bw + self.s(8.0);
                        }
                    }
                }
            }
            let extra = if matches!(msg.kind, Kind::Approval { .. })
                && info.approval.as_ref().is_some_and(|a| a.0 == msg.id)
            {
                btn_h + self.s(6.0)
            } else {
                0
            };
            ly += block_h + self.s(10.0) + extra;
        }
        self.lines = lines;
        if self.lines.is_empty() {
            // 좁은 분할 창에서도 잘리지 않게 줄 바꿈(협업 V1 관찰 11-11).
            let msg = t(Msg::AiEmptyHint);
            let mut hy = tr.y + self.s(16.0);
            for para in msg.split('\n') {
                for line in wrap(dc, para, w - self.s(16.0)) {
                    dc.text(x, hy, tr, &line, th.text_dim);
                    hy += line_h;
                }
            }
        }
        dc.pop_clip();
        self.content_h = (ly + self.scroll_y - tr.y).max(0);
        self.bars.paint(
            dc,
            th,
            tr,
            tr.w,
            self.content_h.max(tr.h),
            0,
            self.scroll_y,
            self.scale,
        );
        // ── 작성 영역
        let mut cy = comp_top;
        dc.fill_rect(Rect::new(b.x, cy, b.w, 1), th.border);
        cy += self.s(8.0);
        if !sess.attachments.is_empty() {
            // 칩 = [본문(클릭 = 미리보기)][×](사용자 11-11 · 이미지는 이름 W×H).
            let mut cx = x;
            let xw = dc.text_width("×") + self.s(12.0);
            for (i, a) in sess.attachments.iter().enumerate() {
                let label = nexa_ctl::draw::ellipsize_middle(dc, &a.chip_text(), self.s(220.0));
                let bw = dc.text_width(&label) + self.s(16.0);
                if cx + bw + xw > x + w {
                    break;
                }
                let r = Rect::new(cx, cy, bw + xw, btn_h);
                let body = Rect::new(cx, cy, bw, btn_h);
                let xr = Rect::new(cx + bw, cy, xw, btn_h);
                let ib = self.buttons.len();
                let hot_body = self.hover == Some(ib);
                let hot_x = self.hover == Some(ib + 1);
                dc.fill_round_rect(r, btn_h / 2, th.panel_bg_alt);
                if hot_body {
                    dc.fill_round_rect(r, btn_h / 2, th.sel_bg);
                }
                dc.stroke_round_rect(r, btn_h / 2, th.border, 1.0);
                let ty = dc.text_center_y(r.y, r.h);
                dc.text(body.x + self.s(8.0), ty, body, &label, th.text);
                dc.text(
                    xr.x + self.s(4.0),
                    ty,
                    xr,
                    "×",
                    if hot_x { th.warn } else { th.text_dim },
                );
                self.buttons.push((Btn::PreviewAttachment(i), body));
                self.buttons.push((Btn::RemoveAttachment(i), xr));
                cx += r.w + self.s(6.0);
            }
            cy += btn_h + self.s(6.0);
        }
        // 미리보기 캐시(열린 첨부가 바뀌었을 때 한 번 디코드 · PNG만 그림 · 그 밖은 안내).
        if let Some(i) = self.preview {
            if self.preview_cache.as_ref().is_none_or(|c| c.0 != i) {
                let (img, info) = match sess.attachments.get(i) {
                    Some(a @ super::Attachment::Image { media, bytes, .. }) => {
                        let img = if media.contains("png") {
                            crate::png::decode(bytes)
                                .map(|(w, h, rgba)| Rc::new(IconImage::from_rgba(w, h, rgba)))
                        } else {
                            None
                        };
                        // 아래 줄 = 추가 정보(치수 · 형식 · 용량 · 사용자 11-11 "미리보기에서 크기 표현").
                        let dims = super::image_dims(media, bytes)
                            .map(|(w, h)| format!("{w}×{h} · "))
                            .unwrap_or_default();
                        let info = format!(
                            "{}
{dims}{media} · {} KB",
                            a.label(),
                            bytes.len() / 1024
                        );
                        (img, info)
                    }
                    Some(a @ super::Attachment::Text { body, .. }) => {
                        let head: String = body.lines().take(12).collect::<Vec<_>>().join("\n");
                        (None, format!("{}\n{head}", a.chip_text()))
                    }
                    Some(a) => (None, a.chip_text()),
                    None => (None, String::new()),
                };
                self.preview_cache = Some((i, img, info));
            }
        } else {
            self.preview_cache = None;
        }
        let cr = Rect::new(x, cy, w, comp_h);
        self.composer_rect = cr;
        let mut inv = Invalidations::default();
        self.composer.set_bounds(cr, &mut inv);
        dc.fill_round_rect(cr, self.s(6.0), th.panel_bg);
        self.composer.paint(dc, th);
        dc.stroke_round_rect(
            cr,
            self.s(6.0),
            if self.composer.is_focused() {
                th.accent
            } else {
                th.border
            },
            1.0,
        );
        cy += comp_h + self.s(6.0);
        // 버튼 줄: [+ 첨부 메뉴][🔑 프로필](아이콘 + 툴팁 · 사용자 11-11)  …  [보내기/■]
        let mut bx = x;
        for (btn, which) in [(Btn::AttachMenu, 3u8), (Btn::Profiles, 4u8)] {
            let r = Rect::new(bx, cy, btn_h, btn_h);
            let idx = self.buttons.len();
            let hot = self.hover == Some(idx);
            dc.fill_round_rect(r, self.s(4.0), th.panel_bg_alt);
            dc.stroke_round_rect(r, self.s(4.0), if hot { th.accent } else { th.border }, 1.0);
            if let Some(img) = self.block_icon(which, th.text) {
                let icon = self.s(16.0);
                let dst = Rect::new(r.x + (r.w - icon) / 2, r.y + (r.h - icon) / 2, icon, icon);
                dc.image_scaled(dst, &img, b);
            }
            self.buttons.push((btn, r));
            bx += btn_h + self.s(6.0);
        }
        // [/] 명령(네모 안 슬래시 · 툴팁 = 키 안내).
        {
            let r = Rect::new(bx, cy, btn_h, btn_h);
            let idx = self.buttons.len();
            let hot = self.hover == Some(idx);
            dc.fill_round_rect(r, self.s(4.0), th.panel_bg_alt);
            dc.stroke_round_rect(r, self.s(4.0), if hot { th.accent } else { th.border }, 1.0);
            let tw = dc.text_width("/");
            let ty = dc.text_center_y(r.y, r.h);
            dc.text(r.x + (r.w - tw) / 2, ty, r, "/", th.text);
            self.buttons.push((Btn::Commands, r));
            bx += btn_h + self.s(6.0);
        }
        {
            let label = if info.busy { "■" } else { "➤" };
            let bw = dc.text_width(label) + self.s(20.0);
            let r = Rect::new(x + w - bw, cy, bw, btn_h);
            // 오른쪽부터: [➤/■] [Auto] [모델 · 노력도] [에이전트](글만 · 바꾸기는 패널/`/` 명령) · 남는 자리 = 상태 글.
            self.auto_now = info.auto;
            self.chip_models = info.models.clone();
            self.model_now = info.model.clone();
            self.effort_now = info.effort.clone();
            let tyc = dc.text_center_y(cy, btn_h);
            let sw = self.s(34.0);
            let ar = Rect::new(r.x - sw - self.s(4.0), cy, sw, btn_h);
            let al = t(Msg::AiAuto);
            let alw = dc.text_width(al);
            dc.text(ar.x - alw - self.s(2.0), tyc, b, al, th.text_dim);
            self.paint_switch(dc, th, ar, info.auto, Btn::Auto);
            let mut right = ar.x - alw - self.s(10.0);
            for (k, chip) in [format!("{} · {}", info.model, info.effort), agent]
                .into_iter()
                .enumerate()
            {
                let cw = dc.text_width(&chip) + self.s(12.0);
                let cr = Rect::new(right - cw, cy, cw, btn_h);
                if cr.x < bx + self.s(8.0) {
                    break;
                }
                dc.fill_round_rect(cr, btn_h / 2, th.panel_bg_alt);
                if k == 0 {
                    // 모델 칩 = 버튼(누르면 큰 팝업).
                    let idx = self.buttons.len();
                    let hot = self.hover == Some(idx);
                    dc.stroke_round_rect(
                        cr,
                        btn_h / 2,
                        if hot { th.accent } else { th.border },
                        1.0,
                    );
                    dc.text(cr.x + self.s(6.0), tyc, cr, &chip, th.text);
                    self.buttons.push((Btn::ModelChip, cr));
                } else {
                    dc.text(cr.x + self.s(6.0), tyc, cr, &chip, th.text_dim);
                }
                right = cr.x - self.s(6.0);
            }
            if !info.status.is_empty() && right - bx > self.s(60.0) {
                let shown =
                    nexa_ctl::draw::ellipsize_middle(dc, &info.status, right - bx - self.s(8.0));
                dc.text(bx, tyc, b, &shown, th.text_dim);
            }
            if info.busy {
                self.paint_button(dc, th, r, label, false, Btn::Send);
            } else {
                // 보내기는 Enter와 같다 — 버튼은 submit.
                let idx = self.buttons.len();
                let hot = self.hover == Some(idx);
                let st = nexa_ctl::tokens::State::of(false, hot, self.pressed == Some(idx), true);
                dc.fill_round_rect(r, self.s(4.0), th.accent);
                dc.state_layer(r, th.text, st);
                let tw = dc.text_width(label);
                let ty = dc.text_center_y(r.y, r.h);
                dc.text(
                    r.x + (r.w - tw) / 2,
                    ty,
                    r,
                    label,
                    nexa_ctl::Color(0x00FF_FFFF),
                );
                self.buttons.push((Btn::Send, r));
            }
        }
        cy += btn_h + self.s(4.0);
        let _ = cy;
        // `/` 팝업 자리(위로).
        if !self.slash.is_empty() {
            let row_h = self.s(24.0);
            let h = row_h * self.slash.len() as i32 + self.s(8.0);
            let pw = (w / 2).max(self.s(240.0)).min(w);
            self.slash_rect = Rect::new(cr.x, (cr.y - h - self.s(4.0)).max(b.y), pw, h);
        } else {
            self.slash_rect = Rect::default();
        }
    }

    /// 스위치(Auto) — 버튼 적중 목록에 등록.
    fn paint_switch(&mut self, dc: &mut dyn DrawCtx, th: &Theme, r: Rect, on: bool, btn: Btn) {
        let tw = self.s(30.0);
        let thh = self.s(16.0);
        let tr = Rect::new(r.x + (r.w - tw) / 2, r.y + (r.h - thh) / 2, tw, thh);
        dc.fill_round_rect(tr, thh / 2, if on { th.accent } else { th.border });
        let kn = thh - self.s(4.0);
        let kx = if on {
            tr.right() - kn - self.s(2.0)
        } else {
            tr.x + self.s(2.0)
        };
        dc.fill_ellipse(
            Rect::new(kx, tr.y + self.s(2.0), kn, kn),
            nexa_ctl::Color(0x00FF_FFFF),
        );
        let idx = self.buttons.len();
        if self.hover == Some(idx) {
            dc.fill_round_rect_alpha(r, self.s(4.0), th.text, 0.05);
        }
        self.buttons.push((btn, r));
    }

    /// 폼 칩(값 + 셰브론 · 누르면 큰 팝업) — 버튼 적중 목록에 등록.
    fn paint_chip(&mut self, dc: &mut dyn DrawCtx, th: &Theme, r: Rect, value: &str, btn: Btn) {
        let rad = self.s(4.0);
        let idx = self.buttons.len();
        let hot = self.hover == Some(idx);
        dc.fill_round_rect(r, rad, th.field_bg);
        dc.stroke_round_rect(r, rad, if hot { th.accent } else { th.border }, 1.0);
        let cw = self.s(10.0);
        let inner = Rect::new(r.x + self.s(8.0), r.y, r.w - cw - self.s(20.0), r.h);
        let shown = nexa_ctl::draw::ellipsize_middle(dc, value, inner.w.max(10));
        let ty = dc.text_center_y(r.y, r.h);
        dc.text(inner.x, ty, inner, &shown, th.text);
        nexa_ctl::controls::draw_chevron_down(
            dc,
            Rect::new(r.right() - cw - self.s(6.0), r.y + (r.h - cw) / 2, cw, cw),
            th.text_dim,
        );
        self.buttons.push((btn, r));
    }

    fn paint_button(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        r: Rect,
        label: &str,
        primary: bool,
        btn: Btn,
    ) {
        let idx = self.buttons.len();
        let hot = self.hover == Some(idx);
        let down = self.pressed == Some(idx);
        let st = nexa_ctl::tokens::State::of(false, hot, down, true);
        if primary {
            dc.fill_round_rect(r, self.s(4.0), th.accent);
        } else {
            dc.fill_round_rect(r, self.s(4.0), th.panel_bg_alt);
            dc.stroke_round_rect(r, self.s(4.0), if hot { th.accent } else { th.border }, 1.0);
        }
        dc.state_layer(r, th.text, st);
        let tw = dc.text_width(label);
        let ty = dc.text_center_y(r.y, r.h);
        dc.text(
            r.x + (r.w - tw) / 2,
            ty,
            r,
            label,
            if primary {
                nexa_ctl::Color(0x00FF_FFFF)
            } else {
                th.text
            },
        );
        self.buttons.push((btn, r));
    }

    fn paint_profiles(&mut self, dc: &mut dyn DrawCtx, th: &Theme, info: &ViewInfo) {
        let b = self.bounds;
        let pad = self.s(16.0);
        let x = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(80.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let row_h = lh + self.s(12.0);
        // 안 보이는 상자는 영역을 비운다(클릭·Tab 대상에서 제외 · 11-11 키 상자 결함) — 목록 페이지는 전부.
        let mut inv = Invalidations::default();
        for tb in self.boxes_mut().into_iter().skip(1) {
            tb.set_bounds(Rect::default(), &mut inv);
        }
        if self.page == Page::ProfileEdit {
            self.paint_profile_form(dc, th, info, x, w, lh, row_h);
        } else {
            self.paint_profile_list(dc, th, info, x, w, lh, row_h);
        }
    }

    /// 작은 아이콘 버튼(hover 배경 · 틴트 아이콘 · 툴팁은 `paint_popup`): `which` = `block_icon` 번호 · 6 = ✕ · 7 = ← 글자.
    fn paint_icon_btn(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        r: Rect,
        which: u8,
        btn: Btn,
        accent: bool,
    ) {
        let idx = self.buttons.len();
        let hot = self.hover == Some(idx);
        if hot {
            dc.fill_round_rect(r, self.s(3.0), th.sel_bg);
        }
        let fg = if accent { th.accent } else { th.text_dim };
        match which {
            6 | 7 => {
                let g = if which == 6 { "✕" } else { "←" };
                let gw = dc.text_width(g);
                let ty = dc.text_center_y(r.y, r.h);
                dc.text(r.x + (r.w - gw) / 2, ty, r, g, fg);
            }
            _ => {
                if let Some(img) = self.block_icon(which, fg) {
                    let icon = self.s(14.0);
                    let dst = Rect::new(r.x + (r.w - icon) / 2, r.y + (r.h - icon) / 2, icon, icon);
                    dc.image_scaled(dst, &img, r);
                }
            }
        }
        self.buttons.push((btn, r));
    }

    /// ★ 목록 페이지(사용자 11-11 "목록과 편집 분리 · 추가 버튼 별도 · 아이콘 버튼"): 제목 + [+] · 행 = 이름 ✓ [WIF] · 모델 · 키 상태
    ///   (버튼 앞에서 자기 영역으로 클립 · Alt 전체 보기도 버튼을 덮지 않는다 · 전체 글 = Alt + hover 말풍선) · 행 오른쪽 아이콘 =
    ///   [✓ 사용][✎ 편집][⧉ 복제][✕ 삭제] · 행 클릭 = 상세.
    #[allow(clippy::too_many_arguments)]
    fn paint_profile_list(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        info: &ViewInfo,
        x: i32,
        w: i32,
        lh: i32,
        row_h: i32,
    ) {
        let b = self.bounds;
        let pad = self.s(16.0);
        let mut y = b.y + pad;
        dc.select_font(FontSlot::Message, true);
        let title_h = dc.text_height();
        dc.text(x, y, b, t(Msg::AiProfilesTitle), th.text);
        dc.select_font(FontSlot::Base, false);
        {
            let side = row_h;
            let r = Rect::new(x + w - side, y + (title_h - side).max(0) / 2, side, side);
            self.paint_icon_btn(dc, th, r, 3, Btn::ProfileNew, true);
        }
        y += title_h + self.s(6.0);
        for l in wrap(dc, t(Msg::AiProfilesHint), w) {
            dc.text(x, y, b, &l, th.text_dim);
            y += lh;
        }
        y += self.s(10.0);
        self.p_rows.clear();
        for (i, p) in info.profiles.iter().enumerate() {
            let r = Rect::new(x, y, w, row_h);
            let cur = p.name == info.current_profile;
            if cur {
                dc.fill_round_rect(r, self.s(4.0), th.sel_bg_inactive);
            }
            let keyed = info.keyed.contains(&p.name);
            let line = format!(
                "{}{}{}  ·  {}  ·  {}",
                p.name,
                if cur { " ✓" } else { "" },
                if p.wif.is_some() { " [WIF]" } else { "" },
                p.model,
                t(if keyed {
                    Msg::AiKeyStored
                } else {
                    Msg::AiKeyMissing
                })
            );
            let ty = dc.text_center_y(r.y, r.h);
            let side = row_h - self.s(4.0);
            let mut bx = r.right() - self.s(4.0);
            for (btn, which) in [
                (Btn::ProfileDelete(i), 6u8),
                (Btn::ProfileDup(i), 0),
                (Btn::ProfileEdit(i), 1),
                (Btn::ProfileUse(i), 5),
            ] {
                bx -= side;
                let br = Rect::new(bx, r.y + self.s(2.0), side, side);
                let accent = cur && matches!(btn, Btn::ProfileUse(_));
                self.paint_icon_btn(dc, th, br, which, btn, accent);
                bx -= self.s(2.0);
            }
            let text_r = Rect::new(r.x, r.y, (bx - r.x - self.s(8.0)).max(0), r.h);
            let shown =
                nexa_ctl::draw::ellipsize_middle(dc, &line, (text_r.w - self.s(16.0)).max(20));
            dc.text(r.x + self.s(8.0), ty, text_r, &shown, th.text);
            self.p_rows.push(format!("{line}  ·  {}", p.base_url));
            self.buttons.push((Btn::ProfileRow(i), text_r));
            y += row_h + self.s(2.0);
        }
        if info.profiles.is_empty() {
            dc.text(x, y, b, t(Msg::AiNoProfile), th.text_dim);
            y += lh + self.s(4.0);
        }
        y += self.s(10.0);
        if !info.note.is_empty() {
            for l in wrap(dc, &info.note, w) {
                dc.text(x, y, b, &l, th.accent);
                y += lh;
            }
            y += self.s(4.0);
        }
        let label = t(Msg::AiBack);
        let bw = dc.text_width(label) + self.s(24.0);
        self.paint_button(
            dc,
            th,
            Rect::new(x, y, bw, row_h),
            label,
            false,
            Btn::BackChat,
        );
    }

    /// ★ 상세/편집 페이지: [←](목록으로 · 저장 안 한 것은 버린다) + 제목 · 폼(이름 · 키/WIF · 모델/노력도 칩 · [고급 ▸]) ·
    ///   [저장 + 검증](저장해도 이 페이지에 머문다) · [로그인…](WIF).
    #[allow(clippy::too_many_arguments)]
    fn paint_profile_form(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        info: &ViewInfo,
        x: i32,
        w: i32,
        lh: i32,
        row_h: i32,
    ) {
        let b = self.bounds;
        let pad = self.s(16.0);
        let mut y = b.y + pad;
        let side = row_h;
        self.paint_icon_btn(dc, th, Rect::new(x, y, side, side), 7, Btn::Back, false);
        dc.select_font(FontSlot::Base, true);
        let title = match &self.p_editing {
            Some(n) => tf(Msg::AiProfileEditing, &[n]),
            None => t(Msg::AiProfileNewTitle).to_string(),
        };
        let ty = dc.text_center_y(y, side);
        dc.text(x + side + self.s(8.0), ty, b, &title, th.text);
        dc.select_font(FontSlot::Base, false);
        y += side + self.s(10.0);
        dc.fill_rect(Rect::new(x, y, w, 1), th.border);
        y += self.s(10.0);
        let _ = pad;
        let label_w = self.s(120.0);
        let fw = (w - label_w).min(self.s(520.0));
        let mut inv = Invalidations::default();
        let wif = self.p_wif;
        let adv = self.p_adv;
        self.lh = lh;
        // 인증 방식 토글(API 키 ↔ WIF · Claude 전용 · 110 §14-5).
        {
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, b, t(Msg::AiFieldAuth), th.text_dim);
            let label = t(if wif { Msg::AiAuthWif } else { Msg::AiAuthKey });
            let bw = dc.text_width(label) + self.s(24.0);
            let r = Rect::new(x + label_w, y, bw, row_h);
            self.paint_button(dc, th, r, label, false, Btn::ProfileAuth);
            y += row_h + self.s(6.0);
        }
        // 안 보이는 상자는 영역을 비운다(클릭·Tab 대상에서 제외 · 11-11 키 상자 결함).
        for tb in [
            &mut self.p_url,
            &mut self.p_ws,
            &mut self.p_key,
            &mut self.p_issuer,
            &mut self.p_client,
            &mut self.p_rule,
            &mut self.p_org,
            &mut self.p_sa,
        ] {
            tb.set_bounds(Rect::default(), &mut inv);
        }
        // ★ 간소화(사용자 11-11 "너무 불편"): 이름 · 키(또는 WIF) · 모델/노력도 칩 · [고급 ▸]에 기본 URL·워크스페이스.
        let mut fields: Vec<(Msg, &mut TextBox)> = vec![(Msg::AiFieldName, &mut self.p_name)];
        if wif {
            fields.push((Msg::AiFieldIssuer, &mut self.p_issuer));
            fields.push((Msg::AiFieldClientId, &mut self.p_client));
            fields.push((Msg::AiFieldRule, &mut self.p_rule));
            fields.push((Msg::AiFieldOrg, &mut self.p_org));
            fields.push((Msg::AiFieldSa, &mut self.p_sa));
        } else {
            fields.push((Msg::AiFieldKey, &mut self.p_key));
        }
        if adv {
            fields.push((Msg::AiFieldBaseUrl, &mut self.p_url));
            fields.push((Msg::AiFieldWorkspace, &mut self.p_ws));
        }
        let scale = self.scale;
        let px = |v: f32| (v * scale).round() as i32;
        for (label, tb) in fields {
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, b, t(label), th.text_dim);
            let r = Rect::new(x + label_w, y, fw, row_h);
            tb.set_bounds(r, &mut inv);
            tb.paint(dc, th);
            y += row_h + px(6.0);
        }
        // 모델 · 노력도 칩(큰 팝업) — 한 줄에 둘.
        {
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, b, t(Msg::AiModel), th.text_dim);
            let half = (fw - self.s(8.0)) / 2;
            let mr = Rect::new(x + label_w, y, half, row_h);
            let er = Rect::new(mr.right() + self.s(8.0), y, fw - half - self.s(8.0), row_h);
            let mv = self.p_model.clone();
            let ev = format!("{}: {}", t(Msg::AiEffort), self.p_effort);
            self.paint_chip(dc, th, mr, &mv, Btn::ProfileModel);
            self.paint_chip(dc, th, er, &ev, Btn::ProfileEffort);
            y += row_h + px(6.0);
        }
        // [고급 ▸/▾]
        {
            let label = t(if adv {
                Msg::AiAdvancedOpen
            } else {
                Msg::AiAdvanced
            });
            let bw = dc.text_width(label) + self.s(20.0);
            let r = Rect::new(x + label_w, y, bw, row_h);
            self.paint_button(dc, th, r, label, false, Btn::ProfileAdvanced);
            y += row_h + px(6.0);
        }
        y += self.s(4.0);
        let mut bx = x + label_w;
        let mut btns = vec![(Btn::ProfileSave, t(Msg::AiSaveValidate), true)];
        if wif {
            btns.push((Btn::ProfileLogin, t(Msg::AiLogin), false));
        }
        for (btn, label, primary) in btns {
            let bw = dc.text_width(label) + self.s(24.0);
            let r = Rect::new(bx, y, bw, row_h);
            self.paint_button(dc, th, r, label, primary, btn);
            bx += bw + self.s(8.0);
        }
        y += row_h + self.s(10.0);
        if !self.form_err.is_empty() {
            dc.text(x, y, b, &self.form_err, th.warn);
            y += lh + self.s(4.0);
        }
        if !info.note.is_empty() {
            // 긴 안내(검증 실패 원문)는 줄 바꿈(사용자 11-11 스크린샷 = 한 줄에 잘림).
            for l in wrap(dc, &info.note, w) {
                dc.text(x, y, b, &l, th.accent);
                y += lh;
            }
            y += self.s(4.0);
        }
        let note = t(if wif {
            Msg::AiWifStorageNote
        } else {
            Msg::AiKeyStorageNote
        });
        for l in wrap(dc, note, w) {
            dc.text(x, y, b, &l, th.text_dim);
            y += lh;
        }
    }

    /// 팝업 층(`/` 목록 · 상자 편집 메뉴).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        // 코드 블록 아이콘 버튼 툴팁(hover 즉시 · 도구줄 선례).
        if self.pressed.is_none() {
            if let Some((btn, r)) = self.hover.and_then(|i| self.buttons.get(i)) {
                let tip = match btn {
                    Btn::ProfileUse(_) => Some(Msg::AiUse),
                    Btn::ProfileEdit(_) => Some(Msg::AiEdit),
                    Btn::ProfileDup(_) => Some(Msg::AiDup),
                    Btn::ProfileDelete(_) => Some(Msg::AiSessDelete),
                    Btn::ProfileNew => Some(Msg::AiProfileNew),
                    Btn::Back => Some(Msg::AiBackToList),
                    Btn::CopyCode(_) => Some(Msg::AiCopy),
                    Btn::InsertCode(_) => Some(Msg::AiInsertEditor),
                    Btn::RunCode(_) => Some(Msg::AiRunCode),
                    Btn::GridCode(_) => Some(Msg::AiRunGrid),
                    Btn::AttachMenu => Some(Msg::AiAttachMenuTip),
                    Btn::Profiles => Some(Msg::AiProfiles),
                    Btn::PreviewAttachment(_) => Some(Msg::AiPreviewTip),
                    Btn::RemoveAttachment(_) => Some(Msg::AiRemoveTip),
                    Btn::Commands => Some(Msg::AiComposerKeys),
                    Btn::Auto => Some(Msg::AiTipAuto),
                    Btn::Send => Some(Msg::AiSendTip),
                    Btn::SideSessions => Some(Msg::AiSideSessionsTip),
                    Btn::SideNew => Some(Msg::AiSideNewTip),
                    Btn::ModelChip => Some(Msg::AiPickModel),
                    _ => None,
                };
                if let Some(tip) = tip {
                    draw_tooltip_in(
                        dc,
                        th,
                        *r,
                        (self.bounds.x, self.bounds.right()),
                        t(tip),
                        self.scale,
                    );
                }
            }
        }
        // ★ Alt 전체 보기(nexa-ctl `show_full`)는 목록 행에서 버튼을 덮지 않고(행 글은 자기 영역으로 클립) hover 행의 전체 글을
        //   말풍선으로(사용자 11-11 "펼쳐도 버튼에 가려 못 본다").
        if self.page == Page::Profiles && nexa_ctl::draw::show_full() {
            let row = self
                .hover
                .and_then(|i| self.buttons.get(i))
                .and_then(|(b, r)| match b {
                    Btn::ProfileRow(i)
                    | Btn::ProfileUse(i)
                    | Btn::ProfileEdit(i)
                    | Btn::ProfileDup(i)
                    | Btn::ProfileDelete(i) => Some((*i, *r)),
                    _ => None,
                });
            if let Some((i, r)) = row {
                if let Some(full) = self.p_rows.get(i) {
                    dc.select_font(FontSlot::Base, false);
                    let lh = dc.text_height();
                    let pad = self.s(8.0);
                    let b = self.bounds;
                    let lines = wrap(dc, full, b.w - self.s(24.0) - pad * 2);
                    let tw = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0);
                    let bw = tw + pad * 2;
                    let bh = lh * lines.len() as i32 + pad * 2;
                    let bx =
                        r.x.min(b.right() - bw - self.s(12.0))
                            .max(b.x + self.s(12.0));
                    let by = if r.bottom() + bh + self.s(4.0) <= b.bottom() {
                        r.bottom() + self.s(4.0)
                    } else {
                        r.y - bh - self.s(4.0)
                    };
                    let br = Rect::new(bx, by, bw, bh);
                    dc.shadow(br, th.text, nexa_ctl::tokens::Elevation::Mid, self.s(6.0));
                    dc.fill_round_rect(br, self.s(4.0), th.panel_bg);
                    dc.stroke_round_rect(br, self.s(4.0), th.border, 1.0);
                    let mut yy = by + pad;
                    for l in &lines {
                        dc.text(bx + pad, yy, br, l, th.text);
                        yy += lh;
                    }
                }
            }
        }
        if self.page == Page::Chat && self.attach_menu.is_open() {
            self.attach_menu.paint(dc, th);
        }
        if self.page == Page::Chat {
            if let Some((_, p)) = &self.picker {
                p.paint(dc, th);
            }
        }
        // 첨부 미리보기 오버레이(가운데 · 이미지는 비율 유지 · 안내 글).
        if let (Page::Chat, Some((_, img, info))) = (self.page, self.preview_cache.as_ref()) {
            let b = self.bounds;
            let pad = self.s(12.0);
            dc.select_font(FontSlot::Base, false);
            let lh = dc.text_height();
            let info_lines: Vec<&str> = info.lines().collect();
            let max_w = (b.w * 4 / 5).max(self.s(200.0));
            let max_h = (b.h * 3 / 4).max(self.s(120.0));
            let (iw, ih) = match img {
                Some(im) => {
                    let (w0, h0) = (im.w.max(1) as i32, im.h.max(1) as i32);
                    let avail_h =
                        (max_h - pad * 2 - lh * (info_lines.len() as i32 + 1)).max(self.s(40.0));
                    let avail_w = max_w - pad * 2;
                    let sc = (avail_w as f32 / w0 as f32)
                        .min(avail_h as f32 / h0 as f32)
                        .min(1.0);
                    (((w0 as f32) * sc) as i32, ((h0 as f32) * sc) as i32)
                }
                None => (0, 0),
            };
            let text_w = info_lines
                .iter()
                .map(|l| dc.text_width(l))
                .max()
                .unwrap_or(0)
                .min(max_w - pad * 2);
            let pw = iw.max(text_w).max(self.s(160.0)) + pad * 2;
            let ph = ih
                + lh * (info_lines.len() as i32 + 1)
                + pad * 2
                + if ih > 0 { self.s(6.0) } else { 0 };
            let r = Rect::new(b.x + (b.w - pw) / 2, b.y + (b.h - ph) / 2, pw, ph);
            dc.fill_rect_alpha(b, th.text, 0.25);
            dc.shadow(r, th.text, nexa_ctl::tokens::Elevation::Mid, self.s(8.0));
            dc.fill_round_rect(r, self.s(6.0), th.panel_bg);
            dc.stroke_round_rect(r, self.s(6.0), th.border, 1.0);
            let mut y = r.y + pad;
            if let Some(im) = img {
                dc.image_scaled(Rect::new(r.x + (r.w - iw) / 2, y, iw, ih), im, r);
                y += ih + self.s(6.0);
            }
            for l in &info_lines {
                let shown = nexa_ctl::draw::ellipsize_middle(dc, l, r.w - pad * 2);
                dc.text(r.x + pad, y, r, &shown, th.text);
                y += lh;
            }
            dc.text(r.x + pad, y, r, t(Msg::AiPreviewClose), th.text_dim);
        }
        if self.page == Page::Chat && !self.slash.is_empty() && self.slash_rect.w > 0 {
            let r = self.slash_rect;
            dc.shadow(r, th.text, nexa_ctl::tokens::Elevation::Mid, self.s(4.0));
            dc.fill_round_rect(r, self.s(4.0), th.panel_bg);
            dc.stroke_round_rect(r, self.s(4.0), th.border, 1.0);
            dc.select_font(FontSlot::Base, false);
            let row_h = self.s(24.0);
            let mut y = r.y + self.s(4.0);
            for (k, &i) in self.slash.iter().enumerate() {
                let (name, _, desc) = SLASH[i];
                let rr = Rect::new(r.x + 1, y, r.w - 2, row_h);
                if k == self.slash_sel {
                    dc.fill_rect(rr, th.sel_bg);
                }
                let ty = dc.text_center_y(y, row_h);
                dc.select_font(FontSlot::Base, true);
                dc.text(rr.x + self.s(8.0), ty, rr, name, th.text);
                let nw = dc.text_width(name);
                dc.select_font(FontSlot::Base, false);
                dc.text(rr.x + self.s(16.0) + nw, ty, rr, t(desc), th.text_dim);
                y += row_h;
            }
        }
        self.composer.paint_popup(dc, th);
        if self.page != Page::Chat {
            if let Some((_, p)) = &self.picker {
                p.paint(dc, th);
            }
            for tb in [
                &self.p_name,
                &self.p_url,
                &self.p_ws,
                &self.p_key,
                &self.p_issuer,
                &self.p_client,
                &self.p_rule,
                &self.p_org,
                &self.p_sa,
            ] {
                tb.paint_popup(dc, th);
            }
        }
    }

    /// 헤드리스 덤프(V1).
    pub(crate) fn dump_text(&self, sess: &Session, info: &ViewInfo) -> String {
        let mut s = format!(
            "page={:?} busy={} msgs={} attachments={} composer={:?} approval={} sel={:?}\n",
            self.page,
            info.busy,
            sess.messages.len(),
            sess.attachments.len(),
            self.composer.text(),
            info.approval.as_ref().map(|a| a.1.as_str()).unwrap_or(""),
            self.sel
        );
        for m in &sess.messages {
            let k = match &m.kind {
                Kind::User => "user".to_string(),
                Kind::Assistant => "assistant".to_string(),
                Kind::ToolCall { name, input } => format!("tool_call:{name} {input}"),
                Kind::ToolResult { name, ok } => format!("tool_result:{name}:{ok}"),
                Kind::Approval { tool } => format!("approval:{tool}"),
                Kind::Error => "error".to_string(),
                Kind::Note => "note".to_string(),
            };
            let first = m.text.lines().next().unwrap_or("");
            s.push_str(&format!("  [{k}] {first}\n"));
        }
        s
    }
}

/// 메시지 하나 → 줄(역할 줄 · 본문 줄 바꿈 · 코드 블록 · 마크다운 한 줄 변환) · `who` = 어시스턴트 역할 이름(Claude/Codex).
fn layout_msg(
    dc: &mut dyn DrawCtx,
    m: &ChatMsg,
    w: i32,
    code_i: &mut usize,
    codes: &mut Vec<String>,
    who: &str,
) -> Vec<Line> {
    let mut out = Vec::new();
    let (role, body_kind) = match &m.kind {
        Kind::User => (t(Msg::AiRoleYou).to_string(), LineKind::Text),
        Kind::Assistant => (who.to_string(), LineKind::Text),
        Kind::ToolCall { name, input } => {
            let short: String = input.chars().take(160).collect();
            (format!("⚙ {name} {short}"), LineKind::Dim)
        }
        Kind::ToolResult { name, ok } => (
            format!(
                "{} {}",
                if *ok { "→" } else { "✗" },
                if name.is_empty() { "result" } else { name }
            ),
            LineKind::Dim,
        ),
        Kind::Approval { tool } => (tf(Msg::AiApprovalTitle, &[tool]), LineKind::Text),
        Kind::Error => (t(Msg::AiErrorTitle).to_string(), LineKind::Error),
        Kind::Note => (String::new(), LineKind::Dim),
    };
    if !role.is_empty() {
        let kind = if matches!(m.kind, Kind::User | Kind::Assistant | Kind::Approval { .. }) {
            LineKind::Role
        } else {
            body_kind
        };
        for l in wrap(dc, &role, w) {
            out.push(Line { text: l, kind });
        }
    }
    let text = if matches!(m.kind, Kind::ToolCall { .. }) {
        String::new()
    } else {
        m.text.clone()
    };
    if matches!(m.kind, Kind::ToolResult { .. }) {
        // 결과는 앞 6줄만.
        let short: Vec<&str> = text.lines().take(6).collect();
        for l in short {
            for w_ in wrap(dc, l, w) {
                out.push(Line {
                    text: w_,
                    kind: LineKind::Dim,
                });
            }
        }
        if text.lines().count() > 6 {
            out.push(Line {
                text: "…".into(),
                kind: LineKind::Dim,
            });
        }
        return out;
    }
    let mut in_code = false;
    let mut code_buf = String::new();
    let mut local = 0usize;
    for raw in text.split('\n') {
        if let Some(rest) = raw.trim_start().strip_prefix("```") {
            if in_code {
                codes.push(std::mem::take(&mut code_buf));
                *code_i += 1;
                push_block_result(&mut out, m, local);
                local += 1;
                in_code = false;
            } else {
                in_code = true;
                let lang = rest.trim();
                out.push(Line {
                    text: if lang.is_empty() {
                        "code".into()
                    } else {
                        lang.to_string()
                    },
                    kind: LineKind::CodeHead(*code_i),
                });
            }
            continue;
        }
        if in_code {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(raw);
            // 코드는 자르지 않고 그대로(긴 줄은 넘침 · 복사는 전체).
            out.push(Line {
                text: raw.to_string(),
                kind: LineKind::Code,
            });
        } else if raw.trim().is_empty() {
            out.push(Line {
                text: String::new(),
                kind: LineKind::Blank,
            });
        } else {
            let (text, kind) = if matches!(m.kind, Kind::User | Kind::Assistant) {
                md_line(raw, body_kind)
            } else {
                (raw.to_string(), body_kind)
            };
            // 제목은 굵은 글꼴이 더 넓다 — 조금 좁게 접는다.
            let ww = if kind == LineKind::Head {
                w - w / 10
            } else {
                w
            };
            for l in wrap(dc, &text, ww) {
                out.push(Line { text: l, kind });
            }
        }
    }
    if in_code {
        // 스트리밍 중 닫히지 않은 블록.
        codes.push(code_buf);
        *code_i += 1;
        push_block_result(&mut out, m, local);
    }
    for a in &m.attachments {
        out.push(Line {
            text: format!("📎 {a}"),
            kind: LineKind::Dim,
        });
    }
    out
}

/// ★ 마크다운 한 줄 → 표시 글·종류(110 §6 "마크다운 렌더 = 2차" · 11-11): 제목(`#`) = 굵게 · 목록(`-`/`*`/`+` · `1.`) = `•`/번호 +
/// 들여쓰기(체크박스 ☐/☑) · 인용(`>`) = 흐리게 `│` · 구분선(`---`/`***`) = 선 · 굵게 표식 `**x**`는 벗긴다. 인라인 코드 백틱은
/// 그대로(고정폭 글꼴이 없으니 표식이 뜻을 전한다) · 표(`|`)도 그대로. 코드 블록 안·도구 결과에는 적용하지 않는다.
fn md_line(raw: &str, base: LineKind) -> (String, LineKind) {
    let trimmed = raw.trim_start();
    let pad = " ".repeat((raw.len() - trimmed.len()).min(8));
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        return (strip_bold(trimmed[hashes..].trim()), LineKind::Head);
    }
    let marks = trimmed.chars().filter(|c| *c == '-' || *c == '*').count();
    if marks >= 3
        && (trimmed.chars().all(|c| c == '-' || c == ' ')
            || trimmed.chars().all(|c| c == '*' || c == ' '))
    {
        return ("────────────".into(), LineKind::Dim);
    }
    if trimmed == ">" {
        return (format!("{pad}│"), LineKind::Dim);
    }
    if let Some(rest) = trimmed.strip_prefix("> ") {
        return (format!("{pad}│ {}", strip_bold(rest)), LineKind::Dim);
    }
    for b in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(b) {
            let rest = if let Some(r) = rest.strip_prefix("[ ] ") {
                format!("☐ {r}")
            } else if let Some(r) = rest
                .strip_prefix("[x] ")
                .or_else(|| rest.strip_prefix("[X] "))
            {
                format!("☑ {r}")
            } else {
                rest.to_string()
            };
            return (format!("{pad}• {}", strip_bold(&rest)), base);
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) && trimmed[digits..].starts_with(". ") {
        return (format!("{pad}{}", strip_bold(trimmed)), base);
    }
    (strip_bold(raw), base)
}

/// `**굵게**` 표식 제거(짝이 있을 때만 — SQL `a**2` 같은 홑 표식은 보존).
fn strip_bold(s: &str) -> String {
    if s.matches("**").count() >= 2 {
        s.replace("**", "")
    } else {
        s.to_string()
    }
}

/// 메시지 배치 캐시 키 — 글 길이 + [실행] 결과 길이(결과가 붙으면 다시 배치).
fn msg_key(m: &ChatMsg) -> usize {
    m.text.len() + m.block_results.iter().map(|r| r.1.len() + 1).sum::<usize>()
}

/// [실행] 결과를 코드 상자 안 꼬리 줄로(앞 12줄 · 한 줄 200자 · 실패는 경고색).
fn push_block_result(out: &mut Vec<Line>, m: &ChatMsg, local: usize) {
    let Some((_, text, ok)) = m.block_results.iter().find(|r| r.0 == local) else {
        return;
    };
    let kind = if *ok {
        LineKind::Result
    } else {
        LineKind::ResultErr
    };
    out.push(Line {
        text: format!("── {} ──", t(Msg::AiResultHead)),
        kind,
    });
    for (n, l) in text.lines().enumerate() {
        if n == 12 {
            out.push(Line {
                text: "…".into(),
                kind,
            });
            break;
        }
        out.push(Line {
            text: l.chars().take(200).collect(),
            kind,
        });
    }
}

fn collect_code(text: &str, codes: &mut Vec<String>) {
    let mut in_code = false;
    let mut buf = String::new();
    for raw in text.split('\n') {
        if raw.trim_start().starts_with("```") {
            if in_code {
                codes.push(std::mem::take(&mut buf));
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(raw);
        }
    }
    if in_code {
        codes.push(buf);
    }
}

/// 단어 단위 줄 바꿈(한 단어가 폭을 넘으면 글자 단위).
pub(crate) fn wrap(dc: &mut dyn DrawCtx, text: &str, max_w: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if dc.text_width(&cand) <= max_w || cur.is_empty() && dc.text_width(word) <= max_w {
            cur = cand;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        // 긴 단어는 글자 단위.
        let mut piece = String::new();
        for ch in word.chars() {
            let mut p2 = piece.clone();
            p2.push(ch);
            if dc.text_width(&p2) > max_w && !piece.is_empty() {
                lines.push(std::mem::take(&mut piece));
            }
            piece.push(ch);
        }
        cur = piece;
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    struct W;
    impl DrawCtx for W {
        fn fill_rect(&mut self, _r: Rect, _c: nexa_ctl::Color) {}
        fn text_opaque(
            &mut self,
            _x: i32,
            _y: i32,
            _c: Rect,
            _t: &str,
            _f: nexa_ctl::Color,
            _b: nexa_ctl::Color,
        ) {
        }
        fn text(&mut self, _x: i32, _y: i32, _c: Rect, _t: &str, _f: nexa_ctl::Color) {}
        fn text_width(&mut self, text: &str) -> i32 {
            text.chars().count() as i32 * 8
        }
    }

    #[test]
    fn wraps_and_lays_out_code_blocks() {
        let mut dc = W;
        assert_eq!(wrap(&mut dc, "aaa bbb ccc", 60), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap(&mut dc, "abcdefghij", 40), vec!["abcde", "fghij"]);
        let m = ChatMsg::new(
            Kind::Assistant,
            "Try:\n```sql\nselect 1\nfrom dual\n```\ndone",
        );
        let mut ci = 0;
        let mut codes = Vec::new();
        let lines = layout_msg(&mut dc, &m, 400, &mut ci, &mut codes, "Claude");
        assert_eq!(codes, vec!["select 1\nfrom dual".to_string()]);
        assert!(matches!(lines[0].kind, LineKind::Role));
        assert!(lines
            .iter()
            .any(|l| matches!(l.kind, LineKind::CodeHead(0))));
        assert_eq!(lines.iter().filter(|l| l.kind == LineKind::Code).count(), 2);
        assert_eq!(ci, 1);
    }

    #[test]
    fn char_selection_resolves_and_copies() {
        let mut dc = W;
        // 글자 폭 8: x=10 → 0 · 10+8*3+3 → 3(반 못 미침) · 10+8*3+5 → 4(반 넘음) · 끝 너머 → 길이.
        assert_eq!(AiView::col_at(&mut dc, "abcdef", 10, 0), 0);
        assert_eq!(AiView::col_at(&mut dc, "abcdef", 10, 10 + 24 + 3), 3);
        assert_eq!(AiView::col_at(&mut dc, "abcdef", 10, 10 + 24 + 5), 4);
        assert_eq!(AiView::col_at(&mut dc, "abcdef", 10, 999), 6);
        assert_eq!(AiView::col_at(&mut dc, "", 10, 50), 0);
        let mut v = AiView {
            lines: vec![
                vec![
                    Line {
                        text: "hello world".into(),
                        kind: LineKind::Text,
                    },
                    Line {
                        text: "second line".into(),
                        kind: LineKind::Text,
                    },
                ],
                vec![Line {
                    text: "third".into(),
                    kind: LineKind::Code,
                }],
            ],
            ..Default::default()
        };
        assert_eq!(v.selection_text(), None);
        v.sel = Some(((0, 6), (0, 11)));
        assert_eq!(v.selection_text().as_deref(), Some("world"));
        // 거꾸로 끌어도 같은 범위 · 여러 줄 = 첫 줄 꼬리 + 중간 통째 + 끝 줄 머리.
        v.sel = Some(((2, 3), (0, 6)));
        assert_eq!(
            v.selection_text().as_deref(),
            Some("world\nsecond line\nthi")
        );
        v.sel = Some(((0, 3), (0, 3)));
        assert_eq!(v.selection_text(), None);
    }

    #[test]
    fn profile_form_required_checks() {
        let mut v = AiView::default();
        v.edit_profile(None, false);
        assert_eq!(v.form_check().map(|e| e.0), Some(1), "name required");
        v.p_name.set_text("api#12");
        assert_eq!(v.form_check().map(|e| e.0), Some(1), "name rule");
        v.p_name.set_text("api1");
        assert_eq!(
            v.form_check().map(|e| e.0),
            Some(4),
            "key required for a new profile"
        );
        v.p_key.set_text("sk-ant-x");
        assert!(v.form_check().is_none());
        v.p_url.set_text("api.anthropic.com");
        assert_eq!(v.form_check().map(|e| e.0), Some(2), "url scheme");
        v.p_url.set_text("https://api.anthropic.com");
        v.p_ws.set_text("Default");
        assert_eq!(v.form_check().map(|e| e.0), Some(3), "workspace id format");
        v.p_ws.set_text("wrkspc_0123");
        assert!(v.form_check().is_none());
        v.p_wif = true;
        assert_eq!(v.form_check().map(|e| e.0), Some(5), "wif issuer required");
        assert!(!v.form_ok() && !v.form_err.is_empty());
        assert!(v.p_issuer.is_focused());
    }

    #[test]
    fn markdown_lines() {
        assert_eq!(
            md_line("## Title", LineKind::Text),
            ("Title".into(), LineKind::Head)
        );
        assert_eq!(
            md_line("- [x] done **now**", LineKind::Text),
            ("• ☑ done now".into(), LineKind::Text)
        );
        assert_eq!(md_line("  * sub", LineKind::Text).0, "  • sub");
        assert_eq!(md_line("2. two", LineKind::Text).0, "2. two");
        assert_eq!(
            md_line("> quoted", LineKind::Text),
            ("│ quoted".into(), LineKind::Dim)
        );
        assert_eq!(md_line("---", LineKind::Text).1, LineKind::Dim);
        // 홑 `**`·제목 아닌 `#`·표는 그대로.
        assert_eq!(
            md_line("select a**2 from t", LineKind::Text).0,
            "select a**2 from t"
        );
        assert_eq!(md_line("#tag", LineKind::Text).0, "#tag");
        assert_eq!(md_line("| a | b |", LineKind::Text).0, "| a | b |");
        let m = ChatMsg::new(Kind::Assistant, "# H\n- one\n```sql\n- not a list\n```");
        let mut dc = W;
        let (mut ci, mut codes) = (0, Vec::new());
        let lines = layout_msg(&mut dc, &m, 400, &mut ci, &mut codes, "Codex");
        assert_eq!(lines[0].text, "Codex");
        assert_eq!(lines[1].kind, LineKind::Head);
        assert_eq!(lines[2].text, "• one");
        assert!(lines
            .iter()
            .any(|l| l.kind == LineKind::Code && l.text == "- not a list"));
    }

    #[test]
    fn slash_popup_and_submit() {
        let mut v = AiView::default();
        v.set_composer_text("/mo");
        assert_eq!(v.slash.len(), 2, "mode · model");
        v.on_event(&InputEvent::Key {
            key: CtlKey::Enter,
            shift: false,
            primary: false,
        });
        assert_eq!(v.composer_text(), "/mode ");
        v.set_composer_text("/clear");
        v.on_event(&InputEvent::Key {
            key: CtlKey::Enter,
            shift: false,
            primary: false,
        });
        assert_eq!(v.take_actions(), vec![ViewAction::Slash("/clear".into())]);
        v.set_composer_text("hello");
        v.composer.set_focused(true);
        v.on_event(&InputEvent::Key {
            key: CtlKey::Enter,
            shift: false,
            primary: false,
        });
        assert_eq!(v.take_actions(), vec![ViewAction::Send("hello".into())]);
        assert!(v.composer_text().is_empty());
    }
}
