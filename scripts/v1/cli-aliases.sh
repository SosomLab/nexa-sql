#!/usr/bin/env bash
# V1 cli-aliases — 다른 CLI(psql · mysql · sqlcmd)에서 넘어온 짧은 옵션(T-51 · D-379 · docs/27 §3): CLI만 · 격리 홈 · 서버 쓰기 0.
#   접속 옵션(-c)을 받는 명령(run · export · shell)에서만 푼다: -U = --user · -D = --db · -Q/-e = --query · -i <file> = 자리 인자 ·
#   -S host[,port](tcp: 허용) = --host/--port · -P <숫자> = --port / 숫자 아님 = 거절 rc 2 · -W = 그냥 받음 · -T = 거절 rc 2 ·
#   -d(방언) · -p(비밀번호) · -u · -h(도움말)는 종전 뜻 · `run -q <sql>` = 경로 없이 질의 글 실행 · grep의 -e/-i · conn 하위 명령 불변.
#   접속 문자열 조립 판정 = 익명 호스트 `db.invalid`(접속은 실패해도 된다 = 오류 글에 host·port가 보이면 PASS).
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1ca-XXXX")
o() { printf '%s\n' "$2" > "$V1_OUT/$1.txt"; }
rc() { "$V1_NSQL" "$@" </dev/null >"$V1_OUT/rc.txt" 2>&1; echo $?; }
M="sqlite::memory:"
v1_home en
# ── run: -Q · -e · -q(여러 문장) · -i ──
o q1 "$(v1_cli run -c "$M" -d sqlite -f csv -Q "select 41+1 as v")"
expect "run-Q-query" "$V1_OUT/q1.txt" "42"
o q2 "$(v1_cli run -c "$M" -d sqlite -f csv -e "select 7 as v;")"
expect "run-e-query" "$V1_OUT/q2.txt" "7"
o q3 "$(v1_cli run -c "$M" -d sqlite -f csv -q "select 1 as a; select 2 as b")"
expect "run-q-first" "$V1_OUT/q3.txt" "a"
expect "run-q-second" "$V1_OUT/q3.txt" "b"
printf 'select 5 as v;\n' > "$WK/q.sql"
o q4 "$(v1_cli run -c "$M" -d sqlite -f csv -i "$(v1_m "$WK/q.sql")")"
expect "run-i-file" "$V1_OUT/q4.txt" "5"
o q5 "$(v1_cli run -c "$M" -d sqlite -f csv -W -Q "select 6 as v")"
expect "run-W-accepted" "$V1_OUT/q5.txt" "6"
# ── 거절 ──
expect_eq "run-P-password-rc2" "$(rc run -c "$M" -P secret "$(v1_m "$WK/q.sql")")" "2"
cp "$V1_OUT/rc.txt" "$V1_OUT/p-secret.txt"
expect "run-P-password-msg" "$V1_OUT/p-secret.txt" "-P <password> is not accepted"
expect_eq "run-T-rc2" "$(rc run -T mssql)" "2"
cp "$V1_OUT/rc.txt" "$V1_OUT/t.txt"
expect "run-T-msg" "$V1_OUT/t.txt" "-T: use -d <dialect>"
expect_eq "run-no-path-no-q-errors" "$(rc run -c "$M" -d sqlite)" "2"
# ── 접속 문자열 조립(-S host,port · -U · -D · -P 숫자) ──
#   필드 형식(--host · --port · --db · --user)은 `conn add`에서 접속 대상이 된다(run·export는 -c <target> 필수 · 11-23 첫 판 = "-c <target> is required")
o s1 "$(v1_cli conn add ms1 -d mssql -S db.invalid,1433 -U sa -D appdb --no-prompt; v1_cli conn show ms1)"
expect "S-host-in-profile" "$V1_OUT/s1.txt" "db.invalid"
expect "S-port-in-profile" "$V1_OUT/s1.txt" "1433"
expect "U-user-in-profile" "$V1_OUT/s1.txt" "sa"
expect "D-db-in-profile" "$V1_OUT/s1.txt" "appdb"
o s2 "$(v1_cli conn add my1 -d mysql -S tcp:db.invalid -P 3399 -U app -D appdb --no-prompt; v1_cli conn show my1)"
expect "S-tcp-prefix-host" "$V1_OUT/s2.txt" "db.invalid"
expect_not "S-tcp-prefix-stripped" "$V1_OUT/s2.txt" "tcp:"
expect "P-number-is-port" "$V1_OUT/s2.txt" "3399"
expect_not "P-number-not-password-refusal" "$V1_OUT/s2.txt" "is not accepted"
expect_eq "run-S-without-c-needs-target" "$(rc run -d mysql -S db.invalid -Q "select 1" --no-prompt)" "2"
# ── export · shell에서도 같은 별칭 ──
o e1 "$(v1_cli export -c "$M" -d sqlite -Q "select 8 as v")"
expect "export-Q-alias" "$V1_OUT/e1.txt" "8"
o sh1 "$(printf 'select 9 as v;\n' | "$V1_NSQL" shell -c "$M" -d sqlite -W 2>&1)"
expect "shell-W-accepted-runs" "$V1_OUT/sh1.txt" "9"
# ── 불변: grep의 -e/-i · conn 하위 명령 · -h 도움말 ──
mkdir -p "$WK/g"; printf 'Select 1;\nselect 2;\n' > "$WK/g/a.sql"
o g1 "$(v1_cli grep -i select "$(v1_m "$WK/g")")"
expect_eq "grep-i-two-lines" "$(grep -c ':1:\|:2:' "$V1_OUT/g1.txt")" "2"
o g2 "$(v1_cli grep -e 'sel+ect' "$(v1_m "$WK/g")")"
expect "grep-e-regex" "$V1_OUT/g2.txt" "select 2"
o c1 "$(v1_cli conn add lite "sqlite:$(v1_m "$WK")/d.sqlite" -d sqlite --no-prompt; v1_cli conn list)"
expect "conn-list-still-works" "$V1_OUT/c1.txt" "lite"
o c2 "$(v1_cli conn test lite)"
expect_not "conn-test-no-usage-error" "$V1_OUT/c2.txt" "unexpected argument"
o h1 "$(v1_cli --help)"
expect "help-coming-from" "$V1_OUT/h1.txt" "Coming from psql, mysql or sqlcmd"
o h2 "$(v1_cli run -h)"
expect "run-h-is-help" "$V1_OUT/h2.txt" "nsql run"
# ── 한국어 안내 ──
rm -rf "$V1_HOME"; v1_home ko
expect_eq "ko-P-password-rc2" "$(rc run -c "$M" -P secret "$(v1_m "$WK/q.sql")")" "2"
cp "$V1_OUT/rc.txt" "$V1_OUT/p-ko.txt"
if LC_ALL=C grep -q $'[\xea-\xed]' "$V1_OUT/p-ko.txt"; then v1_pass "ko-P-message-korean"; else v1_failmsg "ko-P-message-korean" "$(head -c 200 "$V1_OUT/p-ko.txt")"; fi
rm -rf "$WK"
v1_done
