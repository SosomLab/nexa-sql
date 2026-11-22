#!/usr/bin/env bash
# V1 syntax-colors — T-78 ⓑ 구문 색 개별 덮어쓰기(`editor.syntax_<종류>_color`): 헤드리스 창 캡처의 픽셀 색 세기(python + Pillow 필요).
#   sc1 = editor.syntax_keyword_color=#FF00FF → 본문에 마젠타 픽셀 > 0 · sc0 = 키 없음(대조) = 0 · scx = 형식 오류(#ZZZ) = 대조와 같음(0).
#   keyword는 SELECT/FROM이 많아 픽셀 수가 충분하다 · 마젠타(255,0,255)는 기본 프리셋·UI에 없는 색이라 거짓 양성이 적다(대조로 확인).
. "$(dirname "$0")/../lib/v1.sh"
mkdir -p "$V1_OUT"
printf 'SELECT a, b FROM t WHERE a = 1;\nSELECT c FROM u ORDER BY c;\nSELECT d FROM v GROUP BY d;\n' > "$V1_OUT/q.sql"
W=$(v1_m "$V1_OUT")
# 마젠타 근처(안티앨리어싱 허용: R>200 · G<80 · B>200) 픽셀 수
#   ⚠ 이 PC의 python은 pyenv-win .bat 래퍼라 여러 줄 `-c`가 깨진다 → 세는 코드는 파일(scripts/lib/magenta_px.py)로. 시나리오로 옮길 때는
#     (경로는 v1_m으로 넘긴다 · Windows = 섞인 꼴 · 그 밖 = 그대로).
MAG_PY="${MAG_PY:-$V1_ROOT/scripts/lib/magenta_px.py}"
PY=$(command -v python3 || command -v python)
mag() { "$PY" -I "$(v1_m "$MAG_PY")" "$(v1_m "$1")" 2>/dev/null | tail -1 | tr -d '\r' || echo ERR; }
run() { local tag=$1; shift; v1_home en; [ $# -gt 0 ] && v1_set "$@"
  rm -f "$V1_OUT/$tag.png"
  v1_gui "$tag" "open:$W/q.sql,@after:2000:capture:$W/$tag.png,@after:2600:file.exit" 20
  rm -rf "$V1_HOME"; V1_HOME=""; }
run sc1 "editor.syntax_keyword_color=#FF00FF"
run sc0
run scx "editor.syntax_keyword_color=#ZZZ"
run sc8 "editor.syntax_keyword_color=#FF00FFFF"   # 색 창([고르기])이 저장하는 8자리 꼴(11-22 결함: 6자리만 받아 무시됨)
N1=$(mag "$V1_OUT/sc1.png"); N0=$(mag "$V1_OUT/sc0.png"); NX=$(mag "$V1_OUT/scx.png"); N8=$(mag "$V1_OUT/sc8.png")
echo "magenta px: sc1=$N1 sc0=$N0 scx=$NX"
expect_eq "png-exists" "$([ -s "$V1_OUT/sc1.png" ] && [ -s "$V1_OUT/sc0.png" ] && [ -s "$V1_OUT/scx.png" ] && echo y)" "y"
expect_eq "sc1-magenta-present" "$([ "${N1:-0}" != ERR ] && [ "${N1:-0}" -gt 50 ] && echo y)" "y"
expect_eq "sc0-no-magenta" "$N0" "0"
expect_eq "scx-bad-value-like-default" "$NX" "0"
expect_eq "sc8-rgba-value-applies" "$([ "${N8:-0}" != ERR ] && [ "${N8:-0}" -gt 50 ] && echo y)" "y"
for t in sc1 sc0 scx sc8; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
[ "${V1_KEEP_CAPTURE:-0}" = 1 ] || rm -f "$V1_OUT"/sc*.png
v1_done
