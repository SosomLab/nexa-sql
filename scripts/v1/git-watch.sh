#!/usr/bin/env bash
# V1 git-watch — git 감시 판정(11-22 결함 · 협업 초안 · statusbar.git=off에서도 거터가 HEAD를 받는가 · app/paint.rs status_seg_git 수정분).
#   임시 git 저장소(커밋 1 · 10줄) → 7번째 줄만 디스크에서 고침 → 그 파일을 열고 HEAD를 받을 시간을 둔 뒤 `git.next_hunk` →
#   `editor.dump` 머리의 caret=7:1이면 덩어리를 찾았다 = HEAD 본문을 받았다(받지 못하면 덩어리 0 → 캐럿 1:1 그대로).
#   g1 = statusbar.git=off · editor.git_gutter=on(결함 장면) · g0 대조 = statusbar.git=on(종전에도 되던 장면) ·
#   gX 대조 = statusbar.git=off · editor.git_gutter=off · editor.minimap_git=off(감시할 이유가 없으면 그대로 1:1 = 판정이 공짜로 서지 않음을 보임).
. "$(dirname "$0")/../lib/v1.sh"
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 200 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_not_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_failmsg "$1" "/$3/ 있음"; else v1_pass "$1"; fi; }
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1gw-XXXX")
R="$WK/repo"; mkdir -p "$R"
for i in 1 2 3 4 5 6 7 8 9 10; do echo "SELECT $i FROM dual;"; done > "$R/g.sql"
git -C "$R" init -q && git -C "$R" add g.sql && git -C "$R" -c user.name=v1 -c user.email=v1@example.invalid commit -q -m init
sed -i 's/^SELECT 7 FROM dual;$/SELECT 77 FROM dual;/' "$R/g.sql"
GM=$(v1_m "$R/g.sql"); O=$(v1_m "$V1_OUT")
run() { # <태그> <설정 줄…>
  local tag=$1; shift
  v1_home en; v1_set "statusbar.git_secs=1" "$@"
  rm -f "$V1_OUT/$tag-ed.txt"
  v1_gui "$tag" "open:$GM,@after:3500:git.next_hunk,@after:3900:editor.dump:$O/$tag-ed.txt,@after:4300:file.exit" 25
  [ -n "${V1_HOME:-}" ] && rm -rf "$V1_HOME"
}
run gw1 "statusbar.git=off" "editor.git_gutter=on" "editor.minimap_git=off"
run gw0 "statusbar.git=on" "editor.git_gutter=on" "editor.minimap_git=off"
run gwm "statusbar.git=off" "editor.git_gutter=off" "editor.minimap_git=on"
run gwx "statusbar.git=off" "editor.git_gutter=off" "editor.minimap_git=off"
V1_HOME=""
expect_re "g1-off-gutter-on-found-hunk" "$V1_OUT/gw1-ed.txt" '\|caret=7:1\|'
expect_re "g0-on-found-hunk" "$V1_OUT/gw0-ed.txt" '\|caret=7:1\|'
expect_re "gm-off-minimap-on-found-hunk" "$V1_OUT/gwm-ed.txt" '\|caret=7:1\|'
expect_re "gx-dump-ran" "$V1_OUT/gwx-ed.txt" '\|caret='
expect_not_re "gx-all-off-no-hunk" "$V1_OUT/gwx-ed.txt" '\|caret=7:1\|'
for t in gw1 gw0 gwm gwx; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done

# ── E(셋째 독립 검토 후속 · 11-23 묶음 20): 감시 파일이 HEAD 없는 파일로 바뀌면 앞 파일의 git 기준선이 남지 않는다 ──
#   g.sql(7행 수정 = 덩어리 있음)을 띄워 HEAD를 받은 뒤 같은 폴더의 .gitignore된 i.sql(7행이 다른 글)로 전환 → 브랜치·변경 수는 그대로
#   → `git.next_hunk` = "No git changes in this tab"(종전 = 앞 파일 기준선이 남아 i.sql에 덩어리가 보일 수 있었다)
printf 'i.sql\n' > "$R/.gitignore"
for i in 1 2 3 4 5 6 7 8 9 10; do [ "$i" = 7 ] && echo "SELECT 707 FROM dual;" || echo "SELECT $i FROM dual;"; done > "$R/i.sql"
IM=$(v1_m "$R/i.sql")
v1_home en; v1_set "statusbar.git_secs=1" "statusbar.git=on" "editor.git_gutter=on"
v1_gui gwe "open:$GM,@after:3500:git.next_hunk,@after:3800:editor.dump:$O/gwe-a.txt,@after:4000:open:$IM,@after:7000:git.next_hunk,@after:7300:status.dump:$O/gwe-st.txt,@after:7500:editor.dump:$O/gwe-b.txt,@after:7900:file.exit" 25
[ -n "${V1_HOME:-}" ] && rm -rf "$V1_HOME"; V1_HOME=""
expect_re "e-a-had-hunk" "$V1_OUT/gwe-a.txt" '\|caret=7:1\|'
expect "e-b-no-hunks" "$V1_OUT/gwe-st.txt" "No git changes in this tab"
expect_not_re "e-b-caret-not-moved" "$V1_OUT/gwe-b.txt" '\|caret=7:1\|'
expect_not "gwe-no-panic" "$V1_OUT/gwe.stderr" "panicked"
rm -rf "$WK"
v1_done
