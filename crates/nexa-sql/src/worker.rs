//! 워커 스레드 — Runner(엔진 + 세션)를 소유한다. UI는 명령을 보내고 이벤트를 받는다.
//! 세션 객체는 이 스레드 안에서만 만들어지고 산다(`Send` 불요).
//!
//! 연결 프로필(T-16b): `Connect("prod")`처럼 **이름**이 오면 `nsql-vault`에서 푼다. 저장소는 호출마다
//! 연다 — 다른 창·CLI 인스턴스가 방금 저장한 프로필도 그대로 보인다(메모리 캐시 없음).

use nsql_core::{DbError, Dialect, Session};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;
use nsql_vault::Vault;
use std::sync::mpsc;

pub(crate) enum Cmd {
    /// 접속 문자열 **또는 프로필 이름**.
    Connect(String),
    /// 접속 문자열을 프로필로 저장(비밀번호는 봉투에).
    Save {
        name: String,
        target: String,
    },
    Run(String),
    Quit,
}

fn err(message: String) -> RunEvent {
    RunEvent::Error {
        index: 0,
        line: 0,
        error: DbError {
            code: None,
            message,
            position: None,
        },
    }
}

/// 이름이면 저장소에서, 아니면 접속 문자열 파싱.
fn resolve_target(target: &str, default_dialect: Dialect) -> Result<ConnectSpec, String> {
    if nsql_vault::is_profile_name(target) {
        let v = Vault::open_default().map_err(|e| e.to_string())?;
        if let Some(spec) = v.resolve(target).map_err(|e| e.to_string())? {
            return Ok(spec);
        }
    }
    nsql_drivers::parse_target(target, default_dialect)
}

/// 저장된 프로필 이름(상태줄 안내용 · 실패하면 빈 목록).
pub(crate) fn profile_names() -> Vec<String> {
    Vault::open_default()
        .and_then(|v| v.list())
        .map(|l| l.into_iter().map(|p| p.name).collect())
        .unwrap_or_default()
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
            let mut runner =
                Runner::new(default_dialect, opener).with_resolver(Box::new(|name: &str| {
                    let v = Vault::open_default().map_err(|e| e.to_string())?;
                    v.resolve(name).map_err(|e| e.to_string())
                }));
            let mut emit = |e: RunEvent| {
                let _ = etx.send(e);
                wake();
            };
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Cmd::Connect(target) => {
                        match resolve_target(&target, default_dialect) {
                            Ok(spec) => {
                                runner.connect(&spec, &mut emit);
                            }
                            Err(e) => emit(err(e)),
                        }
                        let _ = dtx.send(None);
                        wake();
                    }
                    Cmd::Save { name, target } => {
                        let r = nsql_drivers::parse_target(&target, default_dialect)
                            .and_then(|spec| {
                                let v = Vault::open_default().map_err(|e| e.to_string())?;
                                v.save(&name, &spec).map_err(|e| e.to_string())?;
                                Ok(spec.redacted())
                            });
                        match r {
                            Ok(desc) => emit(RunEvent::Message(format!(
                                "프로필 저장: {name} = {desc} — 이제 접속 칸에 '{name}'만 넣어도 됩니다"
                            ))),
                            Err(e) => emit(err(format!("프로필 저장 실패: {e}"))),
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
