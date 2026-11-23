#!/usr/bin/env bash
# V1 prefs-cats — 설정 분류 "External changes" · "Find in Files"(T-250 · D-488 ②): `config list all` 머리글 순서 · 키 수 · "Files"에 남은 키 0.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_cli config list all > "$V1_OUT/config.txt"
O=$(grep -nE '^\[Editors ▸ (Large files|External changes|Find in Files)\]' "$V1_OUT/config.txt" | cut -d: -f2 | tr '\n' '|')
expect_eq "order" "$O" "[Editors ▸ Large files]|[Editors ▸ External changes]|[Editors ▸ Find in Files]|"
cnt() { awk -v h="$1" '$0==h{f=1;next} f&&/^\[/{exit} f&&/^[a-z_]+\./{n++} END{print n+0}' "$V1_OUT/config.txt"; }
expect_eq "external-8" "$(cnt '[Editors ▸ External changes]')" "8"
expect_eq "find-7" "$(cnt '[Editors ▸ Find in Files]')" "7"
LEFT=$(awk '$0=="[Editors ▸ Files]"{f=1;next} f&&/^\[/{exit} f' "$V1_OUT/config.txt" | grep -cE '^(file\.external_|search\.)')
expect_eq "files-left-0" "$LEFT" "0"
# 탐색기 더블클릭 판정 시간 기본 500 ms(11-23 저녁 975acb89 · 종전 400 · 탐색기도 이 키를 씀)
expect_eq "dblclick-ms-500" "$(grep -E '^ui\.dblclick_ms ' "$V1_OUT/config.txt" | awk '{print $2}')" "500"
v1_done
