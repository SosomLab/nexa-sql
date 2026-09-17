//! 접속 창(별도 창 · Golden "Database Login" 차용 · DR-23 · T-31 · 사용자 09-14).
//!
//! 기본 = **로그인 목록**만(저장 프로필 · 필터 · 이름/종류/사용자/대상) + New/Edit/Delete/Close.
//! New/Edit를 누르면 오른쪽에서 **상세 폼**([`ConnectPanel`] — DB 종류·호스트/포트·DB·사용자·비밀번호·프로필 · Test/Connect/Save · 상태)이
//! 슬라이딩해 들어온다(≈200ms ease-out · Esc = 접기). 목록은 그만큼 좁아진다.
//! 행 클릭 = 폼에 채움 · 더블클릭 = 바로 접속 · 접속되면 창이 닫힌다. `Ctrl/⌘+L` · 툴바 ⇄ · Run ▸ Connect로 연다.
//! 창 골격은 로그 창과 같다(winit + softbuffer + nexa-ctl 래스터). I/O(저장소·워커)는 전부 호스트 몫 — [`ConnWinAction`]으로 요청.

use crate::connect::{ConnState, ConnectPanel, PanelAction};
use crate::probe::{self, ProbeEntry, ProbeHub, ProbePolicy, ProbeReq, ProbeStatus};
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{draw_tooltip, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{Color, FontPrefs, SlotFont, Theme};

/// 신호등 '불가'(빨강) — 테마 `danger`(어두운 빨강)가 아니라 밝은 순빨강 `#FF0000`(사용자 09-15).
const LIGHT_RED: Color = Color(0x00FF_0000);
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{
    Button, Control, EditCtxAction, FiredBy, InputEvent, Invalidations, Key as CtlKey, ScrollBars,
    TextBox, TimeoutButton, Widget,
};

/// ★ 접속 창 조정값(사용자 09-14 "구현 값은 설정으로 · 자주 안 바꾸는 값은 비노출") — 설정 레지스트리에서 부팅 시 주입.
/// 기본값은 설계 상수와 같다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConnTuning {
    /// 삭제 확인 대기(ms · `conn.delete_confirm_ms`).
    pub delete_confirm_ms: u64,
    /// 접속 성공 초록 표시 뒤 창 닫기까지(ms · `conn.close_after_connect_ms`).
    pub close_after_ms: u64,
    /// 툴팁 지연(ms · `ui.tooltip_delay_ms`).
    pub tooltip_ms: u128,
    /// 더블클릭 간격(ms · `ui.dblclick_ms`).
    pub dblclick_ms: u128,
    /// 상세 폼 슬라이딩(ms · `ui.slide_ms`).
    pub slide_ms: f32,
    /// 기본 창 크기(논리 px · `conn.window_w/h`).
    pub window_w: f32,
    pub window_h: f32,
    /// 상세 폼 폭(논리 px · `conn.panel_w`).
    pub panel_w: f32,
    /// 상단 버튼 폭 배율(`conn.button_scale_pct` / 100).
    pub button_scale: f32,
    /// Port 입력란 폭(논리 px · `conn.port_w`).
    pub port_w: f32,
}

impl Default for ConnTuning {
    fn default() -> Self {
        ConnTuning {
            delete_confirm_ms: 5000,
            close_after_ms: 450,
            tooltip_ms: 600,
            dblclick_ms: 400,
            slide_ms: 200.0,
            window_w: 640.0,
            window_h: 520.0,
            panel_w: 292.0,
            button_scale: 1.32,
            port_w: 58.0,
        }
    }
}
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use nsql_vault::{Profile, Vault};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
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
    /// 우클릭 메뉴 Duplicate — `<이름>_Copied`로 복제(비밀번호 포함 · 사용자 09-14).
    Duplicate(String),
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

/// 텍스트 열(원본 index) — 이름 · 종류 · 사용자 · 대상.
const TEXT_COLS: usize = 5;
/// 첫 배치의 폭 비율(사용자가 폭을 조절하기 전까지 창 폭을 따라간다) — 이름 · 종류 · 사용자 · 비밀번호(체크) · 대상.
const COL_FRACS: [f32; TEXT_COLS] = [0.20, 0.14, 0.18, 0.10, 0.38];
/// 비밀번호 열(원본 index) — 체크박스로 그린다(진하게 = 저장됨 · 연하게 = 세션에만 입력됨).
const COL_PASSWORD: usize = 3;
const MIN_COL_W: i32 = 24;

/// 비밀번호 상태 — 2 = 저장됨(봉투) · 1 = 이 실행 동안 입력됨(세션 보관) · 0 = 없음.
fn pw_state(p: &Profile, session: &HashMap<String, String>) -> u8 {
    if p.has_password {
        2
    } else if session.contains_key(&p.name) {
        1
    } else {
        0
    }
}

/// 행의 열 텍스트(그리기·정렬·필터 공용). 비밀번호 열은 상태 숫자(정렬용 · 그리기는 체크박스).
fn cell_of(p: &Profile, col: usize, session: &HashMap<String, String>) -> String {
    match col {
        0 => p.name.clone(),
        1 => p
            .spec
            .dialect
            .map(|d| d.display_name())
            .unwrap_or("")
            .to_string(),
        2 => p.spec.user.clone().unwrap_or_default(),
        COL_PASSWORD => pw_state(p, session).to_string(),
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
fn sort_shown(
    profiles: &[Profile],
    shown: &mut [usize],
    keys: &[(usize, bool)],
    session: &HashMap<String, String>,
) {
    if keys.is_empty() {
        return;
    }
    let key_of = |i: usize, col: usize| cell_of(&profiles[i], col, session).to_lowercase();
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
    /// 창 기하 기억(`wingeom::Memo`): 기록 위치·크기(같은 모니터일 때만 씀) · 마지막 닫힌 (위치, 크기).
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    /// 마지막으로 닫힐 때의 창 위치(물리 px) — 같은 모니터에서 다시 열면 그 자리(사용자 09-16).
    last_pos: Option<(i32, i32)>,
    /// 마지막으로 열 때 메인 창이 있던 모니터 — 메인 창이 다른 모니터로 갔으면 다시 가운데로.
    last_monitor: Option<winit::monitor::MonitorHandle>,
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
    /// 호버 행의 서서히 진해지는 강조 — `IntentFade`(의도 코얼레싱 · 진입 = `grid.hover_fade`).
    hover_fade: IntentFade,
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
    /// 조정값(설정 주입 · 기본 = 설계 상수).
    tuning: ConnTuning,
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
    /// ★ 세션 비밀번호(프로필 이름 → 입력값) — 저장하지 않은 비밀번호도 프로그램이 도는 동안 보관해
    /// Test/Connect·폼 채우기에 쓴다(사용자 09-14). 디스크에 쓰지 않는다.
    session_pw: HashMap<String, String>,
    /// 이 시각에 창을 닫는다(접속 성공 초록을 잠시 보여 준 뒤).
    close_at: Option<Instant>,
    /// 툴팁 대상 + 머물기 시작 시각(아이콘 열·행 버튼 위 · 다국어 · 사용자 09-14).
    tip: Option<(TipTarget, Instant)>,
    /// 목록 모드 결과 안내(상세 패널이 닫혀 있을 때 목록 오른쪽 아래 · 사용자 09-16) — (프로필 이름, 상태).
    note: Option<(String, ConnState)>,
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
    /// ★ 삭제 2단 확인 — 첫 클릭으로 무장(빨간 타이머 버튼 · 5초 카운트다운) · 그 안에 재클릭 = 삭제 · 만료/Esc = 해제.
    del_arm: Option<(TimeoutButton, Instant)>,
    /// 행 우클릭 메뉴(nexa-ctl 공용 · 항목 id만 돌려준다) + 대상 프로필 + 라벨 폭(페인트 때 실측).
    menu: CtxMenu,
    ctx_target: Option<String>,
    ctx_text_w: i32,
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

impl ConnWin {
    pub(crate) fn new(panel: ConnectPanel) -> Self {
        let mut w = ConnWin {
            window: None,
            memo: crate::wingeom::Memo::default(),

            last: None,
            ctx: None,
            surface: None,
            scale: 1.0,
            last_pos: None,
            last_monitor: None,
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
            hover_fade: IntentFade::with_speed(FadeSpeed::Slow),
            col_w: Vec::new(),
            col_w_manual: false,
            sort_keys: Vec::new(),
            hdr_resize: None,
            col_order: (0..TEXT_COLS).collect(),
            tuning: ConnTuning::default(),
            hdr_drag: None,
            resize_cursor: false,
            btn_text_w: [0; 4],
            scroll_y: 0,
            scroll_x: 0,
            bars: ScrollBars::new(),
            test_marks: HashMap::new(),
            conn_marks: HashMap::new(),
            session_pw: HashMap::new(),
            close_at: None,
            tip: None,
            note: None,
            filter: TextBox::new(t(Msg::PhFilter)),
            btn_new: Button::new(t(Msg::BtnNew)),
            btn_edit: Button::new(t(Msg::BtnEdit)),
            btn_delete: Button::new(t(Msg::BtnDelete)),
            btn_close: Button::new(t(Msg::BtnClose)),
            list: Rect::new(0, 0, 0, 0),
            row_h: 24,
            focus: WFocus::Filter,
            focus_btn: 0,
            del_arm: None,
            menu: CtxMenu::new(),
            ctx_target: None,
            ctx_text_w: 0,
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

    /// 조정값 주입(부팅 시 한 번 · 설정 → 창·폼).
    pub(crate) fn set_tuning(&mut self, t: ConnTuning) {
        self.tuning = t;
        self.panel.set_port_w(t.port_w);
    }

    /// 접속 성공 초록 표시를 잠시 보여 준 뒤 닫는다(`conn.close_after_connect_ms`).
    pub(crate) fn close_after_connect(&mut self) {
        self.close_soon(Duration::from_millis(self.tuning.close_after_ms));
    }

    /// 프로브 스레드·정책 주입(부팅 시 한 번).
    pub(crate) fn set_probe(&mut self, hub: ProbeHub, policy: ProbePolicy) {
        self.hub = Some(hub);
        self.policy = policy;
    }

    pub(crate) fn policy(&self) -> &ProbePolicy {
        &self.policy
    }

    /// 정책만 교체(설정 변경 · 실행 속도 향상) — 허브는 그대로, 동시 상한만 함께 갱신.
    pub(crate) fn set_policy(&mut self, policy: ProbePolicy, max_inflight: usize) {
        self.policy = policy;
        if let Some(h) = self.hub.as_mut() {
            h.set_max_inflight(max_inflight);
            h.set_icmp(policy.icmp);
        }
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

    /// 프로필의 마지막 테스트 표시(진행 중 여부 판단).
    pub(crate) fn test_mark(&self, name: &str) -> Option<TestMark> {
        self.test_marks.get(name).copied()
    }

    /// 행 테스트 버튼의 결과 표시를 바꾼다(버튼 기능은 그대로).
    pub(crate) fn set_test_mark(&mut self, name: &str, mark: TestMark) {
        if name.is_empty() {
            return;
        }
        self.test_marks.insert(name.to_string(), mark);
        self.sync_enabled();
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

    /// 접속을 **시도**했다(Test/Connect 시작) — 결과와 무관하게 신호등 대상에 올린다(영속 · 사용자 09-16: 서버는 살아 있는데
    /// 포트가 안 열리는 대상도 실행 시 점검되어야 한다). 활성 프로필은 건드리지 않는다.
    pub(crate) fn mark_attempted(&mut self, name: &str) {
        if name.is_empty() || self.connected.contains(name) {
            return;
        }
        probe::mark_connected(name);
        self.connected.insert(name.to_string());
        self.schedule_probes();
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
            let sent = hub.request(ProbeReq {
                name: name.to_string(),
                host,
                port,
                timeout: self.policy.timeout,
            });
            if sent {
                e.status = ProbeStatus::Checking;
                e.next_at = None;
            } else {
                e.next_at = Some(now);
            }
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
            // 새 대상만 즉시 · 이미 예약된 항목은 그 예약을 지킨다(창을 자주 열어도 확인은 `probe.interval`마다 1회 · 사용자 09-14).
            self.probes
                .entry(p.name.clone())
                .or_insert_with(|| ProbeEntry::fresh(now));
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
                        // 확인이 필요한 대상만 · 각각 별도 스레드. 상한 초과면 예약을 유지해 다음 틱에.
                        let sent = hub.request(ProbeReq {
                            name: p.name.clone(),
                            host: h,
                            port,
                            timeout: self.policy.timeout,
                        });
                        if sent {
                            e.status = ProbeStatus::Checking;
                            e.next_at = None;
                            changed = true;
                        } else {
                            // 슬롯(동시 16) 초과 — 1초 뒤 다시(트래픽 0 · 깨우기만 · T-63).
                            next = Some(next.map_or(now + Duration::from_secs(1), |n: Instant| {
                                n.min(now + Duration::from_secs(1))
                            }));
                        }
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
                // T-63: 대상 집합(한 번 이상 시도) 밖 프로필은 즉시 확인 1회로 끝 — 재예약하지 않는다(시도한 적 없는 서버에 지속 트래픽 금지).
                if !self.connected.contains(&r.name) {
                    e.next_at = None;
                }
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

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
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
        self.sync_enabled();
        self.redraw();
    }

    /// 삭제 뒤 — 인접 항목(다음 · 마지막이었으면 이전)을 선택한다(사용자 09-14 권장안). 목록이 비면 선택 없음.
    /// 반환 = 폼이 펼쳐져 있을 때 폼에 채울 프로필 이름(`None` = 폼을 비워 New 상태로).
    pub(crate) fn after_delete(&mut self) -> Option<String> {
        let old = self.sel;
        self.refresh_profiles(None);
        self.sel = old.and_then(|i| (!self.shown.is_empty()).then(|| i.min(self.shown.len() - 1)));
        if let Some(s) = self.sel {
            self.ensure_visible(s);
        }
        self.sync_enabled();
        self.redraw();
        if !self.detail_open() {
            return None;
        }
        match self.selected_name() {
            Some(n) => Some(n),
            None => {
                self.panel.clear();
                None
            }
        }
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
        sort_shown(
            &self.profiles,
            &mut self.shown,
            &self.sort_keys,
            &self.session_pw,
        );
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
        let tip_ms = self.tuning.tooltip_ms;
        let tip_due = matches!(self.tip, Some((_, t)) if (tip_ms..tip_ms + 40).contains(&t.elapsed().as_millis()));
        let a = self.bars.tick(now_ms);
        let b = self.hover_fade.tick(now_ms);
        // 버튼 hover 페이드(상단 4개 + 폼 3개) — 지금까지 틱이 없어 즉시 켜졌다(사용자 09-14 500ms 요구).
        let c = self.btn_new.tick(now_ms)
            | self.btn_edit.tick(now_ms)
            | self.btn_delete.tick(now_ms)
            | self.btn_close.tick(now_ms)
            | self.panel.tick(now_ms)
            | self.filter.tick(now_ms);
        // 삭제 무장 카운트다운(자체 시계) — 만료면 해제.
        let d = match self.del_arm.as_mut() {
            Some((tb, epoch)) => {
                let changed = tb.tick(epoch.elapsed().as_millis() as u64);
                if tb.fired_by_timeout() {
                    let _ = tb.take_fired();
                    self.disarm_delete();
                    true
                } else {
                    changed
                }
            }
            None => false,
        };
        a || b || c || d || tip_due
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.window.is_some() && self.bars.is_visible()
    }

    pub(crate) fn hover_animating(&self) -> bool {
        self.window.is_some()
            && (self.del_arm.is_some()
                || self.hover_fade.is_animating()
                || self.btn_new.is_animating()
                || self.btn_edit.is_animating()
                || self.btn_delete.is_animating()
                || self.btn_close.is_animating()
                || self.filter.is_animating()
                || self.panel.animating())
    }

    /// 메인 창 위 가운데에 연다(`over` = 메인 창 바깥 좌표·크기). 이미 열려 있으면 앞으로.
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        // 창 규칙(사용자 09-16/09-17): 메인 창과 같은 모니터에 · 기록(설정 `window.login_pos/_size`)이 **같은 모니터**면
        // 기록된 크기로 기록된 위치에 · 다른 모니터면 기본 크기로 메인 창 가로/세로 가운데.
        let same = self.memo.on_same_monitor(owner);
        let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or((
            f64::from(self.tuning.window_w),
            f64::from(self.tuning.window_h),
        ));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinLogin)))
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        let monitor = owner.and_then(Window::current_monitor);
        if let Some(((x, y), _)) = same {
            attrs = attrs.with_position(crate::wingeom::logical(x, y));
        } else if let Some((x, y, w, h)) = over {
            // `over`는 물리 px · 창 크기는 논리 px → 메인 창 배율로 맞춘 뒤 가운데(2x에서 오른쪽 아래로 치우치던 것).
            let s = owner.map_or(1.0, Window::scale_factor);
            let (pw, ph) = ((lw * s) as i32, (lh * s) as i32);
            let cx = x + (w as i32 - pw) / 2;
            let cy = y + (h as i32 - ph) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        self.last_monitor = monitor;
        // 메인 창의 소유 창 — 작업표시줄 항목 하나 · 항상 메인 위(사용자 09-14).
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
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
        // 초록 화살표 = 지금 접속된 프로필 하나만(사용자 09-14) · 나머지는 회색.
        self.conn_marks.clear();
        if let Some(a) = self.active.clone() {
            self.conn_marks.insert(a, ConnectMark::Connected);
        }
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
        let p = if self.tuning.slide_ms <= 0.0 {
            1.0
        } else {
            (t0.elapsed().as_secs_f32() * 1000.0 / self.tuning.slide_ms).clamp(0.0, 1.0)
        };
        let e = 1.0 - (1.0 - p) * (1.0 - p); // ease-out
        self.detail_t = from + (to - from) * e;
        let done = p >= 1.0;
        if done {
            self.detail_t = to;
            self.anim = None;
        }
        // 창 자체가 오른쪽으로 커진다/줄어든다(목록 폭은 그대로 · 폼은 새 영역에 드러난다).
        if let Some(w) = &self.window {
            let lw = f64::from(self.tuning.window_w + self.tuning.panel_w * self.detail_t);
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(
                lw,
                f64::from(self.tuning.window_h),
            ));
        }
        !done
    }

    /// 목록 모드 결과 안내 갱신(행 Test/접속 결과) — 상세 패널이 닫혀 있어도 오른쪽 아래에 보인다(사용자 09-16).
    pub(crate) fn set_note(&mut self, name: &str, st: ConnState) {
        self.note = Some((name.to_string(), st));
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        // 닫히는 자리를 기억(다음 열기 · 같은 모니터일 때만 재사용).
        if let Some(p) = self.window.as_ref().and_then(|w| w.outer_position().ok()) {
            self.last_pos = Some((p.x, p.y));
        }
        if let Some(w) = &self.window {
            if let Some(p) = crate::wingeom::outer_pos(w) {
                self.last = Some((p, crate::wingeom::logical_size(w)));
            }
        }
        self.surface = None;
        self.ctx = None;
        self.window = None;
        self.panel.set_focused(false);
    }

    /// 기억된 기하(설정 `window.<name>_pos`/`_size`) — 열 때 같은 모니터면 그대로 쓴다.
    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    /// 마지막으로 닫힌 (위치, 크기)(1회성 · 호스트가 설정에 저장).
    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
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
        let pw = self.s(self.tuning.panel_w);
        // 상세 폼 = 오른쪽에서 detail_t만큼 들어와 있다(닫힘 = 창 밖).
        let open_px = (pw as f32 * self.detail_t).round() as i32;
        self.panel.set_bounds(Rect::new(w - open_px, 0, pw, h), s);
        let x0 = pad;
        let rw = (w - open_px - x0 - pad).max(0);
        let row = self.s(28.0);
        // 버튼 폭 = 라벨 폭 + 좌우 여백(라벨 미측정이면 72) · 간격 = pad/2(사용자 09-14 "간격 1/3" → "1.5배로").
        let gap = (pad / 2).max(3);
        // 모든 버튼 동일 폭 = (현재 언어에서 가장 긴 라벨 + 여백) × 1.32(사용자 09-14 "20% 넓게" → "10% 더" · i18n).
        let widest = self.btn_text_w.iter().copied().max().unwrap_or(0);
        let uniform = if widest > 0 {
            ((widest + pad * 2) as f32 * self.tuning.button_scale).round() as i32
        } else {
            self.s(72.0)
        };
        let ws = [uniform; 4];
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
        self.btn_delete.set_bounds(
            Rect::new(bx + ws[0] + ws[1] + gap * 2, y1, ws[2], row),
            &mut inv,
        );
        if let Some((tb, _)) = self.del_arm.as_mut() {
            tb.set_bounds(
                Rect::new(bx + ws[0] + ws[1] + gap * 2, y1, ws[2], row),
                &mut inv,
            );
            tb.set_scale(s);
        }
        self.btn_close.set_bounds(
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

    /// 행의 유효 폭(아이콘 열 + 텍스트 열 · 가로 스크롤 반영) — 그 뒤는 **빈 영역**(선택·호버 배경 없음 · 항목 클릭 아님 ·
    /// 전체/빈 영역용 메뉴 자리 · dir2 파일 그리드와 동일 · 사용자 09-14).
    fn row_extent(&self) -> Rect {
        let body = self.body_rect();
        let w = (self.content_size().0 - self.scroll_x).clamp(0, body.w);
        Rect::new(body.x, body.y, w, body.h)
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        let body = self.row_extent();
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
        self.row_at(p).map(|row| TipTarget {
            col,
            row: Some(row),
        })
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
                if pw_state(p, &self.session_pw) == 0 {
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
                if pw_state(p, &self.session_pw) == 0 {
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

    /// 행의 프로필에 비밀번호가 있는가(저장됨 또는 세션 입력) — 없으면 Test/Connect 행 버튼 비활성.
    fn row_has_password(&self, row: usize) -> bool {
        self.shown
            .get(row)
            .and_then(|&i| self.profiles.get(i))
            .is_some_and(|p| pw_state(p, &self.session_pw) > 0)
    }

    /// 이름으로 행 선택(행 Test/Connect가 폼 Test/Connect와 같은 경로를 타도록 · 사용자 09-14).
    pub(crate) fn select_by_name(&mut self, name: &str) {
        self.sel = self
            .shown
            .iter()
            .position(|&i| self.profiles[i].name == name);
        if let Some(s) = self.sel {
            self.ensure_visible(s);
        }
        self.sync_enabled();
        self.redraw();
    }

    /// 상세 폼이 펼쳐져 있는가(호스트가 폼 채우기 여부를 판단).
    pub(crate) fn is_detail_open(&self) -> bool {
        self.detail_open()
    }

    /// 입력된 비밀번호를 세션에 보관(빈 값은 무시 · 프로필 이름이 있을 때만).
    pub(crate) fn remember_pw(&mut self, name: &str, pw: &str) {
        if !name.is_empty() && !pw.is_empty() {
            self.session_pw.insert(name.to_string(), pw.to_string());
            self.redraw();
        }
    }

    /// 스펙에 비밀번호가 없으면 세션 보관값을 채운다(Test/Connect/폼 채우기).
    pub(crate) fn with_session_pw(&self, name: &str, mut spec: ConnectSpec) -> ConnectSpec {
        if spec.password.as_deref().unwrap_or("").is_empty() {
            if let Some(pw) = self.session_pw.get(name) {
                spec.password = Some(pw.clone());
            }
        }
        spec
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

    /// Details·Delete는 선택 항목이 있을 때만(사용자 09-14) — Details는 펼쳐진 폼을 접는 용도로는 항상.
    fn sync_enabled(&mut self) {
        let has_sel = self.sel.is_some();
        self.btn_edit.set_enabled(has_sel || self.detail_open());
        self.btn_delete.set_enabled(has_sel);
        if !has_sel && self.del_arm.is_some() {
            self.disarm_delete();
        }
        // 폼의 프로필이 테스트 중이면 폼 Test/Connect 잠금(다른 프로필은 무관 · 사용자 09-14).
        let testing = self.is_testing(&self.panel.profile_name());
        self.panel.set_testing_lock(testing);
    }

    /// 프로필이 테스트 진행 중인가(행·폼 Test/Connect 잠금 기준).
    pub(crate) fn is_testing(&self, name: &str) -> bool {
        self.test_marks.get(name) == Some(&TestMark::Testing)
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

    /// 삭제 무장 — Delete 자리에 빨간 타이머 버튼(5초). 한 번 더 누르면 삭제.
    fn arm_delete(&mut self) {
        // 잔여 시간 숫자 없이 라벨 + 게이지(진척률)만(사용자 09-14) — 빨간 경고 톤.
        let mut tb = TimeoutButton::new(t(Msg::BtnDelete), self.tuning.delete_confirm_ms)
            .with_warn(true)
            .with_suffix(t(Msg::UnitSecShort))
            .with_show_remaining(false);
        let mut inv = Invalidations::default();
        tb.set_bounds(self.btn_delete.bounds(), &mut inv);
        tb.set_scale(self.scale);
        tb.start(0);
        tb.set_focused(true);
        // 가려지는 원래 Delete의 일시 상태(hover·눌림·포커스)를 비운다 — 타임아웃 뒤 하늘색이 남던 버그(사용자 09-14).
        self.btn_delete.clear_transient();
        self.del_arm = Some((tb, Instant::now()));
        self.redraw();
    }

    /// 삭제 무장 해제(만료 · Esc · 삭제 뒤) — 다시 보이는 Delete는 현재 커서로 hover를 재판정한다.
    fn disarm_delete(&mut self) {
        if self.del_arm.take().is_some() {
            let (x, y) = self.cursor;
            let mut inv = Invalidations::default();
            self.btn_delete
                .on_event(&InputEvent::MouseMove { x, y }, &mut inv);
            self.redraw();
        }
    }

    /// 무장 버튼의 발화 처리 — 클릭 = 삭제 · 만료 = 해제.
    fn poll_delete_arm(&mut self, out: &mut Vec<ConnWinAction>) {
        let Some((tb, _)) = self.del_arm.as_mut() else {
            return;
        };
        match tb.take_fired() {
            Some(FiredBy::Click) => {
                if let Some(n) = self.selected_name() {
                    out.push(ConnWinAction::Delete(n));
                }
                self.disarm_delete();
            }
            Some(FiredBy::Timeout) => self.disarm_delete(),
            None => {}
        }
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
        if self.btn_delete.take_clicked()
            && self.selected_name().is_some()
            && self.del_arm.is_none()
        {
            self.arm_delete();
        }
        if self.btn_close.take_clicked() {
            self.close();
            return true;
        }
        false
    }

    /// 지금 포커스인 텍스트박스(필터 · 폼 입력란) — 클립보드·전체 선택의 대상.
    fn focused_tb(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            WFocus::Filter => Some(&mut self.filter),
            WFocus::Panel => self.panel.focused_textbox(),
            WFocus::List | WFocus::Button => None,
        }
    }

    /// 클립보드 행동(Ctrl+C/X/V · 우클릭 메뉴) — 메인 창과 같은 `clipboard` 모듈(OS 3종).
    fn clip(&mut self, act: EditCtxAction) {
        let mut inv = Invalidations::default();
        let was_filter = self.focus == WFocus::Filter;
        match act {
            EditCtxAction::Copy => {
                if let Some(text) = self.focused_tb().and_then(|tb| tb.copy_selection()) {
                    let _ = crate::clipboard::write_text(&text);
                }
            }
            EditCtxAction::Cut => {
                if let Some(text) = self.focused_tb().and_then(|tb| tb.cut_selection(&mut inv)) {
                    let _ = crate::clipboard::write_text(&text);
                }
            }
            EditCtxAction::Paste => {
                if let Some(text) = crate::clipboard::read_text() {
                    if let Some(tb) = self.focused_tb() {
                        tb.paste(&text, &mut inv);
                    }
                }
            }
            EditCtxAction::Custom(_) => {}
        }
        if was_filter {
            self.refilter();
        }
        self.redraw();
    }

    /// 모달(콤보 팝업·우클릭 메뉴)이 닫힌 직후 — 마우스는 움직이지 않았지만 커서 아래 대상이 바뀌었으므로
    /// 현재 커서 위치로 MouseMove를 한 번 합성해 흘린다(클릭은 통과시키지 않는다 · 호버 페이드만 즉시 시작).
    fn rehover(&mut self, out: &mut Vec<ConnWinAction>) {
        let (x, y) = self.cursor;
        self.route(InputEvent::MouseMove { x, y }, out);
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
                        if self.del_arm.is_some() {
                            self.disarm_delete();
                        } else if self.detail_open() {
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
                    Key::Named(
                        NamedKey::PageDown | NamedKey::PageUp | NamedKey::Home | NamedKey::End,
                    ) if matches!(self.focus, WFocus::List) && !self.shown.is_empty() => {
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
                    // 클립보드·전체 선택(Ctrl/⌘) — 포커스 텍스트박스(필터 · 폼 입력란).
                    Key::Character(c) if self.primary && matches!(c, "c" | "C") => {
                        self.clip(EditCtxAction::Copy)
                    }
                    Key::Character(c) if self.primary && matches!(c, "x" | "X") => {
                        self.clip(EditCtxAction::Cut)
                    }
                    Key::Character(c) if self.primary && matches!(c, "v" | "V") => {
                        self.clip(EditCtxAction::Paste)
                    }
                    Key::Character(c) if self.primary && !self.shift && matches!(c, "z" | "Z") => {
                        let mut inv = Invalidations::default();
                        if let Some(tb) = self.focused_tb() {
                            tb.on_event(&InputEvent::Undo, &mut inv);
                        }
                        self.redraw();
                    }
                    Key::Character(c) if self.primary && matches!(c, "z" | "Z" | "y" | "Y") => {
                        let mut inv = Invalidations::default();
                        if let Some(tb) = self.focused_tb() {
                            tb.on_event(&InputEvent::Redo, &mut inv);
                        }
                        self.redraw();
                    }
                    Key::Character(c) if self.primary && matches!(c, "a" | "A") => {
                        let mut inv = Invalidations::default();
                        if let Some(tb) = self.focused_tb() {
                            tb.on_event(&InputEvent::SelectAll, &mut inv);
                        }
                        self.redraw();
                    }
                    Key::Named(NamedKey::Delete) if matches!(self.focus, WFocus::List) => {
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
            // 휠 변환은 한 곳(`input::wheel_event` · 방향 반전 설정 포함).
            WindowEvent::MouseWheel { delta, .. } => crate::input::wheel_event(delta, self.shift),
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
        self.route_inner(ev, out);
        self.sync_enabled();
    }

    fn route_inner(&mut self, ev: InputEvent, out: &mut Vec<ConnWinAction>) {
        let mut inv = Invalidations::default();
        // 우클릭 직전 — 편집 메뉴의 "붙여넣기" 활성 여부.
        if matches!(ev, InputEvent::RightDown { .. }) {
            let has = crate::clipboard::read_text().is_some_and(|s| !s.is_empty());
            if let Some(tb) = self.focused_tb() {
                tb.set_clipboard_has_text(has);
            }
        }
        // ★ 팝업 메뉴 UX(사용자 09-14): 열린 메뉴(입력란 편집 메뉴 · 우클릭 메뉴)는 모달이지만, **바깥 좌/우클릭은 메뉴를 닫은 뒤
        //   그 클릭을 그대로 진행**한다(다른 컨트롤 선택·포커스 이동·새 메뉴가 한 번의 클릭으로) — 항목을 골랐거나 키(Esc)로 닫았으면 여기서 끝.
        let outside_click = matches!(
            ev,
            InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
        );
        if self.filter.popup_open() {
            self.filter.on_event(&ev, &mut inv);
            if let Some(act) = self.filter.take_edit_ctx() {
                self.clip(act);
                self.redraw();
                return;
            }
            if self.filter.popup_open() || !outside_click {
                if !self.filter.popup_open() {
                    self.rehover(out);
                }
                self.redraw();
                return;
            }
            // 바깥 클릭으로 닫혔다 → 아래 일반 경로로 계속.
        }
        if self.panel.edit_menu_open() {
            if let Some(PanelAction::Edit(act)) = self.panel.route(&ev, &mut inv) {
                self.clip(act);
                self.redraw();
                return;
            }
            if self.panel.edit_menu_open() || !outside_click {
                if !self.panel.edit_menu_open() {
                    self.rehover(out);
                }
                self.redraw();
                return;
            }
        }
        // 열린 우클릭 메뉴 — 고른 항목 id만 받아 실행 · 바깥 클릭은 닫고 계속.
        if self.menu.is_open() && self.menu.on_event(&ev) {
            let picked = self.menu.take_picked();
            if picked.is_none() && !self.menu.is_open() && outside_click {
                // 바깥 클릭으로 닫힘 → 그 클릭을 그대로 진행.
            } else {
                match picked.as_deref() {
                    Some("dup") => {
                        if let Some(n) = self.ctx_target.take() {
                            out.push(ConnWinAction::Duplicate(n));
                        }
                    }
                    Some("new") => self.open_detail(true),
                    // 콤보 우클릭: 선택 항목 / 목록(여러 줄) 복사(사용자 09-14).
                    Some("combo_item") => {
                        let _ = crate::clipboard::write_text(&self.panel.dialect_selected_label());
                    }
                    Some("combo_list") => {
                        let _ = crate::clipboard::write_text(&self.panel.dialect_labels());
                    }
                    _ => {}
                }
                if !self.menu.is_open() {
                    self.rehover(out);
                }
                self.redraw();
                return;
            }
        }
        if let InputEvent::RightDown { x, y } = ev {
            let p = Point { x, y };
            // 입력란 우클릭 = 그 입력란의 편집 메뉴(필터 · 폼).
            if self.filter.bounds().contains(p) {
                self.set_focus(WFocus::Filter);
                let has = crate::clipboard::read_text().is_some_and(|s| !s.is_empty());
                self.filter.set_clipboard_has_text(has);
                self.filter.on_event(&ev, &mut inv);
                self.redraw();
                return;
            }
            // 종류 콤보 우클릭 = 선택 항목 복사 · 목록 복사.
            if self.detail_t > 0.0 && self.panel.dialect_bounds().contains(p) {
                self.set_focus(WFocus::Panel);
                let host = self
                    .window
                    .as_ref()
                    .map(|w| {
                        let sz = w.inner_size();
                        Rect::new(0, 0, sz.width as i32, sz.height as i32)
                    })
                    .unwrap_or(self.list);
                self.menu.set_scale(self.scale);
                self.menu.open_at(
                    x,
                    y,
                    vec![
                        CtxItem::item("combo_item", t(Msg::MnCopyItem)),
                        CtxItem::item("combo_list", t(Msg::MnCopyList)),
                    ],
                    host,
                    self.ctx_text_w,
                );
                self.redraw();
                return;
            }
            if self.detail_t > 0.0 && self.panel.bounds().contains(p) {
                self.set_focus(WFocus::Panel);
                // 폼: 눌린 입력란으로 포커스 이동 뒤 우클릭 전달.
                self.panel.route(
                    &InputEvent::MouseDown {
                        x,
                        y,
                        shift: false,
                        primary: false,
                    },
                    &mut inv,
                );
                self.panel.route(&InputEvent::MouseUp { x, y }, &mut inv);
                let has = crate::clipboard::read_text().is_some_and(|s| !s.is_empty());
                if let Some(tb) = self.panel.focused_textbox() {
                    tb.set_clipboard_has_text(has);
                }
                if let Some(PanelAction::Edit(act)) = self.panel.route(&ev, &mut inv) {
                    self.clip(act);
                }
                self.redraw();
                return;
            }
            // 빈 영역(마지막 컬럼 뒤 · 행 아래) 우클릭 = 전체/빈 영역 메뉴(New).
            if self.body_rect().contains(p) && self.row_at(p).is_none() {
                self.set_focus(WFocus::List);
                let host = self
                    .window
                    .as_ref()
                    .map(|w| {
                        let sz = w.inner_size();
                        Rect::new(0, 0, sz.width as i32, sz.height as i32)
                    })
                    .unwrap_or(self.list);
                self.menu.set_scale(self.scale);
                self.menu.open_at(
                    x,
                    y,
                    vec![CtxItem::item("new", t(Msg::BtnNew))],
                    host,
                    self.ctx_text_w,
                );
                self.redraw();
                return;
            }
            if let Some(row) = self.row_at(p) {
                self.set_focus(WFocus::List);
                self.sel = Some(row);
                self.ctx_target = self.name_at(row);
                let host = self
                    .window
                    .as_ref()
                    .map(|w| {
                        let sz = w.inner_size();
                        Rect::new(0, 0, sz.width as i32, sz.height as i32)
                    })
                    .unwrap_or(self.list);
                self.menu.set_scale(self.scale);
                self.menu.open_at(
                    x,
                    y,
                    vec![CtxItem::item("dup", t(Msg::MnDuplicate))],
                    host,
                    self.ctx_text_w,
                );
                self.redraw();
                return;
            }
        }
        // 열린 콤보(폼)는 모달.
        if self.panel.popup_open() {
            if let Some(a) = self.panel.route(&ev, &mut inv) {
                out.push(ConnWinAction::Panel(a));
            }
            if !self.panel.popup_open() {
                // 팝업이 방금 닫혔다 — 커서 아래 대상(예: Details 버튼)의 hover를 바로 다시 판정(사용자 09-14).
                self.rehover(out);
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
            // 폼 위의 휠 = 포커스와 무관하게 폼으로(상태 메시지 스크롤 등).
            match self.panel.route(&ev, &mut inv) {
                Some(PanelAction::Edit(act)) => self.clip(act),
                Some(a) => out.push(ConnWinAction::Panel(a)),
                None => {}
            }
            self.redraw();
            return;
        } else if is_mouse && !over_panel && self.route_bars(&ev) {
            return;
        }
        if let InputEvent::MouseDown { x, y, shift, .. } = ev {
            let p = Point { x, y };
            if self.panel.bounds().contains(p) && self.detail_t > 0.0 {
                self.set_focus(WFocus::Panel);
            } else if let Some(i) = self.top_btn_at(p).filter(|&i| {
                [
                    &self.btn_new,
                    &self.btn_edit,
                    &self.btn_delete,
                    &self.btn_close,
                ][i]
                    .is_enabled()
            }) {
                // 누른 버튼만 테두리(마지막으로 눌린 하나) — 비활성 버튼은 포커스도 받지 않는다.
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
            } else if self.body_rect().contains(p) && self.row_at(p).is_none() {
                // 빈 영역 클릭 = 선택 해제(아무것도 선택되지 않은 상태 · 사용자 09-14) — 폼이 펼쳐져 있으면 New 상태로 비운다.
                self.set_focus(WFocus::List);
                self.sel = None;
                self.last_click = None;
                if self.detail_open() {
                    self.panel.clear();
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
                    // 비밀번호가 없는 프로필은 행 버튼이 동작하지 않는다 · 테스트 중인 프로필도 끝날 때까지 잠금(사용자 09-14).
                    let testing = self.name_at(row).is_some_and(|n| self.is_testing(&n));
                    if self.row_has_password(row) && !testing {
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
                let double = matches!(self.last_click, Some((r, t)) if r == row && t.elapsed().as_millis() < self.tuning.dblclick_ms);
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
            // 필터 입력란: 다운(캐럿) · 이동(드래그 선택) · 업 — 포커스일 때만(다른 곳 드래그가 새지 않게).
            if self.focus == WFocus::Filter {
                self.filter.on_event(&ev, &mut inv);
                if let Some(act) = self.filter.take_edit_ctx() {
                    self.clip(act);
                }
            }
            self.btn_new.on_event(&ev, &mut inv);
            self.btn_edit.on_event(&ev, &mut inv);
            match self.del_arm.as_mut() {
                Some((tb, _)) => tb.on_event(&ev, &mut inv),
                None => self.btn_delete.on_event(&ev, &mut inv),
            }
            self.btn_close.on_event(&ev, &mut inv);
            self.poll_delete_arm(out);
            if self.handle_top_clicks(out) {
                return;
            }
            if self.detail_t > 0.0 {
                match self.panel.route(&ev, &mut inv) {
                    Some(PanelAction::Edit(act)) => self.clip(act),
                    Some(a) => out.push(ConnWinAction::Panel(a)),
                    None => {}
                }
            }
        } else {
            match self.focus {
                WFocus::Panel => match self.panel.route(&ev, &mut inv) {
                    Some(PanelAction::Edit(act)) => self.clip(act),
                    Some(a) => out.push(ConnWinAction::Panel(a)),
                    None => {}
                },
                WFocus::Filter => {
                    self.filter.on_event(&ev, &mut inv);
                    if let Some(act) = self.filter.take_edit_ctx() {
                        self.clip(act);
                    }
                    self.refilter();
                }
                WFocus::List => {}
                WFocus::Button => {
                    // 포커스 버튼은 Enter/Space로 눌린다. 무장된 Delete는 Enter/Space = 확인 삭제.
                    if self.focus_btn == 2 && self.del_arm.is_some() {
                        if let InputEvent::Key {
                            key: CtlKey::Enter, ..
                        } = ev
                        {
                            if let Some(n) = self.selected_name() {
                                out.push(ConnWinAction::Delete(n));
                            }
                            self.disarm_delete();
                        }
                        self.redraw();
                        return;
                    }
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
        // 버튼 라벨 폭(현재 언어) — 배치 전에 잰다. 무장 라벨은 두 줄이라 폭에 영향 없음 → 버튼이 움직이지 않는다.
        // ★ 물리 px로 잰다(`layout`·메뉴 폭이 물리 px). 논리 px로 재던 동안 HiDPI에서 버튼이 좁아졌다
        //   (맥 2x 실기 09-16: 50px vs Windows 100% 72px — 여백만 배율이 붙고 글자 폭은 안 붙어서).
        let px = font_px * self.scale;
        self.btn_text_w = [Msg::BtnNew, Msg::BtnEdit, Msg::BtnDelete, Msg::BtnClose]
            .map(|m| ui.measure(t(m), px).ceil() as i32);
        self.ctx_text_w = [
            Msg::MnDuplicate,
            Msg::BtnNew,
            Msg::MnCopyItem,
            Msg::MnCopyList,
        ]
        .iter()
        .map(|m| ui.measure(t(*m), px))
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
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
        let extent_w = self.row_extent().w;
        let dragging = self.hdr_drag.filter(|d| d.3);
        let drop_pos = dragging.map(|d| self.drop_pos_at(d.2));
        let tip_ready = self
            .tip
            .filter(|(_, since)| since.elapsed().as_millis() >= self.tuning.tooltip_ms)
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
            match &self.del_arm {
                Some((tb, _)) => tb.paint(&mut dc, th),
                None => self.btn_delete.paint(&mut dc, th),
            }
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
                t(Msg::ColPassword).to_string(),
                t(Msg::ColTarget).to_string(),
            ];
            let col_w = self.col_w.clone();
            // 잉크 기준 세로 가운데(09-16 mac: 고정 −8px 상수는 Windows 1x 맑은 고딕에서만 가운데였다).
            let toff = dc.text_center_y(0, rh);
            let ty = move |y: i32| y + toff;
            let hcells = Rect::new(l.x + icons_w, l.y, (l.w - icons_w - 1).max(0), rh);
            let cells = Rect::new(
                l.x + icons_w,
                body.y,
                (body.right() - l.x - icons_w).max(0),
                body.h,
            );
            // 헤더
            dc.fill_rect(Rect::new(l.x, l.y, l.w, rh), th.chrome_bg);
            dc.fill_rect(Rect::new(l.x, l.y + rh - 1, l.w, 1), th.border);
            // 아이콘 열 머리 = 작은 아이콘(흐리게).
            paint_test_btn(
                &mut dc,
                th,
                Rect::new(l.x + sw, l.y, sw, rh),
                None,
                false,
                true,
            );
            paint_connect_btn(
                &mut dc,
                th,
                Rect::new(l.x + sw * 2, l.y, sw, rh),
                None,
                false,
                true,
            );
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
                // 선택/호버 배경은 유효 컬럼까지만(마지막 컬럼 뒤는 빈 영역 · 사용자 09-14).
                let r = Rect::new(l.x + 1, y, extent_w, rh).intersection(&body);
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
                    Some(ProbeStatus::Down) => LIGHT_RED,
                    Some(ProbeStatus::Unknown) | None => th.border,
                };
                let dot_r = Rect::new(l.x + (sw - dot) / 2 + 1, y + (rh - dot) / 2, dot, dot);
                if dot_r.intersection(&body) == dot_r {
                    dc.fill_ellipse(dot_r, dot_color);
                }
                // 행 버튼 — 테스트(마지막 결과 표시) · 접속. 비밀번호 미저장 = 흐리게(비활성).
                // 저장됐거나 세션에 입력된 비밀번호가 있으면 활성 · 테스트 중이면 잠금(사용자 09-14).
                let testing = self.test_marks.get(&p.name) == Some(&TestMark::Testing);
                let enabled = pw_state(p, &self.session_pw) > 0 && !testing;
                let hb = |b: RowBtn| enabled && self.hover_btn == Some((row, b));
                paint_test_btn(
                    &mut dc,
                    th,
                    Rect::new(l.x + sw, y, sw, rh).intersection(&body),
                    self.test_marks.get(&p.name).copied(),
                    hb(RowBtn::Test),
                    !enabled && !testing, // 테스트 중엔 노란 고리(진행) 표시 유지
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
                        if ci == COL_PASSWORD {
                            // 체크박스: 저장됨 = 진한 체크 · 세션 입력 = 연한 체크 · 없음 = 빈 상자. 크기 = 행 높이의 48%(3/5의 80% · 사용자 09-14).
                            let cs = (rh * 12 / 25).max(8);
                            let bx = Rect::new(cx + pad, y + (rh - cs) / 2, cs, cs);
                            dc.stroke_round_rect(bx, 3, th.border, 1.0);
                            match pw_state(p, &self.session_pw) {
                                2 => nexa_ctl::controls::draw_check_mark(&mut dc, bx, th.text),
                                1 => nexa_ctl::controls::draw_check_mark(&mut dc, bx, th.text_dim),
                                _ => {}
                            }
                        } else {
                            dc.text(
                                cx + pad,
                                ty(y),
                                clip,
                                &cell_of(p, ci, &self.session_pw),
                                th.text,
                            );
                        }
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
            // 목록 모드 결과 안내 — 상세 패널이 닫혀 있을 때 목록 오른쪽 아래(● + "이름 · 문구" 한 줄 · 왼쪽으로 잘림).
            if !detail_visible {
                if let Some((name, st)) = &self.note {
                    let (color, text) = match st {
                        ConnState::Idle => (th.text_dim, String::new()),
                        ConnState::Connecting => (th.warn, t(Msg::StConnectingShort).to_string()),
                        ConnState::Testing => (th.warn, t(Msg::StTesting).to_string()),
                        ConnState::Connected(d) => (th.ok, tf(Msg::StConnectedShort, &[d])),
                        ConnState::TestOk(d) => (th.ok, d.clone()),
                        ConnState::Failed(e) => (th.danger, e.clone()),
                    };
                    if !text.is_empty() {
                        dc.select_font(FontSlot::Status, false);
                        let line = format!("{name} · {text}");
                        let nh = dc.text_height() + pad;
                        let nr = Rect::new(list.x + 1, list.bottom() - 1 - nh, list.w - 2, nh);
                        dc.fill_rect(nr, th.panel_bg);
                        dc.fill_rect(Rect::new(nr.x, nr.y, nr.w, 1), th.border);
                        let dot = (8.0 * s).round() as i32;
                        let tw = dc.text_width(&line).min(nr.w - dot - pad * 3);
                        let tx = nr.right() - pad - tw;
                        let ty = dc.text_center_y(nr.y, nr.h);
                        dc.fill_ellipse(
                            Rect::new(
                                tx - dot - (6.0 * s).round() as i32,
                                nr.y + (nr.h - dot) / 2,
                                dot,
                                dot,
                            ),
                            color,
                        );
                        dc.text(tx, ty, Rect::new(tx, nr.y, tw, nr.h), &line, th.text);
                    }
                }
            }
            if let Some((tg, text)) = &tip_ready {
                let sw = rh;
                let y = match tg.row {
                    None => l.y,
                    Some(row) => body.y + row as i32 * rh - self.scroll_y,
                };
                let anchor = Rect::new(l.x + sw * tg.col, y, sw, rh);
                draw_tooltip(&mut dc, th, anchor, wi, text, s);
            }
            // 우클릭 메뉴 · 입력란 편집 메뉴(최상위 — 다른 컨트롤이 메뉴 위에 그려지지 않게 맨 마지막).
            self.menu.paint(&mut dc, th);
            self.filter.paint_popup(&mut dc, th);
            if detail_visible {
                self.panel.paint_popups(&mut dc, th);
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
    // 호버는 행 배경을 바꾸지 않는다(선택 행의 하늘색 위에 흰 칸이 뜨던 문제 · 사용자 09-14) — 아이콘만 진하게.
    let ring = match mark {
        _ if dim => th.border,
        Some(TestMark::Testing) => th.warn,
        Some(TestMark::Ok) => th.ok,
        Some(TestMark::Failed) => LIGHT_RED,
        None if hover => th.text,
        None => th.text_dim,
    };
    // 고리 = 선(안쪽은 행 배경 그대로 — 선택 행이든 아니든 어울린다).
    dc.stroke_ellipse(Rect::new(x + 1, y + 1, d - 2, d - 2), ring, 2.0);
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

/// 행 접속 버튼 — 오른쪽 삼각형(재생). 클릭 대기 = 회색(호버 시 조금 진하게) · 접속 시도 중 = 파랑 · 접속됨 = 초록(사용자 09-14).
/// 호버는 행 배경을 바꾸지 않는다.
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
    let color = match mark {
        _ if dim => th.border,
        Some(ConnectMark::Connecting) => th.accent,
        Some(ConnectMark::Connected) => th.ok,
        None if hover => th.text,
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
        let none = HashMap::new();
        sort_shown(&ps, &mut shown, &[(0, true)], &none);
        assert_eq!(shown, vec![1, 0, 3, 2], "이름 대소문자 무관(A · b · B · c)");
        sort_shown(&ps, &mut shown, &[(2, true), (0, false)], &none);
        assert_eq!(
            shown,
            vec![2, 0, 3, 1],
            "빈 사용자는 뒤 · 2차 키 이름 내림차순"
        );
        sort_shown(&ps, &mut shown, &[], &none);
        assert_eq!(shown, vec![2, 0, 3, 1], "키 없음 = 그대로");
    }
}
