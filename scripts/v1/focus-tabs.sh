#!/usr/bin/env bash
# V1 focus-tabs — 활성 탭 · 키 포커스(T-516 · 11-23 41e69cb5 `focus.dump`/`focus.type`): AI 분할 창에 포커스가 있다가 다른 탭으로 가면
#   키가 그 탭 편집기로 가야 한다(사용자 결함 ③). 헤드리스 · 격리 홈 · 입력 주입 0(focus.type = 지금 키를 받는 글 상자에 글자 사건).
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1ft-XXXX")
echo "SELECT 1 FROM dual;" > "$WK/a.sql"; echo "SELECT 2 FROM dual;" > "$WK/b.sql"
AM=$(v1_m "$WK/a.sql"); BM=$(v1_m "$WK/b.sql"); O=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/ft-*.txt
v1_home en
# 탭 A(a.sql) · 탭 B(b.sql) → A로 → 분할 창 켬(ai.side · AI가 꺼져 있으면 켜고 연다) → 덤프 → tab.next(B · 분할 창 없음) → 덤프 → 글자 xyz → B 본문
v1_gui ft "open:$AM,@after:2500:open:$BM,@after:4500:tab.prev,@after:5000:focus.dump:$O/ft-0.txt,@after:5300:ai.side,@after:6500:focus.dump:$O/ft-1.txt,@after:6800:tab.next,@after:7300:focus.dump:$O/ft-2.txt,@after:7500:focus.type:xyz,@after:7900:editor.dump:$O/ft-b.txt,@after:8100:tab.prev,@after:8500:editor.dump:$O/ft-a.txt,@after:9000:file.exit,@after:9500:toolbar.pick:project.exit_skip" 30
expect "f0-editor-before" "$V1_OUT/ft-0.txt" "key_target=editor"
expect "f1-side-on" "$V1_OUT/ft-1.txt" "side_on=true"
expect "f1-side-focus" "$V1_OUT/ft-1.txt" "side_focus=true"
expect "f1-key-ai" "$V1_OUT/ft-1.txt" "key_target=ai_view"
expect "f2-side-focus-dropped" "$V1_OUT/ft-2.txt" "side_focus=false"
expect "f2-side-off-in-b" "$V1_OUT/ft-2.txt" "side_on=false"
expect "f2-editor-focused" "$V1_OUT/ft-2.txt" "editor_focused=true"
expect "f2-key-editor" "$V1_OUT/ft-2.txt" "key_target=editor"
expect "f3-typed-into-b" "$V1_OUT/ft-b.txt" "xyz"
expect_not "f3-not-into-a" "$V1_OUT/ft-a.txt" "xyz"
expect_not "ft-no-panic" "$V1_OUT/ft.stderr" "panicked"
rm -rf "$WK"
v1_done
