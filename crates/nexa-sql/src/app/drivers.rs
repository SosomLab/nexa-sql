//! ★ 드라이버 관리자(docs/22 §5-2 · T-29 2차): 창([`crate::driver_win`])의 데이터·동작 = 색인(캐시 → [새로 고침]) · 설치본 목록 ·
//! 설치/갱신(워커 스레드 · 진행은 상태줄) · 삭제 · 확인. CLI `nsql driver`와 같은 `nsql_drivers::ext` 코어를 쓴다.
//!
//! 네트워크(26 §8): 창 열기 = 캐시만 · [새로 고침] = 조건부 GET 1 · [설치] = 자산 3 GET — 전부 사용자 동작 · 동시 1(작업 중이면 버튼 잠금).
use crate::driver_win::{build_rows, DriverRow, DriverWinAction};
use crate::{theme, App, Wake};
use nsql_drivers::ext::install::{
    self, Index, InstallError, InstallErrorKind, InstallReport, Source,
};
use nsql_drivers::ext::{self, keys::DRIVER_KEYS};
use nsql_i18n::{t, tf, Msg};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use winit::event_loop::ActiveEventLoop;

/// 워커 → UI.
pub(crate) enum DriverMsg {
    Progress(String, u64, Option<u64>),
    Installed(Result<InstallReport, InstallError>),
    Index(Result<(Index, bool), InstallError>),
    /// 오프라인 번들 설치 결과(자산별).
    Bundle(Result<install::BundleOutcome, InstallError>),
}

/// 호스트가 드는 드라이버 관리 상태.
#[derive(Default)]
pub(crate) struct DriverTask {
    rx: Option<Receiver<DriverMsg>>,
    /// 마지막으로 읽은 색인(캐시 또는 새로 고침).
    index: Option<Index>,
    index_loaded: bool,
    /// 기동 때의 갱신 확인(`driver.check_updates`)이면 색인 결과로 갱신 수를 상태줄에 알린다.
    badge_check: bool,
    /// ★ 설치 대기열(사용자 10-27 다중 선택·[갱신] 버튼): 워커는 한 번에 하나 — 끝나면 다음을 띄운다. `batch` = (끝난 수, 실패 수, 전체).
    queue: std::collections::VecDeque<String>,
    batch: (usize, usize, usize),
}

/// ★ 대기열의 다음 설치(순수 · 10-27): 끝난 작업을 세고(`ok`) 다음 id를 꺼낸다. 돌려주는 값 = (다음 id, 이번이 몇 번째인지 1부터, 전체).
pub(crate) fn batch_next(
    queue: &mut std::collections::VecDeque<String>,
    batch: &mut (usize, usize, usize),
    ok: bool,
) -> Option<(String, usize, usize)> {
    batch.0 += 1;
    if !ok {
        batch.1 += 1;
    }
    let next = queue.pop_front()?;
    Some((next, batch.0 + 1, batch.2))
}

/// 설치 묶음의 id 정리(순수): 빈 것 제거 · 순서 유지 중복 제거.
pub(crate) fn dedup_ids(ids: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !id.is_empty() && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// 토스트 동작 id([갱신] 버튼 · `menu_action`으로 온다).
pub(crate) const UPDATE_INSTALLED_ACTION: &str = "drivers.update_installed";

impl std::fmt::Debug for DriverTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriverTask")
            .field("busy", &self.rx.is_some())
            .field("index", &self.index.as_ref().map(|i| i.drivers.len()))
            .finish()
    }
}

impl DriverTask {
    pub(crate) fn busy(&self) -> bool {
        self.rx.is_some()
    }
}

impl App {
    fn driver_source(&self) -> Source {
        Source::from_settings(
            self.settings.get("driver.repo"),
            self.settings.int("driver.http_timeout_secs"),
        )
    }

    fn driver_root(&self) -> Option<PathBuf> {
        ext::driver_dir()
    }

    /// 창 열기(메뉴 View ▸ Drivers · 열려 있으면 앞으로) — 색인은 **캐시만** 읽는다(네트워크 0).
    pub(crate) fn open_drivers_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.driver_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
        if !self.driver_task.index_loaded {
            self.driver_task.index_loaded = true;
            if let Some(root) = self.driver_root() {
                self.driver_task.index = install::cached_index(&root);
            }
        }
        self.drivers_idle_status();
    }

    /// 표의 줄 = 색인 + 설치본(그릴 때마다 · 폴더 읽기 한 번 · 창이 열려 있을 때만 불린다).
    /// 설치·색인 오류 = 종류별 짧은 Msg를 상태줄에 · 영어 상세는 로그 창에(사용자 10-23 "다국어 전반 보완").
    fn driver_install_error(&mut self, e: &InstallError) {
        let [a, b] = e.args();
        let text = tf(nsql_i18n::driver_error_msg(e.kind.code()), &[&a, &b]);
        self.driver_win.set_status(&text, false);
        self.log_win.push(crate::LogEntry::new(
            crate::LogKind::Error,
            format!("driver install: {} ({})", e.detail, e.kind.code()),
        ));
    }

    pub(crate) fn driver_rows(&self) -> Vec<DriverRow> {
        let installed = self
            .driver_root()
            .map(|r| ext::list_extensions_in(&r))
            .unwrap_or_default();
        build_rows(
            self.driver_task.index.as_ref(),
            &installed,
            &install::platform(),
            nsql_drivers::builtin_version,
        )
    }

    /// 쉬는 상태줄 = 드라이버 폴더 · 색인 나이.
    fn drivers_idle_status(&mut self) {
        let Some(root) = self.driver_root() else {
            self.driver_win.set_status(t(Msg::DrvStatusNoFolder), false);
            return;
        };
        let age = install::cached_index_age(&root).map_or_else(
            || t(Msg::DrvIndexNone).to_string(),
            |a| tf(Msg::DrvIndexAge, &[&(a.as_secs() / 60).to_string()]),
        );
        let text = tf(Msg::DrvStatusIdle, &[&root.display().to_string(), &age]);
        self.driver_win.set_status(&text, false);
    }

    /// ★ 설치·삭제 뒤 설정 창(DBMS ▸ 드라이버 · DBMS별 드라이버 행)의 읽기 전용 정보를 바로 맞춘다(다운로드 전/후 상태 · 사용자 10-23).
    pub(crate) fn drivers_info_changed(&mut self) {
        if self.prefs_win.window().is_some() {
            self.prefs_win.set_info(crate::dbms_info_values());
            self.prefs_win.refresh(&self.settings);
            self.prefs_win.redraw();
        }
    }

    pub(crate) fn drivers_apply(&mut self, a: DriverWinAction) {
        match a {
            DriverWinAction::None | DriverWinAction::Paint => {}
            DriverWinAction::Refresh => self.drivers_spawn(None),
            DriverWinAction::InstallFile => {
                // 파일 대화상자(열기) → 고르면 `drivers_install_path`.
                self.file_purpose = crate::FilePurpose::DriverPackage;
                self.open_file_dlg = Some(crate::PickerMode::Open);
                self.redraw();
            }
            DriverWinAction::Install(ids) => self.drivers_install_many(ids),
            DriverWinAction::Remove(ids) => {
                // 올라간 판도 지운다(파일은 .trash → 다음 시작) — 재시작까지 그 판이 쓰인다고 안내(사용자 10-23) · 여러 개 = 줄마다 결과를 이어 붙인다.
                let mut texts = Vec::new();
                for id in dedup_ids(ids) {
                    let loaded = ext::is_loaded(&id);
                    texts.push(match ext::remove_extension(&id, None) {
                        Ok(paths) if paths.is_empty() => tf(Msg::DrvStatusNothingToRemove, &[&id]),
                        Ok(_) if loaded => tf(Msg::DrvStatusRemovedRestart, &[&id]),
                        Ok(paths) => tf(Msg::DrvStatusRemoved, &[&id, &paths.len().to_string()]),
                        Err(e) => tf(Msg::DrvStatusError, &[&e]),
                    });
                }
                self.driver_win.set_status(&texts.join(" · "), false);
                self.drivers_info_changed();
            }
            DriverWinAction::Check(ids) => {
                let ids = dedup_ids(ids);
                let mut ok = 0usize;
                let mut texts = Vec::new();
                for id in &ids {
                    texts.push(match ext::check_extension(id) {
                        Ok(c) => {
                            ok += 1;
                            tf(
                                Msg::DrvStatusCheckOk,
                                &[&c.driver_id, &c.version, &c.dialect, &c.abi.to_string()],
                            )
                        }
                        Err(e) => tf(Msg::DrvStatusError, &[&e.message]),
                    });
                }
                let text = if ids.len() > 1 {
                    // 여러 개 = 요약을 상태줄에 · 줄마다 결과는 로그 창에.
                    for t in &texts {
                        self.log_win
                            .push(crate::LogEntry::new(crate::LogKind::Info, t.clone()));
                    }
                    tf(
                        Msg::DrvStatusCheckMany,
                        &[
                            &ids.len().to_string(),
                            &ok.to_string(),
                            &(ids.len() - ok).to_string(),
                        ],
                    )
                } else {
                    texts.pop().unwrap_or_default()
                };
                self.driver_win.set_status(&text, false);
            }
        }
    }

    /// ★ 여러 드라이버 설치/갱신(사용자 10-27): 워커는 하나씩 — 첫 것을 띄우고 나머지는 대기열([`batch_next`]) · 끝나면 요약 한 줄.
    pub(crate) fn drivers_install_many(&mut self, ids: Vec<String>) {
        let mut ids = dedup_ids(ids);
        if ids.is_empty() || self.driver_task.busy() {
            return;
        }
        let total = ids.len();
        let first = ids.remove(0);
        self.driver_task.queue = ids.into_iter().collect();
        self.driver_task.batch = (0, 0, total);
        self.drivers_spawn(Some(first.clone()));
        if total > 1 {
            self.driver_win.set_status(
                &tf(Msg::DrvStatusQueued, &[&first, "1", &total.to_string()]),
                true,
            );
        }
    }

    /// ★ [갱신] 버튼(토스트 · 메뉴 동작 `drivers.update_installed` · 10-27): 범위 = **설치된 드라이버 중 색인에 새 버전이 있는 것만**
    ///   (사용자 "현재 필요한 드라이버로 축소") · 색인은 캐시(기동 확인이 막 받아 둔 것) · 네트워크는 설치 자산만.
    pub(crate) fn drivers_update_installed(&mut self) {
        self.toasts.dismiss_action(UPDATE_INSTALLED_ACTION);
        let Some(root) = self.driver_root() else {
            self.sess.status = t(Msg::DrvStatusNoFolder).into();
            return;
        };
        if self.driver_task.index.is_none() {
            self.driver_task.index = install::cached_index(&root);
            self.driver_task.index_loaded = true;
        }
        let ids: Vec<String> = self
            .driver_task
            .index
            .as_ref()
            .map(|idx| install::updates(idx, &ext::list_extensions_in(&root)))
            .unwrap_or_default()
            .into_iter()
            .map(|u| u.id)
            .collect();
        if ids.is_empty() {
            self.sess.status = t(Msg::DrvStatusNoUpdates).into();
            self.redraw();
            return;
        }
        let (first, n) = (ids[0].clone(), ids.len());
        self.drivers_install_many(ids);
        self.sess.status = tf(Msg::DrvStatusQueued, &[&first, "1", &n.to_string()]);
        self.redraw();
    }

    /// 워커 시작: `None` = 색인 새로 고침 · `Some(id)` = 설치(색인 최신 · 옆에 설치).
    fn drivers_spawn(&mut self, install_id: Option<String>) {
        if self.driver_task.busy() {
            return;
        }
        let Some(root) = self.driver_root() else {
            self.driver_win.set_status(t(Msg::DrvStatusNoFolder), false);
            return;
        };
        let src = self.driver_source();
        let (tx, rx): (Sender<DriverMsg>, Receiver<DriverMsg>) = std::sync::mpsc::channel();
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let status = match &install_id {
            Some(id) => tf(Msg::DrvStatusInstalling, &[id]),
            None => t(Msg::DrvStatusRefreshing).to_string(),
        };
        let spawned = std::thread::Builder::new()
            .name("driver-task".into())
            .spawn(move || {
                let wake = || {
                    if let Ok(px) = proxy.lock() {
                        let _ = px.send_event(Wake);
                    }
                };
                match install_id {
                    None => {
                        let r = install::fetch_index(&src, &root, true);
                        let _ = tx.send(DriverMsg::Index(r));
                    }
                    Some(id) => {
                        let tx2 = tx.clone();
                        let mut progress = |stage: &str, n: u64, total: Option<u64>| {
                            let _ = tx2.send(DriverMsg::Progress(stage.to_string(), n, total));
                            wake();
                        };
                        let r = install::install(
                            &src,
                            &root,
                            &id,
                            None,
                            false,
                            false,
                            DRIVER_KEYS,
                            &mut progress,
                        );
                        let _ = tx.send(DriverMsg::Installed(r));
                    }
                }
                wake();
            });
        if spawned.is_err() {
            self.driver_win
                .set_status(t(Msg::DrvStatusThreadFailed), false);
            return;
        }
        self.driver_task.rx = Some(rx);
        self.driver_win.set_status(&status, true);
    }

    /// ★ 파일에서 설치(패키지 zip 또는 오프라인 번들 · 워커 스레드 · 검증은 `install_bytes`가 그대로).
    pub(crate) fn drivers_install_path(&mut self, path: &std::path::Path) {
        if self.driver_task.busy() {
            return;
        }
        let Some(root) = self.driver_root() else {
            self.driver_win.set_status(t(Msg::DrvStatusNoFolder), false);
            return;
        };
        let (tx, rx): (Sender<DriverMsg>, Receiver<DriverMsg>) = std::sync::mpsc::channel();
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let p = path.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("driver-file".into())
            .spawn(move || {
                let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
                match bytes {
                    Ok(b) if install::is_bundle(&b) => {
                        let _ = tx.send(DriverMsg::Bundle(install::install_bundle(
                            &root,
                            &b,
                            true,
                            DRIVER_KEYS,
                        )));
                    }
                    Ok(_) => {
                        let _ = tx.send(DriverMsg::Installed(install::install_file(
                            &root,
                            &p,
                            true,
                            DRIVER_KEYS,
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(DriverMsg::Installed(Err(InstallError::new(
                            InstallErrorKind::Io,
                            p.display().to_string(),
                            e,
                        ))));
                    }
                }
                if let Ok(px) = proxy.lock() {
                    let _ = px.send_event(Wake);
                }
            });
        if spawned.is_err() {
            self.driver_win
                .set_status(t(Msg::DrvStatusThreadFailed), false);
            return;
        }
        self.driver_task.rx = Some(rx);
        self.driver_win.set_status(
            &tf(Msg::DrvStatusInstalling, &[&path.display().to_string()]),
            true,
        );
        self.driver_win.redraw();
    }

    /// ★ 기동 때 갱신 확인(설정 `driver.check_updates` · 하루 1회 · 색인 캐시가 24시간보다 오래됐을 때만 · 확장이 하나라도 설치돼 있거나
    ///   이 빌드에 내장이 아닌 방언이 있을 때 — 조용히 · 실패 무시 · 자동 교체 없음 · 39 §3 부하원 = 끄는 키 `driver.check_updates`).
    pub(crate) fn drivers_check_updates_at_start(&mut self) {
        if !self.settings.flag("driver.check_updates") {
            return;
        }
        let Some(root) = self.driver_root() else {
            return;
        };
        let stale = install::cached_index_age(&root).is_none_or(|a| a.as_secs() >= 24 * 3600);
        if !stale {
            return;
        }
        let any_ext = !ext::list_extensions_in(&root).is_empty()
            || nsql_drivers::available()
                .into_iter()
                .any(|d| !nsql_drivers::is_builtin(d));
        if !any_ext {
            return;
        }
        self.drivers_check_updates_now();
    }

    /// 갱신 확인 지금(조건 없이 · 기동 확인과 자체 시험 `drivers.check_updates`가 같은 길).
    pub(crate) fn drivers_check_updates_now(&mut self) {
        self.driver_task.badge_check = true;
        self.drivers_spawn(None);
    }

    /// 워커 메시지 → 창(매 `about_to_wait` · 창이 닫혀도 결과는 받아 색인을 갱신한다).
    pub(crate) fn drivers_pump(&mut self) {
        let Some(rx) = self.driver_task.rx.as_ref() else {
            return;
        };
        let mut done = false;
        let mut msgs = Vec::new();
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
        }
        for m in msgs {
            match m {
                DriverMsg::Progress(stage, n, total) => {
                    let pct = match total {
                        Some(t) if t > 0 => format!("{}%", (n * 100 / t).min(100)),
                        _ => nsql_core::fmt_bytes(n),
                    };
                    self.driver_win.set_status(
                        &tf(
                            Msg::DrvStatusProgress,
                            &[&nsql_i18n::driver_stage_text(&stage), &pct],
                        ),
                        true,
                    );
                }
                DriverMsg::Index(r) => {
                    done = true;
                    match r {
                        Ok((idx, cached)) => {
                            let n = idx.drivers.len().to_string();
                            if std::mem::take(&mut self.driver_task.badge_check) {
                                // 기동 갱신 확인: 설치본보다 새 버전이 있으면 상태줄 + 로그(자동 교체 없음 · 22 §3).
                                let installed = self
                                    .driver_root()
                                    .map(|r| ext::list_extensions_in(&r))
                                    .unwrap_or_default();
                                let ups = install::updates(&idx, &installed);
                                if !ups.is_empty() {
                                    let list: Vec<String> = ups
                                        .iter()
                                        .map(|u| format!("{} {} → {}", u.id, u.installed, u.latest))
                                        .collect();
                                    let text = tf(Msg::StDriverUpdates, &[&list.join(", ")]);
                                    self.sess.status = text.clone();
                                    self.log_win
                                        .push(crate::LogEntry::new(crate::LogKind::Info, text));
                                    // ★ 알림 + [갱신] 한 번 클릭(사용자 10-27): 사라지지 않는 카드 · 범위 = 설치된 드라이버의 새 버전만.
                                    self.toasts.push_button_sticky(
                                        crate::toast::ToastKind::Info,
                                        t(Msg::ToastDriverUpdatesTitle),
                                        tf(Msg::ToastDriverUpdatesBody, &[&list.join(", ")]),
                                        t(Msg::BtnDrvUpdate),
                                        UPDATE_INSTALLED_ACTION,
                                    );
                                }
                            }
                            self.driver_task.index = Some(idx);
                            self.driver_win.set_status(
                                &tf(
                                    if cached {
                                        Msg::DrvStatusIndexCached
                                    } else {
                                        Msg::DrvStatusIndexFetched
                                    },
                                    &[&n],
                                ),
                                false,
                            );
                        }
                        Err(e) => self.driver_install_error(&e),
                    }
                }
                DriverMsg::Bundle(r) => {
                    done = true;
                    match r {
                        Ok(list) => {
                            let ok = list.iter().filter(|(_, r)| r.is_ok()).count();
                            let bad: Vec<String> = list
                                .iter()
                                .filter_map(|(n, r)| r.as_ref().err().map(|e| format!("{n}: {e}")))
                                .collect();
                            for b in &bad {
                                self.log_win
                                    .push(crate::LogEntry::new(crate::LogKind::Error, b.clone()));
                            }
                            self.driver_win.set_status(
                                &tf(
                                    Msg::DrvStatusBundle,
                                    &[&ok.to_string(), &bad.len().to_string()],
                                ),
                                false,
                            );
                        }
                        Err(e) => self.driver_install_error(&e),
                    }
                }
                DriverMsg::Installed(r) => {
                    done = true;
                    // 묶음 설치면 줄마다 결과를 로그에(상태줄은 다음 항목·요약이 덮는다).
                    let in_batch = self.driver_task.batch.2 > 1;
                    if in_batch {
                        let line = match &r {
                            Ok(rep) => format!("driver install: {}@{} ok", rep.id, rep.version),
                            Err(e) => format!("driver install: {} ({})", e.detail, e.kind.code()),
                        };
                        self.log_win
                            .push(crate::LogEntry::new(crate::LogKind::Info, line));
                    }
                    let ok = r.is_ok();
                    match r {
                        Ok(rep) => {
                            // 설치가 색인을 새로 받았을 수 있다 → 캐시 다시 읽기.
                            if let Some(root) = self.driver_root() {
                                self.driver_task.index = install::cached_index(&root);
                            }
                            // 같은 id가 이미 올라가 있으면 새 판은 재시작 뒤에 쓰인다(올린 드라이버는 내리지 않는다 · 64 §5).
                            let msg = if ext::is_loaded(&rep.id) {
                                Msg::DrvStatusInstalledRestart
                            } else {
                                Msg::DrvStatusInstalled
                            };
                            self.driver_win.set_status(
                                &tf(
                                    msg,
                                    &[&rep.id, &rep.version, &rep.dir.display().to_string()],
                                ),
                                false,
                            );
                        }
                        Err(e) => self.driver_install_error(&e),
                    }
                    // ★ 대기열(10-27): 다음 설치를 띄우거나 묶음 요약.
                    self.driver_task.rx = None;
                    let next =
                        batch_next(&mut self.driver_task.queue, &mut self.driver_task.batch, ok);
                    match next {
                        Some((id, i, total)) => {
                            self.drivers_spawn(Some(id.clone()));
                            self.driver_win.set_status(
                                &tf(
                                    Msg::DrvStatusQueued,
                                    &[&id, &i.to_string(), &total.to_string()],
                                ),
                                true,
                            );
                        }
                        None if self.driver_task.batch.2 > 1 => {
                            let (n, fail, _) = self.driver_task.batch;
                            let text = tf(
                                Msg::DrvStatusBatchDone,
                                &[&(n - fail).to_string(), &fail.to_string()],
                            );
                            self.driver_win.set_status(&text, false);
                            self.sess.status = text.clone();
                            self.log_win
                                .push(crate::LogEntry::new(crate::LogKind::Info, text));
                            self.driver_task.batch = (0, 0, 0);
                            self.redraw();
                        }
                        None => self.driver_task.batch = (0, 0, 0),
                    }
                }
            }
        }
        if done {
            if self.driver_task.queue.is_empty() && self.driver_task.batch.2 == 0 {
                self.driver_task.rx = None;
            }
            self.driver_win.redraw();
            self.drivers_info_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// ★ 설치 대기열(사용자 10-27): 끝난 수·실패 수를 세고 다음 id와 "n/전체"를 돌려준다 · 비면 None(요약 차례).
    #[test]
    fn batch_queue_counts_and_advances() {
        let mut q: VecDeque<String> = ["b", "c"].into_iter().map(String::from).collect();
        let mut b = (0, 0, 3);
        assert_eq!(batch_next(&mut q, &mut b, true), Some(("b".into(), 2, 3)));
        assert_eq!(b, (1, 0, 3));
        assert_eq!(batch_next(&mut q, &mut b, false), Some(("c".into(), 3, 3)));
        assert_eq!(b, (2, 1, 3), "실패도 끝난 수에 든다");
        assert_eq!(batch_next(&mut q, &mut b, true), None);
        assert_eq!(b, (3, 1, 3));
        // 순서 유지 중복 제거 · 빈 id 제거.
        let ids = dedup_ids(vec!["pg".into(), "".into(), "sqlite".into(), "pg".into()]);
        assert_eq!(ids, ["pg", "sqlite"]);
        assert!(dedup_ids(Vec::new()).is_empty());
    }
}
