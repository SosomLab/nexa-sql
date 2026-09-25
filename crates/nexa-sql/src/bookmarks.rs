//! 북마크 — 앱 쪽 관리자([docs/69](../../../docs/69-bookmarks.md) · T-167 B2·B3 · 사용자 09-22 "북마크 관리 기능 전체").
//!
//! 코어(`nsql-bookmarks`)는 본문을 모른다 → 여기서 편집기 탭을 문서 열쇠([`DocKey`])로 옮기고, 탭의 줄 변경 기록으로 **L0**를,
//! 파일을 열 때·기록이 끊겼을 때 본문으로 **L1** 재탐색을 돌린다. 저장은 워크스페이스 파일
//! `NSQL_HOME/workspaces/<프로젝트 이름|default>.nsql-workspace`의 `"bookmarks"` 키(69 §7-1 개인 쪽 · 공용은 B7).
//! 시각·파일은 여기서만 만진다(코어는 순수).

use crate::editors::Editors;
use nexa_ctl::Color;
use nsql_bookmarks::{
    make_anchor, relocate, Anchor, Bookmark, DocKey, Reason, RelocateOpts, Relocated, State, Store,
};
use nsql_settings::Settings;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 저장 디바운스(69 §7-3: 입력 1초 + 최대 5초).
const SAVE_DEBOUNCE: Duration = Duration::from_secs(1);
const SAVE_MAX_WAIT: Duration = Duration::from_secs(5);

pub(crate) struct Bookmarks {
    pub(crate) store: Store,
    pub(crate) opts: RelocateOpts,
    pub(crate) enabled: bool,
    persist: bool,
    max_per_doc: usize,
    max_total: usize,
    stale_days: u32,
    /// 워크스페이스 파일(없으면 저장 안 함).
    path: Option<PathBuf>,
    /// 폴더 모드의 작업 폴더(`nexa-sql .` · 사용자 09-23) — 파일 모드 로컬 상태를 `<폴더>/.nsql/`에(`project::WorkMode`).
    folder: Option<PathBuf>,
    /// 탭 id → (마지막으로 소비한 줄 변경 번호, 그때의 epoch).
    seq: HashMap<u64, (u64, u64)>,
    /// 다음 `bind_project`에서 **로컬 → 프로젝트 이관**(파일 모드에서 새 프로젝트를 만들 때 · 사용자 09-23): 지금 것을 로컬
    /// 워크스페이스에 다시 쓰지 않고 로컬 파일을 비운다(프로젝트 파일에 이미 담겼다 · 닫으면 로컬 = 빈 세트).
    migrate_local: bool,
    /// 탭 id → 마지막에 본 문서 열쇠(이름 없는 탭이 파일로 저장되면 열쇠를 옮긴다 · C-21).
    docs: HashMap<u64, DocKey>,
    dirty_since: Option<Instant>,
    first_dirty: Option<Instant>,
    /// 표시 변경(거터·패널) 1회성 신호.
    changed: bool,
    /// 마지막으로 지운 것들(5초 되돌리기 토스트 · 69 C-28).
    last_removed: Vec<Bookmark>,
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 파일 경로 비교의 대소문자 무시(Windows·macOS).
const CASE_INSENSITIVE: bool = cfg!(any(windows, target_os = "macos"));

impl Bookmarks {
    pub(crate) fn new() -> Self {
        Bookmarks {
            store: Store::new(),
            opts: RelocateOpts::default(),
            enabled: true,
            persist: true,
            max_per_doc: 500,
            max_total: 5000,
            stale_days: 30,
            path: None,
            folder: None,
            seq: HashMap::new(),
            migrate_local: false,
            docs: HashMap::new(),
            dirty_since: None,
            first_dirty: None,
            changed: false,
            last_removed: Vec::new(),
        }
    }

    /// 설정을 읽는다(시작 · 변경 때 · 69 §9).
    pub(crate) fn apply_settings(&mut self, s: &Settings) {
        self.enabled = s.flag("bookmark.enabled");
        self.persist = s.flag("bookmark.persist");
        self.max_per_doc = s.int("bookmark.max_per_doc").max(0) as usize;
        self.max_total = s.int("bookmark.max_total").max(0) as usize;
        self.stale_days = s.int("bookmark.stale_days").max(0) as u32;
        self.opts = RelocateOpts {
            search_lines: s.int("bookmark.search_lines").max(0) as usize,
            similarity: s
                .get("bookmark.similarity")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.6),
            relocate_unique: s.flag("bookmark.relocate_unique"),
            max_lines: s.int("bookmark.relocate_max_lines").max(0) as usize,
            trim: s.flag("bookmark.anchor_trim"),
            context: s.flag("bookmark.anchor_context"),
            anchor_chars: s.int("bookmark.anchor_chars").max(8) as usize,
        };
        self.changed = true;
    }

    /// 표시가 바뀌었는가(1회성 · 거터·패널·상태줄 갱신 신호).
    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    fn touch(&mut self) {
        self.changed = true;
        let now = Instant::now();
        self.dirty_since = Some(now);
        self.first_dirty.get_or_insert(now);
    }

    // ───────────── 저장소(워크스페이스 파일) ─────────────

    /// 폴더 모드(`nexa-sql .`)의 작업 폴더 — 기동 때 한 번(`bind_project` 전에).
    pub(crate) fn set_folder(&mut self, dir: Option<PathBuf>) {
        self.folder = dir;
    }

    /// 워크스페이스 파일 자리 = 작업 모드(`project::WorkMode` · 사용자 09-23): **파일 모드** = 전역 `<설정>/workspaces/default.nsql-workspace` ·
    /// **폴더 모드** = `<폴더>/.nsql/default.nsql-workspace` · **프로젝트 모드** = 옛 `<설정>/workspaces/<이름>.nsql-workspace`(읽기만 · 이관용 ·
    /// 원천은 프로젝트 파일).
    pub(crate) fn workspace_path(&self, project: Option<&Path>) -> Option<PathBuf> {
        match crate::project::WorkMode::of(project, self.folder.as_deref()) {
            crate::project::WorkMode::Project(p) => {
                let dir = nsql_settings::config_dir()?.join("workspaces");
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "default".into());
                Some(dir.join(format!("{stem}.nsql-workspace")))
            }
            mode => Some(
                mode.local_dir()?
                    .join("workspaces")
                    .join("default.nsql-workspace"),
            ),
        }
    }

    /// 프로젝트가 정해졌다(시작 · 전환 · 닫기) → 그 워크스페이스의 북마크를 읽는다(먼저 지금 것을 저장).
    /// 파일 모드에서 새 프로젝트를 만들 때(사용자 09-23 "로컬과 프로젝트 북마크는 분리") — 다음 `bind_project`가 지금 것을 로컬에
    /// 되쓰지 않고 로컬 파일을 비우게 한다(= 이관 · 프로젝트 파일에는 호스트가 이미 담았다).
    pub(crate) fn mark_migrate_local(&mut self) {
        self.migrate_local = true;
    }

    pub(crate) fn bind_project(&mut self, project: Option<&Path>) {
        if std::mem::take(&mut self.migrate_local) {
            // 이관: 로컬 워크스페이스 파일 = 빈 저장소(있을 때만 씀 · 없으면 그대로 없음).
            self.dirty_since = None;
            self.first_dirty = None;
            if let Some(p) = self.path.as_ref().filter(|p| p.exists()) {
                if let Err(e) = std::fs::write(p, Store::new().to_json()) {
                    eprintln!("bookmarks: {}: {e}", p.display());
                }
            }
        } else {
            self.save_now();
        }
        self.path = self.workspace_path(project);
        self.store = Store::new();
        self.seq.clear();
        if let Some(p) = &self.path {
            if let Ok(text) = std::fs::read_to_string(p) {
                match Store::from_json(&text) {
                    Ok(st) => self.store = st,
                    Err(e) => eprintln!("bookmarks: {}: {e}", p.display()),
                }
            }
        }
        let n = self.store.prune(now_epoch(), self.stale_days);
        if n > 0 {
            self.touch();
        }
        // 파일·폴더 모드: 이름 없는 탭 열쇠는 메모리 전용이므로 파일에서 온 것(옛 규칙의 잔재 · 지난 실행의 탭 id)은 버린다.
        if project.is_none() {
            let before = self.store.items.len();
            self.store
                .items
                .retain(|b| !matches!(b.doc, DocKey::Scratch { .. }));
            if self.store.items.len() != before {
                self.touch();
            }
        }
        // ★ 프로젝트가 열려 있으면 북마크의 원천은 **프로젝트 파일**(09-23 · `project_restore`가 내장 북마크를 올린다) —
        //   워크스페이스 파일은 읽기만(옛 파일 이관) 하고 쓰지 않는다. 프로젝트 없음 = `default.nsql-workspace` 그대로.
        if project.is_some() {
            self.path = None;
        }
        self.changed = true;
    }

    /// 이름 없는 탭의 북마크 열쇠 재매핑(프로젝트 복원 · 사용자 09-23 검토): 프로젝트 파일에 적힌 옛 탭 id → 방금 만든 새 탭 id.
    /// 본문은 프로젝트 파일에서 그대로 올라오므로 줄 앵커가 맞는다. 표에 없는 Scratch 열쇠(탭이 사라진 것)는 그대로 둔다(무효 처리 규칙).
    pub(crate) fn remap_scratch(&mut self, map: &[(u64, u64)]) {
        if map.is_empty() {
            return;
        }
        let mut any = false;
        for b in &mut self.store.items {
            if let DocKey::Scratch { tab } = &mut b.doc {
                if let Some((_, new)) = map.iter().find(|(old, _)| old == tab) {
                    *tab = *new;
                    any = true;
                }
            }
        }
        if any {
            self.docs.clear();
            self.seq.clear();
            self.touch();
        }
    }

    /// 프로젝트 파일에 내장된 북마크(JSON)로 바꾼다(프로젝트 복원 · 사용자 09-23) — 실패하면 그대로.
    pub(crate) fn load_json(&mut self, text: &str) -> bool {
        match Store::from_json(text) {
            Ok(st) => {
                self.store = st;
                self.seq.clear();
                self.changed = true;
                true
            }
            Err(e) => {
                eprintln!("bookmarks(project): {e}");
                false
            }
        }
    }

    /// 디바운스 저장(틱에서 부른다) — 썼으면 true.
    pub(crate) fn tick_save(&mut self) -> bool {
        let Some(d) = self.dirty_since else {
            return false;
        };
        let now = Instant::now();
        let waited = self
            .first_dirty
            .is_some_and(|f| now.duration_since(f) >= SAVE_MAX_WAIT);
        if now.duration_since(d) >= SAVE_DEBOUNCE || waited {
            self.save_now();
            return true;
        }
        false
    }

    /// 디바운스 저장이 걸려 있으면 그 **마감 시각**(입력 뒤 1초 · 처음 더럽혀진 뒤 최대 5초 중 이른 것) — 호스트의 틱 스케줄러가
    /// 이 시각에 깨어 `tick_save`를 부른다(사용자 09-23 "값이 바뀌어도 저장이 안 됨" = 다른 사건이 없으면 틱이 안 돌던 결함).
    pub(crate) fn next_save_at(&self, now: Instant) -> Option<Instant> {
        let d = self.dirty_since?;
        let by_debounce = d + SAVE_DEBOUNCE;
        let by_max = self.first_dirty.map_or(by_debounce, |f| f + SAVE_MAX_WAIT);
        Some(by_debounce.min(by_max).max(now))
    }

    /// 즉시 저장(닫기·전환·종료).
    pub(crate) fn save_now(&mut self) {
        self.dirty_since = None;
        self.first_dirty = None;
        if !self.persist {
            return;
        }
        let Some(p) = &self.path else {
            return;
        };
        if self.store.items.is_empty() && !p.exists() {
            return; // 빈 저장소를 위해 파일을 만들지 않는다.
        }
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // ★ 파일·폴더 모드(= 로컬 워크스페이스 파일에 쓰는 경우)의 이름 없는 탭 북마크는 **메모리 전용**(사용자 09-23 "일반·폴더 모드의
        //   이름 없는 탭은 임시 · 영속은 의미 없음 · 복원 필요 없는 것은 파일에 쓰지 않는다") — 파일에는 파일·객체 열쇠만 담는다.
        //   프로젝트 모드는 여기를 지나지 않는다(`path = None` · 호스트가 `store.to_json()` 전체를 프로젝트 파일에 담고 §101로 재매핑).
        let text = if self
            .store
            .items
            .iter()
            .any(|b| matches!(b.doc, DocKey::Scratch { .. }))
        {
            let mut persisted = self.store.clone();
            persisted
                .items
                .retain(|b| !matches!(b.doc, DocKey::Scratch { .. }));
            persisted.to_json()
        } else {
            self.store.to_json()
        };
        let tmp = p.with_extension("nsql-workspace.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }

    // ───────────── 문서 열쇠 · 본문 ─────────────

    pub(crate) fn doc_key(ed: &Editors, i: usize) -> DocKey {
        match ed.path_of(i) {
            Some(p) => DocKey::file(&p.to_string_lossy()),
            None => DocKey::Scratch { tab: ed.tab_id(i) },
        }
    }

    fn lines_of(ed: &Editors, i: usize) -> Vec<String> {
        let Some(tb) = ed.tab_box(i) else {
            return Vec::new();
        };
        let buf = tb.buf();
        (0..buf.line_count())
            .map(|l| buf.line_text(l).into_owned())
            .collect()
    }

    fn anchor_at(&self, ed: &Editors, i: usize, line: usize, col: u32) -> Anchor {
        let lines = Self::lines_of(ed, i);
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut a = make_anchor(&refs, line, col, &self.opts);
        // 큰 문서는 해시를 생략(0 = "모름")— 재탐색도 줄 번호만(69 §3-5).
        if self.opts.max_lines > 0 && refs.len() > self.opts.max_lines {
            a.doc_hash = 0;
        }
        a
    }

    // ───────────── 명령 ─────────────

    /// 캐럿 줄 토글 — `Ok(Some(true))` = 더함 · `Ok(Some(false))` = 뺌 · `Err` = 상한 키 이름.
    pub(crate) fn toggle_caret(&mut self, ed: &Editors, i: usize) -> Result<bool, &'static str> {
        let (line1, col1) = ed.caret_line_col();
        let line = line1.saturating_sub(1);
        let doc = Self::doc_key(ed, i);
        let a = self.anchor_at(ed, i, line, (col1.saturating_sub(1)) as u32);
        let r = self.store.toggle(
            doc,
            a,
            now_epoch(),
            CASE_INSENSITIVE,
            self.max_per_doc,
            self.max_total,
        );
        match r {
            Ok(added) => {
                self.touch();
                Ok(added.is_some())
            }
            Err(e) => Err(e),
        }
    }

    /// 다음/이전 북마크 줄(0-기준) — 문서 안 순환.
    pub(crate) fn next_line(&mut self, ed: &Editors, i: usize, forward: bool) -> Option<usize> {
        let doc = Self::doc_key(ed, i);
        let from = ed.caret_line_col().0.saturating_sub(1);
        let b = self
            .store
            .next_in_doc(&doc, from, forward, CASE_INSENSITIVE)?;
        let (id, line) = (b.id, b.anchor.line as usize);
        if let Some(b) = self.store.get_mut(id) {
            b.visited = now_epoch();
        }
        Some(line)
    }

    /// 문서의 북마크 전부 제거 — 지운 수.
    pub(crate) fn clear_doc(&mut self, ed: &Editors, i: usize) -> usize {
        let doc = Self::doc_key(ed, i);
        let gone = self.store.clear_doc(&doc, CASE_INSENSITIVE);
        if !gone.is_empty() {
            self.touch();
        }
        gone.len()
    }

    /// 문서의 살아 있는 북마크 줄들(0-기준 · 켜진 그룹만) — 멀티커서·거터.
    pub(crate) fn doc_lines(&self, ed: &Editors, i: usize) -> Vec<usize> {
        let doc = Self::doc_key(ed, i);
        self.store
            .for_doc(&doc, CASE_INSENSITIVE)
            .into_iter()
            .filter(|b| b.is_live() && self.store.group_enabled(b.group))
            .map(|b| b.anchor.line as usize)
            .collect()
    }

    /// 거터 마크(줄 → 색) — 니모닉은 호스트가 라벨로 얹는다(글리프 API는 U-2).
    pub(crate) fn marks_for(
        &self,
        ed: &Editors,
        i: usize,
        color: Color,
        dim: Color,
    ) -> Vec<(usize, Color)> {
        let doc = Self::doc_key(ed, i);
        self.store
            .for_doc(&doc, CASE_INSENSITIVE)
            .into_iter()
            .map(|b| {
                let c = if b.is_live() && self.store.group_enabled(b.group) {
                    color
                } else {
                    dim
                };
                (b.anchor.line as usize, c)
            })
            .collect()
    }

    /// 캐럿 줄의 북마크에 니모닉 지정(없으면 만들고 지정) — 뺏긴 항목이 있으면 그 id.
    /// 그 줄의 상태(거터 우클릭 메뉴 · 사용자 09-23): (북마크 id, 니모닉). 없으면 (None, None).
    pub(crate) fn line_state(
        &self,
        ed: &Editors,
        i: usize,
        line: usize,
    ) -> (Option<u64>, Option<u8>) {
        let doc = Self::doc_key(ed, i);
        match self.store.at_line(&doc, line, CASE_INSENSITIVE) {
            Some(b) => (Some(b.id), b.mnemonic),
            None => (None, None),
        }
    }

    pub(crate) fn set_mnemonic_at_caret(
        &mut self,
        ed: &Editors,
        i: usize,
        n: u8,
    ) -> Result<Option<u64>, &'static str> {
        let (line1, col1) = ed.caret_line_col();
        let line = line1.saturating_sub(1);
        let doc = Self::doc_key(ed, i);
        let id = match self.store.at_line(&doc, line, CASE_INSENSITIVE) {
            Some(b) => b.id,
            None => {
                let a = self.anchor_at(ed, i, line, col1.saturating_sub(1) as u32);
                self.store.add(
                    doc,
                    a,
                    now_epoch(),
                    CASE_INSENSITIVE,
                    self.max_per_doc,
                    self.max_total,
                )?
            }
        };
        let taken = self.store.set_mnemonic(id, Some(n), CASE_INSENSITIVE);
        self.touch();
        Ok(taken)
    }

    pub(crate) fn mnemonic_line(&self, ed: &Editors, i: usize, n: u8) -> Option<usize> {
        let doc = Self::doc_key(ed, i);
        self.store
            .by_mnemonic(&doc, n, CASE_INSENSITIVE)
            .map(|b| b.anchor.line as usize)
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<Bookmark> {
        let r = self.store.remove(id);
        if let Some(b) = &r {
            self.last_removed = vec![b.clone()];
            self.touch();
        }
        r
    }

    /// 마지막 제거 되돌리기(토스트 클릭) — 되살린 수.
    pub(crate) fn undo_remove(&mut self) -> usize {
        let v = std::mem::take(&mut self.last_removed);
        let n = v.len();
        for b in v {
            if self.store.get(b.id).is_none() {
                self.store.items.push(b);
            }
        }
        if n > 0 {
            self.touch();
        }
        n
    }

    pub(crate) fn remove_doc(&mut self, doc: &DocKey) -> usize {
        let gone = self.store.clear_doc(doc, CASE_INSENSITIVE);
        let n = gone.len();
        if n > 0 {
            self.last_removed = gone;
            self.touch();
        }
        n
    }

    pub(crate) fn remove_invalid(&mut self) -> usize {
        let (gone, keep): (Vec<Bookmark>, Vec<Bookmark>) =
            self.store.items.drain(..).partition(|b| !b.is_live());
        self.store.items = keep;
        let n = gone.len();
        if n > 0 {
            self.last_removed = gone;
            self.touch();
        }
        n
    }

    pub(crate) fn set_mnemonic(&mut self, id: u64, n: Option<u8>) {
        self.store.set_mnemonic(id, n, CASE_INSENSITIVE);
        self.touch();
    }

    pub(crate) fn move_group(&mut self, id: u64, gid: u32) {
        if let Some(b) = self.store.get_mut(id) {
            b.group = gid;
            self.touch();
        }
    }

    /// 새 그룹(이름 중복이면 뒤에 ` (2)`) — id.
    pub(crate) fn new_group(&mut self, base: &str) -> u32 {
        let mut name = base.to_string();
        let mut k = 2;
        while self.store.groups.iter().any(|g| g.name == name) {
            name = format!("{base} ({k})");
            k += 1;
        }
        let id = self.store.add_group(&name);
        self.touch();
        id
    }

    pub(crate) fn rename_group(&mut self, gid: u32, name: &str) {
        if let Some(g) = self.store.groups.iter_mut().find(|g| g.id == gid) {
            g.name = name.trim().to_string();
            self.touch();
        }
    }

    pub(crate) fn toggle_group(&mut self, gid: u32) {
        if let Some(g) = self.store.groups.iter_mut().find(|g| g.id == gid) {
            g.enabled = !g.enabled;
            self.touch();
        }
    }

    pub(crate) fn set_default_group(&mut self, gid: u32) {
        for g in self.store.groups.iter_mut() {
            g.default = g.id == gid;
        }
        self.touch();
    }

    /// 그룹 삭제(69 C-11) — 기본 그룹은 못 지운다 · `keep` = 항목을 기본 그룹으로, 아니면 항목까지(되돌리기 가능).
    pub(crate) fn delete_group(&mut self, gid: u32, keep: bool) -> bool {
        let Some(g) = self.store.groups.iter().find(|g| g.id == gid) else {
            return false;
        };
        if g.default {
            return false;
        }
        let def = self.store.default_group();
        if keep {
            for b in self.store.items.iter_mut().filter(|b| b.group == gid) {
                b.group = def;
            }
        } else {
            let (gone, rest): (Vec<Bookmark>, Vec<Bookmark>) =
                self.store.items.drain(..).partition(|b| b.group == gid);
            self.store.items = rest;
            if !gone.is_empty() {
                self.last_removed = gone;
            }
        }
        self.store.groups.retain(|g| g.id != gid);
        self.touch();
        true
    }

    /// 거터 라벨(줄 → 니모닉 숫자).
    pub(crate) fn labels_for(&self, ed: &Editors, i: usize) -> Vec<(usize, String)> {
        let doc = Self::doc_key(ed, i);
        self.store
            .for_doc(&doc, CASE_INSENSITIVE)
            .into_iter()
            .filter(|b| b.is_live() && b.mnemonic.is_some())
            .map(|b| (b.anchor.line as usize, b.mnemonic.unwrap_or(0).to_string()))
            .collect()
    }

    /// 미니맵 표식(줄 → 색) — 살아 있고 그룹이 켜진 것만(흐린 표식은 미니맵에 안 그린다 · 69 U-2).
    pub(crate) fn minimap_for(&self, ed: &Editors, i: usize, color: Color) -> Vec<(usize, Color)> {
        let doc = Self::doc_key(ed, i);
        self.store
            .for_doc(&doc, CASE_INSENSITIVE)
            .into_iter()
            .filter(|b| b.is_live() && self.store.group_enabled(b.group))
            .map(|b| (b.anchor.line as usize, color))
            .collect()
    }

    /// 줄 끝 인라인 라벨(줄 → `🔖 라벨`) — 라벨이 있는 것만(니모닉은 거터 · 본문은 이미 그 줄이므로 안 보여 준다).
    pub(crate) fn inline_for(
        &self,
        ed: &Editors,
        i: usize,
        max_chars: usize,
    ) -> Vec<(usize, String)> {
        let doc = Self::doc_key(ed, i);
        self.store
            .for_doc(&doc, CASE_INSENSITIVE)
            .into_iter()
            .filter(|b| b.is_live())
            .filter_map(|b| {
                let l = b.label.as_deref()?.trim();
                if l.is_empty() {
                    return None;
                }
                // 길이 상한(`bookmark.inline_label_chars`) — 넘치면 줄임표.
                let shown: String = if l.chars().count() > max_chars {
                    l.chars()
                        .take(max_chars.saturating_sub(1))
                        .chain(['\u{2026}'])
                        .collect()
                } else {
                    l.to_string()
                };
                Some((b.anchor.line as usize, format!("\u{25c6} {shown}")))
            })
            .collect()
    }

    pub(crate) fn set_label(&mut self, id: u64, label: Option<String>) {
        if let Some(b) = self.store.get_mut(id) {
            b.label = label.filter(|l| !l.trim().is_empty());
            self.touch();
        }
    }

    /// 문서의 살아 있는 개수 / 전체 살아 있는 개수(상태줄 `북마크 3/12`).
    pub(crate) fn counts_for(&self, ed: &Editors, i: usize) -> (usize, usize) {
        let doc = Self::doc_key(ed, i);
        let here = self
            .store
            .for_doc(&doc, CASE_INSENSITIVE)
            .iter()
            .filter(|b| b.is_live())
            .count();
        (here, self.store.counts().0)
    }

    // ───────────── 위치 추적 ─────────────

    /// 탭의 줄 변경 기록을 소비한다(L0) — 기록이 끊겼으면(epoch 변화 · 기록 밀림) L1 재탐색. 바뀐 것이 있으면 true.
    pub(crate) fn sync_tab(&mut self, ed: &Editors, i: usize) -> bool {
        let Some(tb) = ed.tab_box(i) else {
            return false;
        };
        let tab = ed.tab_id(i);
        let buf = tb.buf();
        let (epoch, seq_now) = (buf.epoch(), buf.change_seq());
        let doc = Self::doc_key(ed, i);
        // 이름 없는 탭 → 파일로 저장(C-21): 열쇠를 옮긴다(복사 아님).
        //   ★ 09-25 결함: 저장 순간의 `docs` 기록에만 의존해, 재시작·복원 뒤(기록 없음) 파일 탭이 된 스크래치 북마크가 `Scratch{tab}`에
        //   남아 패널엔 보이되 거터(니모닉)엔 안 걸렸다 → 지금 열쇠가 파일이고 저장소에 이 탭의 스크래치 열쇠가 남아 있으면 늘 옮긴다.
        let has_scratch = !self
            .store
            .for_doc(&DocKey::Scratch { tab }, CASE_INSENSITIVE)
            .is_empty();
        if let Some(p) = migrate_target(&doc, self.docs.get(&tab), tab, has_scratch) {
            self.scratch_saved(tab, &p);
        }
        self.docs.insert(tab, doc.clone());
        if self.store.for_doc(&doc, CASE_INSENSITIVE).is_empty() {
            self.seq.insert(tab, (seq_now, epoch));
            return false;
        }
        let mut moved = false;
        match self.seq.get(&tab).copied() {
            Some((seq, ep)) if ep == epoch => {
                if seq == seq_now {
                    return false;
                }
                match buf.changes_since(seq) {
                    Some(it) => {
                        let changes: Vec<_> = it.collect();
                        for c in changes {
                            self.store.apply_change(
                                &doc,
                                c.first,
                                c.removed,
                                c.inserted,
                                CASE_INSENSITIVE,
                            );
                        }
                        moved = true;
                    }
                    None => moved |= self.relocate_doc(ed, i),
                }
            }
            _ => moved |= self.relocate_doc(ed, i),
        }
        self.seq.insert(tab, (seq_now, epoch));
        if moved {
            self.touch();
        }
        moved
    }

    /// L1: 지금 본문으로 문서의 북마크를 다시 찾는다(열 때 · 기록이 끊겼을 때) — 옮긴 것이 있으면 true.
    pub(crate) fn relocate_doc(&mut self, ed: &Editors, i: usize) -> bool {
        let doc = Self::doc_key(ed, i);
        let ids: Vec<u64> = self
            .store
            .for_doc(&doc, CASE_INSENSITIVE)
            .iter()
            .map(|b| b.id)
            .collect();
        if ids.is_empty() {
            return false;
        }
        let lines = Self::lines_of(ed, i);
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let hash_now = if self.opts.max_lines > 0 && refs.len() > self.opts.max_lines {
            0
        } else {
            nsql_bookmarks::doc_hash(&refs)
        };
        let mut moved = false;
        let now = now_epoch();
        for id in ids {
            let Some(b) = self.store.get(id) else {
                continue;
            };
            let anchor = b.anchor.clone();
            let was_live = b.is_live();
            let r = if !was_live
                && !matches!(
                    b.state,
                    State::Invalid {
                        reason: Reason::TextGone | Reason::TooBig,
                        ..
                    }
                ) {
                continue; // 파일 없음 등은 다른 신호(L3)가 되살린다.
            } else {
                relocate(&anchor, &refs, None, &self.opts)
            };
            let Some(b) = self.store.get_mut(id) else {
                continue;
            };
            match r {
                Relocated::Same => {
                    if !was_live {
                        b.state = State::Live;
                        moved = true;
                    }
                    if b.anchor.doc_hash != hash_now {
                        b.anchor =
                            make_anchor(&refs, b.anchor.line as usize, b.anchor.col, &self.opts);
                        b.anchor.doc_hash = hash_now;
                    }
                }
                Relocated::Moved(l) => {
                    b.anchor = make_anchor(&refs, l, b.anchor.col, &self.opts);
                    b.anchor.doc_hash = hash_now;
                    b.state = State::Live;
                    b.shifted = false;
                    moved = true;
                }
                Relocated::TooBig => {
                    if b.anchor.line as usize >= refs.len() {
                        b.state = State::Invalid {
                            since: now,
                            reason: Reason::TooBig,
                        };
                        moved = true;
                    }
                }
                Relocated::Gone => {
                    if was_live {
                        b.state = State::Invalid {
                            since: now,
                            reason: Reason::TextGone,
                        };
                        moved = true;
                    }
                }
            }
        }
        if moved {
            self.touch();
        }
        moved
    }

    /// 탭을 새로 읽었다(열기 · 재로드) → 재탐색 + 기록 시작점.
    pub(crate) fn on_opened(&mut self, ed: &Editors, i: usize) {
        self.relocate_doc(ed, i);
        if let Some(tb) = ed.tab_box(i) {
            let b = tb.buf();
            self.seq.insert(ed.tab_id(i), (b.change_seq(), b.epoch()));
        }
        self.changed = true;
    }

    /// 이름 없는 탭이 파일로 저장됐다(C-21) → 문서 열쇠 이동(복사 아님).
    pub(crate) fn scratch_saved(&mut self, tab: u64, path: &Path) {
        let from = DocKey::Scratch { tab };
        let to = DocKey::file(&path.to_string_lossy());
        let mut any = false;
        for b in self.store.items.iter_mut() {
            if b.doc == from {
                b.doc = to.clone();
                any = true;
            }
        }
        if any {
            self.touch();
        }
    }
}

/// 스크래치 → 파일 열쇠 이전 판정(순수 · 09-25): 지금 열쇠가 파일이고, ① 직전 기록이 이 탭의 스크래치였거나 ② 저장소에 이 탭의
/// 스크래치 북마크가 남아 있으면(재시작·복원 뒤 = 기록 없음) 옮길 경로를 준다.
fn migrate_target(
    doc: &DocKey,
    prev: Option<&DocKey>,
    tab: u64,
    has_scratch: bool,
) -> Option<std::path::PathBuf> {
    let DocKey::File { path } = doc else {
        return None;
    };
    let prev_scratch = matches!(prev, Some(DocKey::Scratch { tab: t }) if *t == tab);
    (prev_scratch || has_scratch).then(|| std::path::PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 작업 모드 셋(사용자 09-23): 폴더 모드 = `<폴더>/.nsql/workspaces/default.nsql-workspace` · 파일 모드에서 새 프로젝트를 만들면
    /// 로컬 북마크는 프로젝트로 **이관**(로컬 파일 = 빈 저장소) · 프로젝트 모드는 파일에 안 쓴다(`path = None`).
    #[test]
    fn folder_mode_path_and_migrate_local_on_new_project() {
        let dir = std::env::temp_dir().join(format!("nsql-bm-modes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let mut bm = Bookmarks::new();
        bm.set_folder(Some(dir.clone()));
        bm.bind_project(None);
        let local = dir
            .join(".nsql")
            .join("workspaces")
            .join("default.nsql-workspace");
        assert_eq!(bm.path.as_deref(), Some(local.as_path()));
        // 북마크 하나 → 저장 → 로컬 파일 생김(폴더 모드 = `.nsql/` 아래).
        let a = make_anchor(&["x", "y"], 0, 0, &RelocateOpts::default());
        bm.store
            .add(
                DocKey::File {
                    path: "C:/x/a.sql".into(),
                },
                a,
                1,
                true,
                100,
                1000,
            )
            .expect("add");
        bm.touch();
        bm.save_now();
        assert!(local.is_file(), "folder mode writes under <dir>/.nsql");
        // 새 프로젝트(파일 모드 → 프로젝트): 이관 = 로컬 파일은 빈 저장소 · 프로젝트 모드는 path = None.
        bm.mark_migrate_local();
        bm.bind_project(Some(Path::new("D:/w/demo.nsql-project")));
        assert!(bm.path.is_none());
        let back = Store::from_json(&std::fs::read_to_string(&local).expect("read")).expect("json");
        assert!(back.items.is_empty(), "local file emptied after migration");
        // 닫기 → 폴더 모드로 복귀 = 빈 로컬 세트(프로젝트 것이 남지 않는다).
        bm.bind_project(None);
        assert_eq!(bm.path.as_deref(), Some(local.as_path()));
        assert!(bm.store.items.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 이름 없는 탭 북마크의 옛 id → 새 id 재매핑(사용자 09-23 검토): 표에 있는 것만 바뀌고 · 파일 열쇠는 그대로.
    #[test]
    /// ★ 09-25: 파일 탭에 이 탭의 스크래치 북마크가 남아 있으면(복원 뒤 기록 없음) 옮긴다 · 직전 기록이 스크래치여도 옮긴다 ·
    /// 스크래치 탭·남은 것 없음 = 안 옮긴다.
    #[test]
    fn scratch_bookmarks_migrate_to_file_key_after_restore() {
        let file = DocKey::File {
            path: "/tmp/s.sql".into(),
        };
        let scratch = DocKey::Scratch { tab: 7 };
        assert!(
            migrate_target(&file, None, 7, true).is_some(),
            "복원 뒤 = 기록 없음 + 남은 스크래치"
        );
        assert!(
            migrate_target(&file, Some(&scratch), 7, false).is_some(),
            "저장 순간(C-21)"
        );
        assert!(migrate_target(&file, None, 7, false).is_none());
        assert!(
            migrate_target(&scratch, None, 7, true).is_none(),
            "아직 스크래치"
        );
        assert!(
            migrate_target(&file, Some(&DocKey::Scratch { tab: 8 }), 7, false).is_none(),
            "다른 탭 기록"
        );
    }

    #[test]
    fn remap_scratch_ids_after_restore() {
        let mut bm = Bookmarks::new();
        let a = make_anchor(&["x", "y"], 0, 0, &RelocateOpts::default());
        let id1 = bm
            .store
            .add(DocKey::Scratch { tab: 5 }, a.clone(), 1, true, 100, 1000)
            .expect("add");
        let id2 = bm
            .store
            .add(
                DocKey::File {
                    path: "C:/x/a.sql".into(),
                },
                a,
                1,
                true,
                100,
                1000,
            )
            .expect("add");
        bm.remap_scratch(&[(5, 42), (9, 43)]);
        assert_eq!(
            bm.store.get(id1).map(|b| b.doc.clone()),
            Some(DocKey::Scratch { tab: 42 })
        );
        assert!(matches!(
            bm.store.get(id2).map(|b| b.doc.clone()),
            Some(DocKey::File { .. })
        ));
    }

    /// 디바운스 마감(사용자 09-23 "값이 바뀌어도 저장 안 됨" · 틱 스케줄러 깨움): 깨끗 = None · 더럽힌 뒤 = +1초 · 계속 더럽히면 처음 +5초 상한.
    #[test]
    fn next_save_at_follows_debounce_and_max_wait() {
        let mut bm = Bookmarks::new();
        let t0 = Instant::now();
        assert_eq!(bm.next_save_at(t0), None);
        bm.touch();
        let d = bm.dirty_since.expect("dirty");
        assert_eq!(bm.next_save_at(t0), Some(d + SAVE_DEBOUNCE));
        // 4.5초 동안 계속 더럽혔다고 치면(dirty_since만 뒤로) 상한 = first_dirty + 5초가 먼저.
        bm.dirty_since = Some(d + Duration::from_millis(4500));
        assert_eq!(bm.next_save_at(t0), Some(d + SAVE_MAX_WAIT));
    }

    /// 파일·폴더 모드의 이름 없는 탭 북마크 = 메모리 전용(사용자 09-23): 로컬 파일에는 안 쓰고 · 다시 읽으면 남지 않으며 ·
    /// 프로젝트에 담는 `store.to_json()`에는 그대로 있다.
    #[test]
    fn scratch_bookmarks_stay_in_memory_in_file_mode() {
        let dir = std::env::temp_dir().join(format!("nsql-bm-scratch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let mut bm = Bookmarks::new();
        bm.set_folder(Some(dir.clone()));
        bm.bind_project(None);
        let a = make_anchor(&["x", "y"], 0, 0, &RelocateOpts::default());
        bm.store
            .add(DocKey::Scratch { tab: 3 }, a.clone(), 1, true, 100, 1000)
            .expect("add");
        bm.store
            .add(
                DocKey::File {
                    path: "C:/x/a.sql".into(),
                },
                a,
                1,
                true,
                100,
                1000,
            )
            .expect("add");
        assert!(
            bm.store.to_json().contains("\"tab\": 3"),
            "project embedding keeps scratch"
        );
        bm.touch();
        bm.save_now();
        let local = dir
            .join(".nsql")
            .join("workspaces")
            .join("default.nsql-workspace");
        let back = Store::from_json(&std::fs::read_to_string(&local).expect("read")).expect("json");
        assert_eq!(back.items.len(), 1, "local file has only the file bookmark");
        assert!(matches!(back.items[0].doc, DocKey::File { .. }));
        assert_eq!(bm.store.items.len(), 2, "memory still has both");
        bm.bind_project(None);
        assert_eq!(bm.store.items.len(), 1, "reload = scratch gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
