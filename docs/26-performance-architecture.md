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
2. 예산(§5)의 "유휴 상주"는 **창 프레임버퍼 제외 힙 ≤ 4MB**로 잡는다(현재 ≈1.5MB).
3. 워킹셋 트림(최소화 시 `SetProcessWorkingSetSize(-1,-1)`)은 **하지 않는다** — 숫자만 작아지고 복귀 시 페이지 폴트로 느려진다(Golden식 착시).
4. 창을 닫으면 프레임버퍼를 즉시 놓는다(로그인·로그 창 ✅) · 숨김 창은 만들지 않는다.
5. 진짜 위험은 결과셋이다 — `Vec<Vec<Value>>`(행당 Vec 헤더 24B + 셀당 `Value` 32B + 문자열 힙)라 100만 셀이면 ≈60MB. **T-50 컬럼 지향 저장소 + T-48 상한/스트리밍**이 이 문서의 본론이다(§4).
6. 글리프 캐시·로그 링·편집기 버퍼는 상한이 있다(링 10k건 → 실제 1,024 사전 할당 뒤 순환).

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
| 접속(Connect) | DB 로그인 1 | 클릭당 1 · 같은 서버면 세션 유지 · 위 큐 공유 | **없음**(자동 재접속 금지) | `handle_panel_action(Connect)` · `Runner::connect` |

**검토 체크리스트**(네트워크를 만드는 코드를 추가·변경할 때):
1. 사용자 행동 없이 시작되는 요청인가? → 주기·백오프 상한과 "창이 열려 있을 때만" 조건이 있는가.
2. 실패가 곧바로 재시도를 부르는가? → 지수 백오프 · 프로필당 진행 중 1개 · 상한 유지.
3. 동시성 상한이 있는가? → 스레드 수 · 큐잉(FIFO) · 슬롯 초과 시 깨우기 간격(≥1s).
4. 대상이 사용자가 의도한 서버인가? → 접속한 적 없는 서버에 지속 트래픽 금지(대상 집합).
5. 다중 인스턴스에서 배가되는가? → 인스턴스 수만큼 늘어나는 것을 수용할 수 있는 상한인가.
6. DNS·ICMP 같은 부수 트래픽이 있는가? → 프로브마다 재풀이(수백 프로필이면 캐시 · T-63 잔여).

**한계(알고 두는 것)**: 호스트명은 프로브마다 DNS를 다시 푼다(수십 개까지 무시 가능) · 다중 인스턴스는 각자 확인한다(설계상).
