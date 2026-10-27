//! App — ★ **Insert Sequence** 세션([docs/124 §3](../../../../docs/124-insert-sequence.md) · 사용자 10-27): 팔레트 프롬프트 `seq`에 표현식을
//! 치는 동안 **본문의 캐럿 자리에 실제로 넣어 보여 주고**(VS Code Insert Numbers처럼), Enter = 확정 · Esc/바깥 클릭 = 원문 복원.
//!
//! - 세션 = 시작 때의 구간 목록(`regions()` · 오름차순 · 빈 캐럿 포함)과 원문 · 지금 들어가 있는 글 — 매번 `replace_many`로 직전 삽입분을 새 값으로
//!   **교체**한다(좌표 = 원 구간 + 앞선 길이 변화 누적) · 되돌리기 한 단계(`begin_group`/`end_group`).
//! - 오류 = 미리보기 행 첫 줄에 원인(경고색) + 본문은 공백 · `?`만 치면 미리보기 자리 = 치트 시트.

use crate::*;

pub(crate) struct SeqSession {
    /// 시작한 편집기 탭(다른 탭이면 본문을 건드리지 않는다).
    tab: u64,
    /// 원 구간(원 좌표 · 오름차순).
    regions: Vec<(usize, usize)>,
    /// 원 구간의 글(취소 때 복원).
    orig: Vec<String>,
    /// 지금 본문에 들어가 있는 글(구간마다).
    cur: Vec<String>,
    /// 마지막으로 적용한 질의(같으면 다시 계산하지 않는다).
    last_query: Option<String>,
    /// 마지막 계산의 오류(있으면 본문 = 공백).
    error: Option<String>,
    /// 마지막 값(미리보기 행 · 덤프).
    values: Vec<String>,
}

/// 미리보기 행 수(팔레트 행 상한 안에서 · 마지막 한 줄은 "…외 n개").
const PREVIEW_ROWS: usize = 11;

impl App {
    /// `Edit ▸ Insert Sequence…` · Ctrl+Alt+N: 세션 시작 + 프롬프트 열기 + 기본 표현식 즉시 미리보기.
    pub(crate) fn insseq_open(&mut self) {
        if self.seq.is_some() {
            return;
        }
        let tab = self.editors.active_id();
        let (regions, orig) = {
            let tb = self.editors.cur();
            let regions = tb.regions();
            let orig: Vec<String> = regions
                .iter()
                .map(|&(a, b)| tb.buf().slice_string(a, b))
                .collect();
            (regions, orig)
        };
        self.ed_mut().begin_undo_group();
        self.seq = Some(SeqSession {
            tab,
            regions,
            cur: orig.clone(),
            orig,
            last_query: None,
            error: None,
            values: Vec::new(),
        });
        let ph = self.insseq_default();
        self.palette
            .open_prompt_live("seq", &ph, &tf(Msg::SeqHint, &[&ph]), "");
        self.insseq_update("");
        self.redraw();
    }

    /// 기본 표현식(설정 `editor.seq_default` · 비면 엔진 기본).
    fn insseq_default(&self) -> String {
        let v = self.settings.get("editor.seq_default").unwrap_or("").trim();
        if v.is_empty() {
            insseq::DEFAULT_EXPR.to_string()
        } else {
            v.to_string()
        }
    }

    /// 질의가 바뀌었다(팔레트 입력란 · 기동 명령) — 계산 → 본문 교체 → 미리보기 행.
    pub(crate) fn insseq_update(&mut self, query: &str) {
        let Some(s) = self.seq.as_ref() else {
            return;
        };
        if s.last_query.as_deref() == Some(query) {
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
        let (values, error) = if help {
            (s.values.clone(), s.error.clone())
        } else {
            match insseq::parse(&expr).and_then(|p| p.values(n)) {
                Ok(v) => (v, None),
                Err(e) => (vec![String::new(); n], Some(seq_err_text(&e))),
            }
        };
        if !help {
            self.insseq_apply(&values);
        }
        if let Some(s) = self.seq.as_mut() {
            s.values = values;
            s.error = error;
            s.last_query = Some(query.to_string());
        }
        let rows = self.insseq_rows(help);
        self.palette.set_prompt_rows(rows);
        self.redraw();
    }

    /// 본문 교체 — 구간 i의 지금 자리 = 원 시작 + 앞선 길이 변화 누적 · 길이 = 지금 들어간 글의 글자 수.
    fn insseq_apply(&mut self, texts: &[String]) {
        let Some(s) = self.seq.as_ref() else {
            return;
        };
        if self.editors.active_id() != s.tab {
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

    /// 미리보기 행(`L줄: 값` · 오류 첫 줄 · `?` = 치트 시트) — (글, 경고색).
    fn insseq_rows(&self, help: bool) -> Vec<(String, bool)> {
        let Some(s) = self.seq.as_ref() else {
            return Vec::new();
        };
        if help {
            let ko = nsql_i18n::current_lang() == nsql_i18n::Lang::Ko;
            return insseq::HELP
                .iter()
                .map(|(e, en, k)| (format!("{e}    {}", if ko { k } else { en }), false))
                .collect();
        }
        let mut rows = Vec::new();
        if let Some(e) = &s.error {
            rows.push((e.clone(), true));
        }
        let tb = self.editors.cur();
        let mut shift: isize = 0;
        for (i, &(a, b)) in s.regions.iter().enumerate() {
            if rows.len() >= PREVIEW_ROWS {
                rows.push((
                    tf(Msg::SeqMore, &[&(s.regions.len() - i).to_string()]),
                    false,
                ));
                break;
            }
            let from = (a as isize + shift) as usize;
            let line = tb.buf().line_of(from.min(tb.buf().len())) + 1;
            let v = s.values.get(i).cloned().unwrap_or_default();
            rows.push((format!("L{line}: {v}"), false));
            shift += s.cur[i].chars().count() as isize - (b - a) as isize;
        }
        rows
    }

    /// Enter — 지금 본문이 곧 결과. 캐럿을 각 값의 끝에(다중 캐럿 유지 · Sublime) · 되돌리기 묶음 닫기 · 상태줄.
    pub(crate) fn insseq_commit(&mut self, query: &str) {
        self.insseq_update(query);
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

    /// Esc · 바깥 클릭 · 탭 전환 — 원문 복원 + 구간·캐럿 복원.
    pub(crate) fn insseq_cancel(&mut self) {
        let Some(orig) = self.seq.as_ref().map(|s| s.orig.clone()) else {
            return;
        };
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

    /// 자체 시험 덤프(`seq.dump:<파일>`): 질의 · 오류 · 값 n개 · 본문.
    pub(crate) fn insseq_dump(&self, path: &str) {
        let mut out = String::new();
        match self.seq.as_ref() {
            Some(s) => {
                out.push_str(&format!(
                    "query={}\nerror={}\nn={}\n",
                    s.last_query.clone().unwrap_or_default(),
                    s.error.clone().unwrap_or_default(),
                    s.regions.len()
                ));
                for v in &s.values {
                    out.push_str(&format!("v:{v}\n"));
                }
            }
            None => out.push_str("query=\nerror=\nn=0\n"),
        }
        out.push_str("text:\n");
        out.push_str(&self.editors.cur().text());
        let _ = std::fs::write(path, out);
    }
}

/// 엔진 오류 → 사용자 문구.
fn seq_err_text(e: &insseq::SeqErr) -> String {
    use insseq::SeqErr as E;
    match e {
        E::Parse(s) => tf(Msg::SeqErrParse, &[s]),
        E::ProductNeedsLists => t(Msg::SeqErrProduct).into(),
        E::EmptyList => t(Msg::SeqErrEmptyList).into(),
        E::BaseNeedsInteger => t(Msg::SeqErrBase).into(),
        E::Overflow => t(Msg::SeqErrOverflow).into(),
        E::RangeTooLarge(n) => tf(Msg::SeqErrRange, &[&n.to_string()]),
        E::UnitNeedsTimeOrDate(c) => tf(Msg::SeqErrUnit, &[&c.to_string()]),
        E::PatternNeedsTimeOrDate => t(Msg::SeqErrPattern).into(),
        E::BadRepeat => t(Msg::SeqErrRepeat).into(),
        E::WidthTooLarge => t(Msg::SeqErrWidth).into(),
    }
}
