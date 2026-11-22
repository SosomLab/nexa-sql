#!/usr/bin/env bash
# V1 git-revert — 덩어리 되돌리기 `git.revert_hunk`(C-1 · docs/19 §2 · 11-22 · 협업 초안): 헤드리스 · 격리 홈 · 입력 주입 0 · 디스크·git 저장소 불변.
. "$(dirname "$0")/../lib/v1.sh"
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 200 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1gw-XXXX")
R="$WK/repo"; mkdir -p "$R"
for i in 1 2 3 4 5 6 7 8 9 10; do echo "SELECT $i FROM dual;"; done > "$R/g.sql"
cp "$R/g.sql" "$R/d.sql"
git -C "$R" init -q && git -C "$R" add g.sql d.sql && git -C "$R" -c user.name=v1 -c user.email=v1@example.invalid commit -q -m init
sed -i 's/^SELECT 7 FROM dual;$/SELECT 77 FROM dual;/' "$R/g.sql"
sed -i '/^SELECT 5 FROM dual;$/d' "$R/d.sql"
GM=$(v1_m "$R/g.sql"); DM=$(v1_m "$R/d.sql"); O=$(v1_m "$V1_OUT")
G0=$(md5sum < "$R/g.sql"); D0=$(md5sum < "$R/d.sql")
# 기동 1회 = 새 격리 홈(미저장 본문 hot exit가 다음 기동에 새지 않게) · <태그> <명령 목록(open 뒤)>
rv() { local tag=$1 file=$2 cmds=$3
  v1_home en; v1_set "statusbar.git_secs=1" "editor.git_gutter=on"
  v1_gui "$tag" "open:$file,@after:3500:$cmds" 25
  rm -rf "$V1_HOME"; V1_HOME=""
}
rm -f "$V1_OUT"/rv*.txt
# undo 뒤 캐럿 = 되살린 글 선택 끝(8:1 · sel 120-141)이라 덩어리 밖 → r3 전에 캐럿을 7번째 줄 첫 글자(120)로 옮긴다.
# r1·r2·r3. 덩어리로 가서 되돌림 → 본문 = HEAD · 상태줄 · 디스크 불변 → undo 한 번 = 고친 글 복귀 → 다시 되돌린 뒤 next_hunk = 덩어리 0
rv rv1 "$GM" "git.next_hunk,@after:3900:git.revert_hunk,@after:4200:editor.dump:$O/rv1-ed.txt,@after:4300:status.dump:$O/rv1-st.txt,@after:4600:edit.undo,@after:4900:editor.dump:$O/rv2-ed.txt,@after:5100:editor.caret:120,@after:5300:git.revert_hunk,@after:5500:git.next_hunk,@after:5800:status.dump:$O/rv3-st.txt,@after:6000:editor.dump:$O/rv3-ed.txt,@after:6400:file.exit"
expect "r1-body-head-line" "$V1_OUT/rv1-ed.txt" "SELECT 7 FROM dual;"
expect_not "r1-body-no-edit" "$V1_OUT/rv1-ed.txt" "SELECT 77 FROM dual;"
expect "r1-status-reverted" "$V1_OUT/rv1-st.txt" "Reverted the git change at line 7"
expect_re "r1-caret-hunk-line" "$V1_OUT/rv1-ed.txt" '\|caret=7:'
expect "r2-undo-brings-edit" "$V1_OUT/rv2-ed.txt" "SELECT 77 FROM dual;"
expect_not "r2-undo-one-step" "$V1_OUT/rv2-ed.txt" "SELECT 7 FROM dual;"
expect "r3-no-hunk-left" "$V1_OUT/rv3-st.txt" "No git changes in this tab"
expect "r3-body-head-again" "$V1_OUT/rv3-ed.txt" "SELECT 7 FROM dual;"
# r4. 덩어리 밖(캐럿 1:1)에서 되돌림 → 안내 + 본문 불변
rv rv4 "$GM" "git.revert_hunk,@after:3800:status.dump:$O/rv4-st.txt,@after:4000:editor.dump:$O/rv4-ed.txt,@after:4400:file.exit"
expect "r4-status-no-hunk-at-caret" "$V1_OUT/rv4-st.txt" "No git change at the cursor line"
expect "r4-body-unchanged" "$V1_OUT/rv4-ed.txt" "SELECT 77 FROM dual;"
expect_re "r4-caret-1" "$V1_OUT/rv4-ed.txt" '\|caret=1:1\|'
# r5. 삭제만인 덩어리 — HEAD 5번째 줄이 빠진 파일 · 캐럿을 그 아래 줄(지금 5번째 = "SELECT 6" · 글자 80)로 → 되돌림 → 10줄 · "SELECT 5" 복원
rv rv5 "$DM" "editor.caret:80,@after:3800:git.revert_hunk,@after:4100:status.dump:$O/rv5-st.txt,@after:4300:editor.dump:$O/rv5-ed.txt,@after:4700:file.exit"
expect "r5-status-reverted" "$V1_OUT/rv5-st.txt" "Reverted the git change"
expect "r5-deleted-line-back" "$V1_OUT/rv5-ed.txt" "SELECT 5 FROM dual;"
expect_eq "r5-ten-lines" "$(tail -n +2 "$V1_OUT/rv5-ed.txt" 2>/dev/null | grep -c 'FROM dual;')" "10"
# 디스크 파일 = 그대로(되돌리기는 저장하지 않는다)
expect_eq "r-disk-g-untouched" "$(md5sum < "$R/g.sql")" "$G0"
expect_eq "r-disk-d-untouched" "$(md5sum < "$R/d.sql")" "$D0"
for t in rv1 rv4 rv5; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
