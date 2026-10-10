//! **화면 내보내기 한 곳**(T-147 · D-133 ② · docs/62 §2) — 모든 창이 이 타입으로 픽셀을 낸다.
//!
//! - 기본 = `softbuffer`(3-OS 공통 · 종전 그대로).
//! - macOS이고 설정 `gfx.mac_present = iosurface`(선택 · 기본은 softbuffer)면 nexa-sys `LayerPresenter`(IOSurface 풀 · 할당 0 · 복사 0 ·
//!   색 맞춤은 합성기) — 만들기에 실패하면 조용히 `softbuffer`로 돌아간다.
//!
//! 모양은 `softbuffer::Surface`와 같다(`resize` → `buffer_mut` → 그리기 → `present`) — 창 코드가 뒷단을 모른다.
//! 방식은 **창을 만들 때** 정해진다(설정을 바꾸면 새로 여는 창부터 · 메인 창은 재시작 뒤).
//!
//! ★ **유휴 해제**(118차 mac · nexa-ui 193차 `LayerPresenter::trim_idle`): IOSurface 풀(≤3장)은 레티나 메인 창에서 장당 ≈16 MB다.
//! 마지막 프레임 뒤 `gfx.mac_present_trim_ms`(1500 · 0 = 끔) 동안 프레임이 없으면 호스트 틱이 [`Presenter::trim_if_idle`]로 앞 장(+잠긴 장)만
//! 남긴다 — 다음 프레임은 새 장을 만든다(유휴 메모리 ↔ 첫 프레임 `IOSurfaceCreate` 1회의 거래). softbuffer = 할 일 없음(0).

use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use winit::window::Window;

/// 설정 `gfx.mac_present`가 `iosurface`인가(기동·설정 변경 때 호스트가 맞춘다).
static MAC_LAYER: AtomicBool = AtomicBool::new(false);

/// 설정값을 알린다(`"iosurface"` = 켬 · 그 밖 = `softbuffer` = 기본).
pub(crate) fn set_mode(setting: &str) {
    MAC_LAYER.store(setting == "iosurface", Ordering::Relaxed);
}

/// IOSurface 경로를 쓸 것인가 — macOS **이고** 설정이 켜져 있을 때만(순수 판정 · MC/DC 테스트).
pub(crate) fn use_layer(macos: bool, setting_on: bool) -> bool {
    macos && setting_on
}

type Soft = softbuffer::Surface<Rc<Window>, Rc<Window>>;

enum Kind {
    Soft {
        surface: Soft,
        /// 컨텍스트는 표면보다 오래 살아야 한다.
        _ctx: softbuffer::Context<Rc<Window>>,
    },
    Layer {
        p: nexa_sys::layer_present::LayerPresenter,
        win: Rc<Window>,
    },
}

/// 창 하나의 내보내기.
pub(crate) struct Presenter {
    kind: Kind,
    /// 마지막으로 프레임 버퍼를 빌린 시각(유휴 판정의 기준).
    last_frame: Instant,
    /// 마지막 프레임 뒤 유휴 해제를 이미 했는가(한 번만 · 다음 프레임이 되돌린다).
    trimmed: bool,
}

/// 유휴 해제를 **지금** 할 것인가(순수 판정 · MC/DC 테스트): IOSurface 뒷단이고 · 아직 안 했고 · 끄지 않았고(0) · 마지막 프레임 뒤 `idle_ms`가 지났다.
pub(crate) fn trim_due(layer: bool, trimmed: bool, idle_ms: u64, since_frame_ms: u64) -> bool {
    layer && !trimmed && idle_ms > 0 && since_frame_ms >= idle_ms
}

/// 그릴 버퍼(`[u32]` = `0x00RRGGBB` · 폭 × 높이). 다 그린 뒤 [`Buffer::present`].
pub(crate) enum Buffer<'a> {
    Soft(softbuffer::Buffer<'a, Rc<Window>, Rc<Window>>),
    Layer(nexa_sys::layer_present::Frame<'a>),
}

impl Presenter {
    /// 창에 붙인다. 실패 = `Err(사유)`.
    pub(crate) fn new(win: Rc<Window>) -> Result<Presenter, String> {
        if use_layer(cfg!(target_os = "macos"), MAC_LAYER.load(Ordering::Relaxed)) {
            if let Some(p) = layer_for(&win) {
                return Ok(Presenter {
                    kind: Kind::Layer { p, win },
                    last_frame: Instant::now(),
                    trimmed: false,
                });
            }
        }
        let ctx = softbuffer::Context::new(win.clone())
            .map_err(|e| format!("softbuffer context failed: {e}"))?;
        let surface = softbuffer::Surface::new(&ctx, win)
            .map_err(|e| format!("softbuffer surface failed: {e}"))?;
        Ok(Presenter {
            kind: Kind::Soft { surface, _ctx: ctx },
            last_frame: Instant::now(),
            trimmed: false,
        })
    }

    /// ★ 유휴 해제(위 머리말): 마지막 프레임 뒤 `idle` 동안 프레임이 없었고 아직 안 했으면 IOSurface 풀을 앞 장만 남긴다.
    /// 돌려주는 값 = 놓은 장 수(0 = 할 일 없음 · softbuffer · 끔 · 이미 함 · 아직 이르다).
    pub(crate) fn trim_if_idle(&mut self, now: Instant, idle: Duration) -> usize {
        let since = now.saturating_duration_since(self.last_frame).as_millis() as u64;
        if !trim_due(
            matches!(self.kind, Kind::Layer { .. }),
            self.trimmed,
            idle.as_millis() as u64,
            since,
        ) {
            return 0;
        }
        self.trimmed = true;
        match &mut self.kind {
            Kind::Layer { p, .. } => p.trim_idle(),
            Kind::Soft { .. } => 0,
        }
    }

    /// 유휴 해제가 **남아 있으면** 그 시각(호스트가 다음 깨움을 여기에 맞춘다) · 없으면 `None`(softbuffer · 끔 · 이미 함).
    pub(crate) fn idle_deadline(&self, idle: Duration) -> Option<Instant> {
        if idle.is_zero() || self.trimmed || !matches!(self.kind, Kind::Layer { .. }) {
            return None;
        }
        Some(self.last_frame + idle)
    }

    /// 픽셀 크기를 맞춘다.
    pub(crate) fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), ()> {
        match &mut self.kind {
            Kind::Soft { surface, .. } => surface.resize(w, h).map_err(|_| ()),
            Kind::Layer { p, win } => {
                if p.resize(w.get(), h.get(), win.scale_factor()) {
                    Ok(())
                } else {
                    Err(())
                }
            }
        }
    }

    /// 이번 프레임의 버퍼(= 프레임 시각 갱신 · 유휴 해제 상태 초기화).
    pub(crate) fn buffer_mut(&mut self) -> Result<Buffer<'_>, ()> {
        self.last_frame = Instant::now();
        self.trimmed = false;
        match &mut self.kind {
            Kind::Soft { surface, .. } => surface.buffer_mut().map(Buffer::Soft).map_err(|_| ()),
            Kind::Layer { p, .. } => p.frame().map(Buffer::Layer).ok_or(()),
        }
    }

    /// 이번 프레임 버퍼 — 창 크기에 표면을 맞춘 뒤 준다. 크기 0(최소화)·실패 = `None`(docs/93 §4: 보조 창 11곳의
    /// "크기 확인 → resize → buffer_mut" 상용구를 한 곳으로).
    pub(crate) fn frame(&mut self, size: winit::dpi::PhysicalSize<u32>) -> Option<Buffer<'_>> {
        let (w, h) = (NonZeroU32::new(size.width)?, NonZeroU32::new(size.height)?);
        self.resize(w, h).ok()?;
        self.buffer_mut().ok()
    }

    /// 지금 쥔 표면(프레임 버퍼) 장수 — IOSurface = 풀 길이(0~3 · 유휴 해제 뒤 1) · softbuffer = 1. 메모리 창 "표면" = 창 픽셀 × 4 × 이 값.
    pub(crate) fn surface_count(&self) -> u64 {
        match &self.kind {
            Kind::Soft { .. } => 1,
            Kind::Layer { p, .. } => p.pool_len() as u64,
        }
    }

    /// 진단용 이름(`NSQL_TRACE_FRAMES`).
    pub(crate) fn backend(&self) -> &'static str {
        match self.kind {
            Kind::Soft { .. } => "softbuffer",
            Kind::Layer { .. } => "iosurface",
        }
    }
}

impl Buffer<'_> {
    /// 화면에 낸다.
    pub(crate) fn present(self) -> Result<(), ()> {
        match self {
            Buffer::Soft(b) => b.present().map_err(|_| ()),
            Buffer::Layer(f) => {
                f.present();
                Ok(())
            }
        }
    }
}

impl Deref for Buffer<'_> {
    type Target = [u32];
    fn deref(&self) -> &[u32] {
        match self {
            Buffer::Soft(b) => b,
            Buffer::Layer(f) => f.pixels(),
        }
    }
}

impl DerefMut for Buffer<'_> {
    fn deref_mut(&mut self) -> &mut [u32] {
        match self {
            Buffer::Soft(b) => b,
            Buffer::Layer(f) => f.pixels_mut(),
        }
    }
}

/// 창의 `NSView`에 IOSurface 레이어를 단다(macOS만 · 그 밖 = `None`).
fn layer_for(win: &Rc<Window>) -> Option<nexa_sys::layer_present::LayerPresenter> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = win.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => {
            // SAFETY: winit이 준 살아 있는 NSView · `Presenter`가 창(`Rc<Window>`)을 함께 쥐어 뷰가 먼저 죽지 않는다 ·
            //         창 코드는 전부 메인 스레드(이벤트 루프)에서 돈다.
            unsafe { nexa_sys::layer_present::LayerPresenter::new(h.ns_view.as_ptr()) }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// MC/DC — 조건 둘이 각각 혼자 결과를 바꾼다.
    #[test]
    fn use_layer_mcdc() {
        use super::use_layer as f;
        assert!(f(true, true));
        assert!(!f(false, true), "macOS가 아니면 끔");
        assert!(!f(true, false), "설정이 softbuffer면 끔");
    }

    /// MC/DC — 네 조건이 각각 혼자 결과를 바꾼다(유휴 해제 판정).
    #[test]
    fn trim_due_mcdc() {
        use super::trim_due as f;
        assert!(f(true, false, 1500, 1500), "기준");
        assert!(!f(false, false, 1500, 1500), "softbuffer = 할 일 없음");
        assert!(!f(true, true, 1500, 1500), "이미 했다");
        assert!(!f(true, false, 0, 1500), "0 = 끔");
        assert!(!f(true, false, 1500, 1499), "아직 이르다");
        assert!(f(true, false, 1500, 99_999), "오래 지났어도 한 번");
    }
}
