#!/usr/bin/env bash
# pkg-text-english-check.sh — 패키지·프로그램 안에 박히는 글이 영어인지 점검(규칙 = CLAUDE.md §3 · docs/33 §5-6 · 사용자 10-10).
#   대상 = 사용자·패키지 저장소·OS가 읽는 글만: Cargo description(바이너리 2) · exe VERSIONINFO · MSI · Info.plist · deb/rpm 설명 ·
#          .desktop(`[ko]` 키 제외) · cask desc · winget/choco 글 필드 · pkg 환영문 · postinstall/uninstall 출력 · THIRD-PARTY-NOTICES 머리.
#   제외 = 주석(#·//·<!-- -->) · 저장소 문서(docs/) · 빌드 스크립트의 CI 로그 문구.
#   사용: scripts/pkg-text-english-check.sh [--notices <생성한 THIRD-PARTY-NOTICES.txt>]  → 한글이 있는 줄을 찍고 exit 1.
#         scripts/pkg-text-english-check.sh --selftest  → 한글 판정 자체 확인.
#   한글 판정은 perl(3-OS 동일) — macOS grep은 `[가-힣]` 범위를 locale에 따라 거부한다(10-10 실측 "invalid character range").
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
fail=0
hit() { echo "✗ $1"; fail=1; }
hangul()    { perl -CSD -ne 'print "$.:$_" if /[\x{AC00}-\x{D7A3}\x{3131}-\x{3163}]/' "$@"; }   # 파일 → 번호:줄
hangul_in() { perl -CSD -ne 'print if /[\x{AC00}-\x{D7A3}\x{3131}-\x{3163}]/'; }                # stdin 필터

if [ "${1:-}" = "--selftest" ]; then
    t="$(mktemp)"; printf 'plain line\nName=Nexa SQL\nComment=한글 줄\n' > "$t"
    n=$(hangul "$t" | wc -l | tr -d ' '); rm -f "$t"
    if [ "$n" = 1 ]; then echo "selftest ✓ (한글 1줄 검출)"; exit 0; else echo "selftest ✗ (검출 $n)"; exit 1; fi
fi

check() { # check <파일> <ERE 필터(이 줄만 본다 · 비면 전체)> <설명>
    local f="$1" filt="$2" what="$3" out
    [ -f "$f" ] || { echo "? $f 없음($what)"; return; }
    out=$(hangul "$f" | grep -vE '^[0-9]+:[[:space:]]*(#|//|<!--)')
    [ -n "$filt" ] && out=$(printf '%s\n' "$out" | grep -E "^[0-9]+:.*($filt)")
    if [ -n "$out" ]; then hit "$f ($what)"; printf '%s\n' "$out" | head -5 | sed 's/^/    /'; else echo "✓ $f ($what)"; fi
}

check crates/nexa-sql/Cargo.toml   '^description'            'Cargo description'
check crates/nsql-cli/Cargo.toml   '^description'            'Cargo description'
check packaging/windows/nexa-sql.rc 'VALUE "'                 'exe VERSIONINFO'
check packaging/windows/nsql.rc     'VALUE "'                 'exe VERSIONINFO'
check packaging/windows/nexa-sql.wxs 'Name=|Description=|Manufacturer=|Comments=|Value=' 'MSI 글 필드'
check packaging/macos/Info.plist   '<string>'                'Info.plist'
check packaging/macos/distribution.xml '<title>|title='      'pkg distribution'
check packaging/linux/control      ''                        'deb control(주석 제외 전체)'
check packaging/linux/nexa-sql.spec '^[0-9]+:[^%]'           'rpm spec(글 줄)'
check packaging/linux/nexa-sql.desktop '^[0-9]+:[A-Za-z]+='  '.desktop(기본 키 · [ko] 지역화 키는 허용)'
check packaging/homebrew/nexa-sql.rb 'desc |name '           'cask desc'
check packaging/winget/locale.yaml  ''                       'winget locale'
check packaging/winget/installer.yaml ''                     'winget installer'
check packaging/winget/version.yaml ''                       'winget version'
check packaging/choco/nexa-sql.nuspec ''                     'choco nuspec'
check packaging/choco/tools/chocolateyinstall.ps1 ''         'choco install script'
check packaging/choco/tools/chocolateyuninstall.ps1 ''       'choco uninstall script'
check packaging/macos/scripts/postinstall 'echo|printf'      'postinstall 출력'
check packaging/macos/uninstall.sh  'echo|printf'            'uninstall 출력'
# pkg 환영문 = build-pkg.sh 안 heredoc(welcome.txt) 본문만
w=$(awk '/welcome.txt" <<EOF/{f=1;next} /^EOF$/{f=0} f' packaging/macos/build-pkg.sh | hangul_in)
if [ -n "$w" ]; then hit "packaging/macos/build-pkg.sh (pkg 환영문)"; printf '%s\n' "$w" | sed 's/^/    /'; else echo "✓ packaging/macos/build-pkg.sh (pkg 환영문)"; fi
# THIRD-PARTY-NOTICES 머리 = 생성 스크립트의 lines 리터럴(DBMS NOTICE.md 본문은 T-320 ③ 전까지 제외 · print()는 빌드 로그)
n=$(hangul scripts/third-party-notices.py | grep -E '^[0-9]+:[[:space:]]*(lines \+?= \[|"[^"]|f"|lines\.append)' | grep -v 'print(')
if [ -n "$n" ]; then hit "scripts/third-party-notices.py (THIRD-PARTY-NOTICES 머리글)"; printf '%s\n' "$n" | sed 's/^/    /'; else echo "✓ scripts/third-party-notices.py (THIRD-PARTY-NOTICES 머리글)"; fi
if [ "${1:-}" = "--notices" ] && [ -n "${2:-}" ]; then
    m=$(hangul "$2" | head -5)
    if [ -n "$m" ]; then hit "$2 (생성된 고지 전체 — DBMS NOTICE.md 본문 포함)"; printf '%s\n' "$m" | sed 's/^/    /'; else echo "✓ $2"; fi
fi
if [ "$fail" = 0 ]; then echo "모두 영어 ✓"; else echo "한글이 남은 자리가 있다 — 규칙 docs/33 §5-6"; fi
exit $fail
