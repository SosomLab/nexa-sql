//! App — 로컬 히스토리 동작(C-1 · docs/19 §3 · 11-06): 파일 저장 뒤 시점 기록(`save_to` 성공 경로) · 명령 `edit.history` = 활성 파일
//! 탭의 시점 목록(팔레트 · 최신 먼저) → 고르면 2-pane diff 탭(왼쪽 = 현재 버퍼 · 오른쪽 = 시점 · `open_diff_tab` 공용).
//! 오브젝트 시점(열기/컴파일/서버 본 · 복원)은 O-1에서 같은 저장소(`crate::history`)에 얹는다.

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

    /// 명령 `edit.history`: 활성 파일 탭의 시점 목록(최신 먼저) → 팔레트 `hist:<시점 파일 경로>`.
    pub(crate) fn history_pick(&mut self) {
        if !self.settings.flag("history.enabled") {
            self.sess.status = t(Msg::StHistoryOff).into();
            self.redraw();
            return;
        }
        let list = self
            .editors
            .active_path()
            .and_then(|p| store::file_dir(&p))
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
                    format!("hist:{}", e.path.display()),
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
