# 96. Ctrl 객체 하이퍼링크 + 설명 툴팁(T-256 · 객체 척추의 편집기 마디)

> **요청**(사용자 09-29): *"SQL 구문의 위치와 Alias를 기반해서 테이블, 컬럼 여부를 판단하고 해당 항목에 대한 Description 정보를 Tooltip으로 표시 · 표시 위치는 대상 텍스트의 우상단(설정) · 표시 여부 설정(기본 표시) · CTRL 키를 누르고 있을 때만 동작 · 객체(테이블·프로시저·뷰·컬럼 등)는 하이퍼링크 형태 · 좌클릭 = Description 클립보드 복사(나중에 다른 기능으로 변경 예정) · 우클릭 = 팝업 메뉴 '설명 복사' · CTRL 누르면 SQL 분석으로 하이퍼링크 대상 선별 + 클릭 이벤트 매핑 · 성능향상모드에서는 꺼짐 · 파일 용량 레벨에 따라 자동 꺼짐."*
>
> 자리 = [77 데이터 작업대 구조](77-data-workbench-architecture.md) **객체 척추**의 편집기 마디(편집기의 이름 → `ObjectRef` → `MetaStore` → 동작). 코드 = `crates/nexa-sql/src/app/objlink.rs` · nexa-ui 93차 `TextBox::set_link_marks/set_link_hot/index_at_point`.

## 0. 결론 한 장

| 항목 | 규칙 |
|---|---|
| 켜지는 조건 | **Ctrl(맥 ⌘)을 누르는 동안**만 · `objlink.enabled`(기본 켬) · 구문 SQL 탭만 · **향상 모드·큰 파일 모드에서도 동작**(표시만 `none`으로 고정 · 아래 표시) · 큰 파일 모드(L1+) 또는 문서 크기 > `objlink.max_kb`(기본 512 KB · 0 = 무제한) = **부분 분석**(보이는 구간을 앞뒤 `;`까지 넓힘 · 스크롤로 구간을 벗어나면 다음 hover에서 재분석) |
| 무엇이 링크인가 | **관계 자리**(`FROM`·`JOIN`·`INTO`·`UPDATE`·`TABLE`·`DESC`·`TRUNCATE`·`USING` 뒤 · `FROM` 목록의 `,` 뒤) = 테이블/뷰 · **`별칭.이름`** = 그 별칭 테이블의 컬럼 · **`스키마.테이블`**/**`스키마.테이블.컬럼`** = 카탈로그에서 관계로 확인될 때 · **`이름(`** = 카탈로그의 프로시저/함수/패키지일 때(내장 함수 `NVL`·`SUM` 등은 아님) · **`패키지.루틴`** · 접두 없는 이름 = 문장의 별칭 테이블 중 **컬럼을 가진 첫 테이블**(컬럼이 안 읽혀 있으면 요청만 하고 이번엔 링크 아님) |
| 링크가 아닌 것 | 키워드 · 별칭 자체(`a` · `b`) · CTE/파생 테이블 이름과 그 컬럼(`local`) · 한정자 토큰(`a.` 의 `a`) · 문자열·숫자·바인드·주석 |
| 표시(2차 · 사용자 09-29) | `objlink.display` = **`hover`(기본) = 마우스 아래 링크 하나만 밑줄(1 px)** · `all` = Ctrl 동안 전부 · `none` = 그리지 않음(툴팁·클릭은 동작 · 다시 그리기 0) · 향상 모드(`perf::BOOST`)와 큰 파일 모드는 **`none` 고정** · 커서 아래 링크 = 글자도 링크 색 · 툴팁 = `종류 이름` 한 줄 + 설명(코멘트 · 없으면 "(설명 없음)" · 미확인 = "(현재 연결에 없는 객체)") · 자리 = 링크 글자 기준 `objlink.tooltip_pos`(**우상단** 기본 · 좌상단/우하단/좌하단) · 창 밖으로 안 나감(nexa-ctl `draw_tooltip_in` 안전망 · 61 §2-2) · `objlink.tooltip`(기본 켬) |
| 정상 / 미확인 | 링크는 구조(자리·별칭)로 판정하되 **현재 연결의 메타에서 확인**되면 정상(`known`) · 객체 자리인데 메타에 없으면 **미확인**(설명을 가져올 수 없음) → 각각 색·두께·모양 따로: 정상 `objlink.line_color/line_width/line_style`(기본 테마 강조색 · 1 px · 실선) · 미확인 `objlink.bad_color/bad_width/bad_style`(기본 **밝은 벽돌색 `#B7472A`** · 1 px · 물결) · 모양 = 실선·대시·점선·물결·대시 물결 · 컬럼 목록을 아직 모르는 테이블의 컬럼은 일단 정상 → 메타가 오면(스냅숏 `stamp`) 다시 판정 |
| 클릭 | 좌클릭 = **설명을 클립보드로**(상태줄 "X 설명 복사됨: …" · 설명 없음 = 안내) · **Shift+좌클릭 = `이름 - 설명`**(테이블 = `테이블 - 설명` · 컬럼 = `테이블.컬럼 - 설명` · 스키마 접두 = `objlink.show_schema` · 설명 없음 = 이름만 + 안내 · 사용자 09-29) · 우클릭 = 메뉴 **"설명 복사" · "이름 - 설명 복사"** · 링크 위 클릭은 편집기로 가지 않는다(캐럿 이동 없음) · 링크 밖은 그대로 편집기 |
| 메타 원천 | 탐색기 메타 스냅숏 **단일 원천**(`ExplorerSet::meta_view` · 76·79·85) — 설명 = 객체 코멘트(`ObjEntry.comment` · `TableDetail.comment`) · 컬럼 코멘트(`ColEntry.comment` · L2 코멘트 워머 `col_comments` · 상세 `col_comments`) · 없으면 **지연 요청**(테이블 상세 `request_detail` · 컬럼 `request_columns`) → 다음 hover/그리기에서 채워짐(61 §1-8) |
| 비용 | Ctrl을 **누른 순간 1회** 스캔(렉서 + 문장별 `context_at` 1회 + 심볼 조회) · 본문/탭/메타 stamp/표시 방식이 바뀌면 다음 마우스 이동에서 재스캔 · 부분 분석은 보이는 구간 ± 문장 경계(최대 64 K 글자씩)만 · `none`은 툴팁이 켜져 있을 때만 다시 그림 · Ctrl을 떼면 전부 걷음(비용 0) · 우클릭 메뉴는 `open_menus` 비트 4096(팝업 배타 게이트) |

## 1. 분석 규칙(`objlink::scan` · 순수 함수)

토큰은 nsql-format 렉서(주석·문자열·힌트·바인드 구분 · 바이트 구간)를 쓰고, 별칭 표는 nsql-script `intel::context_at`를 **문장마다 한 번**만 부른다(같은 문장의 토큰은 캐시). 절 추적은 괄호 깊이별로(`FROM` 목록의 `,` 판정). 호스트 메타는 `Resolver` 포트(`object_class` · `has_column`)로만 묻는다 — 시험은 가짜 해석기로 돈다.

| 자리 | 판정 | 링크 종류 |
|---|---|---|
| `FROM t` · `JOIN s.t` · `INSERT INTO t` · `UPDATE t` · `TRUNCATE TABLE t` · `FROM a, b` | 관계 자리 · CTE/파생 이름 제외 · `USING (`는 제외 | Table(`schema` 선택) |
| `a.col`(a = 별칭) | `resolve_alias` → 테이블(local 제외) | Column(owner = 테이블) |
| `s.t.col` | `object_class(s, t)` = Relation | Column |
| `s.pkg.proc` · `pkg.fn(` | Package | Routine(owner = 패키지) |
| `s.t`(관계 자리 아님) | `object_class(s, t)` = Relation/Routine | Table / Routine |
| `fn(` | `object_class(None, fn)` = Routine/Package | Routine |
| `col`(접두 없음) | 별칭 테이블 중 `has_column` 첫 것 | Column |

범위는 **문자 인덱스**(편집기 불변식 · 바이트 → 문자 증분 변환) · 한정자 사슬 전체(`a.col`)가 한 링크.

**정상/미확인 판정**(`Link.known`): 관계 자리 테이블 = `object_class` Relation이면 정상 · 별칭 컬럼 = 테이블이 Relation이고 `has_column` ≠ `Some(false)`(컬럼 목록 미확정 `None`은 정상 취급 · 호스트가 요청) · 카탈로그로 확인해 링크가 된 것(루틴 · `s.t` · 접두 없는 컬럼)은 늘 정상. 접속이 없거나 다른 연결이면 관계 자리 이름은 전부 미확인 = "구조적으로 객체인데 설명은 없다"(사용자 09-29 · SQLite Demo 연결에서 `DEMO_BSY`).

## 2. 설정(`objlink.*` · 분류 Editor ▸ Object Links)

| 키 | 기본 | 뜻 |
|---|---|---|
| `objlink.enabled` | on | Ctrl 동안 링크(향상 모드·큰 파일 모드에서도 켜짐 · 표시만 none) |
| `objlink.display` | hover | 밑줄 표시 방식 all/hover/none · **`perf::BOOST` = none 강제** · 큰 파일 모드 = none |
| `objlink.tooltip` | on | 설명 툴팁 표시 |
| `objlink.tooltip_pos` | top_right | 툴팁 자리(top_right/top_left/bottom_right/bottom_left) |
| `objlink.show_schema` | off | 툴팁·상태줄 이름에 스키마 접두(`BISCM.TB_ORDER`) · 끄면 이름만(컬럼 = `테이블.컬럼`) — 사용자 09-29 |
| `objlink.line_color` | (빈 = 테마 강조색) | 정상 객체 밑줄 색 `#RRGGBB` |
| `objlink.line_width` | 1 | 정상 객체 밑줄 두께 px(0~4 · 0 = 밑줄 없음) |
| `objlink.line_style` | solid | 정상 객체 밑줄 모양 solid/dashed/dotted/wavy/wavy_dashed |
| `objlink.bad_color` | `#B7472A` | 미확인 객체 밑줄 색(밝은 벽돌색 · 빈 = 테마 danger) |
| `objlink.bad_width` | 1 | 미확인 객체 밑줄 두께 |
| `objlink.bad_style` | wavy | 미확인 객체 밑줄 모양 |
| `objlink.max_kb` | 512 | 이 크기 넘는 문서는 **보이는 부분만** 분석(0 = 무제한) · 큰 파일 모드는 늘 부분 |

## 3. 배선(39 §3 부하원 · 30 §2 부품)

- 사건: `ModifiersChanged` → `objlink_sync`(켜기/걷기) · `MouseMove` → `objlink_hover`(본문·탭 바뀌면 재스캔 · hot 갱신 · 메타 선요청) · `MouseDown/RightDown` → `objlink_click`(링크 위면 소비) · `route_objlink_menu`(메뉴 모달 · 바깥 클릭 = 닫고 통과) · 설정 `objlink.*` 변경 → `objlink_sync`.
- 그리기: 툴팁 본문·자리는 **표면을 빌리기 전에** `objlink_tip`으로 셈하고 팝업 층에서 `paint_tip`(nexa-ctl `draw_tooltip_in` · 원하는 좌표를 0×0 기준으로 역산) + `objlink_menu.paint`.
- nexa-ui 93차: `TextBox` `link_marks`/`link_hot`(밑줄·강조색 · 바뀌었을 때만 true) · `index_at_point`(점 → 글자 경계 · 본문 밖 None). **94차**: `link_marks` = `(시작, 끝, known)` · `LinkStyle{color, width, line: LinkLine}` 두 벌 `set_link_styles(ok, bad)` · 모양 그리기 `draw_link_line`(실선·대시 4/2·점 w/2w·물결 = 삼각파 주기 4 px 진폭 2 px·대시 물결 = 2주기 그리고 1주기 비움 · `fill_rect`만) · `visible_range()`(마지막 그리기의 보인 글자 구간 · 부분 분석의 창).
- 표시 방식은 편집기에 넣는 `link_marks`로 구현: `all` = 전부 · `hover` = hot 하나만 · `none` = 빈 목록(hot도 None · 글자 색 안 바꿈). 스타일은 `App::apply_objlink_style`(기동 · `objlink.*` 변경)이 전 탭에 적용(`Editors::set_link_styles` · 새 탭도 물려받음).

## 4. 시험

- 단위: `app::objlink::tests` 3(테이블·컬럼·루틴·패키지·접두 없는 컬럼·내장 함수 제외·별칭/키워드 제외 · 스키마 한정 · CTE 제외 · 문자 인덱스 범위(한글 앞) · **미확인 판정** = 메타에 없는 테이블·그 컬럼·확인 테이블의 없는 컬럼 = `known=false` · 컬럼 미확정 테이블 = 정상) · nsql-settings/i18n 등재 시험 · nexa-ctl `textbox::link_marks_change_detection`(known 차이도 변경 · 스타일 변경 감지 · 그리기 전 `visible_range` None).
- 실기(사용자): ① SQL 탭에서 Ctrl 누른 채 마우스를 이름 위로 → **그 링크만** 1 px 밑줄(기본 hover) · `objlink.display=all`이면 Ctrl 순간 전부 ② 메타에 있는 테이블 = 강조색 실선 · 없는 이름(예: 다른 DB의 테이블) = **벽돌색 물결** + 툴팁 "(현재 연결에 없는 객체)" ③ 링크 위 = 우상단 툴팁(코멘트) ④ 좌클릭 → 상태줄 "복사됨" · 붙여넣기 = 설명 ⑤ 우클릭 → "설명 복사" ⑥ Ctrl 뗌 → 밑줄 사라짐 ⑦ `perf.boost` 켬 → 밑줄 없음(설정 창 display 잠김)이지만 Ctrl+hover 툴팁·클릭은 동작 ⑧ 큰 파일(L1) 탭 → 같음(보이는 부분만 분석) ⑨ 정상/미확인 색·두께·모양 5종 바꿔 보기 ⑩ 툴팁 위치 4종 · 창 모서리 근처에서 잘리지 않음.

## 6. 세션 권한 · 서버별 메타(사용자 09-30 · 설계 원칙)

**원칙** — 메타데이터는 **서버별 1벌**(같은 서버의 연결 N개가 한 저장소를 쓴다 · 수집·접근은 접속한 계정으로) · 그러나 **보이는가는
세션 계정 기준**: 현재 세션에 권한이 없는 행위·설명·객체 접근이 **허용되는 것처럼 보여서는 안 된다**.

| 상황 | 결과 |
|---|---|
| BISCM 세션에서 `trx_demand`(BISCM에 없음 · SQLEDU에 있음) | **미확인**(벽돌색 · "현재 연결에 없는 객체") — 스키마 없는 이름은 세션의 현재 스키마 + PUBLIC에서만 |
| SQLEDU 세션에서 `trx_demand` | 정상 + 설명 |
| BISCM 세션에서 `SQLEDU.trx_demand` | BISCM 계정이 그 메타를 수집했다면(= 권한이 있어 보였다) 정상 + 설명 · 다른 계정이 수집한 것이면 미확인 |
| DB1·DB2 동시 접속 · DB2가 DB1 스키마를 수집 · DB1 세션이 DB2 스키마를 봄 | 메타에는 있지만 DB1 세션에는 **없는 객체** |

**규칙(`schema_visible` · 순수)** = 객체 스키마가 ① 세션의 현재 스키마(서버 답 `current_schema` → 접속 `?schema=` → 사용자) ② `PUBLIC`
③ 그 서버 메타를 **세션 계정이 수집**했을 때(`ExplorerSet::meta_account` = 칸의 메타 세션 사용자 · 사전 뷰 `ALL_*`·`has_table_privilege`가 이미
그 계정의 권한으로 걸러 준다) 중 하나. 판정(`scan`) · 설명(`objlink_describe`) · 선적재(`objlink_prefetch`)가 `objlink_resolve` 한 길.
현재 스키마 = `RunEvent::Connected.schema`(`Sess.cur_schema`) — 사용자 이름 ≠ 기본 스키마인 계정(로그온 트리거)도 맞는다. **현재 스키마를 모르면 스키마 생략 이름은 없음**(전 스키마 폴백 금지 · 09-30 "엄격하게").

**DBMS별** — Oracle: 스키마 = 사용자 · `SYS_CONTEXT('USERENV','CURRENT_SCHEMA')` · PUBLIC 시노님 · 다른 스키마 객체는 GRANT/롤이 있어야(수집 계정
기준으로 근사). SQL Server: 스키마 = `SCHEMA_NAME()`(기본 dbo) · DB 간은 `db.schema.obj`(지금은 같은 DB 안 스키마만). PostgreSQL: `current_schema()` +
`search_path`(지금은 첫 스키마만 · 후속) · `public`. MySQL: 스키마 = 데이터베이스. SQLite: `main`.

**칸 선택** — `explorer.share_catalog`가 꺼져 있으면(기본 · 사용자 "연결별 칸") 계정마다 탐색기 칸·메타가 따로다. 그 연결의 트리를 아직 안 펼쳐 메타가 비었으면 링크·완성·상세는 **같은 카탈로그에서 메타를 가진 칸**을 본다(`ExplorerSet::meta_pane` · 수집 계정 = 그 칸의 계정 · 09-30 진단 = SQLEDU 칸 `schemas=0`이 원인).

**한계·후속(T-264)** — 한 서버 칸의 메타 세션은 **첫 연결 계정**으로 수집한다. 둘째 계정(예 SQLEDU)의 자기 스키마는 규칙 ①로 보이지만, 둘째 계정이
권한을 가진 **다른** 스키마는 수집 계정이 달라 미확인으로 남는다(보수적 = 허용처럼 보이지 않음). 연결별 수집 계정 태그(버킷마다 `collected_by`) +
세션 계정으로 재수집이 다음 단계.

## 7. 우클릭 ▸ 객체 탐색기에서 보기(10-01 ㉗)

| 조각 | 자리 | 요지 |
|---|---|---|
| 메뉴 | `objlink_click`(우) | 설명 복사 · 이름 - 설명 복사 · ─ · **객체 탐색기에서 보기** = `objlink_reveal_target(k).is_some()`일 때만 활성(미확인 = 흐림) |
| 대상 | `objlink_reveal_target` → `explorer::RevealTarget{db, schema, kind, name, member}` | 판정과 같은 해석(`objlink_resolve`) · 컬럼 = 테이블 + 멤버 · `pkg.proc` = 패키지 + 멤버 · SQL Server `DB.스키마` 열쇠 → db |
| 찾기 | `Explorer::reveal` → `step_reveal`(단계 기계 · `drain` 끝마다) | 루트가 아직 안 읽혔으면 루트부터(㉗-c) · 앵커(스키마/SSMS DB) → 종류 폴더(묶음 로컬 펼침) → 객체 → 멤버 하위 폴더(Columns/Procedures/Functions) → 선택(조상 펼침 · 보이게) · 못 찾음 = 상태줄 |
| 세트 | `ExplorerSet::reveal`(칸 전환) · `after_reveal`(**어느 칸이든** `reveal_done` 소비 → 선택 행을 보이는 영역 1/3 지점에 · `drain`의 앵커 보정 **뒤**) | 탐색기 숨김이면 `view.explorer` 켬 · 포커스 = 탐색기 · 같은 서버 다른 계정 = 다른 칸(㉗-b) |
| 시험 | `objlink.reveal:<이름>` · `explorer.selpath:<파일>` · `scripts/win-objlink-reveal-e2e.sh` 4 | 명령은 `primary`(Ctrl)를 잠시 켜 분석 |

## 8. 시스템 객체(10-01 ㉙)

- 메타(탐색기 스냅숏)에 없는 이름은 `builtin_class(dialect, schema, name)`(순수)로 한 번 더 — 시스템 패키지(`builtins::package`) = Package · 사전 객체(`builtins::system_objects`) = Relation · 접두는 `SYS`/`PUBLIC`/`SYSTEM`만. `SnapResolver::object_class`의 폴백.
- 툴팁: 시스템 패키지 멤버 = `builtins::signature`(예 `DBMS_XPLAN.DISPLAY_CURSOR(sql_id => NULL, …)`) · 사전 객체 = "(설명 없음)". "탐색기에서 보기"는 흐림(탐색기에 없다).
- 표에 없는 SYS 패키지는 아직 링크가 아니다(후속 = 접속 뒤 유휴에 SYS 패키지 이름을 L1에 올리기).
- 시험: `objlink.dump:<파일>` · E2E ⑤.

## 5. 후속(T-257)

- 2차(09-29 · 사용자)에서 반영됨: 표시 방식 3택 · 정상/미확인 스타일 · 향상·큰 파일 모드에서 동작 유지(표시 none) · 부분 분석(보이는 줄만 — 아래 목록의 "보이는 줄만 스캔"은 큰 파일·상한 초과에 한해 끝남).

- 좌클릭 동작 교체(사용자 예고 · 예 = `obj.info` 객체 정보 탭 · 77 §1-3) · 메뉴에 `obj.copy_name/qualified`·`obj.data` 추가(✅ `obj.reveal` = §7 10-01) · 컬럼 타입·NULL·기본값을 툴팁 둘째 줄에 · 프로시저 시그니처 · 문장이 큰 파일일 때 **보이는 줄만** 스캔 · 향상 모드 밖에서도 `intel_unsuitable`과 같은 판정 공유.
