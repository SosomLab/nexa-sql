# 42 · DBMS 오류 정규화 — 공통 분류 + 개별 코드 부각 (사용자 요청 09-16)

> **요구**(사용자 09-16): ① 객체가 없어 나는 오류를 우측 하단 **토스트**(유형 + 대상 이름 · 설정 시간(기본 3초) 동안 쌓임 · 반투명) ② 각 DBMS 오류를 분석해 **출력 메시지를 최대한 공통화하되 개별 특징도 포용**하는 구조 ③ `ORA-00942` 같은 **코드는 의미가 있으니 부각**해 로그·토스트에.
> **구조**: `nsql_core::dberr`(의존 0 · 순수 함수) — 원본(코드·메시지)은 그대로 두고 그 위에 **공통 분류** `ErrorClass` + **대상 이름** + **원본 코드 표기**를 얹는다. 표시는 호출자(GUI 상태줄·결과 메시지·로그 창·토스트 · CLI 오류 줄)가 i18n 라벨로.

## 1. 모델

```
Classified { class: ErrorClass, code: Option<String>, object: Option<String> }   // nsql_core::classify(dialect, code, message, stmt)
ErrorClass = NoTable · NoColumn · NoObject · Syntax · Permission · Login · Connection · Unique · ForeignKey · NotNull · Check ·
             Lock · Deadlock · DataType · Resource · Unknown
```

- **공통(전 방언)**: 분류 16종 · 라벨 i18n(`Msg::ErrCls*`) · 표시 형식 `[코드 · 분류: 대상] 원문`.
- **개별(방언)**: `code` = 사용자가 아는 표기 그대로 — Oracle `ORA-00942`/`PLS-00201`/`TNS-12541`(메시지 첫머리에서 · 없으면 숫자로 `ORA-%05d`) · SQL Server `Msg 208` · PostgreSQL `SQLSTATE 42P01`(메시지에 있을 때) · MySQL `#1146` · SQLite `SQLITE 1`. 원문 메시지는 항상 뒤에 그대로.
- **대상 이름**: 메시지의 첫 따옴표 이름(`'x'` · `"x"` · `[x]`) → 없으면(Oracle ORA-00942는 이름을 안 준다) 실패한 **실행문에서 추정**(`FROM/INTO/UPDATE t`).
- **분류 규칙**: 코드 표(Oracle · SQL Server · MySQL) + 메시지 패턴(PostgreSQL · SQLite — 숫자 코드가 없거나 의미가 약함). 표는 `dberr.rs` 한 곳 · 테스트로 고정.

| 분류 | Oracle | SQL Server | PostgreSQL(메시지) | MySQL | SQLite(메시지) |
|---|---|---|---|---|---|
| NoTable | 942 | 208 | relation … does not exist | 1146 | no such table: |
| NoColumn | 904 | 207 | column … does not exist | 1054 | no such column: |
| NoObject | 4043 · 6550+PLS-00201 · 2289 | 2812 · 4121 | … does not exist | 1305 · 1049 | no such … |
| Syntax | 900 · 933 · 923 · 907 · 936 · 6550 | 102 · 105 · 156 | syntax error | 1064 | syntax error |
| Permission / Login | 1031 / 1017 · 1005 | 229 · 230 / 18456 | permission denied / password authentication failed | 1142 · 1044 / 1045 | readonly · access denied / — |
| Connection | 3113 · 3114 · 12541 · 12154 · 1012 · 28 | -2 · 53 · 233 · 10054 | connection refused/closed/reset | 2002 · 2003 · 2006 · 2013 | unable to open database |
| Unique / FK / NotNull / Check | 1 / 2291 · 2292 / 1400 / 2290 | 2627 · 2601 / 547 / 515 / 547 | duplicate key / foreign key / null value in column / check constraint | 1062 / 1451 · 1452 / 1048 / 3819 | UNIQUE / FOREIGN KEY / NOT NULL / CHECK constraint failed |
| Lock / Deadlock | 54 · 30006 / 60 | 1222 / 1205 | could not obtain lock / deadlock detected | 1205 / 1213 | database is locked / — |
| DataType | 1722 · 1858 · 1861 · 1476 · 12899 | 245 · 8114 · 241 · 8152 | invalid input syntax · out of range · value too long | 1366 · 1292 · 1406 | datatype mismatch |
| Resource | 1652 · 1653 · 1654 · 4031 | 1105 · 9002 · 701 | no space left · out of memory | 1021 · 1114 | disk full · out of memory |

## 2. 표시

| 곳 | 형식 |
|---|---|
| GUI 상태줄 · 결과 메시지 탭 · 로그 창 | `ERROR line 2: [ORA-00942 · 테이블/뷰 없음: M4S_X] ORA-00942: 테이블 또는 뷰가 존재하지 않습니다` |
| GUI 토스트(우측 하단 · 쌓임 · `ui.toast_secs` 3초 · `ui.toast_alpha` 85% · 클릭 = 닫기 · 최대 5장) | 제목 `ORA-00942 · 테이블/뷰 없음` · 본문 = 대상 이름(없으면 원문 첫 줄). **분류된 오류만**(Unknown은 상태줄만) |
| CLI | `ERROR line 2: [ORA-00942 · Table or view not found: M4S_X] 원문` · 분류 안 되면 종전 그대로 |

## 3. 원칙

1. **원문은 절대 가리지 않는다** — 코드·원문이 그대로 뒤에 온다(방언 특징 보존). 공통 분류는 앞머리 요약.
2. **분류 실패 = 침묵** — `Unknown`이면 머리말 없이 원문만 · 토스트 없음(오분류보다 낫다).
3. 표는 **한 곳**(`dberr.rs`) · 새 방언/코드는 표에 한 줄 + 테스트.
4. 토스트는 팝업 층(맨 위) · 반투명 · 클릭으로 닫힘 · 마우스 라우팅 규칙(카드 위 클릭은 아래로 안 흘림).

## 4. 후속

- 오류 → **편집기 줄 하이라이트/이동**(line 번호는 이미 있음) · 토스트 클릭 = 그 줄로.
- 분류별 **도움말 링크**(Oracle `docs.oracle.com/error-help/db/ora-00942` 처럼 메시지에 있는 URL 그대로 노출).
- 로그 창 필터(분류별) — 로그 형식 어댑터 확장 범위와 함께 결정.
