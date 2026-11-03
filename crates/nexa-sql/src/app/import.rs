//! ★ Import 창의 호스트 쪽(docs/127 §2-2 · 11-04 · T-8 ⑥): 열기 문맥(`ImportCtx`) · **살펴보기 스레드**(`inspect_source` = 표본만 · 결과는 채널 +
//! `Wake` · 한가할 때 폴링 · 뒤늦은 응답은 순번으로 버림) · 기존 표 열 = 탐색기 메타 캐시(`request_columns` → `columns_cached` 폴링) ·
//! 새 표 기본 이름 = 파일 줄기 정리값.

use crate::*;
use nsql_run::bulk::{Inspect, OnError, SourceOpts};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Mutex;

/// 열기 문맥(탐색기 ▸ Import Data… / 기동 명령).
#[derive(Clone, Debug)]
pub(crate) struct ImportCtx {
    /// 기존 표 표기(인용 · `None` = 새 표).
    pub existing: Option<String>,
    /// 스키마(기존 표의 것 · 새 표를 둘 곳 · 없으면 접속 기본).
    pub schema: Option<String>,
    /// 기존 표의 맨이름(열 캐시 조회용).
    pub name: Option<String>,
    pub path: PathBuf,
    pub server: Option<ConnectSpec>,
    pub dialect: Dialect,
    /// 기존 표 열을 기다리는 중(메타 캐시 폴링).
    pub cols_wait: bool,
    /// 이번 시작이 새 표를 만들었다(끝나면 탐색기 새로 고침).
    pub created: bool,
}

/// 살펴보기 스레드 한 건.
pub(crate) struct InspectJob {
    pub rx: Receiver<Result<Inspect, String>>,
    pub seq: u64,
}

/// 스레드 `import-inspect`에서 `inspect_source` → 채널 + `Wake`. 스레드를 못 만들면 `None`.
pub(crate) fn spawn_inspect(
    path: PathBuf,
    opts: SourceOpts,
    sample: usize,
    preview: usize,
    seq: u64,
    proxy: winit::event_loop::EventLoopProxy<Wake>,
) -> Option<InspectJob> {
    let (tx, rx) = channel();
    let proxy = Mutex::new(proxy);
    std::thread::Builder::new()
        .name("import-inspect".into())
        .spawn(move || {
            let r = nsql_run::bulk::inspect_source(&path.to_string_lossy(), &opts, sample, preview);
            let _ = tx.send(r);
            if let Ok(px) = proxy.lock() {
                let _ = px.send_event(Wake);
            }
        })
        .ok()?;
    Some(InspectJob { rx, seq })
}

/// 새 표 기본 이름 = 파일 줄기 정리값(비면 `imported`).
pub(crate) fn new_table_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let n = nsql_io::infer::column_ident(&stem, 0);
    if n.starts_with("col") && stem.trim().is_empty() {
        "imported".into()
    } else {
        n
    }
}

impl App {
    /// ★ 메뉴 Run ▸ Import Data…(127 §2-1 ③): 현재 세션 · 새 표(기본 스키마) → 파일 창 → Import 창.
    pub(crate) fn import_from_menu(&mut self) {
        if !self.lic_gate(nsql_license::Feature::ImportGui) {
            return;
        }
        self.import_ctx = Some(ImportCtx {
            existing: None,
            schema: None,
            name: None,
            path: PathBuf::new(),
            server: None,
            dialect: self.sess.dialect,
            cols_wait: false,
            created: false,
        });
        self.file_purpose = FilePurpose::Import;
        self.open_file_dlg = Some(PickerMode::Open);
        self.redraw();
    }

    /// 파일 창 결과(또는 기동 명령) → Import 창 열기 + 살펴보기 시작 + (기존 표) 열 요청.
    pub(crate) fn import_open_now(&mut self, el: &ActiveEventLoop) {
        let Some(mut ctx) = self.import_ctx.clone() else {
            return;
        };
        // 방언 = 그 서버의 것 > 지금 세션(기동 명령은 접속 뒤에 열리므로 여기서 다시 읽는다 · 새 표 형 제안·DDL의 기준).
        ctx.dialect = ctx
            .server
            .as_ref()
            .and_then(|s| s.dialect)
            .unwrap_or(self.sess.dialect);
        let owner = self.window.clone();
        let was_open = self.import_win.is_open();
        let defaults = import_win::Defaults {
            batch: self.settings.int("bulk.batch_rows").max(1) as usize,
            commit: self.settings.int("bulk.commit_every").max(0) as usize,
            mode: self
                .settings
                .get("bulk.mode")
                .and_then(nsql_run::bulk::BulkMode::parse)
                .unwrap_or_default(),
            encoding: self
                .settings
                .get("import.encoding")
                .unwrap_or("auto")
                .to_string(),
            on_error: self
                .settings
                .get("import.on_error")
                .and_then(OnError::parse)
                .unwrap_or_default(),
            text_margin_pct: self.settings.int("import.text_margin_pct").clamp(0, 500) as u32,
            json_max_mb: self.settings.int("import.json_max_mb").clamp(1, 4096) as u32,
        };
        let o = import_win::ImportOpen {
            existing: ctx.existing.clone(),
            schema: ctx.schema.clone(),
            new_name: new_table_name(&ctx.path),
            dialect: ctx.dialect,
            path: ctx.path.clone(),
            defaults,
        };
        self.import_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            owner.as_deref(),
            o,
        );
        if !was_open {
            if let (Some(o), Some(c)) = (owner.as_deref(), self.import_win.window()) {
                winfocus::attach_child(o, c);
            }
        }
        self.sync_modal();
        if ctx.existing.is_some() {
            if let Some(name) = ctx.name.clone() {
                self.explorer.request_columns(
                    ctx.server.as_ref(),
                    ctx.schema.as_deref(),
                    &name,
                    true,
                );
                ctx.cols_wait = true;
            }
        }
        self.import_ctx = Some(ctx);
        self.import_reinspect();
    }

    /// 원료 옵션으로 다시 살펴보기(스레드 · 최신 순번만 받는다).
    pub(crate) fn import_reinspect(&mut self) {
        let Some(ctx) = self.import_ctx.as_ref() else {
            return;
        };
        self.import_seq += 1;
        let sample = self.settings.int("import.sample_rows").max(0) as usize;
        let preview = self.settings.int("import.preview_rows").clamp(1, 10_000) as usize;
        self.import_win.set_inspecting();
        self.import_inspect = spawn_inspect(
            ctx.path.clone(),
            self.import_win.source_opts(),
            sample,
            preview,
            self.import_seq,
            self.wake_proxy.clone(),
        );
        if self.import_inspect.is_none() {
            self.import_win
                .set_inspect(Err("inspect thread could not start".into()));
        }
    }

    /// 한가할 때: 살펴보기 결과 · 기존 표 열 캐시.
    pub(crate) fn import_bg_poll(&mut self) {
        if let Some(job) = self.import_inspect.as_ref() {
            match job.rx.try_recv() {
                Ok(r) => {
                    let seq = job.seq;
                    self.import_inspect = None;
                    if seq == self.import_seq {
                        self.import_win.set_inspect(r);
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.import_inspect = None;
                    self.import_win
                        .set_inspect(Err("inspect thread ended".into()));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let waiting = self
            .import_ctx
            .as_ref()
            .filter(|c| c.cols_wait)
            .and_then(|c| {
                c.name
                    .clone()
                    .map(|n| (c.server.clone(), c.schema.clone(), n))
            });
        if let Some((server, schema, name)) = waiting {
            if let Some(cols) =
                self.explorer
                    .columns_cached(server.as_ref(), schema.as_deref(), &name)
            {
                self.import_win.set_table_columns(cols);
                if let Some(c) = self.import_ctx.as_mut() {
                    c.cols_wait = false;
                }
            }
        }
    }
}
