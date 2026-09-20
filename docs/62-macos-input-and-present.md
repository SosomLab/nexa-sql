# 62 · macOS 점검 — 한글 입력(T-139) · 화면 내보내기 비용 · 메모리 · 객체 DDL 실행 (mac 86차 · 09-20)

> 요구(사용자 09-20): ① 저장소 최신화 + 변경 핵심 분석 ② Windows에서 만든 기능이 맥에서도 같게 도는가 ③ 메모리 mac 기준 점검 ④ **한글 입력 종합 점검**(`가나다1234` · `가나다!@#$` · 전 입력란 · 자동화) ⑤ **전 객체 유형 DDL**(Oracle SNOP-DB · Demo SQLite · 생성·수정·삭제 · 내가 만든 객체만 삭제).
> 방법: 전부 자동(사용자 무조작 대기) · `NSQL_HOME` 격리(실제 설정 파일 수정 시각 불변 확인) · 키 입력은 **사용자가 명시적으로 허락**해 주입(보낼 때마다 전경 PID 확인 · 클립보드 미사용) · 화면은 창 단위 `screencapture -l`.

## 1. 한글 입력 (T-139) — 원인 · 수정 · 검증

### 1-1. 점검 결과(수정 전)

| 입력란 | `가나다1234` / `가나다!@#$` 결과 | 판정 |
|---|---|---|
| 편집기 · 설정 검색 · 로그인 폼 · 트랜잭션 로그 검색 · 파일 대화상자 | `ㄱㅏ나다234` / `가나다@#$` | ❌ 첫 음절이 자모로 풀림 + 한글 뒤 첫 1바이트 글자 유실 |
| 찾기 막대 · 파일 검색 | `가나다234` | ❌ 첫 1바이트 글자 유실 |
| 명령 팔레트 | `234` — 한글은 뒤의 **편집기로 샘** | ❌ IME 사건을 포커스 편집기로 보냄 |
| 탐색기 타입어헤드 | `가나다1234` | ✅(IME 끔 + 앱 조합 — 71차) |

### 1-2. 원인(`NSQL_TRACE_IME=1` 트레이스로 확정 — nexa-beep docs/34 H-14·H-26과 같은 뿌리)

1. **첫 키 유출**: 입력란이 IME를 막 붙인 직후의 첫 키는 `KeyboardInput{text:"ㄱ"}`(조합 없는 자모)로 오고 **그 뒤에** `Ime::Enabled`가 온다 → `ㄱ` + `ㅏ…` = 풀린 음절.
2. **첫 1바이트 글자 삼킴**: `Commit("다")` → `Preedit("")` 직후의 `1`은 keydown이 **앱에 도달하지 않는다**(Released만 옴 · winit NSView `interpretKeyEvents` 경계). 이벤트가 없으니 앱 쪽 판정으로는 못 고친다.
3. 팔레트는 글자(`Char`)는 받지만 `Ime` 사건은 `focused_textbox()`(편집기)로 갔다.

### 1-3. 수정 — "한글 입력 소스일 때는 IME를 거치지 않는다"

| 층 | 무엇 |
|---|---|
| nexa-ui `nexa-ctl` | `TextBox` **앱 조합기**: 전역 스위치 `set_hangul_app_compose`(기본 끔 → Windows/Linux 불변 · 상자마다 배선 없음 · nexa-dlg 상자도 같이) · 자모 = 조합(`hangul::Composer` — 타입어헤드와 같은 부품) · 조합 중 글자 = 기존 preedit 표시 · Backspace = 자모 단위 · 자모 아닌 글자·이동·Enter·클릭·붙여넣기·되돌리기·포커스 이탈 = **먼저 확정** · 비밀번호(마스크) 칸은 조합하지 않음 |
| nexa-ui `nexa-sys` | `input_source::is_korean`(Carbon TIS) · `watch`/`take_changed`(분산 알림 `kTISNotifySelectedKeyboardInputSourceChanged`) — 의존 0 · 수동 FFI · 다른 OS = `None` |
| nexa-sql | 설정 **`input.hangul_compose`** = `auto`(기본) / `system` / `app` · 순수 판정 `input::hangul_app_mode`(MC/DC) · `sync_hangul_mode`(기동 · 창 활성화 · 입력 소스 바뀜 · 설정 변경) → nexa-ctl 스위치 + 모든 창 `set_ime_allowed` · 진단 `NSQL_TRACE_IME=1` |

`auto` = **macOS이고 지금 입력 소스가 한글일 때만** 앱 조합. 일본어·중국어 입력기는 시스템 IME가 필요하므로 그대로 둔다(입력 소스가 바뀌면 알림으로 즉시 전환). 잃는 것: 앱 조합 중에는 한자 변환(⌥Return) 없음 → `system`으로 되돌릴 수 있다(그러면 위 결함 둘이 돌아온다).

### 1-4. 검증(같은 자동 배터리 · 수정 뒤)

편집기(**저장한 파일 바이트로 대조**) · 찾기 · 팔레트 · 파일 검색 · 설정 검색 · 트랜잭션 로그 검색 · 파일 대화상자 · 로그인 폼 = 전부 `가나다1234 가나다!@#$` · `가나다 가나다`(공백) · `닭`(겹받침) · `읽어`(도깨비불) · IME 사건 0건(트레이스) · 단위 테스트 4(nexa-ctl 3 + nexa-sql 판정 1).
자동화 메모: 한글 입력 소스에서 System Events `keystroke "s" using command down`은 라틴 글자를 만들려고 **⌥를 같이 누른다**(트레이스 `ALT|SUPER`) → 단축키는 `key code`로 보낸다 · zsh는 `$변수`를 단어로 나누지 않는다 → 시험 스크립트는 bash로.

## 2. 화면 내보내기(present) — 맥에서 커서가 느리게 느껴지는 가장 큰 원인

`NSQL_TRACE_FRAMES=1`(Release · 메인 창 1375×977pt = **2750×1954px** · P3 디스플레이):

| | 프레임 평균 | present | 유휴 CPU |
|---|---|---|---|
| 지금(softbuffer 0.4.8 · `CGColorSpace::new_device_rgb`) | **45.8 ms** | **37.2 ms** | 9.3%(깜빡임 2 fps × 45 ms) |
| 실험: CGImage를 **디스플레이 색 공간**으로 표시 | 19.6 ms | 12.0 ms | 3.8% |
| (참고) Windows 1375×945 · 배율 1.0 | 2~3 ms | — | 0~1% |

표본(`sample`)에 `vImage` 색 변환(`vLookupTable_Planar8toPlanar16` · `vMatrixMultiply_Planar16S` · `vConvert_Planar16Q12toRGB888`)이 잡힌다 = CoreAnimation이 **프레임마다 21 MB를 CPU로 색 변환**한다. 키 하나 = 45 ms ≈ 22 fps가 상한이라, 55 §1~8에서 줄인 편집기 비용(1.6 ms)과 무관하게 맥에서는 느리다.
결정이 필요해 실험 패치는 되돌렸다(**D-133** · T-147): ① softbuffer 포크/패치로 디스플레이 색 공간(가장 작음 · sRGB 값을 P3로 그대로 보내 색이 약간 진해짐) ② 자체 내보내기(IOSurface 배경 레이어 + 색 공간 태그 = GPU 색 맞춤 · 복사 0 · 색 정확 — T-136 D6과 같은 일) ③ 그대로.

### 2-1. 구현(09-21 · 87차 · T-147 · D-133 ②) — 자체 IOSurface 내보내기(선택 사항)

- nexa-ui **nexa-sys `layer_present::LayerPresenter`**(의존 0 · 수동 FFI): 표면 풀(≤ 3 · 합성기가 쥔 장은 `IOSurfaceIsInUse`로 피함) → 프레임마다 할당 0 · **sRGB 색 공간 태그**(색 맞춤 = 합성기) · 레이어 배치는 softbuffer와 같다.
- nexa-sql **`present.rs` `Presenter`/`Buffer`**(모양 = `softbuffer::Surface`) — 창 11곳(메인 · 로그인 · 파일 · 설정 · 색 · 단축키 · 로그 · 트랜잭션 로그 · 세션 · 툴바 플로팅)이 이 타입 하나로 낸다 · 설정 **`gfx.mac_present`** = `softbuffer`(기본) / `iosurface` · 순수 판정 `use_layer`(MC/DC) · 만들기 실패 = 조용히 softbuffer · 새로 여는 창부터(메인 = 재시작 뒤) · `NSQL_TRACE_FRAMES`가 뒷단 이름을 찍는다.
- **함정 둘(실측 · Intel i9-9980HK + AMD)**: ① 'BGRA' 표면의 알파 0 = **투명**(창이 하얗다) → present 때 0xFF 채움 ② 행 바이트를 `폭 × 4`(11000)로 강제하면 표면은 만들어지지만 **합성기가 그리지 않는다** → OS 기본 정렬(11008)에 맡기고, 다르면 중간 버퍼에서 행 단위로(같으면 표면에 직접).

| Release · 2750×1890px · 같은 장면 | 프레임 평균 | present | 유휴 CPU(30초) | 풋프린트 |
|---|---|---|---|---|
| softbuffer(기본) | 51.2 ms | 36.2 ms | 3.1초 | 74 MB |
| **iosurface** | **16.0 ms** | **2.9 ms** | **0.9초** | 94 MB(표면 3 + 중간 버퍼 21 MB) |

메인 · 로그인 · 로그 창 캡처로 표시 확인. 기본값을 바꾸지 않은 까닭: 이 기기 한 대에서만 확인했고(Apple Silicon 미확인) 실패 모양이 "빈 창"이라 치명적이다 → **D-133 = 기본 전환 여부**로 남긴다. 남은 것: `nexa-gfx::Surface` 행 간격(stride) → 중간 버퍼·복사 제거 · 유휴 때 풀 줄이기 · 크기 조절 중 실기.

## 3. 메모리 (Release · 격리 폴더 · SQLite 접속 · `footprint`/`ps` 3회 표본)

| # | 상태 | footprint | RSS | 피크 footprint | Windows Private(26 §7-3) |
|---|---|---:|---:|---:|---:|
| 1 | 기동(메인 + 로그인 창) | 74 MB | 182 | 131 | 12.0 |
| 2 | 접속됨 · 로그인 창 닫힘 | **56 MB** | 133 | 116 | 10.7 |
| 3 | + 로그·트랜잭션 로그·세션·설정 창 | 103 MB | 235 | 178 | 16.4 |
| 5 | + 3.9 MB · 4만 줄 스크립트 | 75 MB | 161 | 136 | (최적화 뒤 ≈ 20) |
| 6 | + 결과 200행 | 56 MB | 135 | 116 | 10.9 |
| 7 | + 결과 10만 행 × 5열 | 82 MB(+26) | 162 | 123 | 41.2(+30.5) |
| 8 | + 69 MB · 70만 줄 파일 | 148 MB(+92) | 234 | **286** | 87(피크 106) |
| — | 8에서 탭 닫은 4초 뒤 | **56 MB** | 145 | — | 회수 ✅(`malloc_zone_pressure_relief` 실기 확인) |

`vmmap` 내역(기동): **MALLOC_LARGE 37 MB + CoreAnimation 28 MB** = 레티나(2×) 프레임버퍼(메인 21.5 MB · 로그인 6.3 MB)가 "앱 버퍼 + CALayer 사본 + 해제 직후 남은 버퍼"로 2~3벌 · 실제 힙(TINY+SMALL+MEDIUM) ≈ 10 MB = Windows Private과 같은 수준. 즉 **차이는 거의 전부 프레임버퍼**(픽셀 4배 × 2~3벌)다. 결과셋 비용은 같다(셀당 ≈ 52 B). 큰 파일 **피크 286 MB**(Windows 106)는 적재 진행 막을 그리는 동안 프레임마다 21 MB 버퍼가 새로 잡혔다 늦게 회수되는 몫으로 본다(D6 버퍼 보존이 같이 해결). 남은 틈: `NSQL_TRACE_MEM`의 `private 0 -> 0`(맥 측정값 미구현).

## 4. 맥 동작 검토(Windows 67~85차 기능)

| 항목 | 맥 결과 |
|---|---|
| 두 저장소 테스트 · clippy | ✅ nexa-ui 352 · nexa-sql 330 · clippy 0 |
| 큰 파일(69 MB) 열기 · 줄 복제 · 되돌리기 — Debug의 `debug_assert`(CoreText 소수 전진폭 ↔ 고정폭 지름길) | ✅ 조용함 · `[load] fill 0.1 ms` |
| 되돌리기 기록 파일(저장 → 재시작 → 되돌리기 → 저장) | ✅ 원문 복원(`undo/*.nsqu`) |
| 메모리 회수(`memtrim` macOS 경로) | ✅ 148 → 56 MB |
| DDL 뒤 탐색기 갱신 · 읽기 트랜잭션 종료 | 단위/전 경로 테스트 ✅(`sqlite_end_to_end_ddl_refresh`) · 시작 인자 접속의 탐색기 루트(65차 수정) ✅ |
| 탐색기 한/영 토글 | Windows 전용(`cfg!(windows)`) — 맥은 자모가 직접 와서 불필요 ✅ |
| 남은 실기(마우스·드래그가 필요) | 툴바/컬럼 드래그 고스트 · 타입어헤드 HUD 위치 드롭다운 · 확장 패널 버튼 |

## 5. 객체 유형 DDL 실행 — Oracle(SNOP-DB) · SQLite(Demo 사본)

안전 규칙: 접두 `NSQLT_` · 시작 전 같은 접두 객체 0 확인 · **전후 `USER_OBJECTS` 스냅숏 대조** · 삭제는 이름을 하나씩 명시(와일드카드·동적 삭제 없음) · 테이블 `PURGE` · 휴지통 건수 불변 확인 · Demo는 **사본**에서만.

**Oracle 19c**(CLI `nsql run` + GUI 편집기 `run.all` 둘 다): TABLE(+PK·CHECK·COMMENT) · INDEX(일반·함수 기반·REBUILD·RENAME) · SEQUENCE(ALTER) · VIEW(OR REPLACE·COMPILE) · MATERIALIZED VIEW(REFRESH) · SYNONYM · GTT(RENAME·TRUNCATE) · TYPE + BODY · FUNCTION · PROCEDURE · PACKAGE + BODY(REF CURSOR) · TRIGGER(DISABLE/ENABLE/COMPILE) · 익명 블록 · `EXEC` OUT 바인드 · `PRINT` 커서 · DBMS_OUTPUT · MERGE — **생성 16 · 수정 20여 문장 · 삭제 24 전부 성공**. 일부러 낸 컴파일 오류는 `3/5 PLS-00201`로 보고(INVALID → 고친 뒤 VALID). 결과: 남은 `NSQLT_` 0 · 휴지통 25 그대로 · **스냅숏 1,649개 완전 일치**.

**SQLite 3.46**: TABLE(AUTOINCREMENT · 생성 컬럼 · FK · WITHOUT ROWID · STRICT) · INDEX(UNIQUE·표현식·부분) · VIEW · TRIGGER(AFTER · INSTEAD OF) · ALTER(ADD/RENAME COLUMN · RENAME TO · DROP COLUMN) · REINDEX · UPSERT + RETURNING · 재귀 CTE · 윈도 함수 · JSON · FTS5 · SAVEPOINT · PRAGMA · EXPLAIN QUERY PLAN · STRICT 위반 오류 보고.

### 5-1. 🔧 발견·수정 — 분할기가 방언을 몰랐다(데이터 안전 문제)

`CREATE TRIGGER … BEGIN …; END;` 뒤의 **17개 문장이 오류도 없이 실행되지 않았다**: 분할기는 `CREATE TRIGGER`·`BEGIN`을 늘 PL/SQL 블록(단독 `/`로 끝남)으로 봐서 뒤 문장들을 한 덩어리로 묶었고, SQLite 드라이버는 그 덩어리의 **첫 문장만** 실행했다. SQLite·PostgreSQL의 `BEGIN;`(트랜잭션 시작)도 같은 함정.
수정 = nsql-script `split_script_in(src, Option<Dialect>)`/`statement_at_in`: Oracle·ODBC·SQL Server·방언 없음 = 종전 그대로(`/`·`GO`) · **어느 방언이든 `BEGIN;`·`BEGIN TRANSACTION|TRAN|WORK|DEFERRED|IMMEDIATE|EXCLUSIVE…` = 트랜잭션 문장** · SQLite·MySQL·PostgreSQL = 본문에 `BEGIN`이 있으면 짝이 맞는 `END` 뒤의 `;`(`CASE … END` 중첩 계산) · 없으면 첫 `;`. 러너(`self.dialect()`) · GUI(`split_items`·캐럿 문장 = 세션 방언 — **러너와 index가 같아야 한다**) · CLI `plan -d`에 배선 · 테스트 5. 수정 뒤 SQLite 전 항목 통과 + GUI 편집기 경로에서 `END;` 뒤 문장 실행 확인 · Oracle 회귀 없음.

### 5-2. 그 밖의 발견(기록 · TODO)

- `SHOW ERRORS` = "unsupported"(우리 예제 `examples/oracle-refcursor-pkg.sql`도 쓴다) → T-148.
- `nsql run` 종료 코드: 도움말은 "실패한 항목 수"였지만 구현은 0/1/2 → **문구를 구현에 맞춤**(도움말 · 40).
- `sqlite:////abs`(슬래시 4개)는 앞 `//`를 되풀이 벗겨 상대 경로가 된다 · 접속 전 SQLite 오류가 기본 방언 표기 `[ORA-00014]`로 보인다 → T-148 · **✅ 09-21**(`sqlite_path` = 한 번만 벗김 · CLI `printer.dialect` = 대상 방언 · GUI `sessions::error_dialect` MC/DC → `[SQLITE 14 …]`).
- 시작 인자로 접속하면 탐색기 루트 이름이 프로필 이름이 아니라 `host:port`.

## 6. 결정 대기

| # | 결정 | 권장 |
|---|---|---|
| **D-133** | 맥 화면 내보내기(§2) — ① softbuffer 디스플레이 색 공간 패치 ② 자체 IOSurface 내보내기(T-136 D6과 함께) ③ 그대로 → **② 구현됨(09-21 · §2-1 · 설정 `gfx.mac_present`)** · 남은 질문 = **기본값을 `iosurface`로 바꿀 것인가** | 며칠 써 보고 이상 없으면 전환(present 36 → 2.9 ms) · Apple Silicon 확인 뒤 |
| **D-134** | `input.hangul_compose` 기본값 — auto(권장 · 구현됨) / system | auto |
