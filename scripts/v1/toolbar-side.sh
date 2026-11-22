#!/usr/bin/env bash
# V1 toolbar-side — 툴바 그룹 세로 도크(T-105 · D-497 = 창 왼쪽·오른쪽 가장자리로 끌어 붙이기): 헤드리스 · 격리 홈 · 입력 주입 0.
#   끌기 자체(마우스)는 입력 주입 금지라 판정하지 않는다(사용자 실기) — 기동 명령 `toolbar.side:<그룹>=left|right|top` ·
#   우클릭 메뉴 항목 `tbs:<그룹>=…`(`toolbar.pick:`) · 덤프 `toolbar.dump:<파일>`로 자리·사각형·편집기 영역·저장 왕복을 본다.
#   덤프 = 그룹마다 `id|top|left|right|float|hidden|x,y,w,h|vertical=true|false` + 끝 두 줄 `layout=<toolbar.layout>` · `editor=x,y,w,h`.
#   s1 기본(배치 없음) = 모든 그룹 top · 가로        s2 edit=left = 세로(h > w)        s3 run=right = 세로
#   e  편집기 x가 왼쪽 띠 폭만큼 늘고 w가 두 띠 합만큼 준다      s4 배치 `edit@L` · `run@R` 저장 → 다시 기동 = 같은 자리
#   s5 edit=top 복귀 · 배치에서 `edit@L` 빠짐                      m  우클릭 메뉴 = 지금 자리를 뺀 두 항목 · `toolbar.pick:tbs:` 로도 붙음
#   s6 떼어 낸(플로팅) 그룹에 side = 붙는다(플로팅 해제 · 창 수 -1)   s7 view 띠도 세로      s8 옛 배치 값 `edit,file;run@100:200` 그대로 읽힘
#   미판정 = 띠 안 버튼 클릭(툴바 클릭 기동 명령 없음 · 메뉴 명령은 같은 id를 부르므로 버튼 판정과 다름) · 힌트 표시 · 가장자리 영역 28 px.
. "$(dirname "$0")/../lib/v1.sh"
O=$(v1_m "$V1_OUT")
row() { tr -d '\r' < "$1" 2>/dev/null | awk -F'|' -v g="$2" '$1==g{print; exit}'; }
side() { row "$1" "$2" | cut -d'|' -f2; }
rect() { row "$1" "$2" | cut -d'|' -f3; }
vert() { row "$1" "$2" | cut -d'|' -f4 | sed 's/^vertical=//'; }
fld() { printf '%s' "$1" | cut -d, -f"$2"; }
tall() { local r; r=$(rect "$1" "$2"); [ -n "$r" ] && [ "$(fld "$r" 4)" -gt "$(fld "$r" 3)" ] 2>/dev/null && echo y; }
ed() { tr -d '\r' < "$1" 2>/dev/null | sed -n 's/^editor=//p' | head -1; }
wn() { sed -n 's/^wins=//p' "$1" 2>/dev/null | head -1; }
rm -f "$V1_OUT"/ts*.txt "$V1_OUT"/ts*.png
v1_home en

# ── s1~s3 · e · 캡처 ──
v1_gui ts1 "@after:2000:toolbar.dump:$O/ts1.txt,@after:2400:toolbar.side:edit=left,@after:3000:toolbar.dump:$O/ts2.txt,@after:3300:toolbar.side:run=right,@after:3900:toolbar.dump:$O/ts3.txt,@after:4200:capture:$O/ts3.png,@after:4800:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/ts1-settings.conf" 2>/dev/null
expect_eq "s1-dump-ran" "$([ -s "$V1_OUT/ts1.txt" ] && echo y)" "y"
expect_not "s1-no-side-or-float" "$V1_OUT/ts1.txt" "|left|"
expect_not "s1-no-right" "$V1_OUT/ts1.txt" "|right|"
expect_not "s1-all-horizontal" "$V1_OUT/ts1.txt" "vertical=true"
expect_eq "s1-edit-top" "$(side "$V1_OUT/ts1.txt" edit)" "top"
expect_eq "s2-edit-left" "$(side "$V1_OUT/ts2.txt" edit)" "left"
expect_eq "s2-edit-vertical" "$(vert "$V1_OUT/ts2.txt" edit)" "true"
expect_eq "s2-edit-tall" "$(tall "$V1_OUT/ts2.txt" edit)" "y"
expect_eq "s3-run-right" "$(side "$V1_OUT/ts3.txt" run)" "right"
expect_eq "s3-run-tall" "$(tall "$V1_OUT/ts3.txt" run)" "y"
E1=$(ed "$V1_OUT/ts1.txt"); E3=$(ed "$V1_OUT/ts3.txt"); LW=$(fld "$(rect "$V1_OUT/ts3.txt" edit)" 3); RW=$(fld "$(rect "$V1_OUT/ts3.txt" run)" 3)
echo "editor before=$E1 after=$E3 · left band w=$LW · right band w=$RW"
expect_eq "e-editor-x-shift-left-band" "$(( $(fld "$E3" 1) - $(fld "$E1" 1) ))" "$LW"
expect_eq "e-editor-w-shrinks-both" "$(( $(fld "$E1" 3) - $(fld "$E3" 3) ))" "$(( LW + RW ))"
expect "s4-layout-edit-left" "$V1_OUT/ts1-settings.conf" "edit@L"
expect "s4-layout-run-right" "$V1_OUT/ts1-settings.conf" "run@R"
expect_eq "cap-png" "$([ -s "$V1_OUT/ts3.png" ] && [ "$(head -c 4 "$V1_OUT/ts3.png" | od -An -tx1 | tr -d ' \n')" = "89504e47" ] && echo y)" "y"

# ── s4 다시 기동 = 같은 자리 · s5 top 복귀 ──
v1_gui ts4 "@after:2000:toolbar.dump:$O/ts4.txt,@after:2400:toolbar.side:edit=top,@after:3000:toolbar.dump:$O/ts5.txt,@after:3500:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/ts5-settings.conf" 2>/dev/null
expect_eq "s4-restored-edit-left" "$(side "$V1_OUT/ts4.txt" edit)" "left"
expect_eq "s4-restored-run-right" "$(side "$V1_OUT/ts4.txt" run)" "right"
expect_eq "s5-edit-back-top" "$(side "$V1_OUT/ts5.txt" edit)" "top"
expect_eq "s5-edit-horizontal" "$(vert "$V1_OUT/ts5.txt" edit)" "false"
expect_not "s5-layout-no-edit-left" "$V1_OUT/ts5-settings.conf" "edit@L"
expect "s5-layout-keeps-run-right" "$V1_OUT/ts5-settings.conf" "run@R"

# ── m 우클릭 메뉴 · s6 플로팅 → 붙임 · s7 view 띠 ──
v1_home en
v1_gui tm "@after:2000:toolbar.menu:edit.undo,@after:2400:toolbar.menudump:$O/tsm1.txt,@after:2700:toolbar.pick:tbs:edit=left,@after:3300:toolbar.dump:$O/tsm2.txt,@after:3600:toolbar.menu:edit.undo,@after:4000:toolbar.menudump:$O/tsm3.txt,@after:4300:toolbar.pick:tbg:file,@after:5000:toolbar.dump:$O/ts6a.txt,@after:5200:windows.dump:$O/ts6a-w.txt,@after:5500:toolbar.side:file=left,@after:6200:toolbar.dump:$O/ts6b.txt,@after:6400:windows.dump:$O/ts6b-w.txt,@after:6700:toolbar.side:view=left,@after:7300:toolbar.dump:$O/ts7.txt,@after:7800:file.exit" 25
expect "m-top-offers-left" "$V1_OUT/tsm1.txt" "tbs:edit=left"
expect "m-top-offers-right" "$V1_OUT/tsm1.txt" "tbs:edit=right"
expect_not "m-top-no-top" "$V1_OUT/tsm1.txt" "tbs:edit=top"
expect_eq "m-pick-docks-left" "$(side "$V1_OUT/tsm2.txt" edit)" "left"
expect "m-left-offers-right" "$V1_OUT/tsm3.txt" "tbs:edit=right"
expect "m-left-offers-top" "$V1_OUT/tsm3.txt" "tbs:edit=top"
expect_not "m-left-no-left" "$V1_OUT/tsm3.txt" "tbs:edit=left"
expect_eq "s6-file-floating" "$(side "$V1_OUT/ts6a.txt" file)" "float"
expect_eq "s6-side-docks-float" "$(side "$V1_OUT/ts6b.txt" file)" "left"
W6A=$(wn "$V1_OUT/ts6a-w.txt"); W6B=$(wn "$V1_OUT/ts6b-w.txt")
if [ -n "$W6A" ] && [ "$W6B" = "$((W6A - 1))" ]; then v1_pass "s6-float-window-closed"; else v1_failmsg "s6-float-window-closed" "wins $W6A → $W6B(기대 -1)"; fi
expect_eq "s7-view-left" "$(side "$V1_OUT/ts7.txt" view)" "left"
expect_eq "s7-view-vertical" "$(vert "$V1_OUT/ts7.txt" view)" "true"

# ── s8 옛 배치 값 ──
v1_home en
v1_set "toolbar.layout=edit,file;run@100:200"
v1_gui ts8 "@after:2000:toolbar.dump:$O/ts8.txt,@after:2600:file.exit" 25
expect_eq "s8-old-edit-top" "$(side "$V1_OUT/ts8.txt" edit)" "top"
expect_eq "s8-old-file-top" "$(side "$V1_OUT/ts8.txt" file)" "top"
expect_eq "s8-old-run-float" "$(side "$V1_OUT/ts8.txt" run)" "float"
expect_not "s8-old-no-side" "$V1_OUT/ts8.txt" "vertical=true"

# ── s9 옆 띠 그룹 숨김/표시 = 본문 폭 회복(T-105 후속 · nexa-ui 3295bcb Resized 보고 + `drain_dock_actions` · 11-23) ──
#   그룹 숨김 = 설정 `toolbar.items`의 순서/표시 문법 `블록:vis`(`edit:0`) · `config.set:` = 설정 창에서 바꾼 것과 같은 길 · 빈 값 = 기본(전부 표시)
v1_home en
v1_gui ts9 "@after:2000:toolbar.dump:$O/ts9a.txt,@after:2300:toolbar.side:edit=left,@after:2900:toolbar.dump:$O/ts9b.txt,@after:3200:config.set:toolbar.items=edit:0,@after:3900:toolbar.dump:$O/ts9c.txt,@after:4200:config.set:toolbar.items=,@after:4900:toolbar.dump:$O/ts9d.txt,@after:5400:file.exit" 25
X9A=$(fld "$(ed "$V1_OUT/ts9a.txt")" 1); X9B=$(fld "$(ed "$V1_OUT/ts9b.txt")" 1); X9C=$(fld "$(ed "$V1_OUT/ts9c.txt")" 1); X9D=$(fld "$(ed "$V1_OUT/ts9d.txt")" 1)
echo "s9 editor.x: base=$X9A left=$X9B hidden=$X9C shown=$X9D"
expect_eq "s9-left-band-shifts" "$([ -n "$X9B" ] && [ -n "$X9A" ] && [ "$X9B" -gt "$X9A" ] && echo y)" "y"
expect_eq "s9-hidden-back-to-base" "$X9C" "$X9A"
expect_eq "s9-edit-hidden-place" "$(side "$V1_OUT/ts9c.txt" edit)" "hidden"
expect_eq "s9-shown-shifts-again" "$X9D" "$X9B"
expect_eq "s9-shown-still-left" "$(side "$V1_OUT/ts9d.txt" edit)" "left"

for t in ts1 ts4 tm ts8 ts9; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; expect_eq "$t-exit-0" "$(cat "$V1_OUT/$t.exit" 2>/dev/null)" "0"; done
[ "${V1_KEEP_CAPTURE:-0}" = 1 ] || rm -f "$V1_OUT"/ts*.png
v1_done
