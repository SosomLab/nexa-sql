//! ★ 보조 창 공통 호스트(T-247 · docs/30 T-65 · docs/93 §8 · 09-29): 로그·메모리·세션·트랜잭션 로그·변수 창이 저마다 들고 있던
//! **창 열기 꼬리**(속성 → 생성 → 기억 자리로 재배치 → 화면 안 → 표면·배율 · IME)와 **닫기 꼬리**(자리·크기 기억)를 한 곳에.
//!
//! 규칙(사용자 09-17): 같은 모니터면 기억한 위치·크기(`wingeom::Memo`) · 아니면 기본 크기로 메인 창 오른쪽(`near` + `dy`) ·
//! 기억 위치는 프레임 기준으로 다시 놓는다(macOS 제목 표시줄 드리프트 · `place_outer`) · 화면 밖으로 안 나감(`keep_on_screen`).

use std::rc::Rc;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// 열기 요청 — 창마다 다른 것만.
pub(crate) struct OpenSpec<'a> {
    pub title: String,
    pub theme: Option<winit::window::Theme>,
    /// 메인 창 (x, y, 폭) — 기억 자리가 없을 때 오른쪽(`x + w + 8`, `y + dy`)에.
    pub near: Option<(i32, i32, u32)>,
    pub dy: i32,
    pub owner: Option<&'a Window>,
    /// 기억 기하(`window.<name>_pos/_size`) — `None` = 기억 안 씀(변수 창).
    pub memo: Option<&'a crate::wingeom::Memo>,
    /// 기억이 없을 때의 논리 크기.
    pub default_size: (f64, f64),
    /// 한글 입력란이 있는 창 = IME 허용(앱 조합 모드면 안 붙임 · T-139).
    pub ime: bool,
}

/// 열린 창(호스트가 필드에 옮겨 담는다).
pub(crate) struct Opened {
    pub window: Rc<Window>,
    pub surface: Option<crate::present::Presenter>,
    pub scale: f32,
}

/// 창 열기 꼬리 — 생성 실패면 `None`(호출자는 조용히 돌아간다 · 종전과 같음).
pub(crate) fn open_window(el: &ActiveEventLoop, spec: OpenSpec<'_>) -> Option<Opened> {
    let same = spec.memo.and_then(|m| m.on_same_monitor(spec.owner));
    let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or(spec.default_size);
    let mut attrs = Window::default_attributes()
        .with_title(spec.title)
        .with_theme(spec.theme)
        .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
    if let Some(((x, y), _)) = same {
        attrs = attrs.with_position(crate::wingeom::logical(x, y));
    } else if let Some((x, y, w)) = spec.near {
        attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
            x + w as i32 + 8,
            y + spec.dy,
        ));
    }
    let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), spec.owner);
    let win = el.create_window(attrs).ok()?;
    if let Some(((x, y), _)) = same {
        crate::wingeom::place_outer(&win, Some((x, y)));
    }
    crate::wingeom::keep_on_screen(&win, spec.owner);
    let win = Rc::new(win);
    let scale = win.scale_factor() as f32;
    let surface = crate::present::Presenter::new(win.clone()).ok();
    if spec.ime {
        win.set_ime_allowed(crate::input::system_ime());
    }
    Some(Opened {
        window: win,
        surface,
        scale,
    })
}

/// 닫기 꼬리 — 지금 (프레임 위치, 논리 크기) · 위치를 못 읽으면 `None`(호스트가 설정에 저장할 1회성 값).
pub(crate) fn last_geom(w: &Window) -> Option<((i32, i32), (f64, f64))> {
    crate::wingeom::outer_pos(w).map(|p| (p, crate::wingeom::logical_size(w)))
}
