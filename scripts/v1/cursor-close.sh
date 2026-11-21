#!/usr/bin/env bash
# V1 cursor-close — T-93 ⓑ(결과 탭이 사라지면 서버 커서도 닫는다 · docs/43 ③): 실서버 Oracle(개발 프로필)에서 "더 있음"으로 온 SELECT의
#   서버 커서를 **다른 접속(nsql CLI)** 의 `v$open_cursor`로 센다(CURSOR_TYPE = OPEN만 · 세션 커서 캐시 = 닫힌 것).
#   ① 결과 탭 닫기(`result.tab.close`) ② 편집기 탭 닫기(`file.close_tab` = 패널이 통째로 사라짐) ③ 대조군(닫지 않음 = 남아 있어야).
#   프로필 = env NSQL_ORACLE_PROFILE(실제 이름은 저장소에 쓰지 않는다 · 익명화) · 없으면 SKIP · 운영 표식(TYPE=PRD)이면 SKIP(접속하지 않음).
#   읽기 접속만(SELECT · v$open_cursor) · 영속 객체 0 · 격리 홈(프로필 사본)은 v1_done에서 삭제 · 키·마우스 주입 0.
#   권한 없음(ORA-00942 등)이면 SKIP 판정 불가 + 원문.
. "$(dirname "$0")/../lib/v1.sh"
v1_need_env NSQL_ORACLE_PROFILE
PROF=$NSQL_ORACLE_PROFILE
row=$(v1_cli conn list | awk -v n="$PROF" '$1==n')
[ -n "$row" ] || { echo "SKIP $V1_NAME 프로필 없음(env NSQL_ORACLE_PROFILE)"; exit 0; }
echo "$row" | awk '{print $3}' | grep -qi prd && { echo "SKIP $V1_NAME 운영 표식 프로필 — 접속하지 않음"; exit 0; }
v1_home en 1
W=$(v1_m "$V1_OUT")
TAG="NSQLCUR$$$RANDOM"
# 세는 쿼리 자신의 글에 표식이 이어 붙지 않게 LIKE 무늬를 둘로 쪼갠다 · CLI 자기 세션은 뺀다.
HALF1=${TAG:0:4}; HALF2=${TAG:4}
count_open() {   # <산출 파일> — 줄 = CURSOR_TYPE<TAB>N
  printf "SELECT cursor_type, COUNT(*) n FROM v\$open_cursor WHERE sql_text LIKE '%%%s'||'%s%%' AND sid <> SYS_CONTEXT('USERENV','SID') GROUP BY cursor_type;\n" "$HALF1" "$HALF2" \
    | "$V1_NSQL" run -c "$PROF" --no-prompt -f tsv - > "$1" 2> "$1.err"   # 접속 안내 줄(주소)은 판정 파일 밖으로
}
open_n() { awk -F'\t' '$1=="OPEN"{print $2; f=1} END{if(!f) print 0}' "$1"; }
wait_file() { local i; for i in $(seq 1 60); do [ -f "$1" ] && return 0; sleep 0.5; done; return 1; }
# 권한 문턱
count_open "$V1_OUT/probe.tsv"
if grep -qE "ORA-|error|Error" "$V1_OUT/probe.tsv.err"; then echo "SKIP $V1_NAME 판정 불가(v\$open_cursor 조회 실패: $(grep -m1 -E 'ORA-|rror' "$V1_OUT/probe.tsv.err" | cut -c1-120))"; rm -rf "$V1_HOME"; exit 0; fi
printf 'SELECT /* %s */ * FROM all_objects\n' "$TAG" > "$V1_OUT/q.sql"
# 한 판 — <태그> <닫기 명령|none>. 시각 = 접속 뒤 ms · 표식 파일로 바깥 세기와 맞춘다.
run_case() {
  local tag=$1 act=$2 c=""
  rm -f "$V1_OUT/$tag".m1 "$V1_OUT/$tag".m2
  c="@connected:open:$W/q.sql,@connected:@after:1500:run.all,@connected:@after:7000:grid.dump:$W/$tag.a.grid,@connected:@after:7200:fs.write:$W/$tag.m1|1"
  [ "$act" != none ] && c="$c,@connected:@after:15000:$act"
  c="$c,@connected:@after:16000:result.dump:$W/$tag.b.res,@connected:@after:16200:fs.write:$W/$tag.m2|1,@connected:@after:24000:file.exit"
  v1_gui "$tag" "$c" 45 "$PROF" &
  local gp=$!
  if wait_file "$V1_OUT/$tag.m1"; then count_open "$V1_OUT/$tag.before.tsv"; else echo "m1 없음" > "$V1_OUT/$tag.before.tsv"; fi
  if wait_file "$V1_OUT/$tag.m2"; then sleep 1; count_open "$V1_OUT/$tag.after.tsv"; else echo "m2 없음" > "$V1_OUT/$tag.after.tsv"; fi
  wait "$gp"
  echo "INFO $V1_NAME $tag OPEN 전=$(open_n "$V1_OUT/$tag.before.tsv") 후=$(open_n "$V1_OUT/$tag.after.tsv")"   # 접속 안내 줄에 주소가 있어 원문은 싣지 않는다(익명화)
}
run_case rtab result.tab.close
run_case etab file.close_tab
run_case ctrl none
for k in rtab etab ctrl; do expect "$k-more" "$V1_OUT/$k.a.grid" "more=true"; done
for k in rtab etab ctrl; do b=$(open_n "$V1_OUT/$k.before.tsv"); [ "$b" -ge 1 ] && v1_pass "$k-open-before" || v1_failmsg "$k-open-before" "OPEN=$b"; done
expect_eq "rtab-closed" "$(open_n "$V1_OUT/rtab.after.tsv")" 0
expect_eq "etab-closed" "$(open_n "$V1_OUT/etab.after.tsv")" 0
a=$(open_n "$V1_OUT/ctrl.after.tsv"); [ "$a" -ge 1 ] && v1_pass "ctrl-still-open" || v1_failmsg "ctrl-still-open" "OPEN=$a(대조군이 닫힘 = 판정 근거 없음)"
v1_done
