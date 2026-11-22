#!/usr/bin/env bash
# license_warn.sh — 성능·캡처 스크립트 공용 조각(T-244 · 11-23): 측정에 쓰는 홈이 무료판(free)이면 시작에 경고 한 줄(중단하지 않음).
#   무료판은 2 MB 넘는 파일이 큰 파일 게이트에 걸려 열기·편집 수치가 왜곡된다(26 §7 · 09-27 §14).
#   사용: . scripts/lib/license_warn.sh; lic_warn <nsql 실행 파일> <NSQL_HOME> [<출력 함수 이름(기본 echo)>]
lic_warn() {
  local nsql=$1 home=$2 out=${3:-echo} st
  st=$(NSQL_HOME="$home" "$nsql" license status 2>/dev/null | tr -d '\r' | awk '$1=="state"{print $2; exit}')
  case "$st" in
    free|"") "$out" "⚠ license = ${st:-unknown} (home=$home) — files over 2 MB hit the free-edition large-file gate, so open/edit timings are skewed. Install a test license first: NSQL_HOME=$home $nsql license install <file> (T-244)" ;;
  esac
  return 0
}
