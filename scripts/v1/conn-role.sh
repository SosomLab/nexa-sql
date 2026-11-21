#!/usr/bin/env bash
# V1 conn-role — Oracle 역할(D-487 ③ · 21 §15): CLI `?role=sysdba` = `role=SYSDBA` · 표시 `AS SYSDBA` · 접속 창 폼 저장(`conn.var:` 길) 뒤에도 유지.
#   폼 배치(라벨 잘림)는 덤프가 없어 캡처로만 남긴다(V1_KEEP_CAPTURE=1일 때).
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_cli conn add sysp "oracle://sys@h.invalid:1521/svc?role=sysdba" --no-prompt > "$V1_OUT/add.txt"
expect "add-as-sysdba" "$V1_OUT/add.txt" "AS SYSDBA"
F=$(grep -l "^name=sysp" "$V1_HOME"/profiles/*.conf 2>/dev/null | head -1)
expect "profile-role" "$F" "role=SYSDBA"
W=$(v1_m "$V1_OUT")
CAP=""; [ "${V1_KEEP_CAPTURE:-0}" = 1 ] && CAP="@after:2800:capture:$W/conn-role.png@Login,"
v1_gui form "@after:1500:conn.edit:sysp,${CAP}@after:3200:conn.var:sysp:zz=1,@after:4500:file.exit" 20
expect "form-save-keeps-role" "$F" "role=SYSDBA"
expect "form-save-happened" "$F" "var.ZZ"
v1_done
