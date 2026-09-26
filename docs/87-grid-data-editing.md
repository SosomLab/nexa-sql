# 87 · 그리드 데이터 편집 — 엑셀식 직접 편집 · 재사용 부품 층(nexa-ctl `gridedit`) + nexa-sql 어댑터(T-182)

> **요청**(사용자 09-26 · Mac 100차): *"그리드에서 데이터를 편집하는 기능 — 데이터 타입·길이·Not Null·Default · 날짜/시간 입력 방법 · LOB/BLOB/CLOB 등 DBMS별 대량 타입(보기) · 한 줄 복제 · 셀/줄/여러 줄 복사·붙여넣기(자동 확장) · 엑셀과 유사하게 직접 설정. 이번 개선으로 그리드의 완성도가 꽤 올라갈 것 — **다른 프로그램에서도 사용할 수 있도록 편집 가능한 그리드를 잘 개선**."*
> **선행 설계**: [77 §2-3 `ChangeSet`](77-data-workbench-architecture.md)(편집 가능 판정 · 덧그리기 · `SqlGen` · 적용 · 안전) · nexa-ui [21 §3-2 셀 안의 컨트롤 · LiveEditor](../../nexa-ui/docs/21-grid-family.md) · [41 SQL 복사 키 규칙](41-sql-copy-key-rules.md) · [43 페치 모델](43-fetch-model-and-result-tabs.md) · [52 세션 통제](52-session-modes.md) · [34 트랜잭션 UX](34-transaction-ux.md) · [56 잠금 방지](56-manual-commit-lock-prevention.md).
> **상태**: 📐 설계(09-26) → ★ **E-1~E-4 구현 · E-5 최소판(09-26 · §10)** → 남은 것 §11.

---

## 0. 한 줄 결론

**편집의 핵심은 DBMS도 그리기도 모르는 부품으로 nexa-ctl에 두고, nexa-sql 그리드는 그 위에 "값 ↔ 문자열 · 메타 ↔ 셀 명세 · 변경 집합 → SQL" 세 어댑터만 얹는다.** 부품 = `nexa_ctl::gridedit` { `CellSpec`(타입·길이·NULL·기본값 · 검증) · `ChangeSet`(셀 수정·행 추가/삭제/복제 · 되돌리기 · 표시 순서) · `paste`(클립보드 행렬 해석 · 자동 확장 적용) · `datetime`(입력 형식 여럿 → 정규형) · `LiveEditor`(편집 중인 셀 **한 곳**의 실제 `TextBox`) · `keymap`(엑셀식 키 → 동작) }. 다른 프로그램(파일 목록 · 설정 표 · 접속 목록)은 자기 그리드에 같은 부품을 꽂는다 — 21 §3-2 "그리는 것과 살아 있는 것을 나눈다"의 구현.

---

## 1. 층 구조

```
┌ nexa-ctl::gridedit (의존 = nexa-ctl 안 · DBMS·SQL·Value 모름 · 값 = Option<String>)
│  spec.rs      CellKind{Text,Number,Bool,Date,Time,DateTime,Binary,Other} · CellSpec{kind,max_len,nullable,default,read_only,name}
│               validate(input) -> Result<Option<String>(정규형), EditError>
│  changeset.rs RowRef{Existing(i),Inserted(k)} · RowStatus{Clean,Modified,Inserted,Deleted}
│               ChangeSet{edits,deleted,inserted,undo,redo} · set_cell/cell/delete_row/insert_row/duplicate_row/undo/redo/clear
│               layout(n_existing) -> &[RowRef]  (표시 순서 · 복제 행 = 원본 바로 아래 · 세대 캐시)
│  paste.rs     parse_matrix(text) -> Vec<Vec<String>>  (TSV · 따옴표 셀 · 셀 안 줄바꿈 · CRLF)
│               apply(cs, anchor, matrix, opts, specs) -> PasteReport{set,added,rejected}  (자동 확장 · 검증)
│  datetime.rs  parse(kind, s) -> Option<String>  ("2026-9-26" · "20260926" · "2026/09/26 14:05" · "T" · "now"/"today" 토큰)
│  live.rs      LiveEditor{tb: TextBox, cell, spec} · begin/paint/on_event -> LiveEvent{Commit(text, Move), Cancel, None}
│  keymap.rs    EditKey → EditAction (F2·Enter·Tab·Esc·Delete·⌘D·⌘C·⌘V·⌘Z·⌘0 · 방향키)  — OS 관례는 호스트가 수식키를 넘김
└──────────────────────────────────────────────────────────────────────────────────────
┌ nexa-sql (어댑터 · 정책)
│  grid.rs      편집 모드 · 셀 덧그리기(색·취선) · LiveEditor 배치 · 키/메뉴 → gridedit 동작 · 붙여넣기 → paste::apply
│  gridedit_adapter.rs  Value ↔ Option<String>(표시 문자열 = 41 규칙 · NULL) · Column+MetaStore → CellSpec
│  editable.rs  편집 가능 판정(출처 문장 단일 테이블 · 키 = PK/UK → 물리 ROWID → 전체 열 D-198) · 읽기 전용 이유
│  sqlgen.rs    ChangeSet → 문장 목록(DELETE→UPDATE→INSERT · 바인드 · 방언 Caps · 날짜 진짜 바인드) · 미리보기 텍스트
│  apply        gate_open → 한 트랜잭션 → 영향 행 수 검사 → 커밋/롤백 UX → 재조회/로컬 반영
│  cellview_win.rs  값 보기 창(텍스트 · 16진수 · 이미지 · 저장 · CLOB 편집) — LOB
```

의존 방향은 아래로만: `gridedit` → `nexa-ctl`(TextBox·DrawCtx·토큰). nexa-sql은 `gridedit`를 쓰되 거꾸로 알려 주지 않는다(DR-7 · 30 §1-2 포트+어댑터).

### 1-1. 지금 코드에서 이어 받을 자리(09-26 조사 · 재발명 금지)

| 있는 것 | 위치 | 어떻게 쓰나 |
|---|---|---|
| 편집 툴바 `tb_edit`(`row.add` `row.del` `row.dup` `row.save` `row.cancel` · 전부 `.disabled()` · `footer_event`의 허용 id가 빈 슬라이스) | `grid.rs` 235·358·1047 · i18n `TipRow*` | 그대로 살려 배선(활성 = 편집 가능 판정 + ChangeSet 상태) |
| 선택 모델 = **표시 좌표** `regions`/`sel_cur`/`sel_anchor` · `region_index()` = 원본 (행, 열) | `grid.rs` 147~304 · 1650 | 편집 앵커 = `sel_cur` → `row_order`/`col_order`로 원본 좌표 · 붙여넣기 앵커도 같은 길 |
| 셀 그리기 `cell_text(v, null)` · 숫자 우측 정렬 · NULL 흐림 | `grid.rs` 2805~2865 · 3278 | `ChangeSet.cell(row,col)` 덧그리기 = 값 대체 + 배경/띠/취선 · `Value::Bytes` = `<n bytes>` 유지 |
| 호스트 훅 `after_grid_event()`(복사·SQL·페치·새로고침·보기 요청 pull) | `main.rs` 6274 · 4 호출처 | `take_edit_request()` 추가 = 값 보기 창·SQL 미리보기·적용·행 재조회 |
| 결과 출처 `source_sql`·`countable`(질의 1문장) · `set_result_origin` | `grid.rs` 536·652 · `main.rs` 12882 | 편집 가능 판정의 입력 |
| 테이블 추정 `guess_table`·`split_table` · alias 분해 `alias_table` · 분류 `classify_sql` | `nsql-io/sqlgen.rs` 146·181 · `nsql-script/intel.rs` 655 · `split.rs` 416 | `editable.rs`의 단일 테이블 판정(JOIN/GROUP/DISTINCT/집합/서브쿼리 FROM 거부는 **새로** 쓴다) |
| 키 흐름 `begin_sql_copy`→`Cmd::Keys`→`ConnOutcome::Keys`→`key_cache` · `choose_key`(41 D-62~66) | `main.rs` 6908~6960 · `worker.rs` 42 · `nsql-io/sqlgen.rs` 111 | 편집 키 = 같은 캐시 · 없으면 물리 키 열 |
| 컬럼 메타 = `MetaStore` `Snapshot::columns(id)` → `ColEntry{data_type, nullable, default, key}` · 채움 = `explorer.rs` `Req::ColumnsMeta`/`meta_set_columns` · 카탈로그 `columns`/`keys`/`table_detail` | `nsql-run/meta.rs` 103·1117 · `explorer.rs` 228·2411 · `nsql-catalog/lib.rs` 1706·899·1229 | `CellSpec` 원천(길이는 `data_type` 문자열 안 `VARCHAR2(50)` → 파싱) · 없으면 urgent 요청 뒤 채움 |
| 실행 = `Session::execute(&ExecRequest{sql, params: Vec<BindParam{name,value,ty,direction}>})` · `Caps.tx_begin`/`server_autocommit`/`marker` | `nsql-core/lib.rs` 339~350·1077 · `caps.rs` 80 | `SqlGen` 출력 = `Vec<ExecRequest>` · 워커에 **새 `Cmd::Apply{key, stmts, …}`**(지금은 바인드 실행 명령이 없다 · `Cmd::Keys`가 소형 왕복의 선례) |
| 통제 `gate_open` · `Sess::blocked` · PROD `prod_confirm_needed` · 트랜잭션 정책 `tx_guard_step` | `main.rs` 5080 · `sessions.rs` 230·785·724 | 적용 전 게이트 · 확인 · 잠금 방지 |
| `Value{Null,Int,Float,Decimal,Str,Bool,Bytes,Cursor}` · `Column{name,type_name}`만 | `nsql-core/lib.rs` 266·361 | 날짜는 `Str`로 온다 → `CellKind`는 `type_name`+메타로 판정 · 바인드 타입은 `VarType`으로(새 Value 변형 없음) |
| `ResultData` 세그먼트 = `Arc<Vec<Vec<Value>>>`(변환 스레드와 공유) | `nsql-core/lib.rs` 908 | **제자리 수정 금지** → ChangeSet 덧그리기(77 결정 그대로) |
| 모달 창 틀 `sqlprev_win.rs`(새로고침·저장·편집기로·복사·닫기 · SQL 하이라이트 상자) · `input_win.rs` | 614줄 · 653줄 | SQL 미리보기 창 = 그대로 재사용(제목·본문만) · 값 보기 창 = 같은 골격 + 탭(텍스트/16진수/이미지) |
| 키 = `nexa_ctl::Key`에 F2 없음 · 본문 더블클릭 감지 없음 · `Char` 미처리 | `event.rs` 12 · `grid.rs` 2285 | F2·⌘D·⌘0 = 키맵 명령 id(`keymap.rs` 1176 `f2`)로 호스트가 그리드에 전달 · 더블클릭 = 400 ms 같은 셀(헤더 경계 선례 `grid.rs` 2543) · 타이핑 = `Char` → 편집 진입 |

없는 것(새로 만든다): `ChangeSet` · `SqlGen`(편집용) · 단일 테이블 판정 · 값 보기 창 · 설정 `grid.edit*` · 워커 바인드 실행 명령 · 그리드 `Char`/더블클릭 처리.

---

## 2. 셀 명세 `CellSpec` — 타입 · 길이 · NULL · 기본값

| 필드 | 원천(nexa-sql) | 편집기에서 하는 일 |
|---|---|---|
| `kind` | 결과 `Column` 타입 이름 → 분류(숫자·문자·날짜·시각·일시·이진·그 밖) · 모르면 `Text` | 숫자 = 숫자만(부호·소수점·지수) · 날짜류 = §4 해석 · Bool = `true/false/1/0/Y/N` · Binary = 인라인 편집 없음(값 보기 창) |
| `max_len` | `MetaStore` 컬럼 길이(문자열 · CHAR/VARCHAR n) · 없으면 None | 넘치면 커밋 거부 + 안내 "n/max" · 편집 중 상태줄에 `12/50` |
| `nullable` | MetaStore `NOT NULL` | 빈 값 커밋 = NULL이면 거부(NOT NULL) · 빈 문자열은 **`grid.edit_empty`**(설정 · 기본 = NULL · 대안 = 빈 문자열) |
| `default` | MetaStore 기본값 식 | 새 행(추가·복제)에서 값을 비우면 `DEFAULT` 키워드로 INSERT(직접 값 넣지 않음 · 서버 몫) · 표시는 흐린 `<default>` |
| `read_only` | 편집 불가 열(계산 열 · 물리 키 열 · ROWID · 편집 불가 판정) | 진입 거부 + 이유 |

검증은 부품이 하고 **정규형 문자열**을 돌려준다(숫자 = 앞뒤 공백 제거 · 날짜류 = ISO). DBMS 리터럴/바인드 변환은 `sqlgen`.

---

## 3. 변경 집합 `ChangeSet` — 세트는 그대로, 덧그린다(DR-33)

| 항목 | 설계 |
|---|---|
| 기존 행 수정 | `edits[(row, col)] = new`(원래 값은 세트에 있으므로 저장 안 함) · 원래 값과 같아지면 항목 제거(= Clean) |
| 삭제 | `deleted{row}` 토글 · 표시 = 취선 + 흐림 · 삭제 행의 셀 편집은 거부 |
| 추가 | `inserted[k] = {cells: Vec<Option<String>>, after: Option<row>}` · **복제** = 원본 셀 복사(키 열·`read_only` 열은 비움) · `after` = 원본 아래 표시 |
| 표시 순서 | `layout(n)` = 기존 행 사이에 `after` 삽입 행을 끼운 `RowRef` 배열 · 삽입이 0이면 배열 없이 직접 매핑(큰 세트 비용 0) · 세대 캐시 |
| 되돌리기 | 자체 `Op` 스택(Set·Delete·Insert·Remove · 붙여넣기 = 묶음) · ⌘Z/⌘⇧Z · 편집기 본문 되돌리기와 독립 |
| 크기 | 셀 수정 = (usize,usize)→Option<String> · 10만 행 세트에서도 바뀐 것만 |

---

## 4. 날짜 · 시간 입력(사용자 "입력 방법")

| 입력 | 해석(정규형) |
|---|---|
| `2026-09-26` · `2026/9/26` · `2026.09.26` · `20260926` | `2026-09-26` |
| 위 + ` 14:05` · ` 14:05:07` · ` 14:05:07.123` · `T14:05` | `2026-09-26 14:05:07.123`(초 없으면 `:00` · 소수 있는 만큼) |
| `14:05` · `140507` (Time 열) | `14:05:00` · `14:05:07` |
| `now` · `today` · `sysdate`(대소문자 무관) | 커밋 시 부품이 **토큰 그대로** 돌려주고 `sqlgen`이 방언 함수로(`SYSTIMESTAMP` · `GETDATE()` · `now()` · `CURRENT_TIMESTAMP`) |
| 그 밖 | 거부 + 안내(정규형 예시) |

표시(편집 진입 시 상자에 넣는 값) = 그리드 표시 문자열(41 규칙 · 이미 ISO) → 사용자가 부분만 고쳐도 된다. 서버로 갈 때는 **바인드 파라미터(진짜 날짜 타입 · T-151)** — 문자열 리터럴이 아니다(NLS/지역 설정 무관).

---

## 5. 대량 타입(LOB) — 보기 우선

| 타입 | 인라인 셀 | 값 보기 창 `cellview_win.rs` |
|---|---|---|
| CLOB/TEXT/NCLOB/`text`/`nvarchar(max)` | 잘린 표시(그리드 지연 변환 43) · 더블클릭 = **값 보기 창** | 텍스트 탭(줄 바꿈 · 검색 · 편집 → 커밋 = ChangeSet 셀) |
| BLOB/BYTEA/`varbinary(max)`/RAW | `<BLOB n bytes>` 표시 · 편집 없음 | 16진수 덤프 탭(오프셋 · 16바이트/줄 · 문자 열) · 이미지 감지(PNG/JPEG/GIF/BMP 시그니처 → 미리보기) · **파일로 저장** · 파일에서 넣기(2차) |
| XML/JSON | 텍스트 + 정리(pretty) 버튼 | 텍스트 탭 |
| 페치 규칙 | 결과 세트에 LOB 본문이 이미 있으면 그대로 · 없으면(드라이버가 잘라 옴) 키로 **그 셀만 재조회**(`SELECT col FROM t WHERE 키`) · 상한 `grid.lob_view_max_mb`(기본 16) |

값 보기 창은 모델리스(메모리 창 규칙 · 80) · 편집기 탭과 같은 고정폭 상자(`preview_box`) · 읽기 전용이면 §214 규칙(잘라내기/붙여넣기 흐림 · 조합 차단).

---

## 6. 엑셀식 상호작용(키 · 마우스 · 클립보드)

| 동작 | 키/마우스 | 규칙 |
|---|---|---|
| 편집 진입 | F2 · Enter · 더블클릭 · **글자 타이핑**(기존 값 대체 · 엑셀) | `CellSpec.read_only`/삭제 행/편집 불가 결과면 상태줄 이유 |
| 커밋 · 이동 | Enter = 커밋 + ↓ · Tab = 커밋 + → · ⇧Tab = ← · 클릭 다른 셀 = 커밋 | 검증 실패 = 상자에 남고 붉은 테두리 + 안내 · Esc = 취소 |
| NULL | ⌘0(맥) / Ctrl+0 · 우클릭 ▸ Set NULL | nullable 아니면 거부 |
| 삭제 키 | Delete = 선택 셀 **비움**(NULL 또는 빈 문자열 `grid.edit_empty`) · ⌘⌫ = 행 삭제 토글 | 엑셀 = 내용 지움 · 행 삭제는 명시 키/메뉴 |
| 한 줄 복제 | ⌘D · 우클릭 ▸ Duplicate row | 원본 아래 새 행 · 키 열 비움(자동 증가/DEFAULT) |
| 행 추가 | ⌘⇧N · 툴바 `+` · 우클릭 ▸ Insert row | 맨 아래(또는 선택 아래) · 기본값 열 = `<default>` |
| 복사 | ⌘C = 선택 셀/영역 TSV(41 규칙 · 지금 있음) | 줄 복사 = 행 선택 ⌘C |
| 붙여넣기 | ⌘V = 앵커 셀부터 행렬 채움 · **아래로 부족하면 행 자동 추가** · 오른쪽으로 넘치면 잘라 내고 안내 | 여러 줄 · 셀 안 줄바꿈(따옴표) · 검증 실패 셀 = 건너뛰고 `PasteReport.rejected` 안내 · 한 묶음 되돌리기 |
| 채우기 | ⌘↩(2차) 선택 영역을 첫 셀 값으로 | 2차 |
| 적용 · 취소 | ✓(툴바) / ⌘S(편집 모드) · ✗ = 전부 되돌림 | 미리보기(생성 SQL) → 실행 |

셀 표시(§221 보충): 수정 셀 = **글자색 `warn` + 굵게**(배경 6 %) · 행 띠 = 추가 초록 · 수정 강조색 · 삭제 빨강 · 삭제 = 취선·흐림 · 검증 실패 = 붉은 테두리 · 적용 실패 행 = 붉은 배경. 상태줄 = `편집 3 · 추가 1 · 삭제 0` + 편집 가능 여부/이유.

---

## 7. 편집 가능 판정 · SQL 생성 · 적용(77 §2-3 그대로 · 세부)

| 단계 | 설계 |
|---|---|
| 판정 | 출처 문장(`Grid.source`)이 **단일 테이블 SELECT**(nsql-script 분해 · JOIN/GROUP/DISTINCT/집합/서브쿼리 FROM 없음) → 테이블 `ObjectRef` → `MetaStore` 컬럼·PK/UK. 키 없음 = 물리 키 열(Oracle `ROWID` · PG `ctid` · SQLite `rowid` · MSSQL `%%physloc%%`)을 **숨은 열로 재조회**(`Requery`) → 그것도 안 되면 D-198(전체 열 = 값 · 1행 확인). 뷰·집계·PROD(`tx.prod_*`)는 읽기 전용 + 이유 |
| `SqlGen` | 순서 DELETE → UPDATE → INSERT · 식별자 인용 = 방언 · 값 = **바인드**(`?`/`:n`/`@pn`/`$n` = `Caps`) · NULL 비교 = `IS NULL` · 날짜 = 날짜 바인드 · `now` 토큰 = 방언 함수 · 새 행 빈 셀 = `DEFAULT`(지원 방언) 또는 열 생략 · **미리보기** = 바인드 값을 리터럴로 치환한 읽기용 텍스트(41 규칙) + 실제 실행은 바인드 |
| 적용 | `gate_open` → 트랜잭션 시작(수동 모드면 이미 열린 것에 이어 감) → 문장 순서 실행 → 각 문장 **영향 행 수 = 1** 검사(`grid.edit_strict` · 0/2+ = 그 문장부터 중단 + 셀 표시 + 롤백 제안) → 자동 커밋 모드면 커밋 · 수동이면 34 UX(Commit/Rollback 배지) → 성공 = `grid.edit_refresh`(재조회 · 기본) 또는 로컬 반영(추가 행의 서버 생성 값은 재조회만 정확) |
| 안전 | PROD 확인(`run.prod_confirm`) · 잠금 방지 56(수동 커밋 열어 둔 채 방치 경고) · 로그 48(생성 문장 · 영향 행 수) · 트랜잭션 로그 44 |

---

## 8. 설정 · 명령 id · 부하원

| 종류 | 항목 |
|---|---|
| 설정 `grid.*` | `edit`(편집 허용 · on) · `edit_empty`(null/empty) · `edit_strict`(on) · `edit_refresh`(requery/local) · `edit_dup_keys`(복제 시 키 열 비움 · on) · `lob_view_max_mb`(16) · `paste_max_rows`(10,000 · 초과 = 안내) · `edit_confirm_rows`(적용 전 확인 문장 수 상한 · 50) |
| 명령 `grid.*` | `grid.edit.begin` · `commit_cell` · `cancel` · `set_null` · `dup_row` · `insert_row` · `delete_row` · `paste` · `apply` · `revert` · `preview_sql` · `view_value` — 팔레트·메뉴·키 같은 id |
| 부하원(39 §3) | LOB 재조회 = 세션 왕복(gate) · 값 보기 창 = 열려 있을 때만 · 붙여넣기 상한 |
| i18n | 전부 `Msg`(안내 · 메뉴 · 상태줄) |

---

## 9. 구현 단계

| 단계 | 내용 | 시험 |
|---|---|---|
| **E-1** nexa-ctl `gridedit` 부품 | spec · changeset · paste · datetime · keymap(순수) · live(TextBox 래핑) | 단위 시험(검증 · 되돌리기 · 행렬 해석 · 자동 확장 · 날짜 24형식 · 레이아웃) |
| **E-2** nexa-sql 어댑터 | `Value`↔문자열 · `Column`+MetaStore → `CellSpec` · 편집 가능 판정(`editable.rs`) | 순수 함수 시험 · 4 방언 판정 표 |
| **E-3** 그리드 UI | 편집 모드 · 덧그리기 · LiveEditor 배치/스크롤 이탈 · 키·메뉴 · 복제/추가/삭제 · 붙여넣기 자동 확장 · 상태줄 | 격리 기동 명령(`grid.edit:<r>,<c>,<text>` · `grid.dump`) 캡처 |
| **E-4** `SqlGen` + 미리보기 + 적용 | 4 방언 문장 · 바인드 · 영향 행 수 · 트랜잭션 · 재조회 | 방언별 골든 · 실서버 임시 테이블(`NSQLT_*` · 61 §2-4 ⑤) |
| **E-5** LOB 값 보기 창 | 텍스트/16진수/이미지 · 저장 · CLOB 편집 | 격리 캡처 |
| **E-6** 문서·확인표 | 87 갱신 · 위키 사용법 · U-* · 성능(편집 모드 페인트 예산 26 §5) | `win-func-check` 시나리오 |

결정 대기: **D-210** 빈 문자열 커밋의 기본 = NULL(`grid.edit_empty`) · **D-211** 복제 행의 키 열 = 비움(DEFAULT/자동 증가) vs 원본 값 복사 후 사용자 수정 · **D-212** 적용 뒤 기본 = 재조회(정확) vs 로컬 반영(빠름) · **D-213** LOB 인라인 편집 허용 범위(CLOB 텍스트만 · BLOB은 파일 넣기).

---

## 10. 구현 1차(09-26 · journal §217)

| 단계 | 들어간 것 | 위치 |
|---|---|---|
| E-1 | `nexa_ctl::gridedit`(spec · datetime · changeset · paste · keymap · live) · 시험 20 | nexa-ui 77차 `45be503` · docs/21 §3-2 |
| E-2 | `gridedit_sql.rs` = 단일 테이블 판정 `analyze`(주석·문자열 제거 골격 · JOIN/GROUP/DISTINCT/집합/서브쿼리/식 거부) · `generate`(DELETE→UPDATE→INSERT · 방언 자리 표시 `Caps.marker` · 날짜 `VarType::Date/Timestamp` 바인드 · `now`/`today` = 방언 식 · 기본값 있는 NULL 열 = 생략) · `preview_text`(리터럴 · 날짜는 `TO_DATE`/`TIMESTAMP ''`/ISO) · 시험 4 | nexa-sql |
| E-3 | `grid.rs`: `EditCfg` · `GridEdit`(ChangeSet + LiveEditor) · 행 모델 = `row_order`에 가상 index(`≥ src_len` = 추가 행) · `rebuild_row_order`(추가 행 = 원본 아래) · 페인트 덧그림(수정 셀 강조색 14 % · 새 행 초록 띠 · 삭제 취선·흐림 · 실패 행 붉은 배경) · 편집기 오버레이(`cell_rect` 추종) · Enter/타이핑/더블클릭 진입 · Delete/Backspace 비움 · Esc · Tab/Enter/↑↓ 커밋 이동 · 우클릭 메뉴 10항목 · 툴바 `row.*` 5 배선 · 붙여넣기(호스트 ⌘V → `paste_text` · 행 자동 추가) · 복사 = 덧그림 스냅숏 · 정렬은 편집 중 잠금 · 푸터에 `수정 n · 추가 m · 삭제 k` | `grid.rs` |
| E-4 | 워커 `Cmd::Apply{key, stmts, strict}` → `Runner::apply_changes`(자동 커밋 = 시작문→전부→커밋 · 실패 롤백 · 수동 = 열어 둠 + 커밋/롤백 표식) → `ConnOutcome::Applied{rep}` → 성공 = 재조회(`grid.edit_refresh`) · 실패 = 토스트 + 행 표시 · 키 = `Cmd::Keys` 캐시(41 D-66 · 결과에 모든 키 열이 있는 PK → 유니크 → 전체 열 D-198) · 세션 바쁨 = 재요청 루프 · 열 명세 = `MetaStore` 컬럼(길이·NOT NULL·기본값 · Oracle DATE = 시각) | `main.rs` `worker.rs` `nsql-run` |
| E-5 | 값 보기 = 읽기 전용 글 창(`SqlPrevWin::open_plain` · 텍스트 · 이진 = 16진수 덤프 `hex_dump`) · SQL 미리보기 = 같은 창 | `sqlprev_win.rs` |
| 설정 | `grid.edit`(on) · `grid.edit_empty`(null/empty) · `grid.edit_strict`(on) · `grid.edit_refresh`(requery/local) · `grid.paste_max_rows`(10,000) | nsql-settings |
| 자체 시험 | 기동 명령 `grid.edit.set:<행>;<열>;<글>` · `grid.edit.cmd:<id>`(`row.dup/del/save/cancel` · `grid.edit.*`) · `grid.select:<행>;<열>` · `grid.dump:<파일>`(행 상태·덧그림·키·명세) — SQLite 격리 E2E(§217): 수정 3·복제·NULL·미리보기·적용·재조회·삭제·되돌리기/다시 하기·적용 전부 서버 값 일치 | `main.rs` |

### 10-1. 실기 결함 수정(§221)

값 미반영(커밋 셀 선독 · `commit_live` 선행) · 클립보드/전체 선택 편집 한정 · 변경 목록 창(`grid.edit.changes`) · 행 식별 띠(초록/강조/빨강) · 실제 키 경로 시험 4 · 메타에 없는 객체 컬럼 재요청. **변경 판정 규칙** = 셀마다 (열, 새 값)을 원본과 따로 들고, 최종 값이 원본과 같으면 항목을 지운다(A→B→A = 미변경 · Esc = 기록 없음) — `ChangeSet::set_cell(original)`.

## 11. 남은 것(T-182 후속)

- 물리 키(Oracle `ROWID` · PG `ctid` · SQLite `rowid` · MSSQL `%%physloc%%`) 숨은 열 재조회 — 지금은 PK/UK → 전체 열(D-198).
- 값 보기 창 = 이미지 미리보기 · 파일로 저장/넣기 · CLOB 편집 → 커밋(§5 2차) · `grid.lob_view_max_mb`.
- `grid.edit_refresh=local`(재조회 없이 반영) — 지금은 requery와 같다.
- 편집기 안 Shift+Tab(← 이동) · F2(북마크와 충돌 → 포커스별 키맵) · ⌘D 복제 키.
- 실서버 4방언 시험(Oracle DATE 바인드 · SQL Server `@p` · PG `$n` · NULL 키) · 편집 모드 페인트 예산(26 §5) · 위키 사용법 · 확인표 U-*.

---

## 12. 다른 그리드 편집기 비교 · 최신 구조 제안(T-230 · 사용자 09-26 "변경 추적 · 수정 반영 · 필요할 때만 다시 읽기 · 행 단위 업데이트")

> 조사 09-26(공식 문서 우선 · ⚠ = 2차 출처). 도구 = DBeaver · DataGrip · SSMS · Azure Data Studio · TablePlus · Toad · SQL Developer · pgAdmin · HeidiSQL · Navicat · Beekeeper Studio · DbGate · Sequel Ace · Excel/Sheets · AG Grid/Handsontable/Glide.

### 12-1. 비교표

| 도구 | 변경 추적 | 전송 시점 | 트랜잭션 | 키 없음 | 저장 뒤 | 동시성 | UX 특징 | 알려진 결함 |
|---|---|---|---|---|---|---|---|---|
| DBeaver | 행 상태 3(Normal/Added/Removed) + 열별 diff(원본 보존 · 같아지면 변경 아님 ⚠) | Save/Cancel + Generate Script | 연결의 Auto/Smart commit | 오류 다이얼로그 → **전 열 키 / 가상 키**(프로필 저장) · ROWID류 가상 열 | 설정 `Refresh after update` = **편집된 행만 재조회**(PG RETURNING ⚠) | WHERE = 키만 · 배치 부분 성공 가능 | 삭제 빨강 · Set NULL/default · Advanced Paste(다중 행·NULL) · 상태 `PRIMARY KEY x`/`VIRTUAL` | 주석·다중 결과셋에서 "unique key 없음"(#37459) |
| DataGrip | 로컬 복사본 · 행 색 3종 · **선택 범위 Revert** | Submit(Ctrl+Enter) + Preview Pending Changes | Tx Auto/Manual(수동 = Submit 뒤 Commit/Rollback) | 읽기 전용 → "Select columns for row identification"(가상 키) | Reload Page 별도 | **update count ≠ 1 = 실패** · 병합 다이얼로그 | Set NULL/DEFAULT 단축키 · Clone Ctrl+D · CSV 줄 붙여넣기 | 메타 오래됨 → count 오류 |
| SSMS(Edit Top 200) | 행 스냅숏 | **행을 떠날 때 즉시 커밋** | 행별 자동 · 실패 행 마커 | 편집 가능하되 **전 열 WHERE**(text에 `%_[` = 잘못된 문장 KB 925719) | 그 행 재조회 · Ctrl+R 전체 | **옛 값 WHERE** → "Data has changed" 3택(덮어쓰기/서버값/취소) | NULL 타이핑 · Ctrl+0 · 조인/집계/식 열 읽기 전용 | 이진 열 편집 불가 |
| Azure Data Studio | SSMS식 | 행 이동 커밋 | 행별 | PK | 행 재조회(identity/getdate 미반영 결함 #6638) | — | 행 Revert 메뉴 | Esc 회귀 #9083 |
| TablePlus | 미커밋 강조(노랑/주황 ⚠) | ⌘S Commit + ⌘⇧P 코드 리뷰 · ⌘⇧⌫ Discard | 단일 커밋 · Safe Mode 5단계 | PG `ctid` · SQLite rowid 미지원(#3504) | 미명시 | — | ⌘I 삽입 · **⌘D 복제** | commit 혼동(#818) |
| Toad for Oracle ⚠ | 행별 Post | Post → Commit 2단계 | 수동/자동 | **ROWID 필수**(`EDIT emp`) | 재조회 | — | Show ROWID 옵션 | — |
| SQL Developer | 대기 변경 | Commit/Rollback 버튼 | Oracle 트랜잭션 | ROWID | Refresh | — | Single Record View · `…` 편집기 | — |
| pgAdmin 4 | 스테이징 | Save Data Changes(F6) 일괄 | **실패 = SAVEPOINT 롤백** · Auto commit/rollback 옵션 | **PK/OID 전부 선택**해야 편집 · 뷰 불가 | 재조회 | — | 빈칸 = NULL · `''` 입력 · Paste 행 = 새 행(SERIAL 보존 변형) · JSON 편집기 | — |
| HeidiSQL | 행 | **행을 떠나면 즉시 UPDATE**(끌 수 없음) | 자동 | 키 없으면 차단 · 선택 열에 키 없어도 차단(#2600) | — | — | Insert value ▸ NULL/함수 | 확인 없음 |
| Navicat ⚠ | 행 | 이동 시 저장 · 18 = Batch Apply/Discard | — | — | — | — | NULL vs 빈 문자열 구분 | — |
| Beekeeper Studio | **초록/빨강/주황** 3색 | Apply/Reset + Copy To Sql | 단일 트랜잭션 | PK 필수(SQLite rowid) | — | — | Backspace = NULL · 다중 행 붙여넣기 | **정렬/필터하면 스테이징 폐기** |
| DbGate 7.2 | 편집 가능 쿼리 결과(Premium) | SQL 미리보기 | — | PK 식별 | — | — | — | 세부 미문서 |
| Sequel Ace | 없음 | **셀 확정마다 즉시 UPDATE** | 자동 | PK | — | — | — | 식 기본값 INSERT 불가 · ENUM 무음 폐기 |
| Excel/Sheets | — | — | — | — | — | — | Enter 모드 vs Edit(F2) · Enter/Tab/Shift 이동 · **Ctrl+Enter 제자리** · 붙여넣기 = 앵커 확장 · 단일 값 → 범위 채움 | — |
| AG Grid | `undoRedoCellEditing` 10단계(정렬/필터/외부 갱신 시 초기화) · **Batch Editing**(pending = 렌더/복사에만 · 정렬·집계 미반영 · commit = undo 1단계) | — | — | — | — | — | `cellValueChanged`(commit 시) | — |
| Handsontable / Glide | UndoRedo 플러그인(`loadData` = 비움) / 상태는 호스트(`onCellsEdited` 일괄 · `onPaste` 탭/줄바꿈) | — | — | — | — | — | `afterChange(source)` | — |

### 12-2. 우리 현재 위치

이미 있음 = ChangeSet 오버레이 + 원복 감지(A→B→A 미변경) · 한 트랜잭션 + 문장당 affected=1 엄격 · PK→UK→전 열 · 적용 뒤 전체 재조회 · NULL 메뉴/Delete · 복제/삽입/삭제 · TSV 붙여넣기 행 확장 · 읽기 전용 사유 · SQL 미리보기 · 변경 목록 창 · 행 식별 띠(초록/강조/빨강). 도구 중 **DataGrip + Beekeeper + DBeaver의 합집합**에 가깝고, 없는 것은 행 단위 재조회 · 동시성 옵션 · 가상 키 · 충돌 UX · 정렬 중 보존 검증.

### 12-3. 제안 구조(2025~26 · 채택)

1. **변경 집합** = 지금 모델 유지(행 상태 {Clean, Modified{열 → (원본, 새 값)}, Inserted, Deleted} · 같아지면 제거) + **행 식별자 고정**(원본 행 index/키) → 정렬·필터는 뷰 투영이라 ChangeSet이 살아남는다(Beekeeper의 "정렬 = 폐기" 회피 · 지금은 편집 중 정렬 잠금 → **정렬 허용 + 보존**으로 바꾼다 · pending 값은 정렬·Σ에 미반영을 명시 = AG Grid).
2. **적용 파이프라인**(지금과 같음 · 보강) = 검증 → `SqlGen`(변경 열만 SET · 키 WHERE) → 미리보기 → `gate_open` → 한 트랜잭션(수동 커밋 = 열린 트랜잭션에 참여 · **SAVEPOINT**로 부분 실패 격리 = pgAdmin) → affected≠1 = 롤백 + 실패 행 마커 → 커밋 → **재조회 전략 §12-4**.
3. **행 단위 재조회(P1)** — 아래.
4. **낙관적 동시성 옵션** `grid.edit_concurrency = key | key_old | all_old`: `key`(기본 · DBeaver/DataGrip) · `key_old` = 변경한 열의 **옛 값만** WHERE에 추가(충돌 감지 ↔ LOB 회피의 균형 · 권장 기본 후보) · `all_old` = SSMS식(LOB/float/text 제외 규칙 필수). affected=0 = "다른 사용자가 수정/삭제" 충돌.
5. **충돌 UX** = 행 단위 3택(SSMS): 덮어쓰기 / 서버값 다시 읽기(내 변경은 diff로 유지) / 계속 편집 — 전체 취소가 아니라 그 행만.
6. **되돌리기 층** = L1 셀 편집기(Esc = 원값) · L2 ChangeSet 연산 로그(셀·붙여넣기 묶음·행 삽입/삭제 = 1단계 · 지금 있음) · L3 적용 뒤 = "역 ChangeSet 만들기" 제안만(자동 실행 금지) · **선택 범위 Revert**(DataGrip).
7. **키보드 표준** = Enter/F2/더블클릭 편집 · Enter 확정+↓ · Tab 확정+→ · **Ctrl+Enter 제자리 확정** · Esc 취소 · Ctrl+0/Delete = NULL · Ctrl+D 복제(그리드 포커스 한정) · Ctrl+S 적용 · Ctrl+Shift+P 미리보기.
8. **키 없음 3안** `grid.edit_no_key = readonly | all_columns | rowid`(Oracle ROWID · PG ctid(VACUUM 뒤 무효 → 재조회 시 재확인) · SQLite rowid · MSSQL %%physloc%%) + **전 열 키 경고 배지 + 사전 `SELECT COUNT(*)` 검사 옵션**(DBeaver 경고 · SSMS 다중 행 방지).
9. **Set DEFAULT**(기본값 괄호 표시) · NULL vs `''` 구분 표시 · 단일 값 → 선택 범위 채움 · SERIAL/IDENTITY 열 건너뛰기 옵션(pgAdmin).

### 12-4. 행 단위 재조회(사용자 "필요한 경우만 다시 읽기 · 행 단위 업데이트")

| 단계 | 규칙 |
|---|---|
| 기본 | 적용 성공 뒤 **변경·추가된 행만** `SELECT <원 select 목록> FROM t WHERE 키 IN (…)`(키가 하나면 `IN` · 복합이면 `OR (k1=… AND k2=…)` 묶음 · 100행 단위)로 다시 읽어 그 행만 교체 · 삭제 행은 로컬 제거 · 다른 행·스크롤·선택·정렬은 그대로 |
| 왕복 0 후보 | PG/SQLite `RETURNING *` · MSSQL `OUTPUT INSERTED.*` · Oracle `RETURNING … INTO`(열 지정) — `Caps` 포트에 `returning: None/Clause/Output` 추가 시 UPDATE/INSERT 응답에서 바로 |
| 추가 행의 키 | INSERT 뒤 키를 모르면(시퀀스/IDENTITY) `RETURNING`으로 받거나 → 없으면 그 행만 **전체 재조회 폴백** |
| 전체 재조회 폴백(순수 판정 + MC/DC) | 키 없음(전 열 키) · 원 문장이 ORDER BY로 순서가 바뀔 수 있고 사용자가 "정렬 반영"을 켬 · 트리거/생성 열이 있다고 메타가 말함(다른 행도 바뀔 수 있음) · Σ 건수/페이지 상한(`more`) 상태 · 서버 페이지 이어 받기 중(커서) · 설정 `grid.edit_refresh = full` |
| 로컬 반영(`local`) | 재조회 없이 ChangeSet 값을 세트에 굳힘(서버 기본값·트리거 결과는 모른다는 배지) — 오프라인/느린 망용 |
| 구현 | `ResultData`는 `Arc` 세그먼트(불변) → 행 교체 = **덧그림 층 2(committed overlay)** 또는 세그먼트 `Arc::make_mut`(변환 스레드 공유 시 복사) — 덧그림 층이 DR-33에 맞다(세트 불변 · 뷰는 그대로) |
| 설정 | `grid.edit_refresh = rows(기본) | full | local` |

### 12-5. 우선순위(TODO 등재)

1. **P1 행 단위 재조회**(§12-4 · `refresh=rows` 기본 · 폴백 판정 순수 함수 + MC/DC · `Caps.returning`) 2. **P1 정렬/필터 중 ChangeSet 보존**(잠금 해제 · pending 미반영 명시) 3. **P2 동시성 `key_old` + 충돌 3택** 4. **P2 전 열 키 경고 배지 + 사전 COUNT 검사** 5. **P2 Set DEFAULT · NULL/`''` 구분 · 범위 채움 · SERIAL 건너뛰기** 6. **P3 실패 행 마커 + 그 행만 재시도 · SAVEPOINT** 7. **P3 키보드 표준(Ctrl+Enter · Ctrl+0 · Ctrl+D 그리드 한정)** 8. **P3 ROWID/ctid/rowid 키 3안**.

출처: DBeaver Data-Editor/Virtual-Keys/Data-Editor-preferences · DataGrip submitting-and-reverting-changes/data-editor-and-viewer/rows · MS Learn work-with-data-in-the-results-pane · troubleshoot error-use-ssms-update-row-table · ADS #6638 #9083 · TablePlus docs/이슈 #266 #3504 · pgAdmin editgrid/query_tool · HeidiSQL help/#2600 · Beekeeper editing-data/#1532 · DbGate 7.2.0 · Sequel Ace #1935 #2616 · AG Grid undo-redo-edits/cell-editing-batch · Handsontable undo-redo · Glide editing.
