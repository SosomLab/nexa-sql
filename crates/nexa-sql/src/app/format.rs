//! App — SQL 포맷(내장 **Basic formatter** + 확장 포맷터 · docs/95 · 사용자 09-28).
//!
//! - 명령 `edit.format`(Shift+Alt+F) = **기본 포맷터**(`format.default` · 없으면 Basic)로 선택 또는 문서 전체.
//! - `edit.format_with` = 팔레트에서 포맷터를 골라 한 번 쓰기 / 기본으로 지정.
//! - `edit.format_preview` = **미리보기 탭**(원본 불변 · 설정 창에서 `format.*`/`ext.sqlfmt_kiros33.*`가 바뀌면 다시 그림 · 문서가 비면 예시 SQL).
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

/// ★ 설정 → 미리보기에서 그 설정이 **처음 드러나는 줄**을 찾는 조각(사용자 09-30 "몇 번째 줄에서 변화를 볼 수 있는지 · 하나면 충분").
///   비교는 줄에서 공백·탭을 지우고 대문자로 바꾼 뒤 포함 여부 — 콤마 위치·간격·대소문자 옵션이 바뀌어도 같은 줄을 찾는다.
///   **Basic(`format.*`)만** 여기 — 확장 설정의 조각은 그 확장 메타 `formatter.marks`가 낸다(사용자 09-30 "확장 것은 확장에" ·
///   `Registry::formatter_marks`).
const PREVIEW_MARKS: &[(&str, &str)] = &[
    ("format.indent", "BASEAS("),
    ("format.indent_width", "BASEAS("),
    ("format.keyword_case", "WITH"),
    ("format.identifier_case", "A.PLANT_CD"),
    ("format.function_case", "SUM(A.QTY)"),
    ("format.comma", "A.ITEM_CD"),
    ("format.comma_gap", "A.ITEM_CD"),
    ("format.as_gap", "ASQTY_GRP"),
    ("format.window_break", "OVER(PARTITION"),
    ("format.comment_space", "--BASECTE"),
    ("format.comment_gap", "--BASECTE"),
    ("format.column_as", "LAST_YYMM"),
    ("format.column_alias_all", "LAST_YYMM"),
    ("format.table_as", "TB_DEMANDA"),
    ("format.auto_alias", "TB_ITEM_TYPE"),
    ("format.where_seed", "WHERE"),
    ("format.seed_gap", "WHERE"),
    ("format.paren_seed", "P.REGION_CD"),
    ("format.logical_newline", "A.YYMMBETWEEN"),
    ("format.logical_gap", "A.YYMMBETWEEN"),
    ("format.cond_indent", "A.YYMMBETWEEN"),
    ("format.operator_spaces", "B.QTY>0"),
    ("format.operator_gap", "B.QTY>0"),
    ("format.operator_long_space", "I.ITEM_NMLIKE"),
    ("format.list_style", "GROUPBY"),
    ("format.line_width", "GROUPBY"),
    ("format.case_inline_max", "CASEWHEN"),
    ("format.join_indent", "INNERJOIN"),
    ("format.dialect_target", "SYSDATE"),
    ("format.keep_oneliners", "UPDATETB_PLAN_RESULT"),
    ("format.stmt_blank_lines", "UPDATETB_PLAN_RESULT"),
    ("format.max_blank_lines", "UPDATETB_PLAN_RESULT"),
    ("format.semicolon_newline", "UPDATETB_PLAN_RESULT"),
];

/// ★ 확장 포맷터의 미리보기 토글 이니셜(사용자 09-30 "설치될 때 겹치지 않는 알파벳 한 글자 · K는 임의"): 이름의 마지막 낱말 첫 글자부터
///   거슬러 후보 → A~Z · `B`(Basic)와 이미 쓰인 글자는 건너뛴다(순수 함수 · `SQL Formatter for kiros33` → `K`).
pub(crate) fn formatter_initial(name: &str, taken: &[char]) -> char {
    let used = |c: char| c == 'B' || taken.contains(&c);
    let words: Vec<char> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.chars().next())
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    words
        .iter()
        .rev()
        .copied()
        .find(|&c| !used(c))
        .or_else(|| ('A'..='Z').find(|&c| !used(c)))
        .unwrap_or('X')
}

/// 포맷 결과에서 조각이 처음 나오는 줄 번호(1 기준 · 공백 제거 + 대문자 비교) — 없으면 None(순수 함수).
pub(crate) fn preview_line_of(out: &str, needle: &str) -> Option<usize> {
    out.lines()
        .position(|l| {
            let norm: String = l
                .chars()
                .filter(|c| !c.is_whitespace())
                .flat_map(char::to_uppercase)
                .collect();
            norm.contains(needle)
        })
        .map(|i| i + 1)
}

impl App {
    /// 공통 옵션 쌍(`format.<키>` · 값 원문 · 레지스트리 기본값 포함). ★ 들여쓰기 단위·폭은 설정 항목을 그대로 두되 **지금은 활성
    /// 탭의 들여쓰기**(상태줄 팝업으로 바꾼 탭별 값 · 없으면 `editor.tab_size`/`editor.indent_spaces`)를 쓴다(사용자 09-29).
    fn format_option_pairs(&self) -> Vec<(String, String)> {
        // 단위·폭이 "활성 탭 설정"(`editor`)이면 그 탭 값 — 문서 포맷도 미리보기도 같다(사용자 09-29).
        let (tab_size, spaces) = self.editors.indent();
        let is_editor = |k: &str| self.settings.get(k).is_none_or(|v| v == "editor");
        nsql_format::OPTION_KEYS
            .iter()
            .map(|k| {
                let key = format!("format.{k}");
                let v = match *k {
                    "indent" if is_editor(&key) => if spaces { "space" } else { "tab" }.to_string(),
                    "indent_width" if is_editor(&key) => tab_size.max(1).to_string(),
                    // 인라인 주석 간격의 "활성 탭 설정" = 그 탭의 단위(사용자 09-30).
                    "comment_gap" if is_editor(&key) => {
                        if spaces { "space" } else { "tab" }.to_string()
                    }
                    _ => self.settings.get(&key).unwrap_or("").to_string(),
                };
                (key, v)
            })
            .collect()
    }

    /// 지금 포맷에 적용될 들여쓰기(탭 폭, 공백 모드) — 단위·폭 각각 "활성 탭 설정"이면 그 탭 값.
    pub(crate) fn format_indent_applied(&self) -> (u8, bool) {
        let (ts, sp) = self.editors.indent();
        let unit = self.settings.get("format.indent").unwrap_or("editor");
        let width = self.settings.get("format.indent_width").unwrap_or("editor");
        let spaces = match unit {
            "editor" => sp,
            u => u.starts_with('s'),
        };
        let tab_size = match width {
            "editor" => ts.clamp(1, 16),
            w => w.trim().parse::<u8>().unwrap_or(4).clamp(1, 16),
        };
        (tab_size, spaces)
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
            // 표시 = 이름만(`(id)` 꼬리 제거 · 사용자 09-30).
            .map(|(id, name, _)| (id, name))
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
    /// ★ 크기 상한(사용자 09-30 · 72 §2): 문서가 `format.max_kb`(0 = 무제한)를 넘거나 큰 파일 모드(L1 이상)면 **문서 전체 포맷은
    ///   거절**하고, 캐럿 문장은 캐럿 앞뒤 창([`FORMAT_WINDOW_BYTES`] · 줄 경계)에서만 찾는다(인텔리센스 창 방식과 같은 생각) ·
    ///   문장이 창 가장자리에 닿으면(창 밖으로 이어질 수 있음) 거절 · 선택·문장도 상한을 넘으면 거절. `Err` = 상태줄 안내 글.
    fn format_target(&self) -> Result<(usize, usize, String, bool), String> {
        let tb = self.editors.cur();
        let buf = tb.buf();
        let max_kb = self.settings.int("format.max_kb").max(0) as usize;
        let limit = max_kb.saturating_mul(1024);
        let large = self.editors.is_large(self.editors.active());
        let limited = large || (limit > 0 && buf.len_bytes() > limit);
        let too_large = |bytes: usize| {
            tf(
                Msg::StFormatTooLarge,
                &[&nsql_core::fmt_bytes(bytes as u64), &max_kb.to_string()],
            )
        };
        if let Some((a, b)) = tb.selection() {
            if b > a {
                let bytes = buf.byte_len(a, b);
                if limit > 0 && bytes > limit {
                    return Err(too_large(bytes));
                }
                return Ok((a, b, buf.slice_string(a, b), false));
            }
        }
        let caret = tb.caret().min(buf.len());
        let (wa, wb) = if limited {
            format_window(buf, caret)
        } else {
            (0, buf.len())
        };
        let text = buf.slice_string(wa, wb);
        let byte_pos = buf.byte_len(wa, caret);
        if let Some(it) = nsql_script::statement_at_in(&text, byte_pos, Some(self.sess.dialect)) {
            let span = it.span;
            let body = &text[span.clone()];
            if !body.trim().is_empty() {
                let cut_off =
                    (span.start == 0 && wa > 0) || (span.end >= text.len() && wb < buf.len());
                if limited && (cut_off || (limit > 0 && body.len() > limit)) {
                    return Err(too_large(body.len()));
                }
                let from = wa + text[..span.start].chars().count();
                let to = from + body.chars().count();
                return Ok((from, to, body.to_string(), false));
            }
        }
        if limited {
            return Err(too_large(buf.len_bytes()));
        }
        Ok((0, buf.len(), text, true))
    }

    /// 포맷 실행(엔진별) — Basic은 in-process · 확장은 WASM `nx_ext_format`(공통 옵션 + 접두 설정 전달).
    pub(crate) fn format_run(
        &mut self,
        engine: &str,
        text: &str,
        preview: bool,
    ) -> Result<String, String> {
        let pairs = self.format_option_pairs();
        self.format_run_pairs(engine, text, preview, pairs)
    }

    /// Basic 설정을 **쓰지 않은** 공통 옵션(레지스트리 기본값) — 미리보기 B 토글 끔(사용자 09-30 "Basic 기능이 모두 꺼진 상태").
    fn format_default_pairs(&self) -> Vec<(String, String)> {
        nsql_format::OPTION_KEYS
            .iter()
            .map(|k| {
                let key = format!("format.{k}");
                let v = nsql_settings::default_of(&key).unwrap_or("").to_string();
                (key, v)
            })
            .collect()
    }

    /// 엔진 하나로 포맷(옵션 pair를 밖에서 받는다 — 미리보기 B 토글).
    fn format_run_pairs(
        &mut self,
        engine: &str,
        text: &str,
        preview: bool,
        mut pairs: Vec<(String, String)>,
    ) -> Result<String, String> {
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
        let (from, to, text, whole) = match self.format_target() {
            Ok(t) => t,
            Err(msg) => {
                self.sess.status = msg;
                self.redraw();
                return;
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
    /// 설정 창에 넣는다 — `format.*`/`ext.sqlfmt_kiros33.*` 변경 · 설정 창 열 때.
    pub(crate) fn prefs_format_preview_refresh(&mut self) {
        if self.prefs_win.window().is_none() {
            return;
        }
        // ★ B/K 토글(사용자 09-30): K = kiros33(보고 있는 분류가 확장이면 기본 켬) · B = Basic 설정 적용(끄면 레지스트리 기본값) ·
        //   둘 다 끔 = 원본 샘플. 확장 분류를 보고 있으면 그 확장으로(사용자 09-29) · 아니면 기본 포맷터.
        // 확장 분류를 볼 때만 확장 토글이 있다(Basic 분류 = B만 · 사용자 09-30) · 그 글자 = 포맷터 이름의 이니셜(설치된 것끼리 겹치지 않게).
        let engines = self.format_engines();
        let ext_engine = self
            .prefs_win
            .selected_extension()
            .filter(|id| engines.iter().any(|(e, _, _)| e == id))
            .map(str::to_string);
        if let Some(id) = &ext_engine {
            let mut taken: Vec<char> = Vec::new();
            let mut letter = 'K';
            for (eid, name, _) in engines.iter().filter(|(e, _, _)| e != BASIC_ENGINE) {
                let c = formatter_initial(name, &taken);
                taken.push(c);
                if eid == id {
                    letter = c;
                }
            }
            self.prefs_win.set_preview_k_letter(letter);
        }
        let (use_basic, use_k) = self.prefs_win.preview_flags();
        let engine = match (use_k, ext_engine) {
            (true, Some(id)) => id,
            _ => BASIC_ENGINE.to_string(),
        };
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
        let out = if !use_basic && !use_k {
            src.clone()
        } else {
            let pairs = if use_basic {
                self.format_option_pairs()
            } else {
                self.format_default_pairs()
            };
            match self.format_run_pairs(&engine, &src, true, pairs) {
                Ok(o) => o,
                Err(e) => format!("-- {}\n-- {e}\n", t(Msg::StFormatPreviewFailed)),
            }
        };
        let state = if !use_basic && !use_k {
            t(Msg::TitlePreviewRaw).to_string()
        } else {
            let mut s = String::new();
            if use_basic {
                s.push('B');
            }
            if use_k && engine != BASIC_ENGINE {
                s.push(self.prefs_win.preview_k_letter());
            }
            s
        };
        let title = format!(
            "{} · {state}",
            tf(
                Msg::TitleFormatPreview,
                &[&self.format_engine_name(&engine)],
            )
        );
        // ★ 미리보기 상자의 탭 폭/단위도 실제 적용값(단위·폭 각각 활성 탭 설정 또는 고정값).
        let (tab_size, spaces) = self.format_indent_applied();
        self.prefs_win.set_preview(title, &out, tab_size, spaces);
        // ★ 카드 덧말(사용자 09-29): 활성 탭의 실제 들여쓰기 — 단위·폭 카드에 강조색으로.
        let (ts, sp) = self.editors.indent();
        let title = self.editors.title_of(self.editors.active());
        let unit = t(if sp { Msg::StSpaces } else { Msg::StTabSize });
        let unit = unit.split(':').next().unwrap_or("").trim().to_string();
        let note = tf(Msg::NoteIndentActiveTab, &[&title, &unit, &ts.to_string()]);
        // `*` 펼치기의 쉼표 뒤 글자도 같은 "활성 탭 설정" — 같은 덧말(사용자 09-29).
        self.prefs_win
            .set_note("intel.star_comma_space", Some(note.clone()));
        // ★ 설정마다 "▶ 미리보기 n행"(사용자 09-30 · 처음 드러나는 줄 하나) — 들여쓰기 둘은 활성 탭 덧말 뒤에 같이.
        for (key, needle) in PREVIEW_MARKS {
            let line =
                preview_line_of(&out, needle).map(|n| tf(Msg::NotePreviewLine, &[&n.to_string()]));
            let text = if *key == "format.indent" || *key == "format.indent_width" {
                Some(match line {
                    Some(l) => format!("{note} · {l}"),
                    None => note.clone(),
                })
            } else {
                line
            };
            self.prefs_win.set_note(key, text);
        }
        // ★ 확장 설정의 표식 = 그 확장 메타(`formatter.marks`) — 지금 미리보기 엔진의 것만 줄 번호, 다른 확장은 비운다.
        let ids: Vec<String> = self
            .format_engines()
            .into_iter()
            .map(|(id, ..)| id)
            .collect();
        for id in ids.iter().filter(|id| id.as_str() != BASIC_ENGINE) {
            let mine = *id == engine && use_k;
            for (key, needle) in self.extensions.formatter_marks(id) {
                let text = if mine {
                    preview_line_of(&out, &needle)
                        .map(|n| tf(Msg::NotePreviewLine, &[&n.to_string()]))
                } else {
                    None
                };
                self.prefs_win.set_note(&key, text);
            }
        }
    }

    /// 설정이 바뀌었다(`format.*` · `ext.sqlfmt_kiros33.*`) → 열려 있는 미리보기 탭만 다시 그린다(닫혀 있으면 아무것도 안 함).
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

/// 큰 문서의 캐럿 문장 탐색 창(바이트 · 캐럿 앞뒤 각각 · 사용자 09-30 · 인텔리센스 `intel.max_doc_kb` 창과 같은 폭).
pub(crate) const FORMAT_WINDOW_BYTES: usize = 256 * 1024;

/// 캐럿 앞뒤 [`FORMAT_WINDOW_BYTES`] 안에서 **줄 경계**로 자른 창 [a, b)(글자 인덱스). 문서 전체를 복사하지 않는다(버퍼 줄 표만). 순수.
pub(crate) fn format_window(buf: &nexa_ctl::edit::TextBuf, caret: usize) -> (usize, usize) {
    let n = buf.line_count();
    if n == 0 {
        return (0, 0);
    }
    let cl = buf.line_of(caret.min(buf.len()));
    let mut top = cl;
    while top > 0 && buf.byte_len(buf.line_start(top - 1), caret) <= FORMAT_WINDOW_BYTES {
        top -= 1;
    }
    let mut bot = cl;
    while bot + 1 < n && buf.byte_len(caret, buf.line_end(bot + 1)) <= FORMAT_WINDOW_BYTES {
        bot += 1;
    }
    (buf.line_start(top), buf.line_end(bot))
}

#[cfg(test)]
mod tests {
    /// 창 방식(사용자 09-30): 줄 경계 · 캐럿 포함 · 작은 문서 = 전체 · 큰 문서 = 앞뒤 창 폭 안.
    #[test]
    fn format_window_is_line_bounded_around_caret() {
        use super::{format_window, FORMAT_WINDOW_BYTES};
        let small = nexa_ctl::edit::TextBuf::from_string("select 1;\nselect 2;\n".into());
        assert_eq!(format_window(&small, 12), (0, small.len()));
        let line = "select a, b from t where x = 1;\n";
        let big: String = line.repeat(40_000); // ≈ 1.3 MB
        let buf = nexa_ctl::edit::TextBuf::from_string(big);
        let caret = buf.len() / 2;
        let (a, b) = format_window(&buf, caret);
        assert!(a <= caret && caret <= b);
        assert_eq!(a, buf.line_start(buf.line_of(a)));
        assert!(buf.byte_len(a, caret) <= FORMAT_WINDOW_BYTES + line.len());
        assert!(buf.byte_len(caret, b) <= FORMAT_WINDOW_BYTES + line.len());
        assert!(b - a < buf.len() / 2);
    }

    /// 확장 토글 이니셜(사용자 09-30): 마지막 낱말 첫 글자 · B 제외 · 겹치면 다음 후보.
    #[test]
    fn formatter_initial_picks_distinct_letter() {
        use super::formatter_initial;
        assert_eq!(formatter_initial("SQL Formatter for kiros33", &[]), 'K');
        assert_eq!(formatter_initial("SQL Formatter for kiros33", &['K']), 'F');
        assert_eq!(formatter_initial("Basic", &[]), 'A', "B는 Basic 몫");
        assert_eq!(formatter_initial("", &[]), 'A');
    }

    /// 미리보기 줄 찾기(사용자 09-30): 공백·대소문자 무시 · Basic 조각은 전부 Basic 샘플 결과에 있다.
    #[test]
    fn preview_marks_all_present_in_basic_sample() {
        use super::{preview_line_of, PREVIEW_MARKS};
        assert_eq!(preview_line_of("a\n  B . Qty\t>\t0\n", "B.QTY>0"), Some(2));
        assert_eq!(preview_line_of("a\n", "ZZZ"), None);
        let out =
            nsql_format::format_basic(nsql_format::SAMPLE_SQL, &nsql_format::Options::default());
        for (key, needle) in PREVIEW_MARKS
            .iter()
            .filter(|(k, _)| k.starts_with("format."))
        {
            assert!(
                preview_line_of(&out, needle).is_some(),
                "{key} ({needle})\n{out}"
            );
        }
    }
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
