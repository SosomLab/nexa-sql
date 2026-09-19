# 58 · 외부 파일 변경 — 병합 정책 · 확인 빈도 · 감지 시점 · 감시 구조 (조사 · 설계) (사용자 요청 09-19)

> **요구**: ① 외부에서 파일이 바뀌면 **병합 가능성을 검토해 문제없으면 바로 반영**, 확인이 필요하면 확인 창으로 "갱신 / 현재 내용 유지" ② 다른 편집기 동작 조사 · 더 좋은 방법 ③ 여러 번 바뀔 때 매번 물을지 · 한 번 묻고 일정 시간 막을지 · 한 번 묻고 무시할지 ④ 비활성 상태에서도 감지할지 · 활성(전환 시점)에서만 할지 ⑤ 성능을 유지하면서 최신성을 확보하는 감시 구조.
> **상태**: ✅ 구현(09-19 · journal 81차 · §5) — D-119~D-124 전부 권장안으로 확정(사용자 "전체 진행"). 앞선 조사 = [15 외부 파일 변경 처리](15-external-file-changes.md)(09-12 · git식 fast-forward/3-way 모델 채택). 이 문서는 15를 **정책·시점·구조까지** 구체화한다. (구현 전에는 편집기 탭에 외부 변경 감지가 없었다 — 설정 JSON 감시 `json_watch`뿐.)

## 0. 결론 여섯 줄

1. **바뀌지 않은 탭(clean)** = 묻지 않고 다시 읽는다 — 모든 주류 편집기의 합의. 통째로 바꾸지 말고 **디스크와의 차이를 편집 한 단계로 적용**해 캐럿·스크롤·실행 취소를 지킨다(Zed 방식).
2. **고친 탭(dirty)** = 3-way 병합(base = 마지막으로 읽거나 저장한 내용 `Editors.saved` · ours = 버퍼 · theirs = 디스크)을 **시도**한다. 겹치는 덩어리가 0이면 바로 반영하고 상태줄에 알린다(Ctrl+Z 한 번으로 되돌림). 겹치면 **비모달 띠**로 묻는다. ※ 조사한 11종 어디에도 자동 병합은 없다 — 우리가 더 나아가는 지점이라 **안전 장치 다섯**(§2-2)을 둔다.
3. **확인은 파일당 하나** — 같은 파일이 또 바뀌면 새 창을 쌓지 않고 그 띠의 내용만 갱신한다. "현재 내용 유지"는 **그 시점의 디스크 서명**을 기억해, 디스크가 다시 바뀔 때만 다시 평가한다(그때 병합이 되면 묻지 않고 반영). 자주 바뀌는 파일은 탭별 "조용히 따라가기".
4. **감지 시점** = 창 활성화 · 탭 전환 · **저장 직전**(필수 안전망) + 창이 활성인 동안 **보이는 탭 하나만** 2초 폴링. 창이 비활성이면 아무것도 하지 않는다(비용 0) — 외부 변경은 대개 사용자가 다른 앱에 가 있는 동안 일어나고, 돌아오는 순간 한 번에 잡힌다(JetBrains·Notepad++·Vim·Kate와 같은 선택).
5. **감시 구조** = OS 와처 없이 **stat 서명(크기 · mtime · 파일 식별자) + 내용 해시 확정**을 전용 스레드에서. 의존성 0 · 원자적 저장(temp+rename)·네트워크 드라이브·inotify 한도 같은 와처의 함정을 피한다. 포트(`FileWatch`)로 두어 나중에 OS 와처 어댑터를 끼울 수 있다.
6. 저장 시점에는 **항상 다시 검증**한다 — 디스크가 base와 다르면 저장을 멈추고 같은 띠로(VS Code "Compare / Overwrite" 자리). 내용이 같으면 통과.

## 1. 다른 편집기(09-19 재조사 · 15 §1 보강)

| 편집기 | clean | dirty | 반복 변경 | 감지 시점 | 감시 방식 |
|---|---|---|---|---|---|
| **VS Code** | 묻지 않고 재로드(끄는 설정 없음 · 요청 큐 ≤ 2로 합침) | **재로드 안 함** · 저장 때 Compare / Overwrite(`files.saveConflictResolution`) · mtime이 새로워도 크기·내용 같으면 통과 | 프롬프트 자체가 없음 | 상시 | parcel-watcher(OS 알림) · 단독 파일은 `fs.watch` → 실패 시 5초 폴링 · 삭제는 100 ms 뒤 재확인 후 "deleted" 표시 |
| **Zed** | **디스크와의 diff를 편집 트랜잭션으로** 적용(undo·앵커 보존) | `has_conflict` 표시 · 저장 때 Overwrite / Discard | 없음 | 상시 | OS 알림 — inode에 건 와처가 원자적 저장 뒤 영구 stale → **경고 없이 덮어쓴 버그**(#63174) |
| **JetBrains** | VFS refresh 때 재로드 | **모달** File Cache Conflict(Load FS / Keep Memory / Show Diff) — "데이터 손실 결정"이라 일부러 모달 | 없음 | 와처는 "바뀜" 표시만 · **반영은 IDE 창·탭 활성화 때** | fsnotifier + 타임스탬프 refresh |
| **Notepad++** | 기본 = 묻는다 · "Update silently" | 묻는다(Yes/No) | **매번** · 억제 = Update silently · 탭별 Monitoring(tail -f) | 창 활성화 · 현재 문서만/전부 옵션 | ReadDirectoryChangesW |
| **Kate** | 묻는다(Reload / Ignore / View Diff / **Enable Auto Reload**) | Overwrite 있는 프롬프트 | 문서별 Auto Reload = 닫을 때까지 조용히 · 여러 파일 한 대화상자 | 창/뷰 포커스 | — |
| **Sublime Text** | 묻지 않고 재로드(`always_prompt_for_file_reload` 기본 false) | 파일마다 대화상자(Reload / Ignore All / Cancel · "Reload All" 미구현 #4801) | 파일마다 | — | inotify 감시 수 한도 문제(#1195) |
| **Vim / Neovim** | `autoread` | W12 경고(OK / Load File) | 매번 | **버퍼 전환 · 쓰기 직전 · 포커스 복귀**(`checktime`) | mtime만 바뀌면 내용 비교(FAT 2초 해상도 대비) |
| **Emacs** | `auto-revert-mode` | 되돌리지 않음 | `auto-revert-tail-mode` | 상시 | 파일 알림 + **5초 폴링 병행** · NFS는 알림 무효 |
| **Visual Studio / SSMS** | 기본 = 묻는다 · "Reload modified files unless there are unsaved changes" | 묻는다(Yes / Yes to All / No / No to All) | Yes to All | 즉시 / 포커스 복귀 | — |
| **DBeaver** | 문서에는 File ▸ Revert뿐(Eclipse 플랫폼에 위임) | — | — | — | Eclipse 네이티브 훅 + 폴링 |

**읽을 점**: ① clean 무음 재로드는 합의 ② dirty는 두 갈래 — *즉시 모달*(JetBrains·Notepad++·VS) vs *저장 때까지 미룸*(VS Code·Zed). 즉시 모달은 "git 작업 중 포커스를 뺏는다"는 불만이 크다 ③ **자동 병합은 아무도 하지 않는다**(VS Code #47250 · Helix 보류) — 반대로 말하면 "dirty인데 겹치지 않는 변경"에서 모두가 사용자에게 일을 시킨다 ④ 반복 억제는 *탭별 조용히 따라가기*(Kate·Notepad++)가 유일한 실용 해법 ⑤ 감지 시점은 IDE급도 "활성화 때 반영"이 주류 ⑥ OS 와처는 함정이 많다(원자적 저장 · 네트워크 드라이브 · 감시 수 한도) — Emacs·VS Code도 폴링을 **병행**한다.

## 2. 정책 설계

### 2-1. 판정 흐름(파일 하나)

```text
서명 변화(크기·mtime·파일 식별자) 감지
  → 300 ms 뒤 서명이 같을 때만 읽는다(쓰는 중인 파일 · 원자적 저장의 삭제→생성 틈)
  → 내용 해시 == base 해시?            → 무시(touch · 동기화 클라이언트 · 내가 저장한 것)
  → 내용 == 버퍼?                       → base만 갱신(같은 수정을 양쪽에서 함) · clean으로
  → 버퍼 clean?                         → 차이를 편집 한 단계로 적용(fast-forward) · 상태줄 "디스크에서 다시 읽음"
  → 버퍼 dirty → 3-way 병합
        겹침 0 + 안전 조건 만족          → 바로 반영 · 상태줄 "외부 변경 n곳 병합 — Ctrl+Z로 취소" · 탭은 dirty 유지
        겹침 있음 / 안전 조건 불만족     → 비모달 띠(파일당 하나)
파일이 없어짐 → 300 ms 뒤 재확인 → 탭 유지 + "(deleted)" 표시 · 저장하면 되살림(버퍼는 절대 버리지 않는다)
```

### 2-2. 자동 병합의 안전 장치(아무도 안 하는 일을 하는 값)

1. **겹치는 덩어리 0일 때만** — 같은 줄·이웃 줄(간격 0)을 양쪽이 고쳤으면 충돌로 본다(git보다 보수적).
2. **편집 한 단계** — Ctrl+Z 한 번으로 병합 전으로. 병합된 줄은 거터에 표시(기존 줄 변경 띠 재사용 · 색만 다르게).
3. **병합 전 버퍼 백업** — `<설정 폴더>/backup/<탭 id>-<시각>.sql`(최근 10개 · 설정으로 개수).
4. **하지 않는 경우**: 인코딩·줄끝이 바뀜 · 파일이 절반 이하로 줄어듦 · 바이너리 · `file.external_merge_max_kb`(2048) 초과 · 실행 중인 탭(실행 범위의 줄 번호가 밀린다 → 실행이 끝난 뒤 평가).
5. **상태줄 + 로그 한 줄** — 무슨 일이 있었는지 나중에도 찾을 수 있게(로그 창 `file` 종류).

SQL은 덩어리가 겹치지 않아도 의미가 깨질 수 있다(위에서 컬럼을 지우고 아래에서 그 컬럼을 쓴다). 그래서 ① 되돌리기 쉬움 ② 표시 ③ 끌 수 있음(`file.external_merge = off` → dirty는 늘 띠로 묻는다)의 셋을 함께 둔다.

### 2-3. 확인 UI — 비모달 띠(편집기 위 · 탭마다)

```text
⚠ 디스크의 파일이 바뀌었고 고친 내용과 3곳이 겹칩니다.   [차이 보기] [디스크 내용으로 갱신] [현재 내용 유지] [✕ 조용히 따라가기…]
```

- **모달이 아니다** — 입력·실행을 막지 않고 포커스를 가져오지 않는다(JetBrains 모달의 대표 불만). 창이 비활성일 때 생긴 띠는 돌아왔을 때 보인다.
- **저장할 때만 모달** — 띠를 둔 채 저장하면 "디스크 내용을 덮어씁니다" 확인(덮어쓰기 / 차이 보기 / 취소). 데이터 손실 결정은 그 순간 하나뿐이다.
- "차이 보기" 1차 = 디스크 내용을 읽기 전용 새 탭으로(좌우 비교 뷰는 [19](19-compare-git-and-object-history.md) 비교 기능이 들어오면 교체).
- 사용자 표현의 "확인 창" = 이 띠 + 저장 시 모달로 해석했다. 즉시 모달을 원하면 D-120 ②.

### 2-4. 여러 번 바뀔 때(요구 ③)

| 방식 | 장점 | 단점 |
|---|---|---|
| 매번 새로 묻기(Notepad++·Sublime·Vim) | 놓치지 않음 | 빌드 산출물·로그·동기화 폴더에서 폭격 → 사용자가 습관적으로 닫아 **정작 중요한 것도 닫는다** |
| 한 번 묻고 일정 시간 막기 | 조용함 | 막힌 시간 안의 *다른* 변경을 놓친다 — 시간은 변경의 의미와 무관 |
| 한 번 묻고 그 뒤로 무시 | 가장 조용함 | 탭이 영구히 낡은 채 저장 때 터진다 |
| **★ 파일당 띠 하나를 갱신 + "유지"는 디스크 서명에 묶기 + 탭별 조용히 따라가기** | 창이 쌓이지 않음 · 새 변경은 다시 평가(병합되면 조용히 끝) · 자주 바뀌는 파일은 사용자가 한 번 정함 | 구현이 조금 더 든다 |

권장 = 마지막 줄. 세부: ① 띠가 떠 있는 동안 또 바뀌면 **띠의 숫자만 갱신**(깜빡임·소리·포커스 이동 없음) ② "현재 내용 유지" = 그 디스크 서명을 `ignored`로 기억 → 서명이 다시 바뀌면 재평가 ③ 같은 파일이 60초 안에 3번 바뀌면 띠에 **"이 탭은 조용히 따라가기"** 제안(clean이면 항상 재로드 · dirty면 병합되는 것만 · Kate "Enable Auto Reload") ④ 여러 파일이 한꺼번에 바뀌면(git checkout) 띠는 각 탭에 · 상태줄에 "3개 파일이 디스크에서 바뀜" 한 줄.

### 2-5. 감지 시점(요구 ④)

| 시점 | 대상 | 이유 |
|---|---|---|
| **창 활성화**(`Focused(true)`) | 열린 파일 전부 stat(50개 ≈ 1 ms 미만 · 스레드에서) | 외부 변경의 대부분은 사용자가 다른 앱에 있을 때 일어난다 |
| **탭 전환** | 그 탭 하나 | 오래 안 본 탭 |
| **저장 직전** | 그 탭 하나(동기 · 필수) | 어떤 감지도 놓칠 수 있다 — 마지막 안전망(Zed #63174 교훈) |
| **활성 창 폴링** | **보이는 탭 하나만** `file.external_poll_ms`(2000 · 0 = 끔) | 포매터·코드 생성기·동기화 클라이언트처럼 *작업 중에* 바뀌는 경우 |
| 창 비활성 | **아무것도 안 함** | 비용 0 · 어차피 돌아올 때 잡힌다 · 백그라운드에서 버퍼를 바꾸면 사용자가 본 적 없는 변경이 생긴다 |

비활성에서도 감지하는 편집기(VS Code·Zed)는 *작업 폴더 전체*를 색인하는 제품이라 와처가 이미 돈다. 우리는 열린 파일 몇 개가 전부라 "활성화 때 일괄 확인"이 같은 최신성을 훨씬 싸게 준다. 예외 = "조용히 따라가기"를 켠 탭은 비활성에서도 폴링한다(로그 tail 용도 · 간격 ×2).

### 2-6. 감시 구조(요구 ⑤)

```text
UI 스레드 ──(경로 목록 · 트리거)──▶ file-watch 스레드 ──(바뀐 파일: 서명 · 내용 · 해시)──▶ UI 스레드
                                    ① stat → 서명 비교(크기 · mtime ns · 파일 식별자)
                                    ② 다르면 300 ms 뒤 다시 stat — 같아질 때까지(쓰는 중)
                                    ③ 읽기 + 해시(FNV/xxh · 자체 구현) → base 해시와 같으면 버림
```

- **포트 + 어댑터**(30 §1): `trait FileWatch { fn watch(paths); fn check(which); fn poll() -> Vec<Changed> }` · 어댑터 ① `StatWatch`(기본 · 의존 0) ② (후보) OS 알림 어댑터 — **부모 폴더에 비재귀로** 걸고 이벤트는 "stat 해 보라"는 신호로만 쓴다(inode에 직접 걸지 않음).
- **UI 스레드에서 파일을 만지지 않는다** — 네트워크 드라이브의 stat은 초 단위로 막힐 수 있다. 저장 직전 확인만 동기(타임아웃 2초 · 넘으면 "확인하지 못함 — 그래도 저장?" ).
- **자기 저장 걸러내기** — 저장 직후의 (서명, 해시)를 base로 기록. 서명이 같으면 읽지도 않는다.
- **mtime 해상도**(FAT 2초) — 서명이 같아도 저장 직전에는 크기 + 해시까지 본다.
- **원자적 저장**(temp + rename) — 경로 기준 stat이라 자연히 따라간다(파일 식별자가 바뀐 것도 "바뀜"으로).
- **비용**: 유휴 = 0(비활성) / stat 1회·2초(활성 · 보이는 탭) · 부하원 원장 [39 §3](39-resource-governance.md) 등재 · 향상 모드(`perf.boost`) = 폴링 끔(활성화·탭 전환·저장 직전은 유지).
- 위치: 순수 로직(서명·해시·3-way 병합 `merge3`)은 `nexa-ui`의 `nexa-fs`(파일) · `nexa-ctl`(병합 — 이미 있는 `diff_lines`의 LCS를 재사용) → 다른 nexa 앱도 쓴다(부품 원장 30 §2 등재).

### 2-7. 설정(안 · 분류 = 파일)

| 키 | 기본 | 뜻 |
|---|---|---|
| `file.external_change` | `auto` | auto = clean 재로드 + dirty 병합 시도 / `ask` = 늘 띠로 묻기 / `off` = 감지 안 함(저장 직전 확인은 유지) |
| `file.external_merge` | on | dirty 탭의 겹치지 않는 변경을 묻지 않고 병합 |
| `file.external_merge_max_kb` | 2048 · HIDDEN | 이보다 크면 병합하지 않고 묻는다 |
| `file.external_check` | `focus_poll` | `focus` = 활성화·탭 전환·저장 직전만 / `focus_poll` = + 보이는 탭 폴링 |
| `file.external_poll_ms` | 2000 | 보이는 탭 폴링 간격(0 = 끔) |
| `file.external_settle_ms` | 300 · HIDDEN | 서명이 안정될 때까지 기다리는 시간 |
| `file.external_backup_keep` | 10 · HIDDEN | 병합 전 백업 보관 개수(0 = 백업 안 함) |

## 3. 결정(사용자 확인)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-119 ✅(09-19) | 고친 탭 + 겹치지 않는 외부 변경 | ① 묻지 않고 병합(안전 장치 다섯 · Ctrl+Z 한 번) ② 띠로 먼저 묻고 [병합] 버튼 ③ 병합 없음(VS Code식 · 저장 때 비교) | **①**(요구 그대로 · 다른 편집기에는 없는 기능이라 끌 수 있게) |
| D-120 ✅(09-19) | 확인이 필요할 때의 UI | ① 비모달 띠 + 저장 때만 모달 ② 감지 즉시 모달 대화상자(JetBrains식) ③ 띠 없이 저장 때만 | **①** |
| D-121 ✅(09-19) | 여러 번 바뀔 때 | ① 파일당 띠 하나 갱신 + "유지"는 디스크 서명에 묶기 + 탭별 조용히 따라가기 ② 매번 묻기 ③ 한 번 묻고 n초 막기 ④ 한 번 묻고 무시 | **①** |
| D-122 ✅(09-19) | 감지 시점 | ① 활성화·탭 전환·저장 직전 + 활성 창에서 보이는 탭 2초 폴링 ② 활성화·탭 전환·저장 직전만 ③ 비활성에서도 상시 | **①** |
| D-123 ✅(09-19) | 감시 방식 | ① stat 서명 + 해시 · 전용 스레드 · 의존 0 ② OS 와처 크레이트(`notify`) ③ ①로 시작 · 포트만 열어 둠 | **③**(= ① + 나중에 어댑터) |
| D-124 ✅(09-19) | 3-way 병합 구현 | ① 자체 구현(`diff_lines` LCS 재사용 · 의존 0) ② `diffy` 크레이트(15 §4-5의 1순위였음) | **①**(DR-3 외부 crate 0 지향 · 줄 단위 diff3는 200줄 안쪽) |

## 4. 단계(T-140)

① `nexa-ctl merge3`(base·ours·theirs → 결과 + 충돌 구간 · 테스트) ② `nexa-fs` 서명·해시·`StatWatch` 스레드 ③ `Editors` — base 해시·서명·`ignored` 서명·따라가기 플래그 · clean 재로드 = diff 적용 ④ 호스트 — 트리거 넷(활성화·탭 전환·저장 직전·폴링) · 띠 · 저장 모달 · 상태줄/로그 ⑤ 삭제/이름 변경 표시 ⑥ 설정 7키 · 39 §3 · 30 §2 · 실기표.

## 5. 구현(09-19 · journal 81차)

| 조각 | 코드 | 비고 |
|---|---|---|
| 3-way 병합 | `nexa-ui` `nexa-ctl/src/merge3.rs` `merge3` · `Merge3` | Myers O(ND) 줄 정합(편집 거리 1,500 상한 → 넘으면 `None` = 묻기) · 안정 줄이 사이에 없으면 한 덩어리(git보다 보수적) · 테스트 5 |
| 되돌리기 한 단계 교체 | `TextBox::replace_all_undoable` | 달라진 가운데만 · 캐럿은 같은 글자에 · 사용자가 보던 자리 유지 |
| 서명·감시 | `nexa-ui` `nexa-fs/src/watch.rs` `FileSig` · `file_sig` · `content_hash`(FNV-1a) · `StatWatch` | 전용 스레드 `file-watch` · 서명이 **연속 두 번 같을 때** 읽음 · 겹친 요청 합침 · 상한 초과 = 읽지 않음 |
| 판정(순수) | `extfile.rs` `decide(DecideIn) -> Decision{Ignore, AdoptBase, Reload, Merge, Ask}` | 판정표 테스트(조건 하나씩 뒤집기) |
| 탭 상태 | `extfile.rs` `ExtInfo{sig, pending, ignored, diverged, deleted, follow, recent}` | 호스트 `App.ext_files`(탭 id 열쇠 · 닫힌 탭은 틱에서 정리) |
| 확인 띠 | `extfile.rs` `Banner` · `Editors::set_top_inset/banner_rect` | 탭 줄과 본문 사이(본문을 가리지 않음) · 버튼 = 디스크 내용 보기 · 디스크 내용으로 갱신 · 현재 내용 유지 · (자주 바뀌면) 조용히 따라가기 |
| 트리거 | `WindowEvent::Focused(true)` = 전부 · `sync_grid_tab` 탭 전환 = 그 탭 · `ext_tick` 폴링 = 보이는 탭(활성 창만) · `save_to` → `ext_save_guard`(동기) | 실행 중인 탭은 건너뜀(서명을 갱신하지 않아 다음 확인 때 다시 온다) |
| 저장 직전 | `ext_save_guard` | 서명이 다르면 내용으로 확정 · 기준·버퍼와 다르면 **첫 저장을 막고 띠 + 상태줄 → 3초 안에 다시 저장(또는 [덮어쓰기]) = 진행** |
| 백업 | `ext_backup` → `<설정 폴더>/backup/<시각>-<파일명>` | 자동 병합 직전에만 · 최근 `file.external_backup_keep`개 |
| 따라가기 | 팔레트 "File: Follow External Changes (This Tab)" · 띠의 버튼 | 띠 없음(겹치면 내 내용 유지) · 비활성 창에서도 간격 ×2로 확인 |

**설계와 달라진 점**: ① D-120의 "저장 때만 모달" → 앱에 범용 모달이 없고 파괴적 동작은 **"3초 안에 한 번 더"** 관례(저장 안 한 탭 닫기 · 프로필 삭제)를 쓰므로 저장 덮어쓰기도 그 관례로(띠에 [덮어쓰기] 버튼 병행) ② "차이 보기" 1차 = 디스크 내용을 원래 구문의 읽기용 탭(No connection)으로 — 좌우 비교 뷰는 [19](19-compare-git-and-object-history.md) ③ 병합된 줄의 거터 표시는 기존 줄 변경 띠(기준 = 디스크)가 그대로 "내 변경"을 보여 주므로 따로 색을 두지 않았다(`Merge3.applied_lines`는 남겨 둠).

**검증**: 단위(`merge3` 5 · `watch` 2 · `replace_all_undoable` 1 · `extfile` 3) · 실제 GUI(입력 주입 없음 — 앱이 떠 있는 동안 스크립트가 파일을 고침): 고치지 않은 탭 = 조용히 다시 읽음 ✅ · 삭제 = 위험색 띠 ✅ · 다시 나타남 = 반영 ✅(수동 순서). ⏳ 실기: 고친 탭 병합(Ctrl+Z 한 번) · 겹침 띠의 세 버튼 · 저장 2단 확인 · git checkout으로 여러 파일 동시 변경 · 네트워크 드라이브.

## 출처

- VS Code: [File Watcher Internals](https://github.com/microsoft/vscode/wiki/File-Watcher-Internals) · [File Watcher Issues](https://github.com/microsoft/vscode/wiki/File-Watcher-Issues) · [textFileEditorModelManager.ts](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/services/textfile/common/textFileEditorModelManager.ts) · [fileService.ts](https://github.com/microsoft/vscode/blob/main/src/vs/platform/files/common/fileService.ts) · [#47250 merge 요청](https://github.com/microsoft/vscode/issues/47250)
- Zed: [buffer.rs reload](https://github.com/zed-industries/zed/blob/main/crates/language/src/buffer.rs) · [#63174 원자적 저장 뒤 stale](https://github.com/zed-industries/zed/issues/63174)
- JetBrains: [System Settings — Sync external changes](https://www.jetbrains.com/help/idea/system-settings.html) · [Virtual File System](https://plugins.jetbrains.com/docs/intellij/virtual-file-system.html) · [File cache conflict popup steals focus](https://youtrack.jetbrains.com/articles/SUPPORT-A-3277/File-cache-conflict-popup-steals-focus-when-using-Git-from-a-terminal)
- Notepad++: [Preferences — File Status Auto-Detection](https://npp-user-manual.org/docs/preferences/) · [#5141 reload clears undo](https://github.com/notepad-plus-plus/notepad-plus-plus/issues/5141)
- Kate: [Config dialog](https://docs.kde.org/trunk_kf6/en/kate/kate/config-dialog.html) · [bug 377505](https://bugs.kde.org/show_bug.cgi?id=377505)
- Vim: [options — autoread](https://vimhelp.org/options.txt.html) · [editing — timestamps](https://vimhelp.org/editing.txt.html) · Emacs: [Auto Revert](https://www.gnu.org/software/emacs/manual/html_node/emacs/Auto-Revert.html)
- Sublime: [always_prompt_for_file_reload](https://forum.sublimetext.com/t/always-prompt-for-file-reload/71236) · [#4801](https://github.com/sublimehq/sublime_text/issues/4801) · [#1195 inotify 한도](https://github.com/sublimehq/sublime_text/issues/1195)
- Visual Studio: [Documents options](https://learn.microsoft.com/en-us/previous-versions/visualstudio/visual-studio-2019/ide/reference/documents-environment-options-dialog-box)
