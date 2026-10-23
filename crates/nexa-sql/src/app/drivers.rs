//! ★ 드라이버 관리자(docs/22 §5-2 · T-29 2차): 창([`crate::driver_win`])의 데이터·동작 = 색인(캐시 → [새로 고침]) · 설치본 목록 ·
//! 설치/갱신(워커 스레드 · 진행은 상태줄) · 삭제 · 확인. CLI `nsql driver`와 같은 `nsql_drivers::ext` 코어를 쓴다.
//!
//! 네트워크(26 §8): 창 열기 = 캐시만 · [새로 고침] = 조건부 GET 1 · [설치] = 자산 3 GET — 전부 사용자 동작 · 동시 1(작업 중이면 버튼 잠금).
use crate::driver_win::{build_rows, DriverRow, DriverWinAction};
use crate::{theme, App, Wake};
use nsql_drivers::ext::install::{self, Index, InstallReport, Source};
use nsql_drivers::ext::{self, keys::DRIVER_KEYS};
use nsql_i18n::{t, tf, Msg};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use winit::event_loop::ActiveEventLoop;

/// 워커 → UI.
pub(crate) enum DriverMsg {
    Progress(String, u64, Option<u64>),
    Installed(Result<InstallReport, String>),
    Index(Result<(Index, bool), String>),
    /// 오프라인 번들 설치 결과(자산별).
    Bundle(Result<install::BundleOutcome, String>),
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
}

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
            DriverWinAction::Install(id) => self.drivers_spawn(Some(id)),
            DriverWinAction::Remove(id) => {
                // 올라간 판도 지운다(파일은 .trash → 다음 시작) — 재시작까지 그 판이 쓰인다고 안내(사용자 10-23).
                let loaded = ext::is_loaded(&id);
                let text = match ext::remove_extension(&id, None) {
                    Ok(paths) if paths.is_empty() => tf(Msg::DrvStatusError, &[&id]),
                    Ok(_) if loaded => tf(Msg::DrvStatusRemovedRestart, &[&id]),
                    Ok(paths) => tf(Msg::DrvStatusRemoved, &[&id, &paths.len().to_string()]),
                    Err(e) => tf(Msg::DrvStatusError, &[&e]),
                };
                self.driver_win.set_status(&text, false);
                self.drivers_info_changed();
            }
            DriverWinAction::Check(id) => {
                let text = match ext::check_extension(&id) {
                    Ok(c) => tf(
                        Msg::DrvStatusCheckOk,
                        &[&c.driver_id, &c.version, &c.dialect, &c.abi.to_string()],
                    ),
                    Err(e) => tf(Msg::DrvStatusError, &[&e.message]),
                };
                self.driver_win.set_status(&text, false);
            }
        }
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
                .set_status(&tf(Msg::DrvStatusError, &["thread"]), false);
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
                        let _ = tx.send(DriverMsg::Installed(Err(e)));
                    }
                }
                if let Ok(px) = proxy.lock() {
                    let _ = px.send_event(Wake);
                }
            });
        if spawned.is_err() {
            self.driver_win
                .set_status(&tf(Msg::DrvStatusError, &["thread"]), false);
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
                        _ => format!("{} KB", n / 1024),
                    };
                    self.driver_win
                        .set_status(&tf(Msg::DrvStatusProgress, &[&stage, &pct]), true);
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
                        Err(e) => self
                            .driver_win
                            .set_status(&tf(Msg::DrvStatusError, &[&e]), false),
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
                        Err(e) => self
                            .driver_win
                            .set_status(&tf(Msg::DrvStatusError, &[&e]), false),
                    }
                }
                DriverMsg::Installed(r) => {
                    done = true;
                    match r {
                        Ok(rep) => {
                            // 설치가 색인을 새로 받았을 수 있다 → 캐시 다시 읽기.
                            if let Some(root) = self.driver_root() {
                                self.driver_task.index = install::cached_index(&root);
                            }
                            self.driver_win.set_status(
                                &tf(
                                    Msg::DrvStatusInstalled,
                                    &[&rep.id, &rep.version, &rep.dir.display().to_string()],
                                ),
                                false,
                            );
                        }
                        Err(e) => self
                            .driver_win
                            .set_status(&tf(Msg::DrvStatusError, &[&e]), false),
                    }
                }
            }
        }
        if done {
            self.driver_task.rx = None;
            self.driver_win.redraw();
            self.drivers_info_changed();
        }
    }
}
