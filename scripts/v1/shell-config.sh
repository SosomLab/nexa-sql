#!/usr/bin/env bash
# V1 shell-config — `nsql shell`의 `${config:키}` 치환(T-359 ⓐ): 설정 값으로 바뀐다(기본 db.fetch_size = 200 · ui.lang = 격리 홈 en).
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
DB="sqlite:$(v1_m "$V1_HOME")/s.sqlite"
printf "SELECT '\${config:db.fetch_size}' AS v;\nSELECT '\${config:ui.lang}' AS l;\n" | "$V1_NSQL" shell -c "$DB" > "$V1_OUT/shell.txt" 2>&1
expect "fetch-size" "$V1_OUT/shell.txt" "200"
expect "ui-lang" "$V1_OUT/shell.txt" "en"
expect_not "unreplaced" "$V1_OUT/shell.txt" '${config:'
v1_done
