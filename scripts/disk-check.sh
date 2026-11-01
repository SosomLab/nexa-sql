#!/usr/bin/env bash
# disk-check.sh — 개발 산출물 디스크 점검·정리(사용자 11-02 "개발에 사용된 폴더가 수십 GB를 차지하지 않도록 관리" · docs/61 §2-4 디스크 규칙).
#   11-02 사고: `target/debug` 121 GB(deps 62 + incremental 56) + 시험 산출물 수백 MB로 맥 디스크가 가득 차(ENOSPC) 두 세션의 셸이 멈췄다.
#   사용: scripts/disk-check.sh            # 점검만 — 볼륨 여유 · target/ 큰 폴더 · 지울 수 있는 것과 크기(dry-run)
#         scripts/disk-check.sh --prune    # 아래 "지워도 되는 것"을 실제로 삭제(빌드 캐시 본체·실기 exe·릴리스 판은 남긴다)
#         scripts/disk-check.sh --prune --cross   # + 교차 타깃 폴더(target/x86_64-*·aarch64-* · check-3os 전체 모드가 다시 만든다)
#   ★ 삭제는 매번 사용자에게 질문(11-02 결정 · 1분 무응답 = 미삭제로 두고 진행) — 기본은 dry-run · `--prune`은 사용자가 답한 뒤에만.
#   지울 후보(61 §2-4): ① 증분 컴파일 캐시 `target/*/incremental`(다음 빌드가 다시 만든다 · 빌드 1~2분 추가)
#     ② 시험·측정 산출물 폴더(`target/v*-*` · `target/t[0-9]*` · `func-check*` · `perf-*` · `capture-*` · `uicheck` · `ui[0-9]*` · `flick*` · `flush-*` ·
#        `textref` · `tmp` · `mac-tests-*` · `clipv1` · `code-health` · `*.log`) — 결론은 journal에 있으므로 폴더는 버려도 된다
#     ③ 세션 스크래치패드 캡처(PNG·BMP · /tmp/claude-*/…/scratchpad/cap)
#   남기는 것: `target/debug`(실기 Debug exe) · `target/release`·`target/lite`(Release lite 판) · `duckdb-download`(build.rs 다운로드 캐시) ·
#     `packaging`·`v130-bundle`·`rel0*`(릴리스 자산 작업본 · 릴리스 규칙) · `cargo clean`은 사용자 지시 때만(재빌드 10분+).
#   기준: 여유 < 20 GB = 경고 + 정리 권고 · 풀 테스트·릴리스 빌드·check-3os 전체 모드 전에는 여유 ≥ 30 GB 확인.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PRUNE=0; CROSS=0
for a in "$@"; do case $a in --prune) PRUNE=1;; --cross) CROSS=1;; -h|--help) sed -n '2,16p' "$0"; exit 0;; esac; done
hr() { echo "── $1"; }
free_gb() { df -Pk "$1" | awk 'NR==2{printf "%d", $4/1024/1024}'; }
hr "볼륨 여유"
df -h "$ROOT" | tail -1
FREE=$(free_gb "$ROOT")
[ "$FREE" -lt 20 ] && echo "⚠ 여유 ${FREE} GB < 20 GB — 정리 권고(--prune)" || echo "여유 ${FREE} GB"
T="$ROOT/target"
[ -d "$T" ] || { echo "target/ 없음"; exit 0; }
hr "target/ 큰 폴더(상위 12)"
du -sh "$T"/* 2>/dev/null | sort -rh | head -12
for sib in nexa-ui nexa-license; do
  [ -d "$ROOT/../$sib/target" ] && printf '%s\t../%s/target\n' "$(du -sh "$ROOT/../$sib/target" 2>/dev/null | cut -f1)" "$sib"
done
# 지울 후보 모으기
cands=()
for d in "$T"/*/incremental "$T"/lite/*/incremental; do [ -d "$d" ] && cands+=("$d"); done
shopt -s nullglob
for p in "$T"/v[0-9]*-* "$T"/t[0-9]* "$T"/func-check* "$T"/perf-* "$T"/capture-* "$T"/uicheck "$T"/ui[0-9]* "$T"/flick* "$T"/flush-* \
         "$T"/textref "$T"/tmp "$T"/mac-tests-* "$T"/clipv1 "$T"/code-health "$T"/*.log; do
  case "$(basename "$p")" in v130-bundle) continue;; esac
  cands+=("$p")
done
if [ "$CROSS" = 1 ]; then for p in "$T"/x86_64-* "$T"/aarch64-*; do cands+=("$p"); done; fi
TMPC=/tmp; [ -d /private/tmp ] && TMPC=/private/tmp   # macOS = /tmp → /private/tmp 심볼릭 링크(같은 폴더를 두 번 세지 않는다)
for c in "$TMPC"/claude-*/*/*/scratchpad/cap; do [ -d "$c" ] && cands+=("$c"); done
shopt -u nullglob
hr "지울 수 있는 것($([ "$PRUNE" = 1 ] && echo '삭제' || echo 'dry-run · --prune으로 삭제'))"
total=0
for c in "${cands[@]}"; do
  kb=$(du -sk "$c" 2>/dev/null | cut -f1); total=$((total + kb))
  printf '%7s  %s\n' "$(du -sh "$c" 2>/dev/null | cut -f1)" "${c#"$ROOT"/}"
  [ "$PRUNE" = 1 ] && rm -rf "$c"
done
echo "합계 ≈ $((total / 1024 / 1024)) GB ($((total / 1024)) MB)"
[ "$PRUNE" = 1 ] && { hr "정리 뒤"; df -h "$ROOT" | tail -1; }
exit 0
