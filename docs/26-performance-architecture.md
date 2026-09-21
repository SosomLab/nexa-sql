# 26 · 성능 아키텍처 — 단계별 계측 · 경량 렌더/메모리 원칙 · 경쟁 제품 분석

> **요청**(사용자 09-14): *"처음부터 기준을 잘 세워야 하는 부분은 성능. 쿼리 작성 → 송신 → 실행 → 페치(부분/전체/메타만) → 수신 → 탑재 → 표시 → 탐색 → 중간 산출물 동적 클렌징 — 사용자 입장에서 각 영역별 수행 시간을 식별하고 어디가 느린지 확인할 수 있게 단계별 구조를 잡고 로깅·모니터링이 가능하도록 처음부터 설계. 프로시저 실행 시 로그 Flush도 성능 지표에. 특히 데이터를 화면 컨트롤에 렌더링하는 위치의 실행·메모리 최적화로 항상 가벼운 구조. 사용자 평이 좋은 앱들의 성능 관련 처리 방식·기술 구조를 조사·분석. DBeaver는 호환성·편의성은 좋지만 성능이 너무 안 좋고 특히 메모리 사용이 과다."*
> **상태**: 골격 코드 ✅ 09-14(`nsql_core::{Stage, Span, Timeline}` · `ExecResult.timing` · `RunEvent::Timing` · Oracle 드라이버 3단계 분리 · CLI `--timing` · GUI 상태줄/그리드 푸터) · 나머지는 §6 작업.

---

## 0. 원칙 다섯 줄

1. **단계는 사용자 관점으로 자른다.** 코드 구조가 아니라 "사용자가 기다리는 구간"이 단계다 — 9단계 + 서버 로그 플러시 + 커밋([`Stage`](../crates/nsql-core/src/lib.rs)).
2. **각 층은 자기 단계만 덧붙인다.** 드라이버는 Execute/Fetch/OutputFlush, 러너는 Commit, 호스트는 Load/Render/Navigate. 남의 단계를 고치거나 추정하지 않는다(모르면 비운다 · 러너가 "execute+fetch" 합산으로 정직하게 표기).
3. **계측은 언제나 켜져 있다.** `Instant` 몇 개의 비용은 마이크로초 — 끄는 옵션 대신 **표시**를 고른다(CLI `--timing` · GUI 상태줄 · 로그 레벨).
4. **메모리는 자릿수로 항상 보인다.** `ResultSet::approx_bytes()`가 그리드 푸터에 상시 표시 — "왜 무거운가"를 사용자가 먼저 본다.
5. **가벼움은 구조로 보장한다.** 가시 영역만 그린다 · 페인트 경로에 할당 0을 향한다 · 결과는 상한과 스트리밍으로 받는다 · 중간 산출물은 단계가 끝나면 버린다(§4).

---

## 1. 경쟁 제품 분석 — 무엇이 빠르고 무엇이 무거운가

| 제품 | 구조 | 빠른/가벼운 이유 · 느린/무거운 이유 | 우리가 취할 것 |
|---|---|---|---|
| **DBeaver**(사용자 지적) | Java/Eclipse(SWT) · JDBC | 유휴 1~2 GB · 이슈에 RSS 4.7 GB·5 GB 보고 · 큰 테이블 로드 후 Java heap space · 기본 `-Xmx1024m`을 늘리라는 안내가 공식 대처 · fetch size 200 기본이나 설정이 안 먹는 이슈 · **결과셋을 힙 객체(행마다 Object[])로 전량 보유** + JVM GC | 반면교사 — 행을 박싱 객체로 들고 있지 않는다 · 힙 상한이 아니라 **결과 상한·스트리밍**으로 다스린다 |
| **TablePlus** | 네이티브(Swift/C++) · 드라이버 직접 | 1초 내 기동 · 큰 결과셋 스크롤 부드러움(네이티브 그리드 가상화) · 메모리 소비가 쿼리·전송에만 | 우리와 같은 방향(네이티브·자체 그리드) — **기본 결과 상한 + 스크롤 시 추가 페치**를 그대로 |
| **DataGrip** | JVM · JDBC | **페이지 500행 기본**(`Limit page size`) · 다음 페이지 = `OFFSET`으로 **재질의**(메모리 대신 서버 왕복) · 서버 실행은 제한하지 않음(Athena 비용 주의) | 페이지 모델의 정직함 — 단, **커서 유지 페치**(Oracle/MSSQL은 서버 커서가 있다)를 1차로, OFFSET 재질의는 커서 없는 방언 폴백(D-43) |
| **SQL*Plus / SQLcl** | 네이티브/JVM · OCI | `ARRAYSIZE` 기본 15 → 150행에 10회 왕복 · 50~2000으로 올리면 왕복 급감(대량 추출 스윗스팟 2000~3000) · `ROWPREFETCH`가 첫 페치를 실행과 합침 | **배열 페치 크기 = 1급 설정**(우리 Oracle 기본 500 · `SET FETCHSIZE`) · 첫 페치 프리페치 |
| **Oracle DBMS_OUTPUT** | 서버 버퍼 → 클라이언트 폴링 | 대부분 클라이언트가 블록 종료 후 회수 · `GET_LINE` 줄마다 왕복 vs **`GET_LINES` 배열**(왕복 감소) · 기본 버퍼 20,000자 · SQL Developer는 폴링 패널 | 지금 구현 = `GET_LINE` 줄 단위(100k 상한) → **`GET_LINES(n)` 배열로 교체**(T-49) · 플러시 시간·줄 수를 `OutputFlush` 단계로 상시 표시 ✅ |
| VS Code · Sublime(편집기) | 자체 렌더 · 가상화 | 보이는 줄만 레이아웃·셰이핑(줄 캐시) · 스크롤 = 인덱스 이동 | 편집기 E4([17](17-editor-incremental-plan.md)) · 그리드 가상화(nexa-dir2 `rows.rs` 이식 — 사용자 09-14 *"그리드는 dir2 grid 차용"*) |

출처: [DBeaver #17962 큰 테이블 메모리](https://github.com/dbeaver/dbeaver/issues/17962) · [#38117 Memory Usage](https://github.com/dbeaver/dbeaver/issues/38117) · [#4590 600MB](https://github.com/dbeaver/dbeaver/issues/4590) · [#2275 fetch-size 무효](https://github.com/dbeaver/dbeaver/issues/2275) · [#1541 최대 메모리 제한](https://github.com/dbeaver/dbeaver/issues/1541) · [heap space 토론](https://github.com/orgs/dbeaver/discussions/12498) · [TablePlus vs DBeaver(Mako)](https://mako.ai/guides/tableplus-vs-dbeaver) · [QueryGlow 비교](https://queryglow.com/blog/tableplus-vs-dbeaver) · [DataGrip Rows/페이지](https://www.jetbrains.com/help/datagrip/rows.html) · [DataGrip 페이지 재질의](https://intellij-support.jetbrains.com/hc/en-us/community/posts/5036270349458-Does-Datagrip-query-again-when-I-change-the-page-in-a-result-tab-Does-it-always-happen) · [SQL*Plus ARRAYSIZE](https://www.oreilly.com/library/view/oracle-sql-plus-the/0596007469/re42.html) · [fetch size 튜닝](https://www.igorkromin.net/index.php/2014/05/21/tuning-the-sqlplus-fetch-size-for-better-performance/) · [ROWPREFETCH](https://connor-mcdonald.com/2020/04/24/sql-plus-the-sweet-spot/) · [DBMS_OUTPUT 문서](https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_OUTPUT.html) · [DBMS_OUTPUT super fast](https://www.thatjeffsmith.com/archive/2016/04/getting-your-dbms_output-super-fast/)

**한 줄 결론**: 무거운 제품의 공통점은 "결과를 전부 힙에 객체로 올리고, 그리기까지 그 객체를 다시 만든다"이다. 가벼운 제품의 공통점은 "받는 양을 정하고(상한·배열), 보이는 것만 그리고(가상화), 네이티브 구조로 든다"이다.

---

## 2. 단계 모델 — 누가 재고 어디에 쌓이나

```
사용자 관점         Compose → Send → Execute → Fetch → Receive → Load → Render → Navigate → Cleanup   (+ OutputFlush · Commit)
재는 층             GUI      러너    드라이버   드라이버  드라이버   호스트  호스트    호스트      호스트          드라이버      러너
쌓이는 곳           ─────────────── ExecResult.timing(드라이버) → Runner가 Commit 덧붙여 RunEvent::Timing ──────▶ 호스트가 Load/Render/Navigate 덧붙임
```

| 단계 | 정의(시작 → 끝) | 측정 층 · 현재 상태 | 부가 지표 |
|---|---|---|---|
| Compose | 마지막 키 입력 → 실행 요청 | GUI · ☐(편집기 E4 뒤) | 문장 길이 |
| Send | 문장 분리·바인드 준비 → 드라이버 호출 직전 | 러너 · ☐(T-47) | 바인드 수 |
| **Execute** | 드라이버 호출 → 첫 응답(행/완료) | 드라이버 · ✅ Oracle(`stmt.query`/`execute`) · 기타 = 러너 합산 "execute+fetch" | — |
| **Fetch** | 첫 응답 → 마지막 행 수신(배열 페치 왕복 포함) | 드라이버 · ✅ Oracle(`array N` note) · ☐ MSSQL 스트림 분리(T-47) | 행 수 · 바이트 |
| Receive | 와이어 → `Value` 디코딩 | 드라이버 · ☐(Fetch에 포함 — 분리는 프로파일링 뒤) | — |
| **Load** | `ResultSet` 수신 → 그리드 모델 준비 | 호스트 · ✅ 그리드 푸터 | 바이트(`approx_bytes`) |
| **Render** | 프레임 그리기(첫 프레임·이후 프레임) | 호스트 · ✅ 그리드 푸터(매 프레임) | 행/셀 수 |
| Navigate | 스크롤·추가 페치·페이지 전환 | 호스트 · ☐(T-48 페치 모델과 함께) | 추가 행 |
| Cleanup | 수신 버퍼·캐시 해제 | 호스트 · ☐ | 회수 바이트 |
| **OutputFlush** | `DBMS_OUTPUT` 회수(프로시저 로그) | 드라이버 · ✅ Oracle(줄 수 · `GET_LINE per line`) | 줄 수 |
| **Commit** | autocommit 커밋 | 러너 · ✅ | — |

표시: CLI `nsql run --timing` → `⏱ execute 12.3ms · fetch 340.1ms (500 rows) 1.2 MB · output 3.2ms (12 lines) · commit 0.4ms · total 356ms` · GUI 상태줄 같은 문자열 · 그리드 푸터 `1–40 / 500 · load 0.8ms · render 6.2ms · ~1.2 MB`.

**메타만 추출**(사용자 요구 "쿼리 플랜·건수만"): `EXPLAIN`/`SET AUTOTRACE`류는 실행 없이 Execute만 · `SELECT COUNT(*)` 래핑은 별도 Action — 둘 다 같은 Timeline을 쓴다(T-48).

---

## 3. 로깅·모니터링

| 층 | 무엇 | 어디에 |
|---|---|---|
| 상시 | 항목마다 `Timeline`(구조체 · 문자열 아님) | `RunEvent::Timing` — 호스트가 원하는 대로 표시/저장 |
| CLI | `--timing` 요약 줄 · `--timing=json`(후속) | stderr(결과와 분리 · 파이프 안전) |
| GUI | 상태줄 요약 · 그리드 푸터 · **실행 히스토리 패널**(항목별 Timeline 표 · 정렬 = 느린 단계) | T-48 |
| 로그 파일 | `NSQL_LOG=timing` 이면 항목마다 한 줄(TSV: ts · 프로필 · 문장 요약 · 단계별 µs · 행 · 바이트) | `<설정 폴더>/logs/timing-YYYYMMDD.tsv` · 회전 · T-47 |
| 예산 경고 | 단계가 예산(§5)을 넘으면 상태줄 색 + 힌트("fetch 3.1s — FETCHSIZE 500 → 2000?") | T-48 |
| 프로시저 로그 | `OutputFlush` 줄 수·시간 · 버퍼 초과(ORA-20000) 감지 → "SET SERVEROUTPUT ON SIZE UNLIMITED" 힌트 | ✅ 단계 · 힌트 ☐ |

계측 원칙: 문자열 포맷은 **표시 시점**에만(경로 중엔 `Duration`·정수만) · `Timeline`은 항목당 1개 · 스팬 수 ≤ 16(고정 상한) — 계측이 성능을 깎지 않는다.

---

## 4. 경량 구조 — 렌더·메모리(사용자 09-14 *"항상 가벼운 구조"*)

### 4-1. 결과 수신 — 받는 양을 정한다
- **기본 상한 1,000행**(TablePlus·DataGrip 500 선례 · D-42) + 그리드 끝에서 "더 가져오기"(커서 유지 · Navigate 단계에 기록). 상한 없이 전량은 export 경로만.
- **배열 페치 크기 = 설정**(`SET FETCHSIZE` · Oracle 기본 500 · MSSQL은 TDS 스트림) · Fetch note에 항상 표시.
- 스트리밍: 드라이버는 배치 콜백(예: 500행마다)으로 넘기고 호스트는 첫 배치로 그리기 시작(T-48 — 지금은 전량 후 1회).

### 4-2. 결과 탑재 — 박싱을 피한다
- 지금 `Vec<Vec<Value>>`(셀마다 enum 32B + String 힙). 10만 행 × 10열 = 셀 100만 = 32 MB + 문자열. **컬럼 지향 저장소**(`ColumnStore`: 타입별 연속 배열 + 문자열은 한 아레나 + 오프셋)로 바꾸면 힙 조각 0 · 캐시 친화 · `approx_bytes` 정확(T-50 · nexa-grid U-3와 함께).
- `Value::approx_bytes`/`ResultSet::approx_bytes`가 상시 푸터 — 사용자가 "무겁다"를 수치로 본다 ✅.

### 4-3. 렌더 — 보이는 것만, 할당 없이
- 가시 행만 그린다(지금 그리드도 `skip(top)`+`break`) · **nexa-dir2 `rows.rs` 가상화**(`VirtualRows<S>`·`RowSource` · 3,212 LOC · 의존 0)로 교체 — 사용자 지시 *"그리드는 dir2 grid 차용"* · 저장된 접속 목록도 같은 그리드(T-31).
- 페인트 경로 할당 0 목표: `cell_text(v) -> String`(매 셀 할당)을 `write_cell(v, &mut buf)` 재사용 버퍼로 · 컬럼 폭은 첫 200행 샘플 1회(지금 ✅) → 스크롤 시 재계산 없음.
- 텍스트 폭 캐시: 같은 문자열 반복(코드값)은 폭 캐시(LRU 4k) · 고정폭 폰트는 `len × advance`로 즉시.
- 프레임 예산 8 ms(120 Hz 여유) — `render`가 넘으면 푸터가 주황.

### 4-4. 탐색 — 인덱스만 움직인다
- 스크롤 = `top` 인덱스 · 셀 재생성 없음 · 추가 페치는 배경 스레드 → 배치 도착 시 푸터 갱신.
- 정렬/필터는 **인덱스 벡터**(행 복제 0) · 컬럼 리사이즈는 폭 배열만.

### 4-5. 클렌징 — 단계가 끝나면 버린다
- 수신 버퍼(드라이버 배열)는 `ResultSet`로 옮기는 즉시 해제 · 새 실행 시 이전 결과는 **탭 정책**(기본: 같은 탭 교체 → 즉시 drop) · 폭 캐시는 결과 교체 시 clear · 히스토리는 요약(Timeline·행 수)만 보관, 데이터는 보관하지 않는다.
- 유휴 시 큰 결과(> 예산) 경고 배지 · 사용자가 "비우기"로 회수 · `Cleanup` 스팬에 회수 바이트.

### 4-6. 프로시저 로그(DBMS_OUTPUT)
- `GET_LINES(lines, numlines)` 배열 회수(왕복 1/100) · 실행 중 **주기 폴링 옵션**(SQL Developer식 — 긴 프로시저의 진행 로그를 보며 기다림) · 버퍼 `UNLIMITED` 기본 안내 · 시간·줄 수 상시 표시 ✅.

---

## 5. 예산(측정 기준 — 어기면 경고 · 릴리스 게이트 후보)

| 지표 | 목표 | 근거 |
|---|---|---|
| 유휴 RSS | ≤ 40 MB(창 1 · 접속 1) | nexa-dir2 B1 30 MB 선례 · DBeaver 600 MB~ 반면교사 |
| 1,000행 × 20열 Load | ≤ 5 ms | 컬럼 저장소 전 실측 후 조정 |
| Render(프레임) | ≤ 8 ms | 120 Hz 여유 |
| 10만 행 메모리 | ≤ 행당 100 B + 문자열 실측 | 컬럼 저장소 목표 |
| Fetch 왕복 | 1,000행 ≤ 2 왕복 | 배열 500 기본 |
| OutputFlush | 1,000줄 ≤ 10 왕복 | `GET_LINES` 100줄 배열 |
| 기동 | ≤ 300 ms(폰트 로드 포함) | TablePlus "1초 내" 이상 |

---

## 6. 결정 · 작업

| # | 결정 | 권장 |
|---|---|---|
| ~~D-42~~ | → ✅ **D-69** 200행 + 탭 로컬(사용자 09-16 · [43](43-fetch-model-and-result-tabs.md)) | 200 |
| ~~D-43~~ | → ✅ **D-70** 서버 커서 유지 + OFFSET 폴백([43 §3](43-fetch-model-and-result-tabs.md)) | 커서 유지 |
| **D-44** | 타이밍 로그 파일 기본 — 끔(권장 · `NSQL_LOG=timing`으로 켬) / 켬 | 끔 |

| ID | 항목 | 의존 |
|---|---|---|
| **T-47** | Send 스팬(러너) · MSSQL/SQLite Execute·Fetch 분리 · `--timing=json` · 로그 파일(TSV · 회전) | ✅ 골격 |
| **T-48** | 페치 모델 — 상한·배치 스트리밍·"더 가져오기"(Navigate) · 예산 경고 · 실행 히스토리 패널 · 메타만(EXPLAIN/COUNT) | D-42 D-43 |
| **T-49** | `DBMS_OUTPUT.GET_LINES` 배열 회수 · 실행 중 주기 폴링 옵션 · UNLIMITED 안내 | — |
| **T-50** | 컬럼 지향 결과 저장소 + 페인트 할당 0 + 폭 캐시 — nexa-grid(U-3 · dir2 `rows.rs` 이식)와 함께 | nexa-ui U-3 |

## 7. 메모리 기준선 — 09-14 실측(Windows 11 · 1920×1080 · 배율 1.0)

**질문**(사용자 · 작업 관리자 캡처): "탭 15개 열린 Golden(8.5MB)보다 nexa-sql(6.3MB · 창 2개)이 상대적으로 무거운 것 아닌가?"
**답**: 아니다. 작업 관리자의 "메모리" 열은 **프라이빗 워킹셋**(지금 RAM에 올라 있는 사적 페이지)이라 앱이 워킹셋을 비우면(`SetProcessWorkingSetSize`/최소화 트림 — Delphi·Win32 앱의 흔한 관행) 실제 점유와 무관하게 작아진다. **커밋된 사적 메모리(Private Bytes)** 로 보면 Golden은 86MB, nexa-sql은 7~9MB다.

| 프로세스(상태) | 워킹셋 | **Private Bytes(커밋)** | 스레드 |
|---|---:|---:|---:|
| **nexa-sql release** · 부팅 직후(메인 1100×720 + 로그인 창 640×520) | 23.9MB | **7.3MB** | 10 |
| nexa-sql debug · 같은 상태 | 25.8MB | 9.1MB | 11 |
| nexa-sql debug · 접속 후 + 로그 창(Oracle OCI 로드) | 39.4MB | 16.5MB | 12 |
| Golden8 64bit · 탭 15개 · 접속 중 | 24.4MB | **86.2MB** | 27 |
| Sublime Text | 27.5MB | 108.4MB | 21 |
| Nexa Dir | 8.9MB | 55.7MB | 34 |
| nexa-clip / nexa-beep | 15.5 / 6.8MB | 22.2 / 9.3MB | 11 / 14 |
| DBeaver 26.1.5 | 232.6MB | 416.3MB | 92 |

### 7-1. nexa-sql 7.3MB의 내역(추정 · 구조에서 계산)

| 항목 | 크기 | 성격 |
|---|---:|---|
| 메인 창 프레임버퍼(softbuffer `CreateDIBSection` · 4B/px · 1100×720) | 3.2MB | 사적 · 창 크기에 비례(배율 1.25면 4.9MB) |
| 로그인 창 프레임버퍼(640×520 · 폼 열면 960×520 = 2.0MB) | 1.3MB | 사적 · **창을 닫으면 해제**(접속 성공 시 자동) |
| 로그 창 프레임버퍼(760×320 · 열었을 때만) | 1.0MB | 사적 · 닫으면 해제 |
| 힙(i18n 표·설정·편집기 버퍼·글리프 래스터·로그 링 ≤1,024건·채널) | ≈1.5MB | 사적 |
| 스레드 스택 커밋(메인·워커·로그 허브·프로브·OS 풀 ≈10개 × 수십 KB) | <1MB | 사적(예약 2MB/개는 커밋 아님) |
| 코드(release exe 4.6MB · debug 11.5MB)·CRT·winit/DWM | 워킹셋에만 | 공유(파일 매핑) |
| 글꼴 mmap(Segoe UI 0.96MB · **맑은 고딕 13.5MB** · Consolas 0.45MB) | 워킹셋에 닿은 페이지만 | **공유(파일 매핑) · 사적 0** — nexa-font `Mmap` |
| Oracle Instant Client(접속 시 ODPI-C dlopen) | +7~10MB | 접속 후에만 · 드라이버 몫(DR-3 예외) |

즉 사적 메모리의 **60% 이상이 창 프레임버퍼**다. GDI/DWM 앱(Golden)은 창 뒷면 버퍼가 OS 쪽(DWM 공유)에 있어 사적 메모리로 안 잡히는 반면, 우리 CPU 래스터(DR-1)는 창마다 4B/px를 사적으로 든다 — 이것이 구조적 차이이고, 창 3개 합쳐도 6MB 이내라 감수한다.

### 7-2. 결론 · 규칙
1. **비교는 release 빌드 · Private Bytes** 로 한다(작업 관리자 "메모리" 열은 트림에 따라 흔들린다). 벤치 명령: `Get-Process nexa-sql | select WorkingSet64, PrivateMemorySize64`.
2. 예산(§5)의 "유휴 상주"는 **창 프레임버퍼 제외 힙 ≤ 6MB**로 잡는다(09-14 ≈1.5MB · 09-19 ≈5.2MB — §7-3 · 늘어난 이유를 설명할 수 있어야 한다).
3. 워킹셋 트림(최소화 시 `SetProcessWorkingSetSize(-1,-1)`)은 **하지 않는다** — 숫자만 작아지고 복귀 시 페이지 폴트로 느려진다(Golden식 착시).
4. 창을 닫으면 프레임버퍼를 즉시 놓는다(로그인·로그 창 ✅) · 숨김 창은 만들지 않는다.
5. 진짜 위험은 결과셋이다 — `Vec<Vec<Value>>`(행당 Vec 헤더 24B + 셀당 `Value` 32B + 문자열 힙)라 100만 셀이면 ≈60MB. **T-50 컬럼 지향 저장소 + T-48 상한/스트리밍**이 이 문서의 본론이다(§4).
6. 글리프 캐시·로그 링·편집기 버퍼는 상한이 있다(링 10k건 → 실제 1,024 사전 할당 뒤 순환).

### 7-3. 09-19 재점검(Release · 사용자 "전체 점검 + 이전 조사와 비교")

방법: `target/release/nexa-sql.exe`를 **격리 설정 폴더**(`NSQL_HOME`)와 기동 명령(`NSQL_STARTUP_CMD` · `@connected:`)으로 시나리오별로 띄워 8~25초 뒤 3초 간격 3회 표본(입력 주입 없음 · 메인 창 1375×945 · 배율 1.0 · SQLite 접속). 스크립트 = 세션 기록의 `mem_probe.ps1`.

| # | 상태 | 워킹셋 | **Private** | 핸들 | GDI | USER | 스레드 | 유휴 CPU(6초) |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 기동 직후(메인 + 로그인 창) | 28.5 MB | **11.98 MB** | 253 | 50 | 24 | 11 | 46 ms |
| 2 | 접속됨(SQLite) + 탐색기 · 로그인 창 닫힘 | 27.7 | **10.73** | 255 | 40 | 20 | 13 | 31 ms |
| 3 | + 로그·트랜잭션 로그·세션·설정 창 4개 | 33.9 | **16.42** | 263 | 86 | 40 | 13 | 125 ms(안정 뒤) |
| 4 | + 확장 패널 | 27.7 | 10.46 | 255 | 46 | 20 | 13 | 0 |
| 5 | + 3.6 MB · 4만 줄 스크립트(미니맵 켬) | 51.6 | **34.86** | 258 | 40 | 20 | 14 | **812 ms** |
| 6 | + 결과 200행(기본 상한) | 28.8 | 10.87 | 258 | 43 | 20 | 14 | 0 |
| 7 | + 결과 **10만 행 × 5열** | 58.0 | **41.23** | 264 | 43 | 20 | 15 | 78 ms |
| 8 | 향상 모드(`perf.boost`) · 접속됨 | 27.5 | 10.60 | 255 | 40 | 20 | 13 | 0 |

**이전 조사와의 차이**

| 항목 | 09-14(§7) | 09-15([37 §6](37-file-picker-performance.md)) | 09-17([45](45-perf-boost-benchmark.md)) | **09-19** | 해석 |
|---|---:|---:|---:|---:|---|
| 기동 Private | 7.3 MB | 8.4 MB | 9.6 MB | **12.0 MB** | +4.7 MB(09-14 대비) — 아래 내역 |
| 기동 워킹셋 | 23.9 | 28.4 | 26.6 | 28.5 | 거의 그대로(코드·글꼴 = 공유 페이지) |
| 스레드(유휴) | 10 | 11 | 14 → 11 | 11(접속 뒤 13) | 접속하면 워커 + 탐색기 메타 스레드 |
| 핸들 | — | 249 | 269 | 253 | 그대로 |
| 향상 모드 Private | — | — | 8.1 | 10.6 | 같은 폭으로 이동(창 크기) |
| exe 크기 | 4.6 MB | — | — | **9.9 MB**(nsql 6.6) | 드라이버 4종 · TLS(`ring`) · 기능 증가 — 예산 30 MB 안(DR-17) |

**+4.7 MB의 내역**: ① **기본 창이 커졌다** — 메인 1100×720 → 1375×945(09-18 · 프레임버퍼 3.2 → **5.2 MB** = +2.0) · 로그인 640×520 → 748×526(+0.3) ② 그 사이 들어온 상주 구조(탭별 그리드 보관 · 트랜잭션 로그 · 세션 컨텍스트 · 확장 레지스트리 · 설정 레지스트리 450+ 키 · i18n 표 2,000+ 줄) ≈ +2 MB ③ 글리프 캐시(GDI ClearType · D-77) ≈ +0.4. 창 프레임버퍼를 뺀 **힙 ≈ 5.2 MB** — §7-2 ②의 기준(≤ 4 MB)을 1.2 MB 넘었다 → 기준을 **≤ 6 MB**로 고치고 다음 점검에서 다시 본다(한도가 아니라 "왜 늘었는지 설명할 수 있는가"가 기준).

**발견 · 조치**

1. 🔧 **세션 창이 열려 있으면 유휴 CPU 90%**(6초 중 5,438 ms) — `RedrawRequested`를 처리한 직후에도 매번 다시 그리기를 요청하는 고리(`main.rs` 세션 창 분기 · Mac 61차 도입). → 그린 뒤에는 요청하지 않는다: **5,438 → 16 ms**. 다른 보조 창(로그·트랜잭션 로그·색·단축키)은 0 ms · 설정 창은 검색 상자 캐럿 깜빡임만(78 ms).
2. ⚠️ **큰 스크립트(3.6 MB · 4만 줄)**: Private +24 MB(파일 크기의 ≈ 6.6배) · 유휴인데 **CPU 812 ms/6초** = 캐럿 깜빡임마다 편집기 그리기 **66 ms/프레임**(`NSQL_TRACE_FRAMES`: editor 65.96 ms · 나머지 합 5 ms). 원인 = `TextBox::paint_multiline`이 매 프레임 본문 전체를 다시 훑는다(줄 수 세기 · 표시 `String` 재조립 3.6 MB · 논리 줄 표 4만 건 · 캐럿 줄 세기). 메모리는 `Vec<char>`(4 B/글자 = 14.6 MB) + 저장 기준 `String` + 줄 변경 기준선 + 매 프레임 임시 할당의 잔류. → **T-136(입력 지연 2차)에 수치와 함께 등록**: 세대(`edit.rev`)로 묶은 줄 표·표시 문자열 캐시. 일반 크기(수백 줄) 스크립트에서는 0 ms라 체감 문제는 큰 파일에 한정.
3. ✅ **결과셋 = 셀당 ≈ 61 B**(10만 행 × 5열 = 50만 셀 → +30.5 MB) — §7-2 ⑤의 추정(60 B/셀)과 일치 · DR-17 예산(10만 행 < 150 MB) 안. 기본 상한 200행에서는 +0.1 MB.
4. ✅ 보조 창 4개 = +5.7 MB(창마다 프레임버퍼 1~2 MB · 닫으면 해제 — §7-2 ④ 그대로) · 확장 패널·향상 모드 = 차이 없음.
5. ✅ 새로 들어온 상주 스레드 없음 — 외부 파일 감시(`file-watch`)는 **파일을 연 뒤 첫 확인 때** 생기고 요청이 없으면 잔다(시나리오 5에서 +1).

### 7-4. 09-19 후속 — 큰 파일 최적화 · 메모리 회수([59](59-large-file-handling.md))

§7-3의 발견 ②(큰 스크립트)를 바로 고쳤다 — 상세·원인·조치는 59 §1~2.

| 항목(4만 줄 · 3.6 MB · 전 기능) | 이전 | 이후 |
|---|---:|---:|
| 유휴 그리기(캐럿 깜빡임마다 · 벤치) | 78 ms | **3.3 ms** |
| 글자 하나 입력(처리 + 그리기 · 벤치) | 209 ms | **16 ms** |
| 앱 유휴 CPU(6초) | 812 ms | **188 ms** |
| 되돌리기 히스토리 | 묶음마다 본문 전체 복사(상한 1,000) | 전체 1개 + 차이 |
| 큰 탭 닫은 뒤 Private | — | 41.5 → **10.5 MB**(기준선) |
| 10만 행 결과를 버린 뒤 | — | 41.1 → **10.6 MB** |

큰 파일을 연 동안의 Private는 34.9 → 41.5 MB로 **늘었다**(그리기 캐시 = 본문의 UTF-8 사본 + 행당 28 B). 속도와 맞바꾼 값이고, 보이지 않는 탭의 캐시는 회수 때 놓는다. 근본 해법은 버퍼 표현 교체(59 §4 2단계 · 24 MB → ≈ 4 MB).

**회수 규칙**(`memtrim.rs`): 큰 것을 놓은 1초 뒤 1회(`mem.trim_on_release`) + 유휴 주기(`mem.trim_secs` 300) — 보이지 않는 탭의 캐시 해제 + 힙 → OS(`HeapOptimizeResources` + `HeapCompact` / `malloc_trim` / `malloc_zone_pressure_relief`). 워킹셋 트림은 여전히 하지 않는다(§7-2 ③).

### 7-5. 09-21 재점검(Windows 89차 · 맥 86~88차 병합 뒤 · Release · 앞 커밋과 A/B)

| 시나리오 | 09-19~20 | 09-21 |
|---|---|---|
| 기동 | 12.0 MB | 11.68 MB |
| 접속(SQLite) | 10.7 MB | 10.09 MB |
| 창 4개 추가 | 16.4 MB | 16.22 MB |
| 2 MB 스크립트 | 27.9 MB | 27.89 MB |
| 결과 10만 행 | 41.2 MB | 41.16 MB |
| 65 MB 파일(상주/피크) | 87 / 106 MB | 86.5 / 106.5 MB |
| 유휴 CPU | 0~31 ms/초 | 0~31 ms/초(60초 타임라인 · 창 4개 포함) |
| 기동(창 보임 / 안정까지 CPU) | — | ≈155 ms / ≈500 ms |

맥에서 들어온 것(`present.rs` · 변수 표 · 입력 창 · 결과 탭 분리)은 Windows 상주·CPU에 영향이 없다. 릭 주기 4종은 평탄(10만 행 재실행의 +0.3 MB/회는 앞 커밋도 같고 탭을 닫으면 10.9 MB로 회수 = 힙 조각). 측정 규칙 추가: **빌드 직후 첫 실행은 버린다** · 회귀 판정은 같은 시각의 A/B([61 §4](61-core-design-and-working-rules.md)). 도구 = `scripts/win-leak-cycle.ps1` · `scripts/win-startup-probe.ps1`. §8 점검: T-150은 **사용자가 실행한 문장 안에서만** 추가 왕복(커서 이름 확인 1 + 커서당 FETCH·CLOSE)을 만들고 스스로 트래픽을 만들지 않는다 · T-151 서명 조회 = 루틴당 1회(캐시 · Oracle·PostgreSQL·SQL Server · 끄는 키 `vars.signature_lookup`) · T-150 끄는 키 `pg.refcursor_expand` · T-155는 문장당 왕복을 1회 **줄인다**.

### 7-6. 09-21 재점검(Windows 89차 끝 · 추가 7~23 반영 뒤 · Release)

| 시나리오 | 89차 초반 | 89차 끝 |
|---|---|---|
| 기동 | 11.68 MB | 11.14 MB |
| 접속(SQLite) | 10.09 MB | 9.71 MB |
| 창 4개 추가 | 16.22 MB | 12.42 MB |
| 2 MB 스크립트 | 27.89 MB | 27.45 MB |
| 결과 10만 행 | 41.16 MB | 40.91 MB |
| 유휴 CPU | 0~31 ms/초 | 6초에 47 ms(창 4개 포함) |
| 기동(창 보임 / 3초 CPU) | ≈155 ms / ≈500 ms | 141 ms / 500 ms |
| 문장 준비(`bench_vars`) | 2.9~3.2 µs | 1.2 µs |

탐색기 우클릭·세션 자격 금고·닫기 확인·키 문지기·오프라인 안내는 상주·CPU에 영향이 없다. 고친 것 = 바인드 열 이름 판정(`lone_select_items`)을 참조 수와 무관하게 **한 번 훑기**로(바인드 수천 개 문장에서 제곱이 되던 것). 측정 스크립트는 `NSQL_NO_ACTIVATE=1`로 띄운다.

### 7-7. 09-22 Linux 첫 점검(Ubuntu 26.04 VM · 2코어 · 3.3 GB · Wayland · Release · 91차) — 도구 `scripts/linux-perf-all.sh`

측정 단위가 다르다: Linux는 **RssAnon**(익명 상주 ≈ Private · Wayland 화면 버퍼는 `memfd` 공유 매핑이라 여기에 안 잡힌다)과 RSS를 같이 적는다. 기기(VM)가 Windows PC보다 1.5~2배 느리므로 절대 수치는 이 표 안에서만 비교한다([journal 09-22](journal/2026-09-22.md) §3).

| 시나리오 | Windows 89차 끝(Private) | **Linux 91차(RssAnon / RSS)** | 해석 |
|---|---:|---:|---|
| 기동(로그인+메인) | 11.14 MB | **4.1 / 34.3 MB** | 창 프레임버퍼가 사적으로 안 잡히는 차이(26 §7-1 ①) — 힙 ≈ 4 MB로 같은 폭 |
| 접속(SQLite) | 9.71 | 3.4 / 30.7 | |
| + 보조 창 4 | 12.42 | 4.7 / 36.8 | 창당 +1.5 MB RSS(공유 버퍼) |
| 2 MB 스크립트 | 27.45 | 15.3 / 43.0 | +12 MB(Windows +17.7) |
| 6.4 MB(L1) / 20 MB / 65 MB | 17.8 / 34.9 / 86.5 | 11.1 / 26.5 / **80.3**(RSS 38.5 / 54.4 / 108.0) | 피크 RSS 65 MB = 108(Windows 106.5) — 같다 |
| 결과 10만 행 × 5열 | 40.91 | **32.6** / 60.7 → 탭 닫으면 3.7 | 셀당 ≈ 61 B 동일 · 회수 ✅ |
| 향상 모드 | — | 3.4 / 30.7 | 유휴 CPU 0 |
| 유휴 CPU | 6초 47 ms | **0~10 ms/초**(창 4개 포함) | 틱 10 ms 해상도 |
| 기동(창 보임 / 3초 CPU) | 141 ms / 500 ms | **522 / 430 ms → 111 / 120 ms**(A/B 번갈아 · −79 %) | ★ Linux 전용 병목 둘: nexa-font 가족 탐색이 `/usr/share/fonts`를 12번 걷고 `statx` 11,786회(→ 걷기 1회 캐시 + `file_type()`) · 툴바/메뉴 아이콘 래스터가 메모 없이 호출마다 + 찾기 막대 11개 즉시 166 ms(→ `memo` + 첫 그리기 때) — 3-OS 공통 코드라 Windows·맥 기동도 그만큼 줄어야 한다(다음 세션에서 A/B) |
| 프레임 평균 | — | 5.8~7.1 ms(present 0.03~0.9) | 예산 8 ms 안 |
| `bench_vars` 문장 준비 | 1.2 µs | 1.5 µs | VM 차 |
| 릭 주기(MB/주기) | 평탄 | 파일 0.02~0.03 · 로그 창 0.000 · 10만 행 재실행 ×16 = 0.20(계단형 · 옛+새 결과 겹침 72 MB → 회수 40 MB = 한 벌 + 힙 조각 8 MB) | 누수 아님(journal §6) |

(이 표의 GUI 수치는 Wayland 백엔드 · 91차 후반에 Linux 창 백엔드 기본이 X11(XWayland)로 바뀌었다 — journal 09-22 §12 · 다음 점검에서 A/B.) exe 크기(Linux · LTO fat · strip): GUI 12.61 MiB · CLI 7.49 MiB(Windows 10.3 / 6.69 — ELF vs PE) · 드라이버 몫(CLI · 64 §3-1의 Linux 판): SQLite만 5.02 · +Oracle 0.32 · +PG 0.62 · +SQL Server 1.45 · 전부 7.36 MiB. §8 점검: 이번 세션에 네트워크를 만드는 코드 변경 없음(측정 스크립트는 사용자가 지정한 프로필로만 접속).

## 8. 네트워크 부하 원칙 — 09-14 검토 · 상시 관리 항목(사용자 요청)

> 상위 원장 = [39 §3-2 자원 거버넌스](39-resource-governance.md)(09-16) — 이 표는 NET 도메인의 근거 표로 그대로 유지한다.

앱이 스스로 만드는 네트워크 트래픽은 **아래 표의 경로뿐**이어야 하고, 각 경로는 **상한**(빈도·동시성·자동 재시도 여부)을 가진다. 새 경로를 추가하거나 상한을 바꾸면 이 표를 갱신하고 journal에 남긴다(설계 검토 항목 · T-63).

| 경로 | 1회 비용 | 빈도·동시 상한 | 자동 재시도 | 근거 코드 |
|---|---|---|---|---|
| 신호등 주기 확인 | TCP SYN 1(+ 실패 시 ICMP 1) | 접속 성공 프로필당 `probe.interval`(60s) · **창이 열려 있을 때만** · 동시 `MAX_INFLIGHT` 16 | 없음(주기) | `conn_win::tick` · `probe::ProbePolicy` |
| 실패 뒤 재확인 | 동일 | `probe.retry_delay`(60s) × 2^(n−1) · `probe.max_retries`(5)번 뒤 상한 유지(32분) | 지수 백오프 | `ProbeEntry::apply` · `failure_wait` |
| 실패 확인 즉시 재프로브 | 동일 | 프로필당 진행 중 1개(≤4초) · 대상 집합 밖 프로필은 **1회로 끝(재예약 금지)** | 없음 | `note_failure` · `drain_probes`(T-63) |
| 실행 전 빠른 판정 | SYN 1 | 사용자 실행 시 · 신호등이 초록이 아닐 때만 | 없음 | `worker::Cmd::Run.preflight` |
| 접속 테스트 | DB 로그인 1 | 클릭당 1 · 같은 프로필 잠금 · **동시 `connect.max_concurrent`(4) · 초과 FIFO 큐** | 없음 | `App::start_test` · `dispatch_attempts` |
| CLI 접속(`-c` · 스크립트 `CONNECT` · `conn test`) | DB 로그인 1 | 명령당 1 · **비밀번호 자리가 없으면(환경 변수·터미널로도 못 채우면) 로그인을 시도하지 않는다**(09-21 · 빈 비밀번호 시도가 쌓여 계정이 잠기는 것을 막는다) | 없음 | `nsql-cli opener` · `password_missing` |
| 접속(Connect) | DB 로그인 1 | 클릭당 1 · 같은 서버면 세션 유지 · 위 큐 공유 | **없음**(자동 재접속 금지) | `handle_panel_action(Connect)` · `Runner::connect` |
| 공유 연결 추가(DR-34 · [52 §2-1](52-session-modes.md)) | DB 로그인 1 | 접속 창 클릭당 1 · **같은 서버·DB·계정은 중복 접속 없음**(기존 연결 활성화) · 동시 유지 상한 `session.max_shared`(8) · 위 큐 공유 | 없음 | `App::login_place` · `sessions::login_plan` |
| 전용 세션 `CONNECT` · 개별 모드 탭 접속 | DB 로그인 1 | 사용자 실행 1회당 1 · 개별 모드 = 탭이 **처음 활성화될 때** 1(안 본 탭은 0) · 상한 `session.max_private`(8) | 없음 | `App::place_run` · `sync_sess` · `connect_quietly` |
| 유휴 닫기 뒤 재접속 | DB 로그인 1 | 닫힌 세션에서 **다음 실행/페치 1회당 1** · 주기 핑·keepalive 질의 **없음**(서버의 유휴 정책을 무력화하지 않는다) · 점검 타이머 30s는 로컬 판정만 | 없음 | `App::idle_tick` · `wake_if_idle` · `sessions::idle_action` |
| 탐색기 메타 접속(서버당 1 · 52 §2-2) | DB 로그인 1 | **처음 보는 서버에 세션이 붙을 때 1** · 같은 서버의 추가 세션 = 0 · 유휴 회수 뒤에는 다음 펼침/소스 요청 1회당 1 · 오프라인(세션 0) 서버에는 **다시 붙지 않는다** · **비밀번호 자리가 없는 스펙으로는 로그인을 시도하지 않는다**(09-21 · 빈 비밀번호 시도가 쌓여 계정이 잠기는 것을 막는다) · 입력한 비밀번호는 세션 자격 금고에서 빌린다(금고 끔 = 일회성 → 그 칸은 유휴 회수 없음) · 금고 값이 거부되면 폐기하고 한 번 다시 묻는다 → **한 번의 접속 동작 = 로그인 시도 최대 2회**(같은 틀린 값으로 되풀이하지 않는다 · 잠김/만료에는 다시 묻지 않는다) | 없음 | `App::explorer_attach` · `explorer::meta_thread`(`resume`) |
| 동작 직전 생존 판정([53](53-connection-liveness.md)) | TCP SYN 1(+ICMP 1) · `probe.timeout` 상한 | 실행·페치·건수·키·커밋 **직전**에, 마지막 성공 뒤 `probe.stale_secs`(60s) 지났거나 직전 오류·드라이버 끊김 힌트일 때만 · 접속 시도 전에는 항상 1 | 없음(실패 = 즉시 오류 · Broken) | `worker::ensure_alive` · `sessions::live_plan` |
| TCP keepalive(PG·SQL Server) | 빈 세그먼트 1(데이터 0) | 접속당 `net.keepalive_secs`(60s) 유휴마다 · 서버 유휴 세션 정책과 무관 · 0 = 끔 | OS(3회 뒤 소켓 오류) | `nsql_drivers::set_net_options` |
| 막힘 감지(L3 · [56 §9](56-manual-commit-lock-prevention.md)) | 메타 세션에 1문장 | **미커밋이 있는 세션만** `tx.block_poll_secs`(30s · 0 = 끔) · 세션 식별자를 알 때 · 세션당 진행 중 1 | 없음(질의 오류 = 그 세션에서 기능 끔) | `App::tx_block_tick` · `explorer::blockers_sql` |
| 탐색기 갱신 T1·T4([57](57-explorer-refresh-after-ddl.md)) | 메타 세션에 카탈로그 1질의/폴더 | **내가 실행한 DDL이 성공했을 때**(스크립트 = 끝난 뒤 폴더별 1회 · 트랜잭션 DDL 수동 커밋 = 커밋 때) · "객체 없음" 오류 = 폴더당 60초 1회 · 읽어 둔 폴더만 | 없음 | `App::meta_flush` · `Explorer::note_missing` |
| 탐색기 유휴 워터마크 T2 | 스키마당 1행 질의 | `meta.refresh_secs`(300s · 0 = 끔 · low/향상 모드 = 0) · 앱 유휴 + 실행 중 세션 없음 + 탐색기 보임 + 메타 세션 온라인(**유휴로 닫힌 세션은 깨우지 않음**) · 서버당 진행 중 1 | 없음 | `App::meta_refresh_tick` · `Explorer::watermark_poll` |
| 외부 파일 변경 확인([58](58-external-change-policy.md)) — **네트워크 드라이브의 파일일 때만** 트래픽 | stat 1(달라졌을 때만 읽기) | 창 활성화(열린 파일 전부 1회) · 탭 전환 · 저장 직전 · 활성 창의 보이는 탭 `file.external_poll_ms`(2s · 0 = 끔 · 향상 모드 0) · **비활성 창 = 0** · 전용 스레드 1(겹친 요청은 합침) | 없음 | `App::ext_check` · `nexa_fs::watch::StatWatch` |
| 확장 저장소 읽기(`index.json` · 설치 파일 · [50 §14](50-extension-system.md)) | HTTPS GET 파일당 1(curl · ≤ 30초) | **사용자 동작으로만**: 확장 패널을 열 때·⟳·팔레트 Install/Add Repository · 진행 중 1개(`ext_fetch_rx`) · 저장소 수만큼 순차 | **없음** | `ext_fetch_start` · `manager::Source::read_raw` |

**검토 체크리스트**(네트워크를 만드는 코드를 추가·변경할 때):
1. 사용자 행동 없이 시작되는 요청인가? → 주기·백오프 상한과 "창이 열려 있을 때만" 조건이 있는가.
2. 실패가 곧바로 재시도를 부르는가? → 지수 백오프 · 프로필당 진행 중 1개 · 상한 유지.
3. 동시성 상한이 있는가? → 스레드 수 · 큐잉(FIFO) · 슬롯 초과 시 깨우기 간격(≥1s).
4. 대상이 사용자가 의도한 서버인가? → 접속한 적 없는 서버에 지속 트래픽 금지(대상 집합).
5. 다중 인스턴스에서 배가되는가? → 인스턴스 수만큼 늘어나는 것을 수용할 수 있는 상한인가.
6. DNS·ICMP 같은 부수 트래픽이 있는가? → 프로브마다 재풀이(수백 프로필이면 캐시 · T-63 잔여).

**한계(알고 두는 것)**: 호스트명은 프로브마다 DNS를 다시 푼다(수십 개까지 무시 가능) · 다중 인스턴스는 각자 확인한다(설계상).
