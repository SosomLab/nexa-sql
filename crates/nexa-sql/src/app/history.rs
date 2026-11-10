//! App — 로컬 히스토리 동작(C-1 · docs/19 §3 · 11-06): 파일 저장 뒤 시점 기록(`save_to` 성공 경로) · 명령 `edit.history` = 활성 탭의
//! 시점 목록(팔레트 · 최신 먼저) → 고르면 2-pane diff 탭(왼쪽 = 현재 버퍼 · 오른쪽 = 시점 · `open_diff_tab` 공용).
//! ★ O-1(11-07 · 19 §3 · D-378): **오브젝트 소스 탭**도 같은 저장소 — 열 때 서버 원본 = `open` · F5 직전 버퍼 = `compile` · 항목 =
//! `<프로필>_<스키마>.<객체>_<종류>`(backups와 같은 줄기) · 명령 `edit.history_restore` = 시점 본문을 **편집기 탭에 넣기**(사용자 11-06 결정 =
//! 바로 컴파일하지 않고 사용자가 F5 · 복원도 `restore` 시점으로 남긴다). 컴파일 성공 뒤 서버 재읽기(`server`)는 후속 조각.

use crate::*;

use crate::history::{self as store, Tag};

/// 시점 종류 라벨.
fn tag_label(tag: Tag) -> &'static str {
    t(match tag {
        Tag::Save => Msg::HistTagSave,
        Tag::Open => Msg::HistTagOpen,
        Tag::Compile => Msg::HistTagCompile,
        Tag::Server => Msg::HistTagServer,
        Tag::Restore => Msg::HistTagRestore,
    })
}

/// 크기 라벨(`1.2 KB` · `3.4 MB` · 작으면 바이트).
fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

impl App {
    /// 설정 상한 `(항목당 수, 전체 바이트)`.
    fn history_limits(&self) -> (usize, u64) {
        let keep = self.settings.int("history.max_per_item").max(1) as usize;
        let mb = self.settings.int("history.max_mb").max(1) as u64;
        (keep, mb * 1024 * 1024)
    }

    /// 저장 직후(`save_to` 성공 경로): 설정 켬 · 대형 파일 아님 → 시점 기록(같은 내용이면 건너뜀) · 새로 썼으면 전체 상한.
    pub(crate) fn history_on_save(&mut self, path: &Path) {
        if !self.settings.flag("history.enabled") {
            return;
        }
        let i = self.editors.active();
        if self.editors.is_large(i) {
            return;
        }
        let Some(dir) = store::file_dir(path) else {
            return;
        };
        let text = self.editors.cur().text();
        let (keep, max_bytes) = self.history_limits();
        match store::store_in(&dir, &text, Tag::Save, keep) {
            Ok(Some(_)) => {
                if let Some(root) = store::root() {
                    store::prune_total_in(&root, max_bytes);
                }
            }
            Ok(None) => {}
            Err(e) => self
                .log_win
                .push(LogEntry::new(LogKind::Info, format!("history: {e}"))),
        }
    }

    /// 오브젝트 탭의 히스토리 항목 줄기(`<프로필>_<스키마>.<객체>_<종류>` · backups와 같은 규칙 · 프로필 = 탭이 묶인 세션의 것).
    fn history_object_stem(&self, o: &explorer::ObjectOrigin) -> String {
        let info = nsql_catalog::ObjectInfo {
            db: String::new(),
            schema: o.schema.clone(),
            name: o.name.clone(),
            kind: o.kind,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        };
        crate::backups::object_stem(&self.sess.profile, &info)
    }

    /// ★ O-1 `server` 시점(19 §3 (c) · 11-10): 오브젝트 탭 `tab`의 객체 소스를 서버에서 다시 받아 **히스토리에만** 넣는다 —
    ///   탭은 건드리지 않는다(응답은 meta.rs `OpenSql`이 `history_server_wait`로 가로챔 · 같은 해시면 저장 규칙대로 생략).
    pub(crate) fn history_server_request(&mut self, tab: u64) {
        if !self.settings.flag("history.enabled") {
            return;
        }
        let Some(o) = self.object_tabs.get(&tab).cloned() else {
            return;
        };
        let info = nsql_catalog::ObjectInfo {
            db: String::new(),
            schema: o.schema.clone(),
            name: o.name.clone(),
            kind: if o.kind == nsql_catalog::ObjectKind::PackageBody {
                nsql_catalog::ObjectKind::Package
            } else {
                o.kind
            },
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        };
        let kind = o.kind;
        let server = o.server.clone();
        self.history_server_wait = Some(o);
        self.explorer
            .reload_source_for(server.as_ref(), &info, kind);
    }

    /// 활성 탭의 히스토리 폴더 — 오브젝트 소스 탭(출처 있음)이면 객체 항목 · 아니면 파일 항목 · 둘 다 아니면 None.
    fn history_dir_active(&self) -> Option<PathBuf> {
        let tab = self.editors.active_id();
        if let Some(o) = self.object_tabs.get(&tab) {
            return store::object_dir(&self.history_object_stem(o));
        }
        self.editors.active_path().and_then(|p| store::file_dir(&p))
    }

    /// ★ O-1: 오브젝트 소스 탭의 시점 기록(열기 = 서버 원본 · 컴파일 직전 = 버퍼 · 복원 = 넣은 본문) — 같은 내용이면 건너뜀 · 상한.
    pub(crate) fn history_object_store(
        &mut self,
        o: &explorer::ObjectOrigin,
        tag: Tag,
        text: &str,
    ) {
        if !self.settings.flag("history.enabled") {
            return;
        }
        let Some(dir) = store::object_dir(&self.history_object_stem(o)) else {
            return;
        };
        self.history_store_in(&dir, tag, text);
    }

    /// 시점 하나 기록(공용): 같은 해시면 안 쓰고 · 새로 썼으면 전체 상한 · 실패는 로그 한 줄.
    fn history_store_in(&mut self, dir: &Path, tag: Tag, text: &str) {
        let (keep, max_bytes) = self.history_limits();
        match store::store_in(dir, text, tag, keep) {
            Ok(Some(_)) => {
                if let Some(root) = store::root() {
                    store::prune_total_in(&root, max_bytes);
                }
            }
            Ok(None) => {}
            Err(e) => self
                .log_win
                .push(LogEntry::new(LogKind::Info, format!("history: {e}"))),
        }
    }

    /// 활성 탭의 시점 목록을 팔레트로(접두 `hist:` = 비교 · `hrst:` = 복원). 비어 있으면 상태줄 안내.
    fn history_list_palette(&mut self, prefix: &str) {
        if !self.settings.flag("history.enabled") {
            self.sess.status = t(Msg::StHistoryOff).into();
            self.redraw();
            return;
        }
        let list = self
            .history_dir_active()
            .map(|d| store::list_in(&d))
            .unwrap_or_default();
        if list.is_empty() {
            self.sess.status = t(Msg::StHistoryNone).into();
            self.redraw();
            return;
        }
        let items: Vec<(String, String)> = list
            .iter()
            .map(|e| {
                (
                    format!("{prefix}{}", e.path.display()),
                    format!(
                        "{}  ·  {}  ·  {}{}",
                        e.when(),
                        tag_label(e.tag),
                        size_label(e.bytes),
                        if e.pinned { "  ·  📌" } else { "" }
                    ),
                )
            })
            .collect();
        self.palette.set_commands(items);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 명령 `edit.history`: 활성 탭(파일 · 오브젝트 소스)의 시점 목록(최신 먼저) → 팔레트 `hist:<시점 파일 경로>` → 비교.
    pub(crate) fn history_pick(&mut self) {
        self.history_list_palette("hist:");
    }

    /// ★ O-1 명령 `edit.history_restore`: 같은 목록 → `hrst:<경로>` → 시점 본문을 **활성 탭에 넣는다**(서버에는 아무것도 안 보냄).
    pub(crate) fn history_restore_pick(&mut self) {
        self.history_list_palette("hrst:");
    }

    /// 팔레트 선택 `hrst:<시점 파일 경로>`: 활성 탭 본문 = 시점 본문(사용자 11-06 결정 = 편집기로 복원 · 확인 뒤 F5로 컴파일) · 복원도
    /// `restore` 시점으로 남긴다(19 §3 "복원 자체도 시점") · 상태줄 안내.
    pub(crate) fn history_restore(&mut self, id: &str) {
        let Some(p) = id.strip_prefix("hrst:") else {
            return;
        };
        let p = PathBuf::from(p);
        let (Some(entry), Ok(snap)) = (store::entry_of(&p), std::fs::read_to_string(&p)) else {
            self.sess.status = t(Msg::StHistoryNone).into();
            self.redraw();
            return;
        };
        if self.editors.is_view_tab_active() {
            self.sess.status = t(Msg::StHistoryNone).into();
            self.redraw();
            return;
        }
        self.editors.cur_mut().set_text(&snap);
        self.editors.cur_mut().goto_line(1);
        let tab = self.editors.active_id();
        if let Some(o) = self.object_tabs.get(&tab).cloned() {
            self.history_object_store(&o, Tag::Restore, &snap);
        } else if let Some(dir) = self.editors.active_path().and_then(|p| store::file_dir(&p)) {
            if self.settings.flag("history.enabled") {
                self.history_store_in(&dir, Tag::Restore, &snap);
            }
        }
        self.sess.status = tf(Msg::StHistoryRestored, &[&entry.when()]);
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// 팔레트 선택 `hist:<시점 파일 경로>` → diff 탭(왼쪽 = 현재 버퍼 · 오른쪽 = 시점 · 공백 무시 = `compare.ignore_ws`).
    pub(crate) fn history_open(&mut self, id: &str) {
        let Some(p) = id.strip_prefix("hist:") else {
            return;
        };
        let p = PathBuf::from(p);
        let (Some(entry), Ok(snap)) = (store::entry_of(&p), std::fs::read_to_string(&p)) else {
            self.sess.status = t(Msg::StHistoryNone).into();
            self.redraw();
            return;
        };
        let title = self.editors.active_title();
        let tab = self.editors.active_id();
        let when = format!("{} · {}", entry.when(), tag_label(entry.tag));
        let cur = self.editors.cur().text();
        let ignore_ws = self.settings.flag("compare.ignore_ws");
        let (a, a_show): (Vec<String>, Vec<String>) = app::compare::buffer_pairs(&cur, ignore_ws)
            .into_iter()
            .unzip();
        let (b, b_show): (Vec<String>, Vec<String>) = app::compare::buffer_pairs(&snap, ignore_ws)
            .into_iter()
            .unzip();
        let obj = format!("{title} ↔ {when}");
        let key = format!("compare:hist:{tab}:{}", entry.stamp);
        let tab_title = format!("⇄ {title} @ {}", entry.when());
        self.open_diff_tab(
            &key,
            &tab_title,
            &a,
            &b,
            &a_show,
            &b_show,
            &obj,
            (t(Msg::CompareLabelCurrent).to_string(), when),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::size_label;

    #[test]
    fn size_label_units() {
        assert_eq!(size_label(12), "12 B");
        assert_eq!(size_label(1536), "1.5 KB");
        assert_eq!(size_label(3 * 1024 * 1024), "3.0 MB");
    }
}
