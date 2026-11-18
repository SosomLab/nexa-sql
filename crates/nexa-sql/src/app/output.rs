//! App — ★ Output 탭 호스트(09-30 · 사용자 "Output 탭 개념" · docs/43 §12): 실행 이벤트 → 실행한 편집기 탭의 Output 버퍼 ·
//! 표시 정책(`output.show` off/auto/errors/always · `output.activate` never/no_results/always) · View ▸ Output 토글 · 설정 반영.
//!
//! 버퍼는 편집기 탭의 결과 패널(`ResultPanel::output`)이 가진다 — 탭을 닫아도 본문은 남고(View ▸ Output으로 다시 연다), 편집기 탭이
//! 닫히면 패널과 함께 사라진다. 결과 영역의 "Output" 탭은 그 버퍼를 보여 주는 자리표시(`ResultTab::is_output` · 그리드는 빈 것).
use crate::output::{OutKind, OutputView};
use crate::*;

impl App {
    fn output_limits(&self) -> (usize, bool) {
        (
            self.settings.int("output.max_lines").clamp(100, 100_000) as usize,
            self.settings.flag("output.timestamps"),
        )
    }

    /// 편집기 탭의 Output 버퍼(없으면 만든다 · 그 탭의 결과 패널이 없으면 `None` = 실행한 적 없는 탭).
    fn output_view_mut(&mut self, ed: u64) -> Option<&mut OutputView> {
        // AI 도구의 숨김 실행은 Output 탭에 적지 않는다(탭이 새로 생기던 관찰 11-12 · 오류는 도구 결과로 어시스턴트에게 간다).
        if self.ai.pending.as_ref().is_some_and(|p| p.silent) {
            return None;
        }
        let (max, ts) = self.output_limits();
        let dbl = self.settings.int("ui.dblclick_ms").clamp(100, 1000) as u64;
        let panel = if ed == self.panel_editor {
            Some(&mut self.panel)
        } else {
            self.panels.get_mut(&ed)
        };
        panel.map(|p| {
            let v = p.output.get_or_insert_with(|| OutputView::new(max, ts));
            v.set_dblclick_ms(dbl);
            v
        })
    }

    /// ★ 탭 제목: Output 탭은 **단일 토글 창**이라 제목은 늘 `Output`(사용자 10-06 "(N) 이름 불필요" — 10-01 T-266의
    /// 읽지 않은 수 배지는 접는다 · 활성이 되면 읽지 않은 수를 거둔다).
    pub(crate) fn output_sync_badge(&mut self) {
        let Some(i) = self.panel.output_index() else {
            return;
        };
        let active = self.panel.active == i;
        let title = t(Msg::ResultTabOutput).to_string();
        if active {
            if let Some(v) = self.panel.output.as_mut() {
                v.take_unread();
            }
        }
        if self.panel.tabs[i].title != title {
            self.panel.tabs[i].title = title;
            self.panel.sync_bar();
        }
    }

    /// Output에서 두 번 클릭한 오류 줄 → 편집기 그 줄(호스트 · 입력 라우팅 뒤).
    pub(crate) fn output_after_event(&mut self) {
        let goto = self
            .panel
            .output_active_mut()
            .and_then(OutputView::take_goto);
        if let Some((n, col)) = goto {
            self.output_goto(n, col);
        }
        let cmd = self
            .panel
            .output_active_mut()
            .and_then(OutputView::take_cmd);
        if let Some(id) = cmd {
            self.menu_action(&id);
        }
    }

    /// 자체 시험(기동 명령 `output.goto_error:<n>`): 뒤에서 n번째 오류 줄을 두 번 클릭한 것과 같은 이동(Output 탭이 활성이 아니어도).
    pub(crate) fn output_goto_error(&mut self, nth: usize) {
        let goto = self.panel.output.as_mut().and_then(|v| {
            v.goto_error_nth(nth);
            v.take_goto()
        });
        if let Some((n, col)) = goto {
            self.output_goto(n, col);
        }
    }

    /// Output 오류 줄 → 편집기 줄(·열)로 캐럿.
    fn output_goto(&mut self, n: usize, col: Option<usize>) {
        {
            self.editors.cur_mut().goto_line(n);
            // ★ 11-19: 열이 있으면 그 글자로(줄 길이를 넘으면 줄 끝 · 실행 뒤 편집으로 줄이 짧아졌을 때).
            if let Some(c) = col {
                let ch = {
                    let buf = self.editors.cur().buf();
                    let l = (n - 1).min(buf.line_count().saturating_sub(1));
                    let len = buf
                        .line_text(l)
                        .trim_end_matches(['\n', '\r'])
                        .chars()
                        .count();
                    buf.line_start(l) + (c - 1).min(len)
                };
                let mut inv = Invalidations::default();
                self.ed_mut().select_range(ch, ch, &mut inv);
            }
            self.set_focus(Focus::Editor);
        }
    }

    /// 활성 패널의 Output 자리표시 탭 하나(그리드는 빈 것 · 고정 = 자동 회수 제외 · 이름 있음 = 번호 매기기 제외).
    fn output_tab(&mut self, fresh: grid::Grid) -> ResultTab {
        let id = self.next_result_id;
        self.next_result_id += 1;
        ResultTab {
            id,
            title: t(Msg::ResultTabOutput).to_string(),
            pinned: true,
            sys_pinned: true,
            named: true,
            sql: String::new(),
            grid: fresh,
            seq: id,
            child_of: None,
            is_output: true,
        }
    }

    /// 한 줄 넣기 + 표시 정책. `ed` = 편집기 탭 id(실행한 탭 = `sess.run_editor`).
    pub(crate) fn output_push(&mut self, ed: u64, kind: OutKind, msg: &str) {
        let Some(v) = self.output_view_mut(ed) else {
            return;
        };
        v.push(kind, msg);
        let mode = self
            .settings
            .get("output.show")
            .unwrap_or("auto")
            .to_string();
        let show = match mode.as_str() {
            "off" => false,
            "errors" => kind != OutKind::Info,
            _ => true,
        };
        if show {
            self.output_ensure_tab(ed, kind);
        }
        self.output_sync_badge();
    }

    /// 탭이 없으면 만들고, 전환 정책이면 그 탭으로(포커스는 그대로 — 편집 중인 캐럿을 뺏지 않는다).
    fn output_ensure_tab(&mut self, ed: u64, kind: OutKind) {
        let act = self
            .settings
            .get("output.activate")
            .unwrap_or("no_results")
            .to_string();
        // 조건 바 실행의 오류는 결과가 그대로 남으므로(그리드 유지) Output으로 넘어가지 않는다(사용자 10-06 "결과가 사라진 것처럼").
        let cond_err = kind == OutKind::Error && self.grid.cond_run_pending();
        let activate = match act.as_str() {
            "always" => true,
            "never" => false,
            _ => (!self.sess.run_had_rs || kind == OutKind::Error) && !cond_err,
        };
        if ed == self.panel_editor {
            let idx = match self.panel.output_index() {
                Some(i) => i,
                None => {
                    let tab = self.output_tab(self.grid.fresh_like());
                    let i = self.panel.push(tab);
                    if self.panel.take_bar_changed() {
                        self.layout();
                    }
                    self.panel.sync_bar();
                    i
                }
            };
            if activate && self.panel.active != idx {
                let f = self.focus;
                self.activate_result(idx);
                self.set_focus(f);
            }
        } else if let Some(p) = self.panels.get_mut(&ed) {
            if p.output_index().is_none() {
                let id = self.next_result_id;
                self.next_result_id += 1;
                let fresh = p
                    .tabs
                    .first()
                    .map_or_else(grid::Grid::default, |t| t.grid.fresh_like());
                let i = p.push(ResultTab {
                    id,
                    title: t(Msg::ResultTabOutput).to_string(),
                    pinned: true,
                    sys_pinned: true,
                    named: true,
                    sql: String::new(),
                    grid: fresh,
                    seq: id,
                    child_of: None,
                    is_output: true,
                });
                if activate {
                    p.active = i;
                }
                p.sync_bar();
            } else if activate {
                if let Some(i) = p.output_index() {
                    p.active = i;
                }
            }
        }
        self.redraw();
    }

    /// View ▸ Output(활성 편집기 탭): 보이면 닫고(본문은 남는다) · 없으면 열고 그 탭으로.
    pub(crate) fn output_toggle(&mut self) {
        if let Some(i) = self.panel.output_index() {
            self.close_result_tab(i);
        } else {
            self.output_open_active();
        }
        self.panel.sync_bar();
        self.redraw();
    }

    fn output_open_active(&mut self) {
        let (max, ts) = self.output_limits();
        self.panel
            .output
            .get_or_insert_with(|| OutputView::new(max, ts));
        let tab = self.output_tab(self.grid.fresh_like());
        let i = self.panel.push(tab);
        if self.panel.take_bar_changed() {
            self.layout();
        }
        self.activate_result(i);
    }

    /// 설정 변경(`output.max_lines` · `output.timestamps` · `output.show`=always).
    pub(crate) fn output_apply_settings(&mut self) {
        let (max, ts) = self.output_limits();
        if let Some(v) = self.panel.output.as_mut() {
            v.set_limits(max, ts);
        }
        for p in self.panels.values_mut() {
            if let Some(v) = p.output.as_mut() {
                v.set_limits(max, ts);
            }
        }
        if self.settings.get("output.show") == Some("always") && self.panel.output_index().is_none()
        {
            self.output_open_active();
        }
        self.redraw();
    }

    /// 편집기 탭을 활성화했을 때(`sync_grid_tab` 뒤): `output.show=always`면 Output 탭을 갖춘다.
    pub(crate) fn output_on_tab_switch(&mut self) {
        if self.settings.get("output.show") == Some("always") && self.panel.output_index().is_none()
        {
            let (max, ts) = self.output_limits();
            self.panel
                .output
                .get_or_insert_with(|| OutputView::new(max, ts));
            let tab = self.output_tab(self.grid.fresh_like());
            self.panel.push(tab);
            if self.panel.take_bar_changed() {
                self.layout();
            }
            self.panel.sync_bar();
        }
    }

    /// 실행 이벤트 → Output 줄(실행한 편집기 탭 = `sess.run_editor`). 로그 창과 별개(로그는 전부 · Output은 사람이 읽는 메시지만).
    pub(crate) fn output_on_event(&mut self, ev: &RunEvent) {
        let ed = self.sess.run_editor;
        if ed == 0 {
            return;
        }
        let done_lines = self.settings.flag("output.done_lines");
        let ms = |d: &Duration| d.as_millis().to_string();
        match ev {
            RunEvent::ResultSet {
                index, rs, elapsed, ..
            } => {
                self.sess.run_had_rs = true;
                if done_lines {
                    let line = tf(
                        Msg::OutStmtResult,
                        &[
                            &(index + 1).to_string(),
                            &rs.rows.len().to_string(),
                            &ms(elapsed),
                        ],
                    );
                    self.output_push(ed, OutKind::Info, &line);
                }
            }
            RunEvent::Done {
                index,
                rows_affected,
                elapsed,
                delivery,
            } => {
                if done_lines {
                    let n = (index + 1).to_string();
                    let mut line = match rows_affected {
                        Some(r) => tf(Msg::OutStmtDoneRows, &[&n, &r.to_string(), &ms(elapsed)]),
                        None => tf(Msg::OutStmtDone, &[&n, &ms(elapsed)]),
                    };
                    // 전달 모드 꼬리표(T-416 · 매개변수 있을 때만).
                    if !delivery.is_empty() {
                        line.push_str(&format!(" · {delivery}"));
                    }
                    self.output_push(ed, OutKind::Info, &line);
                }
            }
            RunEvent::Print { pairs } => {
                let text = nsql_run::print_pairs_text(pairs);
                self.output_push(ed, OutKind::Info, &text);
            }
            RunEvent::VarList { vars } => {
                let text = nsql_run::var_list_text(vars);
                self.output_push(ed, OutKind::Info, &text);
            }
            // ★ T-266 등급(10-30): 서버 메시지의 접두(PG `WARNING:`·`EXCEPTION:` …)로 종류를 고른다 — 그 밖 = Info ·
            //   2차(216차) = SQL Server는 Info 토큰(심각도 ≤ 10)만 오므로 늘 Info(`message_level_in`).
            RunEvent::Message(m) => {
                let kind = match nsql_run::message_level_in(Some(self.sess.dialect), m) {
                    nsql_run::MessageLevel::Error => OutKind::Error,
                    nsql_run::MessageLevel::Warning => OutKind::Warn,
                    nsql_run::MessageLevel::Info => OutKind::Info,
                };
                self.output_push(ed, kind, m);
            }
            RunEvent::Warning(m) => self.output_push(ed, OutKind::Warn, m),
            RunEvent::DriverLimits { driver, limits, .. } => {
                let text = driver_limits_text(driver, limits);
                self.output_push(ed, OutKind::Warn, &text);
            }
            RunEvent::Error { index, line, error } => {
                let text = self.error_output_text(*index, *line, error);
                self.output_push(ed, OutKind::Error, &text);
            }
            _ => {}
        }
    }

    /// ★ 오류 사건 → Output 글(사용자 11-19 "line 1로만 나온다 · 두 번 클릭이 빗나간다"): 줄 번호는 **문서 기준**
    /// (실행한 조각의 시작 줄 `run_line_base`를 더한다) · 서버가 문장 안 위치를 줬으면 그 글자의 줄·열 + 그 자리 소스 조각 한 줄.
    /// 위치가 없거나 문서로 옮길 수 없으면(실행 텍스트가 원문과 다름 · 실행한 탭이 닫힘) 문장 시작 줄만.
    fn error_output_text(
        &mut self,
        index: usize,
        line: usize,
        error: &nsql_core::DbError,
    ) -> String {
        let stmt_line = self.sess.run_line_base + line.max(1);
        let start = self.sess.last_run_starts.get(index).copied().flatten();
        let stmt_len = self
            .sess
            .last_run_items
            .get(index)
            .map_or(0, |s| s.chars().count());
        let ch = error
            .position
            .filter(|p| *p > 0 && *p <= stmt_len)
            .and_then(|p| app::run::error_caret(self.sess.run_src_base, start, Some(p), stmt_len));
        let ed = self.sess.run_editor;
        let at = ch.and_then(|ch| {
            let buf = self.editors.buf_mut_by_id(ed)?.buf();
            let l = buf.line_of(ch);
            let col = ch.checked_sub(buf.line_start(l))?;
            let text = buf.line_text(l);
            let near: String = text
                .trim_end_matches(['\n', '\r'])
                .chars()
                .skip(col)
                .take(ERROR_NEAR_CHARS)
                .collect();
            Some((l + 1, col + 1, near))
        });
        error_output_line(
            stmt_line,
            at.as_ref().map(|(l, c, n)| (*l, *c, n.as_str())),
            &error.message,
        )
    }

    /// 자체 시험(기동 명령 `output.dump:<파일>`): 활성 패널의 Output 본문(없으면 `none`) — 첫 줄 = `shown|lines`.
    pub(crate) fn output_dump(&self) -> String {
        match self.panel.output.as_ref() {
            Some(v) => format!(
                "{}|{}\n{}",
                self.panel.output_index().is_some(),
                v.line_count(),
                v.text()
            ),
            None => "none".to_string(),
        }
    }
}

/// Output 오류 줄에 덧붙이는 소스 조각의 최대 글자 수.
const ERROR_NEAR_CHARS: usize = 60;

/// 오류 줄 글(순수) — `at` = 서버 위치의 문서 (줄, 열, 그 자리부터의 소스 조각) · 없으면 문장 시작 줄 `stmt_line`만.
pub(crate) fn error_output_line(
    stmt_line: usize,
    at: Option<(usize, usize, &str)>,
    message: &str,
) -> String {
    match at {
        Some((l, c, near)) => {
            let head = tf(
                Msg::OutErrorLineCol,
                &[&l.to_string(), &c.to_string(), message],
            );
            let near = near.trim();
            if near.is_empty() {
                head
            } else {
                // 이어지는 줄(들여쓰기 = 같은 종류로 그려지고 두 번 클릭은 머리줄로 거슬러 올라간다).
                format!("{head}\n  {}", tf(Msg::OutErrorAt, &[near]))
            }
        }
        None => tf(Msg::LogLineSummary, &[&stmt_line.to_string(), message]),
    }
}

/// ★ 드라이버 제약(docs/110 §6 · 사용자 10-13) → 사용자 문구(i18n). 종류 하나 = 글 하나.
pub(crate) fn limit_msg(l: nsql_core::DriverLimit) -> Msg {
    use nsql_core::DriverLimit as L;
    match l {
        L::Cancel => Msg::LimitCancel,
        L::ImplicitResults => Msg::LimitImplicitResults,
        L::ExternalAuth => Msg::LimitExternalAuth,
        L::NativeEncryption => Msg::LimitNne,
        L::BulkLoad => Msg::LimitBulkLoad,
        L::OldServer => Msg::LimitOldServer,
    }
}

/// 짧은 이름(토스트용 · 사용자 10-13 "핵심 낱말만").
pub(crate) fn limit_short(l: nsql_core::DriverLimit) -> Msg {
    use nsql_core::DriverLimit as L;
    match l {
        L::Cancel => Msg::LimitShortCancel,
        L::ImplicitResults => Msg::LimitShortImplicitResults,
        L::ExternalAuth => Msg::LimitShortExternalAuth,
        L::NativeEncryption => Msg::LimitShortNne,
        L::BulkLoad => Msg::LimitShortBulkLoad,
        L::OldServer => Msg::LimitShortOldServer,
    }
}

/// 제약 목록 한 덩어리(토스트 본문 · Output 줄): `• 제약` 줄들 + 전환 안내 한 줄.
pub(crate) fn driver_limits_text(driver: &str, limits: &[nsql_core::DriverLimit]) -> String {
    let mut lines: Vec<String> = vec![tf(Msg::ToastDriverLimitsTitle, &[driver])];
    lines.extend(limits.iter().map(|l| format!("• {}", t(limit_msg(*l)))));
    lines.push(t(Msg::LimitHintOracle).to_string());
    lines.join("\n")
}

#[cfg(test)]
mod error_line_tests {
    use super::*;

    /// ★ 11-19: 위치가 있으면 문서 줄·열 + 소스 조각 · 없으면 문장 시작 줄 — 그리고 그 글을 Output 파서가 다시 읽는다.
    #[test]
    fn error_line_has_doc_line_col_and_roundtrips() {
        let with = error_output_line(
            1,
            Some((70, 54, "WITHIN GROUP (ORDER BY x)  ")),
            "ORA-00923: x",
        );
        let mut lines = with.lines();
        let head = lines.next().expect("head");
        assert_eq!(
            crate::output::parse_error_line(&format!("[00:50:02] ✖ {head}")),
            Some((70, Some(54)))
        );
        let second = lines.next().expect("near");
        assert!(
            second.starts_with("  ") && second.ends_with("WITHIN GROUP (ORDER BY x)"),
            "{second}"
        );
        let without = error_output_line(12, None, "ORA-00942: y");
        assert_eq!(
            crate::output::parse_error_line(&format!("✖ {without}")),
            Some((12, None))
        );
        // 조각이 비면 머리줄만.
        assert_eq!(
            error_output_line(1, Some((3, 9, "  ")), "m")
                .lines()
                .count(),
            1
        );
    }
}
