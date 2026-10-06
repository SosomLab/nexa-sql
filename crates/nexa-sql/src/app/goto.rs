//! App — Goto Anything(Ctrl+P · 사용자 10-07 "VS Code식 접두 · 열린 파일 → 최근 → 프로젝트 폴더 퍼지 · `이름:줄`").
//!
//! 팔레트 부품(`palette.rs`)이 접두를 풀고(`parse_goto`), 여기는 **원천**을 준다 — 파일 목록(열린 탭 · 최근 파일 · 프로젝트
//! 폴더 파일 = `parwalk` 병렬 열거 · 캐시 60초) · 명령 목록(`palette_commands`) · 현재 파일 심볼(`goto_symbol_items`) ·
//! 현재 파일 줄(`%` 빠른 검색) · 도움말(`?`). 선택 처리 = `goto_pick`(`file.path:` · `help:` · `#L줄` 꼬리).

use crate::*;
use std::sync::atomic::Ordering;

/// 프로젝트 폴더 파일 캐시 수명(초) — 그 안에 다시 열면 열거를 생략한다.
const GOTO_CACHE_SECS: u64 = 60;
/// `%` 빠른 검색이 보는 최대 줄 수(큰 파일 모드는 아예 비움).
const GOTO_DOC_LINES_MAX: usize = 50_000;

/// `?` 도움말 항목 — (id = `help:<접두>` · 라벨) · Enter = 그 접두를 입력란에.
pub(crate) fn goto_help_items() -> Vec<(String, String)> {
    // 비례폭 글꼴이라 열 맞춤 대신 `예시  —  설명` · 예시는 SQL 맥락(협업 V1 bin34 관찰).
    let row = |prefix: &str, m: Msg| (format!("help:{prefix}"), format!("{prefix}  —  {}", t(m)));
    vec![
        row("emp", Msg::PalHelpFile),
        row("sql/emp", Msg::PalHelpPath),
        row("emp.sql:12", Msg::PalHelpFileLine),
        row(">", Msg::PalHelpCmd),
        row(":12", Msg::PalHelpLine),
        row("@select", Msg::PalHelpSym),
        row("@:", Msg::PalHelpSymKind),
        row("#emp", Msg::PalHelpWorkspaceSym),
        row("%where", Msg::PalHelpQuick),
        row("?", Msg::PalHelpHelp),
    ]
}

impl App {
    /// Ctrl+P — 원천을 모아 팔레트를 Goto 모드로 연다 · 프로젝트 폴더 파일은 캐시(없거나 묵으면 뒤에서 열거해 도착하는 대로 보탬).
    pub(crate) fn open_goto_anything(&mut self, prefill: &str) {
        self.goto_refresh_files();
        let files = self.goto_file_items();
        let cmds = self.palette_commands();
        let syms = self.goto_symbol_items();
        let lines = self.goto_doc_lines();
        self.palette
            .open_goto(files, cmds, syms, lines, goto_help_items(), prefill);
        self.ime_refresh();
        self.redraw();
    }

    /// 파일 항목 = ① 열린 탭(`tab:<id>` · ✓ 활성 · `*` 미저장) ② 최근 파일(`file.recent:<i>`) ③ 프로젝트 폴더 파일(`file.path:<i>` ·
    /// 라벨 = 이름 · 폴더 기준 상대 경로) — 순위 가산은 팔레트가 id 접두로 준다.
    fn goto_file_items(&self) -> Vec<(String, String)> {
        let mut cmds: Vec<(String, String)> = Vec::new();
        // 같은 파일은 한 번만(열린 탭 > 최근 > 프로젝트 · 협업 V1 bin33 ① "열린 탭이 최근에 한 번 더").
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for (id, title, path, dirty, active) in self.editors.tab_entries() {
            if let Some(p) = &path {
                seen.insert(p.clone());
            }
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
            if !seen.insert(p.clone()) {
                continue;
            }
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            // 라벨 = `최근 파일: 이름  —  폴더`(협업 V1 bin33 ② = 이름이 두 번 보임).
            // `display`는 프로젝트 폴더 기준 상대 — 폴더 루트 자체면 빈 글이 되므로 그때는 절대 경로(협업 V1 bin34).
            let dir = p
                .parent()
                .map(|d| {
                    let s = nexa_fs::path::display(d);
                    if s.is_empty() {
                        d.to_string_lossy().into_owned()
                    } else {
                        s
                    }
                })
                .unwrap_or_default();
            cmds.push((
                format!("file.recent:{i}"),
                format!("{}: {name}  —  {dir}", t(Msg::LblFileRecent)),
            ));
        }
        let roots = &self.goto_files_key;
        for (i, p) in self.goto_files.iter().enumerate() {
            if seen.contains(p) {
                continue;
            }
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            // 상대 경로 = 그 파일이 속한 프로젝트 폴더 기준(여러 폴더면 폴더 이름부터).
            let rel = roots
                .iter()
                .find_map(|r| p.strip_prefix(r).ok().map(|rel| (r, rel)))
                .map(|(r, rel)| {
                    let root = r
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if roots.len() > 1 {
                        format!("{root}/{rel}")
                    } else {
                        rel
                    }
                })
                .unwrap_or_else(|| nexa_fs::path::display(p));
            cmds.push((format!("file.path:{i}"), format!("{name}  —  {rel}")));
        }
        cmds
    }

    /// 프로젝트 폴더 파일 캐시 — 폴더 목록이 바뀌었거나 60초가 지났으면 `parwalk`로 다시 열거(백그라운드 · 도착은 `goto_walk_tick`).
    fn goto_refresh_files(&mut self) {
        let folders = self.project.folders.clone();
        if folders.is_empty() {
            self.goto_files.clear();
            self.goto_files_key.clear();
            self.goto_files_at = None;
            self.goto_walk = None;
            return;
        }
        let stale = self.goto_files_key != folders
            || self
                .goto_files_at
                .is_none_or(|t| t.elapsed().as_secs() >= GOTO_CACHE_SECS);
        if !stale || self.goto_walk.is_some() {
            return;
        }
        self.goto_files.clear();
        self.goto_files_key = folders.clone();
        let opts = parwalk::ListOpts {
            show_hidden: self.settings.flag("file.show_hidden"),
            show_dot: self.settings.flag("file.show_dot"),
        };
        let threads = self.settings.int("project.scan_threads").clamp(0, 16) as usize;
        self.goto_walk = Some(parwalk::spawn(folders, opts, threads));
    }

    /// 열거 결과 수거(틱) — 파일만 모으고(`project.scan_max` 0 = 무제한) 팔레트가 Goto 모드로 열려 있으면 목록을 바꾼다. 돌려주는 값 = 다시 그릴 것.
    pub(crate) fn goto_walk_tick(&mut self) -> bool {
        let Some((rx, cancel)) = self.goto_walk.as_ref() else {
            return false;
        };
        let max = self.settings.int("project.scan_max").max(0) as usize;
        let mut changed = false;
        let mut done = false;
        let mut capped = false;
        for _ in 0..64 {
            match rx.try_recv() {
                Ok(parwalk::DirMsg::Dir { entries, .. }) => {
                    if let Ok(es) = entries {
                        for e in es {
                            if !e.is_dir {
                                self.goto_files.push(e.path);
                                changed = true;
                                if max > 0 && self.goto_files.len() >= max {
                                    capped = true;
                                    break;
                                }
                            }
                        }
                    }
                    if capped {
                        break;
                    }
                }
                Ok(parwalk::DirMsg::Done { .. }) => {
                    done = true;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        if capped {
            cancel.store(true, Ordering::Relaxed);
            done = true;
        }
        if done {
            self.goto_walk = None;
            self.goto_files_at = Some(Instant::now());
            self.goto_files.sort();
            changed = true;
        }
        if changed && self.palette.is_open() && self.palette.goto_mode() {
            let files = self.goto_file_items();
            self.palette.set_files(files);
            return true;
        }
        false
    }

    /// 현재 탭의 심볼(팔레트 항목 · `sym:<byte>` · 부적합·큰 파일 = 빈 목록) — Goto Symbol · Goto Anything `@` 공용.
    pub(crate) fn goto_symbol_items(&mut self) -> Vec<(String, String)> {
        let i = self.editors.active();
        if self.intel_unsuitable(i, false).is_some() {
            return Vec::new();
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
        ol.symbols
            .iter()
            .map(|s| {
                let indent = "  ".repeat(s.depth as usize);
                let detail = if s.detail.is_empty() {
                    s.kind.label().to_string()
                } else {
                    format!("{} · {}", s.kind.label(), s.detail)
                };
                (
                    format!("sym:{}", s.byte),
                    format!("{indent}{}  —  {detail}  :{}", s.name, s.line),
                )
            })
            .collect()
    }

    /// `%` 빠른 검색의 원천 = 현재 파일 줄(상한 · 큰 파일 모드는 비움).
    fn goto_doc_lines(&mut self) -> Vec<String> {
        let i = self.editors.active();
        if self.intel_unsuitable(i, false).is_some() {
            return Vec::new();
        }
        self.ed_mut()
            .text()
            .lines()
            .take(GOTO_DOC_LINES_MAX)
            .map(str::to_string)
            .collect()
    }

    /// Goto 선택 처리 — `…#L<n>`(파일 연 뒤 그 줄) · `file.path:<i>` · `help:<접두>`. 돌려주는 값 = 처리했다.
    pub(crate) fn goto_pick(&mut self, id: &str) -> bool {
        if let Some((base, n)) = id.rsplit_once("#L") {
            if let Ok(n) = n.parse::<usize>() {
                self.menu_action(base);
                if n > 0 {
                    self.ed_mut().goto_line(n);
                    self.set_focus(Focus::Editor);
                    self.redraw();
                }
                return true;
            }
        }
        if let Some(rest) = id.strip_prefix("file.path:") {
            if let Some(p) = rest
                .parse::<usize>()
                .ok()
                .and_then(|i| self.goto_files.get(i).cloned())
            {
                self.open_file(&p);
            }
            return true;
        }
        if let Some(prefix) = id.strip_prefix("help:") {
            // 도움말 항목 = 그 접두를 입력란에(팔레트는 그대로 열려 있다).
            let p = prefix.to_string();
            self.palette.set_query(&p);
            self.redraw();
            return true;
        }
        false
    }
}
