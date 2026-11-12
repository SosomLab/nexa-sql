#!/usr/bin/env bash
# tmp-clean.sh — 이 프로젝트가 프로젝트 폴더 밖·`target/` 안에 만든 임시 산출물을 보존 기간이 지나면 한꺼번에 지운다
#   (사용자 11-03 "프로젝트 폴더가 아닌 임시 폴더/파일을 사용한 경우 정리하는 작업을 배치해서 과다하게 누적되지 않도록" · docs/61 §2-4 · D-445).
#   사용: scripts/tmp-clean.sh                 # dry-run — 후보와 크기·나이만 보인다
#         scripts/tmp-clean.sh --apply         # 실제 삭제(세션 마감 때 1회 · 이 프로젝트가 만든 것만이라 묻지 않는다 · 61 §2-4)
#         scripts/tmp-clean.sh --days 3        # 보존 기간(기본 7일 · 0 = 전부)
#   후보(이 프로젝트가 만든 것만 · 다른 프로젝트의 임시 폴더(`nexa-slide-*` 등)·OS 파일은 건드리지 않는다):
#     ① OS 임시 폴더(/tmp · $TMPDIR · %TEMP%)의 `nsql-*` · `nsql_*` · `nexa-sql-*`(격리 홈 `nsql-home*` · 단위 시험 찌꺼기 `nsql-settings-<pid>-*` · `nsql_t###_<pid>.db` · `nexa-sql-debug.{pid,log}`)
#     ② 세션 스크래치패드(<임시>/claude/<이 프로젝트 슬러그>/<세션>/) — 보존 기간 안에 손댄 파일이 없는 세션 폴더
#     ③ `target/<id>/` 시험·측정 산출 폴더(cargo 프로필·릴리스 작업본·다운로드 캐시 제외 = disk-check.sh와 같은 "남기는 것")
#     ★ 즉시 후보(나이 무관) = 실제 설정·프로필 사본이 든 격리 홈(`*/home/` 또는 `nsql-home*` 안에 `nsql-vault`·`profiles`가 있는 것) — 판정이 끝나면 바로 지운다
#   판정 = 그 폴더 안에 보존 기간 안에 바뀐 항목(파일·폴더)이 하나라도 있으면 "살아 있음"(find · 첫 발견에서 멈춤 → 활성 폴더는 즉시) · 표시 나이(≈)는 폴더와 바로 아래 항목까지만 본다.
#   순회 = 후보마다 find 1회 + du 1회(11-03 속도 개선 · 종전 4회 = 첫 적용 553개 약 8분 · 정리 뒤 정상 상태 = 수 초). 3-OS 공통(bash · find · du만).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APPLY=0; DAYS=7
while [ $# -gt 0 ]; do case $1 in --apply) APPLY=1;; --days) DAYS=${2:-7}; shift;; -h|--help) sed -n '2,14p' "$0"; exit 0;; esac; shift; done
hr() { echo "── $1"; }
# 임시 폴더 후보(중복 제거) — Git Bash: /tmp = %TEMP% · macOS: $TMPDIR(/var/folders/…) + /tmp(=/private/tmp) · Linux: /tmp
tmps=()
add_tmp() { local p; p="$(cd "$1" 2>/dev/null && pwd -P)" || return; for t in "${tmps[@]:-}"; do [ "$t" = "$p" ] && return; done; tmps+=("$p"); }
add_tmp /tmp; [ -n "${TMPDIR:-}" ] && add_tmp "$TMPDIR"; [ -n "${TEMP:-}" ] && add_tmp "$(command -v cygpath >/dev/null && cygpath -u "$TEMP" || echo "$TEMP")"
# 이 프로젝트의 스크래치패드 슬러그 = 저장소 경로의 영숫자 외 글자를 '-'로(대소문자 무시 비교)
root_win="$ROOT"; command -v cygpath >/dev/null && root_win="$(cygpath -m "$ROOT")"
slug="$(printf '%s' "$root_win" | sed 's/[^A-Za-z0-9]/-/g' | tr 'A-Z' 'a-z')"
# 살아 있음 = 최근 N일 안에 바뀐 항목(파일이든 폴더든)이 하나라도 있음 — 첫 발견에서 멈춘다(오래된 폴더만 전체 순회).
quiet() { [ "$DAYS" -le 0 ] && return 0; [ -z "$(find "$1" -mtime -"$DAYS" -print -quit 2>/dev/null)" ]; }
# 표시용 나이(≈) = 폴더 자체와 바로 아래 항목 중 가장 최근 mtime(전체 순회 없음).
age_days() { local n; n=$( { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"; find "$1" -mindepth 1 -maxdepth 1 -printf '%T@\n' 2>/dev/null; } | sort -rn | head -1); echo $(( ($(date +%s) - ${n%.*}) / 86400 )); }
is_home_copy() { [ -e "$1/nsql-vault" ] || [ -e "$1/profiles" ] || [ -e "$1/nsql-vault.json" ] || [ -d "$1/vault" ]; }
fmt_kb() { if [ "$1" -ge 1048576 ]; then awk "BEGIN{printf \"%.1fG\", $1/1048576}"; elif [ "$1" -ge 1024 ]; then awk "BEGIN{printf \"%.0fM\", $1/1024}"; else echo "${1}K"; fi; }
cands=(); tags=()
add() { cands+=("$1"); tags+=("$2"); }
shopt -s nullglob
for t in "${tmps[@]}"; do
  for p in "$t"/nsql-* "$t"/nsql_* "$t"/nexa-sql-*; do
    if [ -d "$p" ] && is_home_copy "$p"; then add "$p" "즉시(프로필 사본)"; elif quiet "$p"; then add "$p" "OS 임시"; fi
  done
  for s in "$t"/claude/*/; do
    [ "$(basename "$s" | tr 'A-Z' 'a-z')" = "$slug" ] || continue
    for sess in "$s"*/; do quiet "$sess" && add "$sess" "스크래치패드 세션"; done
  done
done
T="$ROOT/target"
if [ -d "$T" ]; then
  for d in "$T"/*/; do
    n="$(basename "$d")"
    case "$n" in debug|release|lite|it|doc|package|duckdb-download|packaging|v130-bundle|rel0*|x86_64-*|aarch64-*|wasm32-*|i686-*) continue;; esac
    for h in "$d"home "$d"*/home; do [ -d "$h" ] && is_home_copy "$h" && add "$h" "즉시(프로필 사본)"; done
    quiet "$d" && add "$d" "target 산출물"
  done
fi
shopt -u nullglob
hr "임시 산출물 정리 — 보존 ${DAYS}일 · $([ "$APPLY" = 1 ] && echo '삭제' || echo 'dry-run · --apply로 삭제') · 임시 폴더 = ${tmps[*]}"
total=0; n=0
for i in "${!cands[@]}"; do
  c="${cands[$i]}"; [ -e "$c" ] || continue
  kb=$(du -sk "$c" 2>/dev/null | cut -f1); kb=${kb:-0}; total=$((total + kb)); n=$((n + 1))
  printf '%7s  ≈%3d일  %-14s %s\n' "$(fmt_kb "$kb")" "$(age_days "$c")" "${tags[$i]}" "${c#"$ROOT"/}"
  [ "$APPLY" = 1 ] && rm -rf "$c"
done
echo "후보 ${n}개 · 합계 ≈ $((total / 1024)) MB"
exit 0
