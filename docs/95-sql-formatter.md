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

