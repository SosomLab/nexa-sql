# 15 · 외부 파일 변경 처리 — 조사와 개발 방향 (git식 fast-forward / 3-way 병합 검토)

> ★ 09-19: 정책(자동 병합의 안전 장치) · 확인 빈도 · 감지 시점 · 감시 구조는 **[58](58-external-change-policy.md)** 에서 구체화(결정 D-119~124 · T-140).

> 사용자 요청(09-12): *"외부에서 파일이 수정된 경우 다른 프로그램은 어떻게 처리하는지 조사하고 개발 방향 정리. 텍스트 파일은 git처럼 fast-forward · 충돌 시 직접 병합 후 저장하는 개발자 친화 방식 도입 검토."*
> 조사(에이전트 · 웹): Sublime Text · VS Code · IntelliJ/DataGrip · Emacs · Vim/Neovim · Notepad++ · Zed · Helix · DBeaver + diff/merge 크레이트.

## 1. 다른 앱은 어떻게 하나

| 앱 | clean 버퍼 | dirty 버퍼 | 삭제/이름변경 | 세션 간 충돌 |
|---|---|---|---|---|
| **Sublime Text 4** | `reload_file_on_change: true` → **무음 재로드** | *"Has changed on disk. Do you want to reload it?"* Reload / Ignore All(해당 view만 끔) / Cancel ([포럼](https://forum.sublimetext.com/t/ignore-all-when-unsaved-changes-and-file-changed-on-disk-is-not-helpful/62643)) · 다중 파일 "Reload All" 미구현([#4801](https://github.com/sublimehq/sublime_text/issues/4801)) | — | `hot_exit: always`(기본) · `save_on_focus_lost`가 외부 변경을 덮어쓴 버그(4114 수정 · [#4176](https://github.com/sublimehq/sublime_text/issues/4176)) |
| **VS Code** | 자동 재로드 | 메모리 유지 → **저장 시** *"The content of the file is newer"* + **Compare / Overwrite**(2-way diff) · `files.saveConflictResolution: askUser|overwriteFileOnDisk` · 병합 미구현([#47250](https://github.com/microsoft/vscode/issues/47250)) | 탭 유지 + "(Deleted)" 표시, 저장하면 복원(`closeOnFileDelete: false`) | `files.hotExit` |
| **IntelliJ / DataGrip** | 프레임/탭 활성화 시 동기화 | 감지 즉시 **modal** "File Cache Conflict": Load FS Changes / Keep Memory Changes / Show Difference(**2-panel**) ([문서](https://www.jetbrains.com/help/idea/file-cache-conflict.html)) | — | 안전 쓰기(백업 후 저장) |
| **Emacs** | `auto-revert-mode` | 편집 시도 순간 *"changed on disk; really edit? (y, n, r)"* | revert 안 함 | `.#file` 락파일 → *"locked by user: (s, q, p)"* |
| **Vim / Neovim** | `autoread` + FocusGained/`:checktime` → 재로드. Neovim 0.13은 OS 워처로 즉시 | W12 *"changed and the buffer was changed in Vim as well"* → (O)K / (L)oad File | 삭제는 건너뛰고 마지막 텍스트 유지 | 스왑 파일 `E325: ATTENTION` |
| **Notepad++** | Auto-Detection + `Update silently` | *"modified by another program… reload and lose the changes?"* Yes/No(전체 적용 없음) | — | Monitoring(tail -f) 모드 |
| **Zed** | 자동 재로드 | conflict 표시 → 저장 시 modal Overwrite / Discard / Cancel. temp+rename 교체 시 inode 추적 실패로 **경고 없이 덮어쓴** 버그([#63174](https://github.com/zed-industries/zed/issues/63174)) | dirty 삭제 파일 처리 불완전 | — |
| **Helix** | 자동 재로드 없음("not planned") | 저장 거부 → `:write!` 또는 `:reload` 후 undo. diff3 제안 보류 | — | — |
| DBeaver | 공식 감지 문서 없음 · `File → Revert`만 | — | — | — |

**합의**: (a) clean → 조용히 재로드, (b) dirty → 메모리 유지 + conflict 표시, **결정은 저장 시점으로 지연**(Compare/Overwrite/Reload). IntelliJ만 즉시 modal. **자동 병합을 하는 주류 에디터는 없다**(VS Code·Helix 모두 미구현 요청 상태).

## 2. 감지 계층의 함정 (구현 시 반드시)

- **temp+rename 원자 저장**(다른 에디터의 관행) → CREATE/RENAME/WRITE 다중 이벤트 · inode 변경. **경로 기반 감시 + dev/inode 재확인**.
- **debounce** 100~200ms(Neovim 100ms 타이머 · rename 후 워처 재등록).
- **mtime 해상도**(FAT 2초 · ext3 1초) → `(size, mtime_ns, dev/inode)` 변화 후 **내용 해시로 최종 판정**.
- **자기 저장 오탐**(Notepad++ #13897 · IntelliJ IJPL-2214) → 저장 직후 "예상 해시"를 기록해 자기 이벤트 무시.
- **동기화 폴더(iCloud/Dropbox/NFS)** — 사용자의 Sublime 구성이 iCloud 동기화([12](12-user-sublime-profile.md))라 실제 상황. "내용이 같으면 무시" 규칙 필수.

## 3. git식 모델 검토 — 채택 (조건부)

| git | 편집기 대응 | 판정 |
|---|---|---|
| fast-forward(로컬 변경 없음) | clean 버퍼 → 무음 재로드 | ✅ 정확히 대응 |
| 3-way merge(base · ours · theirs) | base = **버퍼가 마지막으로 디스크와 일치했던 스냅샷**(로드/저장/재로드 직후) · ours = 버퍼 · theirs = 디스크 | ✅ 라인 지향 스크립트에 diff3가 잘 맞음 |
| 충돌 마커 `<<<<<<<`/`=======`/`>>>>>>>` (`zdiff3`면 `|||||||` base 포함) | SQL에 마커가 남은 채 실행되면 파서 오류 → 사용자가 놀란다 | ⚠️ **기본은 3-pane 병합 뷰**(base/디스크/버퍼) · 마커 삽입은 고급 옵션 |
| non-conflicting hunk 자동 적용 | 겹치지 않는 변경은 자동 반영, 충돌 hunk만 사용자에게 | ✅ 단 자동 적용 결과는 "dirty + 병합됨" 배지로 표시 |

## 4. 개발 방향 (결정 → DR-21 후보)

1. **기본 정책**: `reload_file_on_change: true`(뷰별 끄기). dirty 버퍼는 즉시 바꾸지 않고 **탭 배지 + 비모달 토스트**("디스크에서 변경됨 — 병합 / 다시 불러오기 / 무시"). 저장 시 conflict면 저장을 막고 병합 뷰.
2. **병합 흐름**: 워처 → 해시 확인 → clean이면 fast-forward · dirty면 diff3 시도 → 충돌 0이면 자동 적용(배지) · 충돌 있으면 3-pane 뷰(수용/거부/직접 편집) → 저장 = 병합 결과 + base 갱신.
3. **삭제/이름변경**: 탭 유지 + "(deleted)" · 저장하면 복원(VS Code).
4. **자동 저장(`save_on_focus_lost`)** 기본 off. 켜더라도 conflict 버퍼는 자동 저장 제외(Sublime #4176 교훈).
5. **크레이트**: `diffy`(MIT/Apache · `merge(ancestor, ours, theirs)` · `ConflictStyle::Diff3`) 1순위 — 단순. hunk별 해결 UI가 필요해지면 `similar 3.x`의 `TextMerge`. 원장 등재 대상(DR-3 예외는 아니므로 D 결정 필요 → D-8).
6. **미저장 버퍼 복원(hot exit)** 과 결합: 세션 파일에 버퍼 내용 + base 스냅샷 해시를 함께 저장해 재시작 후에도 3-way가 가능하게([18](18-session-and-projects.md)).
7. **구현 위치**: `nexa-edit::buffer::disk`(스냅샷·해시·판정 · 순수 로직 테스트) + `nsql-plat::watch`(OS 워처 · debounce) + GUI 병합 뷰(M3 후반).
