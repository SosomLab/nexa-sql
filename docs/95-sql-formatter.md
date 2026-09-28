# 95. SQL 포맷터 — 내장 Basic + 확장 "SQL Formatter for kiros33" (2026-09-28~29 · 사용자 요구)

> 사용자 09-28: "sql formatter 기능을 extension으로 · 이름 = SQL Formatter for kiros33 · 로컬 `ansi-sql-format` 스킬을 그대로 포맷 형식으로 ·
> 구문별 · 공백/탭 · 한 줄/다중 줄처럼 설정 가능한 것은 확장 설정으로 · **기본 포맷 형식은 가장 단순한 Basic formatter를 내장** · 포맷 단축키 =
> 기본 포맷 · 확장 포맷터를 **Default Formatter**로 지정 · 공통 특성은 Basic을 준용 · `WHERE 1=1` · 콤마 위치 · 콤마 뒤 공백/탭 ·
> 모든 열에 별칭 · 다른 포맷터 조사 반영 · Basic이 못 다루는 속성은 확장 설정 · **미리보기**(Basic 기본 기능 + SDK API)".

## 0. 결론 한 장

| 층 | 무엇 | 위치 |
|---|---|---|
| 핵심 | 렉서 · 레이아웃 IR(`Line`/`Part`/`Role`) · **공통 옵션** `Options` · Basic 렌더 · 정렬 보조 | `crates/nsql-format`(의존 0) |
| 앱 | 명령 `edit.format`(**Shift+Alt+F** = 기본 포맷터) · `edit.format_with`(팔레트: 한 번 쓰기 / 기본 지정 / 미리보기) · `edit.format_preview` · 미리보기 탭 · 설정 `format.*` | `app/format.rs` · `keymap.rs` · `menus.rs` · `nsql-settings` |
| ABI | v1.1(추가분): 메타 `formatter{label,sample}` + export `nx_ext_format` | `extensions/wasm.rs` · `extensions/mod.rs` · SDK `lib.rs` |
| 확장 | **SQL Formatter for kiros33** = 스킬 규칙(탭 정렬 · 같은 열 AND · 집합 구분행 · strict) · 설정 `sqlfmt.*` | `extensions/sdk/samples/sql-formatter-kiros33` → `extensions/sql-formatter-kiros33/` |

포맷 범위 = 선택이 있으면 선택만, 없으면 문서 전체(되돌리기 1단계). 원본을 바꾸지 않는 **미리보기 탭**은 `format.*`/`sqlfmt.*`가 바뀌면 다시
그린다(닫으면 다시 열지 않음 · 문서가 비면 엔진의 예시 SQL).

## 1. 다른 포맷터 조사(사용자 "상세 조사해서 반영")

| 도구 | 조정 가능한 것(요약) | 반영 |
|---|---|---|
| sql-formatter(JS) | tabWidth/useTabs · keywordCase/dataTypeCase/functionCase/identifierCase · indentStyle(standard/tabularLeft/tabularRight) · logicalOperatorNewline(before/after) · expressionWidth · linesBetweenQueries · denseOperators · newlineBeforeSemicolon | `format.indent/indent_width` · `*_case` 셋 · `logical_newline` · `line_width` · `stmt_blank_lines` · `operator_spaces` · `semicolon_newline` · tabular(강 정렬)은 후속 |
| pgFormatter | comma(start/end) · keyword/function case · spaces · wrap-limit · wrap-after · no-space-function · keep-newline · comment 유지 | `comma` · `*_case` · `line_width`/`list_style` · 주석 = 늘 유지 |
| SQLFluff | indent_unit/tab_space_size · max_line_length · capitalisation(keywords/identifiers/functions) · comma leading/trailing · operator newline · select targets one per line · aliasing explicit AS(열/테이블) · indented_joins/indented_on_contents | `indent*` · `line_width` · `*_case` · `comma` · `logical_newline` · `list_style` · `column_as`/`table_as` · `join_indent` |
| Poor Man's T-SQL Formatter | indent string · max line width · expand comma lists · trailing commas · expand boolean expressions · expand CASE · expand IN lists · break JOIN on sections · uppercase keywords | `list_style` · `comma` · `case_inline_max` · JOIN/ON 줄 |
| DBeaver | keyword case · indent size/type · line feed before clauses | 절 단독 줄(Basic 기본) |
| Oracle SQL Developer | keyword/identifier case · line breaks(콤마 앞/뒤 · AND/OR 앞/뒤) · commas per line · right-align keywords · align columns/aliases/equals · one-liner 유지 | `comma` · `logical_newline` · **정렬은 kiros33 확장** · `keep_oneliners` |
| Redgate SQL Prompt · Toad | alias/data type alignment · comma style · casing · wrapping · parentheses expanded/collapsed · "river" 정렬 | 정렬(확장) · 괄호 = 서브쿼리만 펼침 · river = 후속(T-255) |

공통으로 뽑은 옵션 = §2 표. 강(river) 정렬 · 괄호 AND/OR 그룹 시드 `(1=1` · 별칭 자동 부여 · 테이블 설명 주석 · PL/SQL 블록 · 방언 치환은 v1 밖(T-255).

## 2. 공통 옵션(`format.*` · Basic·확장 공용 · nsql-format `Options::from_pairs`)

| 키 | 값 | 기본 | 뜻 |
|---|---|---|---|
| `format.default` | `basic` \| 확장 id | basic | Shift+Alt+F가 쓰는 포맷터 |
| `format.indent` · `format.indent_width` | tab/space · 1~16 | tab · 4 | 들여쓰기 단위 · 폭(정렬 탭 스톱) |
| `format.keyword_case` · `identifier_case` · `function_case` | keep/upper/lower | upper · keep · keep | 대소문자(인용 식별자는 불변) |
| `format.comma` · `format.comma_gap` | leading/trailing · space/tab | leading · space | 콤마 위치 · 콤마 뒤 간격(정렬된 AS·연산자 뒤 포함) |
| `format.logical_newline` | before/after | before | AND/OR 줄 앞/뒤 |
| `format.where_seed` | bool | off | `WHERE/ON/HAVING 1=1` 시드(이미 있으면 그대로) |
| `format.list_style` · `format.line_width` | multi/single/auto · 40~400 | multi · 120 | 목록 배치 · 줄 폭 |
| `format.case_inline_max` | 20~400 | 120 | CASE 한 줄 상한 |
| `format.operator_spaces` | bool | on | 연산자 양쪽 공백 |
| `format.column_alias_all` · `column_as` · `table_as` | bool · keep/add/remove | off · keep · keep | 모든 열 별칭 · 명시적 AS |
| `format.join_indent` | bool | off | JOIN 줄 한 단계 안 |
| `format.keep_oneliners` | bool | on | 한 줄 짧은 DML 유지(§19 · SELECT는 늘 포맷) |
| `format.stmt_blank_lines` · `max_blank_lines` | 0~5 · 0~10 | 1 · 2 | 문장 사이 빈 줄 · 연속 빈 줄 상한 |
| `format.newline` · `final_newline` · `semicolon_newline` | keep/lf/crlf · bool · bool | keep · on · off | 개행 · 끝 개행 · `;` 줄 |

## 3. Basic formatter(레이아웃 규칙)

- 문장 = 최상위 `;`로 분리 · `DECLARE/BEGIN/CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE`부터는 끝까지 **원문 통과**(안의 `;`로 쪼개지 않음) ·
  SELECT/WITH/INSERT/UPDATE/DELETE/MERGE 외(DDL 등)도 원문 통과.
- SELECT/FROM/GROUP BY/ORDER BY = 절 단독 줄 + 항목마다 한 단계 안(콤마 위치 옵션) · JOIN = FROM 열(옵션으로 한 단계 안) · `ON` = 첫 조건을 같은 줄에,
  나머지 AND/OR는 ON 열 · WHERE/HAVING = 절 단독 줄 + 조건 한 단계 안(시드가 있으면 `WHERE 1=1` + 모든 조건 AND/OR 줄) · 괄호 안 AND/OR·BETWEEN … AND는 나누지 않음.
- 서브쿼리 `( SELECT … )` = `(` 뒤 줄바꿈 · 한 단계 안 · `)`는 여는 줄의 열 + 뒤 별칭 · 함수·IN 목록 괄호 = 인라인.
- CASE = `case_inline_max` 안이면 한 줄, 넘으면 `CASE / WHEN … THEN … / ELSE / END`(END 뒤 별칭 이어짐).
- INSERT(컬럼 목록·VALUES 행 콤마 목록) · UPDATE(`UPDATE 대상 별칭 SET` 한 줄 + 대입 목록 · `=`는 정렬 조각) · DELETE · MERGE(USING/ON(1=1)/WHEN … THEN/UPDATE SET/INSERT … VALUES) · WITH(CTE 목록 · 각 본문 서브쿼리) · 집합 연산자 = 같은 열.
- 주석: 같은 줄 `-- …`는 줄 끝 조각 · 같은 줄 `/* */`·힌트는 글의 일부 · 독립 주석은 독립 줄(빈 줄 유지).
- **불변식 = 토큰 보존**(공백만 바꾼다 · 시드/AS 추가/대소문자는 명시적 옵션) · 멱등(다시 포맷해도 같다) — 시험 `assert_tokens_kept`.

## 4. 확장 SQL Formatter for kiros33(`sqlfmt.*`)

| 키 | 기본 | 뜻 |
|---|---|---|
| `sqlfmt.strict` | on | 스킬 규정값 강제(탭 · 폭 4 · 콤마 앞 · `,\t` · `1=1` · 대문자 · AND 앞 · CASE 120) — 끄면 `format.*` 그대로 |
| `sqlfmt.align_as` · `align_ops` · `align_order` | on | AS · 비교 연산자 · ORDER BY 방향의 **탭 수직 정렬**(블록 = 같은 들여쓰기·같은 역할의 연속 줄 · 가장 긴 항목 다음 탭 스톱) |
| `sqlfmt.outlier_chars` | 48 | 이보다 긴 항목은 정렬 제외 + 탭 1개(스킬 "아웃라이어") |
| `sqlfmt.and_same_level` | on | WHERE/HAVING의 AND/OR = 절 키워드와 같은 열(§3) · `AND\t조건` |
| `sqlfmt.set_op_dashes` | on | 집합 연산자 앞뒤 대시 구분행(§12 · 키워드 글자 수) |

구현 = `nsql_format::layout::layout` → (같은 열 AND · 구분행) → `AlignPlan`(블록별 탭 스톱 · `line_prefix`/`display_width`/`tabs_to`) → `render_lines(pad)`.

## 5. ABI v1.1(포맷터) · SDK API

- 메타: `"formatter": {"label": {"en","ko"}, "sample": "…"}` — 있으면 호스트가 포맷터로 등록(팔레트 · 기본 지정 · 미리보기 예시).
- export `nx_ext_format(ptr) -> ptr`: 입력 `{"text","dialect","options":{"format.k":"v"…},"settings":{"sqlfmt.k":"v"…},"preview":bool}` → 출력 `{"text":"…"}` 또는 `{"error":"…"}`.
- SDK: `Meta.formatter: Option<Formatter>` · `trait Extension::format(&FormatRequest) -> Result<String,String>`(기본 = 없음) · `FormatRequest::from_json` · `format_response` · 확장은 `nsql-format`(path 의존)으로 공통 옵션을 `Options::from_pairs`로 읽는다.
- 호스트 상한: 포맷 호출 연료 2e9 · 5초 · 입력·반환 1 MB(넘으면 오류 · Basic으로 안내) · 실패 3회 = 세션 동안 정지(기존 브레이커).

## 6. 시험 방법

- 단위: `cargo test -p nsql-format`(15 · 토큰 보존·멱등·옵션) · `cargo test --manifest-path extensions/sdk/Cargo.toml -p sql-formatter-kiros33 --target <host>`(정렬·구분행·비strict).
- 실기: ① 편집기에 한 줄 SELECT → Shift+Alt+F → Basic 결과 · Ctrl+Z 되돌리기 ② 팔레트 "포맷터 골라 포맷…" ▸ kiros33으로 포맷 · 기본으로 지정 → Shift+Alt+F가 kiros33 ③ "포맷 미리보기" 탭 → 설정 창에서 `format.comma`·`sqlfmt.strict`를 바꾸면 탭이 즉시 갱신 ④ 확장 관리자에서 kiros33을 끄면 기본 포맷터가 Basic으로 되돌아감.

## 7. 후속(T-255)

강(river) 정렬 · 괄호 AND/OR 그룹 `(1=1`/`(1=0` 시드 · 별칭 자동 부여(2-1) · 테이블 설명 주석(2-2 · 메타 필요) · PL/SQL 블록(§21) · 방언 치환(§22) · 결과 그리드 "Copy SQL"과 포맷 연동 · CLI `nsql format`.
