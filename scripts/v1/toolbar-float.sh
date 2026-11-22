#!/usr/bin/env bash
# V1 toolbar-float — 플로팅 툴바 창(T-134 · 손잡이 GRIP_W)이 헤드리스로 뜨고 죽지 않는지 + 다시 붙이기(`tbg:` 토글) 배치 저장.
#   끌어 붙이기(마우스 끌기)는 입력 주입 금지라 자동 판정 대상이 아니다(사용자 실기) — 여기서는 생성·생존·배치 왕복만.
#   ① 설정 `toolbar.layout=file@…` 로 기동 = 저장된 플로팅 복원 ② 기동 명령 `toolbar.pick:tbg:edit`(툴바 우클릭 메뉴 항목 = indent_pick) = 떼어 내기 → 창이 만들어져야 배치에 `edit@x:y`가
#   남는다(창 생성 실패면 도크에 float 표시를 안 함 · toolbar.rs open_float) ③ `tbg:edit` 한 번 더 = 다시 붙이기 → `edit@` 사라짐.
#   창은 헤드리스라 숨김(winfocus::attrs_headless) · 플로팅 창은 `windows.dump`·`capture:@제목` 목록(all_windows)에 없어 캡처 판정 불가.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_set "toolbar.layout=file@200:200"
v1_gui f1 "@after:2500:toolbar.pick:tbg:edit,@after:4000:fs.write:$(v1_m "$V1_OUT")/f1-alive.txt|alive,@after:4500:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/f1-settings.conf"
expect "float-restored-file" "$V1_OUT/f1-settings.conf" "file@"
expect "float-new-edit" "$V1_OUT/f1-settings.conf" "edit@"
expect "f1-alive-after-float" "$V1_OUT/f1-alive.txt" "alive"
expect_eq "f1-exit-0" "$(cat "$V1_OUT/f1.exit")" "0"
expect_not "f1-no-panic" "$V1_OUT/f1.stderr" "panicked"
v1_gui f2 "@after:2500:toolbar.pick:tbg:edit,@after:3500:toolbar.pick:tbg:file,@after:4500:file.exit" 25
cp "$V1_HOME/settings.conf" "$V1_OUT/f2-settings.conf"
expect_not "redock-edit" "$V1_OUT/f2-settings.conf" "edit@"
expect_not "redock-file" "$V1_OUT/f2-settings.conf" "file@"
expect_eq "f2-exit-0" "$(cat "$V1_OUT/f2.exit")" "0"
expect_not "f2-no-panic" "$V1_OUT/f2.stderr" "panicked"
v1_done
