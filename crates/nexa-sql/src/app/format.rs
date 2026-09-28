//! App — SQL 포맷(내장 **Basic formatter** + 확장 포맷터 · docs/95 · 사용자 09-28).
//!
//! - 명령 `edit.format`(Shift+Alt+F) = **기본 포맷터**(`format.default` · 없으면 Basic)로 선택 또는 문서 전체.
//! - `edit.format_with` = 팔레트에서 포맷터를 골라 한 번 쓰기 / 기본으로 지정.
//! - `edit.format_preview` = **미리보기 탭**(원본 불변 · 설정 창에서 `format.*`/`sqlfmt.*`가 바뀌면 다시 그림 · 문서가 비면 예시 SQL).
//! - 공통 옵션(`format.*`)은 Basic과 확장이 같이 읽는다(확장에는 `nx_ext_format` 입력 JSON `options`로).

use crate::*;

/// 내장 포맷터 id.
pub(crate) const BASIC_ENGINE: &str = "basic";

impl App {
    /// 공통 옵션 쌍(`format.<키>` · 값 원문 · 레지스트리 기본값 포함).
    fn format_option_pairs(&self) -> Vec<(String, String)> {
        nsql_format::OPTION_KEYS
            .iter()
            .map(|k| {
                let key = format!("format.{k}");
                let v = self.settings.get(&key).unwrap_or("").to_string();
                (key, v)
            })
            .collect()
    }

    /// 쓸 수 있는 포맷터 (id, 이름, 예시 SQL) — Basic + 켜진 확장 포맷터.
    pub(crate) fn format_engines(&self) -> Vec<(String, String, String)> {
        let mut v = vec![(
            BASIC_ENGINE.to_string(),
            t(Msg::FmtBasicName).to_string(),
            nsql_format::SAMPLE_SQL.to_string(),
        )];
        let disabled = self.ext_disabled();
        for (id, label, sample) in self.extensions.formatters(&disabled) {
            let sample = if sample.trim().is_empty() {
                nsql_format::SAMPLE_SQL.to_string()
            } else {
                sample
            };
            v.push((id, label, sample));
        }
        v
    }

    /// 기본 포맷터 id(설정 `format.default` · 지금 쓸 수 없으면 Basic).
    pub(crate) fn format_default_engine(&self) -> String {
        let want = self
            .settings
            .get("format.default")
            .unwrap_or(BASIC_ENGINE)
            .trim()
            .to_string();
        if self.format_engines().iter().any(|(id, _, _)| *id == want) {
            want
        } else {
            BASIC_ENGINE.to_string()
        }
    }

    fn format_engine_name(&self, engine: &str) -> String {
        self.format_engines()
            .into_iter()
            .find(|(id, _, _)| id == engine)
            .map(|(_, name, _)| name)
            .unwrap_or_else(|| engine.to_string())
    }

    /// 포맷 실행(엔진별) — Basic은 in-process · 확장은 WASM `nx_ext_format`(공통 옵션 + 접두 설정 전달).
    fn format_run(&mut self, engine: &str, text: &str, preview: bool) -> Result<String, String> {
        let pairs = self.format_option_pairs();
        if engine == BASIC_ENGINE {
            let opts = nsql_format::Options::from_pairs(
                pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            return Ok(nsql_format::format_basic(text, &opts));
        }
        let dialect = format!("{:?}", self.sess.dialect).to_ascii_lowercase();
        let r =
            self.extensions
                .format_with(engine, text, &dialect, &pairs, &self.settings, preview);
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        r
    }

    /// Shift+Alt+F — 기본 포맷터로 선택 또는 문서 전체.
    pub(crate) fn format_sql_cmd(&mut self) {
        let engine = self.format_default_engine();
        self.format_apply(&engine);
    }

    /// 엔진 하나로 지금 편집기 본문(선택이 있으면 선택만)을 바꾼다 — 되돌리기 1단계.
    fn format_apply(&mut self, engine: &str) {
        let (from, to, text, whole) = {
            let tb = self.editors.cur();
            let all = tb.text();
            match tb.selection() {
                Some((a, b)) if b > a => {
                    let sel: String = all.chars().skip(a).take(b - a).collect();
                    (a, b, sel, false)
                }
                _ => {
                    let n = all.chars().count();
                    (0, n, all, true)
                }
            }
        };
        if text.trim().is_empty() {
            self.sess.status = t(Msg::StFormatNoChange).into();
            self.redraw();
            return;
        }
        let name = self.format_engine_name(engine);
        match self.format_run(engine, &text, false) {
            Ok(out) => {
                // 선택 포맷은 끝 개행을 붙이지 않는다(선택 뒤 글이 이어질 수 있다).
                let out = if whole {
                    out
                } else {
                    out.trim_end_matches(['\n', '\r']).to_string()
                };
                if out == text {
                    self.sess.status = t(Msg::StFormatNoChange).into();
                } else {
                    let mut inv = Invalidations::default();
                    let tb = self.editors.cur_mut();
                    tb.replace_range(from, to, &out, &mut inv);
                    if whole {
                        tb.select_range(0, 0, &mut inv);
                    }
                    self.sess.status = tf(Msg::StFormatted, &[&name]);
                }
            }
            Err(e) => {
                self.sess.status = tf(Msg::StFormatFailed, &[&name, &e]);
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            }
        }
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// 팔레트: 포맷터 고르기(한 번 쓰기 · 기본으로 지정 · 미리보기).
    pub(crate) fn format_with_cmd(&mut self) {
        let default = self.format_default_engine();
        let mut cmds: Vec<(String, String)> = Vec::new();
        for (id, name, _) in self.format_engines() {
            let mark = if id == default { "✓ " } else { "" };
            cmds.push((
                format!("format.use:{id}"),
                format!("{mark}{}", tf(Msg::PalFormatUse, &[&name])),
            ));
        }
        for (id, name, _) in self.format_engines() {
            if id != default {
                cmds.push((
                    format!("format.default:{id}"),
                    tf(Msg::PalFormatSetDefault, &[&name]),
                ));
            }
        }
        for (id, name, _) in self.format_engines() {
            cmds.push((
                format!("format.preview:{id}"),
                tf(Msg::PalFormatPreview, &[&name]),
            ));
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 팔레트에서 고른 항목(`format.<동사>:<엔진>`).
    pub(crate) fn format_pick(&mut self, id: &str) {
        let Some((verb, engine)) = id.strip_prefix("format.").and_then(|s| s.split_once(':'))
        else {
            return;
        };
        let engine = engine.to_string();
        match verb {
            "use" => self.format_apply(&engine),
            "default" => {
                let _ = self.settings.set("format.default", &engine);
                self.persist_settings();
                let name = self.format_engine_name(&engine);
                self.sess.status = tf(Msg::StFormatDefaultSet, &[&name]);
                self.prefs_sync();
                self.format_preview_refresh();
                self.redraw();
            }
            "preview" => self.format_preview_open(&engine),
            _ => {}
        }
    }

    /// 메뉴/팔레트 "Format preview" = 기본 포맷터 미리보기.
    pub(crate) fn format_preview_cmd(&mut self) {
        let engine = self.format_default_engine();
        self.format_preview_open(&engine);
    }

    /// 미리보기 탭 열기(원본 = 선택 > 문서 > 예시) — 원본은 바뀌지 않는다.
    fn format_preview_open(&mut self, engine: &str) {
        let source = {
            let tb = self.editors.cur();
            let all = tb.text();
            match tb.selection() {
                Some((a, b)) if b > a => all.chars().skip(a).take(b - a).collect::<String>(),
                _ => all,
            }
        };
        let source = if source.trim().is_empty() || self.editors.is_view_tab_active() {
            self.format_engines()
                .into_iter()
                .find(|(id, _, _)| id == engine)
                .map(|(_, _, s)| s)
                .unwrap_or_else(|| nsql_format::SAMPLE_SQL.to_string())
        } else {
            source
        };
        self.format_preview = Some((engine.to_string(), source));
        self.format_preview_render(true);
    }

    /// 설정이 바뀌었다(`format.*` · `sqlfmt.*`) → 열려 있는 미리보기 탭만 다시 그린다(닫혀 있으면 아무것도 안 함).
    pub(crate) fn format_preview_refresh(&mut self) {
        if self.format_preview.is_none() {
            return;
        }
        let title = self.format_preview_title();
        if !self.editors.has_info_tab(&title) {
            // 사용자가 닫았다 — 다시 열지 않는다.
            self.format_preview = None;
            return;
        }
        self.format_preview_render(false);
    }

    fn format_preview_title(&self) -> String {
        let engine = self
            .format_preview
            .as_ref()
            .map(|(e, _)| e.clone())
            .unwrap_or_default();
        tf(
            Msg::TitleFormatPreview,
            &[&self.format_engine_name(&engine)],
        )
    }

    fn format_preview_render(&mut self, focus: bool) {
        let Some((engine, src)) = self.format_preview.clone() else {
            return;
        };
        let out = match self.format_run(&engine, &src, true) {
            Ok(o) => o,
            Err(e) => format!("-- {}\n-- {e}\n", t(Msg::StFormatPreviewFailed)),
        };
        let title = self.format_preview_title();
        let created = self
            .editors
            .open_info_tab_like(&title, &out, Some("preview.sql"));
        if created {
            let tab = self.editors.active_id();
            self.make_unconnected(tab);
        }
        if focus {
            self.set_focus(Focus::Editor);
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.layout();
        self.redraw();
    }
}
