# 110 · Oracle Thin 드라이버 — 어댑터 설계 · 설정 2중 종속 · 기능 변화 (사용자 10-13)

> **요구**(사용자 10-13 · 협업 세션에 직접): ① "Rust 기반으로 Instant Client 없이 Oracle 접속이 가능한지 · 방법 · 관련 기술" ② "드라이버 선택 설정을 신설해 Thin / Instant Client 중 고르게 하고, Instant Client를 골랐을 때만 관련 설정이 동작(이미 `oracle.client_mode`에 종속된 설정이 있으니 2중 종속)" ③ "처음부터 Adaptor 기법을 요청했으니 Thin으로 바뀌어도 하위 로직이 변경되지 않게 — 어디서 어떻게 적용되는지 방식과 변경 범위" ④ "Thin으로 바뀌었을 때 지금 동작하는 기능 중 못 쓰는 것을 표로" ⑤ "개발 요청 전까지는 정보 수집·분석·기초 설계만 · 개발 요청은 개발 세션에서".
> **상태**: 기초 설계 완료 → **10-13 개발 착수**(사용자 지시 · D-277~D-280 권장안 채택 · journal 10-13 §2) · 이전 = 기초 설계(개발 착수 전) · 협업 세션 조사 3갈래(설정 틀 · 드라이버 경계 · `oracledb` API) + 개발 세션 Deep 검토 A~D(10-13) + 실서버 읽기 전용 조회(BISCM 19c) · 결정 후보 D-277~D-280 · 단계 T-333~T-336(T-159를 쪼갬).
> **짝 문서**: [64 §4](64-dbms-clients-and-driver-packaging.md)(경량판 · Instant Client 없이) · [22 §0·§4-1](22-driver-extensions.md)(드라이버 배치 · 09-13 스파이크) · [30](30-architecture-patterns.md)(포트+레지스트리+설정) · [43 §3-6](43-fetch-model-and-result-tabs.md)(REF CURSOR 이어 받기) · [44 §6](44-transaction-log.md)(취소) · [87 §14](87-grid-data-editing.md)(데이터 보호) · [89](89-bulk-io-review.md)(대량 적재).

## §0. 결론

1. **가능하다.** Oracle 공식 순수 Rust 드라이버 **`oracledb`**(github.com/oracle/rust-oracledb · `26.0.0-beta.4` · 2026-09-23)가 Instant Client 없이 TNS/TTC 프로토콜을 직접 말한다. 09-13 스파이크(22 §4-1)를 막았던 **OUT/IN OUT 바인드 · REF CURSOR**가 이제 문서화돼 있다. 아직 **beta**이고 **실행 중 취소가 없다**.
2. **어댑터 경계는 이미 있다 — 위층(core·script·run·catalog·GUI·CLI 실행 경로) 변경 0.** 위층은 `Box<dyn Session>`과 `Dialect`/`Caps`만 안다. `oracle` 크레이트(ODPI-C)를 의존하는 크레이트는 `nsql-drivers` 하나뿐이다. 드라이버 선택은 **`nsql_drivers::open`의 `Dialect::Oracle` 가지 한 자리**(`crates/nsql-drivers/src/lib.rs:55`)에서 한다.
3. **설정 2중 종속은 지금 틀로는 안 된다**(키당 부모·조건 1개 · 연쇄 잠금 없음). **연쇄 잠금(부모가 잠기면 자식도 잠김)** 을 설정 창에 넣는 작은 확장으로 된다 — 종속 표는 트리로 남고 기존 3단 종속(`scroll.fast_hud_pos`)의 잠재 결함도 같이 고쳐진다.
4. **Thin으로 바꾸면 못 쓰거나 달라지는 기능**은 §6 표 — 핵심 = 실행 중 즉시 취소 · Implicit Result Sets · OS 인증(`/ as sysdba`) · NNE · 11g · 대량 적재 최속 경로(1차) · 10G 검증자 계정 로그인.
5. **기본값 = Thin**(D-278 · 10-13 사용자 결정 · 권장안 OCI를 뒤집음 · "설치 없이 바로") — 손실은 접속 직후 한 줄 토스트("즉시 취소(■) · 암묵 결과 집합 · OS 인증 · 네이티브 암호화 · 배열 적재 · 12c 미만 서버 미지원 - 자세한 내용은 로그 창 확인") + 로그 창 상세(• 6줄 + 전환 안내)로 알린다. OCI는 고르는 사람이 쓴다.

## §1. 조사 사실

### 1-1. Rust 드라이버 후보

| 크레이트 | 만든 곳 | 판 | 방식 | 판정 |
|---|---|---|---|---|
| **`oracledb`** | **Oracle 공식** | `26.0.0-beta.4`(09-23) · 정식판 없음 · 0.x 판은 다른 사람의 별개 프로젝트였고 전부 yank | 동기 API(tokio 없음) · MSRV 1.89 · UPL-1.0 OR Apache-2.0 | **채택 후보** — 유지 주체 · 동기 API가 우리 세션 스레드 구조와 맞음 |
| `oracle-rs` | 개인(stiang) | 0.1.7 | tokio 비동기 · TLS·지갑 · `RefCursor` 타입 | 보류 — 비동기라 어댑터 안에 런타임을 들여야 함 · 문서 빈약 |
| `oracle` 0.6(지금) | kubo(개인) | 0.6 | ODPI-C → OCI | 표준판에 유지 |

참고 구현(다른 언어 · 같은 원리): python-oracledb thin · node-oracledb thin(6.x) · JDBC thin · ODP.NET Managed · go-ora(순수 Go). Rust 공식판은 python thin과 같은 계열 제약(NNE 없음 등)을 가진다.

### 1-2. `oracledb` 능력 대조(우리 포트 관점)

| 영역 | 지원 | API · 주의 |
|---|---|---|
| 접속 | ✓ | `oracledb::connect(Config)` · `Config::set_connect_string`(Easy Connect · 서술자 · tnsnames 별칭) · `set_config_dir`(TNS_ADMIN · `IFILE` 따라감) · `set_auth_mode(AUTH_MODE_SYSDBA …)` · 프록시 `"proxy[user]"` · `set_new_password`/`change_password` · SID형 Easy Connect 없음(서술자/별칭으로) · `sqlnet.ora`·`oraaccess.xml` 안 읽음 · **TCP 접속 시간 제한 beta.4 미적용**(#35 · main에 수정) |
| 인증 | 비밀번호만 | 외부·Kerberos·토큰·LDAP·SEPS 지갑 ✗ · **12c 검증자만**(#6 · `10G 11G 12C` 계정 ORA-01017 확인 · `11G 12C`는 미검증) |
| 암호화 | TLS/mTLS ✓ · NNE ✗ | TCPS(PEM 지갑 `ewallet.pem`) · NNE = "No - use TLS instead" · rustls 기본 공급자 = aws-lc-rs(#34: ring과 섞이면 panic → `install_default()`) |
| 실행 | ✓ | `execute`/`execute_named`/`query`/`query_row` · `StatementBuilder`(`fetch_array_size`·`prefetch_rows`·`fetch_lobs`·`exclude_from_cache`) · SQL 끝 `;`/`/` 금지 · **빈 SQL `todo!()`**(#43) · **주석(annotation) 열 질의 panic**(#48) · **파싱 실패가 문장 캐시에 남아 같은 글이 계속 실패**(#51) |
| 메타 | ✓ | `Metadata`: `name`·`db_type`·`data_type()`(DDL 글)·`precision`·`scale`·`nullable`·`max_size` |
| 값 | ✓ | `FromDbValue` = String·정수·f64·`OracleNumber`·`OracleTimestamp`·Interval·`Vec<u8>`·`Lob`·`Cursor`·JSON·Vector · `Option<T>` = NULL · 지역 시간대 TIMESTAMP 흠(#8·#42) |
| 바인드 | ✓ | 위치·이름 · OUT = 타입 상수(`&DB_TYPE_NUMBER`) · IN OUT = 초기값 · 결과 = `ExecResult::out_bind_data()`(beta.4부터 이름으로도) · 위치 반복 자리표시자는 값 반복 · 트리거 DDL `:NEW/:OLD` 바인드 수 오류(#33 open) |
| REF CURSOR | ✓ | OUT `&DB_TYPE_CURSOR` → `Cursor`(수명 매개변수 없음 · Send+Sync · Iterator 지연 `next()`) → **페이지 이어 받기 구조상 가능**(문서 명시 아님 · 커서별 fetch size 못 정함 · close = Drop) |
| DBMS_OUTPUT | 수동 | `dbms_output.get_line` 바인드 루프(줄마다 왕복 1) |
| 경고 | ✓ | `Connection::last_warning()`(컴파일 오류 CREATE = Err 없이 성공) |
| **취소** | **✗** | #24 open(유지보수자 = 알려진 제약 · 미래 async API) · 다른 스레드 호출은 내부 mutex에서 막힘 · 서버 `CANCEL SQL` 무시(#23) · `KILL SESSION`은 됨(ORA-00028) · `set_call_timeout`은 연결을 닫을 수 있음(`UnableToRecover`) |
| 트랜잭션 | 부분 | `commit`/`rollback` · autocommit 플래그 없음(우리는 러너가 COMMIT을 보내므로 실해 없음) · 세이브포인트 = SQL · 2PC ✗ |
| LOB | ✓(흠) | 기본 인라인(String/`Vec<u8>`) · `fetch_lobs()` → `Lob` locator(`Read`/`Write`·`trim`) · 임시 LOB `create_lob` · **`Lob::read` 비ASCII(한글) CLOB 깨짐**(#18 open) |
| 배열 DML | ✓(흠) | `execute_batch(BindParameters::Slice)` · 행별 건수·batcherrors 없음 · **서버가 batch error를 보내면 `todo!()` panic**(#41 · PR #47 open) · direct path ✗ |
| Implicit Result Sets | **✗** | 부록 A "Not supported" |
| 그 밖 ✗ | — | 스크롤 커서 · 객체 타입·컬렉션 · AQ · CQN · TAF/AC · startup/shutdown · 11g(하한 12c · 문서상 검증 19c/21c/26ai) · 클라이언트 문자셋 UTF-8만 |
| 오류 | ✓ | `ErrorKind::DbError(DbError{code: ORA 번호, message, offset})` · 끊김 종류 `DeadConnection`/`NotConnected`/`UnableToRecover`/`CallTimeoutExceeded` · `std::error::Error` 미구현 추정 |
| 의존 | — | rustls 0.23(aws-lc-rs = C/asm 빌드 · Windows는 cmake/nasm이 필요할 수 있음) · webpki-roots · aes/cbc/pbkdf2/sha2 · chrono · uuid · whoami · rand |

### 1-3. 사용자 실서버(10-13 · 읽기 전용 조회 · 운영 SNOPDB_19c 제외)
- 저장된 Oracle 프로필 6개 = 전부 BISCM 계열 · **Oracle Database 19c EE**(192.168.0.58).
- `v$session_connect_info` = Encryption/Crypto-checksumming 서비스 줄만(알고리즘 어댑터 줄 없음) → **NNE 미사용**.
- 계정 BISCM·SQLEDU·BISCM_SB `PASSWORD_VERSIONS = 11G 12C` · `sec_case_sensitive_logon = TRUE` → #6 위험 "중간"(10G가 없어 확인된 실패 조건은 아님 · **스파이크 첫 확인 항목**).

## §2. 어댑터 경계 — 지금 구조(사실)

```
위층(엔진·러너·카탈로그·GUI·CLI) ── Box<dyn Session> · Dialect · Caps 만 안다
          │  Opener = FnMut(&ConnectSpec) -> Box<dyn Session>   (nsql-run/src/lib.rs:321)
          ▼
nsql-drivers::open(spec, default)   (nsql-drivers/src/lib.rs:44-66) ← ★ 유일한 레지스트리
          │  Dialect::Oracle => OracleSession::connect(spec)    (:55)
          ▼
nsql-driver-oracle (oracle 0.6 = ODPI-C → OCI)   ← oracle::* 타입은 여기 안에만
```

- **포트** = `nsql_core::Session`(`nsql-core/src/lib.rs:1156-1250`) — 필수 = `dialect`·`execute`·`fetch_cursor`·`commit`·`rollback` · 기본 구현 있음 = `caps`(= `Caps::of(dialect)`)·`cancel_handle`(None)·`fetch_cursor_page`(전부 읽기)·`cursor_supported`(false)·`fetch_next`·`close_cursor`·`set_option`·`set_message_sink`·`is_alive`(true)·`set_call_timeout`(무동작)·`bulk_begin`(Err)·`copy_out`(Err). 부속 = `CancelHandle: Send+Sync`(:1148) · `BulkSink`(bulk.rs:47) · `Caps`(caps.rs:80-117).
- **Driver/Connector trait은 없다** — 생성은 레지스트리가 구체 `XxxSession::connect`를 부른다.
- **접속 입구 전부가 `open` 한 자리를 지난다**(개발 세션 확인): 워커 opener(`nexa-sql/src/worker.rs:561-602` · 편집기 `CONNECT` 전용 세션 · 비밀번호 인출 618/660) · 접속 시험 스레드(worker.rs:1799-1804) · 메타 스레드 `open_meta`(explorer.rs:1257-1282) · CLI opener(`nsql-cli/src/main.rs:280-300`) · Import는 러너의 기존 세션을 씀(새 입구 아님).
- **누수 점검**(`oracle::`·`dpi` 타입 grep): core·script·run·catalog·io·cli·GUI **0건**. 위층에 있는 Oracle 의존은 전부 **방언·Caps·평문 SQL**(분할기 `/`·PL/SQL 블록 · 바인드 대문자 · `VARIABLE`/REFCURSOR · `SHOW ERRORS`=`ALL_ERRORS` · ROWID 키 · `CURRENT_SCHEMA` = `set_option("schema")`) → 두 드라이버가 공유.
- **위층에 있는 OCI 모양 함수**(타입 누수는 아님 · 설정 배관): `nsql_drivers::set_oracle_client`/`oracle_client_info`/`set_net_options`(Oracle = call timeout) — 호출처 `nexa-sql/src/main.rs:2421-2437`(`apply_oracle_client`)·`:2440-2553`(`dbms_info_values`) · `app/settings.rs:396-400` · `app/session.rs:1340-1345` · `nsql-cli/src/main.rs:368-372`.
- **계약으로 묶인 것**: 위층이 `DbError.code`의 **ORA 숫자**를 읽는다(`nsql-core/src/dberr.rs:171·194-218` 분류 · `nexa-sql/src/probe.rs:379-389` 접속 오류 · `worker.rs:280-283` ORA-01017/01005 비밀번호 다시 묻기). 열 `type_name`은 `NUMBER(10,2)`·`VARCHAR2(30)`·`TIMESTAMP(6)` 꼴 글자를 위층이 읽는다(`editable.rs:66-100` · `nsql-run/src/bulk.rs:126-150`).

## §3. 설계 — Thin 어댑터

### 3-1. 배치
- 새 크레이트 **`crates/nsql-driver-oracle-thin`**(`oracledb` 의존은 여기 안에만 · DR-3 원장 등재) · `nsql-drivers` feature **`oracle-thin`**.
- 빌드 조합: **표준판** = `oracle` + `oracle-thin` · **경량판 C** = `oracle-thin`만(64 §4-1 · Instant Client 없이 Oracle).
- `Dialect`는 늘리지 않는다(새 방언 = Caps·분할기·완성 전부 갈라짐) · `ConnectSpec`에 드라이버 칸을 싣지 않는다(접속 문자열·프로필·vault 키까지 번짐 · 요구는 "설정으로 선택").

### 3-2. 선택 자리(한 자리 원칙)
```rust
// nsql-drivers/src/lib.rs:55 (개념)
Dialect::Oracle => match oracle_driver() {        // 프로세스 전역 · set_oracle_driver()
    OracleDriver::Oci  => Ok(Box::new(nsql_driver_oracle::OracleSession::connect(spec)?)),
    OracleDriver::Thin => Ok(Box::new(nsql_driver_oracle_thin::ThinSession::connect(spec)?)),
}
```
- 선택 원천 = `nsql_drivers::set_oracle_driver(kind)` 전역(선례 = `set_mssql_encryption` :70 · `set_oracle_client` :154). GUI(`apply_oracle_client` 기동·변경)·CLI(main.rs:369)가 같은 설정을 넣는다 → GUI·CLI 같은 값.
- **빌드에 없는 드라이버 = 선택지 자체가 없다**(폴백이 아니다): 경량판은 `oracle.driver` 선택 항목이 하나뿐이고, 저장된 값이 없는 쪽을 가리키면 들어 있는 드라이버를 쓰며 `oracle.info_driver`에 "이 빌드에는 없음 → 들어 있는 드라이버"를 표시한다.
- **런타임 실패는 폴백하지 않는다**(D-279): 표준판에서 Instant Client를 못 찾았을 때(DPI-1047) 자동으로 Thin으로 넘어가지 않는다 · 오류 안내에 "`oracle.driver = thin`으로 Instant Client 없이 접속" 한 줄.
- 선택을 따르는 것은 `open`·`oracle_client_info`(정보 칸)·`available()`뿐이다. `set_net_options`(call timeout·keepalive)는 **두 드라이버 전역에 다 넣는다**(static · 선택과 무관 · 선례 = PG·MSSQL keepalive를 둘 다 넣는 drivers lib.rs:74-83).

### 3-3. 위층 불변의 근거 — 어댑터가 지켜야 할 계약(must-match)
| 계약 | OCI 어댑터(지금) | Thin 어댑터가 할 일 |
|---|---|---|
| 바인드 이름 = 대문자 · SQL에 실제 있는 이름만 바인드(ORA-01008 수정) | `stmt.bind_names()` 대문자 비교(lib.rs:652-660) | `Statement::bind_names()`로 같은 필터 |
| DATE/TIMESTAMP = ISO면 진짜 바인드 · 아니면 글 | :665-673 | `OracleTimestamp::new_date/new_timestamp` |
| OUT 수거 → `out_params` · REFCURSOR → `Value::Cursor(id)` | :758-795 | `out_bind_data()` → 같은 `Value` |
| 값 표현(`Value`) · 열 `type_name` 글 | `row_to_values` :369 · `OracleType` Display | **같은 글 꼴**로 만든다(`NUMBER(p,s)`·`VARCHAR2(n)`·`TIMESTAMP(6)` · DATE/TIMESTAMP 글 형식 · NUMBER = Int 또는 Decimal 글) — 대조 시험 필수 |
| `DbError{code: ORA 번호, position}` | `err()` :297 | **코드를 지어내지 않는다**: 서버가 준 ORA 번호가 있으면 `code = Some(n)` 그대로(ORA-01017/01005 → `stale_password` worker.rs:276) · 클라이언트 쪽 끊김(`DeadConnection`·`NotConnected`·`UnableToRecover`·`CallTimeoutExceeded`) = `code = None` + 메시지에 호스트 분류기가 읽는 낱말(probe.rs:379 `is_connection_error` 문구 표 = "not connected"·"connection closed"·"timed out"·"lost contact" 등 · 예 "thin: dead connection (connection closed)" · "thin: call timeout exceeded (timed out)") · `offset()` → `position` · 어댑터 대조 시험에 두 분류기 입력을 넣는다 |
| `set_option` 이름 = `serveroutput`·`fetch_size`·`schema`·`max_rows`·`autocommit` | :597-642 | 같은 이름 · `schema` = `ALTER SESSION SET CURRENT_SCHEMA` · `autocommit`은 **무시**(호출처 0건 · OCI 가지 lib.rs:618-640은 죽은 가지 · Oracle `Caps.server_autocommit=false`라 러너가 자동 커밋 모드의 COMMIT을 스스로 보냄 nsql-run 1150·2599 `autocommit_needs_commit`) |
| DBMS_OUTPUT = 실행 뒤 `GET_LINE` 루프 | :408-437 | 같은 SQL 그대로 이식 |
| REF CURSOR 이어 받기(`cursor_supported`·`fetch_cursor_page`·`fetch_next`·`close_cursor` · 세션당 열린 커서 1 · commit/rollback이 닫음) | :819-912 | `Cursor`를 맵에 소유 보관 · 페이지마다 `next()` N회 · 안 되면 `cursor_supported=false` → 호스트 43 §3-6 "전체 조회 + ⚠"가 이미 처리 |
| `is_alive` = 왕복 없이 | `conn.status()` :579 | `Cell<bool>`(`fn is_alive(&self)` · 단일 스레드라 Sync 불필요) · 끊김 네 종류를 본 execute/commit 경로가 내리고 접속 성공·정상 실행이 올린다 · 호스트 `live_plan`이 TCP SYN으로 보완 |
| `set_call_timeout` | :574 | `Connection::set_call_timeout` |
| `cancel_handle` | `OracleCancel` = `break_execution`(ORA-01013) | **None**(취소 API 없음) → 호스트 = 배치 경계 멈춤 + 두 번째 ■ 강제 중지(`run.force_stop_secs` · `DiscPath::ForceStop` = 세션 버림 · 10-09 구현) |
| `bulk_begin` | ArrayDml(`Connection::batch`) | **1차 = `Err`**(#41 panic 위험) → 러너 다중 행 INSERT 폴백(nsql-run/src/bulk.rs:530) · #41 고쳐지면 `execute_batch`로 |
| `caps()` | `Caps::of(Oracle)` | 기본 그대로(바인드 표기·OUT·커서·EXEC 틀 동일) · 1차는 `bulk_load`만 `None`으로 덮음 — **구조 변경 0**(Caps는 이미 세션 값 · 러너가 `session.caps()`를 넣음 nsql-run 1647·1719) |

### 3-4. 어댑터 안에서 흡수할 Thin 흠(위층에 새지 않게)
- 빈 SQL → 드라이버 호출 전 거름(#43) · 문장 캐시 끔 또는 사용자 SQL은 `exclude_from_cache()`(#51 · IDE 재실행 패턴).
- LOB = 기본 인라인 페치(#18 한글 깨짐 회피 · LOB 값 창은 이미 상한 16 MB).
- TCP 접속 시간 제한 = 호스트 TCP SYN 선검사(이미 있음 · `probe.timeout_ms`)로 흡수(#35).
- tnsnames 별칭 = OCI 어댑터의 `client.rs`(`tns_aliases`·`detect` = 파일 시스템만 · OCI 의존 없음)를 공용으로 쓰거나 `set_config_dir`에 같은 `TNS_ADMIN` 결정을 넘긴다.
- TLS = 워크스페이스는 rustls 0.21 + ring 0.17(tiberius 경유) · oracledb는 rustls 0.23 → 메이저가 달라 별개 크레이트로 공존 = #34(공급자 충돌) 해당 없음 · 남는 우려 = aws-lc-rs C/asm 빌드(§7 · 스파이크 ①).
- 트리거 DDL `:NEW/:OLD`(#33) = DDL이면 바인드 0개로 실행(OCI의 `bind_names` 대조 방식 대신).
- 오류 = `ErrorKind::DbError{code}` → `DbError.code = Some(ORA 번호)` · 끊김 종류 4 → `is_alive=false` + 위층 끊김 분류가 읽는 꼴.
- `todo!()`/panic 가능 경로는 사전 검사로 막는다(release `panic=abort` — 89·T-309의 tiberius `todo!()` 교훈과 같음).

## §4. 설계 — 설정(2중 종속)

### 4-1. 지금 틀의 한계(사실)
- `nsql-settings/src/lib.rs`: `enum Dep { On, NotEmpty, Eq(&str) }`(6735) · `DEPENDS: &[(child, parent, Dep)]`(6823) · `dependency(child)`(6974) = `.find()` → **키당 부모·조건 1개**(같은 자식 둘째 행은 조용히 무시).
- **연쇄 잠금 없음**: 설정 창 `apply_deps`(`nexa-sql/src/prefs_win.rs:740-766`)·`refresh`(459-465)가 부모의 **값**만 보고 부모가 잠겼는지는 안 본다(기존 3단 `scroll.fast_hud_pos → fast_hud → fast`도 같은 잠재 결함).
- 잠금은 UI만(값 유지 · CLI `config set` 됨 · `Settings::set`은 `INFO_KEYS`만 거부).
- 읽기 전용 여부는 `info_` 접두가 아니라 **`INFO_KEYS` 소속**(6766)이 정한다(규칙 원문 = 64 §2 끝 · 61 §1).

### 4-2. 키
| 키 | 종류 | 기본 | 종속 | 뜻 |
|---|---|---|---|---|
| **`oracle.driver`**(신설) | Choice `oci` \| `thin` | **`thin`**(D-278 사용자 결정 · 모르는 값 = Thin) | 없음(맨 위) | 드라이버 선택 · 라벨 "Oracle 드라이버" · 항목 "Instant Client(OCI)" / "Thin(순수 Rust · 설치 없음)" |
| **`oracle.info_driver`**(신설 · `INFO_KEYS`) | 읽기 전용 | — | 늘 잠김 | 지금 쓰일 드라이버 글 = mssql/pg/sqlite `info_driver`와 같은 꼴 · "내장 OCI(Instant Client x.y)" / "내장 Thin(순수 Rust) - 별도 클라이언트 설치가 필요 없습니다" / "이 빌드에는 OCI 없음 → Thin" |
| `oracle.client_mode` | Choice auto\|manual | auto | **`oracle.driver` = `oci`**(새 행) | 그대로 |
| `oracle.client_dir`·`oracle.tns_admin` | 폴더 | "" | `client_mode` = `manual`(기존) → 연쇄로 `driver` = `oci`도 | 그대로 |
| `oracle.info_library`·`info_version`·`info_sqlnet` | 읽기 전용 | — | 늘 잠김 | Thin이면 **값을 비움**("Thin에서는 쓰지 않음") · 칸은 숨기지 않음(자리 흔들림 없음) |
| `oracle.info_tnsnames` | 읽기 전용 | — | 늘 잠김 | Thin도 tnsnames를 읽으므로 **값 유지** |

- 사용자가 말한 `oracle.info_driver`는 **표시 칸**으로 살리고, 고르는 키는 `oracle.driver`로 나눈다(D-277 · `info_` = 읽기 전용 관례 · 다른 DBMS와 같은 모양).
- 프로필별 드라이버 지정(`?driver=thin`)은 하지 않는다(D-280 · 요구는 전역 설정 · 필요해지면 `ConnectSpec`·vault·접속 폼 변경과 함께 별도 설계).

### 4-3. 종속 트리 + 연쇄 잠금
```
oracle.driver ──Eq("oci")──▶ oracle.client_mode ──Eq("manual")──▶ oracle.client_dir
                                                  └─Eq("manual")──▶ oracle.tns_admin
```
- 변경 = `DEPENDS`에 한 줄(`("oracle.client_mode", "oracle.driver", Dep::Eq("oci"))`) + 설정 창 잠금 판정이 **조상을 걸어 올라가며** 하나라도 불만족이면 잠금(순수 함수 `locked_by_chain(key, value_of)` + MC/DC 시험 — 52 §12 규칙). `Dep::All`(한 키 다중 조건)은 만들지 않는다(표가 트리로 남는 쪽이 단순 · 94 키 규칙과 맞음).
- **한 자리 원칙**: 판정 함수는 nsql-settings의 순수 함수 `locked_by_chain(key, value_of: impl Fn(&str) -> &str) -> bool`(조상 걷기 깊이 상한 8 = 순환 방어)로 두고, 설정 창 `apply_deps`(prefs_win.rs:751)와 `refresh`(:463) **둘 다** 그 함수를 부른다(지금은 두 곳이 각자 `dependency()`를 봄) · MC/DC 시험은 nsql-settings에.
- 같은 판정을 `refresh`의 "잠긴 칸은 호스트 값 표시"에도 쓴다(자동 모드에서 탐지 경로를 흐리게 보이는 동작 유지).
- 효과: Thin을 고르면 `client_mode`·`client_dir`·`tns_admin`이 전부 잠기고 값은 남는다(다시 OCI로 오면 그대로 살아남).

### 4-4. 적용 경로 · 재시작
- `app/settings.rs:396`의 변경 감지 목록에 `"oracle.driver"` 추가 → `apply_oracle_client`에서 `set_oracle_driver` + 정보 칸 다시 계산(`dbms_info_values`).
- **재시작 불필요**: 선택은 접속마다 `open`에서 읽는다 · ODPI-C 1회 초기화는 OCI를 **처음 쓸 때** 지연 실행(`init_client` · `is_initialized` 가드)이라 thin → oci 전환도 그때. 이미 열린 Oracle 세션은 **다음 접속부터** 적용(안내 한 줄 · `set_net_options`와 같은 문구). 기존 제약(OCI 초기화 뒤 `client_dir` 변경은 재시작 뒤) 그대로.

## §5. 변경 범위

| 자리 | 변경 | 내용 |
|---|---|---|
| `crates/nsql-driver-oracle-thin`(신설) | **새로 작성** | `ThinSession: Session` · §3-3 계약 · §3-4 흡수 |
| `crates/nsql-drivers` | 작음 | feature `oracle-thin` · `open` Oracle 가지 분기 · `set_oracle_driver` · `oracle_client_info`에 드라이버 구분(`included` = "OCI 들어 있음" → `thin_included` 또는 목록) · `set_net_options`·`available()` |
| `crates/nsql-driver-oracle` | 0(선택: `client.rs` tnsnames 해석 공용화) | — |
| `crates/nsql-settings` | 작음 | 키 2(`oracle.driver`·`oracle.info_driver`) · `INFO_KEYS` 1 · `DEPENDS` 1행 |
| `crates/nsql-i18n` | 작음 | 라벨·설명·선택지·정보 글(`ValDbms…`) |
| `crates/nexa-sql` prefs_win.rs | 작음 | 연쇄 잠금(조상 걷기 · 순수 함수 + 시험) |
| `crates/nexa-sql` main.rs · app/settings.rs | 작음 | `apply_oracle_client`에 드라이버 선택 · 변경 감지 키 · `dbms_info_values` Thin 분기 |
| `crates/nsql-cli` main.rs | 한 줄 | 같은 설정 적용 |
| **nsql-core · nsql-script · nsql-run · nsql-catalog · nsql-io · GUI 실행 경로** | **0** | 포트·방언·Caps만 씀(§2 누수 0) |
| CI · 배포 | 작음 | 경량판 C 빌드 조합 · aws-lc-rs 빌드 도구(Windows) 확인 · 외부 crate 원장 |

## §6. Thin으로 바꿨을 때 — 못 쓰거나 달라지는 기능(지금 동작 기준)

| # | 기능(지금 OCI에서 동작) | Thin | 사용자에게 보이는 차이 · 대응 |
|---|---|---|---|
| 1 | **실행 중 즉시 취소**(■ → ORA-01013 · `break_execution`) | ✗(#24) | 첫 ■ = 다음 배치 경계에서 멈춤 · 긴 단일 문장은 안 멈춤 → 두 번째 ■ = 강제 중지(세션 버림 · 재접속) · 보강 후보 = 별도 접속 `KILL SESSION`(권한 필요) |
| 2 | **Implicit Result Sets**(`DBMS_SQL.RETURN_RESULT` 결과를 결과 탭으로 · OCI 어댑터 lib.rs:797-806 `implicit_result()` 전부 수거) | ✗ | 그 결과 탭이 **오류 없이 조용히 빠짐**(위층 변경 0) → 안내 = "Thin 미지원 · `:cur` REFCURSOR OUT 바인드 + `PRINT`로 대체 또는 OCI" |
| 3 | **OS 인증 `/ as sysdba`**(사용자·비밀번호 빈칸 = external auth) · Kerberos · SEPS 지갑 | ✗ | 접속 거부 → 안내(사용자/비밀번호 + `AUTH_MODE_SYSDBA`는 됨) |
| 4 | **NNE(네이티브 암호화·체크섬)** | ✗ | 서버가 NNE 필수면 접속 실패 → "TLS(TCPS) 또는 OCI" 안내 · 사용자 BISCM = NNE 미사용(§1-3) |
| 5 | **11g 이하 서버** | ✗(12c+) | 접속 오류 안내 |
| 6 | **10G 검증자를 가진 계정** | ✗(#6) | ORA-01017 → 안내(DBA 쪽 비밀번호 재설정) · `11G 12C` 계정은 스파이크로 확인 |
| 7 | **대량 적재 최속 경로**(`nsql import`·Import 창 = Oracle 배열 DML 32.5k행/s) | △(1차 Err) | 다중 행 INSERT 폴백으로 동작하나 느려짐 · #41 해결 뒤 `execute_batch`로 복귀 |
| 8 | `sqlnet.ora` 고급 옵션 · 클라이언트 문자셋 ≠ UTF-8 · Windows 레지스트리 TNS | ✗ | `oracle.info_sqlnet` 칸 비움 · 문자셋은 UTF-8 고정 |
| 9 | TCP 접속 시간 제한(드라이버 자체) | △(beta.4 미적용) | 호스트 TCP SYN 선검사로 같은 결과(사용자 차이 없음) · (10-13 mac 실측 CLI 서버 미도달 Thin 76 s vs OCI 63 s · T-345 · Linux 124차 도달 상태 connect Thin 0.50 s / OCI 0.62~0.74 s · 미도달 미측정) |
| 10 | LOB 스트리밍 읽기 | △(#18 한글) | 인라인 페치로 대체(LOB 값 창 16 MB 상한 그대로 · 사용자 차이 없음 예상) |
| 11 | REF CURSOR 페이지 이어 받기(T-202) | △(스파이크 확인) | 안 되면 "전체 조회 + ⚠"(이미 있는 처리) |
| 12 | 설정 창 Instant Client 정보(라이브러리·버전) | 해당 없음 | Thin이면 값 비움 · 대신 `oracle.info_driver` |
| 13 | **DATE 변수 바인드 타입** | △ | 값은 같으나 TIMESTAMP로 전송(`DUMP` Typ=18 · OCI Typ=12) · DATE 열과 비교할 때 인덱스를 못 탈 수 있음 · oracledb beta.4에 DATE IN 바인드 타입 없음 · 상류 이슈 후보 |
| 14 | **`VARCHAR2(n CHAR)` 열 타입 글** | △ | 서버가 보낸 바이트 길이를 드라이버가 버려 BYTE/CHAR 의미를 못 가름 → Thin = 선언 길이(`VARCHAR2(10)`) · OCI = 바이트 길이(`VARCHAR2(40)`) · 그 밖 CHAR/VARCHAR2 길이는 같음 |
| 15 | 세션 시간대 오프셋 | 접속당 왕복 1 추가 | `TIMESTAMP WITH LOCAL TIME ZONE`을 OCI처럼 세션 시각 + 오프셋으로 보이려고 접속 직후 `SYSTIMESTAMP AT TIME ZONE SESSIONTIMEZONE` 1회(26 §8) |
| 16 | LOB 열 타입 글(CLOB/BLOB/NCLOB) | 조회당 왕복 1 추가(LOB 열 있을 때만) | 인라인 페치가 메타를 LONG 계열로 바꿔 보고 → LONG 계열 열이 있는 조회만 `fetch_lobs` 문장으로 파싱 1회 해 진짜 타입 글로 보정(값·로케이터 없음 · 값은 인라인 그대로 = #18 회피) · **REF CURSOR 결과는 보정 못 함**(LONG으로 보일 수 있음) |
| — | OUT/IN OUT · DBMS_OUTPUT · PL/SQL · LOB 쓰기 · call timeout · tnsnames · Easy Connect · TLS · 프록시 · SYSDBA(비밀번호) · `CURRENT_SCHEMA` · 세이브포인트(SQL) · 데이터 보호 1행 검사(단건 실행) | ○ | 차이 없음 |

## §7. 위험 · 대응

| 위험 | 정도 | 대응 |
|---|---|---|
| beta(API 변경 · beta.3→4에서 타입 바뀜) | 높음 | 정확한 판 고정(`=26.0.0-beta.4`) · 어댑터 한 크레이트에 가둠 · 오르면 어댑터만 수정 |
| `todo!()`/panic 경로(#41·#43·#48) + 우리 release `panic=abort` | 높음 | 사전 검사(빈 SQL·주석만 문장 = 빈 결과) · bulk 1차 Err · 주석 열 질의는 회귀 시험에 넣어 감시 |
| 취소 없음 | 중간 | §6 #1 · 기본 Thin(D-278)이라 사용자가 먼저 만남 → 접속 토스트 + 첫 ■ 안내 + 둘째 ■ 강제 중지 |
| 로그인 검증자(#6) | 중간 | 스파이크 첫 항목 · 실패 시 안내문 |
| 문장 캐시 오류 고착(#51) | 중간 | 캐시 끔/제외 |
| aws-lc-rs 빌드(C/asm · Windows cmake/nasm) · 크기 | 중간 | 스파이크에서 3-OS 빌드·크기 측정 · 필요하면 rustls `ring` 공급자 검토 |
| 값·타입 글 형식 차이로 위층 판정(편집 가능 판정·bulk 타입) 흔들림 | 중간 | OCI ↔ Thin **같은 질의 대조 시험**(골든) |

## §8. 단계(TODO · T-159를 쪼갬)
- **T-333 스파이크**(확인 순서 = 개발 세션 10-13): ① 3-OS 빌드(툴체인 추가 없이 · aws-lc-rs cmake/nasm 요구 여부 · ring 백엔드 feature 유무 · 크기 증가 = 64 §4-1 "A + 드라이버 몫") ② BISCM 로그인(`11G 12C` 검증자 · #6) ③ OUT/IN OUT · REF CURSOR 페이지(`Cursor` 지연 Iterator) ④ DBMS_OUTPUT ⑤ 강제 중지(세션 버림) 뒤 서버 세션 정리 ⑥ Implicit result 빠짐 확인(예상 손실) ⑦ DATE/TIMESTAMP 바인드 · 한글 CLOB · `type_name`/값 글 대조 · call timeout · 4방언 E2E 회귀(grid-edit · conn-cmd · oracle-ddl) — 항목별 통과/실패를 이 문서 §11 표에.
- **T-334 어댑터**: `nsql-driver-oracle-thin` · §3-3 계약 · §3-4 흡수 · 레지스트리 분기 · feature.
- **T-335 설정**: `oracle.driver`·`oracle.info_driver` · `DEPENDS` 1행 · 설정 창 연쇄 잠금(순수 함수 + MC/DC) · 적용 경로·정보 칸 · CLI.
- **T-336 검증·배포**: 통합 시험(`nsql-drivers/tests/integration.rs` Oracle 3종 · `examples/it-oracle.sql` · `oracle-refcursor-pkg.sql` · `lob/oracle.sql` · `variables/oracle.sql`)을 두 드라이버로 · E2E(`oracle-ddl-e2e` · `pkg-overload-e2e` · `define-e2e` · grid-edit · bulk 폴백 · stop-latency = 취소 차이 확인) · 경량판 C 빌드 조합 · 외부 crate 원장 · 위키.

## §9. 결정(10-13 권장안 채택)
- **D-277** 키 모양 = 편집 `oracle.driver`(oci|thin) + 표시 `oracle.info_driver`(읽기 전용) — 권장안.
- **D-278** 기본값 = **`thin`**(10-13 사용자 결정 · 권장안 `oci`와 다름 · 사유 "설치 없이 바로").
- **D-279** Instant Client를 못 찾을 때 Thin 자동 폴백 = 하지 않음(고른 드라이버만 · 안내문에 Thin 전환 방법) — 권장안.
- **D-280** 프로필별 드라이버 지정 = 하지 않음(전역 설정만) — 권장안.

## §10. 출처
- https://github.com/oracle/rust-oracledb · https://docs.rs/oracledb/latest/oracledb/ · 가이드·부록 A·릴리스 노트 https://docs.rs/oracledb/latest/oracledb/guide/index.html · https://crates.io/api/v1/crates/oracledb
- 이슈 https://github.com/oracle/rust-oracledb/issues/6 · /18 · /23 · /24 · /33 · /34 · /35 · /41 · /43 · /48 · /51
- https://docs.rs/oracle-rs · python-oracledb Thin/Thick 차이 https://python-oracledb.readthedocs.io/en/latest/user_guide/appendix_b.html
- 코드 사실 = 협업 조사(10-13) + 개발 세션 Deep 검토 A~D(10-13) — 본문 file:line.

## §11. 스파이크 결과(T-333 · 10-13 V1 3회 · 협업)

| # | 항목 | 결과 | 메모 |
|---|---|---|---|
| ① | 3-OS 빌드 · 크기 | Windows ✓ | 개발 세션: oracledb + aws-lc-rs가 cmake·nasm 없이 빌드(2분 6초) · feature 조합 3(기본 oci+thin · 경량판 C sqlite+oracle-thin · sqlite+oracle) check ✓ · mac ✓ 10-13(Debug 3m44s 경고 0 · cmake 4.4.3·nasm 있음 · Release GUI 22,016,976 B = +4.1 MB · nsql 12,317,200 B = +4.0 MB · 121·122차 합산이라 Thin 단독 미분리) · **Linux ✓ 10-13 124차**(Ubuntu 26.04 VM 2코어 · **cmake·clang·nasm 없음**에서 Debug `-p nexa-sql -p nsql-cli` 5m12s 경고 0 · Release 8m43s 경고 0 · 크기 GUI 25,693,912 B(119차 20.40 MB 대비 +5.3 MB · 맥보다 +3.7 MB = ELF) · nsql 13,480,344 B(+4.5 MB)) |
| ② | BISCM 로그인(11G 12C) | ✓ | BISCM 19c · `11G 12C` 검증자 계정 Thin 접속 성공(CLI·GUI·통합 시험) |
| ③ | OUT/IN OUT · REF CURSOR 페이지 | ✓ | REF CURSOR `PRINT` · OUT · 1200행 페이지 같음 · PL/SQL 블록 읽기 전용 변수 초기값 NULL 결함 → 고침(재검증 ✓) |
| ④ | DBMS_OUTPUT | ✓ | `hello nexa 한글` 두 드라이버 같음 |
| ⑤ | 강제 중지 뒤 서버 세션 정리 | ✓(클라이언트) | 첫 ■ 안내 토스트 · 둘째 ■ 강제 중지(접속 끊음) ✓ · 서버 쪽 세션 잔존은 미측정 |
| ⑥ | Implicit result 빠짐 | ✓(안내) | 내부 오류(TTC type 27) → 제약 안내 오류 글 + 짧은 토스트 "암묵 결과 집합 미지원 - 자세한 내용은 로그 창 확인" + 로그 상세 · panic 없음 |
| ⑦ | 바인드·CLOB·값 글 대조 · E2E 회귀 | ✓ | CLI 동일성 E2E **PASS 19 · FAIL 0** · 실서버 통합 시험 **4/4**(LOB 열 타입 보정 뒤 · 두 드라이버 각각) · 알려진 차이 = §6 #13·#14·#16 |
