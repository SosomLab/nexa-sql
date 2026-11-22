#!/usr/bin/env bash
# V1 enc-reopen — 인코딩으로 다시 열기(T-512 · 상태줄 인코딩 팝업 "다시 열기 ▸" · 기동 명령 `toolbar.pick:enc.reopen:<id>` = 팝업 항목 길 · 11-23):
#   헤드리스 · 격리 홈 · 입력 주입 0 · 임시 파일만 쓴다.
#   e1 EUC-KR 한글 파일 = 처음엔 UTF-8로 읽혀 대체 문자 → 다시 열기(euckr) → 한글 · U+FFFD 없음 · 상태줄 "Reopened as Korean …" · unsaved=false
#      → `edit.undo` 한 번 = 대체 문자 본문으로 돌아감(되돌리기 한 단계)            (종전 = 이미 열린 파일이면 아무 일 없음)
#   e2 미저장 편집이 있으면 거부 = 상태줄 "the tab has unsaved edits" · 붙여 넣은 글 그대로 · 다시 디코드 안 됨
#   e3 다시 연 뒤 고쳐 저장(`file.save`) = 파일 바이트가 EUC-KR 그대로(CP949로 풀면 한글 + 고친 글 · UTF-8로는 안 풀림)
#   e4 UTF-8 파일을 utf8로 다시 열기 = 본문 같음 · "Reopened as"      e5 한국어 UI = 상태줄 한글
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1er-XXXX")
mkk() { printf "SELECT '한글 하나' FROM dual;\nSELECT '한글 둘' FROM dual;\n" | iconv -f UTF-8 -t CP949 > "$WK/k.sql"; }
mkk; printf "SELECT 'plain' FROM dual;\n" > "$WK/u.sql"; printf -- '-- v1er edit\n' > "$WK/p.txt"
KM=$(v1_m "$WK/k.sql"); UM=$(v1_m "$WK/u.sql"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
RE="toolbar.pick:enc.reopen"
hasrep() { LC_ALL=C grep -q $'\xef\xbf\xbd' "$1" 2>/dev/null; }
rm -f "$V1_OUT"/er-*.txt
run() { local tag=$1 lang=$2 cmds=$3; [ -n "$V1_HOME" ] && rm -rf "$V1_HOME"; v1_home "$lang"; v1_gui "$tag" "$cmds" 20; }
# e1
run er1 en "open:$KM,@after:1500:editor.dump:$O/er-1a.txt,@after:1800:$RE:euckr,@after:2600:editor.dump:$O/er-1b.txt,@after:2700:status.dump:$O/er-1s.txt,@after:2800:tabs.dump:$O/er-1t.txt,@after:3000:edit.undo,@after:3400:editor.dump:$O/er-1c.txt,@after:3800:file.exit,@after:4300:toolbar.pick:project.exit_skip"
if hasrep "$V1_OUT/er-1a.txt"; then v1_pass "e1-before-replacement-chars"; else v1_failmsg "e1-before-replacement-chars" "처음 열 때 대체 문자 없음(전제 다름)"; fi
expect "e1-after-korean" "$V1_OUT/er-1b.txt" "한글 둘"
if hasrep "$V1_OUT/er-1b.txt"; then v1_failmsg "e1-after-no-replacement" "다시 연 뒤에도 U+FFFD"; else v1_pass "e1-after-no-replacement"; fi
expect "e1-status-reopened" "$V1_OUT/er-1s.txt" "Reopened as Korean"
expect_not "e1-status-not-lossy" "$V1_OUT/er-1s.txt" "some bytes are not valid"
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 200 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_re "e1-tab-not-dirty" "$V1_OUT/er-1t.txt" '^\*?k\.sql\|.*unsaved=false'
if hasrep "$V1_OUT/er-1c.txt"; then v1_pass "e1-undo-one-step-back"; else v1_failmsg "e1-undo-one-step-back" "undo 뒤 대체 문자 본문이 아님"; fi
# e2
mkk
run er2 en "open:$KM,@after:1500:editor.paste:$W/p.txt,@after:1900:$RE:euckr,@after:2600:editor.dump:$O/er-2.txt,@after:2700:status.dump:$O/er-2s.txt,@after:3100:file.exit,@after:3600:toolbar.pick:project.exit_skip"
expect "e2-status-unsaved-refused" "$V1_OUT/er-2s.txt" "the tab has unsaved edits"
expect "e2-edit-kept" "$V1_OUT/er-2.txt" "-- v1er edit"
expect_not "e2-not-redecoded" "$V1_OUT/er-2.txt" "한글 둘"
# e3
mkk
run er3 en "open:$KM,@after:1500:$RE:euckr,@after:2300:editor.paste:$W/p.txt,@after:2700:file.save,@after:3300:file.exit,@after:3800:toolbar.pick:project.exit_skip"
expect "e3-saved-cp949-korean" <(iconv -f CP949 -t UTF-8 "$WK/k.sql" 2>/dev/null) "한글 둘"
expect "e3-saved-cp949-edit" <(iconv -f CP949 -t UTF-8 "$WK/k.sql" 2>/dev/null) "-- v1er edit"
if iconv -f UTF-8 -t UTF-8 "$WK/k.sql" >/dev/null 2>&1; then v1_failmsg "e3-not-utf8-bytes" "저장본이 UTF-8로 풀림(인코딩이 바뀜)"; else v1_pass "e3-not-utf8-bytes"; fi
# e4
run er4 en "open:$UM,@after:1500:editor.dump:$O/er-4a.txt,@after:1800:$RE:utf8,@after:2600:editor.dump:$O/er-4b.txt,@after:2700:status.dump:$O/er-4s.txt,@after:3100:file.exit,@after:3600:toolbar.pick:project.exit_skip"
expect_eq "e4-utf8-body-same" "$(tail -n +2 "$V1_OUT/er-4b.txt" 2>/dev/null | md5sum)" "$(tail -n +2 "$V1_OUT/er-4a.txt" 2>/dev/null | md5sum)"
expect "e4-status-reopened" "$V1_OUT/er-4s.txt" "Reopened as"
# e5
mkk
run er5 ko "open:$KM,@after:1500:$RE:euckr,@after:2300:status.dump:$O/er-5s.txt,@after:2700:file.exit,@after:3200:toolbar.pick:project.exit_skip"
expect "e5-ko-status" "$V1_OUT/er-5s.txt" "다시 열었습니다"
for t in er1 er2 er3 er4 er5; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
