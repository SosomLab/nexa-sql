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
  v1_home en; v1_set "statusbar.git_secs=1" "editor.git_gutter=on" ${RV_SET:-}
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

# ── 둘째 독립 검토 후속(11-23 묶음 18 · C-1) ──
#   r6 탭을 a(g.sql · 7행 수정) → b(미추적 · 7행이 다른 글)로 바꾼 **직후** 7행에서 되돌림 → b 본문 불변 · "No git change at the cursor line"
#      (종전 = 감시가 a의 HEAD를 들고 있어 b 7행을 a의 HEAD 글로 바꿀 수 있었다)
#   r7 UTF-8 BOM으로 커밋한 파일(수정 없음) 1행에서 되돌림 → 본문 첫 글자가 U+FEFF가 아님 · 덩어리 0
#   r8 EUC-KR 한글 파일을 커밋 → 작업본 한 줄 수정 → 열어 euckr로 다시 열기(`toolbar.pick:enc.reopen:euckr` · T-512) → 한글 · U+FFFD 없음 →
#      되돌림 → "not valid UTF-8" 거부(HEAD 쪽이 깨져 있으므로) · 되돌리기 전후 본문 같음(11-23 묶음 18 첫 판 = T-512 때문에 한글 판정 불가 → 묶음 19에서 되살림)
for i in 1 2 3 4 5 6 7 8 9 10; do [ "$i" = 7 ] && echo "SELECT 700 FROM dual;" || echo "SELECT $i FROM dual;"; done > "$R/u.sql"
printf '\xef\xbb\xbfSELECT 1 FROM dual;\nSELECT 2 FROM dual;\n' > "$R/bom.sql"
printf "SELECT '한글 하나' FROM dual;\nSELECT '한글 둘' FROM dual;\nSELECT '한글 셋' FROM dual;\n" | iconv -f UTF-8 -t CP949 > "$R/k.sql"
git -C "$R" add bom.sql k.sql && git -C "$R" -c user.name=v1 -c user.email=v1@example.invalid commit -q -m enc
printf "SELECT '한글 하나' FROM dual;\nSELECT '한글 둘 고침' FROM dual;\nSELECT '한글 셋' FROM dual;\n" | iconv -f UTF-8 -t CP949 > "$R/k.sql"
UM=$(v1_m "$R/u.sql"); BM=$(v1_m "$R/bom.sql"); KM=$(v1_m "$R/k.sql")
rv rv6 "$GM" "open:$UM,@after:5000:tab.prev,@after:7000:tab.next,@after:7020:editor.caret:120,@after:7040:git.revert_hunk,@after:7300:status.dump:$O/rv6-st.txt,@after:7500:editor.dump:$O/rv6-ed.txt,@after:7900:file.exit,@after:8400:toolbar.pick:project.exit_skip"
expect "r6-other-file-body-unchanged" "$V1_OUT/rv6-ed.txt" "SELECT 700 FROM dual;"
expect "r6-status-no-hunk" "$V1_OUT/rv6-st.txt" "No git change at the cursor line"
rv rv7 "$BM" "git.revert_hunk,@after:3800:editor.dump:$O/rv7-ed.txt,@after:4000:git.next_hunk,@after:4200:status.dump:$O/rv7-st.txt,@after:4600:file.exit"
if tail -n +2 "$V1_OUT/rv7-ed.txt" 2>/dev/null | head -c 3 | od -An -tx1 | grep -q 'ef bb bf'; then v1_failmsg "r7-no-bom-char-in-body" "첫 글자 = U+FEFF"; else v1_pass "r7-no-bom-char-in-body"; fi
expect "r7-body-first-line" "$V1_OUT/rv7-ed.txt" "SELECT 1 FROM dual;"
expect "r7-no-hunks" "$V1_OUT/rv7-st.txt" "No git changes in this tab"
rv rv8 "$KM" "toolbar.pick:enc.reopen:euckr,@after:5000:editor.caret:30,@after:5100:editor.dump:$O/rv8-ed0.txt,@after:5200:git.revert_hunk,@after:5500:status.dump:$O/rv8-st.txt,@after:5700:editor.dump:$O/rv8-ed.txt,@after:6100:file.exit,@after:6600:toolbar.pick:project.exit_skip"
expect "r8-refuses-not-utf8" "$V1_OUT/rv8-st.txt" "not valid UTF-8"
expect "r8-reopened-korean" "$V1_OUT/rv8-ed0.txt" "한글 둘 고침"
if LC_ALL=C grep -q $'�' "$V1_OUT/rv8-ed.txt" 2>/dev/null; then v1_failmsg "r8-no-replacement-char" "본문에 U+FFFD"; else v1_pass "r8-no-replacement-char"; fi
expect_eq "r8-body-unchanged-by-revert" "$(tail -n +2 "$V1_OUT/rv8-ed.txt" 2>/dev/null | md5sum)" "$(tail -n +2 "$V1_OUT/rv8-ed0.txt" 2>/dev/null | md5sum)"
for t in rv6 rv7 rv8; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
# ── C-1 되돌리기 진입점 둘(D-520 · 11-23 d83b8543) ──
#   r9  팝업 [되돌리기]: `git.popup:7` → 열림 · line=7 · 버튼 글 있음 → `githunk.revert`(버튼과 같은 함수) → 본문 = HEAD · 팝업 닫힘 → undo 한 번 = 고친 글
#   r9b 팝업을 연 뒤 본문을 바꾸면(editor.paste) 판이 달라져 되돌리지 않고 안내 · 본문의 7행 고친 글 그대로
#   r10 거터 우클릭 `gutter.menu:7` → 메뉴 마지막 = bmg.git_revert → 고르면 본문 = HEAD
#   r10b 변경 없는 줄(2행) 메뉴 → 항목은 있으나 고르면 본문 불변(비활성 · menudump는 활성 여부를 못 보임)
#   r10c bookmark.enabled=off → 메뉴 = bmg.git_revert 하나
echo "-- pasted" > "$WK/p.txt"; PM=$(v1_m "$WK/p.txt")
rv rv9 "$GM" "git.popup:7,@after:3800:githunk.dump:$O/rv9-p0.txt,@after:3900:githunk.revert,@after:4200:editor.dump:$O/rv9-ed.txt,@after:4300:githunk.dump:$O/rv9-p1.txt,@after:4500:edit.undo,@after:4800:editor.dump:$O/rv9-ed2.txt,@after:5200:file.exit,@after:5700:toolbar.pick:project.exit_skip"
expect "r9-popup-open" "$V1_OUT/rv9-p0.txt" "open=true"
expect "r9-popup-line-7" "$V1_OUT/rv9-p0.txt" "line=7"
expect_re "r9-btn-label" "$V1_OUT/rv9-p0.txt" '^btn_label=.+'
expect "r9-body-head" "$V1_OUT/rv9-ed.txt" "SELECT 7 FROM dual;"
expect_not "r9-body-no-edit" "$V1_OUT/rv9-ed.txt" "SELECT 77 FROM dual;"
expect "r9-popup-closed" "$V1_OUT/rv9-p1.txt" "open=false"
expect "r9-undo-brings-edit" "$V1_OUT/rv9-ed2.txt" "SELECT 77 FROM dual;"
rv rv9b "$GM" "git.popup:7,@after:3800:editor.paste:$PM,@after:4000:githunk.revert,@after:4300:status.dump:$O/rv9b-st.txt,@after:4500:editor.dump:$O/rv9b-ed.txt,@after:4600:githunk.dump:$O/rv9b-p.txt,@after:5000:file.exit,@after:5500:toolbar.pick:project.exit_skip"
expect "r9b-pasted" "$V1_OUT/rv9b-ed.txt" "-- pasted"
expect "r9b-edit-kept" "$V1_OUT/rv9b-ed.txt" "SELECT 77 FROM dual;"
expect "r9b-status-notice" "$V1_OUT/rv9b-st.txt" "No git change at the cursor line"
expect "r9b-popup-closed" "$V1_OUT/rv9b-p.txt" "open=false"
rv rv10 "$GM" "gutter.menu:7,@after:3800:toolbar.menudump:$O/rv10-m.txt,@after:3900:toolbar.pick:bmg.git_revert,@after:4200:editor.dump:$O/rv10-ed.txt,@after:4600:file.exit,@after:5100:toolbar.pick:project.exit_skip"
expect_eq "r10-menu-last-git-revert" "$(grep -v '^$' "$V1_OUT/rv10-m.txt" 2>/dev/null | tail -1)" "bmg.git_revert"
expect_re "r10-menu-has-bookmark" "$V1_OUT/rv10-m.txt" '^bmg\.(toggle|set|add)'
expect "r10-body-head" "$V1_OUT/rv10-ed.txt" "SELECT 7 FROM dual;"
expect_not "r10-body-no-edit" "$V1_OUT/rv10-ed.txt" "SELECT 77 FROM dual;"
rv rv10b "$GM" "gutter.menu:2,@after:3800:toolbar.menudump:$O/rv10b-m.txt,@after:3900:toolbar.pick:bmg.git_revert,@after:4200:editor.dump:$O/rv10b-ed.txt,@after:4600:file.exit,@after:5100:toolbar.pick:project.exit_skip"
expect "r10b-menu-has-item" "$V1_OUT/rv10b-m.txt" "bmg.git_revert"
expect "r10b-body-unchanged" "$V1_OUT/rv10b-ed.txt" "SELECT 77 FROM dual;"
RV_SET="bookmark.enabled=off" rv rv10c "$GM" "gutter.menu:7,@after:3800:toolbar.menudump:$O/rv10c-m.txt,@after:4200:file.exit"
expect_eq "r10c-menu-only-git" "$(grep -v '^$' "$V1_OUT/rv10c-m.txt" 2>/dev/null | tr '\n' '|')" "bmg.git_revert|"
expect_eq "r-disk-g-untouched-2" "$(md5sum < "$R/g.sql")" "$G0"
for t in rv9 rv9b rv10 rv10b rv10c; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
