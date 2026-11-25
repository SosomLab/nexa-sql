#!/usr/bin/env bash
# V1 ai-search-scope — AI 도구 `search_files`: 범위 필수(D-555 · D-558) · 사용자 검색 패널과 분리(D-559 · 11-25).
#   헤드리스 · 격리 홈 · 접속 없음 · 입력 주입 0 · 임시 폴더만. 도구는 기동 명령 `ai.tool:<이름>|<k=v;k=v>|<파일>`로 직접 돌린다.
#   s1 `where` 없음              → ok=false · "NOT run"(프로젝트 전체를 훑지 않는다)
#   s2 `where` = 패턴뿐(`*.sql`) → ok=false · "NOT run"
#   s3 `where` = 폴더            → ok=true · 그 폴더의 일치 줄(`a.sql:2:`) · 다른 폴더 파일은 없음
#   s4 s3 뒤 사용자 검색 패널    → 결과 0줄(AI가 패널에 값을 넣지도 검색을 돌리지도 않았다)
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1as-XXXX"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/as-*.txt
mkdir -p "$WK/in" "$WK/out"
printf 'SELECT 1 FROM dual;\n-- NEEDLE_COL here\n' > "$WK/in/a.sql"
printf -- '-- NEEDLE_COL elsewhere\n' > "$WK/out/b.sql"
v1_home en
v1_gui as "open:$W/in/a.sql,@after:1500:ai.tool:search_files|query=NEEDLE_COL|$O/as-1.txt,@after:2500:ai.tool:search_files|query=NEEDLE_COL;where=*.sql|$O/as-2.txt,@after:3500:ai.tool:search_files|query=NEEDLE_COL;where=$W/in|$O/as-3.txt,@after:7000:search.dump:$O/as-4.txt,@after:7500:file.exit" 20
expect "s1-no-where-rejected" "$V1_OUT/as-1.txt" "ok=false"
expect "s1-says-not-run" "$V1_OUT/as-1.txt" "NOT run"
expect "s2-pattern-only-rejected" "$V1_OUT/as-2.txt" "ok=false"
expect "s3-folder-ok" "$V1_OUT/as-3.txt" "ok=true"
expect "s3-finds-in-folder" "$V1_OUT/as-3.txt" "a.sql:2:"
expect_not "s3-not-outside-folder" "$V1_OUT/as-3.txt" "b.sql"
expect_not "s4-user-panel-untouched" "$V1_OUT/as-4.txt" "a.sql"
expect_not "s4-user-panel-no-query" "$V1_OUT/as-4.txt" "NEEDLE_COL"
expect_not "no-panic" "$V1_OUT/as.stderr" "panicked"
rm -rf "$WK"
v1_done
