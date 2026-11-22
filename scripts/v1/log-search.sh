#!/usr/bin/env bash
# V1 log-search — (T-54 1차 · 로그 창 검색어 필터 · 기동 명령 `log.search:<글>` · `log.stat`의 `search=`/`visible=`).
#   접속 없이 로그 줄을 만드는 길 = 프로젝트 열기 안내 둘(등록 폴더 없음 "project folder(s) not found on this PC" · 원본 없는 탭 "file not found").
. "$(dirname "$0")/../lib/v1.sh"
field() { tr -d '\r' < "$1" 2>/dev/null | sed -n "s/.*$2=\([^ ]*\).*/\1/p" | head -1; }
v1_home en
v1_set "project.autosave=off"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1ls-XXXX")
P="$WK/proj"; mkdir -p "$P/sql"; printf 'SELECT 1 FROM dual;\n' > "$P/sql/a.sql"
PM=$(v1_m "$P"); O=$(v1_m "$V1_OUT")
printf '{"version": 2, "folders": [{"path": "."}, {"path": "%s"}], "active": 0, "tabs": [{"path": "sql/a.sql", "title": "a.sql", "id": 1}, {"path": "sql/gone.sql", "title": "gone.sql", "id": 2}]}\n' "$(v1_m "$WK/nofolder/lib")" > "$P/l.nsql-project"
rm -f "$V1_OUT"/ls-*.txt
v1_gui ls "project.load:$PM/l.nsql-project,@after:1800:log.dump:$O/ls-log.txt,@after:2000:log.stat:$O/ls-s0.txt,@after:2300:log.search:folder(s),@after:2600:log.stat:$O/ls-s1.txt,@after:2900:log.search:zzqxnope,@after:3200:log.stat:$O/ls-s2.txt,@after:3500:log.search:,@after:3800:log.stat:$O/ls-s3.txt,@after:4100:log.search:FILE not,@after:4400:log.stat:$O/ls-s4.txt,@after:4800:file.exit,@after:5400:toolbar.pick:project.exit_skip" 25
N=$(field "$V1_OUT/ls-s0.txt" entries)
expect "s-log-has-folder-line" "$V1_OUT/ls-log.txt" "project folder(s) not found on this PC"
expect "s-log-has-notfound-line" "$V1_OUT/ls-log.txt" "file not found"
expect_eq "s0-search-off" "$(field "$V1_OUT/ls-s0.txt" search)" "-"
expect_eq "s0-all-visible" "$(field "$V1_OUT/ls-s0.txt" visible)" "$N"
expect_eq "s1-search-set" "$(field "$V1_OUT/ls-s1.txt" search)" "folder(s)"
expect_eq "s1-one-line" "$(field "$V1_OUT/ls-s1.txt" visible)" "$(grep -ciF 'folder(s)' "$V1_OUT/ls-log.txt")"
expect_eq "s2-none" "$(field "$V1_OUT/ls-s2.txt" visible)" "0"
expect_eq "s3-cleared" "$(field "$V1_OUT/ls-s3.txt" search)" "-"
expect_eq "s3-all-again" "$(field "$V1_OUT/ls-s3.txt" visible)" "$N"
# s4 = 낱말 둘 · 대소문자 무시("FILE not" ↔ "file not found") — 낱말 전부 포함 줄 수
expect_eq "s4-words-case-insensitive" "$(field "$V1_OUT/ls-s4.txt" visible)" "$(grep -i 'file' "$V1_OUT/ls-log.txt" | grep -ci 'not')"
expect_eq "s-entries-positive" "$([ "${N:-0}" -ge 2 ] && echo y)" "y"
expect_not "ls-no-panic" "$V1_OUT/ls.stderr" "panicked"
rm -rf "$WK"
v1_done
