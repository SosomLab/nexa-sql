//! App — 글꼴 크기 조절(사용자 10-07 "Ctrl + +/−로 편집기 탭·결과 탭 각각 · Ctrl+휠로도").
//!
//! 대상은 둘뿐 — 편집기(`editor.font_size`) · 결과 그리드(`grid.font_size`). 키(`view.zoom_in/out/reset`)는 **포커스**로,
//! Ctrl+휠은 **커서 아래 영역**으로 대상을 고른다. 값은 설정 그대로(저장 → 반영 → 설정 창 갱신 = 설정 창에서 바꾼 것과 같은 길 ·
//! 범위 = 레지스트리 `SettingKind::Size { min, max }`).

use crate::*;

impl App {
    /// 조절 대상 설정 키 — `by_cursor`면 커서 아래 영역(편집기 본문 · 그리드 영역 = 조건 바 포함), 아니면 포커스.
    fn zoom_target(&self, by_cursor: bool) -> Option<&'static str> {
        if by_cursor {
            let p = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.editors.editor_bounds().contains(p) {
                Some("editor.font_size")
            } else if self.grid.outer_bounds().contains(p) {
                Some("grid.font_size")
            } else {
                None
            }
        } else {
            match self.focus {
                Focus::Editor => Some("editor.font_size"),
                Focus::Grid => Some("grid.font_size"),
                _ => None,
            }
        }
    }

    /// 한 단계(1 px) 크게/작게 — 대상이 없으면 false(호출자가 휠을 평소대로 흘린다). `dir` = 0은 기본값으로.
    pub(crate) fn zoom_step(&mut self, dir: i32, by_cursor: bool) -> bool {
        let Some(key) = self.zoom_target(by_cursor) else {
            return false;
        };
        let Some(entry) = nsql_settings::entry(key) else {
            return false;
        };
        let (min, max) = match entry.kind {
            nsql_settings::SettingKind::Size { min, max } => (min, max),
            _ => (8, 40),
        };
        let cur = self.settings.font_px(key).round() as i32;
        let next = if dir == 0 {
            nsql_settings::size_px(entry.default)
                .unwrap_or(cur as f32)
                .round() as i32
        } else {
            (cur + dir.signum()).clamp(min as i32, max as i32)
        };
        let area = if key == "editor.font_size" {
            t(Msg::CatEditor)
        } else {
            t(Msg::CatGrid)
        };
        if next != cur && self.settings.set(key, &next.to_string()).is_ok() {
            self.persist_settings();
            self.apply_setting(key);
            self.prefs_win.refresh(&self.settings);
        }
        self.sess.status = tf(Msg::StFontSize, &[area, &next.to_string()]);
        // ★ HUD(사용자 10-07 "고속 스크롤처럼"): 바뀐 영역 위에 `얼굴 · N px` 캡슐 — 유지 뒤 서서히 사라진다(`zoom.hud*`).
        if self.settings.flag("zoom.hud") {
            let face = self.zoom_face(key);
            // 그리드 = 본문 영역(조건 바 제외 · 협업 V1 bin32 "HUD가 조건 바 버튼을 덮음").
            self.zoom_hud_area = if key == "editor.font_size" {
                self.editors.editor_bounds()
            } else {
                self.grid.body_bounds()
            };
            self.zoom_hud.show(format!("{face} · {next} px"));
        }
        self.redraw();
        true
    }

    /// 글꼴 얼굴 이름 — 설정에 적힌 이름이 있으면 그것 · 비면 적재된 사슬의 첫 이름(편집기 = 고정폭 · 그리드 = 그리드 얼굴 또는 UI).
    fn zoom_face(&self, key: &str) -> String {
        let (face_key, fallback) = if key == "editor.font_size" {
            ("editor.font_face", &self.mono_face)
        } else {
            ("grid.font_face", &self.ui_face)
        };
        match self.settings.get(face_key).map(str::trim) {
            Some(f) if !f.is_empty() => f.to_string(),
            _ if key != "editor.font_size" && self.grid_font.is_some() => self.mono_face.clone(),
            _ => fallback.clone(),
        }
    }

    /// HUD 모양 = 설정 `zoom.hud_*`(고속 스크롤 HUD 항목과 같은 구성 · 색이 비면 테마).
    pub(crate) fn zoom_hud_style(&self) -> nexa_ctl::HudStyle {
        let (bg, _) = color_alpha_setting(&self.settings, "zoom.hud_bg");
        let (fg, _) = color_alpha_setting(&self.settings, "zoom.hud_fg");
        nexa_ctl::HudStyle {
            pos: nexa_ctl::HudPos::parse(self.settings.get("zoom.hud_pos").unwrap_or("top_right")),
            hold_ms: self.settings.int("zoom.hud_hold_ms").clamp(0, 5000) as u64,
            fade_ms: self.settings.int("zoom.hud_fade_ms").clamp(50, 5000) as u64,
            bg: bg.unwrap_or(self.theme.accent),
            bg_alpha: self.settings.int("zoom.hud_bg_alpha").clamp(0, 100) as f32 / 100.0,
            fg: fg.unwrap_or(self.theme.text),
            fg_alpha: self.settings.int("zoom.hud_fg_alpha").clamp(0, 100) as f32 / 100.0,
        }
    }
}
