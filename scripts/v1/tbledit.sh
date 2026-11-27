#!/usr/bin/env bash
# V1 tbledit — 테이블 구조 편집기(docs/119 · 11-27) 왕복: SQLite 파일 DB(실서버 0 · 영속 객체 0 = 격리 홈 안 파일).
#   ① 열기 = 서버 구조 읽힘(컬럼 2 · 변경 없음) ② 컬럼 추가 = ALTER 문장 1 · 미저장 ③ 적용(문장 하나씩 · 문지기) → 다시 읽힘(컬럼 3 · 변경 없음)
#   ④ 이름 바꾸기 = RENAME COLUMN ⑤ SQLite가 못 하는 타입 변경 = 문장이 아니라 "재생성 필요" 막힘(D-318 재생성 금지)
#   ⑥ 새 테이블 = CREATE TABLE → 적용 → 다른 접속(nsql CLI)에서 보인다. 키·마우스 주입 0 · 판정 = `tbledit.dump:` 글.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
DB="$(v1_m "$V1_HOME")/t.sqlite"
v1_cli conn add lite "sqlite:$DB" -d sqlite --no-prompt >/dev/null
printf 'CREATE TABLE item (id INTEGER NOT NULL, name TEXT);\nCREATE INDEX item_ix1 ON item (name);\n' | "$V1_NSQL" run -c lite --no-prompt - >/dev/null 2>&1
W=$(v1_m "$V1_OUT")
C="@connected:@after"
v1_gui tbledit "$C:2500:tbledit.open:main.item,$C:4500:tbledit.dump:$W/a.txt,\
$C:4700:tbledit.add,$C:4800:tbledit.set:2:0=qty,$C:4900:tbledit.set:2:1=INTEGER,$C:5100:tbledit.dump:$W/b.txt,\
$C:5300:tbledit.apply_now,$C:8300:tbledit.dump:$W/c.txt,\
$C:8500:tbledit.set:1:0=title,$C:8700:tbledit.dump:$W/d.txt,\
$C:8900:tbledit.set:1:1=VARCHAR(10),$C:9100:tbledit.dump:$W/e.txt,$C:9300:tbledit.revert,\
$C:9500:tbledit.new:main,$C:9700:tbledit.name:orders,$C:9800:tbledit.add,$C:9900:tbledit.set:0:0=id,$C:10000:tbledit.set:0:1=INTEGER,$C:10100:tbledit.toggle:0:2,\
$C:10300:tbledit.dump:$W/f.txt,$C:10500:tbledit.apply_now,$C:13500:tbledit.dump:$W/g.txt,$C:14000:file.exit" 40 lite
expect "open-loaded" "$V1_OUT/a.txt" "table=main.item new=false loaded=true dirty=false can_apply=false section=columns rows=2"
expect "open-row" "$V1_OUT/a.txt" "row|0||id|INTEGER|Y||"
expect "add-stmt" "$V1_OUT/b.txt" "stmt|Safe|ALTER TABLE main.item ADD COLUMN qty INTEGER"
expect "add-dirty" "$V1_OUT/b.txt" "dirty=true can_apply=true"
expect "add-mark" "$V1_OUT/b.txt" "row|2|+|qty|INTEGER|N||"
expect "applied-reloaded" "$V1_OUT/c.txt" "dirty=false can_apply=false section=columns rows=3"
expect "applied-row" "$V1_OUT/c.txt" "row|2||qty|INTEGER|N||"
expect "rename-stmt" "$V1_OUT/d.txt" "stmt|Safe|ALTER TABLE main.item RENAME COLUMN name TO title"
expect "type-blocked" "$V1_OUT/e.txt" "blocked|NeedsRebuild|title"
expect_not "type-no-stmt" "$V1_OUT/e.txt" "ALTER COLUMN"
expect "new-create" "$V1_OUT/f.txt" "stmt|Safe|CREATE TABLE main.orders (     id INTEGER NOT NULL )"
expect "new-created" "$V1_OUT/g.txt" "table=main.orders new=false loaded=true dirty=false"
cols=$(printf "SELECT name FROM pragma_table_info('item');\n" | "$V1_NSQL" run -c lite --no-prompt -f tsv - 2>/dev/null | tr '\n' ' ')
case "$cols" in *qty*) v1_pass "db-has-qty";; *) v1_failmsg "db-has-qty" "컬럼 = $cols";; esac
case "$cols" in *title*) v1_failmsg "db-no-rename" "되돌린 이름 바꾸기가 적용됨 · $cols";; *) v1_pass "db-no-rename";; esac
tabs=$(printf "SELECT name FROM sqlite_master WHERE type = 'table';\n" | "$V1_NSQL" run -c lite --no-prompt -f tsv - 2>/dev/null | tr '\n' ' ')
case "$tabs" in *orders*) v1_pass "db-has-orders";; *) v1_failmsg "db-has-orders" "테이블 = $tabs";; esac
v1_done
