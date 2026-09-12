//! 워커 스레드 — Runner(엔진 + 세션)를 소유한다. UI는 명령을 보내고 이벤트를 받는다.
//! 세션 객체는 이 스레드 안에서만 만들어지고 산다(`Send` 불요).

use nsql_core::{DbError, Dialect, Session};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;
use std::sync::mpsc;

pub(crate) enum Cmd {
    Connect(String),
    Run(String),
    Quit,
}

pub(crate) struct Handle {
    tx: mpsc::Sender<Cmd>,
    /// 명령 완료 신호(상태 메시지 옵션).
    pub done: mpsc::Receiver<Option<String>>,
}

impl Handle {
    pub(crate) fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

pub(crate) fn spawn(
    default_dialect: Dialect,
    wake: Box<dyn Fn() + Send>,
) -> (Handle, mpsc::Receiver<RunEvent>) {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (etx, erx) = mpsc::channel::<RunEvent>();
    let (dtx, drx) = mpsc::channel::<Option<String>>();
    std::thread::Builder::new()
        .name("nsql-worker".into())
        .spawn(move || {
            let opener: Opener = Box::new(
                move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                    nsql_drivers::open(spec, default_dialect)
                },
            );
            let mut runner = Runner::new(default_dialect, opener);
            let mut emit = |e: RunEvent| {
                let _ = etx.send(e);
                wake();
            };
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Cmd::Connect(target) => {
                        match nsql_drivers::parse_target(&target, default_dialect) {
                            Ok(spec) => {
                                runner.connect(&spec, &mut emit);
                            }
                            Err(e) => emit(RunEvent::Error {
                                index: 0,
                                line: 0,
                                error: DbError {
                                    code: None,
                                    message: e,
                                    position: None,
                                },
                            }),
                        }
                        let _ = dtx.send(None);
                        wake();
                    }
                    Cmd::Run(src) => {
                        // 치환 변수 프롬프트는 최소 GUI에서 빈 값(T-16c에서 대화상자).
                        let mut prompt = |_: &str| Some(String::new());
                        let errs = runner.run_script(&src, &mut prompt, &mut emit);
                        let _ = dtx.send(if errs > 0 {
                            Some(format!("오류 {errs}건"))
                        } else {
                            None
                        });
                        wake();
                    }
                    Cmd::Quit => {
                        if let Some(s) = runner.session.as_mut() {
                            let _ = s.commit();
                        }
                        break;
                    }
                }
            }
        })
        .expect("worker thread");
    (Handle { tx, done: drx }, erx)
}
