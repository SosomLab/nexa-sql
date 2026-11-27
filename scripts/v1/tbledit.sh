#!/usr/bin/env bash
# V1 tbledit — 테이블 구조 편집기(docs/119 · 11-27) 왕복: SQLite 파일 DB(실서버 0 · 영속 객체 0 = 격리 홈 안 파일).
#   ① 열기 = 서버 구조 읽힘(컬럼 2 · 변경 없음) ② 컬럼 추가 = ALTER 문장 1 · 미저장 ③ 적용(문장 하나씩 · 문지기) → 다시 읽힘(컬럼 3 · 변경 없음)
#   ④ 이름 바꾸기 = RENAME COLUMN ⑤ SQLite가 못 하는 타입 변경 = 문장이 아니라 "재생성 필요" 막힘(D-318 재생성 금지)
#   ⓪ 라이선스 안내(3초 뒤에만 확인) ⑥ 새 테이블 = CREATE TABLE → 적용 → 다른 접속(nsql CLI)에서 보인다. ⑦ 미적용 변경 탭 닫기 = 한 번 더 닫아야 닫힌다. ⑧ 주입 패닉 = 편집기 탭만 닫힌다. 키·마우스 주입 0 · 판정 = `tbledit.dump:` 글.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
DB="$(v1_m "$V1_HOME")/t.sqlite"
v1_cli conn add lite "sqlite:$DB" -d sqlite --no-prompt >/dev/null
printf 'CREATE TABLE item (id INTEGER NOT NULL, name TEXT);\nCREATE INDEX item_ix1 ON item (name);\n' | "$V1_NSQL" run -c lite --no-prompt - >/dev/null 2>&1
W=$(v1_m "$V1_OUT")
printf -- "-- edited in the preview\nALTER TABLE main.item ADD COLUMN extra2 TEXT;\n" > "$V1_OUT/edited.sql"
C="@connected:@after"
v1_gui tbledit "$C:2500:tbledit.open:main.item,$C:4000:tbledit.dump:$W/n0.txt,$C:4100:tbledit.add,$C:4200:tbledit.notice_ok,$C:4300:tbledit.dump:$W/n1.txt,$C:6300:tbledit.notice_ok,$C:8500:tbledit.dump:$W/a.txt,\
$C:8700:tbledit.add,$C:8800:tbledit.set:2:0=qty,$C:8900:tbledit.set:2:1=INTEGER,$C:9100:tbledit.dump:$W/b.txt,\
$C:9130:tbledit.undo,$C:9160:tbledit.dump:$W/u1.txt,$C:9190:tbledit.undo,$C:9220:tbledit.undo,$C:9240:tbledit.dump:$W/u2.txt,$C:9260:tbledit.redo,$C:9270:tbledit.redo,$C:9280:tbledit.redo,$C:9290:tbledit.dump:$W/u3.txt,\
$C:9300:tbledit.apply_now,$C:12300:tbledit.dump:$W/c.txt,$C:12350:tbledit.section:history,$C:12400:tbledit.dump:$W/hist.txt,$C:12450:tbledit.section:columns,\
$C:12500:tbledit.set:1:0=title,$C:12700:tbledit.dump:$W/d.txt,\
$C:12900:tbledit.set:1:1=VARCHAR(10),$C:13100:tbledit.dump:$W/e.txt,$C:13300:tbledit.apply_script:$W/edited.sql,$C:16300:tbledit.dump:$W/s.txt,$C:16500:tbledit.revert,\
$C:16700:tbledit.new:main,$C:16900:tbledit.name:orders,$C:17000:tbledit.add,$C:17100:tbledit.set:0:0=id,$C:17200:tbledit.set:0:1=INTEGER,$C:17300:tbledit.toggle:0:2,\
$C:17500:tbledit.dump:$W/f.txt,$C:17700:tbledit.apply_now,$C:20700:tbledit.dump:$W/g.txt,\
$C:20900:tbledit.add,$C:21000:tbledit.set:1:0=memo,$C:21100:tbledit.set:1:1=TEXT,$C:21200:file.close_tab,$C:21500:tbledit.dump:$W/h.txt,\
$C:21600:file.close_tab,$C:21900:tbledit.dump:$W/i.txt,$C:22100:tbledit.open:main.item,$C:23300:tbledit.test_panic,$C:24000:tbledit.dump:$W/p.txt,$C:24100:explorer.dump:$W/alive.txt,$C:24600:file.exit" 60 lite
# ⓪ 라이선스 안내 게이트(D-328 · 격리 홈 = 라이선스 없음): 3초 전에는 [확인]도 편집도 받지 않는다 · 3초 뒤 확인하면 쓸 수 있다.
expect "notice-shown" "$V1_OUT/n0.txt" "notice=on"
expect "notice-early-ok-ignored" "$V1_OUT/n1.txt" "rows=2 notice=on"
expect_not "notice-cleared" "$V1_OUT/a.txt" "notice=on"
expect "open-loaded" "$V1_OUT/a.txt" "table=main.item new=false loaded=true dirty=false can_apply=false section=columns rows=2"
expect "open-row" "$V1_OUT/a.txt" "row|0||id|INTEGER|Y||"
expect "add-stmt" "$V1_OUT/b.txt" "stmt|Safe|ALTER TABLE main.item ADD COLUMN qty INTEGER"
expect "add-dirty" "$V1_OUT/b.txt" "dirty=true can_apply=true"
expect "add-mark" "$V1_OUT/b.txt" "row|2|+|qty|INTEGER|N||"
# ②-b 되돌리기 · 다시 실행(셀 단위): 타입 칸 → 이름 칸 → 행 추가 순으로 되돌아가고, 다시 실행하면 같은 문장으로 돌아온다.
expect "undo-1-type" "$V1_OUT/u1.txt" "row|2|+|qty||N||"
expect "undo-3-clean" "$V1_OUT/u2.txt" "dirty=false can_apply=false section=columns rows=2"
expect "redo-3-back" "$V1_OUT/u3.txt" "stmt|Safe|ALTER TABLE main.item ADD COLUMN qty INTEGER"
expect "applied-reloaded" "$V1_OUT/c.txt" "dirty=false can_apply=false section=columns rows=3"
expect "applied-row" "$V1_OUT/c.txt" "row|2||qty|INTEGER|N||"
# ③-b 적용 기록: History 섹션 한 줄 + 실행한 스크립트 파일(격리 홈 backups/objects/…_applied.sql · 머리 주석 + 보낸 문장).
expect "history-row" "$V1_OUT/hist.txt" "|Applied 1 of 1 statement(s)|"
AP=$(ls "$V1_HOME"/backups/objects/*_main.item_TABLE.*_applied.sql 2>/dev/null | head -1)
if [ -n "$AP" ] && grep -qF "ALTER TABLE main.item ADD COLUMN qty INTEGER;" "$AP"; then v1_pass "history-file"; else v1_failmsg "history-file" "보관 파일 없음 또는 문장 없음: $AP"; fi
expect "rename-stmt" "$V1_OUT/d.txt" "stmt|Safe|ALTER TABLE main.item RENAME COLUMN name TO title"
expect "type-blocked" "$V1_OUT/e.txt" "blocked|NeedsRebuild|title"
expect_not "type-no-stmt" "$V1_OUT/e.txt" "ALTER COLUMN"
# ⑤-b 미리보기에서 고친 스크립트 = 고친 글 그대로 실행(D-317) · 화면의 남은 편집(이름 바꾸기)은 지켜진다.
expect "edited-applied" "$V1_OUT/s.txt" "|extra2|TEXT|"
expect "edited-keeps-pending" "$V1_OUT/s.txt" "RENAME COLUMN name TO title"
expect "new-create" "$V1_OUT/f.txt" "stmt|Safe|CREATE TABLE main.orders (     id INTEGER NOT NULL )"
expect "new-created" "$V1_OUT/g.txt" "table=main.orders new=false loaded=true dirty=false"
# ⑦ 미적용 변경이 있는 탭 닫기 = 첫 번째는 남고(안내) · 곧바로 한 번 더 닫으면 닫힌다(그 뒤 덤프 = 다른 편집기 탭 또는 없음).
expect "close-1-kept" "$V1_OUT/h.txt" "table=main.orders new=false loaded=true dirty=true"
if grep -qF "table=main.orders" "$V1_OUT/i.txt" 2>/dev/null; then v1_failmsg "close-2-closed" "두 번째 닫기 뒤에도 탭이 남음"; else v1_pass "close-2-closed"; fi
# ⑧ 오류 경계(D-329 · Debug 빌드의 주입 패닉): 편집기 탭만 닫히고 앱은 산다(탐색기 덤프가 나오고 종료 코드 0).
if [ -s "$V1_OUT/p.txt" ]; then v1_failmsg "panic-tab-closed" "패닉 뒤에도 편집기 탭이 남음: $(head -c 80 "$V1_OUT/p.txt")"; else v1_pass "panic-tab-closed"; fi
if [ -s "$V1_OUT/alive.txt" ]; then v1_pass "panic-app-alive"; else v1_failmsg "panic-app-alive" "탐색기 덤프 없음 = 앱이 죽었다"; fi
expect_eq "panic-exit-0" "$(cat "$V1_OUT/tbledit.exit")" "0"
expect "panic-logged" "$V1_OUT/tbledit.stderr" "[tbledit] panic in table editor tab"
cols=$(printf "SELECT name FROM pragma_table_info('item');\n" | "$V1_NSQL" run -c lite --no-prompt -f tsv - 2>/dev/null | tr '\n' ' ')
case "$cols" in *qty*) v1_pass "db-has-qty";; *) v1_failmsg "db-has-qty" "컬럼 = $cols";; esac
case "$cols" in *title*) v1_failmsg "db-no-rename" "되돌린 이름 바꾸기가 적용됨 · $cols";; *) v1_pass "db-no-rename";; esac
tabs=$(printf "SELECT name FROM sqlite_master WHERE type = 'table';\n" | "$V1_NSQL" run -c lite --no-prompt -f tsv - 2>/dev/null | tr '\n' ' ')
case "$tabs" in *orders*) v1_pass "db-has-orders";; *) v1_failmsg "db-has-orders" "테이블 = $tabs";; esac
v1_done
