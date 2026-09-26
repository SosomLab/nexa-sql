# 결과 그리드에서 데이터 편집

단일 테이블을 조회한 결과는 그리드에서 바로 고칠 수 있습니다. 고친 내용은 **적용(✓)** 을 누를 때까지 서버에 가지 않고, 적용은 **한 트랜잭션**으로 묶여 하나라도 실패하면 전부 되돌아갑니다.

## 언제 편집할 수 있나

| 결과 | 편집 |
|---|---|
| `SELECT a, b FROM t` · `SELECT * FROM t WHERE …` · `SELECT t.a FROM t t` | 가능 |
| JOIN · GROUP BY · DISTINCT · UNION · 서브쿼리 FROM · 식/별칭 열(`a+1`, `a AS x`) | 읽기 전용(상태줄에 이유) |
| 뷰 · 프로시저 결과 · 커서 · 운영(PROD) 접속 · 설정 `grid.edit` 끔 | 읽기 전용 |

행을 서버에서 **정확히 하나** 찾을 수 있어야 합니다. Nexa SQL은 다음 순서로 방법을 정합니다.

| 등급 | 조건 | 무엇으로 찾나 |
|---|---|---|
| 1급 | PK 또는 UNIQUE 열이 결과에 전부 있다 | 그 키 열(원본 값) |
| 1급-보완 | 키는 있는데 결과에서 빠졌다(`SELECT name FROM emp`) | 키 열을 **숨은 열**로 덧붙여 한 번 다시 조회(화면에는 안 보임 · `grid.edit_hidden_keys`) |
| 2급 | 키가 없다 | DBMS 행 식별자를 숨은 열로 덧붙여 한 번 다시 조회 — Oracle `ROWID` · SQLite `rowid` · PostgreSQL `ctid`+`xmin`(`grid.edit_rowid`) · SQL Server/MySQL은 없음 |
| 3급 | 키도 행 식별자도 없다 | 비교 가능한 **모든 열**의 원본 값(LOB·실수·긴 문자·xml/json·공간형 제외 · `grid.edit_all_cols`) |
| 읽기 전용 | 위 어느 것도 안 된다 | — |

숨은 열은 복사·텍스트 보기·⌘A/Ctrl+A 어디에도 나오지 않고 WHERE 절에만 쓰입니다. 다시 조회에 실패하면(예: SQLite `WITHOUT ROWID` 표) 원래 문장으로 돌아가 3급으로 판정합니다.

## 편집 방법

| 동작 | 키·마우스 |
|---|---|
| 편집 시작 | Enter · 더블클릭 · 그냥 타이핑(첫 글자로 시작) |
| 확정 | Enter(아래 셀로) · Tab / Shift+Tab(오른쪽/왼쪽 셀로) · 다른 셀 클릭 |
| 취소 | Esc |
| 셀 비우기 / NULL | Delete · Backspace(빈 값 = NULL은 `grid.edit_empty`) · 우클릭 ▸ Set NULL |
| 셀 이동(편집기 밖) | 화살표 · Tab / Shift+Tab |
| 행 추가 · 복제 · 삭제 | 툴바 `+ ⧉ −` · 우클릭 메뉴(삭제는 표시만 · 적용 때 DELETE) |
| 붙여넣기 | ⌘V/Ctrl+V — 탭·줄 구분 행렬을 선택 셀부터 채움(행이 모자라면 자동 추가 · 상한 `grid.paste_max_rows`) |
| 되돌리기 / 다시 하기 | ⌘Z/Ctrl+Z · ⌘⇧Z/Ctrl+Y(그리드 편집 전용 이력) |
| 변경 목록 · SQL 미리보기 | 우클릭 ▸ Changes / Preview SQL(보낼 문장을 리터럴로) |
| 적용 · 전부 되돌리기 | 툴바 ✓ / ✕ |

편집 중인 셀은 표시 글자와 같은 자리에 상자가 열리고 긴 글은 첫머리부터 보입니다. 상자 안에서 더블클릭은 단어, 세 번 클릭은 전체 선택입니다(`editor.dblclick` · `editor.triple_click` · `editor.dblclick_underscore`).

## 표시

- **바뀐 셀** = 글자색 주황 + 굵게(배경은 살짝만). 값을 고쳤다가 원래 값으로 되돌리면 바뀐 것으로 치지 않습니다(A → B → A = 변경 없음).
- **행 띠**: 추가 행 초록 · 바뀐 행 강조색 · 삭제 행 빨강 + 취소선 · 적용에 실패한 행 붉은 배경.
- 푸터에 `수정 n · 추가 m · 삭제 k` — 적용 시 트랜잭션 범위입니다.

## 적용과 안전 규칙

적용(✓)은 DELETE → 키 열을 바꾸는 UPDATE → 나머지 UPDATE → INSERT 순서로 **바인드 문장**을 만들어 한 트랜잭션에 보냅니다. 데이터 보호를 위해 다음은 설정으로 끌 수 없습니다.

1. **사전 검사** — UPDATE/DELETE마다 같은 WHERE로 `SELECT COUNT(*)`를 먼저 실행해 대상 행이 정확히 1이 아니면 **아무것도 쓰지 않습니다**(완전히 같은 행이 둘 있는 3급 표 등).
2. **영향 행 수 1** — 실행한 문장이 1행이 아니면 즉시 중단.
3. **되돌림** — 자동 커밋이면 전체 롤백 · 수동 커밋이면 세이브포인트로 되돌리고 필요하면 "ROLLBACK 필요"를 알립니다.
4. **키 중복 사전 검사** — 적용 대기 변경 안에서 같은 키가 둘 이상이 되면 적용 전에 거부합니다.

실패하면 어느 문장(`UPDATE #3 (ID=5)`)이 어느 단계에서 왜 실패했는지 상태줄·로그·토스트에 그대로 적습니다.

## 적용 뒤 화면 갱신(`grid.edit_refresh`)

| 값 | 동작 |
|---|---|
| `rows`(기본) | 바뀐 행만 키로 다시 읽어 **제자리에서** 교체 · 추가 행은 그 자리에 실제 행으로 · 삭제 행 제거 · 스크롤·정렬·선택 유지. 행을 식별할 수 없으면(시퀀스 키 · PostgreSQL ctid 등) 조회 전체를 다시 실행 |
| `requery` | 늘 조회 전체 다시 실행 |
| `local` | 다시 읽지 않고 편집한 값을 그대로 둠(서버 기본값·트리거 결과는 모름) |

## 동시성(`grid.edit_concurrency`)

| 값 | WHERE에 더 비교하는 것 |
|---|---|
| `key`(기본) | 키만 |
| `key_old` | 키 + 내가 고친 열의 옛 값 |
| `all_old` | 키 + 비교 가능한 모든 열의 옛 값 |

다른 세션이 먼저 바꿨으면 사전 검사가 0행을 보고 아무것도 쓰지 않습니다.

## 큰 값(LOB) — 값 보기 창

셀을 고르고 우클릭 ▸ **View value**(또는 Enter로 편집 대신 값 창)를 열면 값을 따로 봅니다.

| 값 | 창 | 할 수 있는 것 |
|---|---|---|
| 글(CLOB · TEXT · 긴 문자열) | Text(편집 가능 셀이면 그대로 편집) | **셀에 반영**(저장은 ✓ 적용) · 파일에서 넣기 · 파일로 저장 · 복사 |
| 이진(BLOB · BYTEA · varbinary) | Hex(16진수 덤프) / Image(PNG·BMP·GIF 미리보기) | 파일에서 넣기(파일 그대로 셀에 · 라벨 `<파일 · n bytes>`) · 파일로 저장(바이트 그대로) |
| JPEG/WebP | 형식은 알려 주지만 미리보기는 아직 없음 | 파일로 저장해서 봅니다 |

상한 `grid.lob_view_max_mb`(16)를 넘는 값은 풀지 않고 16진수 앞부분만 보이며, `grid.lob_image_preview`를 끄면 이진은 늘 16진수입니다. 4,000자를 넘는 글과 이진은 CLOB/BLOB 타입으로 바인드해 저장합니다(Oracle VARCHAR2 4000 · SQL Server NVARCHAR(4000) 상한 회피).

## 관련 설정

`grid.edit` · `grid.edit_empty` · `grid.edit_refresh` · `grid.edit_hidden_keys` · `grid.edit_rowid` · `grid.edit_all_cols` · `grid.edit_concurrency` · `grid.paste_max_rows` · `grid.lob_view_max_mb` · `grid.lob_image_preview` · `tx.prod_*`(운영 접속은 편집 불가).

> 기술 문서: 저장소 `docs/87-grid-data-editing.md`(설계 · 행 식별 등급 · 데이터 보호 불변식).
