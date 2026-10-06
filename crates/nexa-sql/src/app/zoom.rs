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
        self.redraw();
        true
    }
}
