#!/usr/bin/env bash
# V1 gutter-bigdiff — 큰 파일 git 덩어리(T-117 · nexa-ui `diff_seq_hint` LCS 상한 초과 구간 = Myers · 11-23): 헤드리스 · 격리 홈 · 입력 주입 0 · 저장소 불변.
#   임시 git 저장소에 6000줄 파일 커밋 → 작업본 = 10~19행 수정 · 3000행 뒤 1줄 삽입 · 5500행 삭제(공통 앞뒤를 뺀 가운데 구간 ≈ 5490줄 > LCS 상한).
#   덩어리 덤프 명령이 없으므로 `git.next_hunk`를 네 번 부르고 매번 `editor.dump` 머리의 caret 줄을 본다 —
#   고친 뒤 = 서로 다른 세 줄(10 · 3001 근처 · 5500 근처)을 돈 뒤 첫 줄로 돌아온다 · 종전(가운데 전체 = 덩어리 하나) = 한 줄에서 맴돈다.
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1gb-XXXX")
R="$WK/repo"; mkdir -p "$R"
awk 'BEGIN{for(i=1;i<=6000;i++) printf "SELECT %d AS n FROM dual;\n", i}' > "$R/big.sql"
git -C "$R" init -q && git -C "$R" add big.sql && git -C "$R" -c user.name=v1 -c user.email=v1@example.invalid commit -q -m init
awk '{ if (NR>=10 && NR<=19) { sub(/ AS n /, " AS m ") } if (NR==5500) next; print; if (NR==3000) print "SELECT 0 AS inserted FROM dual;" }' "$R/big.sql" > "$R/big.tmp" && mv "$R/big.tmp" "$R/big.sql"
BM=$(v1_m "$R/big.sql"); O=$(v1_m "$V1_OUT"); H0=$(git -C "$R" rev-parse HEAD)
rm -f "$V1_OUT"/gb-*.txt
v1_home en; v1_set "statusbar.git_secs=1" "editor.git_gutter=on"
v1_gui gb "open:$BM,@after:5000:git.next_hunk,@after:5400:editor.dump:$O/gb-1.txt,@after:5600:git.next_hunk,@after:6000:editor.dump:$O/gb-2.txt,@after:6200:git.next_hunk,@after:6600:editor.dump:$O/gb-3.txt,@after:6800:git.next_hunk,@after:7200:editor.dump:$O/gb-4.txt,@after:7600:file.exit,@after:8100:toolbar.pick:project.exit_skip" 30
ln() { tr -d '\r' < "$1" 2>/dev/null | head -1 | sed -n 's/.*|caret=\([0-9]*\):.*/\1/p'; }
L1=$(ln "$V1_OUT/gb-1.txt"); L2=$(ln "$V1_OUT/gb-2.txt"); L3=$(ln "$V1_OUT/gb-3.txt"); L4=$(ln "$V1_OUT/gb-4.txt")
echo "hunk caret lines: $L1 → $L2 → $L3 → $L4"
expect_eq "h1-first-hunk-at-10" "$L1" "10"
if [ -n "$L2" ] && [ "$L2" -ge 2995 ] && [ "$L2" -le 3005 ] 2>/dev/null; then v1_pass "h2-insert-hunk-near-3001"; else v1_failmsg "h2-insert-hunk-near-3001" "caret=$L2"; fi
if [ -n "$L3" ] && [ "$L3" -ge 5495 ] && [ "$L3" -le 5505 ] 2>/dev/null; then v1_pass "h3-delete-hunk-near-5500"; else v1_failmsg "h3-delete-hunk-near-5500" "caret=$L3"; fi
expect_eq "h4-wraps-to-first" "$L4" "$L1"
expect_eq "three-distinct-hunks" "$(printf '%s\n' "$L1" "$L2" "$L3" | sort -u | grep -c .)" "3"
expect_eq "repo-unchanged" "$(git -C "$R" rev-parse HEAD)" "$H0"
expect_not "gb-no-panic" "$V1_OUT/gb.stderr" "panicked"
rm -rf "$WK"
v1_done
