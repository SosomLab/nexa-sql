//! App — SQL 포맷(내장 **Basic formatter** + 확장 포맷터 · docs/95 · 사용자 09-28).
//!
//! - 명령 `edit.format`(Shift+Alt+F) = **기본 포맷터**(`format.default` · 없으면 Basic)로 선택 또는 문서 전체.
//! - `edit.format_with` = 팔레트에서 포맷터를 골라 한 번 쓰기 / 기본으로 지정.
//! - `edit.format_preview` = **미리보기 탭**(원본 불변 · 설정 창에서 `format.*`/`sqlfmt.*`가 바뀌면 다시 그림 · 문서가 비면 예시 SQL).
//! - 공통 옵션(`format.*`)은 Basic과 확장이 같이 읽는다(확장에는 `nx_ext_format` 입력 JSON `options`로).

use crate::*;

/// 내장 포맷터 id.
pub(crate) const BASIC_ENGINE: &str = "basic";

/// 우클릭 메뉴 순서(순수): 기본 포맷터가 첫 줄, 나머지는 등록 순(사용자 09-29 — 확장 없음 = Basic 1줄 · Basic 기본 = Basic, kiros33 ·
/// kiros33 기본 = kiros33, Basic).
pub(crate) fn ordered_engines(default: &str, ids: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(ids.len());
    if ids.iter().any(|i| i == default) {
        out.push(default.to_string());
    }
    out.extend(ids.iter().filter(|i| *i != default).cloned());
    out
}

/// 짧은 이름(순수): Basic = "Basic" · 확장 = 라벨에서 "SQL Formatter for " 류 접두를 뗀 것("SQL Formatter for kiros33" → "kiros33").
pub(crate) fn short_engine_name(engine: &str, label: &str) -> String {
    if engine == BASIC_ENGINE {
        return "Basic".to_string();
    }
    let lower = label.to_ascii_lowercase();
    let hit = ["sql formatter for ", "sql formatter ", "formatter for "]
        .into_iter()
        .find(|pre| lower.starts_with(pre));
    match hit {
        Some(pre) => {
            let rest = label[pre.len()..].trim();
            if rest.is_empty() {
                label.to_string()
            } else {
                rest.to_string()
            }
        }
        None => label.to_string(),
    }
}

impl App {
    /// 공통 옵션 쌍(`format.<키>` · 값 원문 · 레지스트리 기본값 포함). ★ 들여쓰기 단위·폭은 설정 항목을 그대로 두되 **지금은 활성
    /// 탭의 들여쓰기**(상태줄 팝업으로 바꾼 탭별 값 · 없으면 `editor.tab_size`/`editor.indent_spaces`)를 쓴다(사용자 09-29).
    fn format_option_pairs(&self) -> Vec<(String, String)> {
        // `format.indent_from_tab`이면 탭 값 — 문서 포맷도 미리보기도 같다(사용자 09-29 "켜면 설명대로 탭 4가 보여야").
        let from_tab = self.settings.flag("format.indent_from_tab");
        let (tab_size, spaces) = self.editors.indent();
        nsql_format::OPTION_KEYS
            .iter()
            .map(|k| {
                let key = format!("format.{k}");
                let v = match *k {
                    "indent" if from_tab => if spaces { "space" } else { "tab" }.to_string(),
                    "indent_width" if from_tab => tab_size.max(1).to_string(),
                    _ => self.settings.get(&key).unwrap_or("").to_string(),
                };
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

    /// 짧은 이름(우클릭 메뉴 `SQL Format (이름)` · 사용자 09-29): Basic = "Basic" · 확장 = 라벨에서 "SQL Formatter for " 접두를 뗀 것.
    pub(crate) fn format_engine_short(&self, engine: &str) -> String {
        short_engine_name(engine, &self.format_engine_name(engine))
    }

    /// 설정 창 `format.default` 콤보 후보(id, 라벨) — 설치·켜진 포맷터 전부(사용자 09-29 "콤보로 선택").
    pub(crate) fn format_default_choices(&self) -> Vec<(String, String)> {
        self.format_engines()
            .into_iter()
            .map(|(id, name, _)| {
                let label = if id == BASIC_ENGINE {
                    name
                } else {
                    format!("{name} ({id})")
                };
                (id, label)
            })
            .collect()
    }

    /// 우클릭 **Format 그룹**(SQL 구문 탭만 · 사용자 09-29): `SQL Format (기본)` → 나머지 포맷터 → 구분자 → 대문자/소문자(선택 있을 때만).
    pub(crate) fn format_menu_items(&self) -> Option<nexa_ctl::controls::ctxmenu::CtxItem> {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        if self.editors.syntax_name() != "SQL" || self.editors.is_view_tab_active() {
            return None;
        }
        let default = self.format_default_engine();
        let all: Vec<String> = self
            .format_engines()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let ids = ordered_engines(&default, &all);
        let mut children: Vec<CtxItem> = ids
            .iter()
            .map(|id| {
                let short = self.format_engine_short(id);
                let it = CtxItem::item(
                    format!("format.use:{id}"),
                    tf(Msg::MnFormatWithName, &[&short]),
                );
                if *id == default {
                    it.with_shortcut(self.keymap.display_of("edit.format"))
                } else {
                    it
                }
            })
            .collect();
        let has_sel = self.editors.cur().selection().is_some_and(|(a, b)| b > a);
        children.push(CtxItem::Separator);
        children.push(CtxItem::maybe(
            "edit.upper_case",
            t(Msg::MnUpperCase),
            has_sel,
        ));
        children.push(CtxItem::maybe(
            "edit.lower_case",
            t(Msg::MnLowerCase),
            has_sel,
        ));
        Some(CtxItem::submenu(
            "format.group",
            t(Msg::MnFormatGroup),
            children,
        ))
    }

    /// 우클릭 편집 메뉴에 덧붙일 항목 = 확장 메뉴 + Format 그룹 — 우클릭 직전과 확장 변경 때 다시 만든다(선택 유무·기본 포맷터가 바뀐다).
    pub(crate) fn refresh_menu_extras(&mut self) {
        let disabled = self.ext_disabled();
        let km = &self.keymap;
        let mut extras = self
            .extensions
            .menu_extras(&disabled, &|id| km.display_of(id));
        if let Some(group) = self.format_menu_items() {
            extras.push(group);
        }
        self.editors.set_menu_extras(extras);
    }

    /// 포맷 대상: 선택 > **캐럿의 문장(문장 실행 범위 · `statement_at_in`)** > 문서 전체 — (from, to, 글, 전체 여부) 문자 인덱스.
    fn format_target(&self) -> (usize, usize, String, bool) {
        let tb = self.editors.cur();
        let all = tb.text();
        if let Some((a, b)) = tb.selection() {
            if b > a {
                let sel: String = all.chars().skip(a).take(b - a).collect();
                return (a, b, sel, false);
            }
        }
        let byte_pos = all
            .char_indices()
            .nth(tb.caret())
            .map_or(all.len(), |(b, _)| b);
        if let Some(it) = nsql_script::statement_at_in(&all, byte_pos, Some(self.sess.dialect)) {
            let span = it.span;
            let from = all[..span.start].chars().count();
            let to = from + all[span.clone()].chars().count();
            let text = all[span].to_string();
            if !text.trim().is_empty() {
                return (from, to, text, false);
            }
        }
        let n = all.chars().count();
        (0, n, all, true)
    }

    /// 포맷 실행(엔진별) — Basic은 in-process · 확장은 WASM `nx_ext_format`(공통 옵션 + 접두 설정 전달).
    pub(crate) fn format_run(
        &mut self,
        engine: &str,
        text: &str,
        preview: bool,
    ) -> Result<String, String> {
        let mut pairs = self.format_option_pairs();
        if engine == BASIC_ENGINE {
            let mut opts = nsql_format::Options::from_pairs(
                pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            // ★ 테이블 설명 주석(스킬 2-2 · T-255): 현재 연결의 메타에 코멘트가 있는 테이블만(없으면 주석 없음).
            opts.table_comments = self.format_table_comments(text);
            return Ok(nsql_format::format_basic(text, &opts));
        }
        // 확장에도 같은 테이블 설명을(공통 옵션 pair `table_comments` · 확장은 `Options::from_pairs`로 그대로 받는다).
        let comments = self.format_table_comments(text);
        if !comments.is_empty() {
            pairs.push((
                "table_comments".to_string(),
                nsql_format::table_comments_pair(&comments),
            ));
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

    /// 문서에 나온 이름 중 현재 연결 메타에서 코멘트가 있는 테이블 → (대문자 `SCHEMA.TABLE`/`TABLE`, 코멘트). 접속이 없으면 빈 목록.
    fn format_table_comments(&self, text: &str) -> Vec<(String, String)> {
        use nsql_format::Kind;
        use nsql_run::meta::DetailState;
        let spec = self.sess.spec.clone();
        let (names, snap) = self.explorer.meta_view(spec.as_ref());
        let toks = nsql_format::lex(text);
        let mut out: Vec<(String, String)> = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            if t.kind != Kind::Word || nsql_format::is_keyword(&t.text) {
                i += 1;
                continue;
            }
            let (schema, name, step) = if toks.get(i + 1).is_some_and(|d| d.is_punct("."))
                && toks.get(i + 2).is_some_and(|n| n.kind == Kind::Word)
            {
                (Some(t.text.as_str()), toks[i + 2].text.as_str(), 3)
            } else {
                (None, t.text.as_str(), 1)
            };
            let key = match schema {
                Some(s) => format!("{}.{}", s.to_ascii_uppercase(), name.to_ascii_uppercase()),
                None => name.to_ascii_uppercase(),
            };
            if !out.iter().any(|(k, _)| *k == key) {
                if let Some(id) = snap.lookup(names, schema, name) {
                    let comment = snap
                        .object(id)
                        .and_then(|o| o.comment)
                        .map(|s| names.get(s).to_string())
                        .or_else(|| match snap.detail(id) {
                            DetailState::Loaded { detail, .. } => {
                                detail.comment.map(|s| names.get(s).to_string())
                            }
                            _ => None,
                        });
                    if let Some(c) = comment.filter(|c| !c.trim().is_empty()) {
                        out.push((key, c));
                    }
                }
            }
            i += step;
        }
        out
    }

    /// Shift+Alt+F — 기본 포맷터로 선택 또는 문서 전체.
    pub(crate) fn format_sql_cmd(&mut self) {
        let engine = self.format_default_engine();
        self.format_apply(&engine);
    }

    /// 엔진 하나로 편집기 본문을 바꾼다 — 선택이 있으면 선택만 · 없으면 **캐럿의 문장**(문장 실행 범위 · 사용자 09-29) · 문장이 없으면
    /// 전체. `replace_range` = 되돌리기 1단계.
    fn format_apply(&mut self, engine: &str) {
        let (from, to, text, whole) = self.format_target();
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
                    } else {
                        // 문장/선택 포맷 뒤 = 바뀐 구간을 선택해 둔다(어디가 바뀌었는지 보이고 Undo 대상이 분명).
                        let end = from + out.chars().count();
                        tb.select_range(from, end, &mut inv);
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

    /// ★ 설정 창 안 미리보기(사용자 09-29 "포맷을 미리보면서 수정"): 기본 포맷터로 원본(열린 미리보기 탭의 원본 > 예시 SQL)을 포맷해
    /// 설정 창에 넣는다 — `format.*`/`sqlfmt.*` 변경 · 설정 창 열 때.
    pub(crate) fn prefs_format_preview_refresh(&mut self) {
        if self.prefs_win.window().is_none() {
            return;
        }
        let engine = self.format_default_engine();
        let src = self
            .format_preview
            .as_ref()
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| {
                self.format_engines()
                    .into_iter()
                    .find(|(id, _, _)| *id == engine)
                    .map(|(_, _, s)| s)
                    .unwrap_or_else(|| nsql_format::SAMPLE_SQL.to_string())
            });
        let out = match self.format_run(&engine, &src, true) {
            Ok(o) => o,
            Err(e) => format!("-- {}\n-- {e}\n", t(Msg::StFormatPreviewFailed)),
        };
        let title = tf(
            Msg::TitleFormatPreview,
            &[&self.format_engine_name(&engine)],
        );
        // ★ 미리보기 상자의 탭 폭/단위도 실제 적용값(켜짐 = 활성 탭 · 꺼짐 = 설정값).
        let (ts, sp) = self.editors.indent();
        let (tab_size, spaces) = if self.settings.flag("format.indent_from_tab") {
            (ts.clamp(1, 16), sp)
        } else {
            (
                self.settings.int("format.indent_width").clamp(1, 16) as u8,
                self.settings
                    .get("format.indent")
                    .is_some_and(|v| v.starts_with('s')),
            )
        };
        self.prefs_win.set_preview(title, &out, tab_size, spaces);
        // ★ 카드 덧말(사용자 09-29): 지금 활성 탭의 실제 들여쓰기 = 포맷에 적용될 값(켜져 있을 때) — 강조색.
        let title = self.editors.title_of(self.editors.active());
        let unit = t(if sp { Msg::StSpaces } else { Msg::StTabSize });
        let unit = unit.split(':').next().unwrap_or("").trim().to_string();
        let applied = self.settings.flag("format.indent_from_tab");
        let note = tf(
            if applied {
                Msg::NoteIndentFromTabOn
            } else {
                Msg::NoteIndentFromTabOff
            },
            &[&title, &unit, &ts.to_string()],
        );
        self.prefs_win
            .set_note("format.indent_from_tab", Some(note));
    }

    /// 설정이 바뀌었다(`format.*` · `sqlfmt.*`) → 열려 있는 미리보기 탭만 다시 그린다(닫혀 있으면 아무것도 안 함).
    pub(crate) fn format_preview_refresh(&mut self) {
        self.prefs_format_preview_refresh();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 우클릭 Format 그룹 순서(사용자 09-29): 확장 없음 = Basic 1줄 · Basic 기본 = Basic, kiros33 · kiros33 기본 = kiros33, Basic.
    #[test]
    fn menu_order_puts_default_first() {
        let basic = vec!["basic".to_string()];
        assert_eq!(ordered_engines("basic", &basic), vec!["basic"]);
        let both = vec!["basic".to_string(), "sql-formatter-kiros33".to_string()];
        assert_eq!(
            ordered_engines("basic", &both),
            vec!["basic", "sql-formatter-kiros33"]
        );
        assert_eq!(
            ordered_engines("sql-formatter-kiros33", &both),
            vec!["sql-formatter-kiros33", "basic"]
        );
        // 기본으로 지정된 확장이 지금 없으면(끔·제거) 목록에 끼워 넣지 않는다.
        assert_eq!(ordered_engines("gone", &basic), vec!["basic"]);
    }

    #[test]
    fn short_names_strip_formatter_prefix() {
        assert_eq!(short_engine_name("basic", "Basic 포맷터"), "Basic");
        assert_eq!(
            short_engine_name("sql-formatter-kiros33", "SQL Formatter for kiros33"),
            "kiros33"
        );
        assert_eq!(short_engine_name("x", "My Formatter"), "My Formatter");
        assert_eq!(
            short_engine_name("x", "SQL Formatter for "),
            "SQL Formatter for "
        );
    }
}
