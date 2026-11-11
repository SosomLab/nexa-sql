//! ★ Claude 채팅 뷰 탭(docs/110 §14-2 · 사용자 11-11 "하단은 VS Code Claude Code 확장 화면처럼"): 편집기 자리에 그리는 대화 화면 —
//! 스트리밍 대본(글 · 코드 블록[복사·편집기에 넣기] · 도구 호출/결과 · 승인 카드[허용/거부] · 오류) · 아래 작성 상자(Enter = 보내기 ·
//! Shift+Enter = 줄 바꿈 · `/` 명령 팝업) · 첨부 칩(파일 · 클립보드 · 조회 결과) · [보내기]/[■]. 두 번째 페이지 = **프로필 관리**
//! (다중 API 프로필 · 키는 마스킹 입력 → 볼트 · [검증] = `/v1/models`).

use super::wif::Wif;
use super::{ChatMsg, Kind, Mode, Profile, Provider, Session};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Page {
    #[default]
    Chat,
    Profiles,
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
    /// 프로필 페이지 안내 줄(검증 결과 · 저장됨 …).
    pub note: String,
    /// 상태 줄(스트리밍 중 "…" · 마지막 턴 소요·토큰).
    pub status: String,
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
    ProfileUse(String),
    ProfileDelete(String),
    /// 저장(프로필 · 새 키 = Some · 비우면 기존 유지 · 편집 전 이름 = Some이면 이름이 바뀌었을 때 rename).
    ProfileSave(Profile, Option<String>, Option<String>),
    /// 복제(목록 인덱스).
    ProfileDup(String),
    ProfileValidate(Profile, Option<String>),
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
    AttachFile,
    AttachClip,
    Profiles,
    RemoveAttachment(usize),
    Approve(bool),
    CopyCode(usize),
    InsertCode(usize),
    ProfileUse(usize),
    ProfileEdit(usize),
    ProfileDelete(usize),
    ProfileDup(usize),
    ProfileNew,
    ProfileSave,
    ProfileValidate,
    /// 인증 방식 토글(API 키 ↔ WIF).
    ProfileAuth,
    ProfileLogin,
    Back,
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
    /// 코드 블록 본문(버튼 동작용).
    code_blocks: Vec<String>,
    transcript: Rect,
    composer_rect: Rect,
    scroll_y: i32,
    stick_bottom: bool,
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
    p_model: TextBox,
    p_effort: TextBox,
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
            transcript: Rect::default(),
            composer_rect: Rect::default(),
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
            p_model: tb(super::DEFAULT_MODEL),
            p_effort: tb(super::DEFAULT_EFFORT),
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
            &mut self.p_model,
            &mut self.p_effort,
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

    /// 편집 중인 기존 프로필 이름(새 프로필이면 None).
    pub(crate) fn editing_name(&self) -> Option<&str> {
        self.p_editing.as_deref()
    }

    /// 프로필 편집 폼 채우기(None = 새 프로필).
    pub(crate) fn edit_profile(&mut self, p: Option<&Profile>, has_key: bool) {
        let p = p.cloned().unwrap_or_else(|| Profile::new(""));
        self.p_editing = (!p.name.is_empty()).then(|| p.name.clone());
        self.p_name.set_text(&p.name);
        self.p_url.set_text(&p.base_url);
        self.p_ws.set_text(&p.workspace);
        self.p_model.set_text(&p.model);
        self.p_effort.set_text(&p.effort);
        self.p_key.set_text("");
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
        let effort = self.p_effort.text().trim().to_string();
        if !effort.is_empty() && !super::EFFORTS.contains(&effort.as_str()) {
            return Some((5, t(Msg::AiEffortRule).to_string()));
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
                    return Some((7 + i, tf(Msg::AiRequired, &[t(label)])));
                }
            }
        } else if !self.p_has_key && self.p_key.text().trim().is_empty() {
            return Some((6, tf(Msg::AiRequired, &[t(Msg::AiFieldKey)])));
        }
        None
    }

    /// 점검 통과면 `true` · 아니면 안내 글 + 문제 상자에 포커스.
    fn form_ok(&mut self) -> bool {
        match self.form_check() {
            None => {
                self.form_err.clear();
                true
            }
            Some((i, msg)) => {
                self.form_err = msg;
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
        let model = self.p_model.text().trim().to_string();
        if !model.is_empty() {
            p.model = model;
        }
        let effort = self.p_effort.text().trim().to_string();
        if !effort.is_empty() {
            p.effort = effort;
        }
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
    pub(crate) fn set_provider(&mut self, p: Provider) {
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

    pub(crate) fn take_actions(&mut self) -> Vec<ViewAction> {
        std::mem::take(&mut self.actions)
    }

    pub(crate) fn reset_scroll_to_bottom(&mut self) {
        self.stick_bottom = true;
    }

    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        if self.page == Page::Profiles {
            for tb in [
                &mut self.p_name,
                &mut self.p_url,
                &mut self.p_ws,
                &mut self.p_model,
                &mut self.p_effort,
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
        self.composer.popup_open() || !self.slash.is_empty()
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
    #[cfg(test)]
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
        if self.page == Page::Profiles {
            return self.on_event_profiles(ev);
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
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                let changed = h != self.hover;
                self.hover = h;
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
                let cur = self.composer.bounds();
                // 작성 상자 위면 상자 스크롤 · 아니면 대본.
                let _ = cur;
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
            Btn::AttachFile => self.actions.push(ViewAction::AttachFile),
            Btn::AttachClip => self.actions.push(ViewAction::AttachClipboard),
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
            Btn::ProfileValidate => {
                if self.form_ok() {
                    let (p, key) = self.form_profile();
                    self.actions.push(ViewAction::ProfileValidate(p, key));
                }
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
            Btn::Back => self.actions.push(ViewAction::ShowPage(Page::Chat)),
        }
    }

    fn on_event_profiles(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
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
                self.actions.push(ViewAction::ShowPage(Page::Chat));
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
        let mut code_i = 0usize;
        let who = match sess.provider {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        };
        for (i, m) in sess.messages.iter().enumerate() {
            let need = self.lines.get(i).is_none() || self.lens[i] != m.text.len();
            if need {
                let lines = layout_msg(dc, m, w, &mut code_i, &mut self.code_blocks, who);
                if i < self.lines.len() {
                    self.lines[i] = lines;
                    self.lens[i] = m.text.len();
                } else {
                    self.lines.push(lines);
                    self.lens.push(m.text.len());
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
        }
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
        if self.page == Page::Profiles {
            self.paint_profiles(dc, th, info);
            return;
        }
        let pad = self.s(12.0);
        let x = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(80.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let line_h = lh + self.s(4.0);
        // ── 머리줄: 모드 · 모델 · 프로필 · 상태
        let mut y = b.y + self.s(8.0);
        let head = format!(
            "{}  ·  {}  ·  {}{}",
            match sess.provider {
                super::Provider::Claude => t(Msg::AiProviderClaude),
                super::Provider::Codex => t(Msg::AiProviderCodex),
            },
            match sess.mode {
                Mode::Api => t(Msg::AiModeApi),
                Mode::Cli => t(Msg::AiModeCli),
            },
            sess.model,
            if (sess.mode == Mode::Api || sess.provider == super::Provider::Codex)
                && !sess.profile.is_empty()
            {
                format!("  ·  {}", sess.profile)
            } else {
                String::new()
            }
        );
        dc.text(x, y, b, &head, th.text_dim);
        if !info.status.is_empty() {
            let sw = dc.text_width(&info.status);
            dc.text(b.right() - pad - sw, y, b, &info.status, th.text_dim);
        }
        y += lh + self.s(6.0);
        dc.fill_rect(Rect::new(b.x, y, b.w, 1), th.border);
        y += 1;
        let top = y;
        // ── 작성 영역 높이(아래에서 위로 잡는다)
        let text = self.composer.text();
        let n_lines = text.split('\n').count().clamp(1, 8) as i32;
        let comp_h = n_lines * self.composer.line_h().max(lh) + self.s(14.0);
        let btn_h = lh + self.s(10.0);
        let chips_h = if sess.attachments.is_empty() {
            0
        } else {
            btn_h + self.s(6.0)
        };
        let hint_h = lh + self.s(4.0);
        let bottom_h = comp_h + btn_h + chips_h + hint_h + self.s(24.0);
        let comp_top = b.bottom() - bottom_h;
        self.transcript = Rect::new(b.x, top, b.w, (comp_top - top).max(0));
        // ── 대본
        self.rebuild_lines(dc, sess, w - self.s(16.0));
        let total: i32 = self
            .lines
            .iter()
            .map(|ls| ls.len() as i32 * line_h + self.s(10.0))
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
        let lines = std::mem::take(&mut self.lines);
        for (mi, ls) in lines.iter().enumerate() {
            let block_h = ls.len() as i32 * line_h;
            let msg = &sess.messages[mi];
            if ly + block_h >= tr.y && ly <= tr.bottom() {
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
                                .take_while(|q| q.kind == LineKind::Code)
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
                            // [복사] [편집기에 넣기]
                            let labels = [
                                (Btn::CopyCode(*ci), t(Msg::AiCopy)),
                                (Btn::InsertCode(*ci), t(Msg::AiInsertEditor)),
                            ];
                            let mut bx = cr.right() - self.s(6.0);
                            for (btn, label) in labels.iter().rev() {
                                let bw = dc.text_width(label) + self.s(12.0);
                                bx -= bw;
                                let r = Rect::new(bx, yy + self.s(1.0), bw, line_h);
                                let idx = self.buttons.len();
                                let hot = self.hover == Some(idx);
                                if hot {
                                    dc.fill_round_rect(r, self.s(3.0), th.sel_bg);
                                }
                                dc.text(
                                    r.x + self.s(6.0),
                                    yy + self.s(2.0),
                                    tr,
                                    label,
                                    if hot { th.text } else { th.accent },
                                );
                                self.buttons.push((*btn, r));
                                bx -= self.s(4.0);
                            }
                        }
                        LineKind::Code => {
                            dc.text(code_x, yy + self.s(2.0), tr, &l.text, th.syn_string);
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
            let msg = t(Msg::AiEmptyHint);
            for (i, line) in msg.split('\n').enumerate() {
                dc.text(
                    x,
                    tr.y + self.s(16.0) + i as i32 * line_h,
                    tr,
                    line,
                    th.text_dim,
                );
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
            let mut cx = x;
            for (i, a) in sess.attachments.iter().enumerate() {
                let label = format!(
                    "{} ×",
                    nexa_ctl::draw::ellipsize_middle(dc, a.label(), self.s(160.0))
                );
                let bw = dc.text_width(&label) + self.s(16.0);
                if cx + bw > x + w {
                    break;
                }
                let r = Rect::new(cx, cy, bw, btn_h);
                let idx = self.buttons.len();
                let hot = self.hover == Some(idx);
                dc.fill_round_rect(r, btn_h / 2, if hot { th.sel_bg } else { th.panel_bg_alt });
                dc.stroke_round_rect(r, btn_h / 2, th.border, 1.0);
                let ty = dc.text_center_y(r.y, r.h);
                dc.text(r.x + self.s(8.0), ty, r, &label, th.text);
                self.buttons.push((Btn::RemoveAttachment(i), r));
                cx += bw + self.s(6.0);
            }
            cy += btn_h + self.s(6.0);
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
        // 버튼 줄: [파일] [클립보드] [프로필]  …  [보내기/■]
        let mut bx = x;
        for (btn, label) in [
            (Btn::AttachFile, t(Msg::AiAttachFile)),
            (Btn::AttachClip, t(Msg::AiAttachClipboard)),
            (Btn::Profiles, t(Msg::AiProfiles)),
        ] {
            let bw = dc.text_width(label) + self.s(20.0);
            let r = Rect::new(bx, cy, bw, btn_h);
            self.paint_button(dc, th, r, label, false, btn);
            bx += bw + self.s(6.0);
        }
        {
            let label = if info.busy { "■" } else { t(Msg::AiSend) };
            let bw = dc.text_width(label) + self.s(28.0);
            let r = Rect::new(x + w - bw, cy, bw, btn_h);
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
        dc.text(x, cy, b, t(Msg::AiComposerKeys), th.text_dim);
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
        dc.select_font(FontSlot::Message, true);
        let mut y = b.y + pad;
        dc.text(x, y, b, t(Msg::AiProfilesTitle), th.text);
        y += dc.text_height() + self.s(6.0);
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let row_h = lh + self.s(12.0);
        dc.text(x, y, b, t(Msg::AiProfilesHint), th.text_dim);
        y += lh + self.s(10.0);
        // 목록
        for (i, p) in info.profiles.iter().enumerate() {
            let r = Rect::new(x, y, w, row_h);
            let cur = p.name == info.current_profile;
            if cur {
                dc.fill_round_rect(r, self.s(4.0), th.sel_bg_inactive);
            }
            let keyed = info.keyed.contains(&p.name);
            let line = format!(
                "{}{}{}  ·  {}  ·  {}  ·  {}",
                p.name,
                if cur { " ✓" } else { "" },
                if p.wif.is_some() { " [WIF]" } else { "" },
                p.model,
                nexa_ctl::draw::ellipsize_middle(dc, &p.base_url, self.s(220.0)),
                t(if keyed {
                    Msg::AiKeyStored
                } else {
                    Msg::AiKeyMissing
                })
            );
            let ty = dc.text_center_y(r.y, r.h);
            let btns = [
                (Btn::ProfileDelete(i), t(Msg::AiSessDelete)),
                (Btn::ProfileDup(i), t(Msg::AiDup)),
                (Btn::ProfileEdit(i), t(Msg::AiEdit)),
                (Btn::ProfileUse(i), t(Msg::AiUse)),
            ];
            let mut bx = r.right() - self.s(4.0);
            for (btn, label) in btns {
                let bw = dc.text_width(label) + self.s(16.0);
                bx -= bw;
                let br = Rect::new(bx, r.y + self.s(3.0), bw, row_h - self.s(6.0));
                self.paint_button(dc, th, br, label, false, btn);
                bx -= self.s(4.0);
            }
            let shown =
                nexa_ctl::draw::ellipsize_middle(dc, &line, (bx - r.x - self.s(16.0)).max(20));
            dc.text(r.x + self.s(8.0), ty, r, &shown, th.text);
            y += row_h + self.s(2.0);
        }
        if info.profiles.is_empty() {
            dc.text(x, y, b, t(Msg::AiNoProfile), th.text_dim);
            y += lh + self.s(4.0);
        }
        {
            let label = t(Msg::AiProfileNew);
            let bw = dc.text_width(label) + self.s(20.0);
            let r = Rect::new(x, y + self.s(4.0), bw, row_h);
            self.paint_button(dc, th, r, label, false, Btn::ProfileNew);
            y += row_h + self.s(14.0);
        }
        dc.fill_rect(Rect::new(x, y, w, 1), th.border);
        y += self.s(10.0);
        // 폼
        dc.select_font(FontSlot::Base, true);
        dc.text(
            x,
            y,
            b,
            &match &self.p_editing {
                Some(n) => tf(Msg::AiProfileEditing, &[n]),
                None => t(Msg::AiProfileNewTitle).to_string(),
            },
            th.text,
        );
        dc.select_font(FontSlot::Base, false);
        y += lh + self.s(8.0);
        let label_w = self.s(120.0);
        let fw = (w - label_w).min(self.s(520.0));
        let mut inv = Invalidations::default();
        let wif = self.p_wif;
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
            &mut self.p_key,
            &mut self.p_issuer,
            &mut self.p_client,
            &mut self.p_rule,
            &mut self.p_org,
            &mut self.p_sa,
        ] {
            tb.set_bounds(Rect::default(), &mut inv);
        }
        let mut fields: Vec<(Msg, &mut TextBox)> = vec![
            (Msg::AiFieldName, &mut self.p_name),
            (Msg::AiFieldBaseUrl, &mut self.p_url),
            (Msg::AiFieldWorkspace, &mut self.p_ws),
            (Msg::AiModel, &mut self.p_model),
            (Msg::AiEffort, &mut self.p_effort),
        ];
        if wif {
            fields.push((Msg::AiFieldIssuer, &mut self.p_issuer));
            fields.push((Msg::AiFieldClientId, &mut self.p_client));
            fields.push((Msg::AiFieldRule, &mut self.p_rule));
            fields.push((Msg::AiFieldOrg, &mut self.p_org));
            fields.push((Msg::AiFieldSa, &mut self.p_sa));
        } else {
            fields.push((Msg::AiFieldKey, &mut self.p_key));
        }
        let has_key = self.p_has_key;
        let scale = self.scale;
        let px = |v: f32| (v * scale).round() as i32;
        for (label, tb) in fields {
            let ty = dc.text_center_y(y, row_h);
            dc.text(x, ty, b, t(label), th.text_dim);
            let r = Rect::new(x + label_w, y, fw, row_h);
            tb.set_bounds(r, &mut inv);
            tb.paint(dc, th);
            if label == Msg::AiFieldKey && has_key {
                dc.text(r.right() + px(8.0), ty, b, t(Msg::AiKeyKeep), th.text_dim);
            }
            y += row_h + px(6.0);
        }
        y += self.s(4.0);
        let mut bx = x + label_w;
        let mut btns = vec![(Btn::ProfileSave, t(Msg::AiSave), true)];
        if wif {
            btns.push((Btn::ProfileLogin, t(Msg::AiLogin), false));
        }
        btns.push((Btn::ProfileValidate, t(Msg::AiValidate), false));
        btns.push((Btn::Back, t(Msg::AiBack), false));
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
            dc.text(x, y, b, &info.note, th.accent);
            y += lh + self.s(4.0);
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
        if self.page == Page::Profiles {
            for tb in [
                &self.p_name,
                &self.p_url,
                &self.p_ws,
                &self.p_model,
                &self.p_effort,
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
            "page={:?} busy={} msgs={} attachments={} composer={:?} approval={}\n",
            self.page,
            info.busy,
            sess.messages.len(),
            sess.attachments.len(),
            self.composer.text(),
            info.approval.as_ref().map(|a| a.1.as_str()).unwrap_or("")
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
    for raw in text.split('\n') {
        if let Some(rest) = raw.trim_start().strip_prefix("```") {
            if in_code {
                codes.push(std::mem::take(&mut code_buf));
                *code_i += 1;
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
    fn profile_form_required_checks() {
        let mut v = AiView::default();
        v.edit_profile(None, false);
        assert_eq!(v.form_check().map(|e| e.0), Some(1), "name required");
        v.p_name.set_text("api#12");
        assert_eq!(v.form_check().map(|e| e.0), Some(1), "name rule");
        v.p_name.set_text("api1");
        assert_eq!(
            v.form_check().map(|e| e.0),
            Some(6),
            "key required for a new profile"
        );
        v.p_key.set_text("sk-ant-x");
        assert!(v.form_check().is_none());
        v.p_url.set_text("api.anthropic.com");
        assert_eq!(v.form_check().map(|e| e.0), Some(2), "url scheme");
        v.p_url.set_text("https://api.anthropic.com");
        v.p_effort.set_text("ultra");
        assert_eq!(v.form_check().map(|e| e.0), Some(5), "effort choice");
        v.p_effort.set_text("high");
        v.p_wif = true;
        assert_eq!(v.form_check().map(|e| e.0), Some(7), "wif issuer required");
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
