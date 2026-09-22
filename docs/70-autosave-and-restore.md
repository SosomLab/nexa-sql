# 70 · 자동 저장 · 저장 큐 · 프로젝트 복원 — 조사 · 설계 · 권장안

> **요구(사용자 09-22)**: 프로젝트 모드가 시작되면 **열린 파일 목록이 그대로 탭으로 순서대로 복원**되고 **마지막 캐럿까지** 돌아온다(단, **DBMS 접속만 사용자가 직접**). 그러려면 명시적으로 저장할 수 없는 탭(메모리 탭 · 미저장 변경)은 **성능에 영향을 주지 않는 선에서 주기적으로 자동 저장**돼야 한다. "파일 100개를 연 채 전체 저장"처럼 짧은 시간에 같은 요청이 쏟아지면 **저장 큐에 담아 틱 단위로 필요한 것만** 하고, **같은 액션은 마지막 요청만 살리되 대기 시간을 재설정**(만료까지 같은 요청이 없을 때 수행)한다. **참조 파일(디스크에 있는 파일)은 자동 저장에서 제외** — 대신 복원 가능한 **별도 백업 폴더**에 임시 저장했다가 **유효기간이 지나면 정리**하고, **최종 저장은 사용자 몫**이라는 직관을 지킨다. 다른 편집기·IDE의 동작을 검토해 좋은 방법을 제안한다.
>
> 선행: [67 프로젝트](67-project-workspace.md) §2-3·§2-4(저장 시점·복원 초안 · 이 문서가 대체) · [67a 조사](67a-project-research.md) · [58 외부 변경 정책](58-external-change-policy.md)(복원 때 디스크와 다르면) · [59 대용량](59-large-file-handling.md)(큰 버퍼 예외) · [60 되돌리기](60-undo-redo-redesign.md)(`undofile` = 같은 저장소 규약) · [39 자원 거버넌스](39-resource-governance.md)(부하원 등재) · [61 §1 불변식](61-core-design-and-working-rules.md)(본문 전체를 문자열로 뜨는 길은 자주 도는 길에 두지 않는다).

## 0. 결론 여섯 줄

1. **두 층으로 나눈다** — ① **워크스페이스 상태**(탭 목록·순서·활성·캐럿·스크롤·접힘·프로젝트 파일)와 ② **본문 백업**(메모리 탭 전체 · 파일 탭은 *미저장 변경분*만). 둘 다 **원본 파일을 건드리지 않는다**(VS Code hot exit · Sublime 세션과 같은 원칙 — 최종 저장은 사용자).
2. **저장 큐 부품 `SaveQueue`**(nsql-core · 의존 0): 열쇠 = (액션, 대상) · `push(key, delay, max_wait)` = 같은 열쇠가 또 오면 **대기만 재설정**(trailing debounce) · 처음 온 시각 + `max_wait`가 지나면 **강제 수행**(무한 연기 방지) · `tick(now)`가 만료된 것만 돌려준다 · 마지막 요청의 페이로드만 남긴다. "100개 저장 → 프로젝트 파일 1번"이 저절로 된다.
3. **쓰기는 UI 스레드 밖**: 틱은 "무엇을 쓸지"만 고르고, 본문 스냅숏(`buf()` 세대 비교 → 바뀐 것만 문자열로) 과 디스크 쓰기(`tmp` → `rename` 원자적)는 **백업 스레드 하나**가 순서대로. 큰 버퍼(`project.backup_max_mb` 8)는 스냅숏 대신 "저장하세요" 안내(Sublime 4151·큰 파일 모드와 연동).
4. **참조 파일 백업 = `<설정 폴더>/backups/files/<경로 해시>/`** — 최신 1개 + 원본 해시·mtime 프리앰블(VS Code 형식) · 저장·되돌려서 clean·탭 버림이면 **즉시 삭제** · `project.backup_days`(7) 지나면 정리(JetBrains Local History 5일 · Notepad++ 백업 폴더 관례). 메모리 탭은 `backups/<워크스페이스 id>/<탭 id>.txt`.
5. **복원 = 묵시**(VS Code·Sublime): 프로젝트를 열면 워크스페이스의 탭을 순서대로 다시 만들고 파일은 **디스크를 다시 읽고**(참조 = 최신) 백업이 있으면 그 본문을 얹은 뒤 **원본 해시가 디스크와 다르면 탭 띠**(58 `extfile.rs`: 유지 / 디스크 / 비교) · 캐럿·스크롤은 줄·열을 **클램프**해 복원 · 접속은 표식만(자동 재접속 없음 · 26 §8).
6. 설정 6개 + 프리셋(`perf.boost`면 디바운스 2배·상한 4배) · 부하원 원장 등재 · MC/DC 테스트(큐 규칙 · 복원 판정) · 단계 P3-a~e(§7).

---

## 1. 다른 편집기·IDE는 어떻게 하나

| 제품 | 미저장 본문 | 주기·트리거 | 원본 파일을 자동으로 덮는가 | 세션/탭·캐럿 복원 | 백업 정리 |
|---|---|---|---|---|---|
| **Sublime Text** | `hot_exit`(`always`/`only_on_quit`/`disabled`) — **세션 파일**(`Session.sublime_session` · 프로젝트는 `<이름>.sublime-workspace`)에 **미저장 버퍼 본문까지** 통째로 | 종료·창 닫기·프로젝트 전환 때 + 주기 저장(세션 자동 저장 · 수 초) | **아니오**(파일 탭의 변경분은 세션 안에 · 원본 그대로) | 예 — 탭 순서·활성·**캐럿·선택·스크롤·접힘**까지(`selection`·`viewport`) | 세션 파일 하나가 덮어쓰기 · 4151부터 큰 버퍼는 세션에 넣지 않고 종료 때 저장을 묻는다 |
| **VS Code** | `files.hotExit`(`onExit`/`onExitAndWindowClose`/`off`) — **working copy backup** `Backups/<workspace>/<scheme>/<hash>` · 첫 줄 프리앰블(리소스 · 메타) | 내용 바뀌면 `scheduleBackup` — **디바운스 1초**(`BACKUP_SCHEDULE_DELAY`) · 종료 때 대기 중 백업 flush | **아니오**(별개 `files.autoSave`가 있음: `afterDelay`(1s) / `onFocusChange` / `onWindowChange` — 이건 원본을 덮는 다른 기능) | 예 — `workspaceStorage`에 편집기 그룹·탭·**뷰 상태(캐럿·스크롤)** · 시작 때 **묵시 복원**(프롬프트 없음) | 저장·clean·닫기(버림)면 백업 즉시 삭제 · 고아 백업은 다음 시작 때 복원 목록으로 |
| **JetBrains(DataGrip 등)** | 미저장 개념이 거의 없다 — **자동 저장이 원본을 덮는다**(`Save files if the IDE is idle for 15 s` · 프레임 비활성화 · 빌드/실행 전) + **Local History**(내부 스냅숏 · 기본 5일) | 유휴 15초 · 포커스 잃음 | **예**(사용자 의도와 다를 수 있어 우리 요구와 반대) | 예 — 프로젝트 `workspace.xml`(탭·캐럿·접힘) | Local History 5일(`localHistory.daysToKeep`) |
| **DBeaver** | 스크립트 = 파일 · 편집기 닫을 때 자동 저장(옵션) · **Local History**(Eclipse · 7일·50개·1 MB 상한) | 닫기·주기(Eclipse 자동 저장 옵션 · 기본 끔) | 옵션에 따라 예 | 예 — Eclipse 워크벤치 상태(열린 편집기·캐럿) | Eclipse Local History 정리 규칙 |
| **Notepad++** | **주기 백업**(`Backup on save` + `Enable session snapshot and periodic backup` · **7초**) → `backup\` 폴더에 `<이름>@<시각>` | 7초 · 종료 | **아니오**(원본 그대로 · 복원은 백업에서) | 예 — `session.xml`(탭·위치·캐럿) · "Remember current session" | 세션이 닫히면 해당 백업 삭제 · 폴더에 쌓이면 사용자가 정리(`Backup on save`는 무한) |
| **Vim** | **swap 파일**(`.file.swp` · `updatecount` 200타·`updatetime` 4초) | 타수/시간 | 아니오 | `viminfo`/세션 스크립트(`:mksession` · 캐럿 `'"` 마크) | swap은 종료 때 삭제 · 크래시면 다음 열기에 복구 제안 |
| **Emacs** | **auto-save**(`#file#` · 300타/30초 · `auto-save-visited-mode`는 원본 덮기) + `file~` 백업 | 타수/시간 | 기본 아니오(옵션 예) | `desktop-save-mode` | `delete-auto-save-files` · 백업 `kept-new-versions` |
| **TextMate / Nova / Zed** | Zed = **자동 저장 옵션**(`autosave: off / after_delay / on_focus_change / on_window_change`) + 미저장 버퍼 세션 복원 | — | 옵션 | 예(Zed 세션) | — |

**읽는 법**: "원본을 덮지 않고 별도 백업 + 묵시 복원"(Sublime · VS Code · Notepad++ · Vim · Emacs) vs "자동 저장이 원본을 덮음"(JetBrains · Zed/VS Code의 autoSave 옵션). 사용자 요구 = 앞쪽. **디바운스 1초 · 상한 수 초 · 저장/clean/버림이면 백업 즉시 삭제 · 종료 때 flush · 큰 버퍼 예외**가 공통 관례이고, JetBrains/DBeaver의 **유효기간(5~7일) Local History**가 "참조 파일 임시 저장 정리"의 선례다.

---

## 2. 모델 — 무엇을 어디에

```
<설정 폴더>/
  workspaces/<워크스페이스 id>.json      ← 탭 목록·순서·활성·캐럿·스크롤·접힘·프로젝트 경로(프로젝트 없음 = "default")
  backups/
    <워크스페이스 id>/<탭 id>.txt         ← 메모리 탭 본문(제목 · 구문 · 세대 프리앰블)
    files/<경로 해시>.txt                 ← 참조 파일의 **미저장 변경 본문**(원본 경로 · 원본 해시·mtime · 세대 프리앰블)
  undo/<경로 해시>.nsqu                    ← (기존 60) 되돌리기 기록
<프로젝트 폴더>/<이름>.nsql-project        ← 폴더 목록(67 · VCS)
```

- **워크스페이스 id** = 프로젝트 파일 정규화 경로의 해시(`undofile::name_for` 규약 재사용 · Windows 대소문자 무시) · 프로젝트 없음 = `default`(D-148 ①). 프로젝트 파일에는 **개인 상태를 넣지 않는다**(VCS 공유 · Sublime `.sublime-project` ↔ `.sublime-workspace` 분리 · 67 §0-1).
- **탭 항목**: `{ kind: file|memory, path?, title, id, order, active, caret: {line, col}, sel?: [a,b], scroll: {top_line, left_px}, folds?: [...], syntax?, encoding, eol, dirty, backup?: "<파일명>", read_only, large }`. 캐럿·스크롤은 **글자 좌표**(줄·열 — 바이트 아님 · 61 §1) · 복원 때 본문 길이로 **클램프**.
- **백업 파일 프리앰블**(VS Code 관례 · 첫 줄 JSON): `{"v":1,"path":…,"orig_hash":…,"orig_mtime":…,"gen":…,"saved_at":…}` 다음 줄부터 본문(UTF-8 · 줄끝 `\n` · 원래 줄끝은 메타에).
- 접속: 탭별 **표식만**(프로필 이름 · 전용/공유 · 세션 모드) — 복원 때 재접속하지 않는다(사용자 요구 · 26 §8 "접속한 적 없는 서버에 자동 트래픽 금지").

---

## 3. 저장 큐 `SaveQueue` — 틱 + 같은 액션 병합 + 대기 재설정 + 상한

```rust
// nsql-core/src/savequeue.rs  (의존 0 · 순수 · 테스트 가능)
pub enum SaveKey { Workspace(WsId), TabBackup(TabId), FileBackup(PathHash), Project(WsId), Settings }
pub struct Pending<P> { key: SaveKey, payload: P, first_at: Instant, due: Instant, deadline: Instant }
pub struct SaveQueue<P> { items: Vec<Pending<P>> }   // 열쇠당 최대 1개

impl<P> SaveQueue<P> {
    /// 같은 열쇠가 있으면 페이로드를 **바꾸고 `due`만 뒤로**(`now + delay`) · `deadline`(= first_at + max_wait)은 그대로.
    pub fn push(&mut self, key: SaveKey, payload: P, now: Instant, delay: Duration, max_wait: Duration);
    /// `now >= min(due, deadline)`인 것을 **키 순서대로** 꺼낸다(프로젝트/워크스페이스는 마지막에 — 본문 백업보다 뒤).
    pub fn take_due(&mut self, now: Instant) -> Vec<(SaveKey, P)>;
    /// 전부 꺼낸다(종료 · 프로젝트 닫기 · 전환).
    pub fn flush(&mut self) -> Vec<(SaveKey, P)>;
    /// 열쇠를 버린다(저장했다 · 탭 닫았다 = 백업 필요 없음).
    pub fn cancel(&mut self, key: &SaveKey);
    /// 다음 틱이 필요한 가장 이른 시각(호스트가 잠들 때 깨울 시점 · 없으면 None — 매 프레임 돌지 않는다).
    pub fn next_due(&self) -> Option<Instant>;
}
```

- ★ **"0.5초 간격으로 2시간 저장 버튼을 누르면 저장이 안 되나?"(사용자 09-22)** — 두 겹으로 막는다. ① 사용자의 **명시적 저장**(Ctrl+S · Save All · 메뉴)은 큐를 거치지 않고 **즉시** 원본에 쓴다(큐는 자동 백업 · 워크스페이스 · 프로젝트 파일처럼 *파생* 저장만) ② 파생 저장도 `deadline = first_at + max_wait`가 **재요청에 밀리지 않으므로** 아무리 자주 와도 `max_wait`(5초)에 한 번은 쓴다 — trailing debounce(유휴 1초)와 throttle(상한 5초)의 결합. leading-edge(첫 요청 즉시)는 100개 저장의 첫 1개에서 프로젝트를 쓰고 99개는 어차피 뒤라 이득이 없어 쓰지 않는다.
- **사용자 요구 둘을 정확히 이 부품이 담당**: ① 틱 단위로 필요한 저장만(`take_due`) ② 같은 액션은 마지막 요청만 + 대기 재설정(`push`가 `due`를 미룸). 여기에 ③ **상한 `deadline`**(계속 타이핑해도 `max_wait`마다 한 번은 쓴다 — VS Code·Sublime엔 없고 우리가 더한 것 · 데이터 유실 최소화) ④ **잠자기 친화**(`next_due`로 다음 깨움을 잡는다 · 39 §2 유휴 CPU 0).
- 페이로드는 **본문이 아니라 "무엇을 쓸지"**(탭 id · 세대 번호). 실제 본문은 수행 시점에 `buf()`에서 한 번만 뜬다(그 사이 100번 타이핑해도 스냅숏 1번).
- **"100개 저장 → 프로젝트 파일 100번"**: `Save All`은 탭마다 `Editors::mark_saved` → 워크스페이스 `push(Workspace)` 100번 → `due`만 100번 밀리고 `deadline`(2초)에 **1번** 쓴다. 본문 백업은 저장된 탭이므로 `cancel(TabBackup)` 100번(즉시 삭제는 백업 스레드에 삭제 요청 1개씩 — 삭제는 값싸다).
- MC/DC 테스트: (같은 열쇠 재요청 · 다른 열쇠 · delay 만료 · deadline 만료 · cancel 뒤 · flush) — 시각은 인자(`now`)라 실시간 없이 시험.

## 4. 백업 스레드 — 쓰기는 UI 밖 · 원자적 · 순서대로

- 스레드 **하나**(`backup-writer` · 39 §3 등재): 채널로 `Write{path, bytes}` / `Delete{path}` / `Prune{dir, days}` 를 받아 순서대로 처리 · `tmp` 쓰기 → `rename`(원자적 · 크래시해도 반쪽 파일 없음) · 실패는 로그 창 한 줄(반복 금지 · 같은 경로는 1회).
- UI 스레드가 하는 것: `take_due` → 탭마다 **세대 비교**(`EditState::rev` · 마지막 백업 세대와 같으면 건너뜀) → 바뀐 것만 `buf()`로 문자열(61 §1 예외 — 백업 시점 1회 · 큰 파일 모드 탭은 제외) → 채널 `send`. 2 MB 본문 문자열화 ≈ 1~2 ms(55 §6 실측 규모) · 100개 탭이 동시에 만료돼도 한 틱에 **최대 N개**(`project.backup_batch` 8)만 보내고 나머지는 다음 틱(프레임 예산 39 §2).
- 큰 버퍼(`project.backup_max_mb` 8 초과 · 큰 파일 모드 L1/L2): 백업하지 않고 탭 띠 "이 탭은 자동 백업되지 않습니다 — 저장하세요"(Sublime 4151 관례) · 종료·프로젝트 닫기 때 **묻는다**(§6).
- 메모리 회수: 문자열은 보내고 나면 스레드가 drop · `memtrim.rs`(59) 규약 그대로.

## 5. 참조 파일(디스크 파일)의 임시 저장 — 원본은 사용자 몫

| 사건 | 동작 |
|---|---|
| 파일 탭이 dirty로 바뀜(첫 변경) | `push(FileBackup)`(delay 1s · max 5s) |
| 계속 편집 | `due`만 밀린다 · 5초마다 한 번은 쓴다 |
| 백업 쓰기 | `backups/files/<경로 해시>.txt` = 프리앰블(원본 경로 · **열 때의 원본 해시·mtime** · 세대) + 본문 전체(변경분 diff가 아니라 전체 — 복원 단순·손상에 강함 · 8 MB 상한이라 감당) |
| 사용자가 저장 | `cancel` + `Delete` — **원본이 유일한 진실**로 돌아간다 |
| 되돌려서 clean · 탭 버림(저장 안 함 선택) | 위와 같음(즉시 삭제) |
| 앱 종료·크래시 | 백업이 남는다 → 다음에 그 파일을 열면(프로젝트 복원이든 File ▸ Open이든) **"저장하지 않은 변경이 있습니다 — 복원 / 버리기 / 비교"** 탭 띠(58 부품) · 복원 = 백업 본문을 올리고 dirty · 원본 해시 ≠ 지금 디스크 해시면 "그 사이 파일이 바뀌었습니다"를 덧붙임 |
| 유효기간 | `project.backup_days`(7) 넘은 백업은 시작 때 + 하루 1회 `Prune`(60 `undo_persist_days`와 같은 정리 통로) · 정리 전에 "복원 목록"에서 사용자가 볼 수 있다(§6) |
| 여러 판 | **최신 1개만**(JetBrains Local History식 다중 판은 D-179 — 되돌리기 기록 파일(60)이 이미 편집 단위 이력을 가진다 · 둘을 합치면 "판" 없이도 어느 시점으로든 돌아간다) |

메모리 탭(Script_n)은 저장할 원본이 없으므로 **백업이 곧 본문** — 같은 큐·같은 스레드 · `backups/<ws>/<tab id>.txt` · 탭 닫기(버림)면 삭제 · 워크스페이스에 탭 항목이 남아 있는 한 정리하지 않는다(유효기간 예외).

> ✅ 09-23 구현(journal §89~90): 프로젝트 파일 = 탭·캐럿 앵커·활성·북마크 · 파일 탭 미저장 본문 = `backups/files/<경로 해시>.txt`(자동 저장 때 dirty만 · 저장/버림 = 삭제 · 복원 = 디스크 해시가 같을 때만 · `project.backup_days`). 남음 = 저장 큐 스레드(§3~4 · 지금은 UI 스레드에서 30초/2초 디바운스 동기 쓰기) · `.bak` 손상 대비 · 복원 토스트 목록.

## 6. 복원 — 프로젝트를 열 때 · 시작 때

1. 워크스페이스 JSON 읽기(손상이면 `.bak` → 둘 다 실패면 빈 창 + 로그 · 18 §3). 탭을 **order 순서대로** 만든다(자리 탭 `begin_load_tab` → 순차 적재 스레드 = journal 09-22 §30 `multi-open`과 **같은 부품** — 100개 파일도 UI를 막지 않는다).
2. 파일 탭: **디스크를 다시 읽는다**(참조 = 최신) → `backups/files/<해시>`가 있으면 → 프리앰블의 원본 해시 vs 지금 디스크 해시 → 같으면 백업 본문을 올리고 dirty(조용히) · 다르면 백업 본문 + 탭 띠 "유지 / 디스크에서 다시 읽기 / 비교"(58) · 파일이 사라졌으면 백업 본문 + 띠 "원본 없음 — 다른 이름으로 저장".
3. 메모리 탭: 백업 본문 · 제목 · 구문 · dirty.
4. 캐럿·선택·스크롤·접힘: 본문 길이로 클램프해 복원(줄이 줄었으면 마지막 줄) · 활성 탭 · 동시 편집 집합은 복원하지 않는다(단일 · 단순).
5. 접속: 탭 표식만 · 상태줄 "복원된 탭 n개(미저장 m개)" 토스트(클릭 = 목록 팔레트 · 항목 = 탭으로) — 묻지 않는다(D-146 ①).
6. 프로젝트 닫기·전환·종료: 큐 `flush` → 백업 스레드가 다 쓸 때까지 **최대 `project.flush_wait_ms`(1500)** 기다린 뒤 종료(VS Code는 무한 대기 후 강제 · 우리는 상한 + 로그) · 큰 버퍼 탭만 저장을 묻는다(3택: 저장 / 버림 / 취소 · 34 규약).

## 7. 설정 · 부하원 · 단계

| 키 | 기본 | 뜻 |
|---|---|---|
| `project.hot_exit` | `always` | `always` / `only_on_quit` / `off`(off = 종료 때 저장 여부를 묻는다 · 백업 없음) |
| `project.backup_debounce_ms` | 1000(HIDDEN) | 변경 뒤 유휴 대기(`perf.boost` = 2000) |
| `project.backup_max_wait_ms` | 5000(HIDDEN) | 계속 편집해도 이 주기마다 한 번은 쓴다(`perf.boost` = 20000) |
| `project.workspace_debounce_ms` | 2000(HIDDEN) | 탭 열기/닫기/이동/캐럿 이동 뒤 워크스페이스 쓰기(캐럿은 **탭 전환·포커스 잃음·5초 유휴** 때만 — 매 키 아님) |
| `project.backup_max_mb` | 8 | 넘는 버퍼는 백업하지 않고 안내 |
| `project.backup_days` | 7 | 참조 파일 백업 유효기간(메모리 탭 백업은 워크스페이스에 남아 있는 한 예외) |
| `project.backup_batch` | 8(HIDDEN) | 한 틱에 스냅숏 뜨는 탭 수 상한 |
| `project.flush_wait_ms` | 1500(HIDDEN) | 종료 때 백업 스레드 완료 대기 상한 |

부하원 원장(39 §3): `backup-writer` 스레드 1(요청당 파일 1개 · 유휴 = 잠) · 스냅숏 문자열(탭당 본문 크기 · 8 MB 상한 · 배치 8) · 디스크 = `backups/`(정리 7일) · 틱 = `next_due`가 있을 때만.

| 단계 | 내용 | 부품 |
|---|---|---|
| **P3-a** | `nsql-core::savequeue`(§3) + MC/DC 테스트 · `backup-writer` 스레드(`backup.rs` · `*_in` 폴더 인자 · 원자적 쓰기 · Prune) | 의존 0 |
| **P3-b** | 워크스페이스 JSON(`workspace.rs` · 탭 항목 · 캐럿·스크롤 · `undofile::name_for` 재사용) · 쓰기 시점 배선(탭 사건 · 캐럿은 전환/유휴 때) | 67 · 61 |
| **P3-c** | 본문 백업 배선(dirty 전환 → push · 저장/clean/버림 → cancel+Delete · 큰 버퍼 안내 띠) · 참조 파일 백업 프리앰블 | 58 띠 · 59 |
| **P3-d** | 복원(§6 · 순차 적재 부품 재사용 · 띠 3택 · 복원 토스트 팔레트) · 종료 flush · `project.hot_exit` off 경로 | multi-open · 34 |
| **P3-e** | 실측(100 탭 Save All = 프로젝트 1회 쓰기 · 2 MB 탭 타이핑 중 프레임 영향 0 · `NSQL_TRACE_FRAMES`) · 캡처 · 문서 · 39 원장 | 45 벤치 규약 |

## 8. 결정(권장안 · 사용자 확인)

| # | 물음 | 권장 |
|---|---|---|
| **D-178** | 참조 파일 백업 판 수: ① 최신 1개 ② Local History식 N개(5일) | **①** — 되돌리기 기록 파일(60)이 이력을 이미 가진다 · 단순·작음. 요구가 생기면 ②는 `backups/files/<해시>/<시각>.txt`로 확장 가능 |
| **D-179** | 캐럿·스크롤을 워크스페이스에 쓰는 시점: ① 매 이동(디바운스) ② 탭 전환·포커스 잃음·5초 유휴·종료 | **②** — 타이핑 중 디스크 쓰기 0 · 복원 정확도는 사실상 같다 |
| **D-180** | 복원 때 백업 본문 vs 디스크가 다를 때: ① 묵시로 백업 올리고 띠 ② 먼저 묻기 | **①**(VS Code·Sublime · D-146과 일관) |
| **D-181** | `project.hot_exit` 기본: ① `always` ② `only_on_quit` | **①**(Sublime 기본 · 창 닫기 = 종료인 단일 창 앱) |
| **D-182** | 100개 `Save All` 진행 표시: ① 상태줄 카운트만 ② 실행 상태 카드 | **①** — 저장은 빠르다(원자적 쓰기 100개 < 1초) · 실패만 토스트 |
| **D-183** | 백업 폴더 위치: ① 설정 폴더 `backups/` ② 프로젝트 폴더 옆 `.nsql/` | **①**(VCS 오염 없음 · DR-27 사용자 폴더 규약) |

---

*작성 2026-09-22(93차 · win). 구현 = T-165 P3(a~e) · 저장 큐는 설정 저장(`persist_settings` 반복 호출)에도 같은 부품을 쓸 후보(30 §2 부품 원장 등재).*
