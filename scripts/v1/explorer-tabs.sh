#!/usr/bin/env bash
# V1 explorer-tabs — 탐색기 연결 행 흐린 글 끝 "· N tabs"(T-462 ② · D-117): SQLite 한 연결에 탭 3개 = `3 tabs` · 탭 1개 = 표시 없음.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_cli conn add lite "sqlite:$(v1_m "$V1_HOME")/t.sqlite" -d sqlite --no-prompt >/dev/null
W=$(v1_m "$V1_OUT")
v1_gui tabs "@connected:@after:2500:explorer.dump:$W/one.txt,@connected:@after:3000:file.new,@connected:@after:3300:file.new,@connected:@after:4500:explorer.dump:$W/three.txt,@connected:@after:5200:file.exit" 20 lite
expect_not "one-tab-no-count" "$V1_OUT/one.txt" "tabs"
expect "three-tabs" "$V1_OUT/three.txt" "3 tabs"
v1_done
