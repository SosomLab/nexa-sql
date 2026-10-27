//! App — ★ **프롬프트 변환 세션**([docs/124 §3·§7](../../../../docs/124-insert-sequence.md) · 148차 일반화 · 사용자 10-27): 확장이 선언한
//! 프롬프트(내장 Insert Sequence · wasm 메타 `prompts`)를 팔레트 라이브 프롬프트로 열고, 표현식을 치는 동안 **값 공급자(확장 `transform`)** 가
//! 준 값을 캐럿 자리에 실제로 넣어 보여 준다(VS Code Insert Numbers처럼) · Enter = 확정 · Esc/바깥 클릭 = 원문 복원.
//!
//! - 세션 = 시작 때의 구간 목록(`regions()` · 오름차순 · 빈 캐럿 포함)과 원문 · 지금 들어가 있는 글 — 매번 `replace_many`로 직전 삽입분을 새 값으로
//!   **교체**한다(좌표 = 원 구간 + 앞선 길이 변화 누적) · 되돌리기 한 단계(`begin_group`/`end_group`).
//! - 값은 확장이 **제안**만 한다(본문 쓰기 권한 0 · 적용·복원·되돌리기는 여기 = 87 §14 데이터 보호 정신) · 오류 = 미리보기 행 첫 줄에
//!   원인(경고색) + 본문은 공백 · `?`만 치면 미리보기 자리 = 치트 시트(프롬프트 메타 `help`).
//! - ★ 범용 축(사용자 10-27 "SDK는 범용 구조"): 단계 `phase`(`live` 키마다 · `commit` Enter · `cancel` Esc) · 요청에 `selection` · 응답의
//!   적용 방식 `apply`(`regions` 구간마다 값 · `text` 선택 전체를 글 하나로 · `none` 본문은 두고 행만).
//! - ★ 행 고르기(T-447 · 153차): 확장이 준 행에 `id`가 있으면 팔레트가 ↑↓/클릭으로 고르게 하고, Enter의 `pick`이 commit 요청에 실린다.
//! - 팔레트 프롬프트 id = `seq:<프롬프트 id>`([`PROMPT_PREFIX`] · input.rs · startup_cmd.rs가 접두로 알아본다).

use crate::*;
use extensions::{Prompt, TransformReq};

/// 미리보기 행 = (글, 경고색, 고를 수 있으면 id).
type Row = (String, bool, Option<String>);

pub(crate) struct SeqSession {
    /// 시작한 편집기 탭(다른 탭이면 본문을 건드리지 않는다).
    tab: u64,
    /// 프롬프트 선언(메타 사본) + 소유 확장 id(변환 호출 대상).
    prompt: Prompt,
    ext_id: String,
    /// 원 구간(원 좌표 · 오름차순) · 시작 때의 줄 번호(1부터 · 변환 요청에 실어 보낸다).
    regions: Vec<(usize, usize)>,
    lines: Vec<usize>,
    /// 원 구간의 글(취소 때 복원).
    orig: Vec<String>,
    /// 지금 본문에 들어가 있는 글(구간마다).
    cur: Vec<String>,
    /// 활성 선택의 글(첫 구간 시작 ~ 마지막 구간 끝 · 상한).
    selection: String,
    /// 적용 방식(`regions` · `text` · `none`) — 첫 응답이 정하고 세션 동안 바꾸지 못한다.
    mode: String,
    /// 마지막으로 적용한 질의(같으면 다시 계산하지 않는다).
    last_query: Option<String>,
    /// 마지막 계산의 오류(있으면 본문 = 공백).
    error: Option<String>,
    /// 마지막 값(미리보기 행 · 덤프).
    values: Vec<String>,
    /// 확장이 준 미리보기 행(비면 호스트가 `L줄: 값`으로 만든다).
    rows: Vec<Row>,
}

/// 미리보기 행 수(팔레트 행 상한 안에서 · 마지막 한 줄은 "…외 n개").
const PREVIEW_ROWS: usize = 11;
/// 변환 요청에 실어 보내는 구간 원문의 상한(`n`이 진짜 수 · wasm 입력 버퍼 상한 안에서).
const REQ_REGIONS_CAP: usize = 256;
/// 요청의 `selection` 글자 상한.
const REQ_SELECTION_CAP: usize = 64 * 1024;
/// 팔레트 프롬프트 id 접두(뒤 = 프롬프트 id).
pub(crate) const PROMPT_PREFIX: &str = "seq:";

impl App {
    /// 프롬프트 명령(`edit.insert_sequence` · wasm `prompts[].id` · Ctrl+Alt+N): 세션 시작 + 라이브 프롬프트 + 기본 표현식 즉시 미리보기.
    /// 소유 확장이 꺼져 있으면 상태줄 안내만.
    pub(crate) fn prompt_open(&mut self, id: &str) {
        if self.seq.is_some() {
            return;
        }
        let disabled = self.ext_disabled();
        let Some((ext_id, prompt)) = self.extensions.prompt_owner(id, &disabled) else {
            if let Some(name) = self.extensions.prompt_ext_name(id) {
                self.sess.status = tf(Msg::StPromptExtOff, &[&name]);
                self.redraw();
            }
            return;
        };
        let tab = self.editors.active_id();
        let (regions, lines, orig, selection) = {
            let tb = self.editors.cur();
            let regions = tb.regions();
            let lines: Vec<usize> = regions
                .iter()
                .map(|&(a, _)| tb.buf().line_of(a.min(tb.buf().len())) + 1)
                .collect();
            let orig: Vec<String> = regions
                .iter()
                .map(|&(a, b)| tb.buf().slice_string(a, b))
                .collect();
            let selection = match (regions.first(), regions.last()) {
                (Some(&(a, _)), Some(&(_, b))) if b > a && b - a <= REQ_SELECTION_CAP => {
                    tb.buf().slice_string(a, b)
                }
                _ => String::new(),
            };
            (regions, lines, orig, selection)
        };
        self.ed_mut().begin_undo_group();
        self.seq = Some(SeqSession {
            tab,
            prompt,
            ext_id,
            regions,
            lines,
            cur: orig.clone(),
            orig,
            selection,
            mode: "regions".to_string(),
            last_query: None,
            error: None,
            values: Vec::new(),
            rows: Vec::new(),
        });
        let ph = self.insseq_default();
        let hint = self
            .seq
            .as_ref()
            .map(|s| s.prompt.hint.text().replace("{0}", &ph))
            .unwrap_or_default();
        self.palette
            .open_prompt_live(&format!("{PROMPT_PREFIX}{id}"), &ph, &hint, "");
        self.insseq_update("");
        self.redraw();
    }

    /// 기본 표현식 = 프롬프트의 `placeholder_key` 설정 값(비면 메타 `placeholder` · 내장 = `ext.insert_sequence.default`).
    fn insseq_default(&self) -> String {
        let Some(s) = self.seq.as_ref() else {
            return String::new();
        };
        let v = s
            .prompt
            .placeholder_key
            .as_deref()
            .and_then(|k| self.settings.get(k))
            .map(str::trim)
            .unwrap_or("");
        if v.is_empty() {
            s.prompt.placeholder.clone()
        } else {
            v.to_string()
        }
    }

    /// 질의가 바뀌었다(팔레트 입력란 · 기동 명령) — 확장 변환(`live`) → 본문 교체 → 미리보기 행.
    pub(crate) fn insseq_update(&mut self, query: &str) {
        self.insseq_run(query, "live", false, None);
    }

    /// 변환 요청 한 벌(단계 · 질의 · 상황 · 고른 행).
    fn insseq_request(
        &self,
        s: &SeqSession,
        text: String,
        phase: &str,
        pick: Option<String>,
    ) -> TransformReq {
        TransformReq {
            id: s.prompt.id.clone(),
            text,
            n: s.regions.len(),
            regions: s
                .lines
                .iter()
                .zip(&s.orig)
                .take(REQ_REGIONS_CAP)
                .map(|(l, t)| (*l, t.clone()))
                .collect(),
            lang: if nsql_i18n::current_lang() == nsql_i18n::Lang::Ko {
                "ko".to_string()
            } else {
                "en".to_string()
            },
            doc_path: self
                .editors
                .active_path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            phase: phase.to_string(),
            selection: s.selection.clone(),
            pick,
        }
    }

    /// 변환 한 번(`live`/`commit`) → 적용 방식에 따라 본문 교체 → 상태 갱신 → 미리보기 행. `force` = 같은 질의라도 다시(확정 단계).
    fn insseq_run(&mut self, query: &str, phase: &str, force: bool, pick: Option<String>) {
        let Some(s) = self.seq.as_ref() else {
            return;
        };
        if !force && s.last_query.as_deref() == Some(query) {
            return;
        }
        let n = s.regions.len();
        let expr = if query.trim().is_empty() {
            self.insseq_default()
        } else {
            query.to_string()
        };
        // `?` = 도움말 보기(본문은 직전 결과 그대로).
        let help = query.trim() == "?";
        // (값, 오류, 행, 적용할 값이 있는가, 글 모드로 다시 잡아야 하는가)
        let (values, error, rows, apply, rebase) = if help {
            (
                s.values.clone(),
                s.error.clone(),
                s.rows.clone(),
                false,
                false,
            )
        } else {
            let req = self.insseq_request(s, expr, phase, pick);
            match self.extensions.transform(&s.ext_id, &req, &self.settings) {
                Ok(out) => {
                    let rows: Vec<Row> = out
                        .rows
                        .into_iter()
                        .map(|r| (r.text, r.warn, r.id))
                        .collect();
                    if let Some(e) = out.error {
                        (vec![String::new(); n], Some(e), rows, true, false)
                    } else if out.apply == "none" {
                        // 본문은 두고 행만(정보 프롬프트 · 고르기 목록).
                        (s.values.clone(), None, rows, false, false)
                    } else if out.apply == "text" {
                        let pristine = s.mode == "regions" && s.cur == s.orig;
                        if s.mode == "text" || pristine {
                            (vec![out.text], None, rows, true, s.mode != "text")
                        } else {
                            (
                                vec![String::new(); n],
                                Some(
                                    "transform: apply mode changed during the session".to_string(),
                                ),
                                rows,
                                true,
                                false,
                            )
                        }
                    } else {
                        let mut v = out.values;
                        v.resize(n, String::new());
                        (v, None, rows, true, false)
                    }
                }
                Err(e) => (vec![String::new(); n], Some(e), Vec::new(), true, false),
            }
        };
        // WASM 확장의 `nx_log` 줄·오류를 로그 창에(docs/75 §6) · 설정 덮어쓰기 요청 적용(148차).
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        self.ext_apply_setting_sets();
        if rebase {
            self.insseq_rebase_text();
        }
        if apply {
            self.insseq_apply(&values);
        }
        if let Some(s) = self.seq.as_mut() {
            s.values = values;
            s.error = error;
            s.rows = rows;
            s.last_query = Some(query.to_string());
        }
        let rows = self.insseq_rows(help);
        self.palette.set_prompt_choices(rows);
        self.redraw();
    }

    /// `apply = text`: 구간 여럿을 **하나**(첫 시작 ~ 마지막 끝)로 다시 잡는다(본문이 아직 원문일 때만 부른다).
    fn insseq_rebase_text(&mut self) {
        let Some(s) = self.seq.as_mut() else {
            return;
        };
        let (Some(&(a, _)), Some(&(_, b))) = (s.regions.first(), s.regions.last()) else {
            return;
        };
        let line = s.lines.first().copied().unwrap_or(1);
        let whole = self.editors.cur().buf().slice_string(a, b);
        s.regions = vec![(a, b)];
        s.lines = vec![line];
        s.orig = vec![whole.clone()];
        s.cur = vec![whole];
        s.mode = "text".to_string();
    }

    /// 본문 교체 — 구간 i의 지금 자리 = 원 시작 + 앞선 길이 변화 누적 · 길이 = 지금 들어간 글의 글자 수.
    fn insseq_apply(&mut self, texts: &[String]) {
        let Some(s) = self.seq.as_ref() else {
            return;
        };
        if self.editors.active_id() != s.tab || texts.len() != s.regions.len() {
            return;
        }
        let mut edits: Vec<(usize, usize, &str)> = Vec::with_capacity(texts.len());
        let mut shift: isize = 0;
        for (i, &(a, b)) in s.regions.iter().enumerate() {
            let cur_len = s.cur[i].chars().count();
            let orig_len = b - a;
            let from = (a as isize + shift) as usize;
            let to = from + cur_len;
            if s.cur[i] != texts[i] {
                edits.push((from, to, texts[i].as_str()));
            }
            shift += cur_len as isize - orig_len as isize;
        }
        if !edits.is_empty() {
            let mut inv = Invalidations::default();
            self.ed_mut().replace_many(&edits, &mut inv);
        }
        if let Some(s) = self.seq.as_mut() {
            s.cur = texts.to_vec();
        }
    }

    /// 미리보기 행(`L줄: 값` · 오류 첫 줄 · `?` = 치트 시트 · 확장이 준 행이 있으면 그것) — (글, 경고색, id).
    fn insseq_rows(&self, help: bool) -> Vec<Row> {
        let Some(s) = self.seq.as_ref() else {
            return Vec::new();
        };
        if help {
            return s
                .prompt
                .help
                .iter()
                .map(|(e, l)| (format!("{e}    {}", l.text()), false, None))
                .collect();
        }
        let mut rows: Vec<Row> = Vec::new();
        if let Some(e) = &s.error {
            rows.push((e.clone(), true, None));
        }
        if !s.rows.is_empty() {
            rows.extend(s.rows.iter().take(PREVIEW_ROWS).cloned());
            return rows;
        }
        let tb = self.editors.cur();
        let mut shift: isize = 0;
        for (i, &(a, b)) in s.regions.iter().enumerate() {
            if rows.len() >= PREVIEW_ROWS {
                rows.push((
                    tf(Msg::SeqMore, &[&(s.regions.len() - i).to_string()]),
                    false,
                    None,
                ));
                break;
            }
            let from = (a as isize + shift) as usize;
            let line = tb.buf().line_of(from.min(tb.buf().len())) + 1;
            let v = s.values.get(i).cloned().unwrap_or_default();
            rows.push((format!("L{line}: {v}"), false, None));
            shift += s.cur[i].chars().count() as isize - (b - a) as isize;
        }
        rows
    }

    /// Enter — `commit` 단계로 한 번 더 묻고(고른 행 `pick` 포함 · 확정 때만 할 일이 있는 플러그인) 지금 본문이 곧 결과. 캐럿을 각 값의
    /// 끝에(다중 캐럿 유지 · Sublime) · 되돌리기 묶음 닫기 · 상태줄.
    pub(crate) fn insseq_commit(&mut self, query: &str, pick: Option<String>) {
        self.insseq_run(query, "commit", true, pick);
        let Some(s) = self.seq.take() else {
            return;
        };
        if self.editors.active_id() == s.tab {
            let mut ends = Vec::with_capacity(s.regions.len());
            let mut shift: isize = 0;
            for (i, &(a, b)) in s.regions.iter().enumerate() {
                let from = (a as isize + shift) as usize;
                let len = s.cur[i].chars().count();
                ends.push(from + len);
                shift += len as isize - (b - a) as isize;
            }
            let regions: Vec<(usize, usize)> = ends.iter().map(|&e| (e, e)).collect();
            let tb = self.ed_mut();
            tb.set_regions_pub(&regions);
            tb.end_undo_group();
        }
        self.sess.status = tf(Msg::StSeqInserted, &[&s.regions.len().to_string()]);
        self.redraw();
    }

    /// Esc · 바깥 클릭 · 탭 전환 — `cancel` 단계 알림(결과는 쓰지 않는다) + 원문 복원 + 구간·캐럿 복원.
    pub(crate) fn insseq_cancel(&mut self) {
        let Some(orig) = self.seq.as_ref().map(|s| s.orig.clone()) else {
            return;
        };
        if let Some(s) = self.seq.as_ref() {
            let req =
                self.insseq_request(s, s.last_query.clone().unwrap_or_default(), "cancel", None);
            let _ = self.extensions.transform(&s.ext_id, &req, &self.settings);
        }
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        self.ext_apply_setting_sets();
        self.insseq_apply(&orig);
        let Some(s) = self.seq.take() else {
            return;
        };
        if self.editors.active_id() == s.tab {
            let tb = self.ed_mut();
            tb.set_regions_pub(&s.regions);
            tb.end_undo_group();
        }
        self.sess.status = t(Msg::StSeqCancelled).into();
        self.redraw();
    }

    pub(crate) fn insseq_active(&self) -> bool {
        self.seq.is_some()
    }

    /// 열린 팔레트 프롬프트가 이 세션의 것인가(`seq:` 접두 · 키 입력마다 갱신할지 판단).
    pub(crate) fn insseq_live_prompt(&self) -> bool {
        self.seq.is_some()
            && self
                .palette
                .prompt_id()
                .is_some_and(|p| p.starts_with(PROMPT_PREFIX))
    }

    /// 자체 시험 덤프(`seq.dump:<파일>`): 프롬프트·확장·방식 · 질의 · 오류 · 값 n개 · 고를 수 있는 행 · 본문.
    pub(crate) fn insseq_dump(&self, path: &str) {
        let mut out = String::new();
        match self.seq.as_ref() {
            Some(s) => {
                out.push_str(&format!(
                    "prompt={}\next={}\nmode={}\nquery={}\nerror={}\nn={}\npick={}\n",
                    s.prompt.id,
                    s.ext_id,
                    s.mode,
                    s.last_query.clone().unwrap_or_default(),
                    s.error.clone().unwrap_or_default(),
                    s.regions.len(),
                    self.palette.prompt_pick().unwrap_or_default()
                ));
                for v in &s.values {
                    out.push_str(&format!("v:{v}\n"));
                }
                for (text, _, id) in &s.rows {
                    if let Some(id) = id {
                        out.push_str(&format!("row:{id}:{text}\n"));
                    }
                }
            }
            None => out.push_str("prompt=\next=\nmode=\nquery=\nerror=\nn=0\npick=\n"),
        }
        out.push_str("text:\n");
        out.push_str(&self.editors.cur().text());
        let _ = std::fs::write(path, out);
    }
}
