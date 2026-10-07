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
    let row = |prefix: &str, m: Msg| (format!("help:{prefix}"), format!("{prefix}  -  {}", t(m)));
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
        self.goto_sent = self.goto_files.len();
        self.goto_last_push = Instant::now();
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
        let (mut cmds, seen) = self.goto_fixed_items();
        cmds.extend(self.goto_project_items(0, &seen));
        cmds
    }

    /// 열린 탭·최근 파일 항목 + 그 경로 집합(프로젝트 파일 중복 제거용).
    fn goto_fixed_items(&self) -> (Vec<(String, String)>, std::collections::HashSet<PathBuf>) {
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
                format!("{act}{title}{mark}  -  {where_}"),
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
                format!("{}: {name}  -  {dir}", t(Msg::LblFileRecent)),
            ));
        }
        (cmds, seen)
    }

    /// 프로젝트 폴더 파일 항목 — `from`번째부터(열거가 도착하는 대로 **새 것만** 라벨을 만든다 · 재생성 0 · 사용자 10-07 "멈춤").
    fn goto_project_items(
        &self,
        from: usize,
        seen: &std::collections::HashSet<PathBuf>,
    ) -> Vec<(String, String)> {
        let mut cmds: Vec<(String, String)> = Vec::new();
        let roots = &self.goto_files_key;
        for (i, p) in self.goto_files.iter().enumerate().skip(from) {
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
            cmds.push((format!("file.path:{i}"), format!("{name}  -  {rel}")));
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
        // ★ 옛 목록이 있으면 **비우지 않고** 새 버퍼에 모아 끝날 때 교체한다(사용자 10-07 "실제 파일이 있는데 안 나옴" = 재열거 동안
        //   목록이 비던 결함 · 협업 105 ①). 첫 열거(옛 목록 없음)만 도착하는 대로 바로 보탠다.
        self.goto_into_new = !self.goto_files.is_empty();
        self.goto_files_new.clear();
        self.goto_capped = false;
        self.goto_files_key = folders.clone();
        let opts = parwalk::ListOpts {
            show_hidden: self.settings.flag("file.show_hidden"),
            show_dot: self.settings.flag("file.show_dot"),
            // 제외 폴더(설정 `project.exclude` · 쉼표 구분 · D-259 기본 `.git,.svn,.hg,node_modules,target,.nsql`).
            skip_dirs: self
                .settings
                .get("project.exclude")
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
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
        let into_new = self.goto_into_new;
        // ★ 시간 예산으로 수거(틱당 4ms) — 종전 "64메시지"는 틱 빈도에 묶여 큰 트리(target/ 수십만 파일)에서 10초 넘게 "읽는 중"이었다
        //   (협업 V1 bin38 실측 · 디스크가 아니라 수거가 병목).
        let t0 = Instant::now();
        loop {
            if t0.elapsed() >= Duration::from_millis(4) {
                break;
            }
            match rx.try_recv() {
                Ok(parwalk::DirMsg::Dir { entries, .. }) => {
                    if let Ok(es) = entries {
                        let dst = if into_new {
                            &mut self.goto_files_new
                        } else {
                            &mut self.goto_files
                        };
                        for e in es {
                            if !e.is_dir {
                                dst.push(e.path);
                                changed = true;
                                if max > 0 && dst.len() >= max {
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
            self.goto_capped = true;
        }
        if done {
            // 정렬하지 않는다 — 항목 id가 `file.path:<번호>`(목록 위치)라 순서가 곧 열쇠.
            self.goto_walk = None;
            self.goto_files_at = Some(Instant::now());
            if into_new {
                // 재열거 끝 = 새 목록으로 **교체**하고 팔레트 목록을 통째로 다시(인덱스가 바뀌므로 덧붙이기 아님).
                self.goto_files = std::mem::take(&mut self.goto_files_new);
                self.goto_into_new = false;
                self.goto_sent = self.goto_files.len();
                self.goto_last_push = Instant::now();
                if self.palette.is_open() && self.palette.goto_mode() {
                    let files = self.goto_file_items();
                    self.palette.set_files(files);
                    self.palette.set_files_note(&self.goto_files_note());
                    return true;
                }
                return changed;
            }
        }
        if into_new {
            // 새 버퍼에 모으는 중 — 옛 목록을 그대로 둔다.
            return false;
        }
        // 팔레트가 Goto로 열려 있으면 **새 파일만** 라벨을 만들어 덧붙인다 · 100ms 스로틀(끝났으면 바로 · 사용자 10-07 "멈춤").
        let pending = self.goto_files.len() > self.goto_sent;
        if pending
            && (done || self.goto_last_push.elapsed() >= Duration::from_millis(100))
            && self.palette.is_open()
            && self.palette.goto_mode()
        {
            let (_, seen) = self.goto_fixed_items();
            let items = self.goto_project_items(self.goto_sent, &seen);
            self.goto_sent = self.goto_files.len();
            self.goto_last_push = Instant::now();
            self.palette.append_files(items);
            self.palette.set_files_note(&self.goto_files_note());
            return true;
        }
        changed && done
    }

    /// 파일 목록 안내(팔레트 목록 끝 흐린 줄 · 덤프): 상한에 닿았으면 알린다 — "있는 파일이 안 나오는" 것을 조용히 두지 않는다.
    fn goto_files_note(&self) -> String {
        if self.goto_capped {
            tf(
                Msg::PalFilesCapped,
                &[&self.goto_files.len().to_string(), "project.scan_max"],
            )
        } else if self.goto_walk.is_some() {
            tf(Msg::PalFilesScanning, &[&self.goto_files.len().to_string()])
        } else {
            String::new()
        }
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
                    format!("{indent}{}  -  {detail}  :{}", s.name, s.line),
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
