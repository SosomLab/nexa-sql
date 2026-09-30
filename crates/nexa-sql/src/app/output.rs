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
        let (max, ts) = self.output_limits();
        let panel = if ed == self.panel_editor {
            Some(&mut self.panel)
        } else {
            self.panels.get_mut(&ed)
        };
        panel.map(|p| p.output.get_or_insert_with(|| OutputView::new(max, ts)))
    }

    /// 활성 패널의 Output 자리표시 탭 하나(그리드는 빈 것 · 고정 = 자동 회수 제외 · 이름 있음 = 번호 매기기 제외).
    fn output_tab(&mut self, fresh: grid::Grid) -> ResultTab {
        let id = self.next_result_id;
        self.next_result_id += 1;
        ResultTab {
            id,
            title: t(Msg::ResultTabOutput).to_string(),
            pinned: true,
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
    }

    /// 탭이 없으면 만들고, 전환 정책이면 그 탭으로(포커스는 그대로 — 편집 중인 캐럿을 뺏지 않는다).
    fn output_ensure_tab(&mut self, ed: u64, kind: OutKind) {
        let act = self
            .settings
            .get("output.activate")
            .unwrap_or("no_results")
            .to_string();
        let activate = match act.as_str() {
            "always" => true,
            "never" => false,
            _ => !self.sess.run_had_rs || kind == OutKind::Error,
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
            } => {
                if done_lines {
                    let n = (index + 1).to_string();
                    let line = match rows_affected {
                        Some(r) => tf(Msg::OutStmtDoneRows, &[&n, &r.to_string(), &ms(elapsed)]),
                        None => tf(Msg::OutStmtDone, &[&n, &ms(elapsed)]),
                    };
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
            RunEvent::Message(m) => self.output_push(ed, OutKind::Info, m),
            RunEvent::Warning(m) => self.output_push(ed, OutKind::Warn, m),
            RunEvent::Error { line, error, .. } => {
                let text = tf(Msg::LogLineSummary, &[&line.to_string(), &error.message]);
                self.output_push(ed, OutKind::Error, &text);
            }
            _ => {}
        }
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
