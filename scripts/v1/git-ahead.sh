#!/usr/bin/env bash
# V1 git-ahead — (상태줄 git 세그먼트 `main ↑2 ↓1 (n)` · 로컬 추적 ref만 · fetch 0) 판정.
#   준비 = 로컬 bare 원격(파일 경로 · 네트워크 0) · 로컬 커밋 2(ahead 2) · 다른 클론이 1 push → 로컬 `git fetch`(준비 단계 · 파일 경로) = behind 1.
#   앱 판정 = `statusbar.dump:<파일>`(다음 프레임이 상태줄 세그먼트를 `id<탭>글` 한 줄씩 · 11-22 추가).
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1ab-XXXX")
G=(git -c user.name=v1 -c user.email=v1@example.invalid -c init.defaultBranch=main -c core.autocrlf=false)
"${G[@]}" init -q --bare "$WK/origin.git"
"${G[@]}" init -q "$WK/a"; printf 'SELECT 1 FROM dual;\n' > "$WK/a/a.sql"
"${G[@]}" -C "$WK/a" add a.sql; "${G[@]}" -C "$WK/a" commit -q -m c0
"${G[@]}" -C "$WK/a" branch -M main; "${G[@]}" -C "$WK/a" remote add origin "$WK/origin.git"; "${G[@]}" -C "$WK/a" push -q -u origin main
"${G[@]}" clone -q "$WK/origin.git" "$WK/b"
printf 'SELECT 2 FROM dual;\n' > "$WK/b/b.sql"; "${G[@]}" -C "$WK/b" add b.sql; "${G[@]}" -C "$WK/b" commit -q -m b1; "${G[@]}" -C "$WK/b" push -q origin main
for i in 1 2; do printf -- "-- a$i\n" >> "$WK/a/a.sql"; "${G[@]}" -C "$WK/a" commit -q -am "a$i"; done
"${G[@]}" -C "$WK/a" fetch -q origin
printf -- '-- dirty\n' >> "$WK/a/a.sql"   # 변경 1(괄호 안 수)
# 추적 없는 저장소
"${G[@]}" init -q "$WK/n"; printf 'SELECT 3 FROM dual;\n' > "$WK/n/n.sql"; "${G[@]}" -C "$WK/n" add n.sql; "${G[@]}" -C "$WK/n" commit -q -m n0; "${G[@]}" -C "$WK/n" branch -M main
# 준비 판정(앱이 부를 명령과 같은 것)
expect_eq "prep-ab-2-1" "$(git -C "$WK/a" rev-list --left-right --count 'HEAD...@{u}' | tr -s '\t ' ' ')" "2 1"
expect_eq "prep-no-upstream-fails" "$(git -C "$WK/n" rev-list --left-right --count 'HEAD...@{u}' >/dev/null 2>&1; echo $?)" "128"
# 앱 판정(`statusbar.dump` · 상태줄 세그먼트 글)
O=$(v1_m "$V1_OUT"); AM=$(v1_m "$WK/a/a.sql"); NM=$(v1_m "$WK/n/n.sql")
run() { local tag=$1 file=$2; shift 2; v1_home en; v1_set "statusbar.git_secs=1" "$@"
  v1_gui "$tag" "open:$file,@after:3500:statusbar.dump:$O/$tag-sb.txt,@after:3900:file.exit" 25; rm -rf "$V1_HOME"; V1_HOME=""; }
run ab1 "$AM" "statusbar.git=on"
run ab0 "$NM" "statusbar.git=on"
run abx "$AM" "statusbar.git=off"
expect "ab1-arrows" "$V1_OUT/ab1-sb.txt" "$(printf 'git	main ↑2 ↓1 (1)')"
expect "ab0-plain" "$V1_OUT/ab0-sb.txt" "$(printf 'git	main (0)')"
expect_not "ab0-no-arrow" "$V1_OUT/ab0-sb.txt" "↑"
expect_eq "abx-dump-ran" "$([ -f "$V1_OUT/abx-sb.txt" ] && echo y)" "y"
expect_not "abx-no-git-seg" "$V1_OUT/abx-sb.txt" "$(printf 'git	')"
expect "abx-other-segs" "$V1_OUT/abx-sb.txt" "$(printf 'pos	Ln 1')"   # 덤프가 실제로 그려진 상태줄을 담았다(부정 판정 전제)
for t in ab1 ab0 abx; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
