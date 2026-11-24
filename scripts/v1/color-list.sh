#!/usr/bin/env bash
# V1 color-list — T-119 색 창 목록 모드(`colors.open:` · `colors.op:` · `colors.dump:` · 격리 홈).
#   a 빈 설정 = 테마 6색 · b set → 설정 6항목(첫 항목 교체) · c add/del/down/up/sel:99 → 설정 반영 ·
#   d 실제 괄호 색(저장소의 rainbow-pairs WASM을 격리 홈에 설치 · 마젠타 픽셀 · python + Pillow) · e 키 모드·기본 모드 유지 · f 색 창 캡처가 만들어지는지.
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
expect_re "a-list-mode" "$V1_OUT/cl-a.txt" "^mode=list key=$K sel=0 n=6 dropped=0 colors="
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
# ── d. 실제 괄호 색(11-22 nexa-ui marks 병합 수정 뒤 되살림 · 협업 초안) ──
#   Rainbow Pairs는 설치 기록 없는 내장이면 효과가 없다(사용자 09-17 · app/extensions.rs:46·87-92) → 저장소의 WASM 패키지
#   (`extensions/rainbow-pairs/{extension.json,rainbow_pairs.wasm}` · git 추적됨)로 격리 홈에 설치 기록을 만들고 `extensions.enabled=on`.
#   판정 = 괄호 영역 픽셀: 사용자 색 #FF00FF×6 = 마젠타 > 0 · 무지개 끔 = 0 · 확장 없음(기본) = 0. 대조 셋은 같은 본문.
EXTSRC="$V1_ROOT/extensions/rainbow-pairs"
EXTVER=$(grep -oE '"version": *"[^"]+"' "$EXTSRC/extension.json" | head -1 | sed -E 's/.*"([^"]+)"$/\1/')
mkinst() { mkdir -p "$V1_HOME/extensions/rainbow-pairs/$EXTVER"; cp "$EXTSRC/extension.json" "$EXTSRC/rainbow_pairs.wasm" "$V1_HOME/extensions/rainbow-pairs/$EXTVER/"
  printf '{\n  "format": 1,\n  "id": "rainbow-pairs",\n  "name": "Rainbow Pairs",\n  "version": "%s",\n  "kind": "wasm",\n  "placed": []\n}\n' "$EXTVER" > "$V1_HOME/extensions/rainbow-pairs/installed.json"; }
printf 'SELECT ((((1)))) FROM ((((dual))));\nSELECT (((((2))))) FROM dual;\n' > "$V1_OUT/b.sql"
cap() { local t=$1; shift; v1_home en; for kv in "$@"; do [ "$kv" = INST ] && mkinst || v1_set "$kv"; done
  rm -f "$V1_OUT/$t.png"; v1_gui "$t" "open:$O/b.sql,@after:2500:capture:$O/$t.png,@after:3000:file.exit" 20; rm -rf "$V1_HOME"; V1_HOME=""; }
cap cld1 "extensions.enabled=on" INST "$K=#FF00FF,#FF00FF,#FF00FF,#FF00FF,#FF00FF,#FF00FF"
cap cld2 "extensions.enabled=on" INST "$K=#FF00FF,#FF00FF,#FF00FF,#FF00FF,#FF00FF,#FF00FF" "ext.rainbow_pairs.enabled=off"
cap cld0
PY=$(command -v python3 || command -v python); MP=$(v1_m "$V1_ROOT/scripts/lib/magenta_px.py")
mp() { "$PY" -I "$MP" "$(v1_m "$1")" 2>/dev/null | tail -1 | tr -d '\r'; }
D1=$(mp "$V1_OUT/cld1.png"); D2=$(mp "$V1_OUT/cld2.png"); D0=$(mp "$V1_OUT/cld0.png")
echo "bracket magenta px: on=$D1 rainbow_off=$D2 no_ext=$D0"
expect_eq "d-brackets-magenta" "$([ "${D1:-0}" -gt 50 ] 2>/dev/null && echo y)" "y"
expect_eq "d-rainbow-off-none" "$D2" "0"
expect_eq "d-no-extension-none" "$D0" "0"
for t in cld1 cld2 cld0; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
[ "${V1_KEEP_CAPTURE:-0}" = 1 ] || rm -f "$V1_OUT"/cld*.png
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
