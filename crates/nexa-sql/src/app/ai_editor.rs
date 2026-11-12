//! App — AI 창 ↔ 편집기 연동(docs/110 §18 · 사용자 11-12 "상호운용성 강화"):
//!
//! - 편집기 → AI: 우클릭 "AI ▸"(설명 · 오류 수정 · 최적화 검토 · 첨부) · 팔레트 명령 `ai.explain_selection` 등.
//! - 오류 → AI: 실행 오류 알림 버튼 · Output 우클릭 `ai.fix_error`(마지막 오류의 문장·메시지·줄을 붙여 보낸다).
//! - AI → 편집기: 코드 블록 [교체](선택 또는 커서 문장) · 답의 줄 번호 클릭 → 그 줄로.
//! - 그리드 → AI: 우클릭 "고급 복사 ▸ AI에 첨부"(선택 영역을 첨부 칩으로 · 전송은 사용자가).
//! - 변경 미리보기: 편집기를 바꾸는 도구(`edit_editor` · `set_editor_text`)의 승인 카드에 전후 줄.
//!
//! 동작은 설정으로 바꾼다(확장성 · 사용자 11-12): `ai.send_target`(side|tab) · `ai.error_offer` · `ai.replace_target`
//! (selection|statement) · `ai.diff_context`(0 = 미리보기 끔) · `ai.line_links` · `ai.prompt_explain/_fix/_optimize`(빈 값 = 기본 문구).

use crate::ai::Attachment;
use crate::*;
use nsql_settings::json::{parse as json_parse, Json};

/// 편집기 부분 편집 계획(도구 `edit_editor` · 실행과 미리보기가 같은 계산을 쓴다).
pub(crate) struct EditPlan {
    /// 바꿀 글자 범위(시작 · 길이).
    pub start: usize,
    pub len: usize,
    /// 줄 범위(1부터 · 양끝 포함).
    pub a: usize,
    pub b: usize,
    pub op: String,
    /// 새 글(`select`면 None).
    pub new: Option<String>,
}

impl App {
    /// 채팅 **뷰 탭**이 활성이면 가장 최근의 편집기 탭으로 바꾼다(없으면 새 탭) — 편집기를 겨냥하는 동작의 첫 단계.
    pub(crate) fn ai_target_editor(&mut self) {
        if !self.ai_view_active() {
            return;
        }
        let target = self
            .editors
            .tab_history()
            .into_iter()
            .rev()
            .map(|(id, _)| id)
            .find(|id| {
                self.editors
                    .index_of_id(*id)
                    .is_some_and(|i| self.editors.view_key_of(i).is_none())
            });
        match target.and_then(|id| self.editors.index_of_id(id)) {
            Some(i) => self.editors.switch(i),
            None => self.editors.new_tab(None),
        }
        self.layout();
        self.sync_grid_tab();
    }

    /// 커서 문장의 (글자 시작 · 글자 끝 · 줄 a · 줄 b) — 없으면 None.
    fn ai_caret_statement(&self, full: &str) -> Option<(usize, usize, usize, usize)> {
        let byte_pos = full
            .char_indices()
            .nth(self.editors.cur().caret())
            .map_or(full.len(), |(x, _)| x);
        let it = nsql_script::statement_at_in(full, byte_pos, Some(self.sess.dialect))?;
        if full[it.span.clone()].trim().is_empty() {
            return None;
        }
        let from = full[..it.span.start].chars().count();
        let to = from + full[it.span.clone()].chars().count();
        let line = |b: usize| full[..b.min(full.len())].matches('\n').count() + 1;
        Some((
            from,
            to,
            line(it.span.start),
            line(it.span.end.saturating_sub(1).max(it.span.start)),
        ))
    }

    /// 편집기의 작업 대상: 선택(있고 `prefer_sel`이면) → 커서 문장 — (글자 시작 · 끝 · 글 · 줄 a · 줄 b).
    fn ai_editor_target(&self, prefer_sel: bool) -> Option<(usize, usize, String, usize, usize)> {
        let full = self.editors.cur().text();
        let chars: Vec<char> = full.chars().collect();
        let line_at = |ch: usize| chars.iter().take(ch).filter(|c| **c == '\n').count() + 1;
        if prefer_sel {
            if let Some((s0, s1)) = self.editors.cur().selection() {
                let (lo, hi) = (s0.min(s1), s0.max(s1).min(chars.len()));
                if hi > lo {
                    let text: String = chars[lo..hi].iter().collect();
                    return Some((lo, hi, text, line_at(lo), line_at(hi.saturating_sub(1))));
                }
            }
        }
        let (from, to, a, b) = self.ai_caret_statement(&full)?;
        let text: String = chars[from..to.min(chars.len())].iter().collect();
        Some((from, to, text, a, b))
    }

    /// AI 창을 앞으로(설정 `ai.send_target`: side = 편집기 옆 분할 창 · tab = 채팅 탭).
    fn ai_bring_pane(&mut self) {
        self.ai_init();
        if self.settings.get("ai.send_target") == Some("tab") {
            self.ai_open_chat();
        } else {
            if !self.ai_side_on() {
                self.ai_toggle_side();
            }
            // ★ 지금 세션을 이 편집기 탭의 분할 창 세션으로 묶는다 — 묶지 않으면 첫 그리기(`ai_side_prepare`)가 "이 탭의 세션 없음"으로
            //   보고 빈 세션을 새로 만들어, 방금 보낸 요청이 다른 세션에서 돌았다(협업 V1 11-12).
            if self.ai.cur.is_none() {
                self.ai_new_session();
            }
            let tid = self.editors.active_id();
            if let Some(id) = self.ai_cur().map(|s| s.id.clone()) {
                self.ai.tab_sessions.insert(tid, id);
            }
            self.ai.side_tab = Some(tid);
            self.ai.on_chat = None;
        }
    }

    /// 요청 문구(설정 `ai.prompt_<kind>`가 비면 기본 문구).
    fn ai_prompt_of(&self, kind: &str) -> String {
        let custom = self
            .settings
            .get(&format!("ai.prompt_{kind}"))
            .unwrap_or("")
            .trim()
            .to_string();
        if !custom.is_empty() {
            return custom;
        }
        t(match kind {
            "fix" => Msg::AiPromptFix,
            "optimize" => Msg::AiPromptOptimize,
            _ => Msg::AiPromptExplain,
        })
        .to_string()
    }

    /// ★ 편집기 → AI(우클릭 "AI ▸" · 팔레트): `explain` · `fix` · `optimize` = 선택(없으면 커서 문장)을 붙여 바로 보낸다 ·
    /// `attach` = 첨부 칩으로만 올린다(질문은 사용자가 적는다).
    pub(crate) fn ai_editor_request(&mut self, kind: &str) {
        if self.ai_view_active() {
            self.sess.status = t(Msg::AiNeedEditorTab).into();
            self.redraw();
            return;
        }
        let Some((_, _, sql, a, b)) = self.ai_editor_target(true) else {
            self.sess.status = t(Msg::AiNoStatement).into();
            self.redraw();
            return;
        };
        // 글은 창을 열기 **전에** 떴다(채팅 탭으로 열면 활성 탭이 바뀐다).
        self.ai_bring_pane();
        if kind == "attach" {
            let (body, hidden) = crate::ai::redact_count(&sql);
            if self.ai.cur.is_none() {
                self.ai_new_session();
            }
            if let Some(s) = self.ai_cur_mut() {
                s.attachments.push(Attachment::Text {
                    label: format!("editor L{a}-{b}"),
                    body,
                });
            }
            self.ai.status = if hidden > 0 {
                tf(Msg::AiSecretMasked, &[&hidden.to_string()])
            } else {
                t(Msg::AiAttached).to_string()
            };
            self.ai_sync_panel();
            self.redraw();
            return;
        }
        let prompt = self.ai_prompt_of(kind);
        let where_ = tf(Msg::AiEditorLines, &[&a.to_string(), &b.to_string()]);
        self.ai_send(&format!("{prompt}\n{where_}\n\n```sql\n{sql}\n```"));
    }

    /// 실행 오류를 기억한다(문장 · 메시지 · 편집기 줄) — 요청 자리는 **기존 실행 오류 카드**의 [AI] 버튼(`ai.error_offer`) ·
    /// Output·편집기 우클릭. 별도 알림은 만들지 않는다(사용자 11-12).
    /// AI 도구의 숨김 실행에서 난 오류 · AI가 꺼져 있을 때는 아무것도 하지 않는다.
    pub(crate) fn ai_note_error(&mut self, stmt: &str, message: &str, line: usize) {
        if !self.ai_enabled() || self.ai.pending.as_ref().is_some_and(|p| p.silent) {
            return;
        }
        if stmt.trim().is_empty() {
            return;
        }
        self.ai.last_error = Some((stmt.to_string(), message.to_string(), line));
    }

    /// ★ 오류 → AI(알림 버튼 · Output 우클릭 · 팔레트 `ai.fix_error`): 마지막 오류의 문장·메시지·줄을 붙여 수정을 요청한다.
    pub(crate) fn ai_fix_last_error(&mut self) {
        let Some((stmt, message, line)) = self.ai.last_error.clone() else {
            self.sess.status = t(Msg::AiNoLastError).into();
            self.redraw();
            return;
        };
        self.ai_target_editor();
        self.ai_bring_pane();
        let prompt = self.ai_prompt_of("fix");
        let err = tf(Msg::AiErrorAt, &[&line.to_string(), message.trim()]);
        self.ai_send(&format!("{prompt}\n{err}\n\n```sql\n{stmt}\n```"));
    }

    /// ★ AI → 편집기(코드 블록 [교체]): 설정 `ai.replace_target`대로 선택(우선) 또는 커서 문장을 블록 글로 바꾼다 ·
    /// 대상이 없으면 커서에 넣는다. 되돌리기 가능.
    pub(crate) fn ai_replace_code(&mut self, code: &str) {
        self.ai_target_editor();
        let prefer_sel = self.settings.get("ai.replace_target") != Some("statement");
        match self.ai_editor_target(prefer_sel) {
            Some((from, to, _, a, b)) => {
                let mut inv = Invalidations::default();
                let text = code.trim_end_matches('\n');
                self.ed_mut().replace_range(from, to, text, &mut inv);
                self.sess.status = tf(Msg::AiReplaced, &[&a.to_string(), &b.to_string()]);
                self.ai.side_focus = false;
                self.ai_view.set_focused(false);
                self.set_focus(Focus::Editor);
                self.redraw();
            }
            None => self.ai_insert_editor(code),
        }
    }

    /// ★ 답의 줄 번호 클릭 → 편집기 그 줄로(캐럿 이동 + 스크롤 · 포커스는 편집기).
    pub(crate) fn ai_goto_line(&mut self, n: usize) {
        self.ai_target_editor();
        self.ed_mut().goto_line(n.max(1));
        self.ai.side_focus = false;
        self.ai_view.set_focused(false);
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// ★ 그리드 → AI(우클릭 "고급 복사 ▸ AI에 첨부"): 선택 영역(마크다운 표)을 첨부 칩으로 — 행 수는 `ai.max_rows`까지 ·
    /// 비밀은 가린다 · 전송은 사용자가 질문을 적어 직접 한다(행 데이터를 붙인 것 = 이 세션의 행 데이터 동의).
    pub(crate) fn ai_attach_grid(&mut self, text: &str) {
        if text.trim().is_empty() {
            self.sess.status = t(Msg::AiGridEmpty).into();
            return;
        }
        let max = self.settings.int("ai.max_rows").max(1) as usize;
        let all: Vec<&str> = text.lines().collect();
        // 마크다운 표 = 머리 줄 + 구분 줄 + 행.
        let total = all.len().saturating_sub(2);
        let mut body = all
            .iter()
            .take(max + 2)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if total > max {
            body.push_str(&format!("\n… ({total} rows selected, {max} attached)"));
        }
        let (body, hidden) = crate::ai::redact_count(&body);
        self.ai_bring_pane();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        let n = total.min(max);
        if let Some(s) = self.ai_cur_mut() {
            s.rows_allowed = true;
            s.attachments.push(Attachment::Text {
                label: format!("grid ({n} row(s))"),
                body,
            });
        }
        self.ai.status = if hidden > 0 {
            tf(Msg::AiSecretMasked, &[&hidden.to_string()])
        } else {
            t(Msg::AiAttached).to_string()
        };
        self.ai_sync_panel();
        self.redraw();
    }

    /// 도구 `edit_editor`의 계획(줄 범위 = 인자 → 선택이 걸친 줄 → 커서 문장 · 새 글 = `replace`의 text 또는 줄 연산).
    pub(crate) fn ai_edit_plan(&self, input: &str) -> Result<EditPlan, String> {
        let v = json_parse(input).unwrap_or(Json::Obj(Vec::new()));
        let g = |k: &str| crate::ai::jstr(&v, k);
        let gi = |k: &str| match crate::ai::jget(&v, k) {
            Some(Json::Num(n)) => *n as i64,
            Some(Json::Str(s)) => s.parse().unwrap_or(0),
            _ => 0,
        };
        let op = g("op");
        let full = self.editors.cur().text();
        let lines: Vec<&str> = full.split('\n').collect();
        let n = lines.len();
        let (mut a, mut b) = (gi("from_line"), gi("to_line"));
        if a <= 0 {
            match self.ai_editor_target(true) {
                Some((_, _, _, la, lb)) => {
                    a = la as i64;
                    b = lb as i64;
                }
                None => return Err("no selection and no statement at the caret".into()),
            }
        }
        if b <= 0 {
            b = a;
        }
        if a <= 0 || a > b || b as usize > n {
            return Err(format!(
                "bad line range {a}..{b} (the editor has {n} lines)"
            ));
        }
        let (a, b) = (a as usize, b as usize);
        let start: usize = lines[..a - 1].iter().map(|l| l.chars().count() + 1).sum();
        let len: usize = lines[a - 1..b]
            .iter()
            .map(|l| l.chars().count() + 1)
            .sum::<usize>()
            - 1;
        let new = match op.as_str() {
            "select" => None,
            "replace" => Some(g("text").trim_end_matches('\n').to_string()),
            other => Some(
                crate::ai::tools::lines_op(&lines[a - 1..b], other)
                    .ok_or_else(|| format!("unknown op {other}"))?
                    .join("\n"),
            ),
        };
        Ok(EditPlan {
            start,
            len,
            a,
            b,
            op,
            new,
        })
    }

    /// ★ 변경 미리보기(승인 카드 본문 · 설정 `ai.diff_context` 0 = 끔): 편집기를 바꾸는 도구면 전후 줄 글 — 아니면 None.
    pub(crate) fn ai_edit_preview(&self, tool: &str, input: &str) -> Option<String> {
        let ctx = self.settings.int("ai.diff_context").clamp(0, 20) as usize;
        if ctx == 0 || self.ai_view_active() {
            return None;
        }
        let (head, cur, new) = match tool {
            "edit_editor" => {
                let p = self.ai_edit_plan(input).ok()?;
                let new = p.new.clone()?;
                // 편집기 전체 글에 계획을 적용한 글과 견준다(줄 번호 = 편집기 기준 · 앞뒤 문맥 줄 포함).
                let full = self.editors.cur().text();
                let chars: Vec<char> = full.chars().collect();
                let end = (p.start + p.len).min(chars.len());
                let after: String = chars[..p.start.min(chars.len())].iter().collect::<String>()
                    + &new
                    + &chars[end..].iter().collect::<String>();
                (
                    tf(
                        Msg::AiPreviewLines,
                        &[&p.op, &p.a.to_string(), &p.b.to_string()],
                    ),
                    full,
                    after,
                )
            }
            "set_editor_text" => {
                let v = json_parse(input).ok()?;
                let mode = crate::ai::jstr(&v, "mode");
                if mode == "insert" || mode == "new_tab" {
                    return None;
                }
                (
                    t(Msg::AiPreviewWhole).to_string(),
                    self.editors.cur().text(),
                    crate::ai::jstr(&v, "text"),
                )
            }
            _ => return None,
        };
        let body = crate::ai::diff::preview(&cur, &new, ctx, 80);
        if body.is_empty() {
            return Some(format!("{head}\n{}", t(Msg::AiPreviewSame)));
        }
        Some(format!("{head}\n{}", crate::ai::redact(&body)))
    }

    /// 층 이름(도구 글).
    fn ai_layer_text(l: nsql_script::Layer) -> &'static str {
        match l {
            nsql_script::Layer::Local => "tab",
            nsql_script::Layer::Shared => "shared",
            nsql_script::Layer::Global => "global",
            nsql_script::Layer::Fixed => "profile",
        }
    }

    /// ★ 도구 `list_vars`(사용자 11-12 "AI 창에서 변수 목록을 조회"): 변수 창과 같은 행(비밀은 가린 값) — 탭 구분 글.
    pub(crate) fn ai_vars_list(&self) -> String {
        let rows = self.vars_rows();
        if rows.is_empty() {
            return "(no variables in this tab)".into();
        }
        let mut out = String::from("name\ttype\tlayer\tvalue\tformula\tnote\n");
        for r in rows {
            let mut note: Vec<&str> = Vec::new();
            if r.stale {
                note.push("stale (recalculated on next use)");
            }
            if r.shadowed {
                note.push("shadowed by a nearer layer");
            }
            if r.secret {
                note.push("secret");
            }
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                r.name,
                r.ty,
                Self::ai_layer_text(r.layer),
                r.value,
                r.formula,
                note.join("; ")
            ));
        }
        out
    }

    /// ★ 도구 `get_var`: 한 변수의 종류(값/수식) · 수식 · 수식이 기대는 변수와 그 값 · 지금 값(가려지지 않은 가장 가까운 층).
    pub(crate) fn ai_var_get(&self, name: &str) -> Result<String, String> {
        let key = name.trim().trim_start_matches(':');
        let rows = self.vars_rows();
        let find = |n: &str| {
            rows.iter()
                .filter(|r| r.name.eq_ignore_ascii_case(n))
                .min_by_key(|r| r.shadowed)
        };
        let Some(r) = find(key) else {
            return Err(format!(
                "no variable named {key} in this tab (use list_vars)"
            ));
        };
        let mut out = format!(
            "name: {}\ntype: {}\nlayer: {}\nkind: {}\ncurrent value: {}\n",
            r.name,
            r.ty,
            Self::ai_layer_text(r.layer),
            if r.formula.is_empty() {
                "value"
            } else {
                "formula"
            },
            r.value
        );
        if r.stale {
            out.push_str("note: stale - the value is recalculated the next time it is used\n");
        }
        if !r.formula.is_empty() {
            out.push_str(&format!("formula: {}\n", r.formula));
            // 수식 안의 `:이름` = 의존 변수.
            let mut deps: Vec<String> = Vec::new();
            let chars: Vec<char> = r.formula.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == ':'
                    && chars
                        .get(i + 1)
                        .is_some_and(|c| c.is_alphabetic() || *c == '_')
                {
                    let mut j = i + 1;
                    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    let d: String = chars[i + 1..j].iter().collect();
                    if !deps.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
                        deps.push(d);
                    }
                    i = j;
                } else {
                    i += 1;
                }
            }
            for d in deps {
                match find(&d) {
                    Some(x) => out.push_str(&format!("depends on: {} = {}\n", x.name, x.value)),
                    None => out.push_str(&format!("depends on: {d} (not set)\n")),
                }
            }
        }
        Ok(out)
    }

    /// ★ 도구 `set_var`: 변수 창에서 값을 고친 것과 같은 길(`vars_apply` · DB로 가는 것 없음) → 바뀐 뒤의 상태를 돌려준다.
    pub(crate) fn ai_var_set(
        &mut self,
        name: &str,
        value: &str,
        null: bool,
    ) -> Result<String, String> {
        let key = name.trim().trim_start_matches(':').to_string();
        if key.is_empty() {
            return Err("empty variable name".into());
        }
        if self.ai_view_active() {
            return Err("the chat tab is active; switch to a SQL editor tab first".into());
        }
        let action = if null {
            vars_win::VarsWinAction::SetNull(key.clone())
        } else {
            vars_win::VarsWinAction::Set(key.clone(), value.to_string())
        };
        self.vars_apply(action);
        self.redraw();
        match self.ai_var_get(&key) {
            Ok(now) => Ok(format!("set.\n{now}")),
            // 치환 변수(&) · 프로필 층 거절 등은 상태줄 글로 알린다.
            Err(_) => Ok(format!("requested: {}", self.sess.status)),
        }
    }
}
