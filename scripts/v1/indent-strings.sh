#!/usr/bin/env bash
# V1 indent-strings — Enter 자동 들여쓰기가 문자열·줄 주석 안의 괄호/낱말을 보지 않는다(T-115 · nexa-ui `indent_code_mask` · 11-23): 헤드리스 · 격리 홈 · OS 입력 0.
#   새 탭에 `editor.paste`로 한 줄(끝 개행 없음) → `editor.key:Enter`(편집기 키 사건) → `editor.dump` 본문 둘째 줄의 앞 공백 수.
#   기본 규칙(`editor.indent_rules=sql`)은 여는 괄호로는 들여쓰지 않고 BEGIN 같은 낱말로 들여쓴다(11-23 첫 판에서 확인 · `  f(` → 2) →
#   대조는 낱말로: s2 `  BEGIN` → 6(= 2 + 4)   s4 `  x = 'BEGIN'` → 2(문자열 안 낱말 = 무시)   s3 `  x -- BEGIN` → 2(주석 안 낱말 = 무시)
#   s1 `  WHERE c = '('` → 2(문자열 안 괄호 · 기본 규칙에선 괄호가 원래 들여쓰지 않으므로 회귀 감시만)
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1is-XXXX")
W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
printf "  WHERE c = '('" > "$WK/s1.txt"; printf '  BEGIN' > "$WK/s2.txt"; printf "  x = 'BEGIN'" > "$WK/s4.txt"; printf '  x -- BEGIN' > "$WK/s3.txt"
# editor.dump = 머리 한 줄 + 본문 → 본문 둘째 줄 = 셋째 줄
#   들여쓰기 폭 = 공백 1 · 탭 4(기본 설정이면 늘어난 몫이 탭) · 기동마다 새 격리 홈(같은 홈이면 hot exit로 앞 실행의 Scratch 본문이 남는다 · 11-23 첫 판)
ind() { tr -d '\r' < "$1" 2>/dev/null | sed -n '3p' | awk '{ n=0; for(i=1;i<=length($0);i++){ c=substr($0,i,1); if(c==" ") n++; else if(c=="\t") n+=4; else break } print n }'; }
rm -f "$V1_OUT"/is-*.txt
for s in s1 s2 s3 s4; do
  [ -n "$V1_HOME" ] && rm -rf "$V1_HOME"; v1_home en
  v1_gui "is$s" "@after:1500:editor.paste:$W/$s.txt,@after:1900:editor.key:Enter,@after:2300:editor.dump:$O/is-$s.txt,@after:2700:file.exit,@after:3200:toolbar.pick:project.exit_skip" 20
done
echo "indent after Enter: s1=$(ind "$V1_OUT/is-s1.txt") s2=$(ind "$V1_OUT/is-s2.txt") s3=$(ind "$V1_OUT/is-s3.txt") s4=$(ind "$V1_OUT/is-s4.txt")"
expect_eq "s1-string-paren-ignored" "$(ind "$V1_OUT/is-s1.txt")" "2"
expect_eq "s2-begin-indents" "$(ind "$V1_OUT/is-s2.txt")" "6"
expect_eq "s4-string-word-ignored" "$(ind "$V1_OUT/is-s4.txt")" "2"
expect_eq "s3-comment-word-ignored" "$(ind "$V1_OUT/is-s3.txt")" "2"
expect "s1-first-line-kept" "$V1_OUT/is-s1.txt" "WHERE c = '('"
for s in s1 s2 s3 s4; do expect_not "is$s-no-panic" "$V1_OUT/is$s.stderr" "panicked"; done
rm -rf "$WK"
v1_done
