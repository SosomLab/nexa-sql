#!/usr/bin/env bash
# V1 prefs-hidden — T-66 1/3(D-515 · 11-24 8045acdc) 비노출 설정 26개: `config list all`에는 26개 모두 · 설정 창 검색(edit.prefs)에는 0개.
#   대조 = 같은 검색에 보이는 노출 키(grid.null_text · editor.font_size)가 있어야 한다(검색이 키에 걸리는지 = 0개 판정이 헛돌지 않게).
. "$(dirname "$0")/../lib/v1.sh"
KEYS="log.tip_ms log.pad log.row_extra log.switch_track_w log.switch_gap log.switch_label_extra log.wheel_div log.window_w log.window_h log.menu_min_w editor.tab_tip_ms editor.close_confirm_ms editor.sections_settle_ms editor.detect_indent_lines editor.badge_menu_w editor.tab_menu_w grid.cell_pad grid.chip_strip_h grid.page_box_w grid.insert_band_w grid.autosize_rows grid.autofit_rows grid.type_sample_rows grid.dblclick_ms grid.mark_tip_ms grid.result_menu_w"
v1_home en
O=$(v1_m "$V1_OUT")
v1_cli config list all > "$V1_OUT/config.txt"
n=0; miss=""; for k in $KEYS; do if grep -qE "^${k//./\.} " "$V1_OUT/config.txt"; then n=$((n+1)); else miss="$miss $k"; fi; done
expect_eq "cli-26-keys${miss:+ (없음:$miss)}" "$n" "26"
for q in log grid editor; do
  v1_gui "ph-$q" "edit.prefs:$q.,@after:3000:prefs.dump:$O/ph-$q.txt,@after:3400:file.exit" 20
  h=0; for k in $KEYS; do case $k in $q.*) grep -qxF "$k" "$V1_OUT/ph-$q.txt" 2>/dev/null && h=$((h+1));; esac; done
  expect_eq "prefs-$q-hidden-0" "$h" "0"
  expect_not "ph-$q-no-panic" "$V1_OUT/ph-$q.stderr" "panicked"
done
expect "prefs-grid-search-works" "$V1_OUT/ph-grid.txt" "grid.null_text"
expect "prefs-editor-search-works" "$V1_OUT/ph-editor.txt" "editor.font_size"
v1_done
