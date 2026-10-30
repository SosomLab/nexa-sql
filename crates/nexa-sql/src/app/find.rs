//! App — 찾기·바꾸기 · 파일 검색(docs/36).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 편집기 본문에서 질의 일치 위치(문자 인덱스 · 대소문자 옵션) 전부.
    /// 찾기 조건 → 정규식(D-76): 정규식 모드가 아니어도 같은 엔진(글자 그대로 이스케이프)을 쓴다 — 단어 단위·대소문자 규칙 한 곳.
    /// ★ T-266 Output 안 찾기(10-30): 찾기 막대의 대상 상자 — 찾기를 **Output 탭에 포커스가 있을 때 열었으면**(`find_on_output`) Output
    ///   본문(읽기 전용 · 표식·선택·스크롤만 · 바꾸기는 읽기 전용이라 무변경) · 그 밖 = 활성 편집기. Output 탭이 닫히거나 바뀌면 편집기로 돌아간다.
    pub(crate) fn find_box(&self) -> &TextBox {
        if self.find_on_output {
            if let Some(o) = self.panel.output_active_ref() {
                return o.body();
            }
        }
        self.editors.cur()
    }

    pub(crate) fn find_box_mut(&mut self) -> &mut TextBox {
        // 빌림 규칙: 가지 안에서 빌린 참조를 돌려주고 다른 가지에서 `self`를 또 쓰면 NLL이 거부한다 → 가지 안에서 전부 끝낸다.
        if self.find_on_output && self.panel.output_active_ref().is_some() {
            return match self.panel.output_active_mut() {
                Some(o) => o.body_mut(),
                None => unreachable!("output_active_ref was Some"),
            };
        }
        self.ed_mut()
    }

    fn find_rx(&mut self) -> Option<fancy_regex::Regex> {
        let q = self.find.query();
        if q.is_empty() {
            return None;
        }
        match rx::compile(
            &q,
            self.find.regex(),
            self.find.case_sensitive(),
            self.find.whole_word(),
        ) {
            Ok(r) => Some(r),
            Err(e) => {
                self.find.set_status(tf(Msg::StFindBadRegex, &[&e]));
                self.find.set_has_matches(false);
                None
            }
        }
    }

    fn find_matches(&mut self) -> Vec<(usize, usize)> {
        let mut out = self.find_matches_all();
        // 선택 범위에서 찾기(≡ · Alt+L): 켤 때 잡은 범위 안의 일치만.
        if let Some((a, e)) = self.find_scope {
            out.retain(|(s, t)| *s >= a && *t <= e);
        }
        out
    }

    /// 본문 전체의 일치 구간(글자 인덱스). 보통 찾기(정규식 아님 · 질의에 줄바꿈 없음)는 **버퍼의 줄을 하나씩** 본다 —
    /// 종전에는 찾기 입력마다 본문 전체를 문자열 + 글자 배열로 다시 만들었다(65 MB 파일 = 임시 325 MB · T-142).
    fn find_matches_all(&mut self) -> Vec<(usize, usize)> {
        // ★ 정규식 모드 = fancy-regex(문자 인덱스로 변환) — 엔진이 문자열 하나를 요구한다.
        if self.find.regex() {
            let Some(r) = self.find_rx() else {
                return Vec::new();
            };
            let full = self.find_box_mut().text();
            return rx::find_all(&r, &full);
        }
        let q: Vec<char> = self.find.query().chars().collect();
        let (cs, ww) = (self.find.case_sensitive(), self.find.whole_word());
        let buf = self.find_box().buf();
        if q.is_empty() || q.len() > buf.len() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if q.contains(&'\n') {
            // 여러 줄 질의(드묾): 본문을 글자 배열로 떠서 종전 방식 그대로.
            let text: Vec<char> = buf.iter_from(0).collect();
            find_in_chars(&text, 0, &q, cs, ww, &mut out);
            return out;
        }
        let mut line: Vec<char> = Vec::new();
        for l in 0..buf.line_count() {
            let s = buf.line_text(l);
            if s.len() < q.len() {
                continue; // 바이트 수가 글자 수보다 작을 수는 없다 — 이 줄에는 들어갈 자리가 없다.
            }
            line.clear();
            line.extend(s.chars());
            find_in_chars(&line, buf.line_start(l), &q, cs, ww, &mut out);
        }
        out
    }

    /// ★ 찾기를 열 때의 편집기 선택을 적어 둔다(131차 · 사용자 10-19 이관) — 반환 = **여러 줄 선택이라 지켜야 하는가**
    ///   (씨앗으로 쓰지 않는 선택 = 열어도 캐럿·선택을 옮기지 않는다 · VS Code). 한 줄 선택도 범위 후보로는 보관(`always` 설정).
    pub(crate) fn find_note_open_selection(&mut self) -> bool {
        let ed = self.find_box_mut();
        let sel = ed.selection().filter(|(a, b)| b > a);
        let multi = sel.is_some_and(|(a, b)| selection_spans_lines(ed.buf(), a, b));
        self.find_pending_scope = sel;
        multi
    }

    /// 설정 `editor.find_auto_scope`(VS Code `autoFindInSelection`): never = 아무것도 · multiline = 여러 줄 선택이면 켬 ·
    /// always = 선택이 있으면 켬. 켜는 길은 ≡ 토글과 같은 `ScopeChanged`.
    pub(crate) fn find_auto_scope(&mut self) {
        let mode = self
            .settings
            .get("editor.find_auto_scope")
            .unwrap_or("never")
            .to_string();
        let Some((a, b)) = self.find_pending_scope else {
            return;
        };
        let multi = selection_spans_lines(self.find_box_mut().buf(), a, b);
        if auto_scope_on(&mode, multi) {
            self.find.set_in_selection(true);
            self.find_action(FindAction::ScopeChanged);
        }
    }

    /// 일치 표식·개수·버튼 상태만 갱신(캐럿·선택은 그대로) — 여러 줄 선택을 지키며 열 때.
    pub(crate) fn find_refresh(&mut self) {
        self.find_step_in(true, false, false);
    }

    /// 다음/이전 일치로 이동(순환) — `advance`면 현재 선택을 지나서, 아니면 캐럿부터.
    pub(crate) fn find_step(&mut self, forward: bool, advance: bool) {
        self.find_step_in(forward, advance, true);
    }

    /// `move_caret`가 거짓이면 일치를 고르되 선택은 옮기지 않는다(표식·"n/N"만).
    fn find_step_in(&mut self, forward: bool, advance: bool, move_caret: bool) {
        if !self.find.is_visible() {
            return;
        }
        let matches = self.find_matches();
        self.find.set_has_matches(!matches.is_empty());
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.find_box_mut().set_find_marks(Vec::new());
            self.redraw();
            return;
        }
        let sel = self.find_box_mut().selection();
        let caret = self.find_box_mut().caret();
        let idx = if forward {
            let from = match sel {
                Some((a, b)) if advance => {
                    if b > a {
                        a + 1
                    } else {
                        caret
                    }
                }
                Some((a, _)) => a,
                None => caret,
            };
            matches.iter().position(|(s, _)| *s >= from).unwrap_or(0)
        } else {
            let from = sel.map_or(caret, |(a, _)| a);
            matches
                .iter()
                .rposition(|(s, _)| *s < from)
                .unwrap_or(matches.len() - 1)
        };
        let (s, e) = matches[idx];
        if move_caret {
            let mut inv = Invalidations::default();
            self.find_box_mut().select_range(s, e, &mut inv);
            self.find_moved = true; // 검색을 수행했다(⑩) → Esc는 이 자리에 남는다.
        }
        self.find.set_status(tf(
            Msg::StFindCount,
            &[&(idx + 1).to_string(), &matches.len().to_string()],
        ));
        // 일치 전부 표시(T-73).
        self.find_box_mut().set_find_marks(matches);
        self.redraw();
    }

    /// 현재 선택이 일치면 바꾸고 다음으로.
    /// 일치 `(a, b)`에 넣을 치환문 — 정규식 모드면 `$1`/`${name}` 확장 · 아니면 글자 그대로.
    fn find_expansion(&mut self, a: usize) -> String {
        let repl = self.find.replacement();
        let full = self.find_box_mut().text();
        let base = if self.find.regex() {
            match self.find_rx() {
                Some(r) => rx::expand_at(&r, &full, a, &repl),
                None => repl,
            }
        } else {
            repl
        };
        if !self.find.preserve_case() {
            return base;
        }
        // 대소문자 보존(AB · Alt+A): 일치가 전부 대문자면 대문자로 · 첫 글자만 대문자면 첫 글자만 · 전부 소문자면 소문자로.
        let matches = self.find_matches();
        let Some(&(s, e)) = matches.iter().find(|(s, _)| *s == a) else {
            return base;
        };
        let matched: String = full.chars().skip(s).take(e - s).collect();
        preserve_case(&matched, &base)
    }

    fn find_replace_one(&mut self) {
        let matches = self.find_matches();
        if let Some((a, b)) = self.find_box_mut().selection() {
            if matches.contains(&(a, b)) {
                let repl = self.find_expansion(a);
                let mut inv = Invalidations::default();
                self.find_box_mut().replace_range(a, b, &repl, &mut inv);
            }
        }
        self.find_step(true, false);
    }

    fn find_replace_all(&mut self) {
        let matches = self.find_matches();
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.redraw();
            return;
        }
        // 뒤에서 앞으로(앞 인덱스가 안 밀리게) · 확장은 원본 본문 기준으로 먼저 계산.
        let repls: Vec<String> = matches
            .iter()
            .map(|(a, _)| self.find_expansion(*a))
            .collect();
        // ★ 한 번 훑어 전부 바꾼다 + 되돌리기 **한 단계**(종전 = 일치마다 `replace_range` — 3 MB 본문 2,000건이 15초 ·
        //   되돌리기도 2,000번 눌러야 했다 · docs/60).
        let mut inv = Invalidations::default();
        let edits: Vec<(usize, usize, &str)> = matches
            .iter()
            .zip(repls.iter())
            .map(|((a, b), r)| (*a, *b, r.as_str()))
            .collect();
        self.find_box_mut().replace_many(&edits, &mut inv);
        self.find
            .set_status(tf(Msg::StReplacedN, &[&matches.len().to_string()]));
        self.redraw();
    }

    pub(crate) fn find_action(&mut self, a: FindAction) {
        match a {
            FindAction::None => {}
            FindAction::Next => self.find_step(true, true),
            FindAction::Prev => self.find_step(false, true),
            FindAction::Changed => {
                // 펼침 토글은 패널 높이가 바뀐다 — 다시 배치.
                self.layout();
                self.find_step(true, false);
            }
            FindAction::Replace => self.find_replace_one(),
            FindAction::ReplaceAll => self.find_replace_all(),
            FindAction::ScopeChanged => {
                if self.find.in_selection() {
                    // ★ 131차: 지금 선택이 여러 줄이면 그것 · 아니면 열 때 보관한 선택(일치 이동으로 선택이 바뀌었어도) · 그것도 없으면 지금 선택.
                    let cur = self.find_box_mut().selection();
                    let cur_multi = cur.is_some_and(|(a, b)| {
                        b > a && selection_spans_lines(self.find_box_mut().buf(), a, b)
                    });
                    let pick = pick_find_scope(cur, cur_multi, self.find_pending_scope);
                    match pick {
                        Some((a, b)) if b > a => {
                            self.find_scope = Some((a, b));
                            self.find_box_mut().set_find_scope(Some((a, b)));
                            // 범위 안 첫 일치로(선택은 범위 표시가 대신한다).
                            let mut inv = Invalidations::default();
                            self.find_box_mut().select_range(a, a, &mut inv);
                        }
                        _ => {
                            self.find.set_in_selection(false);
                            self.sess.status = t(Msg::StFindNoSelection).into();
                        }
                    }
                } else {
                    self.find_scope = None;
                    self.find_box_mut().set_find_scope(None);
                }
                self.find_step(true, false);
            }
            FindAction::SelectAll => {
                let matches = self.find_matches();
                if matches.is_empty() {
                    self.find.set_status(t(Msg::StFindNone));
                } else {
                    let n = matches.len();
                    self.find_box_mut().set_regions_pub(&matches);
                    self.find
                        .set_status(tf(Msg::StSelections, &[&n.to_string()]));
                    self.set_focus(Focus::Editor);
                }
                self.redraw();
            }
            FindAction::Cancel => {
                // ★ Esc = 취소(131차 · 사용자 10-20): **검색을 한 번도 안 했으면** 열 때 자리로 캐럿·선택 복귀 · 검색으로 일치에
                //   갔으면(`find_moved`) 그 자리 유지(⑩) → 닫기와 같은 정리.
                let origin = self.find_origin.take();
                if let Some((caret, sel)) = origin.filter(|_| cancel_restores(self.find_moved)) {
                    let mut inv = Invalidations::default();
                    match sel {
                        Some((a, b)) => self.find_box_mut().select_range(a, b, &mut inv),
                        None => self.find_box_mut().select_range(caret, caret, &mut inv),
                    }
                }
                self.find_action(FindAction::Close);
            }
            FindAction::Close => {
                self.find.close();
                self.find_scope = None;
                self.find_pending_scope = None;
                self.find_origin = None;
                self.find_box_mut().set_find_scope(None);
                self.find_box_mut().set_find_marks(Vec::new());
                self.layout();
                self.set_focus(Focus::Editor);
                self.redraw();
            }
        }
    }

    /// 검색 시작 — 열린 탭 본문 · 활성 파일 폴더 · 설정을 모아 패널에 넘긴다.
    pub(crate) fn start_search(&mut self) {
        let mut excludes: Vec<String> = self
            .settings
            .get("search.excludes")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        // ★ D-260(사용자 10-07): Ctrl+P·프로젝트 필터의 제외 폴더(`project.exclude`)를 파일 검색에도 — gitignore 폴더 패턴 `이름/`
        //   (어느 깊이든 그 이름의 폴더) · 검색 로그엔 무시 규칙으로 걸러진 것과 같이 보인다.
        for n in
            super::goto::exclude_names(self.settings.get(super::goto::EXCLUDE_KEY).unwrap_or(""))
        {
            let pat = format!("{n}/");
            if !excludes.contains(&pat) {
                excludes.push(pat);
            }
        }
        // 범위 상자가 비었을 때의 기본 = 작업 모드별(사용자 09-23): 파일 모드 = 열린 파일만 · 폴더 모드 = + 그 폴더 이하 ·
        //   프로젝트 모드 = + 프로젝트 폴더(파일이 있는 곳) + 프로젝트에 추가한 폴더.
        let mut default_roots: Vec<PathBuf> = Vec::new();
        match project::WorkMode::of(self.project.path.as_deref(), self.arg_folder.as_deref()) {
            project::WorkMode::File => {}
            project::WorkMode::Folder(d) => default_roots.push(d),
            project::WorkMode::Project(p) => {
                if let Some(d) = p.parent() {
                    default_roots.push(d.to_path_buf());
                }
                for f in &self.project.folders {
                    if !default_roots.iter().any(|r| r == f) {
                        default_roots.push(f.clone());
                    }
                }
            }
        }
        // ★ D-260(협업 bin49 f): 기본 루트 **자체**가 제외 이름(예 `target`)을 품으면 뺀다 — gitignore 패턴은 루트 아래만 보기 때문.
        //   사용자가 범위 상자에 직접 적은 루트는 그대로(명시 = 뜻).
        {
            let excl = super::goto::exclude_names(
                self.settings.get(super::goto::EXCLUDE_KEY).unwrap_or(""),
            );
            default_roots.retain(|r| {
                !r.components().any(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    excl.iter().any(|n| *n == s)
                })
            });
        }
        let ctx = SearchCtx {
            tabs: self.editors.tab_texts(),
            current_dir: self
                .editors
                .active_path()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            default_roots,
            max_file_kb: self.settings.int("search.max_file_kb").max(0) as usize,
            threads: self.settings.int("search.threads").clamp(0, 16) as usize,
            gitignore: self.settings.flag("search.gitignore"),
            excludes,
        };
        self.search.start(ctx);
        self.redraw();
    }

    /// 결과 행 → 그 탭/파일을 열고 줄로 이동 · 일치 구간 선택.
    /// 파일은 **프로젝트 탐색기와 같은 규칙**(사용자 09-23): 한 번 클릭 = 미리보기 탭(포커스는 패널에) · 더블클릭/Enter = 정식 탭 + 편집기 포커스.
    pub(crate) fn open_search_result(&mut self, req: search_panel::OpenReq) {
        match (req.tab, &req.path) {
            (Some(id), _) => self.editors.switch_to_id(id),
            (None, Some(p)) => self.project_open_req(project_panel::OpenReq {
                path: p.clone(),
                permanent: req.permanent,
            }),
            (None, None) => return,
        }
        let start = {
            let ed = self.find_box_mut();
            ed.goto_line(req.line);
            ed.caret() + req.col
        };
        let mut inv = Invalidations::default();
        self.find_box_mut()
            .select_range(start, start + req.len, &mut inv);
        if req.permanent {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    /// 찾기 패널이 열려 있으면 일치 구간 전부를 편집기에 표시(반투명 · T-73) · 닫혀 있으면 지운다.
    pub(crate) fn sync_find_marks(&mut self) {
        let marks = if self.find.is_visible() {
            self.find_matches()
        } else {
            Vec::new()
        };
        self.find_box_mut().set_find_marks(marks);
    }
}

/// 선택 `[a, b)`가 둘 이상의 줄에 걸치는가(끝이 개행 바로 뒤면 그 개행의 줄까지만 센다 · 131차).
pub(crate) fn selection_spans_lines(buf: &nexa_ctl::edit::TextBuf, a: usize, b: usize) -> bool {
    b > a && buf.line_of(a) != buf.line_of(b - 1)
}

/// ≡(선택 범위에서 찾기)를 켤 때 쓸 범위(131차 · 순수 판정): 지금 선택이 여러 줄이면 그것 · 아니면 열 때 보관한 선택 ·
/// 그것도 없으면 지금 선택(한 줄이어도).
pub(crate) fn pick_find_scope(
    cur: Option<(usize, usize)>,
    cur_multi: bool,
    pending: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    if cur_multi {
        return cur;
    }
    pending.or(cur)
}

/// Esc(취소)가 열 때 자리로 되돌리는가(131차 ⑩ · 순수 판정): 이번에 연 뒤 검색으로 캐럿이 옮겨진 적이 없을 때만.
pub(crate) fn cancel_restores(moved: bool) -> bool {
    !moved
}

/// 찾기를 열 때 캐럿을 첫 일치로 옮기는가(131차 10-20 · 순수 판정): 한 줄 선택을 씨앗으로 넣었을 때만 · 여러 줄 선택(지킴)이나
/// 선택 없이 연 경우(지난 검색어)는 옮기지 않는다 — 다음 찾기를 눌러야 이동 · Esc = 열 때 자리로.
pub(crate) fn open_moves_caret(keep_multiline: bool, seeded: bool) -> bool {
    !keep_multiline && seeded
}

/// 설정 `editor.find_auto_scope` × 선택이 여러 줄인가 → 자동으로 켜는가(131차 · MC/DC).
pub(crate) fn auto_scope_on(mode: &str, multi: bool) -> bool {
    match mode {
        "always" => true,
        "multiline" => multi,
        _ => false,
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn selection_spans_lines_counts_newline_at_end_only_once() {
        let buf = nexa_ctl::edit::TextBuf::from_string("ab\ncd\nef".into());
        assert!(!selection_spans_lines(&buf, 0, 2), "한 줄 안");
        assert!(selection_spans_lines(&buf, 0, 4), "두 줄에 걸침");
        assert!(!selection_spans_lines(&buf, 0, 3), "줄 끝 개행까지 = 한 줄");
        assert!(selection_spans_lines(&buf, 2, 4), "개행부터 다음 줄로");
        assert!(!selection_spans_lines(&buf, 3, 3), "빈 선택");
    }

    #[test]
    fn pick_scope_prefers_multiline_then_pending_then_current() {
        let cur = Some((10, 12));
        let pend = Some((0, 40));
        assert_eq!(
            pick_find_scope(cur, true, pend),
            cur,
            "지금 선택이 여러 줄 = 그것"
        );
        assert_eq!(
            pick_find_scope(cur, false, pend),
            pend,
            "한 줄(일치 이동 뒤) = 열 때 보관한 선택"
        );
        assert_eq!(
            pick_find_scope(cur, false, None),
            cur,
            "보관 없음 = 지금 선택"
        );
        assert_eq!(pick_find_scope(None, false, pend), pend);
        assert_eq!(pick_find_scope(None, false, None), None);
    }

    #[test]
    fn cancel_restores_only_when_no_search_moved_the_caret() {
        assert!(cancel_restores(false), "열기만 = 원래 자리로");
        assert!(!cancel_restores(true), "검색으로 이동 = 그 자리 유지");
    }

    #[test]
    fn open_moves_caret_only_for_single_line_seed() {
        assert!(open_moves_caret(false, true), "한 줄 선택 씨앗 = 그 일치로");
        assert!(
            !open_moves_caret(false, false),
            "선택 없음(지난 검색어) = 캐럿 그대로"
        );
        assert!(!open_moves_caret(true, false), "여러 줄 선택 = 지킴");
        assert!(!open_moves_caret(true, true));
    }

    #[test]
    fn auto_scope_mc_dc() {
        assert!(!auto_scope_on("never", true));
        assert!(!auto_scope_on("never", false));
        assert!(auto_scope_on("multiline", true));
        assert!(!auto_scope_on("multiline", false));
        assert!(auto_scope_on("always", true));
        assert!(auto_scope_on("always", false));
        assert!(!auto_scope_on("garbage", true), "모르는 값 = never");
    }
}
