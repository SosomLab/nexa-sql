//! App — 코드 완성·시그니처·아웃라인·이동(docs/76).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 캐럿 앞 식별자 길이(줄 안 · 트리거 판정용 · 본문 복사 0).
    fn intel_prefix_len(&self) -> usize {
        let ed = self.editors.cur();
        let buf = ed.buf();
        let caret = ed.caret();
        let l = buf.line_of(caret);
        let start = buf.line_start(l);
        let line = buf.line_text(l);
        let col = caret.saturating_sub(start);
        line.chars()
            .take(col)
            .collect::<Vec<char>>()
            .iter()
            .rev()
            .take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '$' || **c == '#')
            .count()
    }

    /// 편집기가 사건을 처리한 뒤 — 글자면 트리거 규칙 · 열린 팝업은 이동/클릭에 닫힌다.
    pub(crate) fn intel_after_event(&mut self, ev: &InputEvent) {
        if !self.intel.cfg().enabled {
            return;
        }
        match ev {
            // Backspace = `Char('\u{8}')`: 열린 팝업은 접두가 남아 있으면 다시 거르고 없으면 닫는다.
            InputEvent::Char { c: '\u{8}', .. } => {
                if self.intel.is_open() {
                    if self.intel_prefix_len() == 0 {
                        self.intel.close();
                    } else {
                        self.intel_request(false);
                    }
                }
            }
            InputEvent::Char { c, .. } => {
                let n = self.intel_prefix_len();
                if self.intel.after_char(*c, n) {
                    self.intel_request(false);
                }
                // `(` = 시그니처 도움(내장 함수·DBMS_* 멤버 · 상태줄 + 카드 · T-178) · 카드가 떠 있으면 글자마다 다시 판정.
                if *c == '(' || self.sig_card.is_some() {
                    self.intel_signature_help();
                }
            }
            // Esc = 카드 숨김(완성 팝업과 같은 키) · 그 밖의 키·클릭 = 캐럿이 움직였으니 카드가 떠 있으면 다시 판정.
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } if self.sig_card.is_some() => {
                self.sig_card = None;
                if self.intel.is_open() {
                    self.intel.close();
                }
                self.redraw();
            }
            InputEvent::Key { .. }
            | InputEvent::MouseDown { .. }
            | InputEvent::RightDown { .. } => {
                if self.intel.is_open() {
                    self.intel.close();
                }
                if self.sig_card.is_some() {
                    self.intel_signature_help();
                }
            }
            _ => {}
        }
    }

    /// 시그니처 카드의 글과 캐럿 자리(팝업 층 그리기) — 활성 탭·편집기 포커스·설정이 맞을 때만.
    pub(crate) fn sig_card_tip(&self) -> Option<(String, Rect)> {
        let (text, tab) = self.sig_card.as_ref()?;
        if !self.intel.cfg().signature_card
            || self.focus != Focus::Editor
            || self.editors.active_id() != *tab
        {
            return None;
        }
        let tb = self.editors.cur();
        let p = tb.caret_point()?;
        // `point_at`의 y = 줄 바닥 → 캐럿 줄 사각형은 한 줄 위부터(카드가 캐럿 줄을 덮지 않게 · 10-06).
        let lh = tb.line_h();
        Some((text.clone(), Rect::new(p.x, p.y - lh, 1, lh)))
    }

    /// 코드 기능(아웃라인 · 완성 · Goto Symbol)을 이 탭에 써도 되는가 — 큰 파일 단계가 아니고 · **구문이 SQL**이고 · 본문이
    /// 이진스럽지 않아야(첫 8 KB에 NUL 없음 · U+FFFD 8개 미만). 아니면 `Some(안내)`를 돌려준다(사용자 09-23 "`.o` 파일 아웃라인 = 앱 종료 ·
    /// 적합하지 않은 파일은 아웃라인 무시"). 분석기 자체도 패닉하지 않게 고쳤지만(nsql-script fuzz 시험) 뜻 없는 심볼을 만들지 않는다.
    /// `light` = 가벼운 요청(수동 완성 · 시그니처 도움 — 창 방식이라 큰 파일 1단계에서도 된다) · 아니면(자동 팝업 · 아웃라인 · Goto Symbol)
    /// 1단계부터 끈다 · 2단계는 전부 끈다(09-24 §187 · 72).
    fn intel_unsuitable(&self, i: usize, light: bool) -> Option<Msg> {
        if self.editors.is_large(i) {
            let lvl = self.editors.large_level(i);
            if lvl >= 2 || !light {
                return Some(Msg::StLargeFileFeatureOff);
            }
        }
        if self.editors.syntax_name() != "SQL" {
            return Some(Msg::StIntelUnsuitable);
        }
        let buf = self.editors.cur().buf();
        let n = buf.line_count().min(64);
        let mut fffd = 0usize;
        for l in 0..n {
            let line = buf.line_text(l);
            if line.contains('\0') {
                return Some(Msg::StIntelUnsuitable);
            }
            fffd += line.matches('\u{fffd}').count();
            if fffd >= 8 {
                return Some(Msg::StIntelUnsuitable);
            }
        }
        None
    }

    /// 후보 조립 + 팝업(`manual` = Ctrl+Space · 자동 설정과 무관). 큰 파일 단계(L1+)·비SQL·이진 파일에서는 하지 않는다(72 §2).
    pub(crate) fn intel_request(&mut self, manual: bool) {
        if !self.intel.cfg().enabled || self.focus != Focus::Editor {
            return;
        }
        let i = self.editors.active();
        if let Some(why) = self.intel_unsuitable(i, manual) {
            if manual {
                self.sess.status = t(why).into();
            }
            return;
        }
        let tab = self.editors.tab_id(i);
        let (rev, text, caret_c, anchor) = {
            let ed = self.editors.cur();
            (ed.rev(), ed.text(), ed.caret(), ed.caret_point())
        };
        let caret_b = text
            .char_indices()
            .nth(caret_c)
            .map_or(text.len(), |(b, _)| b);
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or_default();
        let spec = self.sess.spec.clone();
        let dialect = Some(self.sess.dialect);
        let (names, snap) = self.explorer.meta_view(spec.as_ref());
        let view = intel::MetaView { names, snap };
        // 접두 시작 글자의 좌표(바이트 → 글자 인덱스 → 마지막 그리기의 줄 배치) — 팝업이 타이핑 중 제자리에 있게(09-23).
        // `*` 펼치기의 쉼표 뒤 글자 = 활성 탭의 들여쓰기 단위(설정 "편집기 설정 따름" · 기본).
        let (_, spaces) = self.editors.indent();
        self.intel.set_editor_tab(!spaces);
        let ed = self.editors.cur();
        let point_of = |b: usize| ed.point_at(text[..b.min(text.len())].chars().count());
        let opened = self.intel.request(
            tab,
            rev,
            &text,
            caret_b,
            dialect,
            Some(&view),
            anchor,
            host,
            self.scale,
            &point_of,
        );
        let needs = self.intel.take_needs();
        for n in needs {
            self.explorer
                .request_columns(spec.as_ref(), n.schema.as_deref(), &n.table, n.urgent);
        }
        // ★ 객체 목록 즉시 채움(`스키마.` · 현재 스키마 · 사전 — 09-23): 탐색기 메타 세션 1건씩 · 오면 아래 drain이 팝업을 다시 그린다.
        for s in self.intel.take_need_objects() {
            self.explorer.request_objects(spec.as_ref(), &s);
        }
        // `JOIN … ON` 조건 조각이 기다리는 테이블 제약(T-178) — 백그라운드 메타 세션 · 오면 팝업을 다시 그린다.
        for id in self.intel.take_need_details() {
            self.explorer.request_detail(spec.as_ref(), id);
        }
        self.intel_card_settle();
        // 예산 초과 = 로그 창 한 줄(개발자 상세 · D-202의 근거).
        if let Some((n, ms)) = self.intel.take_over_budget() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "[intel] candidates={n} took {ms} ms (> intel.budget_ms {})",
                    self.intel.cfg().budget_ms
                ),
            ));
        }
        // 자기 감속(§187): 연속 초과로 이 탭의 문서 낱말이 꺼졌다 — 상태줄 한 번.
        if self.intel.take_degraded().is_some() {
            self.sess.status = t(Msg::StIntelDegraded).into();
        }
        if opened || manual {
            self.redraw();
        }
    }

    /// 상세 카드가 쓸 메타(테이블 상세·컬럼)를 미리 요청(강조 행이 바뀔 때 · 이미 있으면 0 · 09-24).
    /// ★ 인텔리센스 캐시 명시 갱신(79 §4 · T-188): 현재 스키마 / 이 서버 / 전 서버 — 버킷 `Stale`·컬럼 `Unknown` + 그 스키마(전부면
    /// 현재 스키마·사전) 즉시 다시 읽기 · 상태줄 · 열린 팝업은 닫는다(다음 요청이 새 것으로).
    pub(crate) fn intel_refresh(&mut self, id: &str) {
        let spec = self.sess.spec.clone();
        let (schema, all) = match id {
            "intel.refresh_all" => (None, true),
            "intel.refresh_server" => (None, false),
            _ => (self.explorer.current_schema(spec.as_ref()), false),
        };
        let (b, o) = self
            .explorer
            .refresh_meta(spec.as_ref(), schema.as_deref(), all);
        self.intel.close();
        self.sess.status = tf(Msg::StIntelRefreshed, &[&b.to_string(), &o.to_string()]);
        self.redraw();
    }

    /// 강조 행이 바뀐 뒤: 목표만 적고, 머문 대상이 넘어왔을 때만 선조회(빠른 스크롤 중 서버 상세 질의 0 · 09-24).
    pub(crate) fn intel_card_settle(&mut self) {
        let now = Instant::now();
        self.intel.note_hover(now);
        if self.intel.card_tick(now) {
            self.intel_card_prefetch();
        }
    }

    pub(crate) fn intel_card_prefetch(&mut self) {
        if !self.intel.cfg().detail_card {
            return;
        }
        if let Some(id) = self.intel.card_target().and_then(|t| t.needs()) {
            let spec = self.sess.spec.clone();
            self.explorer.request_detail(spec.as_ref(), id);
        }
    }

    /// 확정 글자를 편집기에(접두 구간 교체 · `caret_back` = `NAME()` 안으로).
    pub(crate) fn intel_apply(&mut self) {
        let Some(a) = self.intel.take_accept() else {
            return;
        };
        let text = self.editors.cur().text();
        let from = text[..a.replace.start.min(text.len())].chars().count();
        let to = text[..a.replace.end.min(text.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().replace_range(from, to, &a.text, &mut inv);
        if a.caret_back > 0 {
            let at = (from + a.text.chars().count()).saturating_sub(a.caret_back);
            self.ed_mut().select_range(at, at, &mut inv);
            // 괄호 안에서 바로 시그니처(확정한 함수의).
            self.intel_signature_help();
        }
        self.redraw();
    }

    /// 시그니처 도움(`intel.signature_help`): 캐럿을 감싸는 `(`의 주인이 내장 함수면 상태줄에 시그니처 한 줄.
    pub(crate) fn intel_signature_help(&mut self) {
        if !self.intel.cfg().signature_help || self.focus != Focus::Editor {
            return;
        }
        let i = self.editors.active();
        if self.intel_unsuitable(i, true).is_some() {
            return;
        }
        let (text, caret_c) = {
            let ed = self.editors.cur();
            (ed.text(), ed.caret())
        };
        let caret_b = text
            .char_indices()
            .nth(caret_c)
            .map_or(text.len(), |(b, _)| b);
        let tab = self.editors.active_id();
        match self
            .intel
            .signature_at(&text, caret_b, Some(self.sess.dialect))
        {
            Some(sig) => {
                self.sess.status = tf(Msg::StIntelSignature, &[&sig]);
                if self.intel.cfg().signature_card {
                    self.sig_card = Some((sig, tab));
                }
                self.redraw();
            }
            None if self.sig_card.is_some() => {
                // 괄호 밖으로 나갔다(또는 모르는 함수) = 카드 내림.
                self.sig_card = None;
                self.redraw();
            }
            None => {}
        }
    }

    /// Goto Symbol(Ctrl+R · Sublime): 문서 아웃라인 심볼 목록을 팔레트에 — 고르면 `sym:<byte>`.
    pub(crate) fn open_goto_symbol(&mut self) {
        let i = self.editors.active();
        if let Some(why) = self.intel_unsuitable(i, false) {
            self.sess.status = t(why).into();
            self.redraw();
            return;
        }
        let tab = self.editors.tab_id(i);
        let (rev, text) = {
            let ed = self.editors.cur();
            (ed.rev(), ed.text())
        };
        let dialect = Some(self.sess.dialect);
        let ol = self
            .intel
            .outline_for(tab, rev, &|| text.clone(), dialect)
            .clone();
        let mut cmds: Vec<(String, String)> = Vec::new();
        for s in &ol.symbols {
            let indent = "  ".repeat(s.depth as usize);
            let detail = if s.detail.is_empty() {
                s.kind.label().to_string()
            } else {
                format!("{} · {}", s.kind.label(), s.detail)
            };
            cmds.push((
                format!("sym:{}", s.byte),
                format!("{indent}{}  —  {detail}  :{}", s.name, s.line),
            ));
        }
        if cmds.is_empty() {
            self.sess.status = t(Msg::OutlineEmpty).into();
            self.redraw();
            return;
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 아웃라인 패널 동기 — 활성 탭·본문 세대가 바뀌었을 때만 심볼을 다시 준다(큰 파일 단계는 비움).
    pub(crate) fn outline_sync(&mut self) {
        if !self.outline_panel.is_visible() {
            return;
        }
        let i = self.editors.active();
        let tab = self.editors.tab_id(i);
        let rev = self.editors.cur().rev();
        if self.outline_panel.key() == Some((tab, rev)) {
            return;
        }
        if !self.intel.cfg().enabled || self.intel_unsuitable(i, false).is_some() {
            // 적합하지 않은 파일(큰 파일 · SQL 아님 · 이진) = 빈 아웃라인(사용자 09-23).
            self.outline_panel
                .set_symbols((tab, rev), &nsql_script::outline::Outline::default());
            return;
        }
        let text = self.editors.cur().text();
        let dialect = Some(self.sess.dialect);
        let ol = self
            .intel
            .outline_for(tab, rev, &|| text.clone(), dialect)
            .clone();
        self.outline_panel.set_symbols((tab, rev), &ol);
    }

    /// 아웃라인 패널의 열기 요청 → 그 자리로.
    pub(crate) fn outline_pump(&mut self) {
        if let Some(b) = self.outline_panel.take_open() {
            self.goto_byte(b);
        }
    }

    /// 바이트 오프셋으로 캐럿 이동(심볼 이동).
    pub(crate) fn goto_byte(&mut self, byte: usize) {
        let text = self.editors.cur().text();
        let idx = text[..byte.min(text.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(idx, idx, &mut inv);
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// Goto Anything(T-96 · Sublime Ctrl+P): 열린 탭(제목 · 경로 · `*`) + 최근 파일 · `:숫자` = 줄 이동.
    pub(crate) fn open_goto_anything(&mut self, prefill: &str) {
        let mut cmds: Vec<(String, String)> = Vec::new();
        for (id, title, path, dirty, active) in self.editors.tab_entries() {
            let mark = if dirty { "*" } else { "" };
            let where_ = path
                .as_deref()
                .map(nexa_fs::path::display)
                .unwrap_or_else(|| t(Msg::PalUntitled).to_string());
            let act = if active { "✓ " } else { "" };
            cmds.push((
                format!("tab:{id}"),
                format!("{act}{title}{mark}  —  {where_}"),
            ));
        }
        for (i, p) in self.recent_files().iter().enumerate() {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            cmds.push((
                format!("file.recent:{i}"),
                format!(
                    "{}: {name}  {}",
                    t(Msg::LblFileRecent),
                    nexa_fs::path::display(p)
                ),
            ));
        }
        self.palette.set_commands(cmds);
        self.palette.open(prefill);
        self.ime_refresh();
        self.redraw();
    }

    /// 인텔리센스 설정 + 라이선스(절별 완성·상세 카드 = Pro · 기본 완성은 무료).
    pub(crate) fn intel_cfg(&self) -> intel::IntelCfg {
        let mut c = intel::IntelCfg::from_settings(&self.settings);
        let ok = self.entitled(nsql_license::Feature::GrammarCompletion);
        c.grammar = ok;
        c.detail_card = c.detail_card && ok;
        c
    }
}
