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
| 확장 | **SQL Formatter for kiros33** = 스킬 규칙(탭 정렬 · 같은 열 AND · 집합 구분행 · strict) · 설정 `ext.sqlfmt_kiros33.*` | `extensions/sdk/samples/sql-formatter-kiros33` → `extensions/sql-formatter-kiros33/` |

포맷 범위 = 선택이 있으면 선택만, 없으면 문서 전체(되돌리기 1단계). 원본을 바꾸지 않는 **미리보기 탭**은 `format.*`/`ext.sqlfmt_kiros33.*`가 바뀌면 다시
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
| `format.indent` · `format.indent_width` | editor/tab/space · editor/2/3/4/6/8 | **editor · editor** | 들여쓰기 단위 · 폭(정렬 탭 스톱) — **활성 탭 설정**(`editor`) = 포맷하는 탭의 상태줄 탭 크기/공백(사용자 09-29 · 옛 `format.indent_from_tab`은 폐기 · 꺼져 있던 사용자는 이주로 `tab`·`4` 명시) · 미리보기 = 실제 적용값 · CLI(`nsql format`)는 탭이 없어 `editor` = 탭·4 |
| `format.keyword_case` · `identifier_case` · `function_case` | keep/upper/lower | upper · keep · keep | 대소문자(인용 식별자는 불변) |
| `format.comma` · `format.comma_gap` | leading/trailing · space/tab | **trailing** · space | 콤마 위치 · 콤마 뒤 간격(정렬된 AS·연산자 뒤 포함) — Basic 기본 = DBeaver 일반형(콤마 뒤 · 사용자 09-29 "내 기준은 kiros33에서") · `Options::default()`(라이브러리)는 leading 그대로 |
| `format.logical_newline` | before/after | before | AND/OR 줄 앞/뒤 |
| `format.logical_gap` | space/tab | space | 줄 앞 AND/OR와 조건 사이(공백/탭 · 줄 앞 배치일 때만 · 사용자 09-29) |
| `format.where_seed` | bool | off | `WHERE/ON/HAVING 1=1` 시드(이미 있으면 그대로) |
| `format.seed_gap` | space/tab | space | 시드 앞 구분 — `WHERE 1=1` / `WHERE\t1=1`(`ON`·`HAVING`도) · 사용자 09-29 |
| `format.cond_indent` | indent/same | indent | WHERE/HAVING 뒤 AND/OR 줄 = 한 단계 안 / WHERE와 같은 열(ON은 늘 ON 열) · 사용자 09-29 |
| `format.list_style` · `format.line_width` | multi/single/auto · 40~400 | multi · 120 | 목록 배치 · 줄 폭 |
| `format.case_inline_max` | 20~400 | 120 | CASE 한 줄 상한 |
| `format.operator_spaces` | bool | on | 연산자 양쪽 공백 |
| `format.operator_gap` | space/tab | space | 그 공백의 글자(공백/탭 · 위가 켜졌을 때만 · 사용자 09-29) · 단어 연산자 `IN`·`IS`·`LIKE`·`NOT IN`·`NOT LIKE`·`IS NOT`도 연산자 조각 |
| (연산자 부류) | — | — | **단항 후위** `IS [NOT] NULL/TRUE/FALSE/UNKNOWN` = 왼쪽 간격·정렬만(오른쪽 없음) · **이항** `=` `<>` `!=` `<` `>` `<=` `>=` `<=>` `^=` `~=` `IN` `IS` `LIKE` `ILIKE` `REGEXP` `RLIKE` `SIMILAR TO` `IS [NOT] DISTINCT FROM` + `NOT …` = 좌우 간격(4자 이상 오른쪽 공백 1개) · **BETWEEN**/`NOT BETWEEN` = 왼쪽 간격 + 오른쪽 공백 1개(`x AND y`의 AND는 공백 하나 · 조건 연산어 아님) · **연산자 없는 조건**(`REGEXP_LIKE(...)` 같은 함수 호출 · `[NOT] EXISTS (…)` · 불리언 컬럼 · `NOT 컬럼`) = 조각 없음 → 정렬·간격 대상 아님 — 표 `layout::WORD_OPS`(긴 구절 우선) · `op_kind`(사용자 09-30) |
| `format.operator_long_space` | bool | on | 4자 이상 연산자(`LIKE` · `NOT IN` · `IS NOT` …)는 왼쪽 = 간격 설정 · **오른쪽만 공백 1개**(사용자 09-29 정정) |
| `format.window_break` | 0~400 | 0 | 윈도우 `OVER (…)` 안이 이 글자 수를 넘으면 `OVER (` 줄바꿈 + PARTITION BY / ORDER BY / 프레임 각 줄(한 단계 안) + `)` 줄 · 0 = 늘 한 줄(사용자 09-30 · 스킬 §8) |
| `format.comment_space` · `format.comment_gap` | bool · editor/space/tab | on · **editor** | 인라인(줄 끝) 주석 앞 간격 켬/끔 · 글자(활성 탭 단위 = 기본 · 공백 두 칸 · 탭 · 사용자 09-30) |
| `format.as_gap` | space/tab | space | `AS` 앞뒤 간격(열·테이블 별칭 · 정렬 채움이 있으면 뒤만 · 사용자 09-29) |
| `format.column_alias_all` · `column_as` · `table_as` | bool · keep/add/remove | off · keep · keep | 모든 열 별칭 · 명시적 AS |
| `format.join_indent` | bool | off | JOIN 줄 한 단계 안 |
| `format.keep_oneliners` | bool | on | 한 줄 짧은 DML 유지(§19 · SELECT는 늘 포맷) |
| `format.stmt_blank_lines` · `max_blank_lines` | 0~5 · 0~10 | 1 · 2 | 문장 사이 빈 줄 · 연속 빈 줄 상한 |
| `format.newline` · `final_newline` · `semicolon_newline` | keep/lf/crlf · bool · bool | keep · on · off | 개행 · 끝 개행 · `;` 줄 |

## 3. Basic formatter(레이아웃 규칙)

- 문장 = 최상위 `;`로 분리 · `DECLARE/BEGIN/CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE`부터는 **블록 끝까지 원문 통과**(안의 `;`로 쪼개지 않음 ·
  끝 = 단독 `/`·`GO` 줄 앞 · 없으면 `BEGIN`/`CASE`…`END` 짝이 닫힌 뒤 다음 최상위 `;` · `BEGIN;`/`BEGIN TRANSACTION`은 블록 아님 · 10-03 = 종전 "문서 끝까지"에서 바꿈 ·
  [journal 10-03 §4](journal/2026-10-03.md)) ·
  SELECT/WITH/INSERT/UPDATE/DELETE/MERGE 외(DDL 등)도 원문 통과.
- ★ **§3-1 스크립트 명령 · `EXEC`**(사용자 10-03 · [journal 10-03 §2](journal/2026-10-03.md) · `layout::exec_statement`): 문장 끝을 **실행기(nsql-script `split`)와 같은 규칙**으로
  정한다 — 스크립트 명령 줄(`PRINT` · `SET` · `VARIABLE` · `:setvar` · `@파일` · 단독 `/`·`GO` · 목록 = `COMMAND_WORDS` = nsql-script `is_command_start`의 사본)은
  **그 줄에서 끝**(원문 그대로 · 앞뒤 빈 줄 수도 원문) · 단독 `/`·`GO` 줄은 SQL 문장의 끝.
  `EXEC`/`EXECUTE`: 홀로 선 `EXEC` = 다음 줄부터 `;` · 빈 줄 · `/` · 다음 명령 줄까지(블록) · 한 줄 `EXEC …` = 그 줄(열린 괄호 · 콤마 · `@a = 1` 인자는 이어짐).
  ① 본문 `SELECT`/`WITH`(= `SELECT … INTO`) → `EXEC` 한 줄 + 평소 포맷(한 줄 꼴도 블록 꼴로 · `;` 없이 다음 줄이 SQL이면 `;`를 붙인다 · 자동 별칭 없음)
  ② 본문이 프로시저·패키지 호출 → **한 줄**(줄 바꿈만 없앤다 · 이미 한 줄 = 간격 원문 그대로) ③ 그 밖(대입 `:v := …` · `OPEN … FOR` · 주석 낀 호출) → 원문.
  치환 변수 `&v`·`&&v`·`&1` · `${이름[:형식]}` · 접두 문자열 `N'…'`·`E'…'`·Oracle `q'[…]'`는 한 토큰(대소문자·띄어쓰기 손대지 않음) ·
  단항 부호(`-1`)와 PG 캐스트(`a::text`)는 붙여 쓴다(10-03).
- SELECT/FROM/GROUP BY/ORDER BY = 절 단독 줄 + 항목마다 한 단계 안(콤마 위치 옵션) · JOIN = FROM 열(옵션으로 한 단계 안) · `ON` = 첫 조건을 같은 줄에,
  나머지 AND/OR는 ON 열 · WHERE/HAVING = 절 단독 줄 + 조건 한 단계 안(시드가 있으면 `WHERE 1=1` + 모든 조건 AND/OR 줄) · 괄호 안 AND/OR·BETWEEN … AND는 나누지 않음.
- 서브쿼리 `( SELECT … )` = `(` 뒤 줄바꿈 · 한 단계 안 · `)`는 여는 줄의 열 + 뒤 별칭 · 함수·IN 목록 괄호 = 인라인.
- CASE = `case_inline_max` 안이면 한 줄, 넘으면 `CASE / WHEN … THEN … / ELSE / END`(END 뒤 별칭 이어짐).
- INSERT(컬럼 목록·VALUES 행 콤마 목록) · UPDATE(`UPDATE 대상 별칭 SET` 한 줄 + 대입 목록 · `=`는 정렬 조각) · DELETE · MERGE(USING/ON(1=1)/WHEN … THEN/UPDATE SET/INSERT … VALUES) · WITH(CTE 목록 · 각 본문 서브쿼리) · 집합 연산자 = 같은 열.
- 주석: 같은 줄 `-- …`는 줄 끝 조각 · 같은 줄 `/* */`·힌트는 글의 일부 · 독립 주석은 독립 줄(빈 줄 유지).
  - ★ **목록 안 독립 주석(10-27 · 142차 win · T-440)**: 항목 **뒤** 주석(같은 줄 `-- note`)은 별칭·AS 처리 **뒤**의 줄 끝 조각(주석이 항목과 `AS` 사이에 끼지 않는다 · `expr_item` `item_ends_at`/`at_trailing_comment` → `drain_comments`) · 다음 줄 독립 주석은 **항목 들여쓰기의 독립 줄** · 항목 **앞** 독립 주석은 아직 글이 없는 항목 줄(비었거나 줄 앞 콤마만)을 주석 뒤로 미룬다(`comment()` `pending` · 다음 항목 들여쓰기 유지) · 꼬리 콤마 모드 = 사이 주석 줄을 건너뛰고 항목 줄 끝에 `,`(`render_lines`) · 앞/꼬리 콤마 · 탭 간격 · 첫 항목 앞 주석 꼴이 모두 같은 결과 · 멱등(종전 = 콤마 뒤 줄바꿈 + 주석 줄이면 다시 포맷할 때 들여쓰기 소실 · 1차 수정 중에는 빈 줄 누적).
- **불변식 = 토큰 보존**(공백만 바꾼다 · 시드/AS 추가/대소문자는 명시적 옵션) · 멱등(다시 포맷해도 같다) — 시험 `assert_tokens_kept`.

## 4. 확장 SQL Formatter for kiros33(`ext.sqlfmt_kiros33.*`)

| 키 | 기본 | 뜻 |
|---|---|---|
| `ext.sqlfmt_kiros33.strict` | on | 스킬 규정값 강제(탭 · 폭 4 · 콤마 앞 · `,\t` · `1=1` · 대문자 · AND 앞 · CASE 120 · **`;` 다음 줄 1칸**(사용자 09-30)) — 끄면 `format.*` 그대로 |
| `ext.sqlfmt_kiros33.align_as` · `align_ops` · `align_order` | on | AS · 비교 연산자 · ORDER BY 방향의 **탭 수직 정렬**(블록 = 같은 들여쓰기·같은 역할의 연속 줄 · 가장 긴 항목 다음 탭 스톱) |
| `ext.sqlfmt_kiros33.outlier_chars` | **40** | 이 길이(들여쓰기·앞 공백을 뺀 실제 글자부터)까지의 항목만 정렬 기준(가장 긴 항목 → 다음 탭 스톱 · 별칭/방향 없는 항목도 기준에 든다) · 더 긴 항목은 정렬 제외 + AS/연산자 **앞뒤 탭 1개씩**(공백 없음 · 사용자 09-30 재정정 · 스킬 "아웃라이어") |
| (미리보기 예제) | — | 기본 샘플 + **확장 설정별 확인 문장** `KIROS33_SAMPLE`(사용자 09-30 · align_as/force_as/outlier · align_ops/and_same_level/strict · set_op_dashes(UNION ALL) · align_order) — 켜고 끄면 그 자리가 바뀐다 · 설정 카드의 "▶ 미리보기 n행" 조각은 **확장 메타 `formatter.marks`**(75 §3-1 · 호스트 표는 Basic만 · 09-30) |
| `ext.sqlfmt_kiros33.force_as` | on | 열 별칭에 `AS` 강제(`sum(a.qty) qty` → `sum(a.qty) AS qty` · Basic `column_as = add`) + 순수 컬럼도 `AS 컬럼명`(`a.plant_cd AS plant_cd` · Basic `column_alias_all` · 사용자 09-30) |
| `ext.sqlfmt_kiros33.window_break` | 40 | 스킬 §8 윈도우 배치: 짧은 `OVER(…)` 한 줄 · 넘으면 `OVER (` + 절마다 한 줄 + `)` 뒤 **탭 1개** AS(정렬 열과 무관) — Basic `window_break`에 강제(사용자 09-30) |
| `ext.sqlfmt_kiros33.dup_alias` | numbered | JOIN으로 별칭이 겹칠 때(사용자 09-30 3택): `numbered` = 컬럼명1, 컬럼명2 · `qualified` = 컬럼명_테이블별칭(없으면 순번) · `keep` = 그대로 — SELECT 목록 단위 후처리 `dedup_aliases` |
| `ext.sqlfmt_kiros33.and_same_level` | on | WHERE/HAVING의 AND/OR = 절 키워드와 같은 열(§3) · `AND\t조건` |
| `ext.sqlfmt_kiros33.set_op_dashes` | on | 집합 연산자 앞뒤 대시 구분행(§12 · 키워드 글자 수) |

버전 = **1.3.1**(09-30 · 1.3.0 = dup_alias · window_break · 아웃라이어 탭 · BETWEEN/IS NULL 정렬 · 방향 없는 항목 정렬 기준 · marks · **1.3.1 = 다중 행 항목(`OVER (…)`의 `)` 줄)도 AS 정렬 블록에 포함 `merged_blocks`**) — 규칙 = wasm이 바뀌는 커밋마다 세 곳 버전 올림(75 §3-1). 호스트에 남은 kiros33 전용 = 설정 키·라벨 정적 등록뿐(75 §9 ① 동적 등록 전까지).

구현(1.1.0 · 09-29) = **Basic 확장 API**(§5-1): `strict_options`(공통 옵션 위 덮어쓰기 · `cond_indent` = `and_same_level`) → `nsql_format::prepare` → 구분행 손질 → `AlignPlan`(블록별 탭 스톱 · `line_prefix`/`display_width`/`tabs_to`) → `nsql_format::render(pad)`. 별칭 자동 부여 · 테이블 설명 · 방언 치환 · 괄호 그룹 시드 · AND/OR 뒤 간격 · 연산자 간격은 전부 Basic 몫(확장 코드 0) — 1.0.0의 `AND\t` 후처리·`same_level_conditions`는 지웠다.

- **1.3.2**(10-03): 코드 변경 없이 Basic 코어 재빌드 — `EXEC` 문 · 명령 줄 · 블록 끝 · `${…}`·접두 문자열 토큰 · 단항 부호·`::`(§3-1 · [journal 10-03](journal/2026-10-03.md)).
- **1.3.4**(10-27 · 142차 win · T-440): SELECT 목록 안 주석 줄 — 정렬 블록(`merged_blocks`)이 주석 줄을 건너뛰고 투명하게 이어 붙임 · `AlignPlan::build`는 주석 줄을 기준 폭에서 제외(주석 뒤 항목도 같은 AS 열) + Basic 코어 §3 "목록 안 독립 주석" · wasm 191,079 B · 시험 `comment_lines_inside_select_list_keep_alignment`.

## 5. ABI v1.1(포맷터) · SDK API

- 메타: `"formatter": {"label": {"en","ko"}, "sample": "…"}` — 있으면 호스트가 포맷터로 등록(팔레트 · 기본 지정 · 미리보기 예시).
- export `nx_ext_format(ptr) -> ptr`: 입력 `{"text","dialect","options":{"format.k":"v"…},"settings":{"ext.sqlfmt_kiros33.k":"v"…},"preview":bool}` → 출력 `{"text":"…"}` 또는 `{"error":"…"}`.
- SDK: `Meta.formatter: Option<Formatter>` · `trait Extension::format(&FormatRequest) -> Result<String,String>`(기본 = 없음) · `FormatRequest::from_json` · `format_response` · 확장은 `nsql-format`(path 의존)으로 공통 옵션을 `Options::from_pairs`로 읽는다.
- 호스트 상한: 포맷 호출 연료 2e9 · 5초 · 입력·반환 1 MB(넘으면 오류 · Basic으로 안내) · 실패 3회 = 세션 동안 정지(기존 브레이커).

### 5-1. Basic 확장 API(사용자 09-29 "확장은 Basic 설정 위에 추가 기능을 얹는 형태")

| 단계 | 함수(`nsql_format`) | 하는 일 |
|---|---|---|
| ① 준비 | `prepare(src, &Options) -> Vec<Line>` | Basic 전처리 전부 = 방언 치환 · 레이아웃 · 시드(`1=1` · 괄호 그룹) · 별칭 · 테이블 설명 · 조건 줄 자리 · 간격(콤마·AND/OR·연산자) |
| ② 손질 | 확장 클로저 `FnOnce(&mut Vec<Line>, &Options)` | 줄 추가/역할/들여쓰기 바꾸기(예: 집합 연산자 구분행) |
| ③ 렌더 | `render(&lines, &Options, src, Option<Pad>) -> String` | Basic 렌더 + `pad(줄, 조각, 지금까지 글)` = 조각 앞 채움(탭 정렬) |
| 한 번에 | `format_with(src, &Options, tweak, pad)` | ①→②→③ · `format_basic` = `format_with(_, _, 없음, 없음)` |

- 공통 옵션은 `Options::from_pairs(req.options)`로 받고 확장은 **필요한 값만 덮어쓴다**(`..base.clone()`) — Basic에 옵션이 늘면 확장은 그대로 따라온다.
- 호스트 → 확장 부가 정보도 pair로: `table_comments`(줄마다 `이름<TAB>설명` · `table_comments_pair`) — 앱이 Basic과 같은 목록을 넘긴다.
- ★ **`Part::AsRaw(String)`(10-04 · 추가 변형)**: `format.keyword_case=keep`이고 원문이 `AS`가 아닐 때 별칭 AS가 원문 글자(`as`/`As`)를 든 채 나온다 · 렌더 = `Part::As`와 같은 간격·정렬 규칙 · upper/lower에서는 나오지 않는다(동작 변화 0) · `As(String)`으로 바꾸지 않은 것 = 확장 옛 소스·빌드 호환. **확장이 `Part::As`로 정렬 대상을 고르면 `Part::AsRaw(_)`도 함께 매치할 것**(kiros33 1.3.3 `AlignKind::As` · [journal 10-04 §4](journal/2026-10-04.md)).
- 정렬된 연산자 뒤 간격 = `operator_gap`(4자 이상 = 공백 1개) — 확장의 채움(`pad`)과 Basic 규칙이 한 벌.

## 6. 시험 방법

- 단위: `cargo test -p nsql-format`(15 · 토큰 보존·멱등·옵션) · `cargo test --manifest-path extensions/sdk/Cargo.toml -p sql-formatter-kiros33 --target <host>`(정렬·구분행·비strict).
- 실기: ① 편집기에 한 줄 SELECT → Shift+Alt+F → Basic 결과 · Ctrl+Z 되돌리기 ② 팔레트 "포맷터 골라 포맷…" ▸ kiros33으로 포맷 · 기본으로 지정 → Shift+Alt+F가 kiros33 ③ "포맷 미리보기" 탭 → 설정 창에서 `format.comma`·`ext.sqlfmt_kiros33.strict`를 바꾸면 탭이 즉시 갱신 ④ 확장 관리자에서 kiros33을 끄면 기본 포맷터가 Basic으로 되돌아감.

## 7. 후속(T-255)

✅ 09-29 1차(§9): 괄호 AND/OR 그룹 `(1=1`/`(1=0` 시드 · 별칭 자동 부여(2-1) · 테이블 설명 주석(2-2 · 메타) · 방언 치환(§22) · 결과 그리드 "Copy SQL" 포맷 연동 · CLI `nsql format`. **남음**: 강(river) 정렬(렌더 구조 변경 = 절 키워드 우측 정렬 + 첫 항목 같은 줄 · 별도 설계) · PL/SQL 블록(§21 · 통과 블록의 계단식 재들여쓰기 + 내부 단위 쿼리 포맷 · 별도 설계).

## 8. 우클릭 Format 그룹 · 포맷 범위 · 기본 포맷터 콤보(사용자 09-29)

- **우클릭 메뉴 Format 그룹**(SQL 구문 탭만 · `App::format_menu_items` · 우클릭 직전 `refresh_menu_extras`로 새로 만든다): `SQL Format (기본 포맷터)`가 첫 줄(단축키 표시) → 나머지 설치 포맷터가 `SQL Format (이름)`으로 → 구분자 → 대문자로 / 소문자로(**선택이 있을 때만 활성**). 이름 = `format_engine_short`(Basic = "Basic" · 확장 = 라벨에서 "SQL Formatter for " 접두 제거 → "kiros33"). 확장이 없으면 `SQL Format (Basic)` 한 줄 · kiros33 설치 + Basic 기본 = Basic, kiros33 순 · kiros33 기본 = kiros33, Basic 순.
- **포맷 범위**(`format_target` · 메뉴·Shift+Alt+F 공통): 선택이 있으면 선택만 → 없으면 **캐럿의 문장**(`nsql_script::statement_at_in` = "문장 실행"과 같은 범위) → 문장이 없으면 문서 전체. `replace_range`라 **되돌리기 1단계** · 문장/선택 포맷 뒤에는 바뀐 구간을 선택해 둔다.
- **`format.default` = 콤보**: 설정 창이 `Text` 항목에 호스트가 준 동적 후보(`PrefsWin::set_dyn_choices` · 확장 켜기/끄기 때 `apply_extensions`가 갱신)를 콤보로 그린다 · 후보 = `format_default_choices`(Basic · `라벨 (id)`).
- **들여쓰기 = 활성 탭의 값**(사용자 09-29): `format.indent`/`format.indent_width` 설정 항목은 형태를 유지하되 지금은 `Editors::indent()`(탭별 재정의 > `editor.tab_size`/`editor.indent_spaces`)를 넣는다(`format_option_pairs`). 탭별 재정의는 상태줄 팝업으로 바꾸며 새 탭은 설정 기본값으로 시작 · **파일 모드**(프로젝트 없음)는 저장할 곳이 없어 늘 설정값(본문은 안 바꾼다 · 일괄 변환은 기존 기능) · **폴더/프로젝트 모드**는 `TabState.indent`(`tab_size`·`spaces`)로 탭별 저장 → 다시 열 때 복원(`set_indent_of`).
- 🔧 설정 창 콤보 드롭다운이 **다음 카드의 콤보 상자 아래**로 그려지던 결함: 콤보 층에서 **열린 콤보를 맨 마지막**에 그린다(`prefs_win.rs` paint 두 번 지나기).

## 8-1. 설정 창 안 미리보기(사용자 09-29 "포맷을 미리보면서 수정 · 변경점 1곳")

- 미리보기 **토글**(복사 버튼 왼쪽 · 사용자 09-30): Basic 분류 = **B 하나**(Basic 설정 적용 · 끄면 원본) · 확장 포맷터 분류 = **B + 확장 이니셜**(예: kiros33 = `K` · `formatter_initial` = 이름 마지막 낱말 첫 글자 · B·기존 글자와 겹치지 않게 · 확장이 설치되어야 보인다) · 그때 미리보기 = 그 확장 · 둘 다 끔 = 원본 샘플 · 제목에 `· B` `· BK` `· K` `· 원본`.
- 카드 덧말 **"▶ 미리보기 n행"**(사용자 09-30 · 처음 드러나는 줄 하나): `PREVIEW_MARKS`(설정 키 → 조각 · 공백 제거·대문자 비교)로 포맷 결과에서 매번 다시 찾는다 — 옵션을 바꿔 줄이 밀려도 맞다 · Basic 30키 · kiros33 8키.
- 미리보기 본문은 **편집기와 같은 고정폭 글꼴·크기**(`mono_font` · `editor.font_size` · 별도 그리기 문맥)로 그린다 — 포맷터의 정렬은 칸 단위라 UI 글꼴(가변폭)로는 열이 어긋났다(사용자 09-30 · 3-OS 동일).
- 미리보기 **복사**(사용자 09-30): 제목 줄 오른쪽 복사 버튼 = 전체 글(카드 키 복사와 같은 `copybtn` 되먹임) · 상자 안 드래그 선택 + 우클릭 메뉴/Ctrl+C = 선택 복사 — Basic·확장 분류 공통(같은 상자).
- 미리보기 엔진 = **보고 있는 분류**: 확장 포맷터의 분류(예: SQL Formatter for kiros33)를 보면 그 확장으로, Format 분류면 기본 포맷터(`format.default`)로 · 분류 전환(`PrefsAction::Category`)·값 변경마다 다시 그림(사용자 09-29).

설정 창에서 **Format**(또는 확장 SQL Formatter) 분류를 고르면 카드 영역이 위 55 %로 줄고 아래에 **미리보기**(제목 = `미리보기 — 엔진` · 읽기 전용 SQL 상자 · 선택·복사·스크롤 가능)가 붙는다. 원본 = 열려 있는 미리보기 탭의 원본 > 기본 포맷터의 예시 SQL · 값(`format.*`/`ext.sqlfmt_kiros33.*`)을 바꾸는 순간 `App::prefs_format_preview_refresh`가 기본 포맷터로 다시 포맷해 넣는다(`PrefsWin::set_preview`). 다른 분류로 가면 카드 영역이 원래대로. 별도 편집기 탭 미리보기(`edit.format_preview`)는 그대로 있다(자기 문서로 보고 싶을 때).

## 9. T-255 1차(09-29) — 옵션 3 · 메타 주석 · Copy SQL · CLI

| 항목 | 구현 | 설정 |
|---|---|---|
| 괄호 AND/OR 그룹 시드(스킬 §3) | 조건 문맥의 `(` 뒤에 서브쿼리가 아니고 최상위 AND/OR가 있으면(`paren_is_cond_group` · BETWEEN AND·CASE 안 제외) `(1=1`(AND) / `(1=0`(OR · 첫 연산자 기준) + 조건 한 단계 더 + `)` 같은 열(`paren_group`) · 원문 시드 유지 · 멱등 | `format.paren_seed`(off) |
| 별칭 자동 부여(스킬 2-1) | FROM/JOIN(`in_from`)의 별칭 없는 테이블·인라인뷰에 `A`…`Z`, `AA`…(`alias_name`) · 문장 안 단어(`stmt_words`)와 안 겹치게 · 기존 별칭 유지 · UPDATE/MERGE 대상은 안 건드림 · 상관 서브쿼리 `S1`/`C1` 규칙과 컬럼 한정(`별칭.컬럼`)은 메타가 필요해 후속 | `format.auto_alias`(off) |
| 테이블 설명 주석(스킬 2-2) | `Options.table_comments`(대문자 `S.T`/`T` → 설명)를 호스트가 채움(`App::format_table_comments` = 문서의 이름을 메타 스냅숏 `lookup` · 객체 코멘트 > 상세 코멘트) → 물리 테이블 항목 줄 끝 `--\t설명`(`table_comment`) · 없으면 없음 · 확장 포맷터에는 안 감 | (메타 있으면 자동) |
| 방언 치환(스킬 §22) | 렉서 뒤 토큰 치환 `substitute_dialect`: ISNULL/NVL/COALESCE · SUBSTRING/SUBSTR · LEN/LENGTH/CHAR_LENGTH · GETDATE()/SYSDATE/CURRENT_TIMESTAMP · EXCEPT/MINUS · 인자 순서·구조가 다른 것(CHARINDEX/INSTR · TOP · #TEMP · `+` 결합)은 안 건드림 | `format.dialect_target`(none/ansi/oracle/tsql) |
| Copy SQL 포맷 | 결과 탭 메뉴 ▸ Copy SQL 때 기본 포맷터로 정돈(실패 = 원문) | `grid.copy_sql_format`(off) |
| CLI | `nsql format [<file|->] [-o <file>] [--in-place]` = 내장 Basic + 앱과 같은 `format.*` 설정(`settings_cached`) · 확장 포맷터는 CLI에 없음 | — |

시험: nsql-format `paren_group_seed` · `dialect_substitution` · `auto_alias_for_tables` · `table_description_comments`(+ 기존 15) · nsql-settings/i18n 등재.


## 10. 강(river) 정렬 설계 — `format.indent_style = standard|river`(10-30 · D-416 · T-255 · 설계 = 협업 · ✅ 10-31 1차 구현 · ✅ 10-31 2차 = river 완료)

> 사용자 결정(10-30): 확장이 아니라 **Basic 옵션**으로. 기본 = `standard`(지금 그대로). 아래 = 구현 전 설계 — 바꿀 때 이 절을 고친다.

> ★ **구현됨(10-31 1차 · 192차 19·20차 · 개발 · [journal 10-31 §5](journal/2026-10-31.md))** — `Options.indent_style: IndentStyle {Standard, River}`(pair `indent_style` · `OPTION_KEYS` 등재) · `Line.col: Option<usize>`(Some = 공백 열 접두 · 줄 앞 콤마 `, `) · `prepare()` = layout 뒤 **`river::apply`**(새 모듈 `river.rs` · IR 후처리 · Walker 무변경 = §10-2 그대로) · 세그먼트 = 깊이 0의 SELECT/WITH/UPDATE/DELETE(집합 연산자 뒤 새 강 · INSERT/MERGE·Raw 제외) · 깊이별 강 폭 = 절·JOIN·ON·AND/OR 머리 예약어 폭 최댓값 → 오른쪽 맞춤 · 절 키워드만 있는 줄 + 첫 항목 합침 · 항목 = 강+1(줄 앞 콤마 = 강 열) · 그 밖 줄 = 깊이×단위 공백 · 탭 단위여도 강은 공백. V1(CLI) = 강 폭이 `LEFT OUTER JOIN`(15)이면 `SELECT` 앞 9칸(설계대로). ~~**남음** = 서브쿼리 안쪽 강 세부 · CASE 상대 들여쓰기~~ · 확장 kiros33 정렬은 `col` 있는 줄을 건너뜀(문서화된 동작).
>
> ★ **2차 구현됨(10-31 · 192차 33차 · 개발 · [journal 10-31 §18](journal/2026-10-31.md))** — `river.rs` 열 프레임 `Frame{open_indent, inner0, close_col}` · 앞 줄이 `(`로 끝나고 이 줄이 더 깊으면 안쪽 블록 기준 열 = `(` 다음 열 → 안쪽 강 = 그 열 + 안쪽 키워드 폭 최댓값 · 닫는 `)` = `(` 열 · 여러 줄 CASE = `CASE` 낱말 열 기준 WHEN/ELSE +한 단 · END = CASE 열 · 중첩 · 깊이가 열림 이하로 돌아오면 닫힘(§10-1 "서브쿼리 = `(` 다음 열이 새 강의 기준 · CASE = 항목 열 기준 상대 들여쓰기" 그대로) · 시험 1 · V1(CLI · 콤마 앞 · `case_inline_max` 40) = `     , (` / `        SELECT max(b.d)`(8) / `       ) AS last_d`(7) / `           WHEN …`(11) / `       END grp`(7) / ` WHERE EXISTS (` / `               SELECT 1`(15) / `              )`(14) · 멱등 ✓ · standard 꼴 정상. **river 남음 없음.**
>
> ★ **결함 수정(10-31 · 192차 34차 · 협업 발견 → 개발 · [journal 10-31 §19](journal/2026-10-31.md))** — river + `format.cond_indent = same`이면 WHERE 첫 조건이 다음 줄 0열로 가던 결함(river.rs가 조건 줄 강 깊이를 "한 단계 위"로 가정) → `river_levels(seg)` 한 번 훑기(조건 줄 = 가장 가까운 앞 절의 강 깊이 · ON은 종전과 같음) + 첫 조건 잇기 = 같은 깊이도 허용 · V1 ✓(`   WHERE x = 1` · 서브쿼리·EXISTS 안쪽 · 멱등 · `indent` 불변).

### 10-1. 모양

```sql
SELECT a.id,                         -- 절 키워드를 강 열에 오른쪽 맞춤 + 첫 항목 같은 줄
       a.name
  FROM emp a
  JOIN dept d ON d.id = a.dept_id
 WHERE a.sal > 100
   AND d.loc = 'SEOUL'               -- AND/OR도 강에 오른쪽 맞춤
 GROUP BY a.dept_id
 ORDER BY a.name
```

- **강 열**(river) = 그 **질의 블록**(SELECT 하나 · 서브쿼리마다 따로)에 나온 절 키워드 폭의 최댓값(`SELECT` 6 · `ORDER BY`/`GROUP BY` 8 · `LEFT JOIN` 9 …). 키워드 = 강 열에서 끝나게 왼쪽 채움 · 항목 = 강 열 + 1.
- 항목 이어 줄(두 번째 열부터) = 강 열 + 1 · 줄 앞 콤마(`comma = leading`)는 콤마를 강 열에(`     , b`).
- `JOIN` 계열 = 절 키워드와 같이 오른쪽 맞춤 · `ON` = 지금처럼 같은 줄 · 이어지는 ON의 AND/OR = 강 맞춤.
- 서브쿼리 `( SELECT …` = `(` 다음 열이 **새 강의 기준 열**(안쪽 블록의 강은 그 열 + 안쪽 최댓값) · 닫는 `)` = 여는 줄의 `(` 열.
- 집합 연산자(`UNION ALL`) = 블록 기준 열에 단독 줄 · 위아래 블록은 각자 강.
- CASE(여러 줄) = 항목 시작 열 기준 상대 들여쓰기(지금의 CASE 규칙을 열 기준으로 옮김).
- 독립 주석 줄 = 강 열 + 1.
- 적용 문장: SELECT · WITH(각 CTE 본문) · UPDATE(`UPDATE t` / `   SET a = 1` / ` WHERE …`) · DELETE. **INSERT · MERGE = v1에서 standard 그대로**(VALUES 목록·WHEN 가지는 강의 이득이 작고 규칙이 커진다 · 요구 생기면 v2).
- 들여쓰기 단위가 탭이어도 강 정렬 줄은 **공백**으로 맞춘다(열 정렬이 목적 · 탭은 열이 보장되지 않는다) — 설정 설명에 적는다.

### 10-2. 구현 지점(현재 코드 · 협업 탐색)

| 단계 | 지점 | 변경 |
|---|---|---|
| 옵션 | `nsql-format/src/lib.rs` `Options`(:137) · `from_pairs`(:295) · `Default`(:203) | `pub indent_style: IndentStyle { Standard, River }` · 키 `indent_style` |
| IR | `layout.rs` `Line`(:58 · `indent`=단계 수) | **`pub col: Option<usize>` 한 칸 추가** — Some이면 줄 접두 = 공백 col개(단계·콤마 접두 무시). 기존 생성 지점은 `Line::new`(:67)가 None으로 |
| 변환 | 새 모듈 `nsql-format/src/river.rs` · `prepare`(lib.rs:592) = `layout()` 뒤 `if River { river::apply(&mut lines, opts) }` | **IR 후처리**(Walker 4천 줄은 손대지 않는다): `blocks()`(layout.rs:433)로 문장 단위 → 역할(`Role::Clause/Item/Cond/Join/SetOp/Close/Case/Comment`)과 `indent`로 질의 블록·깊이를 복원 → Clause 줄 다음의 첫 Item/Cond 줄을 **Clause 줄로 합침** · 각 줄에 `col` 계산 |
| 렌더 | `layout.rs` `render_lines`(:307) · `line_prefix`(:87) | `col`이 Some이면 `" ".repeat(col)` 접두 |
| 문장 판정 | `format_statement`(:916)의 `known` | river 대상 = SELECT/WITH/UPDATE/DELETE만 · INSERT/MERGE 줄은 `apply`가 건너뜀 |
| 설정 | `nsql-settings` REGISTRY `format.indent_style`(Choice standard/river · 기본 standard) · i18n 라벨/설명 · 설정 창 미리보기(§8-1)는 그대로 반영 | · ✅ off/indent/full(29차 `full` 추가 · `ValFmtPlsqlFull`) |
| CLI | `nsql format --indent-style river`(공통 옵션 pairs 그대로) | |
| 확장 | kiros33(탭 정렬 · strict)은 Basic `Line` 위에서 돈다 → **river일 때 확장 정렬 끔**(확장이 `col`이 있는 줄은 건너뜀 · 문서화) | 확장 SDK 문서 한 줄 |

### 10-3. 불변식·시험

- 토큰 보존(`assert_tokens_kept`) · 멱등(두 번 포맷 = 같음) — river에도 그대로.
- 시험 = 위 예 · 서브쿼리 2단(안쪽 강) · `LEFT OUTER JOIN`(강이 넓어짐) · 줄 앞 콤마 · UNION · CASE 항목 · 주석 · UPDATE/DELETE · INSERT/MERGE 불변 · 탭 단위에서 공백 강.
- 난이도 = 중(새 모듈 1 + IR 한 칸 + 렌더 분기) · Walker 무변경이라 standard 회귀 위험 낮음.

## 11. PL/SQL 블록 포맷 설계 — `format.plsql = off|indent|full`(10-30 · D-416 · T-255 · 설계 = 협업 · ✅ 10-31 `indent` 구현 · ✅ 10-31 `full` 구현(29차))

> ★ **`indent` 구현됨(10-31 · 192차 20차 · 개발 · [journal 10-31 §5](journal/2026-10-31.md))** — 새 모듈 `nsql-format/src/plsql.rs` `reindent(text, toks, base, opts) -> Option<String>` = 통과(Raw) 블록의 줄바꿈은 두고 줄 앞 공백만 구조 깊이로(§11-3 표 = 스택 Decl/Begin/If/Loop/Case/When · 루틴 머리 이어 줄 +1 · `/` 0 · 주석 줄 = 지금 깊이 · 여러 줄 문자열/블록 주석 안 줄은 손대지 않음) · 판정 실패(짝 없는 END · 안 닫힘 · `WHEN`/`IF`가 THEN과 다른 줄) = `None` = 원문 · `Options.plsql: Off|Indent`(pair `plsql` · `full`은 지금 Indent로 받음) · `layout()` 통과 가지에서 호출 · 방언 = Oracle 계열(oracle·tibero·db2 PL/SQL 호환) · V1(CLI) = §11-3 예시 본문을 왼쪽 정렬한 파일 → 예시와 같은 계단 · off = 입력과 동일. **남음** = `full`(§11-4 · v2) · T-SQL/PG 블록(v2).

### 11-1. 지금

`layout()`(layout.rs:452)이 `starts_block_passthrough`(:627 · `DECLARE` · 트랜잭션 아닌 `BEGIN` · `CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE|BODY`)이면 `block_end`(:586)까지 **원문 통과**(`push_raw` :549 · `Role::Raw`) — 블록 안의 `;`로 쪼개지 않기 위한 안전장치(§3).

### 11-2. 옵션

| 값 | 동작 |
|---|---|
| `off`(**기본** · 지금 그대로) | 원문 통과 |
| `indent` | **계단식 재들여쓰기만** — 원문 줄바꿈은 그대로 두고 줄 앞 공백만 구조 깊이로 다시 매김 |
| `full` | `indent` + 블록 안 **단위 SQL 문장**(SELECT/INSERT/UPDATE/DELETE/MERGE · `CURSOR c IS SELECT` · `OPEN c FOR SELECT`)을 지금의 SQL 포맷으로 펼쳐 그 깊이에 놓음 |

기본을 `off`로 두는 이유 = 소스를 망가뜨리면 안 되는 영역 · 사용자가 켜고 써 본 뒤 기본 변경 여부를 정한다(설정 먼저 규칙과 같은 방향).

### 11-3. `indent` 규칙(Oracle PL/SQL · 줄 단위 깊이 계산)

```sql
CREATE OR REPLACE PROCEDURE p (a NUMBER) IS     -- 머리(깊이 0) · 인자 목록 이어 줄 = +1
  v NUMBER;                                     -- IS/AS 뒤 선언부 = +1
BEGIN                                           -- 머리 깊이
  IF a > 0 THEN                                 -- 본문 = +1
    UPDATE t SET x = 1 WHERE id = a;            -- IF 안 = +1
  ELSIF a < 0 THEN                              -- 중간 키워드 = 여는 줄 깊이
    NULL;
  END IF;
  FOR r IN c LOOP
    NULL;
  END LOOP;
EXCEPTION                                       -- 머리 깊이
  WHEN OTHERS THEN                              -- +1
    RAISE;                                      -- +2
END p;
/
```

| 분류 | 단어(줄 첫 낱말 기준 · 주석·문자열 토큰 제외) | 이 줄 깊이 | 다음 줄 깊이 |
|---|---|---|---|
| 여는 것 | `DECLARE` · `BEGIN` · `IS`/`AS`(루틴·패키지 머리 끝) · `… THEN`(IF/ELSIF/WHEN 줄 끝) · `… LOOP`(FOR/WHILE/단독 LOOP 줄 끝) · `CASE`(문장) | 지금 | +1 |
| 중간 | `ELSIF` · `ELSE` · `EXCEPTION` · (EXCEPTION/CASE 안) `WHEN` | −1 | 그대로(+1 다시) |
| 닫는 것 | `END` · `END IF` · `END LOOP` · `END CASE` · `END <이름>` | −1 | −1 |
| 그 밖 | 문장 · 선언 · `NULL;` · `RETURN` · `RAISE` · `<<라벨>>` · `PRAGMA` | 지금 | 그대로 |

- 패키지 본문 안 `PROCEDURE x IS` / `FUNCTION f RETURN … IS` = 여는 것(중첩) · `END x;`로 닫힘.
- 트리거 머리(`BEFORE … ON … FOR EACH ROW`) = 머리 이어 줄(+1) · `DECLARE`/`BEGIN`부터 위 규칙.
- **여러 줄 문자열·주석 안의 줄은 손대지 않는다**(렉서 토큰 경계로 판정 · `q'[…]'` 포함).
- 깊이가 음수가 되거나 블록 끝에 0이 아니면 = **판정 실패 → 그 블록은 원문 통과**(로그 1줄).

### 11-4. `full` 추가 규칙

> ★ **구현됨(10-31 · 192차 29차 · 개발 · [journal 10-31 §14](journal/2026-10-31.md))** — `PlsqlStyle::Full`(pair `plsql = full`) · `plsql.rs` `format_unit_sql` = `indent` 걷기 중 줄 첫 낱말이 SELECT/WITH/INSERT/UPDATE/DELETE/MERGE(또는 `CURSOR … IS`/`OPEN … FOR` 뒤 · 다음 줄이어도 = 멱등)면 괄호 밖 첫 `;`까지를 `layout::format_statement`(깊이 0)로 펼쳐 줄마다 그 깊이(접두 줄이 있으면 +1)로 넣고 소비(스택 불변) · `;` 뒤 같은 줄 주석 보존 · `;` 없음/`;` 뒤 다른 토큰 = indent 규칙 · `FOR r IN (SELECT …) LOOP`·동적 SQL·`RETURN (SELECT …)` = 그대로 · 짧은 UPDATE/INSERT/DELETE = `keep_oneliners` 한 줄 · `reindent(src, text, toks, base, opts)` · 설정 선택지 `full` · V1(CLI) = CURSOR 뒤 SELECT 한 단 더 여러 줄 · 긴 SELECT INTO 블록 깊이 여러 줄 + `; -- cnt` 보존 · 짧은 UPDATE 한 줄 · 멱등 ✓. **남음** = T-SQL/PG 블록(v2) · 토큰 보존 검사(§11-5 '안전')는 옵션(대소문자·AS) 영향으로 생략 · `FOR … IN (SELECT`·동적 SQL.

- 단위 문장 = 줄 첫 낱말이 SELECT/WITH/INSERT/UPDATE/DELETE/MERGE(또는 `CURSOR … IS` · `OPEN … FOR` 뒤)부터 괄호 밖 `;`까지 → `format_statement`(layout.rs:916)에 **기준 깊이 인자**를 더해 호출(`Walker::statement(indent)` :1153은 이미 깊이를 받는다 · 지금 호출은 0) · `SELECT … INTO 변수`는 기존 `select_into_vars`(:1249) 규칙 그대로.
- `FOR r IN (SELECT …) LOOP`(괄호 안 질의) · 동적 SQL 문자열(`EXECUTE IMMEDIATE '…'`) = **v1 대상 아님**(원문 줄 유지 · 깊이만).
- T-SQL(`BEGIN … END` · `IF … ELSE` THEN 없음 · `WHILE`) · PostgreSQL `DO $$ … $$`/함수 본문(달러 인용 = 토큰 하나) = **v2**(방언별 표 · v1은 Oracle 계열 = oracle·tibero·db2 PL/SQL 호환만).

> ★ **v2 구현됨(10-31 · 192차 32차 · 개발 · [journal 10-31 §17](journal/2026-10-31.md))** — 원천 방언 = pair `dialect`(`SrcDialect` Generic/Oracle/Mssql/Postgres · **설정 키 아님** · GUI = 세션 방언 · CLI `nsql format -d`(기본 oracle)) · 렉서 = 달러 인용 태그 `$$`·`$body$` = 낱말 하나 · layout `block_end` = 달러 본문 안 `;`·BEGIN/END를 세지 않고 닫는 태그 뒤 첫 `;`에서 끝(종전 결함 = `AS $$ SELECT 1; $$ LANGUAGE sql;`이 문서 끝까지 삼킴) · `DO` 블록 통과 · 시험 2(`tsql_rules` · `plpgsql_dollar_body`) · V1(CLI) ✓ = T-SQL 계단 · `-d oracle` = 원문 · PL/pgSQL 본문 계단 + 뒤 SELECT 따로 · 멱등.

**T-SQL 규칙 표(dialect = mssql · `indent`/`full` 공통)**

| 줄 | 깊이 |
|---|---|
| `IF 조건` · `ELSE` · `WHILE 조건`(THEN 없음) | 그 깊이 · 다음 줄이 `BEGIN`이 아니면 **그 한 줄만 +1** |
| `BEGIN` … `END` · `BEGIN TRY` … `END TRY` · `BEGIN CATCH` … `END CATCH` | 블록(여는 줄 = 바깥 깊이 · 안 = +1) |
| `DECLARE @x …` | 문장(블록 아님) |
| `CASE` … `WHEN` … `END` | 식(스택 안 바꿈) |
| `GO` | 0 |
| 머리 `CREATE PROCEDURE … AS` | 뒤 `BEGIN` 없는 본문 허용 |

**PL/pgSQL 차이(어느 방언이든)**

| 줄 | 깊이 |
|---|---|
| 머리 `… AS $$` · `DO $$` | 0 · 이어 줄 +1 · `$$`로 끝 |
| 본문 `DECLARE`/`BEGIN`/`IF … THEN`/`END IF` | Oracle 공통 표(§11-3) |
| `$$ LANGUAGE plpgsql;` · `$$;` | 0 |

**남음** = T-SQL `BEGIN` 없는 `IF` 안 여러 문장(한 줄만 +1) · `IF … ELSE IF` 연쇄.

### 11-5. 구현 지점

| 지점 | 변경 |
|---|---|
| `lib.rs` `Options` | `pub plsql: PlsqlStyle { Off, Indent, Full }` · 키 `plsql` · ✅ 20차 Off/Indent · 29차 Full |
| `lib.rs` `Options.dialect` | `SrcDialect { Generic, Oracle, Mssql, Postgres }` · pair `dialect`(설정 키 아님) · GUI `format_run_pairs` = 세션 방언 · CLI `-d`(기본 oracle) · ✅ 32차 |
| `layout.rs` `layout()` 통과 분기(:452 아래 `starts_block_passthrough` 갈래) | `opts.plsql != Off`면 새 모듈 `plsql::format_block(&toks[i..j], src, opts, blank) -> Option<Vec<Line>>` · `None`(판정 실패·토큰 불일치) = 지금처럼 `push_raw` |
| 새 모듈 `nsql-format/src/plsql.rs` | 줄 단위 깊이 계산(§11-3 표 = 상수 표 + 순수 함수) · `full`이면 단위 문장 구간을 `format_statement(…, base)`로 · ✅ 20차 `reindent` · 29차 `format_unit_sql` |
| `format_statement` | 기준 깊이 인자 추가(기존 호출 = 0) · 29차 = 깊이 0으로 펼친 뒤 줄마다 들여 넣음(인자 추가 대신) |
| 안전 | 결과 토큰열(공백·줄바꿈 제외)이 원문과 다르면 원문 반환 · 멱등 확인 · 멱등 ✓ · 토큰열 비교는 옵션(대소문자·AS) 영향으로 생략(29차) |
| 설정 | REGISTRY `format.plsql`(Choice off/indent/full · 기본 off) · i18n · 미리보기 예시에 작은 프로시저 하나 |
| 확장 | 블록 줄 = `Role::Raw` 유지(확장 정렬 대상 아님) |

### 11-6. 시험

- 위 예 + 패키지 명세/본문(중첩 루틴) · 트리거 · 익명 블록 · CASE 문 · 예외 블록 · 라벨 · 여러 줄 문자열/`q'[…]'` 안 키워드 · 깊이 실패 → 원문 · `full`의 SELECT INTO · 커서 · 멱등 · 토큰 보존.
- 분기 표(여는/중간/닫는)는 MC/DC 대상(52 §12 규칙).
- 표본 소스는 **익명화한 것만** 저장소에(실제 업무 패키지 금지).
- 난이도 = `indent` 중 · `full` 중(+) · 렌더·Walker 변경은 `format_statement` 기준 깊이 한 곳.

## 12. 포맷 프리셋 — `format.preset = default|kiros33|river|custom`(10-31 · D-431 · 192차 35차 · ✅ 구현)

> 요청(사용자 10-31 저녁) = 현재 설정 → `kiros33` · 기본값 → `Default` · river용 → `River` 프리셋을 만들고 포맷 설정에서 골라 묶음으로 바꾸게. 설정 먼저 규칙으로 먼저 설정 창 **Import…** + 프리셋 JSON 3개를 안내했고(journal §19) → 사용자가 개발로 확정(AskUserQuestion 2회 답 · 개발 세션) → [journal 10-31 §20](journal/2026-10-31.md).

| 항목 | 결정(D-431) |
|---|---|
| 대상 | 내장 Basic 포매터 옵션만(스타일 키 = `format.*` − `format.default` · `format.max_kb` · `format.preset` = `is_format_style_key`) |
| 프리셋 | 내장 3개만(사용자 저장 없음) · `default` = 레지스트리 기본값에서 생성 · `kiros33` = 기본값과 다른 12키(간격 4종 tab · comma leading · cond_indent same · function_case upper · where_seed · paren_seed · auto_alias · column_alias_all · semicolon_newline on) · `river` = 2키(indent_style river · comma leading) · 표 = `nsql_settings::FORMAT_PRESETS` |
| 적용 | **복사** — 고르는 순간 스타일 키 값을 설정에 써 넣음(`apply_settings_json_text` 한 길 = 저장·반영·설정 창·미리보기 갱신 · 재진입 가드) · 즉시 적용 + 5초 [되돌리기] 토스트(`format.preset_undo` · 적용 전 스냅숏) |
| 상태 표시 | 스타일 키가 바뀌면 일치하는 프리셋 이름을 `format.preset`에 동기 · 없으면 `custom` |
| 진입점 4 | ① 설정 창 SQL Formatting 맨 위 "Preset" 드롭다운(기존 Choice 카드 · 라이브 미리보기) ② 미리보기와 같은 창 ③ 팔레트 "Format preset…"(`edit.format_preset` → `format.preset:<이름>` · 현재 = ✓) + "Format with…" 목록 끝 · Edit 메뉴 ④ CLI `nsql format --preset <이름>`(그 실행만 설정 위에 덮음 · 설정은 안 바꿈 · 모르는 이름 = 종료 2) |
| 범위 | 프로젝트 설정으로 덮을 수 없음(`project_allowed` 제외) · `OPTION_KEYS`의 `preset` = 포매터가 무시 |

- **V1(10-31 35차 · 협업)** ✓ = 설정 창 Format 첫 카드 `format.preset` · GUI `format.preset:kiros33` → 토스트 "포맷 프리셋 kiros33 적용(옵션 13개 바뀜)" + 5초 되돌리기 · 스타일 키 하나 바꿈 → `custom` · CLI `--preset kiros33/river` 꼴 · `--preset nope` = 종료 2 + 안내 · 팔레트 3행.
- **관찰(P2 · 개발 판단 대기)** = CLI `nsql config set format.preset river`는 값만 기록하고 프리셋을 적용하지 않음 → 표시 이름과 실제 옵션이 어긋난 채 저장.
