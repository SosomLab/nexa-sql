#!/usr/bin/env bash
# V1 ext-diff — 외부 변경 띠 [차이 보기] = 좌우 비교 탭(T-140 · C-1 `open_diff_tab`) 판정.
#   미저장 편집이 있는 탭이라야 띠가 뜬다(깨끗하면 `file.external_change=auto`가 조용히 다시 읽음) → `editor.paste` 로 더럽힘 →
#   `fs.write` 로 디스크 본문을 바꿈 → 폴링(500 ms) 뒤 `ext.banner:diff` → `tabs.dump` 에 뷰 탭(view=true) · `compare.dump` hunks ≥ 1.
. "$(dirname "$0")/../lib/v1.sh"
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 300 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_not_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_failmsg "$1" "/$3/ 있음"; else v1_pass "$1"; fi; }
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1xd-XXXX")
F="$WK/x.sql"; printf 'SELECT 1 FROM dual;\nSELECT 2 FROM dual;\n' > "$F"
printf -- '-- local edit\n' > "$WK/p.txt"
FM=$(v1_m "$F"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/xd*.txt
run() { local tag=$1 cmds=$2; v1_home en; v1_set "file.external_poll_ms=500" "file.external_check=focus_poll"
  v1_gui "$tag" "open:$FM,@after:1200:editor.paste:$W/p.txt,$cmds" 25; rm -rf "$V1_HOME"; V1_HOME=""; }
# xd1 = 디스크 변경 → 띠 → [차이 보기]
run xd1 "@after:1600:fs.write:$FM|SELECT 1 FROM dual;\\nSELECT 22 FROM dual;\\n,@after:3600:tabs.dump:$O/xd1-tabs0.txt,@after:3800:ext.banner:diff,@after:4300:tabs.dump:$O/xd1-tabs.txt,@after:4500:compare.dump:$O/xd1-cmp.txt,@after:4700:status.dump:$O/xd1-st.txt,@after:5200:file.exit"
expect_not_re "xd1-no-view-before" "$V1_OUT/xd1-tabs0.txt" 'view=true'
expect_re "xd1-diff-view-tab" "$V1_OUT/xd1-tabs.txt" 'view=true'
# compare.dump = 첫 줄 `none`(T-283 객체 비교 상태) + 비교 탭 줄 `rows=… replace=… hunks=N …` — 디스크 쪽 바뀐 줄(2 → 22) = replace ≥ 1 · 지역 편집 = editor_only ≥ 1
expect_re "xd1-compare-hunks" "$V1_OUT/xd1-cmp.txt" ' hunks=[1-9]'
expect_re "xd1-disk-change-replace" "$V1_OUT/xd1-cmp.txt" ' replace=[1-9]'
expect_re "xd1-local-edit-left" "$V1_OUT/xd1-cmp.txt" ' editor_only=[1-9]'
expect_re "xd1-tab-title-disk" "$V1_OUT/xd1-tabs.txt" '⇄ x\.sql \(disk\)\|.*view=true\|ro=true'
# (대조 없음) 띠가 없어도 `ext.banner:diff` 기동 명령은 디스크와의 비교 탭을 연다(띠 버튼 자체는 띠가 있을 때만 화면에 있음 · 종전 안내 탭 길도 같았음)
#   → "띠 없음 = 뷰 탭 없음" 대조는 성립하지 않아 뺐다(xd1-no-view-before가 변경 감지 전 상태를 대신 본다).
for t in xd1; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
