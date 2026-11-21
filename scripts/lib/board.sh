#!/usr/bin/env bash
# board.sh — 작업판(사용자 11-21 "어떤 작업이 언제 시작해 어느 정도 진행됐고 언제쯤 끝날지" · D-491). 두 세션 공용.
#   저장 = target/status/board.tsv(한 줄 = 사건 · 덧붙이기만 = 잠금 없이 동시 쓰기에 안전) → 읽을 때 id별 마지막 상태로 접는다.
#   사람용 = target/status/now.md(사건마다 다시 만든다 · VS Code에서 열어 두면 저절로 갱신).
# 사용(직접 실행 또는 source 뒤 `board …`):
#   bash scripts/lib/board.sh start <id> <작업자> "<제목>" <예상 분> [--by <적은 이>]
#        작업자 = 개발 | 협업 | 하위:<이름>(하위 에이전트) | 원격:<이름>(다른 PC Remote Control · 클라우드 세션)
#        --by = 작업자가 판에 직접 쓸 수 없어 마스터 세션이 대신 적을 때 그 세션 이름(규칙 ⑨ · 사용자 11-21)
#   bash scripts/lib/board.sh step  <id> <n>/<N> "<지금 단계>" [<남은 초(ETA 계산용)>] [--seen] [--ev "<진행 증거>"]
#        --seen = 마스터가 그쪽 보고·기록을 확인한 시각으로 남긴다(now.md "확인 21:40(12분 전)")
#        --ev   = 확인 때 본 진행 증거 한 줄(단계 · 새 커밋 해시 · 산출 파일 수) — 지난 확인과 같으면 "정체 1회",
#                 두 번 연속 같으면 now.md 맨 위 "✗ 원격 <이름> 진행 없음(N분)"(살아 있음 ≠ 진행 · 규칙 ⑩ · 사용자 11-21)
#   bash scripts/lib/board.sh ack   <id>   # 위임 직후 작업자의 첫 응답을 확인한 시각(없으면 now.md "응답 미확인")
#   bash scripts/lib/board.sh done  <id> ok|fail|skip "<한 줄 결과>"
#   bash scripts/lib/board.sh alert <id> "<첫 실패 · 경고 한 줄>"   # now.md 맨 위 ✗ 줄(진행 중인 동안)
#   now.md 맨 위 ⚠ "멈춤 의심" = 마지막 step 신호가 3분 넘게 없거나 시작 뒤 예상 분의 3배를 넘김.
#   대리 기록(--by)인 일은 스스로 신호를 못 보내므로 "멈춤 의심" 대신 "⏰ 확인할 때"(마지막 확인 뒤 10분 넘음 = 마스터가
#   10분마다 확인 · 규칙 ⑩) · "⚠ 예상의 3배"로 구분한다.
#   bash scripts/lib/board.sh render   # now.md만 다시
# 규칙: 5분 넘게 걸릴 일(빌드 · 시험 · 측정 · 하위 에이전트 조사 · 긴 번역)은 시작 전에 start, 끝나면 done(docs/claude-batch-flow.md §6).
# 칸(TSV): epoch초 · id · 사건(start|step|ack|done|alert) · 작업자 · 제목 · 예상 분 · 진행(n/N) · 메모 · 결과 · 남은 초 · 적은 이 · 확인 표시 · 진행 증거
_BOARD_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOARD_DIR="${BOARD_DIR:-$_BOARD_ROOT/target/status}"
BOARD_TSV="$BOARD_DIR/board.tsv"; BOARD_MD="$BOARD_DIR/now.md"
_board_clean() { printf '%s' "$1" | tr '\t\r\n' '   '; }
_board_add() { mkdir -p "$BOARD_DIR"; printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date +%s)" "$@" >> "$BOARD_TSV"; }
_board_hm() { date -d "@$1" +%H:%M 2>/dev/null || date -r "$1" +%H:%M 2>/dev/null || echo "$1"; }
board_render() {
  mkdir -p "$BOARD_DIR"; touch "$BOARD_TSV"
  local now; now=$(date +%s)
  # 24시간 지난 완료 id의 사건을 걷어낸다(파일이 2000줄을 넘을 때만 · 임시 파일 + mv).
  if [ "$(wc -l < "$BOARD_TSV")" -gt 2000 ]; then
    awk -F'\t' -v cut=$((now - 86400)) 'NR==FNR { if ($3=="done" && $1<cut) old[$2]=1; next } !($2 in old)' "$BOARD_TSV" "$BOARD_TSV" > "$BOARD_TSV.tmp" && mv "$BOARD_TSV.tmp" "$BOARD_TSV"
  fi
  awk -F'\t' -v now="$now" '
    function hm(t,  c) { c = "date -d @" t " +%H:%M 2>/dev/null"; c | getline r; close(c); return r }
    function dur(s) { return (s >= 3600 ? int(s/3600) "h" : "") int((s%3600)/60) "m" (s < 600 ? sprintf("%02ds", s%60) : "") }
    function ago(t) { return hm(t) "(" int((now-t)/60) "분 전)" }
    { id=$2 }
    $3=="start" { st[id]=$1; ses[id]=$4; ti[id]=$5; est[id]=$6; by[id]=$11; state[id]="running"; prog[id]=""; note[id]=""; rem[id]=""; upd[id]=0; chk[id]=0; ack[id]=0; ev[id]=""; evt[id]=0; stl[id]=0; ord[++n]=id }
    $3=="ack"   { ack[id]=$1 }
    $3=="step"  { prog[id]=$7; note[id]=$8; rem[id]=$10; upd[id]=$1
                  if ($12=="seen") { chk[id]=$1; if (!ack[id]) ack[id]=$1 }
                  if ($13!="") { if ($13==ev[id]) stl[id]++; else { stl[id]=0; ev[id]=$13; evt[id]=$1 } } }
    $3=="done"  { state[id]=$9; en[id]=$1; res[id]=$8 }
    $3=="alert" { al[id]=$8 }
    END {
      print "# 작업판 — 갱신 " hm(now) "\n"
      # ★ 첫 실패 · 멈춤 의심은 맨 위에(진행 중인 일만 · 규격 ⑥ · ④).
      for (i=n; i>=1; i--) { id=ord[i]; if (seen0[id]++) continue; if (state[id]=="running" && al[id]!="") print "> ✗ **" ti[id] "**: " al[id] "\n" }
      for (i=n; i>=1; i--) { id=ord[i]; if (seen1[id]++) continue; if (state[id]!="running") continue
        if (by[id]!="") { last=(chk[id]>0?chk[id]:st[id])
          if (stl[id]>=2) print "> ✗ " ses[id] " 진행 없음(" int((now-evt[id])/60) "분): **" ti[id] "** — 증거 \"" ev[id] "\"가 " stl[id] "번 연속 같음 → 사용자 고지 + 재지시·회수·다른 작업자\n"
          if (!ack[id]) print "> ⚠ 응답 미확인: **" ti[id] "** (" ses[id] ") — 위임 " ago(st[id]) "\n"
          if (now-last > 600) print "> ⏰ 확인할 때: **" ti[id] "** (" ses[id] " · 대리 " by[id] ") — 마지막 확인 " ago(last) "\n"
          else if (est[id]>0 && now-st[id] > est[id]*60*3) print "> ⚠ 예상의 3배: **" ti[id] "** (" ses[id] ")\n"
          continue }
        if (upd[id]>0 && now-upd[id] > 180) print "> ⚠ 멈춤 의심: **" ti[id] "** — 신호가 " int((now-upd[id])/60) "분째 없음\n"
        else if (est[id]>0 && now-st[id] > est[id]*60*3) print "> ⚠ 멈춤 의심: **" ti[id] "** — 예상의 3배를 넘김\n" }
      print "## 진행 중\n\n| 제목 | 작업자 | 적은 이 | 시작 | 경과 | 진행 | 지금 단계 | 예상 종료 |\n|---|---|---|---|---|---|---|---|"
      run=0
      for (i=n; i>=1; i--) { id=ord[i]; if (seen[id]++) continue; if (state[id]!="running") continue; run++
        eta = (rem[id]!="" ? upd[id]+rem[id] : (est[id]>0 ? st[id]+est[id]*60 : 0))
        late = (eta>0 && now>eta) ? " ⚠ 늦음" : ""
        who = "자기"
        if (by[id]!="") { who = "대리:" by[id] (ack[id] ? " · 응답 " hm(ack[id]) : " · 응답 미확인") (chk[id]>0 ? " · 확인 " ago(chk[id]) : " · 확인 없음")
          if (chk[id]>0 && now-chk[id] > 600) who = who " ⏰"
          if (ev[id]!="") who = who " · 증거 " ev[id] (stl[id]==1 ? " (정체 1회)" : (stl[id]>=2 ? " (정체 " stl[id] "회)" : "")) }
        printf "| %s | %s | %s | %s | %s | %s | %s | %s%s |\n", ti[id], ses[id], who, hm(st[id]), dur(now-st[id]), (prog[id]==""?"-":prog[id]), (note[id]==""?"-":note[id]), (eta>0?hm(eta):"예상 없음"), late }
      if (!run) print "| (없음) | | | | | | | |"
      print "\n## 최근 끝난 일\n\n| 제목 | 작업자 | 결과 | 소요 | 끝 | 한 줄 |\n|---|---|---|---|---|---|"
      k=0; delete seen
      for (i=n; i>=1 && k<5; i--) { id=ord[i]; if (seen[id]++) continue; if (state[id]=="running") continue; k++
        printf "| %s | %s | %s | %s | %s | %s |\n", ti[id], ses[id], state[id], dur(en[id]-st[id]), hm(en[id]), res[id] }
      if (!k) print "| (없음) | | | | | |"
    }' "$BOARD_TSV" > "$BOARD_MD.tmp" && mv "$BOARD_MD.tmp" "$BOARD_MD"
}
board() {
  local cmd=${1:-render}; shift || true
  # 선택 인자 --by <이름> · --seen 은 어디에 와도 된다(위치 인자에서 걷어낸다).
  local by="" seenf="" evv="" a=(); while [ $# -gt 0 ]; do case "$1" in --by) by=$(_board_clean "${2:-}"); shift 2;; --seen) seenf=seen; shift;; --ev) evv=$(_board_clean "${2:-}"); shift 2;; *) a+=("$1"); shift;; esac; done
  set -- "${a[@]}"
  case "$cmd" in
    start) _board_add "$1" start "$(_board_clean "$2")" "$(_board_clean "$3")" "${4:-0}" "" "" "" "" "$by" "" "" ;;
    step)  _board_add "$1" step "" "" "" "$2" "$(_board_clean "${3:-}")" "" "${4:-}" "" "$seenf" "$evv" ;;
    ack)   _board_add "$1" ack "" "" "" "" "" "" "" "" "" "" ;;
    done)  _board_add "$1" done "" "" "" "" "$(_board_clean "${3:-}")" "${2:-ok}" "" "" "" "" ;;
    alert) _board_add "$1" alert "" "" "" "" "$(_board_clean "${2:-}")" "" "" "" "" "" ;;
    render) ;;
    *) echo "board start|step|ack|done|alert|render"; return 2 ;;
  esac
  board_render
}
# 직접 실행이면 바로 처리(source면 함수만 정의).
if [ "${BASH_SOURCE[0]}" = "$0" ]; then board "$@"; fi
