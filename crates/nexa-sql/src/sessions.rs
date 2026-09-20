//! 세션 컨텍스트(docs/52) — **실행 통제의 단위 = 세션**.
//!
//! 기본 사상(사용자 09-18): *1 인스턴스 = 1 서버 · 1 계정*. 그래서 기본은 **공유 세션**(id 0) 하나이고,
//! 탭 전용(Private) 세션은 명시적 예외다 — 편집기에서 `CONNECT …`를 실행한 탭 · 또는 `session.mode = per-editor`.
//!
//! 구조: [`Sess`] 하나 = 워커 스레드 하나(= Runner · DB 세션 하나) + 그 세션에 딸린 **모든 실행 상태**(busy · 트랜잭션 대기 ·
//! 실행 중 문장 · 토스트 · 트랜잭션 로그 · 키 캐시 · 상태줄 글). 호스트(`App`)는 `sess`(지금 보이는 탭의 세션)와 `parked`(나머지)를
//! 들고, 활성 탭이 바뀌면 통째로 맞바꾼다(`mem::swap` 1회) — 기존 코드는 늘 `self.sess.*`만 보면 된다.
//!
//! 통제 규칙(§3): 한 세션에는 **한 번에 한 작업**. 실행·Explain·새로고침·추가 페치·전체 조회·건수·키 조회·Commit/Rollback·접속은
//! 전부 [`Sess::blocked`] 하나로 막힌다. 공유 세션이면 그 세션을 쓰는 **모든 탭**이 함께 막히고, 탭 전용 세션이면 그 탭만 막힌다.
//! 중지(■)·접속 해제만 예외(막힌 상태를 푸는 동작).

use crate::runtoast::RunToast;
use crate::worker;
use nsql_core::Dialect;
use nsql_i18n::Msg;
use nsql_run::RunEvent;
use nsql_script::{Command, ConnectSpec, ItemKind};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 수동 커밋 대기 문장 하나(T-77) — 편집기 탭 · 시각 · 시:분 · 요약.
#[derive(Clone, Debug)]
pub(crate) struct TxItem {
    pub editor: u64,
    pub at: Instant,
    pub when: String,
    pub summary: String,
    /// 문장 종류(docs/44 §5 · 버튼 색·툴팁·로그 Tx 열).
    pub class: nsql_core::TxClass,
}

/// 설정 `session.mode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SessionMode {
    /// 모든 탭이 공유 세션 하나(기본) — `CONNECT`를 실행한 탭만 전용 세션.
    Shared,
    /// 탭마다 자기 세션(처음 활성화될 때 기본 접속 정보로 연결 · 해제하면 그 탭은 실행 불가).
    PerEditor,
}

impl SessionMode {
    pub(crate) fn parse(v: Option<&str>) -> Self {
        match v {
            Some("per-editor" | "per-tab") => SessionMode::PerEditor,
            _ => SessionMode::Shared,
        }
    }
}

/// 공유 세션 id.
pub(crate) const SHARED: u64 = 0;

/// 세션 하나의 컨텍스트 — 워커 + 그 세션에 딸린 실행 상태 전부.
pub(crate) struct Sess {
    pub id: u64,
    /// 전용 세션의 주인 편집기 탭 id(`None` = 공유 세션).
    pub owner: Option<u64>,
    pub worker: worker::Handle,
    pub events: mpsc::Receiver<RunEvent>,
    /// 실행·접속·Commit/Rollback이 진행 중(워커 `done` 신호로 풀린다).
    pub busy: bool,
    /// 진행 중인 보조 요청 수(추가 페치 · 전체 조회 · 건수 · 키 조회 — 각자의 응답으로 줄어든다).
    pub aux: u32,
    pub connected: bool,
    /// 접속 설명(비밀번호 가림) — 탭 툴팁·상태줄.
    pub desc: String,
    /// 접속 창에서 붙인 프로필 이름(공유 연결 목록·탐색기 루트 이름 · 없으면 빈 글).
    pub profile: String,
    /// 다시 붙을 때 쓸 스펙(유휴 해제 뒤 재접속 · 스크립트 `CONNECT`면 프로필 이름만 든 스펙일 수 있다 = 워커가 저장소에서 푼다).
    pub spec: Option<ConnectSpec>,
    /// 접속 창이 시도한 스펙(성공하면 탐색기 메타 세션이 같은 스펙으로 붙는다).
    pub last_spec: Option<ConnectSpec>,
    /// 접속 창의 시도가 진행 중(결과가 오면 시도 큐를 잇는다) — 자동 접속·재접속의 결과와 구별.
    pub attempt_inflight: bool,
    /// 사용자가 명시적으로 끊었다(개별 모드: 자동 접속하지 않는다 · 아이콘 = 끊김).
    pub user_disconnected: bool,
    /// 이 세션의 해제를 시작한 경로(로그 한 줄에 "어느 경로로 어떤 서버를" · 사용자 09-19). 접속되면 비운다.
    pub disc_path: Option<DiscPath>,
    /// 유휴로 끊었다(다음 실행 때 `spec`으로 조용히 다시 붙는다 · §6).
    pub idle_closed: bool,
    /// ★ 끊김을 **확인**했다(docs/53 §3 Broken): 동작 직전 빠른 판정 실패 · 접속성 오류 · 드라이버가 끊김을 앎. 세션 객체·스펙은 그대로 —
    ///   다음 동작 때 판정 → 살아 있으면 재접속(설정) · 사용자가 VPN을 다시 켜면 그 자리에서 이어진다. 표식 = 끊김 · 플러그 = 빨강.
    pub broken: bool,
    /// 닫는 중(공유 모드의 전용 세션이 해제됨) — 호스트가 다음 틱에 거둔다.
    pub closing: bool,
    /// 실행 앞에 끼워 보낸 재접속의 `done` 신호 수 — 그만큼은 busy를 풀지 않고 넘긴다(뒤따르는 실행이 아직 돈다).
    pub skip_done: u32,
    /// 마지막 사용 시각(유휴 판정).
    pub last_used: Instant,
    pub dialect: Dialect,
    pub status: String,
    // 트랜잭션(DR-30 · T-77) — 세션 단위.
    pub tx_dirty: bool,
    pub tx_pending: Vec<TxItem>,
    pub tx_read: bool,
    pub tx_stale_logged: bool,
    /// 이 세션에서 마지막으로 문장을 실행한 시각 — 유휴 미커밋 판정의 시계(docs/56 L2 · "그 세션의 문장 실행만" = 활동).
    pub last_exec: Instant,
    /// 마지막 미커밋 경고 시각(재알림 간격 `tx.remind_min`).
    pub tx_warned_at: Option<Instant>,
    /// "나중에/연장"으로 미룬 끝 시각.
    pub tx_snooze_until: Option<Instant>,
    /// 자동 처리 카운트다운 마감.
    pub tx_countdown: Option<Instant>,
    /// 내 미커밋 때문에 기다리는 세션 수(docs/56 L3 · 마지막 폴링 결과) · 다음 폴링 시각 · 이 서버에서 기능이 꺼졌는가(권한 없음).
    pub tx_blockers: usize,
    pub tx_block_next: Instant,
    pub tx_block_off: bool,
    /// 지금 이 세션 때문에 기다리는 세션들의 설명(트랜잭션 로그 창 "차단 중" 띠 · `tx_blockers`와 함께 갱신).
    pub tx_blocker_who: Vec<String>,
    /// 운영 접속의 변경 문장 실행 2단 확인(같은 본문을 3초 안에 다시 실행하면 진행 · docs/56 §4).
    pub prod_armed: Option<(u64, std::time::Instant)>,
    /// 이번 실행에서 성공한 DDL의 대상(docs/57 T1) — 실행이 끝나면 폴더별로 한 번 탐색기에 반영한다.
    pub ddl_now: Vec<nsql_core::DdlTarget>,
    /// 트랜잭션 DDL 방언의 수동 커밋에서 **커밋을 기다리는** DDL 대상(커밋 = 반영 · 롤백·세션 소실 = 버림 · D-107).
    pub ddl_wait: Vec<nsql_core::DdlTarget>,
    /// 이 세션에서 **세션 상태를 바꾸는 문장**이 실행됐다(`ALTER SESSION`·`SET`·임시 테이블·PL/SQL 블록 …) — 닫으면 그 상태를
    /// 잃으므로 유휴 닫기 대상에서 뺀다(§6-4). 새로 접속하면 거짓.
    pub stateful: bool,
    // 실행 하나의 상태.
    pub run_editor: u64,
    pub run_tab: u64,
    /// 이번 실행에서 마지막으로 결과를 낸 문장 번호 — 같은 문장이 결과를 또 내면 딸린 결과 탭으로 보낸다([`extra_result_slot`]).
    pub run_set_stmt: Option<usize>,
    /// 이번 실행에서 쓴 딸린 결과 탭 수(실행이 끝나면 이보다 뒤의 것은 걷는다).
    pub run_children: u32,
    /// 지금 끝나기를 기다리는 작업이 **스크립트 실행**인가(접속·커밋의 완료 신호와 구별).
    pub run_tracking: bool,
    pub last_run_items: Vec<String>,
    pub run_line_base: usize,
    pub single_run: bool,
    pub run_cancel_requested: bool,
    pub run_cancel_drops: bool,
    pub last_rows: Option<usize>,
    pub last_secs: Option<f64>,
    pub run_toast: RunToast,
    // Oracle 라이브 로그(T-71) — 공유(탐색기와 짝인) 세션만 쓴다.
    pub live_sid: Option<String>,
    pub live_next: Instant,
    pub live_since: Option<String>,
    pub live_last: String,
    pub live_final: bool,
    // Copy SQL 키 캐시(접속당 · docs/41).
    pub key_cache: HashMap<String, Option<nsql_core::KeyInfo>>,
    pub sql_wait: Option<nsql_io::SqlKind>,
    pub view_wait: Option<nsql_io::SqlKind>,
}

impl Sess {
    pub(crate) fn new(
        id: u64,
        owner: Option<u64>,
        worker: worker::Handle,
        events: mpsc::Receiver<RunEvent>,
        dialect: Dialect,
    ) -> Self {
        let now = Instant::now();
        Sess {
            id,
            owner,
            worker,
            events,
            busy: false,
            aux: 0,
            connected: false,
            desc: String::new(),
            profile: String::new(),
            spec: None,
            last_spec: None,
            attempt_inflight: false,
            user_disconnected: false,
            disc_path: None,
            idle_closed: false,
            broken: false,
            closing: false,
            skip_done: 0,
            last_used: now,
            dialect,
            status: String::new(),
            tx_dirty: false,
            tx_pending: Vec::new(),
            tx_read: false,
            tx_stale_logged: false,
            last_exec: Instant::now(),
            tx_warned_at: None,
            tx_snooze_until: None,
            tx_countdown: None,
            tx_blockers: 0,
            tx_block_next: Instant::now(),
            tx_block_off: false,
            tx_blocker_who: Vec::new(),
            prod_armed: None,
            ddl_now: Vec::new(),
            ddl_wait: Vec::new(),
            stateful: false,
            run_editor: 0,
            run_tab: 0,
            run_set_stmt: None,
            run_children: 0,
            run_tracking: false,
            last_run_items: Vec::new(),
            run_line_base: 0,
            single_run: false,
            run_cancel_requested: false,
            run_cancel_drops: false,
            last_rows: None,
            last_secs: None,
            run_toast: RunToast::new(),
            live_sid: None,
            live_next: now,
            live_since: None,
            live_last: String::new(),
            live_final: false,
            key_cache: HashMap::new(),
            sql_wait: None,
            view_wait: None,
        }
    }

    /// ★ 통제의 단일 판정(§3): 이 세션에 새 작업을 보낼 수 없는가.
    pub(crate) fn blocked(&self) -> bool {
        self.busy || self.aux > 0
    }

    pub(crate) fn is_private(&self) -> bool {
        self.owner.is_some()
    }

    pub(crate) fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// 보조 요청 하나가 끝났다.
    pub(crate) fn aux_done(&mut self) {
        self.aux = self.aux.saturating_sub(1);
        self.touch();
    }
}

/// 스크립트의 **첫** 접속 명령 — 호스트가 실행 전에 세션 배치를 정할 때 본다(워커로 보내기 전 · 파싱만).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ConnectIntent {
    /// `CONNECT 대상` — 이 탭에 전용 세션을 만든다(이미 있으면 그 세션이 대상을 바꾼다).
    Connect(ConnectSpec),
    /// `DISCONNECT`가 `CONNECT`보다 먼저 나온다.
    Disconnect,
}

pub(crate) fn connect_intent(src: &str) -> Option<ConnectIntent> {
    nsql_script::split_script(src)
        .into_iter()
        .find_map(|it| match it.kind {
            ItemKind::Command(Command::Connect(spec)) => Some(ConnectIntent::Connect(spec)),
            ItemKind::Command(Command::Disconnect) => Some(ConnectIntent::Disconnect),
            _ => None,
        })
}

// ───────────────────────── 순수 판정(docs/52 §12 MC/DC 표의 대상) ─────────────────────────
// 화면·워커를 모르는 함수들 — 호스트는 사실만 모아 넘기고 결과대로 움직인다. 조건 하나하나가 결과를 **단독으로** 뒤집는지
// 아래 테스트가 쌍으로 보인다(MC/DC). 새 조건을 넣으면 그 쌍도 같이 넣는다.

/// D1 탭 → 세션: ① 그 탭의 전용 세션 ② 탭이 묶인 공유 세션(살아 있을 때) ③ 활성 공유 세션.
pub(crate) fn route_tab(
    private: Option<u64>,
    bound: Option<u64>,
    bound_alive: bool,
    default: u64,
) -> u64 {
    if let Some(id) = private {
        return id;
    }
    match bound {
        Some(id) if bound_alive => id,
        _ => default,
    }
}

/// 접속 창이 본 공유 세션 하나의 사실.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SharedView {
    pub id: u64,
    /// 새로 붙으려는 대상과 같은 서버·DB·계정인가.
    pub same_server: bool,
    pub connected: bool,
    pub blocked: bool,
    pub idle_closed: bool,
    /// 이 세션에 묶인 탭 수.
    pub bound_tabs: usize,
}

/// D2 접속 창 Connect의 배치.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LoginPlan {
    /// 같은 서버에 이미 붙은(또는 유휴로 닫힌) 공유 세션 — 중복 접속하지 않고 그 세션을 쓴다(활성화 · 필요하면 재접속).
    Use(u64),
    /// 그 세션이 다른 작업 중 — 지금은 못 한다.
    Busy(u64),
    /// 끊긴 채 놀고 있는 공유 세션 객체를 다시 쓴다(워커 재사용).
    Recycle(u64),
    /// 새 공유 세션을 추가한다(기존 연결은 그대로).
    New,
    /// 상한(`session.max_shared`).
    Limit,
}

pub(crate) fn login_plan(shared: &[SharedView], max_shared: usize) -> LoginPlan {
    if let Some(s) = shared
        .iter()
        .find(|s| s.same_server && (s.connected || s.idle_closed || s.blocked))
    {
        return if s.blocked {
            LoginPlan::Busy(s.id)
        } else {
            LoginPlan::Use(s.id)
        };
    }
    // 묶인 탭이 있는 끊긴 세션은 건드리지 않는다(그 탭들이 말없이 다른 서버로 넘어간다) — 세션이 하나뿐일 때만 예외(종전 동작).
    let only = shared.len() == 1;
    if let Some(s) = shared
        .iter()
        .find(|s| !s.connected && !s.blocked && !s.idle_closed && (s.bound_tabs == 0 || only))
    {
        return LoginPlan::Recycle(s.id);
    }
    if shared.len() < max_shared {
        LoginPlan::New
    } else {
        LoginPlan::Limit
    }
}

/// 첫 `CONNECT` 명령을 스크립트에서 지운다(같은 서버에 이미 붙어 있고 `connect.reconnect_same`이 꺼져 있을 때 — 줄 번호가
/// 흔들리지 않게 그 자리를 공백으로 채운다 · 나머지 문장은 그대로 실행).
pub(crate) fn strip_first_connect(src: &str) -> String {
    let Some(it) = nsql_script::split_script(src)
        .into_iter()
        .find(|it| matches!(it.kind, ItemKind::Command(Command::Connect(_))))
    else {
        return src.to_string();
    };
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..it.span.start]);
    for ch in src[it.span.start..it.span.end].chars() {
        out.push(if ch == '\n' { '\n' } else { ' ' });
    }
    out.push_str(&src[it.span.end..]);
    out
}

/// 첫 접속 명령 **앞에** 서버로 갈 문장이 있는가(D-99) — 새 전용 세션에는 아직 접속이 없어 그 문장은 돌 수 없다.
pub(crate) fn statements_before_connect(src: &str) -> bool {
    for it in nsql_script::split_script(src) {
        match it.kind {
            ItemKind::Command(Command::Connect(_) | Command::Disconnect) => return false,
            ItemKind::Sql(_) | ItemKind::Command(Command::Exec { .. }) => return true,
            _ => {}
        }
    }
    false
}

/// ★ 세션 상태를 바꾸는 문장인가(§6-4 · D-103) — 접속을 닫으면 사라지는 것들: 세션 설정(`ALTER SESSION`·`SET`·`USE`·`PRAGMA`) ·
/// 임시 객체(`#temp`·`TEMPORARY`) · 준비된 문장/커서/`LISTEN`/`ATTACH`/잠금 · **PL/SQL·프로시저 호출**(패키지 전역 변수·애플리케이션
/// 컨텍스트를 바꿀 수 있다 — 문장만 봐서는 알 수 없으므로 보수적으로 전부) · 세션 변수/컨텍스트 함수(MySQL `@v :=` · PG `set_config` ·
/// SQL Server `sp_set_session_context` · 권고 잠금). 판정은 보수적이다: 의심스러우면 참(= 닫지 않는다).
/// ⚠️ 글만 봐서는 모르는 것: Oracle 전역 임시 테이블(`ON COMMIT PRESERVE ROWS`)에 대한 평범한 INSERT · 함수 안에서 바뀌는 패키지 상태.
pub(crate) fn alters_session_state(stmt: &str) -> bool {
    let up = stmt.trim_start().to_ascii_uppercase();
    let mut words = up.split_whitespace();
    let first = words.next().unwrap_or("");
    let second = words.next().unwrap_or("");
    let by_verb = match first {
        "SET" | "USE" | "PRAGMA" | "ATTACH" | "DETACH" | "PREPARE" | "DEALLOCATE" | "LISTEN"
        | "UNLISTEN" | "LOCK" | "EXEC" | "EXECUTE" | "CALL" | "BEGIN" | "DECLARE" | "DO"
        | "LOAD" | "RESET" | "DISCARD" => true,
        "ALTER" => second == "SESSION",
        "CREATE" | "DROP" => {
            matches!(second, "TEMP" | "TEMPORARY" | "LOCAL" | "PRIVATE")
                || up.contains(" TEMPORARY ")
                || up.contains(" TEMP ")
        }
        _ => false,
    };
    by_verb
        || up.contains('#')
        || up.contains(":=")
        || [
            "SET_CONFIG(",
            "PG_ADVISORY_LOCK",
            "GET_LOCK(",
            "SP_SET_SESSION_CONTEXT",
            "SP_GETAPPLOCK",
            "DBMS_SESSION.",
            "DBMS_APPLICATION_INFO.",
            "DBMS_LOCK.",
            "DBMS_OUTPUT.ENABLE",
        ]
        .iter()
        .any(|k| up.contains(k))
}

/// D3 실행 배치(§4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Placement {
    /// 공유 탭에서 `CONNECT` 앞에 서버로 갈 문장이 있다 — 실행하지 않고 안내한다(D-99).
    Refuse,
    /// 지금 세션에서 그대로 실행.
    Run,
    /// 이 탭에 전용 세션을 새로 만든다.
    NewPrivate,
    /// 이미 전용 세션 — 그 세션이 대상을 바꾼다.
    Retarget,
    /// 전용 세션을 닫는다(실행하지 않음).
    ClosePrivate,
}

pub(crate) fn placement(
    intent: Option<&ConnectIntent>,
    is_private: bool,
    private_connect: bool,
    preceded: bool,
) -> Placement {
    match intent {
        Some(ConnectIntent::Connect(_)) if private_connect => {
            if is_private {
                // 전용 탭 = 앞 문장은 지금 세션에서 돌고 CONNECT에서 대상을 바꾼다(정상).
                Placement::Retarget
            } else if preceded {
                Placement::Refuse
            } else {
                Placement::NewPrivate
            }
        }
        Some(ConnectIntent::Disconnect) if is_private => Placement::ClosePrivate,
        _ => Placement::Run,
    }
}

/// D4 끊긴 공유 세션 객체를 거둘 것인가 — 아무도 안 쓰고(묶인 탭 0 · 활성 아님) 되살릴 것도 아닐 때만.
pub(crate) fn reap_shared(
    connected: bool,
    blocked: bool,
    idle_closed: bool,
    is_default: bool,
    bound_tabs: usize,
) -> bool {
    !connected && !blocked && !idle_closed && !is_default && bound_tabs == 0
}

/// D5 탭 표식.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BadgeKind {
    Private,
    Shared,
    Off,
}

/// 표식은 **항상** 보인다(사용자 09-18 "항상 보여지도록") — 공유 = 중립 플러그 · 전용 = 강조 플러그 · 미연결/끊김 = 사선.
/// `multi_shared`는 더 이상 표시를 가르지 않는다(메뉴에서 고를 수 있는 것은 같다).
pub(crate) fn badge_kind(private: bool, multi_shared: bool, connected: bool) -> BadgeKind {
    let _ = multi_shared;
    match (connected, private) {
        (false, _) => BadgeKind::Off,
        (true, true) => BadgeKind::Private,
        (true, false) => BadgeKind::Shared,
    }
}

/// D6 통제가 화면에 내는 값 — (문장 실행, 전체 실행·Explain·페치·건수·새로고침, Commit/Rollback, 중지).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct GateView {
    pub run_statement: bool,
    pub run_other: bool,
    pub commit: bool,
    pub stop: bool,
}

pub(crate) fn gate_view(busy: bool, aux: u32, multi_caret: bool, has_pending: bool) -> GateView {
    let blocked = busy || aux > 0;
    GateView {
        run_statement: !blocked && !multi_caret,
        run_other: !blocked,
        commit: !blocked && has_pending,
        stop: blocked,
    }
}

/// 접속 해제를 시작한 경로(로그용 · docs/54 §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiscPath {
    Toolbar,
    ConnWin,
    Badge,
    Explorer,
    SessionsWin,
    Script,
    Idle,
    TabClose,
    Stop,
    Switch,
}

impl DiscPath {
    pub(crate) fn msg(self) -> Msg {
        match self {
            DiscPath::Toolbar => Msg::DiscPathToolbar,
            DiscPath::ConnWin => Msg::DiscPathConnWin,
            DiscPath::Badge => Msg::DiscPathBadge,
            DiscPath::Explorer => Msg::DiscPathExplorer,
            DiscPath::SessionsWin => Msg::DiscPathSessionsWin,
            DiscPath::Script => Msg::DiscPathScript,
            DiscPath::Idle => Msg::DiscPathIdle,
            DiscPath::TabClose => Msg::DiscPathTabClose,
            DiscPath::Stop => Msg::DiscPathStop,
            DiscPath::Switch => Msg::DiscPathSwitch,
        }
    }
}

/// D16 툴바 Disconnect의 뜻(docs/54 · 사용자 09-19) — 지금 탭이 쥔 연결의 종류와 그 연결을 함께 쓰는 탭 수로 정한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DisconnectPlan {
    /// 전용 세션 = 이 탭만의 것 → 바로 해제.
    Private,
    /// 공유 연결인데 이 탭만 쓴다 → 바로 해제.
    SharedAlone,
    /// 공유 연결을 다른 탭도 쓴다 → "모두 해제 / 이 탭만 떼기 / 취소"를 묻는다.
    SharedAsk,
}

/// 한 실행 안에서 도착한 결과 집합을 어느 결과 탭에 둘 것인가.
/// `None` = 실행을 시작한 탭(종전) · `Some(n)` = n번째 딸린 탭(0부터).
///
/// 규칙: **같은 문장이 두 번째 이후로 낸 결과**만 딸린 탭으로 간다(REF CURSOR 여러 개 · 암묵 결과 · 다중 결과 집합).
/// 다른 문장의 결과는 종전처럼 시작 탭을 덮는다(문장별 탭은 별도 결정). `children` = 이번 실행에서 이미 쓴 딸린 탭 수.
pub(crate) fn extra_result_slot(
    last_stmt: Option<usize>,
    stmt: usize,
    children: u32,
) -> Option<u32> {
    (last_stmt == Some(stmt)).then_some(children)
}

/// 오류 코드 표기에 쓸 방언 — 접속돼 있으면 세션의 방언 · **아직 아니면 붙으려던 대상의 방언**.
/// 접속 실패는 `Connected`보다 먼저 오므로 세션 방언(기본 Oracle · 또는 직전 서버)으로 분류하면 SQLite 실패가
/// `[ORA-00014]`로 보인다(86차 mac 점검 · T-148).
pub(crate) fn error_dialect(connected: bool, session: Dialect, target: Option<Dialect>) -> Dialect {
    match target {
        Some(d) if !connected => d,
        _ => session,
    }
}

/// `private` = 지금 세션이 전용 · `bound_tabs` = 지금(공유) 세션에 묶인 탭 수(전용이면 무시).
pub(crate) fn disconnect_plan(private: bool, bound_tabs: usize) -> DisconnectPlan {
    if private {
        DisconnectPlan::Private
    } else if bound_tabs > 1 {
        DisconnectPlan::SharedAsk
    } else {
        DisconnectPlan::SharedAlone
    }
}

/// D15 동작 직전 생존 판정(docs/53 §3): (빠른 판정을 할 것인가, 판정 뒤 재접속을 시도할 것인가).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct LivePlan {
    pub probe: bool,
    pub reconnect: bool,
}

/// `preflight` = UI 요청(신호등 ≠ 초록) · `suspect` = 직전 접속성 오류 · `dead_hint` = 드라이버가 끊김을 앎 · `stale` = 마지막 성공이 오래됨 ·
/// `allow` = 이 명령은 재접속해도 되는가(커밋/롤백 = 아니오) · `auto` = 설정 `connect.auto_reconnect`.
pub(crate) fn live_plan(
    preflight: bool,
    suspect: bool,
    dead_hint: bool,
    stale: bool,
    allow: bool,
    auto: bool,
) -> LivePlan {
    LivePlan {
        probe: preflight || suspect || dead_hint || stale,
        reconnect: (suspect || dead_hint) && allow && auto,
    }
}

/// 유휴 세션에 할 일(§6) — 순수 판정(시계·설정은 호출자가 준다).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IdleAction {
    Keep,
    /// 세션을 닫는다(스펙은 남겨 다음 실행 때 조용히 재접속).
    Close,
}

/// 유휴 정책 입력.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IdleInput {
    pub dialect: Dialect,
    pub private: bool,
    pub connected: bool,
    pub blocked: bool,
    /// 미커밋 문장·읽기 트랜잭션이 열려 있다 — 닫으면 잃는다.
    pub tx_open: bool,
    /// 세션 상태를 바꾸는 문장이 실행됐다([`alters_session_state`]) — 닫으면 그 설정·임시 데이터를 잃는다.
    pub stateful: bool,
    pub idle: Duration,
    /// `session.idle_secs`(0 = 끔).
    pub limit_secs: u64,
    /// `session.idle_shared`(공유 세션도 대상인가).
    pub include_shared: bool,
}

/// 유휴 미커밋 처리 방식(설정 `tx.idle_action` · docs/56 L2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TxIdleAction {
    Warn,
    Rollback,
    Commit,
}

impl TxIdleAction {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim() {
            "warn" => Self::Warn,
            "commit" => Self::Commit,
            _ => Self::Rollback,
        }
    }
}

/// 유휴 미커밋 판정의 입력(세션 하나 · 한 틱).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TxGuardIn {
    /// 대기 중인 변경이 있는가.
    pub pending: bool,
    /// 세션이 실행·보조 작업 중인가(통제 · docs/52 §3) — 그동안은 아무것도 하지 않는다.
    pub blocked: bool,
    /// 마지막 문장 실행 뒤 흐른 시간.
    pub idle: Duration,
    pub stale_min: u64,
    /// 0 = 한 번만.
    pub remind_min: u64,
    pub action: TxIdleAction,
    pub limit_min: u64,
    /// 마지막 경고 뒤 흐른 시간(없으면 아직 경고 전).
    pub since_warn: Option<Duration>,
    /// "나중에/연장"으로 미룬 중인가.
    pub snoozed: bool,
    /// 카운트다운 중이면 남은 시간(0 = 만료).
    pub counting: Option<Duration>,
}

/// 이번 틱에 할 일.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TxGuardStep {
    None,
    /// 경고 카드를 띄운다(첫 경고 또는 재알림).
    Warn,
    /// 자동 처리 카운트다운을 시작한다.
    StartCountdown,
    /// 카운트다운 진행 중(카드 갱신만).
    Counting,
    /// 카운트다운 만료 — 자동 처리(`action`)를 실행한다.
    Fire,
}

/// docs/56 L2 판정(순수 · MC/DC): 대기 변경 없음·작업 중 = 아무것도 안 함 → 카운트다운 중이면 그 흐름 → 미룸이면 쉼 →
/// 자동 처리 방식이고 한도를 넘겼으면 카운트다운 시작 → 경고 시점(첫 경고 = `stale_min` · 재알림 = `remind_min`)이면 경고.
pub(crate) fn tx_guard_step(i: TxGuardIn) -> TxGuardStep {
    if !i.pending || i.blocked {
        return TxGuardStep::None;
    }
    if let Some(left) = i.counting {
        return if left.is_zero() {
            TxGuardStep::Fire
        } else {
            TxGuardStep::Counting
        };
    }
    if i.snoozed {
        return TxGuardStep::None;
    }
    let idle_min = i.idle.as_secs() / 60;
    if i.action != TxIdleAction::Warn && idle_min >= i.limit_min {
        return TxGuardStep::StartCountdown;
    }
    if idle_min >= i.stale_min {
        let due = match i.since_warn {
            None => true,
            Some(d) => i.remind_min > 0 && d.as_secs() / 60 >= i.remind_min,
        };
        if due {
            return TxGuardStep::Warn;
        }
    }
    TxGuardStep::None
}

/// 닫아도 되는가: 접속돼 있고 · 한가하고 · 열린 트랜잭션이 없고 · 한도를 넘겼을 때만.
/// SQLite는 서버가 없다(파일 핸들 하나) — 닫아서 얻는 것이 없고 `:memory:`는 닫으면 데이터가 사라지므로 **항상 유지**.
pub(crate) fn idle_action(i: IdleInput) -> IdleAction {
    if i.limit_secs == 0
        || !i.connected
        || i.blocked
        || i.tx_open
        || i.stateful
        || i.dialect == Dialect::Sqlite
        || (!i.private && !i.include_shared)
        || i.idle.as_secs() < i.limit_secs
    {
        return IdleAction::Keep;
    }
    IdleAction::Close
}

/// 접속 유형에 따른 미커밋 기준(docs/56 §4 2차): **운영**이면 (첫 경고 분, 자동 처리 한도 분) 각각 전역 값과 운영 값 중
/// **작은 쪽** — 운영 접속이 전역 설정보다 느슨해지는 일은 없다. 그 외 유형은 전역 값 그대로.
pub(crate) fn tx_limits_for(prod: bool, global: (u64, u64), prod_vals: (u64, u64)) -> (u64, u64) {
    if prod {
        (
            global.0.min(prod_vals.0).max(1),
            global.1.min(prod_vals.1).max(1),
        )
    } else {
        global
    }
}

/// 운영 접속에서 **변경 문장**(DML·DDL·PL/SQL 블록 등 SELECT가 아닌 것)을 실행하려 한다 → 한 번 더 확인할 것인가(순수 판정).
pub(crate) fn prod_confirm_needed(prod: bool, setting_on: bool, items: &[String]) -> bool {
    prod && setting_on
        && items.iter().any(|sql| {
            let k = nsql_core::first_keyword(sql);
            !k.is_empty()
                && !matches!(
                    k.as_str(),
                    "SELECT"
                        | "WITH"
                        | "EXPLAIN"
                        | "SHOW"
                        | "DESC"
                        | "DESCRIBE"
                        | "VALUES"
                        | "PRAGMA"
                )
        })
}

/// 실행한 DDL의 탐색기 반영을 **커밋까지 미루는가**(docs/57 D-107 · 순수 판정): 트랜잭션 DDL 방언 · 수동 커밋 · 설정 켬이
/// 모두 참일 때만. 하나라도 거짓이면 실행이 끝난 직후 반영한다.
pub(crate) fn ddl_waits_for_commit(transactional: bool, autocommit: bool, on_commit: bool) -> bool {
    transactional && !autocommit && on_commit
}

#[cfg(test)]
mod tests {
    /// 딸린 결과 탭 판정 — 같은 문장의 두 번째 결과부터만.
    #[test]
    fn extra_result_slot_rules() {
        use super::extra_result_slot as f;
        assert_eq!(f(None, 0, 0), None, "첫 결과 = 시작 탭");
        assert_eq!(
            f(Some(0), 0, 0),
            Some(0),
            "같은 문장의 두 번째 결과 = 첫 딸린 탭"
        );
        assert_eq!(f(Some(0), 0, 1), Some(1), "세 번째 = 둘째 딸린 탭");
        assert_eq!(f(Some(0), 1, 1), None, "다른 문장 = 시작 탭(종전)");
    }

    /// 오류 표기 방언(MC/DC) — 조건 둘(접속됨 · 대상 방언 있음)이 각각 혼자 결과를 바꾼다.
    #[test]
    fn error_dialect_mcdc() {
        use super::error_dialect as f;
        use nsql_core::Dialect::{Oracle, Sqlite};
        // 기준: 미접속 + 대상 있음 → 대상.
        assert_eq!(f(false, Oracle, Some(Sqlite)), Sqlite);
        // 접속됨만 뒤집음 → 세션.
        assert_eq!(f(true, Oracle, Some(Sqlite)), Oracle);
        // 대상 없음만 뒤집음 → 세션.
        assert_eq!(f(false, Oracle, None), Oracle);
    }

    /// 접속 유형: 운영 = 더 엄격한 쪽 · 그 외 = 전역 그대로 / 실행 확인 = 운영 · 설정 · 변경 문장 셋이 모두 참일 때만.
    #[test]
    fn conn_env_rules() {
        use super::{prod_confirm_needed as need, tx_limits_for as lim};
        assert_eq!(lim(false, (10, 30), (5, 10)), (10, 30));
        assert_eq!(lim(true, (10, 30), (5, 10)), (5, 10));
        assert_eq!(
            lim(true, (3, 8), (5, 10)),
            (3, 8),
            "전역이 더 엄격하면 전역"
        );
        let dml = vec!["select 1".to_string(), "update t set a = 1".to_string()];
        let ro = vec![
            "select 1".to_string(),
            "with x as (select 1) select * from x".to_string(),
        ];
        assert!(need(true, true, &dml));
        assert!(!need(false, true, &dml), "운영 아님");
        assert!(!need(true, false, &dml), "설정 끔");
        assert!(!need(true, true, &ro), "조회만");
    }

    /// MC/DC: 세 조건 각각이 혼자서 결과를 뒤집는 쌍.
    #[test]
    fn ddl_waits_for_commit_mcdc() {
        use super::ddl_waits_for_commit as f;
        assert!(f(true, false, true));
        assert!(!f(false, false, true), "방언");
        assert!(!f(true, true, true), "자동 커밋");
        assert!(!f(true, false, false), "설정");
    }

    /// docs/56 L2 — 유휴 미커밋 판정 MC/DC.
    #[test]
    fn tx_guard_step_mcdc() {
        use super::{tx_guard_step, TxGuardIn, TxGuardStep, TxIdleAction};
        use std::time::Duration;
        let min = |m: u64| Duration::from_secs(m * 60);
        let base = TxGuardIn {
            pending: true,
            blocked: false,
            idle: min(12),
            stale_min: 10,
            remind_min: 10,
            action: TxIdleAction::Rollback,
            limit_min: 30,
            since_warn: None,
            snoozed: false,
            counting: None,
        };
        assert_eq!(
            tx_guard_step(base),
            TxGuardStep::Warn,
            "10분 넘음 · 첫 경고"
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                pending: false,
                ..base
            }),
            TxGuardStep::None
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                blocked: true,
                ..base
            }),
            TxGuardStep::None
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                idle: min(9),
                ..base
            }),
            TxGuardStep::None,
            "아직"
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                snoozed: true,
                ..base
            }),
            TxGuardStep::None,
            "미룸"
        );
        let warned = TxGuardIn {
            since_warn: Some(min(3)),
            ..base
        };
        assert_eq!(tx_guard_step(warned), TxGuardStep::None, "재알림 간격 전");
        assert_eq!(
            tx_guard_step(TxGuardIn {
                since_warn: Some(min(10)),
                idle: min(22),
                ..base
            }),
            TxGuardStep::Warn,
            "재알림"
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                since_warn: Some(min(50)),
                remind_min: 0,
                idle: min(25),
                ..base
            }),
            TxGuardStep::None,
            "remind 0 = 한 번만"
        );
        let over = TxGuardIn {
            idle: min(31),
            since_warn: Some(min(1)),
            ..base
        };
        assert_eq!(tx_guard_step(over), TxGuardStep::StartCountdown);
        assert_eq!(
            tx_guard_step(TxGuardIn {
                action: TxIdleAction::Warn,
                since_warn: Some(min(1)),
                ..over
            }),
            TxGuardStep::None,
            "경고만 = 자동 처리 없음"
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                snoozed: true,
                ..over
            }),
            TxGuardStep::None,
            "연장 중에는 카운트다운도 쉼"
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                counting: Some(Duration::from_secs(20)),
                ..over
            }),
            TxGuardStep::Counting
        );
        assert_eq!(
            tx_guard_step(TxGuardIn {
                counting: Some(Duration::ZERO),
                ..over
            }),
            TxGuardStep::Fire
        );
        assert_eq!(TxIdleAction::parse("warn"), TxIdleAction::Warn);
        assert_eq!(TxIdleAction::parse("bogus"), TxIdleAction::Rollback);
    }

    /// D16 — 전용/공유 · 묶인 탭 수: 조건별 독립 영향.
    #[test]
    fn disconnect_plan_mcdc() {
        use super::{disconnect_plan, DisconnectPlan};
        assert_eq!(
            disconnect_plan(true, 5),
            DisconnectPlan::Private,
            "전용이면 탭 수 무관"
        );
        assert_eq!(disconnect_plan(false, 1), DisconnectPlan::SharedAlone);
        assert_eq!(disconnect_plan(false, 0), DisconnectPlan::SharedAlone);
        assert_eq!(disconnect_plan(false, 2), DisconnectPlan::SharedAsk);
    }

    use super::*;

    #[test]
    fn mode_parses_with_shared_default() {
        assert_eq!(SessionMode::parse(None), SessionMode::Shared);
        assert_eq!(SessionMode::parse(Some("shared")), SessionMode::Shared);
        assert_eq!(
            SessionMode::parse(Some("per-editor")),
            SessionMode::PerEditor
        );
        assert_eq!(SessionMode::parse(Some("per-tab")), SessionMode::PerEditor);
        assert_eq!(SessionMode::parse(Some("?")), SessionMode::Shared);
    }

    /// 프로필 이름 · 접속 문자열(따옴표) 둘 다 · 첫 접속 명령만 본다 · `CONNECT BY` 조각은 접속이 아니다.
    #[test]
    fn connect_intent_finds_first_connection_command() {
        assert_eq!(connect_intent("select 1 from dual;"), None);
        let Some(ConnectIntent::Connect(s)) = connect_intent("CONNECT prod\nselect 1;") else {
            panic!("profile name");
        };
        assert_eq!(s.user.as_deref(), Some("prod"));
        assert!(s.host.is_none());
        let Some(ConnectIntent::Connect(s)) =
            connect_intent("select 1;\nconnect \"oracle:192.0.0.1:1521/DB\"\n")
        else {
            panic!("connection string");
        };
        assert_eq!(s.dialect, Some(Dialect::Oracle));
        assert_eq!(s.host.as_deref(), Some("192.0.0.1"));
        assert_eq!(
            connect_intent("disconnect\nconnect prod\n"),
            Some(ConnectIntent::Disconnect)
        );
        assert_eq!(
            connect_intent("SELECT 1 FROM t\nSTART WITH a IS NULL\nCONNECT BY PRIOR a = b;"),
            None
        );
        assert_eq!(connect_intent("CONNECT BY PRIOR a = b;"), None);
    }

    /// D1 MC/DC: private · bound · bound_alive 각각이 단독으로 결과를 바꾼다.
    #[test]
    fn mcdc_route_tab() {
        // 기준: 전용 없음 · 묶임 있음 · 살아 있음 → 묶인 세션.
        assert_eq!(route_tab(None, Some(7), true, 1), 7);
        assert_eq!(route_tab(Some(9), Some(7), true, 1), 9, "private만 뒤집음");
        assert_eq!(route_tab(None, None, true, 1), 1, "bound만 뒤집음");
        assert_eq!(
            route_tab(None, Some(7), false, 1),
            1,
            "bound_alive만 뒤집음"
        );
    }

    fn sv(id: u64) -> SharedView {
        SharedView {
            id,
            same_server: false,
            connected: true,
            blocked: false,
            idle_closed: false,
            bound_tabs: 0,
        }
    }

    /// D2 MC/DC: 같은 서버(중복 금지) · 바쁨 · 재활용 조건 4개 · 상한.
    #[test]
    fn mcdc_login_plan() {
        let with = |f: fn(&mut SharedView)| {
            let mut s = sv(1);
            f(&mut s);
            s
        };
        // ① 같은 서버 + 접속됨 → Use · same_server만 끄면 New · blocked만 켜면 Busy.
        assert_eq!(
            login_plan(&[with(|s| s.same_server = true)], 4),
            LoginPlan::Use(1)
        );
        assert_eq!(login_plan(&[sv(1)], 4), LoginPlan::New);
        assert_eq!(
            login_plan(
                &[with(|s| {
                    s.same_server = true;
                    s.blocked = true;
                })],
                4
            ),
            LoginPlan::Busy(1)
        );
        // 같은 서버라도 "살아 있는 근거"(connected ∨ idle_closed ∨ blocked)가 하나도 없으면 중복이 아니다 → 재활용.
        let dead_same = with(|s| {
            s.same_server = true;
            s.connected = false;
        });
        assert_eq!(login_plan(&[dead_same], 4), LoginPlan::Recycle(1));
        let idle_same = with(|s| {
            s.same_server = true;
            s.connected = false;
            s.idle_closed = true;
        });
        assert_eq!(
            login_plan(&[idle_same], 4),
            LoginPlan::Use(1),
            "idle_closed만 뒤집음"
        );
        // ② 재활용: 기준 = 끊김·한가·유휴 아님·묶인 탭 0(세션 둘) → Recycle(2).
        let dead = |f: fn(&mut SharedView)| {
            let mut s = sv(2);
            s.connected = false;
            f(&mut s);
            s
        };
        assert_eq!(login_plan(&[sv(1), dead(|_| {})], 4), LoginPlan::Recycle(2));
        assert_eq!(
            login_plan(&[sv(1), dead(|s| s.connected = true)], 4),
            LoginPlan::New
        );
        assert_eq!(
            login_plan(&[sv(1), dead(|s| s.blocked = true)], 4),
            LoginPlan::New
        );
        assert_eq!(
            login_plan(&[sv(1), dead(|s| s.idle_closed = true)], 4),
            LoginPlan::New
        );
        assert_eq!(
            login_plan(&[sv(1), dead(|s| s.bound_tabs = 2)], 4),
            LoginPlan::New
        );
        // 묶인 탭이 있어도 세션이 하나뿐이면 재활용(종전 동작: 모든 탭이 새 접속을 따른다) — only만 뒤집음.
        assert_eq!(
            login_plan(&[dead(|s| s.bound_tabs = 2)], 4),
            LoginPlan::Recycle(2)
        );
        // ③ 상한: len < max만 뒤집음.
        assert_eq!(login_plan(&[sv(1), sv(2)], 3), LoginPlan::New);
        assert_eq!(login_plan(&[sv(1), sv(2)], 2), LoginPlan::Limit);
    }

    /// D3 MC/DC: intent 종류 · is_private · private_connect.
    #[test]
    fn mcdc_placement() {
        let c = ConnectIntent::Connect(ConnectSpec::default());
        let d = ConnectIntent::Disconnect;
        assert_eq!(
            placement(Some(&c), false, true, false),
            Placement::NewPrivate
        );
        assert_eq!(
            placement(Some(&c), true, true, false),
            Placement::Retarget,
            "is_private만"
        );
        assert_eq!(
            placement(Some(&c), false, false, false),
            Placement::Run,
            "private_connect만"
        );
        assert_eq!(
            placement(None, false, true, false),
            Placement::Run,
            "intent만"
        );
        assert_eq!(
            placement(Some(&c), false, true, true),
            Placement::Refuse,
            "preceded만(D-99)"
        );
        assert_eq!(
            placement(Some(&c), true, true, true),
            Placement::Retarget,
            "전용 탭은 앞 문장 허용"
        );
        assert_eq!(
            placement(Some(&c), false, false, true),
            Placement::Run,
            "설정이 꺼져 있으면 종전대로"
        );
        assert_eq!(
            placement(Some(&d), true, true, false),
            Placement::ClosePrivate
        );
        assert_eq!(
            placement(Some(&d), false, true, false),
            Placement::Run,
            "공유 탭의 DISCONNECT는 워커가"
        );
        assert_eq!(
            placement(Some(&d), true, false, false),
            Placement::ClosePrivate,
            "설정과 무관"
        );
    }

    #[test]
    fn strip_first_connect_keeps_lines_and_rest() {
        let src = "CONNECT prod\nselect 1;\nCONNECT other\n";
        let out = strip_first_connect(src);
        assert_eq!(out.lines().count(), src.lines().count());
        assert!(out.starts_with("            \nselect 1;"), "{out:?}");
        assert!(out.contains("CONNECT other"), "두 번째는 그대로");
        assert_eq!(strip_first_connect("select 1;"), "select 1;");
    }

    #[test]
    fn statements_before_connect_detects_only_server_bound_items() {
        assert!(!statements_before_connect("CONNECT prod\nselect 1;"));
        assert!(statements_before_connect("select 1;\nCONNECT prod\n"));
        assert!(statements_before_connect("EXEC :v := 1\nCONNECT prod\n"));
        // 클라이언트 명령(REM · PROMPT · DEFINE)은 서버로 가지 않는다.
        assert!(!statements_before_connect(
            "REM hi\nPROMPT x\nDEFINE a=1\nCONNECT prod\n"
        ));
        // 접속 명령이 없는 스크립트는 호출자가 intent로 먼저 거른다(여기서는 참이어도 쓰이지 않는다).
        assert!(statements_before_connect("select 1;"));
    }

    /// §6-4: 세션에 남는 것을 만드는 문장 = 참 · 평범한 조회/DML/영구 DDL = 거짓.
    #[test]
    fn session_state_classifier() {
        for s in [
            "ALTER SESSION SET NLS_DATE_FORMAT='YYYY-MM-DD'",
            "alter session set current_schema = HR",
            "SET search_path TO app",
            "SET NOCOUNT ON",
            "USE master",
            "CREATE TEMP TABLE t(a int)",
            "CREATE GLOBAL TEMPORARY TABLE g(a int) ON COMMIT PRESERVE ROWS",
            "CREATE PRIVATE TEMPORARY TABLE ora$ptt_x(a int)",
            "SELECT * INTO #t FROM emp",
            "INSERT INTO #t VALUES (1)",
            "PREPARE p AS SELECT 1",
            "LISTEN ch",
            "BEGIN pkg.init; END;",
            "DECLARE v NUMBER; BEGIN v := 1; END;",
            "EXEC sp_set_session_context 'k', 1",
            "CALL set_ctx()",
            "SELECT set_config('app.user', 'x', false)",
            "SELECT pg_advisory_lock(1)",
            "SELECT GET_LOCK('a', 1)",
            "SELECT @x := 1",
            "SET @x = 1",
            "PRAGMA foreign_keys = ON",
            "ATTACH 'a.db' AS a",
        ] {
            assert!(alters_session_state(s), "{s}");
        }
        for s in [
            "SELECT * FROM emp",
            "UPDATE emp SET sal = sal + 1",
            "INSERT INTO t VALUES (1)",
            "DELETE FROM t",
            "CREATE TABLE t(a int)",
            "ALTER TABLE t ADD b int",
            "DROP TABLE t",
            "COMMIT",
            "WITH x AS (SELECT 1 a FROM dual) SELECT * FROM x",
        ] {
            assert!(!alters_session_state(s), "{s}");
        }
    }

    /// D4 MC/DC: 다섯 조건 각각이 단독으로 "거두지 않음"을 만든다.
    #[test]
    fn mcdc_reap_shared() {
        assert!(reap_shared(false, false, false, false, 0));
        assert!(!reap_shared(true, false, false, false, 0));
        assert!(!reap_shared(false, true, false, false, 0));
        assert!(!reap_shared(false, false, true, false, 0));
        assert!(!reap_shared(false, false, false, true, 0));
        assert!(!reap_shared(false, false, false, false, 1));
    }

    /// D5 MC/DC.
    #[test]
    fn mcdc_badge_kind() {
        assert_eq!(
            badge_kind(false, false, true),
            BadgeKind::Shared,
            "공유 하나뿐이어도 보인다"
        );
        assert_eq!(
            badge_kind(true, false, true),
            BadgeKind::Private,
            "private만"
        );
        assert_eq!(
            badge_kind(false, true, true),
            BadgeKind::Shared,
            "multi_shared는 표시를 가르지 않는다"
        );
        assert_eq!(
            badge_kind(true, false, false),
            BadgeKind::Off,
            "connected만(전용)"
        );
        assert_eq!(
            badge_kind(false, true, false),
            BadgeKind::Off,
            "connected만(공유)"
        );
        assert_eq!(
            badge_kind(false, false, false),
            BadgeKind::Off,
            "미연결은 늘 보인다"
        );
    }

    /// D6 MC/DC: busy · aux · multi_caret · has_pending.
    #[test]
    fn mcdc_gate_view() {
        let open = gate_view(false, 0, false, true);
        assert_eq!(
            open,
            GateView {
                run_statement: true,
                run_other: true,
                commit: true,
                stop: false
            }
        );
        let busy = gate_view(true, 0, false, true);
        assert_eq!(
            busy,
            GateView {
                run_statement: false,
                run_other: false,
                commit: false,
                stop: true
            }
        );
        assert_eq!(
            gate_view(false, 1, false, true),
            busy,
            "aux만으로도 똑같이 막힌다"
        );
        let multi = gate_view(false, 0, true, true);
        assert!(!multi.run_statement && multi.run_other && multi.commit && !multi.stop);
        let nopend = gate_view(false, 0, false, false);
        assert!(nopend.run_statement && nopend.run_other && !nopend.commit);
    }

    /// D15 MC/DC: probe = 네 조건의 OR(각각 단독으로 켠다) · reconnect = (suspect ∨ dead) ∧ allow ∧ auto(각각 단독으로 끈다).
    #[test]
    fn mcdc_live_plan() {
        let off = live_plan(false, false, false, false, true, true);
        assert_eq!(
            off,
            LivePlan {
                probe: false,
                reconnect: false
            }
        );
        assert!(
            live_plan(true, false, false, false, true, true).probe,
            "preflight만"
        );
        assert!(
            live_plan(false, false, false, true, true, true).probe,
            "stale만"
        );
        let s = live_plan(false, true, false, false, true, true);
        assert!(s.probe && s.reconnect, "suspect = 판정 + 재접속");
        let d = live_plan(false, false, true, false, true, true);
        assert!(d.probe && d.reconnect, "dead_hint = 판정 + 재접속");
        assert!(
            !live_plan(false, true, false, false, false, true).reconnect,
            "allow만 끔(커밋/롤백)"
        );
        assert!(
            !live_plan(false, true, false, false, true, false).reconnect,
            "auto만 끔"
        );
        assert!(
            !live_plan(false, false, false, true, true, true).reconnect,
            "stale만으로는 재접속 안 함"
        );
    }

    fn input() -> IdleInput {
        IdleInput {
            dialect: Dialect::Oracle,
            private: true,
            connected: true,
            blocked: false,
            tx_open: false,
            stateful: false,
            idle: Duration::from_secs(3600),
            limit_secs: 1800,
            include_shared: false,
        }
    }

    #[test]
    fn idle_close_only_when_safe() {
        assert_eq!(idle_action(input()), IdleAction::Close);
        // 끔 · 아직 · 바쁨 · 트랜잭션 열림 · 미접속 = 유지.
        for f in [
            (|i: &mut IdleInput| i.limit_secs = 0) as fn(&mut IdleInput),
            |i| i.idle = Duration::from_secs(10),
            |i| i.blocked = true,
            |i| i.tx_open = true,
            // 세션 상태(설정·임시 데이터·패키지 상태)가 있는 세션은 닫지 않는다(D-103).
            |i| i.stateful = true,
            |i| i.connected = false,
            // SQLite는 닫지 않는다(:memory: 데이터 소실 · 서버 부하 0).
            |i| i.dialect = Dialect::Sqlite,
            // 공유 세션은 설정을 켰을 때만.
            |i| i.private = false,
        ] {
            let mut i = input();
            f(&mut i);
            assert_eq!(idle_action(i), IdleAction::Keep, "{i:?}");
        }
        let mut i = input();
        i.private = false;
        i.include_shared = true;
        assert_eq!(idle_action(i), IdleAction::Close);
    }
}
