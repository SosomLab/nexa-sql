# SQL 포맷(Format SQL)

**Shift+Alt+F** = 기본 포맷터로 포맷(선택이 있으면 선택만, 없으면 문서 전체 · Ctrl+Z로 되돌리기).
Edit 메뉴 · 명령 팔레트에도 같은 항목이 있습니다.

## 포맷터 둘

| 포맷터 | 어디서 | 특징 |
|---|---|---|
| **Basic**(내장) | 기본 | 절 단독 줄 · 항목마다 한 줄 · 콤마 위치/간격 · `WHERE 1=1` 시드 · 대소문자 · 서브쿼리 펼침 · 짧은 CASE는 한 줄 |
| **SQL Formatter for kiros33**(확장) | Extension Manager ▸ Install | Basic 공통 옵션 위에 **탭 수직 정렬**(AS · 비교 연산자 · ORDER BY 방향) · AND/OR를 WHERE 열에 · 집합 연산자 대시 구분행 · `ext.sqlfmt_kiros33.strict`(기본 켬) = 스킬 규정값 강제 |

## 자주 쓰는 순서

1. **한 번 쓰기**: 팔레트 ▸ "포맷터 골라 포맷…" ▸ `SQL Formatter for kiros33(으)로 포맷`.
2. **기본으로 지정**: 같은 목록에서 "…을(를) 기본 포맷터로" → 이후 Shift+Alt+F가 그 포맷터를 씁니다(설정 `format.default`).
3. **미리보기**: 팔레트/Edit ▸ "포맷 미리보기" → 읽기 전용 탭에 현재 문서(또는 선택 · 비면 예시 SQL)의 포맷 결과. 설정 창에서 `SQL 포맷`/`SQL Formatter for kiros33` 값을 바꾸면 **즉시 다시 그립니다**(원본은 바뀌지 않음 · 탭을 닫으면 끝).

## 공통 옵션(Preferences ▸ Editor ▸ SQL 포맷 · `format.*`)

들여쓰기 단위(탭/공백)·폭 · 키워드/식별자/함수 대소문자 · 콤마 위치(줄 앞/줄 끝)·콤마 뒤 간격(공백/탭) · AND/OR 위치 · `WHERE 1=1` 시드 · 목록(항목마다 줄/한 줄/자동)·줄 폭 · CASE 한 줄 상한 · 연산자 공백 · 모든 열에 별칭 · 열/테이블 별칭 `AS`(그대로/추가/제거) · JOIN 들여쓰기 · 한 줄 DML 유지 · 문장 사이 빈 줄·연속 빈 줄 상한 · 개행·끝 개행·`;` 줄.

이 옵션은 **Basic과 확장이 같이** 읽습니다. kiros33은 `ext.sqlfmt_kiros33.strict`가 켜져 있으면 스킬 규정값(탭 · 폭 4 · 콤마 앞 · `,\t` · `1=1` · 대문자 · AND 앞)을 강제하고, 끄면 위 공통 값을 그대로 따릅니다.

## 원문이 유지되는 것

PL/SQL 블록(`DECLARE`/`BEGIN`/`CREATE … PROCEDURE …`) · DDL · 주석 안의 SQL · 한 줄로 쓴 짧은 `INSERT … VALUES`/`UPDATE`/`DELETE`(설정 `format.keep_oneliners`). 별칭 자동 부여 · 테이블 설명 주석 · 방언 치환은 아직 하지 않습니다.

## 포맷터 확장 만들기(SDK)

`extensions/sdk` 샘플 `sql-formatter-kiros33`을 복제해 `Meta.formatter`를 내고 `Extension::format(&FormatRequest)`를 구현합니다. 공통 옵션은 `nsql-format`의 `Options::from_pairs(req.options)`로 그대로 읽을 수 있습니다. 자세히 = `docs/95-sql-formatter.md` §5.
