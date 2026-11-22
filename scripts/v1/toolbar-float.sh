#!/usr/bin/env bash
# V1 toolbar-float — 플로팅 툴바 창(T-134 · 손잡이 GRIP_W)이 헤드리스로 뜨고 죽지 않는지 + 다시 붙이기(`tbg:` 토글) 배치 저장.
#   끌어 붙이기(마우스 끌기)는 입력 주입 금지라 자동 판정 대상이 아니다(사용자 실기) — 여기서는 생성·생존·배치 왕복만.
#   ① 설정 `toolbar.layout=file@…` 로 기동 = 저장된 플로팅 복원 ② 기동 명령 `toolbar.pick:tbg:edit`(툴바 우클릭 메뉴 항목 = indent_pick) = 떼어 내기 → 창이 만들어져야 배치에 `edit@x:y`가
#   남는다(창 생성 실패면 도크에 float 표시를 안 함 · toolbar.rs open_float) ③ `tbg:edit` 한 번 더 = 다시 붙이기 → `edit@` 사라짐.
#   창은 헤드리스라 숨김(winfocus::attrs_headless) · 11-22 묶음 5부터 플로팅 창도 `windows.dump`·`capture:<png>@<제목>` 대상(`capture_windows`) →
#   fw-* = 창 자체 판정(덤프 줄 · 창 수 증감 · 캡처 PNG). 창 제목 = `Nexa SQL - <그룹 제목>`(격리 홈 영어 = `Nexa SQL - Edit`).
#   툴팁 글 덤프 명령은 없다(툴팁 판정 = 미판정).
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_set "toolbar.layout=file@200:200"
O=$(v1_m "$V1_OUT")
v1_gui f1 "@after:2000:windows.dump:$O/f1-w0.txt,@after:2500:toolbar.pick:tbg:edit,@after:3200:windows.dump:$O/f1-w1.txt,@after:3500:capture:$O/fw.png@Nexa SQL - Edit,@after:4000:fs.write:$O/f1-alive.txt|alive,@after:4500:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/f1-settings.conf"
expect "float-restored-file" "$V1_OUT/f1-settings.conf" "file@"
expect "float-new-edit" "$V1_OUT/f1-settings.conf" "edit@"
expect "f1-alive-after-float" "$V1_OUT/f1-alive.txt" "alive"
expect_eq "f1-exit-0" "$(cat "$V1_OUT/f1.exit")" "0"
expect_not "f1-no-panic" "$V1_OUT/f1.stderr" "panicked"
wn() { sed -n 's/^wins=//p' "$1" 2>/dev/null | head -1; }
W0=$(wn "$V1_OUT/f1-w0.txt"); W1=$(wn "$V1_OUT/f1-w1.txt")
FL=$(grep -F "Nexa SQL - Edit	" "$V1_OUT/f1-w1.txt" 2>/dev/null | head -1)
if [ -n "$FL" ] && printf '%s' "$FL" | grep -qE '	[1-9][0-9]*x[1-9][0-9]*	(visible|hidden)$'; then v1_pass "fw-window-listed"
else v1_failmsg "fw-window-listed" "줄='$FL' · $(tr '
' ' ' < "$V1_OUT/f1-w1.txt" 2>/dev/null | head -c 200)"; fi
expect_not "fw-not-before" "$V1_OUT/f1-w0.txt" "Nexa SQL - Edit"
if [ -n "$W0" ] && [ "$W1" = "$((W0 + 1))" ]; then v1_pass "fw-count-float"; else v1_failmsg "fw-count-float" "떼기 전 wins=$W0 · 뒤 wins=$W1"; fi
if [ -s "$V1_OUT/fw.png" ] && [ "$(head -c 4 "$V1_OUT/fw.png" | od -An -tx1 | tr -d ' 
')" = "89504e47" ]; then v1_pass "fw-capture-png"
else v1_failmsg "fw-capture-png" "PNG 없음/머리 다름 · $(grep -F '[capture]' "$V1_OUT/f1.stderr" 2>/dev/null | head -2 | tr '
' ' ')"; fi
rm -f "$V1_OUT/fw.png"
v1_gui f2 "@after:2000:windows.dump:$O/f2-w0.txt,@after:2500:toolbar.pick:tbg:edit,@after:3200:windows.dump:$O/f2-w1.txt,@after:3500:toolbar.pick:tbg:file,@after:4200:windows.dump:$O/f2-w2.txt,@after:4500:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/f2-settings.conf"
expect_not "redock-edit" "$V1_OUT/f2-settings.conf" "edit@"
expect_not "redock-file" "$V1_OUT/f2-settings.conf" "file@"
expect_eq "f2-exit-0" "$(cat "$V1_OUT/f2.exit")" "0"
expect_not "f2-no-panic" "$V1_OUT/f2.stderr" "panicked"
V0=$(wn "$V1_OUT/f2-w0.txt"); V1=$(wn "$V1_OUT/f2-w1.txt"); V2=$(wn "$V1_OUT/f2-w2.txt")
if [ -n "$V0" ] && [ "$V1" = "$((V0 - 1))" ] && [ "$V2" = "$((V0 - 2))" ]; then v1_pass "fw-count-redock"; else v1_failmsg "fw-count-redock" "wins $V0 → $V1 → $V2(기대 -1 · -2)"; fi
expect_not "fw-redock-gone" "$V1_OUT/f2-w1.txt" "Nexa SQL - Edit"
if [ -n "$W0" ] && [ "$V2" = "$((W0 - 1))" ]; then v1_pass "fw-count-back"; else v1_failmsg "fw-count-back" "f1 시작 wins=$W0(file 플로팅 포함) · f2 끝 wins=$V2(기대 $((W0 - 1)))"; fi
v1_done
