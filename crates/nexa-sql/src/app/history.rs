//! App — 로컬 히스토리 동작(C-1 · docs/19 §3 · 11-06): 파일 저장 뒤 시점 기록(`save_to` 성공 경로) · 명령 `edit.history` = 활성 탭의
//! 시점 목록(팔레트 · 최신 먼저) → 고르면 2-pane diff 탭(왼쪽 = 현재 버퍼 · 오른쪽 = 시점 · `open_diff_tab` 공용).
//! ★ O-1(11-07 · 19 §3 · D-378): **오브젝트 소스 탭**도 같은 저장소 — 열 때 서버 원본 = `open` · F5 직전 버퍼 = `compile` · 항목 =
//! `<프로필>_<스키마>.<객체>_<종류>`(backups와 같은 줄기) · 명령 `edit.history_restore` = 시점 본문을 **편집기 탭에 넣기**(사용자 11-06 결정 =
//! 바로 컴파일하지 않고 사용자가 F5 · 복원도 `restore` 시점으로 남긴다). 컴파일 성공 뒤 서버 재읽기(`server`)는 후속 조각.

use crate::*;

use crate::history::{self as store, HistReq, Tag};

/// 시점 종류 라벨.
fn tag_label(tag: Tag) -> &'static str {
    t(match tag {
        Tag::Save => Msg::HistTagSave,
        Tag::Open => Msg::HistTagOpen,
        Tag::Compile => Msg::HistTagCompile,
        Tag::Server => Msg::HistTagServer,
        Tag::Restore => Msg::HistTagRestore,
        Tag::External => Msg::HistTagExternal,
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

    /// ★ T-513(11-23) 서버 소스 요청: 오브젝트 탭 `tab`의 객체 소스를 서버에서 다시 받는다 — **표식(token)을 실어** 보내고 응답·실패가
    ///   그 표식을 들고 돌아온다(`OpenSql { token }` · `SourceError(token)` → [`App::history_response`] · [`App::history_req_fail`]).
    ///   종전 = 대기 칸 하나 + 객체 이름 비교 + 시간 만료로 "다음 소스 응답은 내 것"을 추측했다(독립 검토 4회 연속 지적:
    ///   요청이 겹치면 덮어씀 · 답이 늦으면 중복 탭 · 실패하면 다음 열기를 삼킴 · 다른 서버의 같은 이름과 섞임).
    ///   `check` = 서버 변경 확인(`edit.history_check` · 자동 확인) · 아니면 컴파일 뒤 `server` 시점 저장. 탭은 건드리지 않는다.
    ///   반환 = 보냈는가(탐색기가 오프라인이거나 그 칸에 다른 소스 요청이 진행 중이면 `false` — 아무것도 남기지 않는다).
    pub(crate) fn history_server_request(&mut self, tab: u64, check: bool, auto: bool) -> bool {
        if !self.settings.flag("history.enabled") {
            return false;
        }
        let Some(o) = self.object_tabs.get(&tab).cloned() else {
            return false;
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
        self.history_reqs_expire();
        self.history_token += 1;
        let token = self.history_token;
        if !self
            .explorer
            .reload_source_tok(o.server.as_ref(), &info, o.kind, token)
        {
            return false;
        }
        // 항목 폴더는 **지금** 정한다(요청을 건 순간의 세션 프로필) — 응답 때는 활성 탭이 달라져 있을 수 있다.
        let dir = store::object_dir(&self.history_object_stem(&o));
        self.history_reqs.push(HistReq {
            token,
            tab,
            origin: o,
            dir,
            check,
            auto,
            at: Instant::now(),
        });
        true
    }

    /// 답이 오지 않은 요청 기록을 걷는다(탐색기 소스 타임아웃 + 여유 · 최소 30초). 걷힌 뒤 늦게 온 답은 표식을 못 찾아
    /// **조용히 버려진다**(탭으로 열리지 않는다) — 기록이 남아 있어도 다른 요청을 막지 않는다(막는 것은 탐색기 칸의 상태뿐).
    fn history_reqs_expire(&mut self) {
        let limit = 30.max(self.settings.int("explorer.timeout").max(0) as u64 + 15);
        self.history_reqs
            .retain(|r| r.at.elapsed().as_secs() < limit);
    }

    /// 표식 `token`의 요청이 실패로 끝났다(응답 오류 · 끊김 · 재결합 · 세대 바뀜) — 기록을 지운다(안내는 탐색기의 `Fail`이 한다).
    pub(crate) fn history_req_fail(&mut self, token: u64) {
        self.history_reqs.retain(|r| r.token != token);
    }

    /// 표식이 달린 소스 응답(meta.rs `OpenSql { token != 0 }`) — 그 요청의 용도대로 처리한다. 모르는 표식(만료된 요청의 늦은 답)은
    /// 버린다.
    pub(crate) fn history_response(&mut self, token: u64, text: &str) {
        let Some(i) = self.history_reqs.iter().position(|r| r.token == token) else {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("history: late source response dropped (request {token})"),
            ));
            return;
        };
        let req = self.history_reqs.remove(i);
        if req.check {
            self.history_check_done(&req, text);
            return;
        }
        // 컴파일 뒤 `server` 시점: 같은 내용이면 저장이 생략되므로(최신과 같은 해시) 그 최신 버퍼 시점을 `server`로 다시
        //   표시해 서버 기준을 남긴다.
        if let Some(dir) = &req.dir {
            self.history_store_in(dir, Tag::Server, text);
            store::mark_newest_server_in(dir, store::content_hash(text));
        }
        // 요청 때의 "소스 읽는 중…" 상태줄을 갈아 끼운다(협업 V1 11-10 "4초 잔류").
        self.sess.status = t(Msg::StHistoryServerSaved).into();
    }

    /// ★ O-1 외부 변경 확인(명령 `edit.history_check` · 19 §3 · 11-22): 활성 오브젝트 탭의 객체 소스를 서버에서 다시 받아
    ///   **마지막 서버 본**(열기 · 컴파일 뒤 서버 본 · 앞선 외부 변경)과 해시를 견준다 — 탭은 건드리지 않는다.
    pub(crate) fn history_check(&mut self) {
        if !self.settings.flag("history.enabled") {
            self.sess.status = t(Msg::StHistoryOff).into();
            return;
        }
        let tab = self.editors.active_id();
        if !self.object_tabs.contains_key(&tab) {
            self.sess.status = t(Msg::StHistoryCheckNoObject).into();
            return;
        }
        if !self.history_server_request(tab, true, false) {
            // 보내지 못했다 = 탐색기가 오프라인이거나 그 칸에 다른 소스 요청이 진행 중.
            let offline = self
                .object_tabs
                .get(&tab)
                .is_some_and(|o| self.explorer.source_ready_for(o.server.as_ref()).1);
            self.sess.status = t(if offline {
                Msg::ExpNotConnected
            } else {
                Msg::StHistoryCheckBusy
            })
            .into();
        }
    }

    /// ★ O-1 자동 확인(D-499 · 사용자 11-23 "탭 활성화 때 최소 간격으로 · 기본 끔"): 오브젝트 탭이 **활성화될 때** 그 탭의 마지막
    ///   확인에서 `history.server_check_secs`초(0 = 끔 = 기본)가 지났으면 서버 변경 확인을 한 번 건다 — 주기 폴링이 아니다
    ///   (보고 있지 않은 탭에는 조회가 나가지 않는다 · 26 §8). 처음 보는 탭(방금 연 것)은 시각만 적고 확인하지 않는다. 실행 중이면
    ///   건너뛴다. 바뀌지 않았으면 조용히 끝난다(상태줄을 비운다). `about_to_wait`마다 불리지만 활성 탭이 바뀐 순간에만 일한다.
    pub(crate) fn history_auto_check(&mut self) {
        let tab = self.editors.active_id();
        if tab == self.history_auto_tab {
            return;
        }
        self.history_auto_tab = tab;
        self.history_reqs_expire();
        let secs = self.settings.int("history.server_check_secs").max(0) as u64;
        if secs == 0 || !self.settings.flag("history.enabled") {
            return;
        }
        if !self.object_tabs.contains_key(&tab) || self.sess.busy {
            return;
        }
        let now = Instant::now();
        self.history_auto_seen
            .retain(|(id, _)| self.object_tabs.contains_key(id));
        match self.history_auto_seen.iter().find(|(id, _)| *id == tab) {
            None => {
                self.history_auto_seen.push((tab, now));
                return;
            }
            Some((_, at)) => {
                if now.duration_since(*at).as_secs() < secs {
                    return;
                }
            }
        }
        // 보냈을 때만 간격을 소비한다(못 보냈으면 — 오프라인 · 다른 소스 요청 중 — 다음 탭 활성화 때 다시 시도).
        if self.history_server_request(tab, true, true) {
            if let Some((_, at)) = self.history_auto_seen.iter_mut().find(|(id, _)| *id == tab) {
                *at = now;
            }
        }
    }

    /// 외부 변경 확인의 응답 처리 — 달라졌으면 `external` 시점 저장 + 상태줄·로그·알림 카드 · 같으면 상태줄만(자동 확인은 조용히).
    fn history_check_done(&mut self, req: &HistReq, text: &str) {
        let (o, auto) = (&req.origin, req.auto);
        let Some(dir) = req.dir.clone() else {
            return;
        };
        // ★ 가장 최신 시점이 **버퍼 시점**(컴파일 직전 · 복원)이고 서버 글과 같으면 = 내가 컴파일한 것이 서버에 그대로 있다
        //   (컴파일 뒤 서버 본 저장이 건너뛰어졌거나 실패한 경우) → 그 시점을 `server`로 표시해 기준으로 삼는다(검토 11-23 ·
        //   외부 변경으로 알리지 않는다 · 같은 내용은 새로 저장되지 않으므로 표시해 두지 않으면 확인할 때마다 되풀이된다).
        let hash = store::content_hash(text);
        store::mark_newest_in(&dir, hash, Tag::Server);
        let verdict = store::server_check(&store::list_in(&dir), hash);
        let name = format!("{}.{}", o.schema, o.name);
        match verdict {
            // 자동 확인(D-499)은 바뀌지 않았으면 조용히 — 요청 때의 "소스 읽는 중…" 상태줄만 비운다.
            store::ServerCheck::Unchanged(_) if auto => self.sess.status = String::new(),
            store::ServerCheck::Unchanged(when) => {
                self.sess.status = tf(Msg::StHistoryCheckSame, &[&name, &when]);
            }
            store::ServerCheck::NoBaseline => {
                self.history_store_in(&dir, Tag::Open, text);
                self.sess.status = if auto {
                    String::new()
                } else {
                    tf(Msg::StHistoryCheckFirst, &[&name])
                };
            }
            store::ServerCheck::Changed(when) => {
                self.history_store_in(&dir, Tag::External, text);
                let msg = tf(Msg::StHistoryCheckChanged, &[&name, &when]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, format!("history: {msg}")));
                // ★ 알림 카드(O-1 "변경 알림 + 비교 바로 열기"): 누르면 **요청을 건 그 탭**으로 가서 방금 저장한 서버 본과
                //   편집기를 비교한다(같은 해시의 external 시점 · 탭이 그새 닫혔으면 카드 없이 상태줄·로그만).
                let snap = store::list_in(&dir)
                    .into_iter()
                    .find(|e| e.tag == Tag::External && e.hash == hash);
                if let (Some(e), true) = (snap, self.object_tabs.contains_key(&req.tab)) {
                    self.toasts.push_action(
                        toast::ToastKind::Info,
                        name,
                        format!("{msg}\n{}", t(Msg::HistToastCompare)),
                        &format!("histobj:{}|{}", req.tab, e.path.to_string_lossy()),
                    );
                }
                self.sess.status = msg;
            }
        }
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
        // ★ 컴파일 뒤 서버 본이 직전 버퍼 시점과 같으면 저장이 생략된다(같은 내용은 안 쓴다) → 그 최신 시점을 `server`로
        //   다시 표시해 **서버 기준**을 남긴다(검토 11-23): 안 남기면 다음 확인이 옛 `open`과 견줘 자기 컴파일을 외부 변경으로 알린다.
        if tag == Tag::Server {
            store::mark_newest_server_in(&dir, store::content_hash(text));
        }
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

    /// ★ O-1 명령 `edit.history_pin`: 같은 목록 → `hpin:<경로>` → 그 시점의 고정을 켜거나 끈다(고정 = 상한 정리에서 보존 · 19 §3).
    pub(crate) fn history_pin_pick(&mut self) {
        self.history_list_palette("hpin:");
    }

    /// 팔레트 선택 `hpin:<시점 파일 경로>`: 고정 토글(파일 이름만 바뀐다) · 상태줄 안내 · 실패 = 로그 + 상태줄.
    pub(crate) fn history_pin(&mut self, id: &str) {
        let Some(p) = id.strip_prefix("hpin:") else {
            return;
        };
        let p = PathBuf::from(p);
        let when = store::entry_of(&p).map(|e| e.when()).unwrap_or_default();
        match store::toggle_pin(&p) {
            Ok((_, on)) => {
                let m = if on {
                    Msg::StHistoryPinned
                } else {
                    Msg::StHistoryUnpinned
                };
                self.sess.status = tf(m, &[&when]);
            }
            Err(e) => {
                self.log_win
                    .push(LogEntry::new(LogKind::Info, format!("history: {e}")));
                self.sess.status = t(Msg::StHistoryNone).into();
            }
        }
        self.redraw();
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
