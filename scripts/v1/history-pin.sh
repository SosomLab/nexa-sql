#!/usr/bin/env bash
# V1 history-pin — (O-1 · 로컬 히스토리 시점 고정 `edit.history_pin` · docs/19 §3): 헤드리스 · 격리 홈 · 입력 주입 0.
#   a. 파일을 열고 본문을 두 번 바꿔 저장(`editor.paste:<파일>` + `file.save`) → 시점 2개(`*_save_*.sql`)
#   b. `edit.history_pin` → `palette.pick:0` → 상태줄 "Pinned snapshot" · `*_pin.sql` 1개 · 다시 열면 팔레트에 "📌"
#   c. 같은 항목을 다시 → "Unpinned snapshot" · `*_pin.sql` 0개 · 팔레트에 "📌" 없음
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1hp-XXXX")
F="$WK/h.sql"; printf 'SELECT 1 FROM dual;\n' > "$F"
printf -- '-- v1hp edit one\n' > "$WK/p1.txt"; printf -- '-- v1hp edit two\n' > "$WK/p2.txt"
FM=$(v1_m "$F"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
pins() { find "$V1_HOME/history" -name '*_pin.sql' 2>/dev/null | wc -l | tr -d ' '; }
saves() { find "$V1_HOME/history" -name '*.sql' 2>/dev/null | wc -l | tr -d ' '; }
rm -f "$V1_OUT"/hp-*.txt

# a. 시점 두 개
v1_gui hpa "open:$FM,@after:1200:editor.paste:$W/p1.txt,@after:1600:file.save,@after:2200:editor.paste:$W/p2.txt,@after:2600:file.save,@after:3200:edit.history,@after:3600:palette.dump:$O/hp-a-pal.txt,@after:4000:file.exit" 25
expect_eq "a-two-snapshots" "$(saves)" "2"
expect_eq "a-no-pin-yet" "$(pins)" "0"
expect_not "a-palette-no-pin-mark" "$V1_OUT/hp-a-pal.txt" "📌"
# b. 고정 켜기
v1_gui hpb "open:$FM,@after:1200:edit.history_pin,@after:1600:palette.dump:$O/hp-b-pal0.txt,@after:1900:palette.pick:0,@after:2300:status.dump:$O/hp-b-status.txt,@after:2700:edit.history,@after:3100:palette.dump:$O/hp-b-pal.txt,@after:3500:file.exit" 25
expect "b-pin-palette-opened" "$V1_OUT/hp-b-pal0.txt" "hpin:"
expect "b-status-pinned" "$V1_OUT/hp-b-status.txt" "Pinned snapshot"
expect_eq "b-one-pin-file" "$(pins)" "1"
expect_eq "b-still-two-snapshots" "$(saves)" "2"
expect "b-palette-pin-mark" "$V1_OUT/hp-b-pal.txt" "📌"
# c. 같은 항목 다시 = 고정 끄기(팔레트 0번 = 같은 시점 · 목록 순서는 시각순이라 이름 변경과 무관)
v1_gui hpc "open:$FM,@after:1200:edit.history_pin,@after:1600:palette.pick:0,@after:2000:status.dump:$O/hp-c-status.txt,@after:2400:edit.history,@after:2800:palette.dump:$O/hp-c-pal.txt,@after:3200:file.exit" 25
expect "c-status-unpinned" "$V1_OUT/hp-c-status.txt" "Unpinned snapshot"
expect_eq "c-no-pin-file" "$(pins)" "0"
expect_eq "c-still-two-snapshots" "$(saves)" "2"
expect_eq "c-dump-ran" "$([ -s "$V1_OUT/hp-c-pal.txt" ] && echo y)" "y"
expect_not "c-palette-no-pin-mark" "$V1_OUT/hp-c-pal.txt" "📌"
for t in hpa hpb hpc; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
