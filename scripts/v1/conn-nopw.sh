#!/usr/bin/env bash
# V1 conn-nopw — "비밀번호 없음" 프로필(T-133 · 21 §14): CLI 저장 = `nopassword=on`(봉투 없음) · conn show / 저장 안내 = `***` 없음.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
OUT=$(v1_cli conn add pgnp "postgres://u:@h.invalid:5432/db" --no-prompt)
echo "$OUT" > "$V1_OUT/add.txt"
F=$(grep -l "^name=pgnp" "$V1_HOME"/profiles/*.conf 2>/dev/null | head -1)
expect "profile-key" "$F" "nopassword=on"
expect_not "no-secret" "$F" "secret="
expect "add-line-no-mask" "$V1_OUT/add.txt" "postgres://u@h.invalid:5432/db"
v1_cli conn show pgnp > "$V1_OUT/show.txt"
expect "show-no-mask" "$V1_OUT/show.txt" "postgres://u@h.invalid:5432/db"
v1_done
