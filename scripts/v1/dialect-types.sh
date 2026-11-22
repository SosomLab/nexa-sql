#!/usr/bin/env bash
# V1 dialect-types — T-78 ⓐ 방언별 자료형 강조(판정 = `editor.tokens:<파일>` `글<탭>종류`).
#   방언 탭 = SQLite 프로필 접속(서버 없이 · rclose.sh 방식) · 대조 = 접속 없는 탭(기본 "SQL" 스펙).
#   대조 낱말 = `DATETIME`(SQLITE_TYPES에 있고 기본 스펙 datatypes 줄 = nexa-ui highlight.rs:591에 없음).
#   `x TIMESTAMP WITH TIME ZONE` = TIMESTAMP는 자료형 · WITH는 자료형이 아님(여러 낱말 이름은 첫 낱말만) · TIME은 기본 스펙 datatypes에 원래 있어 양쪽 다 Datatype(판정 안 함).
. "$(dirname "$0")/../lib/v1.sh"
tok() { grep -P "^$2\t" "$1" 2>/dev/null | head -1 | cut -f2 | tr -d '\r'; }
v1_home en
v1_cli conn add lite "sqlite:$(v1_m "$V1_HOME")/d.sqlite" -d sqlite --no-prompt >/dev/null
mkdir -p "$V1_OUT"
printf 'CREATE TABLE t (a DATETIME, b INTEGER, c VARCHAR(10), x TIMESTAMP WITH TIME ZONE);\n' > "$V1_OUT/q.sql"
W=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/dt-*.txt
# 방언 탭(SQLite 접속)
v1_gui dts "@connected:open:$W/q.sql,@connected:@after:1500:editor.tokens:$W/dt-sqlite.txt,@connected:@after:2000:file.exit" 20 lite
# 대조 = 접속 없음(기본 SQL 스펙)
v1_gui dtn "open:$W/q.sql,@after:1500:editor.tokens:$W/dt-plain.txt,@after:2000:file.exit" 20
expect_eq "s-datetime-datatype" "$(tok "$V1_OUT/dt-sqlite.txt" DATETIME)" "Datatype"
expect_eq "s-integer-datatype" "$(tok "$V1_OUT/dt-sqlite.txt" INTEGER)" "Datatype"
expect_eq "s-timestamp-datatype" "$(tok "$V1_OUT/dt-sqlite.txt" TIMESTAMP)" "Datatype"
expect_not "s-with-not-datatype" "$V1_OUT/dt-sqlite.txt" "$(printf 'WITH\tDatatype')"
expect_not "s-zone-not-datatype" "$V1_OUT/dt-sqlite.txt" "$(printf 'ZONE\tDatatype')"
expect_eq "s-create-keyword" "$(tok "$V1_OUT/dt-sqlite.txt" CREATE)" "Keyword"
expect_eq "n-dump-ran" "$(tok "$V1_OUT/dt-plain.txt" CREATE)" "Keyword"
expect_not "n-datetime-not-datatype" "$V1_OUT/dt-plain.txt" "$(printf 'DATETIME\tDatatype')"
expect_eq "n-integer-datatype-common" "$(tok "$V1_OUT/dt-plain.txt" INTEGER)" "Datatype"
for t in dts dtn; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
v1_done
