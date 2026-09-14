//! 접속 창(별도 창 · Golden "Database Login" 차용 · DR-23 · T-31 · 사용자 09-14).
//!
//! 기본 = **로그인 목록**만(저장 프로필 · 필터 · 이름/종류/사용자/대상) + New/Edit/Delete/Close.
//! New/Edit를 누르면 오른쪽에서 **상세 폼**([`ConnectPanel`] — DB 종류·호스트/포트·DB·사용자·비밀번호·프로필 · Test/Connect/Save · 상태)이
//! 슬라이딩해 들어온다(≈200ms ease-out · Esc = 접기). 목록은 그만큼 좁아진다.
//! 행 클릭 = 폼에 채움 · 더블클릭 = 바로 접속 · 접속되면 창이 닫힌다. `Ctrl/⌘+L` · 툴바 ⇄ · Run ▸ Connect로 연다.
//! 창 골격은 로그 창과 같다(winit + softbuffer + nexa-ctl 래스터). I/O(저장소·워커)는 전부 호스트 몫 — [`ConnWinAction`]으로 요청.

use crate::connect::{ConnectPanel, PanelAction};
use crate::probe::{self, ProbeEntry, ProbeHub, ProbePolicy, ProbeReq, ProbeStatus};
use nexa_ctl::draw::{draw_tooltip, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::tokens::{hover_alpha, HoverFade};
use nexa_ctl::{
    Button, Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use nsql_vault::{Profile, Vault};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
pub(crate) enum ConnWinAction {
    /// `RedrawRequested` — 호스트가 폰트·테마를 넘겨 [`ConnWin::paint`]를 부른다.
    Paint,
    /// 폼의 요청(접속·테스트·저장·프로필 로드) — 호스트 `handle_panel_action`.
    Panel(PanelAction),
    /// 목록 더블클릭 · 행의 접속 버튼 — 저장소에서 읽어 폼에 채우고 바로 접속(성공 시 활성 탭에 적용 · 창 닫힘).
    Login(String),
    /// 행의 테스트 버튼 — 접속만 해 보고 끊는다(결과는 버튼 위에 표시 · 버튼은 계속 누를 수 있다 · 사용자 09-14).
    TestProfile(String),
    /// 목록에서 프로필 삭제.
    Delete(String),
}

/// 행 테스트 버튼 위에 얹는 마지막 결과(사용자 09-14 — 형태·기능은 그대로, 표시만 바뀐다).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestMark {
    Testing,
    Ok,
    Failed,
}

/// 행 접속 버튼의 상태색(사용자 09-14): 없음 = 어두운 회색 · 접속 중 = 파랑 · 접속됨 = 초록(잠시 뒤 창 닫힘).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectMark {
    Connecting,
    Connected,
}

/// 행 안의 아이콘 버튼(신호등 다음 두 칸).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowBtn {
    Test,
    Connect,
}

/// 고정 폭 아이콘 열 수 — 신호등 · 테스트 · 접속.
const ICON_COLS: i32 = 3;

/// 툴팁 대상 — 아이콘 열(0 신호등 · 1 테스트 · 2 접속) × 헤더(None) 또는 행.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TipTarget {
    col: i32,
    row: Option<usize>,
}

/// 아이콘 위에 이만큼 머물면 툴팁(사용자 09-14).
const TIP_MS: u128 = 600;

/// 텍스트 열(원본 index) — 이름 · 종류 · 사용자 · 대상.
const TEXT_COLS: usize = 4;
/// 첫 배치의 폭 비율(사용자가 폭을 조절하기 전까지 창 폭을 따라간다).
const COL_FRACS: [f32; TEXT_COLS] = [0.22, 0.16, 0.20, 0.42];
const MIN_COL_W: i32 = 24;

/// 행의 열 텍스트(그리기·정렬·필터 공용).
fn cell_of(p: &Profile, col: usize) -> String {
    match col {
        0 => p.name.clone(),
        1 => p
            .spec
            .dialect
            .map(|d| d.display_name())
            .unwrap_or("")
            .to_string(),
        2 => p.spec.user.clone().unwrap_or_default(),
        _ => target_of(p),
    }
}

/// 헤더 클릭 정렬 키 갱신(결과 그리드와 같은 규칙): 일반 클릭 = 단일 키 3단(▲ → ▼ → 해제) ·
/// Shift = 결합 키 추가/토글(dir2 방식).
fn toggle_sort_key(keys: &mut Vec<(usize, bool)>, col: usize, additive: bool) {
    let pos = keys.iter().position(|(c, _)| *c == col);
    match (additive, pos) {
        (false, Some(0)) if keys.len() == 1 => {
            if keys[0].1 {
                keys[0].1 = false;
            } else {
                keys.clear();
            }
        }
        (false, _) => *keys = vec![(col, true)],
        (true, Some(i)) => {
            if keys[i].1 {
                keys[i].1 = false;
            } else {
                keys.remove(i);
            }
        }
        (true, None) => keys.push((col, true)),
    }
}

/// 결합 정렬(안정 · 대소문자 무관 · 빈 값은 뒤) — 표시 인덱스 벡터만 재배열.
fn sort_shown(profiles: &[Profile], shown: &mut [usize], keys: &[(usize, bool)]) {
    if keys.is_empty() {
        return;
    }
    let key_of = |i: usize, col: usize| cell_of(&profiles[i], col).to_lowercase();
    shown.sort_by(|&a, &b| {
        use std::cmp::Ordering;
        for (col, asc) in keys {
            let (ka, kb) = (key_of(a, *col), key_of(b, *col));
            let o = match (ka.is_empty(), kb.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => ka.cmp(&kb),
            };
            if o != Ordering::Equal {
                return if *asc { o } else { o.reverse() };
            }
        }
        Ordering::Equal
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WFocus {
    Panel,
    Filter,
    /// 목록(행 선택 · ↑↓ · Enter 접속 · Delete).
    List,
    /// 상단 버튼 하나(`focus_btn`) — 마지막으로 누른 버튼이 테두리를 가진다(포커스 규칙 · 사용자 09-14).
    Button,
}

pub(crate) struct ConnWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    started: Instant,
    pub(crate) panel: ConnectPanel,
    profiles: Vec<Profile>,
    /// 필터 통과한 profiles index.
    shown: Vec<usize>,
    /// 선택(shown 기준 위치).
    sel: Option<usize>,
    hover: Option<usize>,
    /// 마우스가 올라간 행 아이콘 버튼.
    hover_btn: Option<(usize, RowBtn)>,
    /// 호버 행의 서서히 진해지는 강조(nexa-clip과 같은 `HoverFade` · 진입 = `grid.hover_fade`).
    hover_fade: HoverFade,
    /// 텍스트 열 폭(px · 원본 index) — 비어 있으면 배치 때 비율로 채운다.
    col_w: Vec<i32>,
    /// 사용자가 폭을 조절한 뒤엔 창 폭을 따라가지 않는다.
    col_w_manual: bool,
    /// 결합 정렬 키(원본 열 · 오름차순).
    sort_keys: Vec<(usize, bool)>,
    /// 헤더 경계 드래그 = 폭 조절(열 · 시작 x · 시작 폭).
    hdr_resize: Option<(usize, i32, i32)>,
    /// 표시 순서 → 원본 열(헤더 DnD로 이동 · 사용자 09-14).
    col_order: Vec<usize>,
    /// 헤더 드래그(표시 위치 · 시작 x · 현재 x · 4px 이상 움직임 · Shift) — 움직였으면 이동, 아니면 정렬(결과 그리드와 동일).
    hdr_drag: Option<(usize, i32, i32, bool, bool)>,
    /// 지금 커서가 폭 조절 모양인가(바뀔 때만 set_cursor).
    resize_cursor: bool,
    /// 상단 버튼 4개의 라벨 실측 폭(px · 페인트 때 현재 언어로 잰다 — 폭은 라벨에 맞춘다 · 사용자 09-14).
    btn_text_w: [i32; 4],
    /// 목록 스크롤(픽셀).
    scroll_y: i32,
    scroll_x: i32,
    /// 오버레이 스크롤바(결과 그리드와 같은 nexa-ctl 공용 · 축별 표시).
    bars: ScrollBars,
    /// 프로필별 마지막 테스트 결과(행 버튼 표시).
    test_marks: HashMap<String, TestMark>,
    /// 프로필별 접속 버튼 상태(접속 중/접속됨).
    conn_marks: HashMap<String, ConnectMark>,
    /// 이 시각에 창을 닫는다(접속 성공 초록을 잠시 보여 준 뒤).
    close_at: Option<Instant>,
    /// 툴팁 대상 + 머물기 시작 시각(아이콘 열·행 버튼 위 · 다국어 · 사용자 09-14).
    tip: Option<(TipTarget, Instant)>,
    filter: TextBox,
    btn_new: Button,
    btn_edit: Button,
    btn_delete: Button,
    btn_close: Button,
    list: Rect,
    row_h: i32,
    focus: WFocus,
    /// `WFocus::Button`일 때 어느 버튼(0 New · 1 Details · 2 Delete · 3 Close).
    focus_btn: usize,
    last_click: Option<(usize, Instant)>,
    /// 상세 폼 열림 정도 0.0(닫힘)~1.0(열림) — 슬라이딩 애니메이션 현재값.
    detail_t: f32,
    /// (시작 시각 · from · to) — 진행 중 애니메이션.
    anim: Option<(Instant, f32, f32)>,
    /// 서버 신호등(프로필 이름 → 상태·재시도) — [`crate::probe`].
    probes: HashMap<String, ProbeEntry>,
    hub: Option<ProbeHub>,
    /// 한 번 이상 접속 성공한 프로필(대상 집합).
    connected: HashSet<String>,
    policy: ProbePolicy,
    /// 지금 세션이 붙어 있는 프로필(실행 중 접속성 오류 → 이 서버를 즉시 재프로브).
    active: Option<String>,
}

const SLIDE_MS: f32 = 200.0;

const PANEL_W: f32 = 320.0;
/// 기본 창 크기(목록만 · 사용자 캡처 09-14) — New/Edit 시 오른쪽으로 PANEL_W만큼 커진다.
const BASE_W: f32 = 640.0;
const BASE_H: f32 = 520.0;
const DBLCLICK_MS: u128 = 400;

impl ConnWin {
    pub(crate) fn new(panel: ConnectPanel) -> Self {
        let mut w = ConnWin {
            window: None,
            ctx: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            started: Instant::now(),
            panel,
            profiles: Vec::new(),
            shown: Vec::new(),
            sel: None,
            hover: None,
            hover_btn: None,
            hover_fade: HoverFade::default(),
            col_w: Vec::new(),
            col_w_manual: false,
            sort_keys: Vec::new(),
            hdr_resize: None,
            col_order: (0..TEXT_COLS).collect(),
            hdr_drag: None,
            resize_cursor: false,
            btn_text_w: [0; 4],
            scroll_y: 0,
            scroll_x: 0,
            bars: ScrollBars::new(),
            test_marks: HashMap::new(),
            conn_marks: HashMap::new(),
            close_at: None,
            tip: None,
            filter: TextBox::new(t(Msg::PhFilter)),
            btn_new: Button::new(t(Msg::BtnNew)),
            btn_edit: Button::new(t(Msg::BtnEdit)),
            btn_delete: Button::new(t(Msg::BtnDelete)),
            btn_close: Button::new(t(Msg::BtnClose)),
            list: Rect::new(0, 0, 0, 0),
            row_h: 24,
            focus: WFocus::Filter,
            focus_btn: 0,
            last_click: None,
            detail_t: 0.0,
            anim: None,
            probes: HashMap::new(),
            hub: None,
            connected: probe::connected_names(),
            policy: ProbePolicy::default(),
            active: None,
        };
        w.refresh_profiles(None);
        w
    }

    /// 프로브 스레드·정책 주입(부팅 시 한 번).
    pub(crate) fn set_probe(&mut self, hub: ProbeHub, policy: ProbePolicy) {
        self.hub = Some(hub);
        self.policy = policy;
    }

    pub(crate) fn policy(&self) -> &ProbePolicy {
        &self.policy
    }

    /// 지금 세션이 붙은 프로필 이름(없으면 빈 문자열).
    pub(crate) fn active_name(&self) -> &str {
        self.active.as_deref().unwrap_or("")
    }

    /// 이름의 신호등 상태(대상이 아니면 None).
    pub(crate) fn status_of(&self, name: &str) -> Option<ProbeStatus> {
        self.probes.get(name).map(|e| e.status)
    }

    /// 행 접속 버튼의 상태색(None = 기본 회색).
    pub(crate) fn set_connect_mark(&mut self, name: &str, mark: Option<ConnectMark>) {
        if name.is_empty() {
            return;
        }
        match mark {
            Some(m) => {
                self.conn_marks.insert(name.to_string(), m);
            }
            None => {
                self.conn_marks.remove(name);
            }
        }
        self.redraw();
    }

    pub(crate) fn clear_connect_marks(&mut self) {
        self.conn_marks.clear();
        self.redraw();
    }

    /// 잠시 뒤 창을 닫는다(호스트 `tick`이 시각을 본다).
    pub(crate) fn close_soon(&mut self, after: std::time::Duration) {
        self.close_at = Some(Instant::now() + after);
        self.redraw();
    }

    /// 행 테스트 버튼의 결과 표시를 바꾼다(버튼 기능은 그대로).
    pub(crate) fn set_test_mark(&mut self, name: &str, mark: TestMark) {
        if name.is_empty() {
            return;
        }
        self.test_marks.insert(name.to_string(), mark);
        self.redraw();
    }

    /// 접속 성공 — 이 프로필을 신호등 **대상**에 올린다(영속). 신호등 자체는 바꾸지 않는다 —
    /// 목적이 접속 전 확인이라 다음에 창을 열 때 실제 프로브로 판정한다(사용자 09-14).
    pub(crate) fn mark_connected(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        probe::mark_connected(name);
        self.connected.insert(name.to_string());
        self.active = Some(name.to_string());
    }

    /// 세션이 끊겼다(Disconnect) — 활성 프로필 해제.
    pub(crate) fn clear_active(&mut self) {
        self.active = None;
    }

    /// 접속 실패가 **확인**됐다(Connect/Test 실패 · 실행 중 접속성 오류) — 그 서버만 지금 바로 다시 묻는다(사용자 09-14).
    /// 대상 집합(한 번 이상 접속)에 없던 프로필도 이번엔 묻는다. 창이 닫혀 있어도 보낸다 — 다음에 열 때 최신 상태가 보이도록.
    pub(crate) fn note_failure(&mut self, name: &str) {
        if !self.policy.enabled || name.is_empty() {
            return;
        }
        let Some(p) = self.profiles.iter().find(|p| p.name == name) else {
            return;
        };
        let (Some(host), Some(port)) = (p.spec.host.clone(), p.spec.port) else {
            return;
        };
        let now = Instant::now();
        let e = self
            .probes
            .entry(name.to_string())
            .or_insert_with(|| ProbeEntry::fresh(now));
        if e.status == ProbeStatus::Checking {
            return;
        }
        if let Some(hub) = &self.hub {
            hub.request(ProbeReq {
                name: name.to_string(),
                host,
                port,
                timeout: self.policy.timeout,
            });
            e.status = ProbeStatus::Checking;
            e.next_at = None;
            self.redraw();
        }
    }

    /// 대상 프로필(한 번 이상 접속 · 호스트/포트 있음)을 지금 바로 묻도록 예약 — 창을 열 때.
    /// 기존 항목은 누적 횟수를 보존한 채 당긴다(`poke`) · 새 항목만 `fresh`.
    fn schedule_probes(&mut self) {
        if !self.policy.enabled || self.window.is_none() {
            return;
        }
        let now = Instant::now();
        for p in &self.profiles {
            if !self.connected.contains(&p.name) || p.spec.host.is_none() || p.spec.port.is_none() {
                continue;
            }
            self.probes
                .entry(p.name.clone())
                .or_insert_with(|| ProbeEntry::fresh(now))
                .poke(now);
        }
    }

    /// 예약된 프로브를 보낸다(순차 스레드) · 다음 예약 시각을 돌려준다(호스트 WaitUntil).
    /// 주기 갱신(`probe.interval`)은 창이 열려 있을 때만 돈다.
    pub(crate) fn tick(&mut self, now: Instant) -> Option<Instant> {
        // 예약된 닫기(접속 성공 초록 표시 뒤).
        if let Some(t) = self.close_at {
            if t <= now {
                self.close_at = None;
                self.close();
                return None;
            } else if self.window.is_some() {
                let probe_next = self.tick_probes(now);
                return Some(probe_next.map_or(t, |p| p.min(t)));
            }
        }
        self.tick_probes(now)
    }

    fn tick_probes(&mut self, now: Instant) -> Option<Instant> {
        if self.window.is_none() || !self.policy.enabled {
            return None;
        }
        let Some(hub) = &self.hub else { return None };
        let mut next: Option<Instant> = None;
        let mut changed = false;
        for p in &self.profiles {
            let Some(e) = self.probes.get_mut(&p.name) else {
                continue;
            };
            match e.next_at {
                Some(t) if t <= now => {
                    if let (Some(h), Some(port)) = (p.spec.host.clone(), p.spec.port) {
                        hub.request(ProbeReq {
                            name: p.name.clone(),
                            host: h,
                            port,
                            timeout: self.policy.timeout,
                        });
                        e.status = ProbeStatus::Checking;
                        e.next_at = None;
                        changed = true;
                    }
                }
                Some(t) => next = Some(next.map_or(t, |n: Instant| n.min(t))),
                None => {}
            }
        }
        if changed {
            self.redraw();
        }
        next
    }

    /// 프로브 결과 수거(호스트가 Wake마다 부른다). 바뀐 게 있으면 true.
    pub(crate) fn drain_probes(&mut self) -> bool {
        let Some(hub) = &self.hub else { return false };
        let now = Instant::now();
        let mut changed = false;
        while let Some(r) = hub.try_recv() {
            if let Some(e) = self.probes.get_mut(&r.name) {
                e.apply(r.outcome, now, &self.policy);
                changed = true;
            }
        }
        if changed {
            self.redraw();
        }
        changed
    }

    fn probe_status(&self, name: &str) -> Option<ProbeStatus> {
        self.probes.get(name).map(|e| e.status)
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    /// 저장소를 다시 읽어 목록·폼 콤보를 갱신(`select` = 선택 유지할 이름).
    pub(crate) fn refresh_profiles(&mut self, select: Option<&str>) {
        self.profiles = Vault::open_default()
            .and_then(|v| v.list())
            .unwrap_or_default();
        self.refilter();
        if let Some(n) = select {
            self.sel = self.shown.iter().position(|&i| self.profiles[i].name == n);
        }
        self.redraw();
    }

    /// 언어 전환 — 라벨 재생성.
    pub(crate) fn relabel(&mut self) {
        self.panel.relabel();
        let text = self.filter.text();
        self.filter = TextBox::new(t(Msg::PhFilter)).with_text(&text);
        self.btn_new.set_label(t(Msg::BtnNew));
        self.btn_edit.set_label(t(Msg::BtnEdit));
        self.btn_delete.set_label(t(Msg::BtnDelete));
        self.btn_close.set_label(t(Msg::BtnClose));
        self.layout();
        self.redraw();
    }

    fn refilter(&mut self) {
        let q = self.filter.text().trim().to_lowercase();
        self.shown = self
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.name.to_lowercase().contains(&q)
                    || target_of(p).to_lowercase().contains(&q)
                    || p.spec
                        .user
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        sort_shown(&self.profiles, &mut self.shown, &self.sort_keys);
        if self.sel.is_some_and(|s| s >= self.shown.len()) {
            self.sel = None;
        }
        self.clamp_scroll();
    }

    /// 헤더 클릭 정렬(선택은 이름으로 따라간다).
    fn toggle_sort(&mut self, col: usize, additive: bool) {
        let keep = self.selected_name();
        toggle_sort_key(&mut self.sort_keys, col, additive);
        self.refilter();
        if let Some(n) = keep {
            self.sel = self.shown.iter().position(|&i| self.profiles[i].name == n);
        }
    }

    // ── 목록 기하(스크롤 · 열)

    fn icons_w(&self) -> i32 {
        self.row_h * ICON_COLS
    }

    fn header_rect(&self) -> Rect {
        Rect::new(self.list.x, self.list.y, self.list.w, self.row_h)
    }

    /// 행 영역(헤더 아래 · 테두리 안).
    fn body_rect(&self) -> Rect {
        Rect::new(
            self.list.x + 1,
            self.list.y + self.row_h,
            (self.list.w - 2).max(0),
            (self.list.h - self.row_h - 1).max(0),
        )
    }

    /// 콘텐츠 크기 — 아이콘 열 + 텍스트 열 폭 합 · 행 수 × 행 높이.
    fn content_size(&self) -> (i32, i32) {
        let w = self.icons_w() + self.col_w.iter().sum::<i32>();
        let h = self.shown.len() as i32 * self.row_h;
        (w, h)
    }

    fn clamp_scroll(&mut self) {
        let body = self.body_rect();
        let (cw, ch) = self.content_size();
        self.scroll_x = self.scroll_x.clamp(0, (cw - body.w).max(0));
        self.scroll_y = self.scroll_y.clamp(0, (ch - body.h).max(0));
    }

    /// 선택 행이 보이도록 세로 스크롤을 맞춘다(키보드 이동).
    fn ensure_visible(&mut self, row: usize) {
        let body = self.body_rect();
        let top = row as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + body.h {
            self.scroll_y = top + self.row_h - body.h;
        }
        self.clamp_scroll();
        self.bars.show();
    }

    /// 헤더 x → 표시 위치(col_order index).
    fn header_pos_at(&self, x: i32) -> Option<usize> {
        let mut cx = self.list.x + self.icons_w() - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(MIN_COL_W);
            if x >= cx && x < cx + cw {
                return Some(pos);
            }
            cx += cw;
        }
        None
    }

    /// 드래그 목표 위치(현재 x 기준 · 열 중앙을 넘으면 그 다음) — 삽입 미리보기 선도 같은 값.
    fn drop_pos_at(&self, x: i32) -> usize {
        let mut cx = self.list.x + self.icons_w() - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(MIN_COL_W);
            if x < cx + cw / 2 {
                return pos;
            }
            cx += cw;
        }
        self.col_order.len()
    }

    /// 헤더에서 열 오른쪽 경계 ±6px 안이면 그 열 — 폭 조절 손잡이.
    fn header_edge_at(&self, x: i32) -> Option<usize> {
        let grip = self.s(6.0);
        let mut cx = self.list.x + self.icons_w() - self.scroll_x;
        for &ci in &self.col_order {
            cx += self.col_w.get(ci).copied().unwrap_or(MIN_COL_W);
            if (x - cx).abs() <= grip {
                return Some(ci);
            }
        }
        None
    }

    fn header_edge_hover(&self, p: Point) -> bool {
        self.hdr_resize.is_some()
            || (self.header_rect().contains(p) && self.header_edge_at(p.x).is_some())
    }

    /// 스크롤바·휠 처리 — 소비했으면 true(호스트는 그 이벤트를 목록에 다시 쓰지 않는다).
    fn route_bars(&mut self, ev: &InputEvent) -> bool {
        let body = self.body_rect();
        if body.w <= 0 || body.h <= 0 {
            return false;
        }
        let (cw, ch) = self.content_size();
        let (nx, ny, consumed) = self.bars.on_event(
            ev,
            body,
            cw.max(body.w),
            ch.max(body.h),
            self.scroll_x,
            self.scroll_y,
            self.scale,
        );
        let moved = nx != self.scroll_x || ny != self.scroll_y;
        self.scroll_x = nx;
        self.scroll_y = ny;
        self.clamp_scroll();
        if moved || consumed {
            self.redraw();
        }
        consumed
    }

    /// 스크롤바 페이드 타이머(호스트 `about_to_wait`) — 다시 그려야 하면 true.
    pub(crate) fn tick_bars(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let tip_due = matches!(self.tip, Some((_, t)) if (TIP_MS..TIP_MS + 40).contains(&t.elapsed().as_millis()));
        let a = self.bars.tick(now_ms);
        let b = self.hover_fade.tick(now_ms);
        // 버튼 hover 페이드(상단 4개 + 폼 3개) — 지금까지 틱이 없어 즉시 켜졌다(사용자 09-14 500ms 요구).
        let c = self.btn_new.tick(now_ms)
            | self.btn_edit.tick(now_ms)
            | self.btn_delete.tick(now_ms)
            | self.btn_close.tick(now_ms)
            | self.panel.tick(now_ms);
        a || b || c || tip_due
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.window.is_some() && self.bars.is_visible()
    }

    pub(crate) fn hover_animating(&self) -> bool {
        self.window.is_some()
            && (self.hover_fade.is_animating()
                || self.btn_new.is_animating()
                || self.btn_edit.is_animating()
                || self.btn_delete.is_animating()
                || self.btn_close.is_animating()
                || self.panel.animating())
    }

    /// 메인 창 위 가운데에 연다(`over` = 메인 창 바깥 좌표·크기). 이미 열려 있으면 앞으로.
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        let (lw, lh) = (f64::from(BASE_W), f64::from(BASE_H));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinLogin)))
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let Ok(win) = el.create_window(crate::icon::with_icon(attrs)) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        if let Ok(ctx) = softbuffer::Context::new(win.clone()) {
            if let Ok(s) = softbuffer::Surface::new(&ctx, win.clone()) {
                self.surface = Some(s);
            }
            self.ctx = Some(ctx);
        }
        win.set_ime_allowed(true);
        self.window = Some(win);
        self.refresh_profiles(None);
        self.schedule_probes();
        self.detail_t = 0.0;
        self.anim = None;
        self.set_focus(WFocus::List);
        if self.profiles.is_empty() {
            self.open_detail(true);
        }
        self.layout();
        self.redraw();
    }

    /// 상세 폼 열기(슬라이드 인) · `clear` = New(폼 비움) / false = Edit(폼은 호스트가 채운다).
    fn open_detail(&mut self, clear: bool) {
        if clear {
            self.panel.clear();
            self.sel = None;
        }
        if self.detail_t < 1.0 {
            self.anim = Some((Instant::now(), self.detail_t, 1.0));
        }
        self.set_focus(WFocus::Panel);
        self.panel.focus_host();
        self.redraw();
    }

    /// 상세 폼 접기(슬라이드 아웃).
    fn close_detail(&mut self) {
        if self.detail_t > 0.0 {
            self.anim = Some((Instant::now(), self.detail_t, 0.0));
        }
        self.set_focus(WFocus::List);
        self.redraw();
    }

    fn detail_open(&self) -> bool {
        self.detail_t > 0.0 || matches!(self.anim, Some((_, _, to)) if to > 0.0)
    }

    /// 애니메이션 진행(페인트 시작에 호출) — 아직 진행 중이면 true.
    fn advance(&mut self) -> bool {
        let Some((t0, from, to)) = self.anim else {
            return false;
        };
        let p = (t0.elapsed().as_secs_f32() * 1000.0 / SLIDE_MS).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - p) * (1.0 - p); // ease-out
        self.detail_t = from + (to - from) * e;
        let done = p >= 1.0;
        if done {
            self.detail_t = to;
            self.anim = None;
        }
        // 창 자체가 오른쪽으로 커진다/줄어든다(목록 폭은 그대로 · 폼은 새 영역에 드러난다).
        if let Some(w) = &self.window {
            let lw = f64::from(BASE_W + PANEL_W * self.detail_t);
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(lw, f64::from(BASE_H)));
        }
        !done
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
        self.panel.set_focused(false);
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(10.0);
        let pw = self.s(PANEL_W);
        // 상세 폼 = 오른쪽에서 detail_t만큼 들어와 있다(닫힘 = 창 밖).
        let open_px = (pw as f32 * self.detail_t).round() as i32;
        self.panel.set_bounds(Rect::new(w - open_px, 0, pw, h), s);
        let x0 = pad;
        let rw = (w - open_px - x0 - pad).max(0);
        let row = self.s(28.0);
        // 버튼 폭 = 라벨 폭 + 좌우 여백(라벨 미측정이면 72) · 간격 = pad/3(사용자 09-14 "간격 1/3 · 폭 최소").
        let gap = (pad / 3).max(2);
        let bw = |i: usize| -> i32 {
            let tw = self.btn_text_w[i];
            if tw > 0 {
                tw + pad * 2
            } else {
                self.s(72.0)
            }
        };
        let ws = [bw(0), bw(1), bw(2), bw(3)];
        let total: i32 = ws.iter().sum::<i32>() + gap * 3;
        let mut inv = Invalidations::default();
        // 제목 줄(라벨) → 필터 + 버튼 줄 → 목록
        let y1 = pad + self.s(22.0);
        let bx = x0 + rw - total;
        self.filter
            .set_bounds(Rect::new(x0, y1, (bx - x0 - pad).max(0), row), &mut inv);
        self.btn_new
            .set_bounds(Rect::new(bx, y1, ws[0], row), &mut inv);
        self.btn_edit
            .set_bounds(Rect::new(bx + ws[0] + gap, y1, ws[1], row), &mut inv);
        self.btn_delete
            .set_bounds(
                Rect::new(bx + ws[0] + ws[1] + gap * 2, y1, ws[2], row),
                &mut inv,
            );
        self.btn_close
            .set_bounds(
                Rect::new(bx + ws[0] + ws[1] + ws[2] + gap * 3, y1, ws[3], row),
                &mut inv,
            );
        for c in [
            &mut self.btn_new,
            &mut self.btn_edit,
            &mut self.btn_delete,
            &mut self.btn_close,
        ] {
            c.set_scale(s);
        }
        self.filter.set_scale(s);
        let ly = y1 + row + pad;
        self.list = Rect::new(x0, ly, rw, (h - ly - pad).max(0));
        self.row_h = self.s(24.0);
        // 텍스트 열 폭 — 사용자가 조절하기 전까지는 창 폭을 비율로 따라간다.
        if !self.col_w_manual || self.col_w.len() != TEXT_COLS {
            let avail = (self.list.w - 2 - self.icons_w()).max(MIN_COL_W * TEXT_COLS as i32);
            self.col_w = COL_FRACS
                .iter()
                .map(|f| ((avail as f32 * f) as i32).max(MIN_COL_W))
                .collect();
        }
        self.clamp_scroll();
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        let body = self.body_rect();
        if !body.contains(p) {
            return None;
        }
        let i = ((p.y - body.y + self.scroll_y) / self.row_h.max(1)) as usize;
        (i < self.shown.len()).then_some(i)
    }

    /// 행 아이콘 버튼 명중 — 신호등 다음 두 칸(테스트 · 접속).
    fn row_btn_at(&self, p: Point) -> Option<(usize, RowBtn)> {
        let row = self.row_at(p)?;
        let sw = self.row_h;
        let dx = p.x - self.list.x;
        if dx >= sw && dx < sw * 2 {
            Some((row, RowBtn::Test))
        } else if dx >= sw * 2 && dx < sw * 3 {
            Some((row, RowBtn::Connect))
        } else {
            None
        }
    }

    /// 커서 아래 툴팁 대상(아이콘 열 3개 × 헤더/행).
    fn tip_target_at(&self, p: Point) -> Option<TipTarget> {
        let sw = self.row_h.max(1);
        let dx = p.x - self.list.x;
        if dx < 0 || dx >= sw * ICON_COLS {
            return None;
        }
        let col = dx / sw;
        if self.header_rect().contains(p) {
            return Some(TipTarget { col, row: None });
        }
        self.row_at(p).map(|row| TipTarget { col, row: Some(row) })
    }

    /// 툴팁 문구(다국어) — 헤더는 열 설명 · 행은 그 프로필의 현재 상태.
    fn tip_text(&self, tg: TipTarget) -> String {
        let Some(row) = tg.row else {
            return t(match tg.col {
                0 => Msg::TipColStatus,
                1 => Msg::TipColTest,
                _ => Msg::TipColConnect,
            })
            .to_string();
        };
        let Some(p) = self.shown.get(row).and_then(|&i| self.profiles.get(i)) else {
            return String::new();
        };
        let name = p.name.as_str();
        match tg.col {
            0 => {
                let st = t(match self.probes.get(name).map(|e| e.status) {
                    Some(ProbeStatus::Up) => Msg::TipStUp,
                    Some(ProbeStatus::Checking) => Msg::TipStChecking,
                    Some(ProbeStatus::PortClosed) => Msg::TipStPortClosed,
                    Some(ProbeStatus::Down) => Msg::TipStDown,
                    Some(ProbeStatus::Unknown) | None => Msg::TipStUnknown,
                });
                tf(Msg::TipRowStatus, &[name, st])
            }
            1 => {
                let mut s = tf(Msg::TipRowTest, &[name]);
                if let Some(m) = self.test_marks.get(name) {
                    s.push_str(" · ");
                    s.push_str(t(match m {
                        TestMark::Testing => Msg::TipTesting,
                        TestMark::Ok => Msg::TipTestOk,
                        TestMark::Failed => Msg::TipTestFailed,
                    }));
                }
                if !p.has_password {
                    s.push('\n');
                    s.push_str(t(Msg::TipNoPassword));
                }
                s
            }
            _ => {
                let mut s = tf(Msg::TipRowConnect, &[name]);
                if let Some(m) = self.conn_marks.get(name) {
                    s.push_str(" · ");
                    s.push_str(t(match m {
                        ConnectMark::Connecting => Msg::TipConnecting,
                        ConnectMark::Connected => Msg::TipConnected,
                    }));
                }
                if !p.has_password {
                    s.push('\n');
                    s.push_str(t(Msg::TipNoPassword));
                }
                s
            }
        }
    }

    /// 툴팁 대상 위에 머무는 중인가(호스트가 ≈30ms 타이머를 돌린다).
    pub(crate) fn tooltip_pending(&self) -> bool {
        self.window.is_some() && self.tip.is_some()
    }

    /// 행의 프로필에 비밀번호 봉투가 있는가(없으면 Test/Connect 행 버튼 비활성).
    fn row_has_password(&self, row: usize) -> bool {
        self.shown
            .get(row)
            .and_then(|&i| self.profiles.get(i))
            .is_some_and(|p| p.has_password)
    }

    fn name_at(&self, row: usize) -> Option<String> {
        self.shown
            .get(row)
            .and_then(|&i| self.profiles.get(i))
            .map(|p| p.name.clone())
    }

    fn selected_name(&self) -> Option<String> {
        self.sel
            .and_then(|s| self.shown.get(s))
            .map(|&i| self.profiles[i].name.clone())
    }

    /// ★ 포커스 규칙(CLAUDE.md §3): 이 창의 포커스 소유자는 **하나** — 패널 / 필터 / 목록 / 상단 버튼 하나.
    /// 마지막 클릭·입력이 있었던 곳만 링을 가지고 나머지는 전부 꺼진다(패널 안쪽은 `ConnectPanel::own_focus`가 같은 일을 한다).
    fn set_focus(&mut self, f: WFocus) {
        self.focus = f;
        self.panel.set_focused(f == WFocus::Panel);
        self.filter.set_focused(f == WFocus::Filter);
        let fb = self.focus_btn;
        for (i, b) in [
            &mut self.btn_new,
            &mut self.btn_edit,
            &mut self.btn_delete,
            &mut self.btn_close,
        ]
        .into_iter()
        .enumerate()
        {
            b.set_focused(f == WFocus::Button && i == fb);
        }
    }

    /// 상단 버튼 명중(index).
    fn top_btn_at(&self, p: Point) -> Option<usize> {
        [
            self.btn_new.bounds(),
            self.btn_edit.bounds(),
            self.btn_delete.bounds(),
            self.btn_close.bounds(),
        ]
        .iter()
        .position(|b| b.contains(p))
    }

    /// 상단 버튼의 클릭 결과 처리 — 창을 닫았으면 true.
    fn handle_top_clicks(&mut self, out: &mut Vec<ConnWinAction>) -> bool {
        if self.btn_new.take_clicked() {
            self.open_detail(true);
        }
        if self.btn_edit.take_clicked() {
            // 이미 펼쳐져 있으면 Esc와 같이 접는다(사용자 09-14).
            if self.detail_open() {
                self.close_detail();
            } else if let Some(n) = self.selected_name() {
                out.push(ConnWinAction::Panel(PanelAction::LoadProfile(n)));
                self.open_detail(false);
            }
        }
        if self.btn_delete.take_clicked() {
            if let Some(n) = self.selected_name() {
                out.push(ConnWinAction::Delete(n));
            }
        }
        if self.btn_close.take_clicked() {
            self.close();
            return true;
        }
        false
    }

    // ── 이벤트

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> Vec<ConnWinAction> {
        let mut out = Vec::new();
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => out.push(ConnWinAction::Paint),
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
            }
            WindowEvent::CursorLeft { .. } => {
                // 창 밖으로 나가면 호버·툴팁 해제(페이드 아웃).
                self.hover = None;
                self.hover_btn = None;
                self.hover_fade.set(None);
                self.tip = None;
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let p = Point {
                    x: self.cursor.0,
                    y: self.cursor.1,
                };
                let h = self.row_at(p);
                let hb = self.row_btn_at(p);
                if h != self.hover || hb != self.hover_btn {
                    self.hover = h;
                    self.hover_btn = hb;
                    self.hover_fade.set(h);
                    self.redraw();
                }
                // 툴팁 대상이 바뀌면 머물기 시작 시각을 다시 잡는다(같은 대상이면 유지).
                let tg = self.tip_target_at(p);
                if tg != self.tip.map(|(t, _)| t) {
                    self.tip = tg.map(|t| (t, Instant::now()));
                    self.redraw();
                }
                // 헤더 경계 위 = 폭 조절 커서(바뀔 때만).
                let over_edge = self.header_edge_hover(p);
                if over_edge != self.resize_cursor {
                    self.resize_cursor = over_edge;
                    if let Some(w) = &self.window {
                        w.set_cursor(if over_edge {
                            winit::window::CursorIcon::ColResize
                        } else {
                            winit::window::CursorIcon::Default
                        });
                    }
                }
                self.route(InputEvent::MouseMove { x: p.x, y: p.y }, &mut out);
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                let tb = match self.focus {
                    WFocus::Panel => self.panel.focused_textbox(),
                    WFocus::Filter => Some(&mut self.filter),
                    WFocus::List | WFocus::Button => None,
                };
                if let Some(tb) = tb {
                    match ime {
                        Ime::Preedit(t, _) => tb.set_preedit(t, &mut inv),
                        Ime::Commit(t) => {
                            tb.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                            }
                        }
                        _ => {}
                    }
                    if self.focus == WFocus::Filter {
                        self.refilter();
                    }
                    self.redraw();
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) if !self.panel.popup_open() => {
                        if self.detail_open() {
                            self.close_detail();
                        } else {
                            self.close();
                        }
                    }
                    Key::Named(NamedKey::Enter)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if let Some(n) = self.selected_name() {
                            out.push(ConnWinAction::Login(n));
                        }
                    }
                    Key::Named(NamedKey::ArrowDown)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if !self.shown.is_empty() {
                            let s = self.sel.map_or(0, |s| (s + 1).min(self.shown.len() - 1));
                            self.sel = Some(s);
                            self.ensure_visible(s);
                            self.redraw();
                        }
                    }
                    Key::Named(NamedKey::ArrowUp)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if let Some(s) = self.sel.map(|s| s.saturating_sub(1)) {
                            self.sel = Some(s);
                            self.ensure_visible(s);
                        }
                        self.redraw();
                    }
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight)
                        if matches!(self.focus, WFocus::List) =>
                    {
                        let step = self.row_h * 2;
                        if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowLeft)) {
                            self.scroll_x -= step;
                        } else {
                            self.scroll_x += step;
                        }
                        self.clamp_scroll();
                        self.bars.show();
                        self.redraw();
                    }
                    Key::Named(NamedKey::PageDown | NamedKey::PageUp | NamedKey::Home | NamedKey::End)
                        if matches!(self.focus, WFocus::List) && !self.shown.is_empty() =>
                    {
                        let page = (self.body_rect().h / self.row_h.max(1)).max(1) as usize;
                        let last = self.shown.len() - 1;
                        let s = match kev.logical_key.as_ref() {
                            Key::Named(NamedKey::PageDown) => {
                                self.sel.map_or(page.min(last), |s| (s + page).min(last))
                            }
                            Key::Named(NamedKey::PageUp) => {
                                self.sel.map_or(0, |s| s.saturating_sub(page))
                            }
                            Key::Named(NamedKey::Home) => 0,
                            _ => last,
                        };
                        self.sel = Some(s);
                        self.ensure_visible(s);
                        self.redraw();
                    }
                    Key::Named(NamedKey::Delete)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if let Some(n) = self.selected_name() {
                            out.push(ConnWinAction::Delete(n));
                        }
                    }
                    _ => {
                        if let Some(ie) = self.to_input(ev) {
                            self.route(ie, &mut out);
                        }
                    }
                }
            }
            _ => {
                if let Some(ie) = self.to_input(ev) {
                    self.route(ie, &mut out);
                }
            }
        }
        out
    }

    fn to_input(&self, event: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey, shift: bool, primary: bool| InputEvent::Key {
            key: k,
            shift,
            primary,
        };
        Some(match event {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: self.primary,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                (ElementState::Pressed, MouseButton::Right) => InputEvent::RightDown { x, y },
                _ => return None,
            },
            // 휠: 가로 성분(틸트 휠·트랙패드)이 있으면 HWheel · Shift+세로 휠 = 가로(관례) · 아니면 세로.
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => ((*dx * 120.0) as i32, (*dy * 120.0) as i32),
                    MouseScrollDelta::PixelDelta(p) => (p.x as i32, p.y as i32),
                };
                if dx != 0 {
                    InputEvent::HWheel { delta: dx }
                } else if self.shift {
                    InputEvent::HWheel { delta: -dy }
                } else {
                    InputEvent::Wheel { delta: dy }
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter, self.shift, self.primary),
                    Key::Named(NamedKey::Escape) => key(CtlKey::Escape, false, false),
                    Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowRight) => {
                        key(CtlKey::Right, self.shift, self.primary)
                    }
                    Key::Named(NamedKey::Home) => key(CtlKey::Home, self.shift, self.primary),
                    Key::Named(NamedKey::End) => key(CtlKey::End, self.shift, self.primary),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete, false, false),
                    Key::Named(NamedKey::Tab) => InputEvent::Char { c: '\t', now_ms: 0 },
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) if !self.primary => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        InputEvent::Char { c, now_ms: 0 }
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    fn route(&mut self, ev: InputEvent, out: &mut Vec<ConnWinAction>) {
        let mut inv = Invalidations::default();
        // 열린 콤보(폼)는 모달.
        if self.panel.popup_open() {
            if let Some(a) = self.panel.route(&ev, &mut inv) {
                out.push(ConnWinAction::Panel(a));
            }
            self.redraw();
            return;
        }
        let is_mouse = matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        // 헤더 폭 조절 드래그(진행 중이면 다른 처리보다 먼저).
        match ev {
            InputEvent::MouseMove { x, .. } if self.hdr_resize.is_some() => {
                if let Some((ci, x0, w0)) = self.hdr_resize {
                    if let Some(w) = self.col_w.get_mut(ci) {
                        *w = (w0 + (x - x0)).max(MIN_COL_W);
                    }
                    self.col_w_manual = true;
                    self.clamp_scroll();
                    // 넘치면 가로 막대가 보이도록(오버레이는 스크롤 전엔 숨어 있어 "스크롤이 없다"로 보인다).
                    let body = self.body_rect();
                    if self.content_size().0 > body.w {
                        self.bars.show();
                    }
                }
                self.redraw();
                return;
            }
            InputEvent::MouseUp { .. } if self.hdr_resize.is_some() => {
                self.hdr_resize = None;
                self.redraw();
                return;
            }
            // 헤더 열 드래그(이동) — 4px 넘게 움직이면 DnD · 삽입 위치 미리보기.
            InputEvent::MouseMove { x, .. } if self.hdr_drag.is_some() => {
                if let Some(d) = self.hdr_drag.as_mut() {
                    d.2 = x;
                    if (x - d.1).abs() > 4 {
                        d.3 = true;
                    }
                }
                self.redraw();
                return;
            }
            InputEvent::MouseUp { x, .. } if self.hdr_drag.is_some() => {
                let (pos, _, _, moved, shift) =
                    self.hdr_drag.take().unwrap_or((0, 0, 0, false, false));
                if moved {
                    // 이동했으면 정렬은 하지 않는다(사용자 09-14).
                    let to = self.drop_pos_at(x);
                    move_col(&mut self.col_order, pos, to);
                } else if let Some(&ci) = self.col_order.get(pos) {
                    self.toggle_sort(ci, shift);
                }
                self.redraw();
                return;
            }
            _ => {}
        }
        // 휠은 커서가 폼 위가 아니면 목록 스크롤 · 스크롤바 썸 드래그/호버는 목록보다 먼저.
        let cursor = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let over_panel = self.detail_t > 0.0 && self.panel.bounds().contains(cursor);
        if matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. }) {
            if !over_panel {
                self.route_bars(&ev);
                return;
            }
        } else if is_mouse && !over_panel && self.route_bars(&ev) {
            return;
        }
        if let InputEvent::MouseDown { x, y, shift, .. } = ev {
            let p = Point { x, y };
            if self.panel.bounds().contains(p) && self.detail_t > 0.0 {
                self.set_focus(WFocus::Panel);
            } else if let Some(i) = self.top_btn_at(p) {
                // 누른 버튼만 테두리(마지막으로 눌린 하나).
                self.focus_btn = i;
                self.set_focus(WFocus::Button);
            } else if self.filter.bounds().contains(p) {
                self.set_focus(WFocus::Filter);
            } else if self.header_rect().contains(p) {
                // 헤더: 경계 = 폭 조절 · 열 = 누르고 놓으면 정렬(Shift = 결합) · 끌면 이동(MouseUp에서 판정).
                self.set_focus(WFocus::List);
                if let Some(ci) = self.header_edge_at(x) {
                    let w0 = self.col_w.get(ci).copied().unwrap_or(MIN_COL_W);
                    self.hdr_resize = Some((ci, x, w0));
                } else if let Some(pos) = self.header_pos_at(x) {
                    self.hdr_drag = Some((pos, x, x, false, shift));
                }
                self.redraw();
                return;
            } else if let Some(row) = self.row_at(p) {
                self.set_focus(WFocus::List);
                let now = Instant::now();
                // 행 아이콘 버튼(테스트 · 접속) — 선택만 바꾸고 더블클릭 판정은 하지 않는다.
                if let Some((_, b)) = self.row_btn_at(p) {
                    self.sel = Some(row);
                    self.last_click = None;
                    // 비밀번호가 저장되지 않은 프로필은 행 버튼이 동작하지 않는다(사용자 09-14).
                    if self.row_has_password(row) {
                        if let Some(n) = self.name_at(row) {
                            out.push(match b {
                                RowBtn::Test => ConnWinAction::TestProfile(n),
                                RowBtn::Connect => ConnWinAction::Login(n),
                            });
                        }
                    }
                    self.redraw();
                    return;
                }
                let double = matches!(self.last_click, Some((r, t)) if r == row && t.elapsed().as_millis() < DBLCLICK_MS);
                self.sel = Some(row);
                self.last_click = Some((row, now));
                if let Some(n) = self.selected_name() {
                    if double {
                        out.push(ConnWinAction::Login(n));
                    } else if self.detail_open() {
                        out.push(ConnWinAction::Panel(PanelAction::LoadProfile(n)));
                    }
                }
                self.redraw();
                return;
            }
        }
        if is_mouse {
            self.btn_new.on_event(&ev, &mut inv);
            self.btn_edit.on_event(&ev, &mut inv);
            self.btn_delete.on_event(&ev, &mut inv);
            self.btn_close.on_event(&ev, &mut inv);
            if self.handle_top_clicks(out) {
                return;
            }
            if self.detail_t > 0.0 {
                if let Some(a) = self.panel.route(&ev, &mut inv) {
                    out.push(ConnWinAction::Panel(a));
                }
            }
        } else {
            match self.focus {
                WFocus::Panel => {
                    if let Some(a) = self.panel.route(&ev, &mut inv) {
                        out.push(ConnWinAction::Panel(a));
                    }
                }
                WFocus::Filter => {
                    self.filter.on_event(&ev, &mut inv);
                    self.refilter();
                }
                WFocus::List => {}
                WFocus::Button => {
                    // 포커스 버튼은 Enter/Space로 눌린다.
                    match self.focus_btn {
                        0 => self.btn_new.on_event(&ev, &mut inv),
                        1 => self.btn_edit.on_event(&ev, &mut inv),
                        2 => self.btn_delete.on_event(&ev, &mut inv),
                        _ => self.btn_close.on_event(&ev, &mut inv),
                    }
                    if self.handle_top_clicks(out) {
                        return;
                    }
                }
            }
        }
        self.redraw();
    }

    // ── 그리기

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
        let animating = self.advance();
        // 버튼 라벨 폭(현재 언어) — 배치 전에 잰다.
        self.btn_text_w = [Msg::BtnNew, Msg::BtnEdit, Msg::BtnDelete, Msg::BtnClose]
            .map(|m| ui.measure(t(m), font_px).ceil() as i32);
        if animating || self.anim.is_none() {
            self.layout();
        }
        let statuses: Vec<Option<ProbeStatus>> = self
            .profiles
            .iter()
            .map(|p| self.probe_status(&p.name))
            .collect();
        let body = self.body_rect();
        let (content_w, content_h) = self.content_size();
        let dragging = self.hdr_drag.filter(|d| d.3);
        let drop_pos = dragging.map(|d| self.drop_pos_at(d.2));
        let tip_ready = self
            .tip
            .filter(|(_, since)| since.elapsed().as_millis() >= TIP_MS)
            .map(|(tg, _)| (tg, self.tip_text(tg)));
        let (Some(win), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let caret_on = (self.started.elapsed().as_millis() / 500) % 2 == 0;
        let detail_px = self.panel.bounds().x;
        let detail_visible = self.detail_t > 0.0;
        let pad = (10.0 * s).round() as i32;
        let list = self.list;
        let row_h = self.row_h;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: font_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, ui, s)
                .with_fonts(prefs)
                .with_caret_on(caret_on);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            if detail_visible {
                self.panel.paint(&mut dc, th);
                dc.fill_rect(Rect::new(detail_px, 0, 1, hi), th.border);
            }
            dc.select_font(FontSlot::Base, false);
            let x0 = list.x;
            dc.text(
                x0,
                pad,
                Rect::new(x0, 0, list.w, hi),
                t(Msg::LblLoginList),
                th.text_dim,
            );
            self.filter.paint(&mut dc, th);
            self.btn_new.paint(&mut dc, th);
            self.btn_edit.paint(&mut dc, th);
            self.btn_delete.paint(&mut dc, th);
            self.btn_close.paint(&mut dc, th);
            // 목록
            let l = list;
            dc.fill_rect(l, th.panel_bg);
            dc.fill_rect(Rect::new(l.x, l.y, l.w, 1), th.border);
            dc.fill_rect(Rect::new(l.x, l.bottom() - 1, l.w, 1), th.border);
            dc.fill_rect(Rect::new(l.x, l.y, 1, l.h), th.border);
            dc.fill_rect(Rect::new(l.right() - 1, l.y, 1, l.h), th.border);
            let rh = row_h;
            // 앞 세 열 = 신호등 · 테스트 · 접속(고정 폭 · 아이콘 · 가로 스크롤 무관) · 텍스트 열은 `col_w`(폭 조절 · 가로 스크롤).
            let sw = rh;
            let icons_w = sw * ICON_COLS;
            let col_names = [
                t(Msg::ColName).to_string(),
                t(Msg::ColType).to_string(),
                t(Msg::ColUser).to_string(),
                t(Msg::ColTarget).to_string(),
            ];
            let col_w = self.col_w.clone();
            let ty = |y: i32| y + (rh - dc_text_h(rh)) / 2;
            let hcells = Rect::new(l.x + icons_w, l.y, (l.w - icons_w - 1).max(0), rh);
            let cells = Rect::new(l.x + icons_w, body.y, (body.right() - l.x - icons_w).max(0), body.h);
            // 헤더
            dc.fill_rect(Rect::new(l.x, l.y, l.w, rh), th.chrome_bg);
            dc.fill_rect(Rect::new(l.x, l.y + rh - 1, l.w, 1), th.border);
            // 아이콘 열 머리 = 작은 아이콘(흐리게).
            paint_test_btn(&mut dc, th, Rect::new(l.x + sw, l.y, sw, rh), None, false, true);
            paint_connect_btn(&mut dc, th, Rect::new(l.x + sw * 2, l.y, sw, rh), None, false, true);
            let col_order = self.col_order.clone();
            let mut cx = l.x + icons_w - self.scroll_x;
            let mut drop_x: Option<i32> = None;
            for (pos, &ci) in col_order.iter().enumerate() {
                let name = &col_names[ci];
                let cw = col_w.get(ci).copied().unwrap_or(MIN_COL_W);
                let clip = Rect::new(cx, l.y, cw, rh).intersection(&hcells);
                if dragging.is_some_and(|d| d.0 == pos) {
                    dc.fill_rect(clip, th.sel_bg);
                }
                if drop_pos == Some(pos) {
                    drop_x = Some(cx);
                }
                // 정렬 배지: ▲/▼ + 결합 순번(키가 2개 이상일 때) — 결과 그리드와 같은 표기.
                let badge = self.sort_keys.iter().position(|(k, _)| *k == ci).map(|i| {
                    let arrow = if self.sort_keys[i].1 { "▲" } else { "▼" };
                    if self.sort_keys.len() > 1 {
                        format!("{arrow}{}", i + 1)
                    } else {
                        arrow.to_string()
                    }
                });
                let name_clip = if let Some(bd) = &badge {
                    let bw = dc.text_width(bd);
                    dc.text(cx + cw - pad - bw, ty(l.y), clip, bd, th.accent);
                    Rect::new(cx, l.y, (cw - bw - pad * 2).max(0), rh).intersection(&hcells)
                } else {
                    Rect::new(cx, l.y, (cw - pad).max(0), rh).intersection(&hcells)
                };
                dc.text(cx + pad, ty(l.y), name_clip, name, th.text_dim);
                // 열 경계선(폭 조절 손잡이 위치).
                dc.fill_rect(
                    Rect::new(cx + cw - 1, l.y + 4, 1, rh - 8).intersection(&hcells),
                    th.border,
                );
                cx += cw;
            }
            // 삽입 위치 미리보기 — 목표 열 왼쪽(맨 끝이면 마지막 열 오른쪽)에 강조선(헤더 + 행 영역 전체 높이).
            if drop_pos.is_some() {
                let dx = drop_x.unwrap_or(cx) - 1;
                dc.fill_rect(
                    Rect::new(dx, l.y, 2, body.bottom() - l.y).intersection(&l),
                    th.accent,
                );
            }
            let first = (self.scroll_y / rh.max(1)) as usize;
            let sub = self.scroll_y % rh.max(1);
            for (row, &pi) in self.shown.iter().enumerate().skip(first) {
                let y = body.y + (row - first) as i32 * rh - sub;
                if y >= body.bottom() {
                    break;
                }
                let r = Rect::new(l.x + 1, y, l.w - 2, rh).intersection(&body);
                let selected = Some(row) == self.sel;
                if selected {
                    dc.fill_rect(r, th.sel_bg);
                }
                // 호버 = 전경색 알파 오버레이가 서서히(진행도) — 선택 행은 차이분만(두 번 칠하지 않는다).
                let ha = hover_alpha(selected, self.hover_fade.value(row));
                if ha > 0.0 {
                    dc.fill_rect_alpha(r, th.text, ha);
                }
                let p = &self.profiles[pi];
                // 신호등(초록 가능 · 노랑 확인 중 · 빨강 불가 · 회색 대상 아님)
                let dot = (rh / 2).max(6);
                let dot_color = match statuses.get(pi).copied().flatten() {
                    Some(ProbeStatus::Up) => th.ok,
                    Some(ProbeStatus::Checking) => th.warn,
                    Some(ProbeStatus::PortClosed) => th.accent,
                    Some(ProbeStatus::Down) => th.danger,
                    Some(ProbeStatus::Unknown) | None => th.border,
                };
                let dot_r = Rect::new(l.x + (sw - dot) / 2 + 1, y + (rh - dot) / 2, dot, dot);
                if dot_r.intersection(&body) == dot_r {
                    dc.fill_ellipse(dot_r, dot_color);
                }
                // 행 버튼 — 테스트(마지막 결과 표시) · 접속. 비밀번호 미저장 = 흐리게(비활성).
                let enabled = p.has_password;
                let hb = |b: RowBtn| enabled && self.hover_btn == Some((row, b));
                paint_test_btn(
                    &mut dc,
                    th,
                    Rect::new(l.x + sw, y, sw, rh).intersection(&body),
                    self.test_marks.get(&p.name).copied(),
                    hb(RowBtn::Test),
                    !enabled,
                );
                paint_connect_btn(
                    &mut dc,
                    th,
                    Rect::new(l.x + sw * 2, y, sw, rh).intersection(&body),
                    self.conn_marks.get(&p.name).copied(),
                    hb(RowBtn::Connect),
                    !enabled,
                );
                let mut cx = l.x + icons_w - self.scroll_x;
                for &ci in &col_order {
                    let cw = col_w.get(ci).copied().unwrap_or(MIN_COL_W);
                    let clip = Rect::new(cx, y, (cw - pad).max(0), rh).intersection(&cells);
                    if clip.w > 0 && clip.h > 0 {
                        dc.text(cx + pad, ty(y), clip, &cell_of(p, ci), th.text);
                    }
                    cx += cw;
                }
            }
            // 드래그 고스트 — 끌고 있는 열 이름을 커서를 따라 반투명 칩으로.
            if let Some((pos, _, x, _, _)) = dragging {
                if let Some(&ci) = col_order.get(pos) {
                    let name = &col_names[ci];
                    let tw = dc.text_width(name);
                    let chip = Rect::new(x - tw / 2 - pad, l.y + 2, tw + pad * 2, rh - 4);
                    dc.fill_rect_alpha(chip, th.accent, 0.18);
                    dc.text(chip.x + pad, ty(l.y), chip, name, th.text);
                }
            }
            if self.shown.is_empty() {
                dc.text(
                    body.x + pad,
                    ty(body.y),
                    body,
                    t(Msg::StNoProfiles),
                    th.text_dim,
                );
            }
            // 오버레이 스크롤바(필요할 때만 · 축별 · 반투명) — 행 영역 위.
            self.bars.paint(
                &mut dc,
                th,
                body,
                content_w.max(body.w),
                content_h.max(body.h),
                self.scroll_x,
                self.scroll_y,
                s,
            );
            // 툴팁(최상위) — 아이콘 셀 아래.
            if let Some((tg, text)) = &tip_ready {
                let sw = rh;
                let y = match tg.row {
                    None => l.y,
                    Some(row) => body.y + row as i32 * rh - self.scroll_y,
                };
                let anchor = Rect::new(l.x + sw * tg.col, y, sw, rh);
                draw_tooltip(&mut dc, th, anchor, wi, text, s);
            }
        }
        let _ = buf.present();
        if animating {
            self.redraw();
        }
    }
}

/// 행 테스트 버튼 — 고리(결과 색: 회색 없음 · 노랑 진행 · 초록 성공 · 빨강 실패) + 가운데 점. 호버 시 바탕.
/// 결과가 와도 모양·기능은 같다(다시 누르면 재시도 · 사용자 09-14). `dim` = 헤더용 흐린 그림.
fn paint_test_btn(
    dc: &mut dyn DrawCtx,
    th: &Theme,
    cell: Rect,
    mark: Option<TestMark>,
    hover: bool,
    dim: bool,
) {
    if cell.w <= 0 || cell.h <= 0 {
        return;
    }
    let d = (cell.h * 3 / 5).max(8);
    let x = cell.x + (cell.w - d) / 2;
    let y = cell.y + (cell.h - d) / 2;
    if hover {
        dc.fill_rect(cell, th.panel_bg_alt);
    }
    let ring = match mark {
        _ if dim => th.border,
        Some(TestMark::Testing) => th.warn,
        Some(TestMark::Ok) => th.ok,
        Some(TestMark::Failed) => th.danger,
        None => th.text_dim,
    };
    let bg = if hover { th.panel_bg_alt } else { th.panel_bg };
    dc.fill_ellipse(Rect::new(x, y, d, d), ring);
    let t = 2;
    dc.fill_ellipse(Rect::new(x + t, y + t, d - 2 * t, d - 2 * t), bg);
    // 가운데: 성공 = 채운 점 · 실패 = 가로 막대 · 진행/없음 = 작은 점.
    let c = (x + d / 2, y + d / 2);
    match mark {
        Some(TestMark::Ok) if !dim => {
            let r = (d / 4).max(2);
            dc.fill_ellipse(Rect::new(c.0 - r, c.1 - r, 2 * r, 2 * r), th.ok);
        }
        Some(TestMark::Failed) if !dim => {
            let r = (d / 4).max(2);
            dc.fill_rect(Rect::new(c.0 - r, c.1 - 1, 2 * r, 2), th.danger);
        }
        _ => {
            let r = (d / 6).max(1);
            dc.fill_ellipse(Rect::new(c.0 - r, c.1 - r, 2 * r, 2 * r), ring);
        }
    }
}

/// 행 접속 버튼 — 오른쪽 삼각형(재생). 기본 어두운 회색 · 접속 중 파랑 · 접속됨 초록(사용자 09-14) · 호버 강조.
fn paint_connect_btn(
    dc: &mut dyn DrawCtx,
    th: &Theme,
    cell: Rect,
    mark: Option<ConnectMark>,
    hover: bool,
    dim: bool,
) {
    if cell.w <= 0 || cell.h <= 0 {
        return;
    }
    let d = (cell.h * 3 / 5).max(8);
    let x = cell.x + (cell.w - d) / 2;
    let y = cell.y + (cell.h - d) / 2;
    if hover {
        dc.fill_rect(cell, th.panel_bg_alt);
    }
    let color = match mark {
        _ if dim => th.border,
        Some(ConnectMark::Connecting) => th.accent,
        Some(ConnectMark::Connected) => th.ok,
        None if hover => th.accent,
        None => th.text_dim,
    };
    let inset = d / 6;
    dc.fill_triangle(
        (x + inset, y + inset),
        (x + d - inset / 2, y + d / 2),
        (x + inset, y + d - inset),
        color,
    );
}

/// 열 이동(결과 그리드와 동일): `pos`를 빼고 `to`(drop 위치 · len 허용)에 넣되, 뒤로 옮길 땐 빠진 칸만큼 당긴다.
fn move_col(order: &mut Vec<usize>, pos: usize, mut to: usize) {
    if pos >= order.len() {
        return;
    }
    let c = order.remove(pos);
    if to > pos {
        to -= 1;
    }
    order.insert(to.min(order.len()), c);
}

/// 행 높이 안 글자 세로 위치 계산용(라스터 글꼴 높이 ≈ 행 높이 - 8px 여백).
fn dc_text_h(row_h: i32) -> i32 {
    (row_h - 8).max(8)
}

/// `host:port/database` (파일 DB는 경로).
fn target_of(p: &Profile) -> String {
    let host = p.spec.host.as_deref().unwrap_or("");
    let db = p.spec.database.as_deref().unwrap_or("");
    match p.spec.port {
        Some(port) if !host.is_empty() => format!("{host}:{port}/{db}"),
        _ if host.is_empty() => db.to_string(),
        _ => format!("{host}/{db}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_script::ConnectSpec;

    fn prof(name: &str, user: Option<&str>) -> Profile {
        Profile {
            name: name.into(),
            spec: ConnectSpec {
                user: user.map(String::from),
                ..ConnectSpec::default()
            },
            has_password: true,
        }
    }

    #[test]
    fn sort_keys_follow_grid_rules() {
        let mut k = Vec::new();
        toggle_sort_key(&mut k, 0, false);
        assert_eq!(k, vec![(0, true)]);
        toggle_sort_key(&mut k, 0, false);
        assert_eq!(k, vec![(0, false)]);
        toggle_sort_key(&mut k, 1, true);
        assert_eq!(k, vec![(0, false), (1, true)], "Shift = 키 추가");
        toggle_sort_key(&mut k, 1, true);
        assert_eq!(k, vec![(0, false), (1, false)], "Shift 재클릭 = 방향 전환");
        toggle_sort_key(&mut k, 1, true);
        assert_eq!(k, vec![(0, false)], "세 번째 = 제거");
        toggle_sort_key(&mut k, 0, false);
        assert_eq!(k, vec![], "▼ 상태에서 일반 클릭 = 해제");
        toggle_sort_key(&mut k, 2, false);
        assert_eq!(k, vec![(2, true)], "일반 클릭 = 단일 키");
    }

    #[test]
    fn move_col_forward_backward_end_and_noop() {
        let mut o = vec![0, 1, 2, 3];
        move_col(&mut o, 0, 3);
        assert_eq!(o, vec![1, 2, 0, 3], "앞 열을 3번째 앞으로");
        move_col(&mut o, 3, 0);
        assert_eq!(o, vec![3, 1, 2, 0], "뒤 열을 맨 앞으로");
        move_col(&mut o, 1, 4);
        assert_eq!(o, vec![3, 2, 0, 1], "맨 끝(len)으로");
        move_col(&mut o, 2, 2);
        assert_eq!(o, vec![3, 2, 0, 1], "제자리");
        move_col(&mut o, 2, 3);
        assert_eq!(o, vec![3, 2, 0, 1], "바로 다음 칸 = 제자리");
    }

    #[test]
    fn sort_shown_is_case_insensitive_stable_and_empty_last() {
        let ps = vec![
            prof("b", Some("x")),
            prof("A", None),
            prof("c", Some("x")),
            prof("B", None),
        ];
        let mut shown: Vec<usize> = (0..4).collect();
        sort_shown(&ps, &mut shown, &[(0, true)]);
        assert_eq!(shown, vec![1, 0, 3, 2], "이름 대소문자 무관(A · b · B · c)");
        sort_shown(&ps, &mut shown, &[(2, true), (0, false)]);
        assert_eq!(shown, vec![2, 0, 3, 1], "빈 사용자는 뒤 · 2차 키 이름 내림차순");
        sort_shown(&ps, &mut shown, &[]);
        assert_eq!(shown, vec![2, 0, 3, 1], "키 없음 = 그대로");
    }
}
