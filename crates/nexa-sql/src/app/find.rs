//! App — 찾기·바꾸기 · 파일 검색(docs/36).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 편집기 본문에서 질의 일치 위치(문자 인덱스 · 대소문자 옵션) 전부.
    /// 찾기 조건 → 정규식(D-76): 정규식 모드가 아니어도 같은 엔진(글자 그대로 이스케이프)을 쓴다 — 단어 단위·대소문자 규칙 한 곳.
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
            let full = self.ed_mut().text();
            return rx::find_all(&r, &full);
        }
        let q: Vec<char> = self.find.query().chars().collect();
        let (cs, ww) = (self.find.case_sensitive(), self.find.whole_word());
        let buf = self.editors.cur().buf();
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

    /// 다음/이전 일치로 이동(순환) — `advance`면 현재 선택을 지나서, 아니면 캐럿부터.
    pub(crate) fn find_step(&mut self, forward: bool, advance: bool) {
        if !self.find.is_visible() {
            return;
        }
        let matches = self.find_matches();
        self.find.set_has_matches(!matches.is_empty());
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.ed_mut().set_find_marks(Vec::new());
            self.redraw();
            return;
        }
        let sel = self.ed_mut().selection();
        let caret = self.ed_mut().caret();
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
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(s, e, &mut inv);
        self.find.set_status(tf(
            Msg::StFindCount,
            &[&(idx + 1).to_string(), &matches.len().to_string()],
        ));
        // 일치 전부 표시(T-73).
        self.ed_mut().set_find_marks(matches);
        self.redraw();
    }

    /// 현재 선택이 일치면 바꾸고 다음으로.
    /// 일치 `(a, b)`에 넣을 치환문 — 정규식 모드면 `$1`/`${name}` 확장 · 아니면 글자 그대로.
    fn find_expansion(&mut self, a: usize) -> String {
        let repl = self.find.replacement();
        let full = self.ed_mut().text();
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
        if let Some((a, b)) = self.ed_mut().selection() {
            if matches.contains(&(a, b)) {
                let repl = self.find_expansion(a);
                let mut inv = Invalidations::default();
                self.ed_mut().replace_range(a, b, &repl, &mut inv);
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
        self.ed_mut().replace_many(&edits, &mut inv);
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
                    match self.ed_mut().selection() {
                        Some((a, b)) if b > a => {
                            self.find_scope = Some((a, b));
                            self.ed_mut().set_find_scope(Some((a, b)));
                            // 범위 안 첫 일치로(선택은 범위 표시가 대신한다).
                            let mut inv = Invalidations::default();
                            self.ed_mut().select_range(a, a, &mut inv);
                        }
                        _ => {
                            self.find.set_in_selection(false);
                            self.sess.status = t(Msg::StFindNoSelection).into();
                        }
                    }
                } else {
                    self.find_scope = None;
                    self.ed_mut().set_find_scope(None);
                }
                self.find_step(true, false);
            }
            FindAction::SelectAll => {
                let matches = self.find_matches();
                if matches.is_empty() {
                    self.find.set_status(t(Msg::StFindNone));
                } else {
                    let n = matches.len();
                    self.ed_mut().set_regions_pub(&matches);
                    self.find
                        .set_status(tf(Msg::StSelections, &[&n.to_string()]));
                    self.set_focus(Focus::Editor);
                }
                self.redraw();
            }
            FindAction::Close => {
                self.find.close();
                self.find_scope = None;
                self.ed_mut().set_find_scope(None);
                self.ed_mut().set_find_marks(Vec::new());
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
            let ed = self.ed_mut();
            ed.goto_line(req.line);
            ed.caret() + req.col
        };
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(start, start + req.len, &mut inv);
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
        self.ed_mut().set_find_marks(marks);
    }
}
