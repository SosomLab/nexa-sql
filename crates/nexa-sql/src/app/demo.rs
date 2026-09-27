//! App — 데모 프로필·샘플 데이터(docs/21 §5).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 데모 SQLite 파일 = 사용자 설정 폴더(`NSQL_HOME`)/demo.sqlite — 설치본·포터블 규약 그대로(exe 옆 금지).
    fn demo_path() -> Option<PathBuf> {
        nsql_settings::config_dir().map(|d| d.join("demo.sqlite"))
    }

    /// 'Demo' 프로필과 파일이 둘 다 있는가.
    pub(crate) fn demo_exists() -> bool {
        let has_profile = Vault::open_default()
            .ok()
            .and_then(|v| v.peek("Demo").ok().flatten())
            .is_some();
        has_profile && Self::demo_path().is_some_and(|p| p.exists())
    }

    /// 최초 실행 1회 팝업(창 가운데): 만들기 / 나중에.
    pub(crate) fn open_demo_prompt(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let Some(w) = self.window.as_ref() else {
            return;
        };
        let sz = w.inner_size();
        let r = Rect::new(
            (sz.width as i32 / 2 - px(150.0, self.scale)).max(0),
            (sz.height as i32 / 3).max(0),
            0,
            0,
        );
        let items = vec![
            CtxItem::item("demo.ask", t(Msg::DemoAsk)),
            CtxItem::Separator,
            CtxItem::item("demo.create", t(Msg::DemoYes)),
            CtxItem::item("demo.later", t(Msg::DemoLater)),
        ];
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 데모 만들기(메뉴 · 팝업): 배경 스레드에서 `demo.sqlite`에 내장 스크립트(`examples/demo.sql`)를 실행 → 끝나면 프로필 저장.
    pub(crate) fn start_demo_create(&mut self) {
        if self.demo_job.is_some() || self.demo_ready {
            return;
        }
        let Some(path) = Self::demo_path() else {
            self.sess.status = tf(Msg::StDemoFailed, &["NSQL_HOME"]);
            return;
        };
        self.sess.status = t(Msg::StDemoCreating).into();
        let (tx, rx) = std::sync::mpsc::channel();
        self.demo_job = Some(rx);
        std::thread::Builder::new()
            .name("nsql-demo".into())
            .spawn(move || {
                let r = create_demo_db(&path);
                let _ = tx.send(r);
            })
            .ok();
        self.redraw();
    }

    /// 생성 결과: 프로필 'Demo' 저장 → 접속 창 목록 갱신 → 메뉴 비활성 → 안내.
    pub(crate) fn finish_demo(&mut self, r: Result<String, String>) {
        match r.and_then(|path| {
            let target = format!("sqlite:{path}");
            let spec = nsql_drivers::parse_target(&target, Dialect::Sqlite)?;
            Vault::open_default()
                .and_then(|v| v.save("Demo", &spec))
                .map_err(|e| e.to_string())?;
            Ok(path)
        }) {
            Ok(path) => {
                self.demo_ready = true;
                self.rebuild_menus();
                self.conn_win.refresh_profiles(Some("Demo"));
                self.sess.status = tf(Msg::StDemoCreated, &[&path]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
            Err(e) => {
                self.sess.status = tf(Msg::StDemoFailed, &[&e]);
                self.toasts
                    .push(toast::ToastKind::Error, t(Msg::MnDemoCreate), e);
            }
        }
        self.redraw();
    }
}
