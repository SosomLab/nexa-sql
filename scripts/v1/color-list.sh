#!/usr/bin/env bash
# V1 color-list — T-119 색 창 목록 모드(`colors.open:` · `colors.op:` · `colors.dump:` · 격리 홈).
#   a 빈 설정 = 테마 6색 · b set → 설정 6항목(첫 항목 교체) · c add/del/down/up/sel:99 → 설정 반영 ·
#   e 키 모드·기본 모드 유지 · f 색 창 캡처가 만들어지는지. 실제 괄호 색 판정은 넣지 않았다(11-22: 헤드리스 캡처에 괄호 색이 안 보임 = 원인 미확정).
. "$(dirname "$0")/../lib/v1.sh"
mkdir -p "$V1_OUT"
O=$(v1_m "$V1_OUT")
K=ext.rainbow_pairs.colors
field() { tr -d '\r' < "$1" 2>/dev/null | sed -n "s/.*$2=\([^ ]*\).*/\1/p" | head -1; }
conf() { grep "^$K=" "$1/settings.conf" 2>/dev/null | head -1 | cut -d= -f2- | tr -d '\r'; }
items() { printf '%s' "$1" | awk -F, '{print NF}'; }
rm -f "$V1_OUT"/cl-*.txt
# ── a·b. 열기(빈 설정) → set ──
v1_home en
v1_gui cla "colors.open:$K,@after:900:colors.dump:$O/cl-a.txt,@after:1200:colors.op:set:#FF00FF,@after:1500:colors.dump:$O/cl-b.txt,@after:1900:file.exit" 20
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 200 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_re "a-list-mode" "$V1_OUT/cl-a.txt" "^mode=list key=$K sel=0 n=6 colors="
CB=$(conf "$V1_HOME")
expect_re "b-dump-first-magenta" "$V1_OUT/cl-b.txt" 'colors=#FF00FF,'
expect_eq "b-conf-6-items" "$(items "$CB")" "6"
expect_eq "b-conf-first-magenta" "${CB%%,*}" "#FF00FF"
expect_eq "b-conf-matches-dump" "$CB" "$(field "$V1_OUT/cl-b.txt" colors)"
# ── c. add → del → down → up → sel:99(같은 홈 · 앞 설정 이어감) ──
v1_gui clc "colors.open:$K,@after:900:colors.op:add,@after:1100:colors.dump:$O/cl-c1.txt,@after:1300:colors.op:del,@after:1500:colors.dump:$O/cl-c2.txt,@after:1700:colors.op:sel:0,@after:1800:colors.op:down,@after:2000:colors.dump:$O/cl-c3.txt,@after:2200:colors.op:up,@after:2400:colors.dump:$O/cl-c4.txt,@after:2600:colors.op:sel:99,@after:2800:colors.dump:$O/cl-c5.txt,@after:3200:file.exit" 20
expect_eq "c-add-n7" "$(field "$V1_OUT/cl-c1.txt" n)" "7"
expect_eq "c-add-sel1" "$(field "$V1_OUT/cl-c1.txt" sel)" "1"
expect_eq "c-del-n6" "$(field "$V1_OUT/cl-c2.txt" n)" "6"
C3=$(field "$V1_OUT/cl-c3.txt" colors); C4=$(field "$V1_OUT/cl-c4.txt" colors)
expect_eq "c-down-moves-first" "$(printf '%s' "$C3" | cut -d, -f2)" "#FF00FF"
expect_eq "c-up-restores" "${C4%%,*}" "#FF00FF"
expect_eq "c-sel99-no-change" "$(field "$V1_OUT/cl-c5.txt" colors) $(field "$V1_OUT/cl-c5.txt" sel)" "$C4 $(field "$V1_OUT/cl-c4.txt" sel)"
expect_eq "c-conf-reflects-last" "$(conf "$V1_HOME")" "$C4"
rm -rf "$V1_HOME"; V1_HOME=""
# ── e. 키 모드 · 기본 모드 ──
v1_home en
v1_gui cle "colors.open:editor.syntax_keyword_color,@after:900:colors.dump:$O/cl-e1.txt,@after:1300:colors.open:ui.hover_color,@after:1700:colors.dump:$O/cl-e2.txt,@after:2100:file.exit" 20
expect_re "e-key-mode" "$V1_OUT/cl-e1.txt" '^mode=key key=editor.syntax_keyword_color '
expect_re "e-default-mode" "$V1_OUT/cl-e2.txt" '^mode=default'
# ── f. 색 창 캡처(목록 모드 · 제목 "Colors" 또는 설정 라벨) ──
v1_gui clf "colors.open:$K,@after:1200:windows.dump:$O/cl-f-wins.txt,@after:1500:capture:$O/cl-f.png@colors,@after:2000:file.exit" 20
rm -rf "$V1_HOME"; V1_HOME=""
expect_eq "f-capture-exists" "$([ -s "$V1_OUT/cl-f.png" ] && echo y)" "y"
for t in cla clc cle clf; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
v1_done
