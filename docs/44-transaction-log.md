# 44 · 트랜잭션 로그(DBeaver Query Manager 방식) — 원본 검토 · 우리 설계 · 롤백 표시 보강 (사용자 요청 09-17)

> **요청**: *"Transaction log를 DBeaver처럼 만들어 줄 수 있어? 소스 레벨에서 구현 방식과 설계를 검토하고 도입 방법을 안내 · 롤백을 하고 다시 로그를 보니 실행은 성공했지만 내가 롤백한 내용은 표시가 안 되는데 이 부분을 추가해서 설계."*
> **위치**: [34 트랜잭션 UX](34-transaction-ux.md)(DR-30 · 대기 목록 `tx_pending`)의 **기록 계층** · [32 서버 메시지·로그](32-server-messages-and-live-log.md)(로그 창) 옆의 새 창 · [26](26-performance-architecture.md) Timeline이 시간 원천 · [39 §3](39-resource-governance.md) 부하원(메모리 링).
> 09-17 후반: 실행 취소 포트 `nsql_core::CancelHandle`(§4 (C) 취소 열) ✅ — SQLite `interrupt` · PG `cancel_query` · Oracle `break_execution` · **SQL Server = `mssql.encrypt=login`이면 TDS Attention(세션 유지) · `required`면 소켓 종료(트랜잭션 Lost · 재접속)**(journal 52차 SQL Server 취소).
> **상태**: ✅ 1차 구현(09-17 52차 · T-107 — `nsql-run::txlog` + `txlog_win.rs` + 호스트 `tx_close(outcome)`) · 잔여 = T-107b(`tx_pending` 원천 교체 · 우클릭 · 중지 연결 · 서버 롤백 판정 §4).

---

## 0. 결론 다섯 줄

1. DBeaver의 트랜잭션 로그 = **Query Manager(QM)** 의 문장 실행 메타(`QMMStatementExecuteInfo`)를 **현재 실행 컨텍스트·현재 savepoint**로 걸러 표로 보여주는 창이다. 별도 저장 구조가 아니라 QM 이벤트 히스토리(메모리 10,000건 링)를 필터한 것.
2. 커밋/롤백은 메타 객체의 **`committed` 플래그 하나**로만 남는다(`QMMTransactionInfo.commit()` = `committed=true` · `rollback()` = `committed=false` → savepoint `close(commit)`). 뷰어는 이 플래그로 **행 배경색만** 바꾸고, **Result 열은 오류가 아니면 항상 `Success`** — 그래서 사용자가 본 대로 "성공했지만 롤백됐다"가 글자로 남지 않는다.
3. 우리는 같은 골격(수집기 → 메모리 링 → 필터 → 표)을 따르되, 문장마다 **트랜잭션 결과를 상태 열로 명시**한다: `대기(pending)` · `커밋됨 hh:mm` · **`롤백됨 hh:mm`** · `암묵 커밋(DDL)` · `자동 커밋` · `접속 끊김(서버 롤백)` · `중지`. 색은 상태의 보조.
4. 수집 지점은 이미 있다: 러너 `RunEvent`(Begin/ResultSet/Done/Error/Timing) + 호스트의 커밋/롤백/암묵 커밋 판정(`tx_on_done` · `tx_clear`). 새 부품 `nsql-run::TxLog`(수집·링·필터 · UI 무의존)와 창 `txlog_win.rs`(로그 창 골격 재사용) 둘로 나눈다.
5. 시간·행수는 [26](26-performance-architecture.md) Timeline 단계(Execute·Fetch·OutputFlush)를 그대로 쓴다(중지 시점 표시도 같은 기준).

---

## 1. DBeaver 원본 검토(devel · 09-17 확인)

### 1-1. 구조

| 계층 | 클래스(패키지) | 역할 |
|---|---|---|
| 수집 | `runtime/qm/QMMCollectorImpl`(`org.jkiss.dbeaver.model`) — `QMExecutionHandler` 구현 | 드라이버 호출 훅: `handleContextOpen/Close` · `handleTransactionAutocommit/Commit/Rollback/Savepoint` · `handleStatementOpen/ExecuteBegin/ExecuteEnd/Close` · `handleResultSetOpen/Close/Fetch` · `handleFetchError`. 메타 객체를 만들고 `tryFireMetaEvent(object, BEGIN/END/UPDATE, ts, session)`으로 이벤트 풀에 넣는다. **메타 질의는 설정으로 건너뜀**(`isSkipMetadataQueries`). |
| 메타 모델 | `model/qm/meta/QMMSessionInfo → QMMConnectionInfo → QMMTransactionInfo → QMMTransactionSavepointInfo → QMMStatementInfo → QMMStatementExecuteInfo` | 실행 하나 = `QMMStatementExecuteInfo{statement, savepoint, queryString, fetchRowCount, updateRowCount(-1), errorCode, errorMessage, fetchBegin/EndTime, transactional, schema, catalog, previous}` · `getDuration()` = open~close + fetch 구간. |
| 트랜잭션 상태 | `QMMTransactionInfo{connection, previous, committed, savepointStack}` · `QMMTransactionSavepointInfo{transaction, name, committed, previous, lastExecute}` | `commit()` → `committed=true` + 열린 savepoint 전부 `close(true)` · `rollback(toSavepoint)` → `committed=false` + 스택을 대상까지 `close(false)`. savepoint의 실행 목록은 `lastExecute`에서 `previous`로 거슬러 가며 `exec.getSavepoint()==this`인 것만(연결 리스트 · 복사 없음). |
| 히스토리 | `QMMCollectorImpl.EventDispatcher` | 250ms마다 풀을 비워 리스너에 통지 · `MAX_HISTORY_EVENTS = 10000` 링(`pastEvents.subList`). 파일 저장은 별도(QMDB · 선택). |
| 필터 | `model/qm/filters/QMEventCriteria` · `QMEventFilter` | `objectTypes` · `queryTypes(DBCExecutionPurpose: USER/USER_FILTERED/USER_SCRIPT/UTIL/META/META_DDL)` · `searchString` · `containerId` · `sessionId` · `fetchingSize 200` · 기간·정렬. |
| 뷰어 | `ui/controls/querylog/QueryLogViewer`(`org.jkiss.dbeaver.ui.editors.sql`) | 열 = **Time · Type · Text · Duration · Rows · Result**(+DataSource · Context 숨김). Type = `"SQL / " + purpose 제목`(User·Util·Meta). Text = `getSingleLineString`(줄바꿈 → ¶는 SWT 표시). Result = 오류면 `[code] message` · 아니면 **`Success`**. |
| 트랜잭션 로그 창 | `ui/controls/txn/TransactionInfoDialog` → `TransactionLogDialog`(`org.jkiss.dbeaver.core`) | 제목 `Transaction log [<datasource> : <context>]` · **모덜리스** · 크기 기억(`DBeaver.TransactionLogDialog`) · 체크 2: **Show all queries**(User 외 Util/Meta 포함) · **Show previous transactions**(끄면 `exec.getSavepoint() == currentSP`만) · `createContextFilter`가 `QMEventFilter`를 만들어 `logViewer.setFilter`. |
| 툴바 배지 | `ui/controls/txn/TransactionMonitorToolbar` | 현재 savepoint의 갱신 문장 수(`QMUtils.getTransactionState`: 오류 아님 · META/UTIL 아님만 셈 · `txnStartTime` = 첫 문장) · 250ms 갱신 · 색 = 문장 수 0~400 그라디언트(`colorCommitted`→`colorReverted`) · 툴팁 = 실행 수·갱신 수·경과 초. |

### 1-2. 행 색 규칙(`QueryLogViewer.getObjectBackground` · 원문)

```java
if (exec.hasError())            return colorReverted;                      // 오류 = 빨강
sp = exec.getSavepoint();
if (sp == null)                 return null;                               // 자동 커밋 = 색 없음
else if (sp.isClosed())         return sp.isCommitted() ? colorUncommitted : colorTransaction;
else                            return colorUncommitted;                   // 열린(대기) 트랜잭션
```

색 이름이 뒤바뀌어 보이지만(닫힌+커밋 = `colorUncommitted`, 열린 = `colorUncommitted`, 닫힌+롤백 = `colorTransaction`) 실제 테마 기본값에서 **커밋된 행은 흰색, 대기 행은 노랑, 롤백된 행은 옅은 노랑/회색, 오류는 빨강**으로 보인다. 사용자 캡처 2장은 이 규칙과 일치한다: 롤백 뒤 새로 실행한 3문장 = 노랑(열린 savepoint) · **롤백된 UPDATE 2건 = 흰색·`Success`**(닫힌 savepoint에 `colorTransaction`이 테마상 거의 흰색이라 구별이 안 되고, Result 열은 성공 여부만).

### 1-3. 왜 롤백이 안 보이나(원인 세 가지)

1. **Result 열이 실행 결과만** 말한다(`Success`/오류). 트랜잭션 결과(커밋·롤백)는 열이 없다.
2. 트랜잭션 결과는 **색으로만**, 그것도 savepoint 객체의 플래그를 뷰어가 다시 그릴 때만 반영된다(이벤트는 `END`가 트랜잭션 객체에 대해서만 오고, 문장 행은 `UPDATE`를 받지 않는다 → 창을 다시 열거나 새 이벤트가 와야 색이 바뀐다).
3. 툴바 롤백(다른 실행 컨텍스트) · 자동 커밋 전환 · 접속 종료(서버 측 롤백)는 문장 행에 아무 흔적도 남기지 않는다.

---

## 2. 우리 설계

### 2-1. 부품 둘(30 §2 등재)

| 부품 | 크레이트 | 역할 |
|---|---|---|
| **`TxLog`**(수집기 + 링 + 필터) | `nsql-run::txlog`(UI 무의존 · CLI도 씀) | `RunEvent`와 호스트의 트랜잭션 판정을 받아 `TxEntry`를 만들고 `TxRecord`에 묶는다. 링 상한 `txlog.max_entries`(기본 10,000 · 39 §3 부하원). 필터 = `TxFilter{ text, purposes: {User, Meta, Util}, only_current_tx, only_this_editor }`. 조회 = `entries(filter) -> impl Iterator<&TxEntry>`(최신 우선 · 복사 0). |
| **`txlog_win.rs`**(창) | `nexa-sql` | 로그 창 골격(두 번째 창 · 표 · 검색 상자 · 스위치 · 우클릭 메뉴 · 파일 내보내기)을 그대로 쓴다(WindowHost T-65 전까지는 복사 최소). 제목 `Nexa SQL — Transaction log [프로필 : 탭]`. 모덜리스 · 크기/위치 기억(설정 HIDDEN). |

### 2-2. 모델

```rust
/// 문장 실행 하나(DBeaver QMMStatementExecuteInfo에 해당).
pub struct TxEntry {
    pub id: u64,
    pub at: LocalTime,                  // 시작 시각
    pub editor: u64,                    // 어느 편집기 탭에서(0 = CLI/탐색기)
    pub purpose: Purpose,               // User · Meta(탐색기/카탈로그) · Util(COUNT·페이지 재질의·PRINT)
    pub text: String,                   // 원문(표는 한 줄 · ¶ 표시 · 툴팁 전문)
    pub timeline: Timeline,             // 26 §2 단계 그대로(Execute·Fetch·OutputFlush·Navigate) → Duration 열 = 합
    pub rows: Option<u64>,              // 페치 행 또는 영향 행
    pub result: ExecOutcome,            // Ok · Err{code, message} · Stopped{stage, rows}   ← 중지 시점 = 그 단계
    pub tx: Option<u64>,                // 속한 TxRecord(자동 커밋이면 None · 그 문장이 곧 커밋)
}

/// 트랜잭션(수동 커밋 구간 · DBeaver QMMTransactionSavepointInfo에 해당 · savepoint는 1단만).
pub struct TxRecord {
    pub id: u64,
    pub started: LocalTime,
    pub ended: Option<LocalTime>,
    pub outcome: TxOutcome,             // Pending · Committed · RolledBack · ImplicitCommit{by: "CREATE"} · LostOnDisconnect
    pub updates: usize,                 // 영향 행 > 0인 문장 수(툴바 배지와 같은 수 · 34 DR-30)
}
```

- **트랜잭션 결과를 문장 행에 표시** = `TxEntry.tx → TxRecord.outcome`을 표시 시점에 읽는다(문장 행을 갱신할 필요 없음 · DBeaver 1-3 ②의 문제 해소). 상태 열 문구: `대기` · `커밋됨 14:02` · **`롤백됨 14:05`** · `암묵 커밋(CREATE)` · `자동 커밋` · `끊김(롤백)`. Result 열은 실행 결과(`성공`/`[코드] 메시지`/`중지 · Fetch 1.2s · 3,200행`)로 두고 **Tx 열을 따로** 둔다 — 두 사실을 한 칸에 섞지 않는다.
- **색**(설정 키 · 테마 토큰): 대기 = accent 12% · 롤백됨 = 취소선 + 회색 60% · 오류 = danger 15% · 커밋됨/자동 = 없음 · 중지 = warning 12%. 색은 보조이고 문자가 주(색약 대응).

### 2-3. 수집 규칙(어디서 무엇을 기록하나)

| 사건 | 기록 |
|---|---|
| `RunEvent::Begin` | `TxEntry` 생성(text · purpose User · editor) |
| `ResultSet`/`Done`/`Timing` | rows · timeline 채움 · result Ok |
| `Error` | result Err · **오류 문장은 Tx에 넣지 않는다**(DBeaver `QMUtils` 규칙과 동일 · 배지 수에도 미포함) |
| 중지(T-108 ■) | result Stopped{stage, rows}(부분 행 = 행 단위 · 미완 행 폐기) |
| 수동 모드 DML(영향 행>0) | 열린 `TxRecord`가 없으면 열고(`started` = 그 문장 시각) entry.tx = 그 id · `updates += 1` — 지금 `tx_on_done`이 `tx_pending`에 넣는 자리 |
| COMMIT/ROLLBACK 문장 · 메뉴 · 툴바 · 상태줄 팝업 · 탭 닫기/종료 팝업 | `TxRecord.outcome = Committed/RolledBack` · `ended` — 지금 `tx_clear`가 부르는 자리(이유를 인자로) |
| DDL 등 암묵 커밋(`implicit_commit`) | `ImplicitCommit{by}` |
| 자동 커밋 모드의 문장 | `tx = None` · Tx 열 `자동 커밋` |
| 접속 해제 · 세션 소실(패닉) · 자동 재접속 | 열린 TxRecord = `LostOnDisconnect`(서버가 롤백) |
| 탐색기·카탈로그 질의(별도 메타 세션) | purpose Meta(옵션 · `txlog.meta` 기본 off — DBeaver의 "skip metadata queries"와 같은 기본) |
| 추가 페치·COUNT·전체 조회 | purpose Util(같은 문장의 후속 · 원 entry에 Navigate 단계로 덧붙이거나 별 행 · 기본 off) |

### 2-4. 창(UI)

- **열**: 시간 · 종류(`SQL / 사용자`·`메타`·`유틸`) · 문장(한 줄 · ¶ · 툴팁 전문 · 더블클릭 = 편집기에 열기) · 소요(`0.016s` · 툴팁 = 단계별 26 §2) · 행 · 결과 · **Tx**(위 문구).
- **상단 검색 상자**: 문장 부분 일치(대소문자 무시 · 정규식 옵션은 찾기 위젯 부품).
- **스위치**: `모든 질의 표시`(User 외 Meta/Util) · `이전 트랜잭션 표시`(끄면 열린 TxRecord 것만 + 자동 커밋 최근 것) · `이 탭만`(편집기 탭 필터 · DBeaver 컨텍스트 필터에 해당).
- **우클릭**: 문장 복사 · 편집기에 열기 · 다시 실행 · 로그 내보내기(CSV · 로그 창 파일 싱크 재사용).
- **툴바 배지**(T-108과 공유): Commit/Rollback 버튼 오른쪽 위 작은 캡슐에 `updates`(열린 TxRecord) — 배지 수의 원천은 `TxLog`(지금의 `tx_pending.len()`을 대체) · 오래되면(`tx.stale_min`) 빨강.
- **열기**: View ▸ 트랜잭션 로그 · 상태줄 트랜잭션 팝업 "로그…" · 단축키(키맵 등록).

### 2-5. CLI

`nsql shell`: `\txlog [n]` = 최근 n건 표(같은 `TxLog` · 같은 열) · `--txlog <file>`로 CSV 싱크. `nsql run`은 기록만(표시 없음).

### 2-6. 저장

기본 = 세션 메모리 링(앱 종료 시 사라짐 · DBeaver 기본과 같음). 선택 = `txlog.file`(JSONL/CSV 싱크 · 로그 파일 싱크와 같은 회전 규칙 `log.file_max_kb`). DB 저장(DBeaver QMDB)은 하지 않는다 — 포터블 D-78과도 맞고 필요하면 파일 싱크로 충분.

### 2-7. 설정 키(레지스트리에만 · 30 §1)

`txlog.max_entries`(10000 · PERF) · `txlog.meta`(off) · `txlog.util`(off) · `txlog.file`(빈) · `txlog.color_pending/rolled_back/error/stopped` · `txlog.win_bounds`(HIDDEN) · `txlog.show_all`/`show_previous`/`this_tab`(HIDDEN 자동 기억).

---

## 3. 구현 순서(T-107 · 예상 규모 중)

1. `nsql-run::txlog`(모델 · 수집 API `on_event(&RunEvent, ctx)` · `open_tx/close_tx(outcome)` · 링 · 필터 · 테스트: 커밋/롤백/암묵/끊김 4경로 · 오류 미포함 · 링 회전) — 반나절.
2. 호스트 배선: `tx_pending` → `TxLog` 원천 교체(배지·상태줄·팝업 목록이 같은 표를 읽음) · 중지(T-108) 결과 연결.
3. `txlog_win.rs`: 로그 창 골격 복사 최소화(열 표 · 검색 · 스위치 3 · 우클릭 · 내보내기) · i18n · 설정 키 · View 메뉴/단축키.
4. CLI `\txlog` · 문서(34 §2 갱신 · 39 §3 부하원 등재 · 30 §2 부품).
5. 실기: 수동 모드에서 UPDATE 2 → 롤백 → 창에 `롤백됨 hh:mm` + 취소선 · 새 UPDATE → `대기` · DDL → `암묵 커밋(CREATE)` · 접속 해제 → `끊김(롤백)`.

**선행/연관**: T-108(■ 실행 중지 · 부분 행) · T-54(탭별 세션이 되면 TxRecord가 탭별로) · T-65(WindowHost로 창 골격 공유).

---

## 4. DBMS 공용성 검토(사용자 09-17 "전체 DBMS에 공용 적용이 가능한지 · 미지원 DBMS 보완")

트랜잭션 계열 요구는 셋이다: **(A) 로그의 트랜잭션 결과 표시**(T-107) · **(B) 수동 커밋 대기 수 배지·잃는 순간 확인**(DR-30 · 있음) · **(C) 실행 중지 + 중지 시점까지의 행**(T-108). 원칙: **(A)(B)는 서버가 아니라 클라이언트가 아는 사실**(우리가 보낸 문장·우리가 보낸 COMMIT/ROLLBACK)로만 만들므로 방언과 무관하게 공용이다. 방언 차이는 ① 암묵 커밋 규칙 ② 자동 커밋의 뜻 ③ 취소 API 세 곳에만 있다.

| 방언 | 암묵 커밋(DDL 등) | 자동 커밋 전환 | 서버 롤백 사유(우리가 못 보는 것) | 취소 API(C) | 판정 |
|---|---|---|---|---|---|
| **Oracle** | DDL 전부 · `CONNECT`/종료 시 커밋 | 세션 옵션(드라이버 `autocommit`) | 데드락 희생(ORA-00060은 문장만 롤백) · 세션 강제 종료(ORA-00028) · 접속 끊김 | `OCIBreak` = oracle crate `Connection::break_execution(&self)` — Arc 공유 필요 · 부분 행 = 배열 페치 배치 단위 | (A)(B) ✅ · (C) ✅ |
| **PostgreSQL** | 없음(DDL도 트랜잭션) · 단 `VACUUM`·`CREATE DATABASE` 등은 블록 밖 | 프로토콜 기본 자동 · 수동 = `BEGIN` | **오류 뒤 블록은 aborted** — 이후 문장 전부 실패(`25P02`) · 롤백만 가능 · 데드락(`40P01`) · 직렬화 실패(`40001`)면 **트랜잭션 전체 롤백** | `CancelToken::cancel_query` · 커서 FETCH 배치 단위 | (A)(B) ✅ **+ aborted 상태 표시 필요** · (C) ✅ |
| **SQLite** | 없음 | 기본 자동 · 수동 = `BEGIN` | `SQLITE_BUSY`/`SQLITE_INTERRUPT` 뒤 문장 롤백(트랜잭션은 유지) · 스키마 변경 충돌 | `sqlite3_interrupt` = rusqlite `InterruptHandle`(Send) · 부분 행 = 행 루프 단위 | (A)(B) ✅ · (C) ✅ |
| **SQL Server** | 없음(DDL 트랜잭션 가능) · 단 `ALTER DATABASE` 등 일부 | 기본 자동(`SET IMPLICIT_TRANSACTIONS`로 수동) | **XACT_ABORT/심각 오류 = 배치·트랜잭션 자동 롤백**(오류 뒤 `@@TRANCOUNT` 0) · 데드락 희생(1205) = 전체 롤백 | tiberius(async) **취소 API 없음** — TDS Attention 미노출 | (A)(B) ✅ **+ 자동 롤백 감지 필요** · (C) ❌ |
| MySQL(예정) | **DDL 전부 암묵 커밋** · `LOCK TABLES` 등 | `autocommit` 변수 | 데드락(1213) = 전체 롤백 · 잠금 대기 초과(1205) = 문장만 | `KILL QUERY <id>`(별도 세션) | (A)(B) ✅ · (C) 보완 |
| ODBC(예정) | 드라이버 의존 | `SQL_ATTR_AUTOCOMMIT` | 알 수 없음 | `SQLCancel` | 보수적으로 처리 |

### 4-1. 공용으로 두는 것

- **Tx 열의 6상태**(대기·커밋됨·롤백됨·암묵 커밋·자동 커밋·끊김)와 배지 수는 방언 무관. 암묵 커밋 판정은 이미 방언표(`implicit_commit(dialect, stmt)`)로 있고 MySQL·ODBC 행만 추가한다.
- 수집 지점(`RunEvent` · `tx_on_done` · `tx_clear`)은 러너 공용이라 드라이버를 몰라도 된다.

### 4-2. 미지원·부분 지원 보완

| 결손 | 보완 |
|---|---|
| **서버가 스스로 롤백했는데 우리가 모른다**(PG aborted · MSSQL XACT_ABORT/데드락 · Oracle 세션 종료) | 오류 코드 표(방언별 · `nsql-core::Dialect::rollback_class(code)`: `WholeTx` / `StatementOnly` / `Unknown`)로 판정 → `WholeTx`면 열린 `TxRecord`를 **`RolledBackByServer{code}`** 로 닫고 배지 0 · Tx 열 "서버 롤백(1205)" · PG는 추가로 **`aborted` 상태**를 세션에 기억해 다음 문장 실행 전 "롤백이 필요합니다" 안내(DBeaver도 이 경우 "transaction aborted" 배너). 코드 표는 [42 오류 정규화](42-db-error-normalization.md)의 같은 표에 열 하나 추가. |
| **자동 커밋의 뜻이 다름**(Oracle 세션 옵션 vs PG/SQLite/MSSQL 프로토콜) | 이미 드라이버 `set_option("autocommit")` 포트로 추상화 · Tx 열 "자동 커밋"은 러너의 `autocommit` 플래그로 판정(서버 상태를 묻지 않음). MSSQL `IMPLICIT_TRANSACTIONS`처럼 사용자가 SQL로 바꾼 경우는 문장 감지(`SET IMPLICIT_TRANSACTIONS ON/OFF` · `SET AUTOCOMMIT`)로 플래그를 따라간다(현재 `SET AUTOCOMMIT` 스크립트 경로와 같은 방식). |
| **취소 API 없음**(MSSQL · ODBC 일부) | 3단 폴백: ① 세션 옵션이 있으면 **문장 타임아웃**(`db.statement_timeout` · MSSQL `SET LOCK_TIMEOUT`/tiberius 없음 → 워커 측 타이머) ② **페치 단계 취소**는 공용(배치 사이 깃발 · 이미 있음) — 실행(첫 응답) 단계만 취소 불가 ③ 그래도 안 되면 **세션 포기**: 워커가 그 실행 스레드를 버리고(`catch_unwind` 격리와 같은 구조) 새 세션으로 재접속 · 이전 세션은 서버가 정리(MSSQL은 접속 끊김 = 롤백) · UI는 "중지 = 재접속(미커밋 롤백)"을 **확인 팝업**으로 알린다(잃는 순간 규칙 DR-30). ■ 버튼 툴팁이 방언별 지원 수준을 말한다("중지 가능" / "페치만 중지" / "재접속으로 중지"). |
| **부분 행이 행 단위가 아닐 위험** | 없음: 모든 드라이버가 행 단위 `Vec<Value>`로만 결과를 만들고(DR-33), 중단은 행 루프/배치 경계에서만 일어난다. 드라이버가 셀 디코딩 중 오류를 내면 그 행은 버린다(규칙 명시). |
| **MySQL DDL 암묵 커밋·`KILL QUERY`** | `implicit_commit` 표 확장 · 취소 = 보조 세션에서 `KILL QUERY <connection_id>`(접속 시 `CONNECTION_ID()` 1회 저장 · 부하 표 26 §8 등재) |

### 4-3. 결론

(A)(B)는 지금 설계로 **전 방언 공용**이며, 보완은 "서버 롤백 감지 코드 표" 하나로 끝난다. (C)는 드라이버 취소 포트가 있는 3종(Oracle·PG·SQLite)에서 완전하고, MSSQL/ODBC는 **페치 취소 + 재접속 폴백 + 툴팁 안내**로 같은 사용자 경험을 낸다. T-108 구현 순서: 포트 → SQLite → PG → Oracle → MSSQL 폴백 → 서버 롤백 코드 표(T-107과 공유).

## 6. 실행 취소 — OS × DBMS 동작·영향도(사용자 09-17 "윈도우/맥/리눅스에서 DBMS별로 정상 동작할지 검토")

| DBMS | 방식 | Windows | macOS | Linux | 취소 뒤 세션 | 취소 뒤 트랜잭션 | 주의 |
|---|---|---|---|---|---|---|---|
| **SQLite** | `sqlite3_interrupt`(rusqlite `InterruptHandle` · Send+Sync) | ✅ | ✅ | ✅ | 유지 | 유지(문장만 `SQLITE_INTERRUPT`) | 실행 중 문장이 없을 때 부르면 no-op · 접속 스레드 하나에 묶인 핸들이라 다른 세션에 영향 없음 · `:memory:` 공유 세션(D-47)도 같음 |
| **PostgreSQL** | `CancelToken::cancel_query`(백엔드 PID+비밀키로 **새 TCP 접속** · CancelRequest) | ✅ | ✅ | ✅ | 유지 | **aborted**(오류 `57014` 뒤 블록은 `ROLLBACK` 필요 · §4 PG 행) | 동기 접속이라 서버가 안 닿으면 OS 접속 타임아웃(Windows ≈ 21s)만큼 막힘 → **별도 스레드에서 호출**(워커 `cancel_run` · 09-17) · TLS 접속(T-70)이 오면 취소도 같은 커넥터 사용 · 방화벽이 새 접속을 막으면 취소 불가(문장은 계속) |
| **Oracle** | `OCIBreak`(oracle crate `break_execution` · `Arc<Connection>` + `Weak` 핸들) | ✅(사용자 확인 09-17) | ✅ Instant Client | ✅ Instant Client | 유지 | 유지(문장만 `ORA-01013`) | ODPI-C threaded 모드(oracle crate 기본) 필요 · **OOB(TCP urgent) 차단 네트워크**에서는 break가 서버의 다음 읽기 때까지 지연 → 12c+는 in-band break 자동 · 필요하면 `sqlnet.ora DISABLE_OOB=ON` · `set_autocommit`은 취소 진행 중 찰나에만 재시도(`Arc::get_mut`) |
| **SQL Server** `mssql.encrypt=login` | TDS **Attention**(소켓 복제본에 8바이트 · 로그인 뒤 평문) | ✅ | ✅ | ✅ | 유지(`DONE_ATTN` 뒤 부분 결과로 정상 종료) | 유지(`XACT_ABORT ON`이면 롤백) | 서버 `Force Encryption=Yes`면 `Off` 로그인이 거부됨 → `required`로 · 데이터가 평문으로 흐름(LAN·개발용) · 복제본은 논블로킹 플래그를 공유(Unix O_NONBLOCK은 file description 공유 · Windows FIONBIO도 소켓 공유) → WouldBlock 재시도 처리 |
| **SQL Server** `mssql.encrypt=required` | **소켓 종료**(`shutdown(Both)` 복제본) | ✅ `WSADuplicateSocket` 복제 | ✅ `dup` | ✅ `dup` | **끊김** → 다음 실행 때 자동 재접속(`connect.auto_reconnect` · 워커 `suspect`) | **서버가 롤백**(세션 종료) · 호스트 `tx_close(Lost)` | 서버는 배치를 즉시 중단(접속 끊김 감지) · 세션 옵션(`SET` 등)은 재접속 뒤 초기화 · 탐색기 메타 세션은 별개라 영향 없음 |
| SQL Server 대안 | `KILL <spid>`(별도 접속) | — | — | — | 유지 | 롤백 | `ALTER ANY CONNECTION` 권한 필요(일반 사용자 불가) → 옵션으로만 검토(T-108 후속) |
| 공통 | future drop / 타임아웃(tiberius 예제류) | — | — | — | **정리 안 됨**(읽다 만 토큰 스트림 · 재사용 불가) | 서버는 다음 전송 시도까지 계속 실행 | 우리 동기 `block_on` 구조에도 맞지 않음 · 채택하지 않음 |

- **공통 흐름**: 툴바/카드 ■ → `Handle::cancel_run`(별도 스레드에 위임 · 즉시 반환) → 드라이버가 실행을 끊음 → 오류(interrupted/57014/ORA-01013/IO)는 `run_cancel_requested`로 "중지됨" 판정 · Attention은 오류 없이 부분 결과로 끝나므로 `worker.done`에서 판정 · 세션을 끊는 방식은 트랜잭션 Lost + 재접속 안내.
- **영향 없음**: 탐색기 메타 세션(별도 접속) · 다른 탭의 결과 · 로그/트랜잭션 로그(오류 행으로 기록) · 취소 스레드는 1회성(부하원 등재 불필요 · 상한 = 실행 1회당 1).
- **실기 체크**(3-OS): SQLite 긴 쿼리(`WITH RECURSIVE` 1e8) ■ → 즉시 · PG `pg_sleep(60)` ■ → 1초 안 `57014` · Oracle `dbms_lock.sleep`/대량 조인 ■ → `ORA-01013` · MSSQL `WAITFOR DELAY '00:01:00'` ■ → login 모드 즉시 중지·세션 유지 / required 모드 접속 끊김 뒤 다음 실행 자동 재접속.

### 6-1. SQL Server — 취소 방식 설정 · 전 구간 암호화 조합 · 접속 전 탐침(사용자 09-17)

- 설정 2개: `mssql.encrypt`(`required` 기본 · `login`) · `mssql.cancel`(`attention` 기본 · `socket`). 두 방식 모두 구현 — `socket`은 항상 소켓 종료 · `attention`은 **성립할 때만** Attention이고 아니면 소켓 종료로 대체(로그 창에 안내).
- **Attention이 성립하는 조건** = 로그인 뒤 TDS 패킷이 **평문**일 것. 성립 = 클라이언트 `Off`(로그인만) **그리고** 서버 정책 `ENCRYPT_OFF`/`NOT_SUP`. 깨지는 조합 = ① 우리 설정 `required`(클라이언트가 전 구간 요청) ② 서버 `Force Encryption=Yes`(`ENCRYPT_REQ` 응답 · tiberius `negotiated_encryption`이 `Off`를 조용히 `On`으로 올림). **이유**: 전 구간 암호화면 로그인 뒤 모든 바이트가 tiberius 안의 rustls 세션(키·시퀀스·MAC)을 통과한다. 우리가 소켓 복제본에 쓰는 8바이트는 TLS 레코드가 아니라 평문이라 서버 TLS 계층에서 복호화·MAC 검증에 실패 → 서버가 접속을 끊는다(= 의도치 않은 소켓 종료). 평문 구간이라야 Attention 패킷이 그대로 TDS 패킷으로 읽힌다.
- **접속 전 판단(구현)**: `probe_server_encryption` — 별도 TCP로 PRELOGIN(ENCRYPTION=OFF)을 보내 서버의 ENCRYPTION 바이트만 읽고 닫는다(1 RTT · 로그인 없음 · 타임아웃 3s). `OFF`/`NOT_SUP` → 이 접속은 `login`(Attention 가능) · `REQ`/`ON`/실패 → **이 접속만 `required`로 임시 적용**(설정은 그대로) · 결과는 접속 설명에 `encrypt=login(Attention)` / `encrypt=required(server forces · Attention off)`로 붙어 상태줄·로그에 보인다. 접속 직후 판단은 tiberius가 협상 결과를 노출하지 않아 불가 → 탐침이 답.
- 미채택: future drop/타임아웃(클라이언트 대기만 끊고 읽다 만 토큰 스트림이 남아 재사용 불가 · 서버는 다음 전송까지 계속) · `KILL spid`(`ALTER ANY CONNECTION` 권한 필요 · 옵션 후보).
