# 60 · 되돌리기/다시 실행 — 전체 점검 · 다른 편집기 조사 · 재설계(구현) · 남은 제안 (사용자 요청 09-19)

> **요구**: redo·undo 설계를 전체 점검하고, **저장 데이터를 최소화**하면서 **빠르게 저장/복원**할 수 있도록 다른 편집기 조사 결과와 함께 정리해 제안.
> **상태**: §1 점검 ✅ · §2 조사 ✅ · §3 재설계 **✅ 구현**(nexa-ui 46차 · journal 83차) · §5 남은 제안 **✅ 전부 구현**(D-129~132 = 권장안 · 사용자 09-20 "추천한 대로 · T-142 선행" · nexa-ui 47차 · journal 84차 · **§7**).
> 관련: [59 대용량 파일](59-large-file-handling.md) · [55 입력 지연](55-editor-input-latency.md) · [58 외부 변경](58-external-change-policy.md) · [15 외부 변경(병합)](15-external-file-changes.md).

## 0. 결론

1. 종전 설계(82차의 "차이 저장" 포함)는 **기록할 때마다 본문 전체를 훑었다** — 묶음을 열 때 직전 상태와의 공통 앞·뒤를 찾는 O(n) 비교 + 맨 위 상태의 전체 복사. 저장량은 줄었지만 시간은 줄지 않았다. 실측으로 **붙여넣기 100 KB = 34초 · 모두 바꾸기 2,000건 = 15초(되돌리기 2,000단계)** 가 나왔다.
2. 조사한 편집기 11종 중 가장 적게 저장하는 쪽(Emacs · CodeMirror 6)의 공통점은 **"버퍼에 없는 쪽 글자만 저장"** 이다 — 삽입은 위치·길이만(글자는 버퍼에 있다), 삭제는 지운 글자만. 양쪽을 다 저장하는 Scintilla·VS Code의 절반 이하다.
3. 그래서 **연산 기록(operation log)** 으로 다시 짰다: `적용 = pos에서 n글자를 지우고 s를 넣는다` — 적용하면 그 역연산이 나오므로 되돌리기와 다시 실행이 같은 함수다. 기록·복원 모두 **O(편집 크기)**, 저장은 **지운 글자만**, 더러움 판정은 **상태 id 비교 O(1)**, 메모리는 **바이트 예산**(`editor.undo_budget_mb` 64)으로 묶인다.
4. 결과(같은 벤치 `nexa-ctl examples/bench_undo`): 붙여넣기 100 KB **34,074 → 1.0 ms** · 모두 바꾸기 2,000건 **15,447 → 3.4 ms**(되돌리기 2,000단계 → **1단계**) · 단어 타이핑 **6.79 → 1.66 ms** · 타이핑만 한 히스토리 **320만 글자 → 0**.
5. 남은 것은 넷: 재시작 뒤에도 남는 히스토리 파일 · 거대 단일 연산의 디스크 흘리기 · 긴 정지 뒤 묶음 끊기 · 외부 재로드를 "최소 편집"으로 넣어 되돌리기 유지(§5).

## 1. 종전 설계 점검(무엇이 문제였나)

| # | 점검 항목 | 종전(≤ 82차) | 문제 |
|---|---|---|---|
| U1 | 저장 단위 | 묶음마다 상태 스냅샷(`Snap`) — 82차에 `SnapBuf::{Full, Delta}`로 맨 위만 전체·나머지는 공통 앞·뒤를 뺀 차이 | 저장량은 줄었으나 **차이를 구하려면 본문 전체 비교 O(n)** · 맨 위는 여전히 `Vec<char>` 전체(4 B/글자) |
| U2 | 기록 시점 비용 | 편집마다 `record` → 직전 스냅샷과 비교 | 3.6 MB 파일에서 단어마다 수 ms · 붙여넣기는 글자마다 `insert` → **O(n²)** |
| U3 | 여러 곳 편집(다중 캐럿 · 모두 바꾸기) | 캐럿마다/건마다 따로 기록 | 모두 바꾸기 2,000건 = **되돌리기 2,000번** · 차이 구간이 "첫 곳~끝 곳 전체"라 가장 비쌈 |
| U4 | 본문을 바꾸는 길 | `buf.splice`가 5곳에 흩어져 있음(`edit/ops.rs`) | 일부는 세대(`rev`)를 올리지 않아 **그리기 캐시가 낡은 본문을 그릴 수 있었다** · 읽기 전용·기록을 한 곳에서 걸 수 없음 |
| U5 | 더러움(저장본과 다른가) | 호스트가 저장본 `String`과 본문을 비교(세대별 캐시) | 큰 파일에서 저장본 사본 = 파일 크기만큼 메모리 · 비교 O(n) |
| U6 | 상한 | 개수(`editor.undo_max` 1,000)만 | 묶음 하나가 수십 MB일 수 있어 **개수로는 메모리를 못 묶는다** |
| U7 | 묶음 규칙 | 단어 경계(공백 뒤 첫 글자 = 새 묶음) · 캐럿 이동·종류 전환에서 끊음 | 규칙 자체는 적절(Sublime·VS Code와 같은 계열) — 유지 |
| U8 | 영속 | 없음(세션 안에서만) | VS Code·IntelliJ와 같은 수준 — §5-1 |

## 2. 다른 편집기(09-19 조사 · 원문 확인 범위는 §6)

| 편집기 | 저장 모델 | 묶는 규칙 | 상한 | 영속 | 저장 지점(더러움) |
|---|---|---|---|---|---|
| **Scintilla / Notepad++** | 글자별 연산 열(종류·위치·길이) + 글자 더미 — **삽입·삭제 글자 둘 다** · 원소 폭을 최댓값에 맞춰 1~8 B로(`ScaledVector` — 64비트에서 흔히 75% 절감) | 앞 연산의 끝에 이어지는 삽입 · 1~2자 Backspace/Delete · 저장 지점에서 끊음 · IME는 잠정 연산 | 없음 | API는 있으나 문서가 "unfinished"(같은 파일이어야 함) | 정수 인덱스 · 지점 아래로 되돌린 뒤 편집하면 −1 |
| **VS Code** | `TextChange`(옛 글 + 새 글 **둘 다**) + 앞뒤 선택 · 닫을 때 `ArrayBuffer`로 직렬화(글자당 2 B + 원소당 168 B) | 타이핑↔비타이핑 · 공백↔비공백 전환에서 끊음(시간 기준 없음) | 없음(닫힌 파일 스택 20 MB) | 세션 안(다시 열 때 SHA1 일치) · 재시작 뒤는 없음(#15135) | `alternativeVersionId` 비교 O(1) |
| **Vim** | 바뀌기 전 **줄 전체** · 트리 | 명령 또는 입력 세션 | 1,000(`undolevels`) | `undofile`(본문 SHA-256 · 줄 수 · 소유자 검사) | `save_nr` |
| **Emacs** | 삽입 = **범위만**(글자 저장 0 · 연속 삽입은 범위를 늘림) · 삭제 = 글자 | 자기 삽입 20타마다 경계 | **바이트 3단**: 160 K / 240 K / 24 M(넘는 단일 명령은 경고 후 버림) | 없음 | `(t . time)` |
| **IntelliJ** | 옛 글 + 새 글(압축) · 타임스탬프 | `groupId` | 문서 100 · 전역 10 | 없음(Local History는 별도) | — |
| **Helix** | `ChangeSet`(Retain/Delete/Insert) — Delete가 글자를 안 담아 **정방향 + 역방향 둘 다** 저장 · 트리 | 명령·입력 세션 | 없음("currently unbounded") | 없음 | — |
| **CodeMirror 6** | **역 ChangeSet만**(지운 글자만) + 시작 선택 | 500 ms + 인접 + 입력 종류 | ≈ 100 이벤트 | JSON | 앱이 구현 |
| **Zed** | CRDT — 글자를 지우지 않고 묘비 · `UndoMap`(홀수 = 되돌려짐) | 300 ms | 없음 | — | — |
| **Xi** | CRDT 되돌리기 그룹 | 편집 종류 | 20 그룹 + gc | — | — |
| **fredbuf** | 불변 조각 트리의 루트를 편집마다 보관 — 되돌리기 O(log n) · 지운 글자 복사 0 | — | 덧붙임 버퍼가 계속 자람 | — | — |

읽어 낸 것:

- **가장 적게 저장** = Emacs · CM6(버퍼에 없는 쪽만). 타이핑 1자의 저장 글자 = 0 B. Scintilla ≈ 4~6 B · VS Code 2 B + 고정 60~230 B · Vim = 줄 길이.
- **가장 빠른 복원** = 연산 기록 계열 전부 O(편집 크기). 스냅샷·차이 계열(종전 Nexa)만 O(본문).
- **메모리를 실제로 묶는 곳은 Emacs뿐**(바이트 3단) — 나머지는 개수 상한이거나 무제한. 거대 단일 연산은 Emacs(버림) · Vim(`ul=-1` 권고) · IntelliJ(확인 정책)가 다룬다.
- **더러움은 모두 O(1)**(인덱스·버전 id) — 본문 비교를 하는 곳은 없다.
- **선형이 다수**(Scintilla · VS Code · CM6 · IntelliJ). 트리(Vim · Helix · Kakoune)는 다시 실행 가지가 예산을 먹고 UI 비용이 크다.

## 3. 재설계(✅ 구현 — nexa-ctl `edit.rs` · `edit/ops.rs` · `controls/textbox.rs`)

### 3-1. 기록 = 연산, 저장 = 지운 글자만

```
Op  { pos, remove_n, insert: String, insert_n }     // 적용 = pos에서 remove_n 글자를 지우고 insert를 넣는다
Txn { id, ops, caret, anchor, extra, bytes }        // 되돌리기 한 단계 + 돌아갈 캐럿·선택(다중 캐럿 포함)
```

- **적용하면 역연산이 나온다**(지운 글자가 역연산의 `insert`) → 되돌리기 묶음을 적용한 결과가 곧 다시 실행 묶음이다. 함수 하나(`apply_txn`)로 양방향.
- 타이핑은 "k글자 지우기"로 기록된다 — 글자는 버퍼에 있으므로 **저장 글자 0**. 삭제만 그 글자를 든다.
- 묶음 안의 연산은 **뒤에서 앞으로** 적용한다(각 `pos` = 자기보다 뒤의 연산이 끝난 좌표). 위치가 오름차순·비겹침이면(다중 캐럿 · 모두 바꾸기) 본문을 **한 번만 훑어** 재배치한다 — `Vec<char>`에 splice를 N번 하면 O(N·n)이다.

### 3-2. 본문을 바꾸는 길은 하나

모든 편집이 `splice_rec`(한 곳) / `replace_many_inner`(여러 곳)를 지난다. 흩어져 있던 `buf.splice` 5곳을 이 길로 옮겼다 → ① 세대가 빠짐없이 오른다(U4의 낡은 캐시 결함 해소) ② 기록·**읽기 전용**·예산 회계를 한 곳에서 건다 ③ 버퍼 표현을 바꿀 때(T-142) 고칠 곳이 두 함수다.

### 3-3. 묶음(코얼레싱)

- 인접 규칙(Scintilla) + 단어 경계(종전 · Sublime): 앞 연산의 끝에 이어지는 타이핑은 길이만 늘린다 · Backspace/Delete 연속은 지운 글자를 앞/뒤에 이어 붙인다 · 공백 뒤 첫 글자·캐럿 이동·종류 전환·저장에서 끊는다.
- **묶음 API** `begin_group`/`end_group`(중첩 가능) — 호스트가 "여러 편집 = 한 단계"를 선언한다. `replace_many`는 그 자체로 한 단계(모두 바꾸기 · 3-way 병합 반영).
- 붙여넣기 = splice 한 번(글자마다 `insert`하던 O(n²) 제거).

### 3-4. 저장 지점 = 상태 id

`serial`(새 상태마다 +1) · 묶음마다 `id` · `base_id`(히스토리 바닥) · `saved_id`. 기록 = 새 id · 되돌리기 = 앞 묶음의 id(없으면 `base_id`) · 다시 실행 = 그 묶음의 id. **`is_saved() = 현재 id == saved_id`** — 본문 비교 없음. 호스트(`editors.rs`)는 이것으로 더러움을 먼저 판정하고, 큰 파일 탭은 저장본 사본을 아예 들지 않는다([59 §5](59-large-file-handling.md)). 저장하면 열린 묶음을 닫는다(코얼레싱이 id를 안 바꾼 채 내용을 바꾸지 못하게 — Scintilla와 같은 규칙).

### 3-5. 바이트 예산

묶음의 바이트 = 글자 + 고정비(연산 48 B · 묶음 96 B — 글자 0인 연산 수만 개도 예산에 잡히게). 되돌리기 + 다시 실행 합이 예산을 넘으면 **가장 오래된 묶음부터** 버린다(최신 1개는 남김 · 바닥 id 갱신). 저장 지점이 버려져도 따로 할 일은 없다 — 그 id에 다시 닿을 수 없을 뿐이고 더러움은 유지된다. 개수 상한(`editor.undo_max`)은 보조로 남는다. 설정 `editor.undo_budget_mb`(64 · 0 = 무제한).

### 3-6. 실측(`cargo run --release -p nexa-ctl --example bench_undo`)

| 시나리오(본문 3.2 MB) | 종전 | 재설계 |
|---|---|---|
| 붙여넣기 100 KB | 34,074 ms | **1.0 ms** |
| 모두 바꾸기 2,000건 | 15,447 ms · 되돌리기 2,000단계 | **3.4 ms · 1단계** |
| 단어 타이핑(단어당) | 6.79 ms | **1.66 ms** |
| 타이핑만 한 뒤 히스토리가 쥔 글자 | 3,200,000 | **0** |

### 3-7. 검증

- **속성 테스트** `random_edits_match_snapshot_model`: 자체 난수(40 시드 × 120 연산 — 삽입·삭제·선택 교체·다중 캐럿·모두 바꾸기·되돌리기·다시 실행)를 "상태 전체를 복사해 두는 단순 모델"과 대조. 끝에서 전부 되돌리면 원본 · 전부 다시 실행하면 최종본.
- `history_budget_evicts_oldest_and_keeps_accounting`(회계 = Σ묶음 · 최신 1개 보존) · `groups_and_typing_storage`(타이핑 저장 0 · 묶음 = 1단계) · `read_only_blocks_every_mutation_path`(모든 편집 길 차단).

## 4. 호스트 쪽 변화(nexa-sql)

- 찾아 바꾸기 "모두" = `replace_many` 한 번(한 단계 · 한 번 훑기).
- `is_dirty` = 저장 지점 먼저(O(1)) → 다르면 종전 비교(되돌려서 저장본과 같아진 경우를 위해 · 큰 파일 탭은 비교 없이 더러움).
- 외부 변경 반영([58](58-external-change-policy.md))은 되돌리기 한 단계로 남는다(`replace_all_undoable` — 본문 == 기준이면 저장 지점도 찍는다).
- 설정 `editor.undo_budget_mb` → 전 탭 + 새 탭.

## 5. 남은 제안(✅ 09-20 전부 권장안으로 확정·구현 → §7)

| # | 제안 | 근거(§2) | 비용·위험 | 권장 |
|---|---|---|---|---|
| **D-129** 재시작 뒤에도 남는 히스토리 | hot exit 때 탭별 파일(`NSQL_HOME/undo/<경로 해시>` · 헤더 = 매직·버전·버퍼 길이·**본문 해시**·`saved_id`·`cur_id` · 묶음마다 길이+CRC · tmp+rename). 읽을 때 해시·길이가 다르면 **전부 버림**(부분 복구 없음) | Vim `undofile` · VS Code 닫힌 파일 스택(SHA1) · Scintilla(같은 파일 요구) | 디스크 부하원 1(39 §3 등재 · 상한 = 파일당 N MB 최신분) · 버퍼 표현이 바뀌면 버전으로 폐기 | **T-142(버퍼 교체) 뒤에** — 좌표 단위가 글자 → 바이트로 바뀌면 형식도 바뀐다. 지금 만들면 두 번 만든다 |
| **D-130** 거대 단일 연산 | 묶음 하나가 `editor.undo_spill_mb`(8)를 넘으면 지운 글자를 임시 파일에 두고 메모리엔 오프셋만 · 실패하거나 바깥 상한을 넘으면 실행 전에 "되돌릴 수 없습니다 — 계속?" | Emacs `undo-outer-limit`(경고 후 버림) · IntelliJ 확인 정책 · Vim `ul=-1` 권고 | 지금은 예산이 "최신 1개는 보존"이라 **100 MB 전체 삭제 한 번이면 예산을 넘겨 든다** — 큰 파일 모드 L2에서만 현실적 위험 | **L2에서만 확인 창**(디스크 흘리기 없이) 먼저 · 흘리기는 실사용에서 필요해지면 |
| **D-131** 시간으로 끊기 | 긴 정지(1~2초) 뒤의 타이핑은 새 묶음(`editor.undo_group_ms` HIDDEN) | CM6 500 ms · Zed 300 ms — 단독 기준으로 쓰면 단어 규칙과 부딪침 | 되돌리기 단위가 "사용자가 생각한 단위"에 가까워짐 · 위험 낮음 | **채택 권장 · 보조 조건으로만**(1,500 ms) |
| **D-132** 재로드 = 최소 편집 | 외부 변경을 본문 전체 교체가 아니라 줄 단위 최소 편집 목록으로 넣어 한 묶음에 기록(캐럿은 편집으로 옮김) | VS Code `_computeEdits`(공통 앞·뒤 줄) | 지금도 한 단계로 되돌릴 수 있으나 **묶음이 본문 전체를 든다**(큰 파일 = 예산을 한 번에 소모) · `merge3`의 Myers 결과를 그대로 쓸 수 있음 | **채택 권장**(T-140 후속 · 작음) |
| (보류) 선형 → 트리 | `before/after id`가 이미 부모 링크 구실을 해 나중에 확장 가능 | Vim · Helix · Kakoune | 다시 실행 가지가 예산을 먹음 · 보여 줄 UI가 필요 | **보류** — 요구가 생기면 |

## 7. 남은 제안의 구현(09-20 · journal 84차 · [59 §6](59-large-file-handling.md) 버퍼 교체 뒤)

| # | 구현 | 어디 | 설정 |
|---|---|---|---|
| **D-131** 쉬었다 치면 새 묶음 | 이어 붙이던 타이핑·삭제라도 직전 편집에서 이만큼 쉬었으면 새 단계 — 단어 경계 규칙의 **보조**(단독 기준 아님) | nexa-ctl `EditState::set_group_pause_ms` · `record` | `editor.undo_group_ms` 1500(0 = 끔 · HIDDEN) |
| **D-132** 재로드 = 최소 줄 편집 | 외부 변경 반영이 "첫 차이~끝 차이 한 덩이"가 아니라 **바뀐 줄들만** — nexa-ctl `merge3::line_edits`(Myers 정합 재사용 · 끝 개행 유무·끝에 덧붙임/지움 처리 · 14×14 조합 테스트) → `replace_many` 한 단계. 위아래 한 줄씩 + 가운데 1,000줄 = 기록 100 B 미만(종전 = 1,000줄 전부). 정합을 포기할 만큼 다르면 한 덩이 교체로 물러난다 · 캐럿은 편집들 너머로 옮긴다 | `TextBox::replace_all_undoable` | — |
| **D-130** 거대 편집 확인 | 한 번에 기준 이상을 **지우는** 편집은 처음엔 막히고(본문·선택 그대로) 상태줄 + 토스트로 알린다 · **지우는 동작**(Delete · Backspace · 잘라내기 · 편집 명령 · 모두 바꾸기)을 3초 안에 되풀이하면 진행하고 **그 탭의 히스토리를 비운다**(그 기록 = 지운 글 전체를 들지 않는다 — Emacs `undo-outer-limit`). **타이핑·붙여넣기·조합으로 덮어쓰는 것은 몇 번을 해도 확인이 되지 않는다** — 빠르게 두 글자를 친 것이 65 MB 삭제의 확인이 되면 안 된다("먼저 지우세요"로 안내). 권장안은 "L2에서만"이었으나 기준을 **바이트**로 두어 어느 탭에나 건다(32 MB를 지우려면 어차피 큰 파일이다) | nexa-ctl `EditState::giant_refused`(진입점마다) · 호스트 `giant_notice` | `editor.undo_giant_mb` 32(0 = 끔 · HIDDEN) |
| **D-129** 재시작 뒤에도 남는 기록 | 이 앱에는 미저장 탭 복원(hot exit)이 없다 → **Vim `undofile` 방식**: 파일을 **저장할 때** 기록을 `<설정 폴더>/undo/<경로 해시>.nsqu`에 쓰고, 그 파일을 **다시 열 때** 본문의 길이·해시가 기록의 것과 같으면 들인다(들인 직후 = "저장됨"). 하나라도 어긋나면(밖에서 고침 · 잘린 파일 · 한 바이트 뒤집힘 · 판 다름) **통째로 버린다**. 형식 = `NSQU` · 판 · 본문 글자 수·바이트 수·해시 · 상태 id들 · 단계들(연산 = 위치·지울 수·넣을 글) · 꼬리 해시. 상한을 넘으면 오래된 단계부터 뺀다. 큰 파일 모드 탭 · 읽기 전용 · "앞부분만"은 쓰지 않는다 | nexa-ctl `export_history`/`import_history` · `TextBuf::content_hash` · nexa-sql `undofile.rs` · `undo_persist_save`/`_load` | `editor.undo_persist` on · `editor.undo_persist_mb` 4 · `editor.undo_persist_days` 30(둘은 HIDDEN) |

**알아 둘 것(D-129)**: 기록에는 **지운 글자가 평문으로** 들어 있다(타이핑한 글자는 없다). 사용자 설정 폴더 안에만 두고 30일 뒤 지우며, 끄면(`editor.undo_persist` off) 쓰지도 읽지도 않는다. 주류 편집기의 같은 부류(VS Code hot exit · Sublime hot_exit · JetBrains Local History)도 기본이 켬이라 기본값을 켬으로 두었다 — 민감한 환경이면 끄는 것을 권한다.

검증: 단위 테스트(`long_pause_starts_a_new_undo_group` · `line_edits_rebuild_new_text` · `reload_records_only_changed_lines` · `giant_edit_needs_a_repeated_delete` · `history_file_round_trip_and_rejections` · `content_hash_ignores_gap_position` · `undofile::store_load_remove_round_trip`) + 실기(격리 폴더 · 기동 명령): ① 줄 복제·주석 → 저장 → **앱 종료 → 새 프로세스로 열기 → 되돌리기 2번 = 원문**(기록 파일 200 B) ② 65 MB 전체 선택 → 줄 삭제 = 막힘 + 안내 · 3초 안에 한 번 더 = 진행.


## 6. 조사 범위와 한계

- **원문 확인**: Scintilla 소스·문서 · Vim `options.txt`/`undo.txt` · xi-editor · IntelliJ 상수는 직접 읽음. VS Code · Emacs `undo.c` · Helix · CM6 · ProseMirror · Zed · Kakoune · Vim `undo.c` · fredbuf 블로그는 요약 도구로 확인.
- **미검증**: Sublime(비공개 — `soft_undo` 한 줄만 확인 · `mark_undo_groups_for_gluing`은 출처 못 찾음) · Lapce(경로 404) · Word/AbiWord 조각 테이블은 조사하지 못함.

## 출처

Scintilla [`UndoHistory.cxx`](https://raw.githubusercontent.com/notepad-plus-plus/notepad-plus-plus/master/scintilla/src/UndoHistory.cxx) · [`ScintillaDoc`](https://raw.githubusercontent.com/notepad-plus-plus/notepad-plus-plus/master/scintilla/doc/ScintillaDoc.html) · VS Code [`editStack.ts`](https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/editor/common/model/editStack.ts) · [`textChange.ts`](https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/editor/common/core/textChange.ts) · [`undoRedoService.ts`](https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/undoRedo/common/undoRedoService.ts) · [`modelService.ts`](https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/editor/common/services/modelService.ts) · [#15135](https://github.com/Microsoft/vscode/issues/15135) · Vim [`undo.c`](https://raw.githubusercontent.com/vim/vim/master/src/undo.c) · [`undo.txt`](https://vimhelp.org/undo.txt.html) · Emacs [Undo 매뉴얼](https://www.gnu.org/software/emacs/manual/html_node/elisp/Undo.html) · [`undo.c`](https://raw.githubusercontent.com/emacs-mirror/emacs/master/src/undo.c) · IntelliJ [`registry.properties`](https://raw.githubusercontent.com/JetBrains/intellij-community/master/platform/util/resources/misc/registry.properties) · Helix [`history.rs`](https://raw.githubusercontent.com/helix-editor/helix/master/helix-core/src/history.rs) · CodeMirror [`history.ts`](https://raw.githubusercontent.com/codemirror/commands/main/src/history.ts) · ProseMirror [`history.ts`](https://raw.githubusercontent.com/ProseMirror/prosemirror-history/master/src/history.ts) · Zed [`text.rs`](https://raw.githubusercontent.com/zed-industries/zed/main/crates/text/src/text.rs) · [CRDT 블로그](https://zed.dev/blog/crdts) · Xi [CRDT 문서](https://xi-editor.io/docs/crdt-details.html) · Kakoune [`buffer.hh`](https://raw.githubusercontent.com/mawww/kakoune/master/src/buffer.hh) · [fredbuf](https://cdacamar.github.io/data%20structures/algorithms/benchmarking/text%20editors/c++/editor-data-structures/)
