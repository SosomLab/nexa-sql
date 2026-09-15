# 37 · 파일 선택기·트리 성능 점검 — 실측과 수정 계획

> **요청**(사용자 09-15): *"파일 탐색기 기능에 nexa-dir2 기능을 차용해서 추가했는데 기능의 복잡도, 메모리 사용량, 속도 등에 대해서 점검해줘."*
> **기준 커밋**: `nexa-ui` c4d79ea(백그라운드 열거) · `nexa-sql` 71ebd60. **조사 세션은 소스를 고치지 않았다** — 수정은 작업 세션이 한다.
> **선행**: [nexa-ui 20 파일 대화상자](../../nexa-ui/docs/20-file-management-and-dialogs.md) · [nexa-ui 21 그리드 계열](../../nexa-ui/docs/21-grid-family.md) · [26 §8 성능](26-performance-architecture.md) · [30 아키텍처 패턴](30-architecture-patterns.md).
> **측정 환경**: Windows 11 · `rustc -O` 독립 벤치(§5 재현 절차) · OS 캐시 hot · 순서 편향 제거(양방향 평균).

---

## 0. 세 줄

1. **c4d79ea가 UI 블로킹 문제는 껐다** — `nexa-fs::lister`(요청당 스레드 · 256 배치 · Drop 취소)와 `probes` 캐시로 폴더 진입이 UI를 막지 않는다.
2. **남은 병목은 UI 쪽 한 곳에 몰려 있다** — `TreeModel::flatten()`에 캐시가 없어 **행 목록을 프레임마다 통째로 복사**한다. 4,899행에서 `rows()` 1회 = **4.33 ms**, 프레임당 2~3회 = **60fps 예산의 51%**.
3. **고칠 것은 3건이고 전부 국소 변경이다**(P-1·P-2·P-3). 이 셋이면 실사용 범위에서 끝난다. 구조 문제(P-5)는 별건으로 남긴다.

---

## 1. 이미 해결된 것 (c4d79ea · 현재 코드로 확인함)

| 조사에서 지적 | 현재 | 확인 위치 |
|---|---|---|
| 폴더 진입이 UI 스레드를 막음(System32 **663 ms**) | ✅ `nexa-fs::lister` — 요청당 스레드 · 256 배치 스트리밍 · `ListHandle` Drop = 취소 · `try_recv`만 | `nexa-fs/src/lister.rs` |
| `make_row`가 행마다 `has_visible_child()`(하위 폴더마다 `read_dir`) | ✅ 워커의 `ListMsg::Probe` → `self.probes` 조회로 대체 | `nexa-dlg/src/lib.rs:1166-1170` |
| `make_row`가 행마다 `exts.clone()` | ✅ 제거됨 | `nexa-dlg/src/lib.rs:1125-1170` |
| 사이드바가 전체 나열 후 `.filter(is_dir)`로 버림 | ✅ `ListOpts.dirs_only` | `nexa-fs/src/lister.rs` |
| 아이콘 조회가 UI를 막을 위험 | ✅ 원래부터 정상(`SHGFI_USEFILEATTRIBUTES` = 디스크 안 건드림 · 워커 · `Lookup::Pending` 폴백) + 유휴 30초 종료 추가 | `nexa-fs/src/shell.rs` |

**이 커밋의 접근은 옳다.** 아래 남은 항목은 그 위에 얹는 것이지 되돌리는 것이 아니다.

---

## 2. 남은 문제

### P-1 🔴 P0 · `rows()`에 캐시가 없다 — 가장 큰 병목

**위치**: `nexa-ui/crates/nexa-ctl/src/controls/tree.rs:223-225`

```rust
fn rows(&self) -> Vec<FlatRow> {
    self.model().flatten()      // 호출마다 전체 트리를 깊은 복사
}
```

`flatten()`은 행마다 `path.clone()`(Vec) + `label.clone()`(String) + `cells.clone()`(String 6개)을 만든다 — **행당 힙 할당 9회**. 캐시가 없으니 호출할 때마다 전부 다시 만든다.

**측정 — `rows()` 1회 비용**

| 행 수 | 시간 | 할당 횟수 | 60fps 예산 |
|---:|---:|---:|---:|
| 65 (홈) | 30.7 µs | 585 | 0.2% |
| 1,601 (`target/debug/deps`) | 1.17 ms | 14,409 | 7.0% |
| **4,899 (System32)** | **4.33 ms** | 44,091 | **25.9%** |
| 20,000 | 15.7 ms | 180,000 | **94.0%** |

**프레임당 1회가 아니다.** `tree.rs` 안에서만 8곳이 부른다 — `paint()`(738) · `content_size()`(686, 스크롤바가 페인트 중 호출) · `row_hit()`(270, **MouseMove마다**) · `move_selection()`(229) · 선택 라벨(428) · TreeView `paint()`(498) · 행 조회(240·250). `nexa-dlg`가 9곳 더 쓴다.

> **4,899행 × 3회 = 8.58 ms/프레임** — 예산의 51%를 행 목록 복사에만 쓴다.

**메모리 — counting allocator 실측**

| 행 수 | `TreeNode` 상주 | `flatten()` 사본(**매 프레임 생성·소멸**) |
|---:|---:|---:|
| 65 | 26.8 KB | 29.5 KB |
| 1,601 | 658 KB | 622 KB (14,420 할당) |
| **4,899** | **2.01 MB** | **2.03 MB (44,104 할당)** |

행당 421 B 상주는 정상이다. 문제는 **같은 크기를 매 프레임 버리고 다시 만드는 것** — 4,899행 60fps면 **초당 122 MB · 265만 할당**.

> ⚠️ **오해 금지 — 이것은 메모리 사용량 문제가 아니라 CPU 문제다.** §6 실측에서 System32 표시 중 프로세스 사설 메모리는 11.96 MB로 **미동도 없었다.** 사본은 같은 프레임 안에서 할당·해제되므로 할당자가 같은 영역을 재사용한다. P-1을 고쳐야 하는 이유는 **프레임 예산의 51%를 먹기 때문**이지 메모리가 늘어서가 아니다.

**수정안**

`TreeModel`에 평탄화 결과를 캐시하고 변경 시 무효화한다.

```rust
// TreeModel
flat: std::cell::RefCell<Option<std::rc::Rc<Vec<FlatRow>>>>,

pub fn flatten(&self) -> std::rc::Rc<Vec<FlatRow>> {
    if let Some(f) = self.flat.borrow().as_ref() { return f.clone(); }
    let mut out = Vec::new();
    Self::walk(&self.roots, &mut Vec::new(), 0, &mut out);
    let rc = std::rc::Rc::new(out);
    *self.flat.borrow_mut() = Some(rc.clone());
    rc
}
fn invalidate(&mut self) { *self.flat.borrow_mut() = None; }
```

`rows()`는 `Rc<Vec<FlatRow>>`를 돌려준다(호출부는 `.iter()`/`.get()` 그대로 쓴다).

⚠️ **무효화 지점을 빠뜨리면 조용한 표시 버그가 된다.** 최소한 이 경로 전부:

- `TreeModel::toggle()` (tree.rs:147)
- `roots`를 직접 바꾸는 모든 곳 — `nexa-dlg`의 `lazy_load_places()`가 `model_mut().roots`를 직접 수정한다
- `TreeModel::new()` / 노드 교체

> 안전하게 하려면 `roots`를 비공개로 돌리고 변경을 메서드로만 열어 무효화를 강제하는 편이 낫다. 지금은 `pub roots`라 외부에서 조용히 바꿀 수 있다.

---

### P-2 🔴 P0 · 페인트가 전체 행을 순회한다

**위치**: `tree.rs:738-742`(TreeGrid) · `tree.rs:498-502`(TreeView)

```rust
for (i, row) in self.rows().iter().enumerate() {
    let y = top - self.scroll_y + rh * i as i32;
    if y < top || y + rh > bottom { continue; }   // ← 화면 밖을 '건너뛰기'만 한다
```

첫 가시 행을 계산하지 않고 **전체를 돈다.** 20,000행이면 화면에 40행이 보여도 20,000번 순회한다.

**수정안**

```rust
let rows = self.rows();
let first = ((self.scroll_y / rh).max(0) as usize).min(rows.len());
let count = ((bottom - top) / rh) as usize + 2;
for (i, row) in rows.iter().enumerate().skip(first).take(count) { /* ... */ }
```

★ **같은 패턴이 nexa-sql에도 있다** — [explorer.rs:987](../crates/nexa-sql/src/explorer.rs#L987) `screen_rows()`가 페인트마다 펼쳐진 전 노드의 `Vec`을 새로 만들고, [explorer.rs:998](../crates/nexa-sql/src/explorer.rs#L998) `row_at()`이 MouseMove마다 또 만든다. **부품 공통 결함이므로 같이 고친다.**

---

### P-3 🟡 P1 · `fs::metadata(path)` — 25~53배 손해

**위치**: `nexa-ui/crates/nexa-fs/src/lib.rs:143`(`entry_of`) · `:187`(`has_visible_child`) · `:229`(`has_subfolder`)

```rust
let meta = fs::metadata(&path).or_else(|_| de.metadata());
```

Windows에서 `DirEntry::metadata()`는 `FindNextFile`이 이미 돌려준 데이터를 읽어 **syscall이 없다**. `fs::metadata(&path)`는 경로를 다시 해석하고 파일을 연다.

**측정 (OS 캐시 hot · 양방향 평균)**

| 폴더 | 항목 | `fs::metadata(path)` | `DirEntry::metadata()` | 배수 |
|---|---:|---:|---:|---:|
| 홈 | 65 | 1.54 ms | 386 µs | 4.0× |
| `C:\Windows` | 127 | 2.99 ms | 104 µs | **28.6×** |
| `target/debug/deps` | 1,601 | 31.9 ms | 651 µs | **49.1×** |
| **System32** | 4,899 | **122 ms** | **2.29 ms** | **53.3×** |

주석대로 **의도된 선택**이다 — *"링크는 대상 기준(폴더 링크는 폴더로 진입 가능해야 한다)"*. 정확성은 맞지만 대가가 53배다.

**수정안** — 링크일 때만 경로 stat. 동작은 같고 비용만 빠진다.

```rust
let meta = match de.file_type() {
    Ok(t) if t.is_symlink() => fs::metadata(&path).or_else(|_| de.metadata()),
    _ => de.metadata(),
};
```

> 지금은 워커 스레드라 **UI를 막지는 않는다.** 하지만 첫 화면 도착 지연과 CPU에 그대로 반영되고, 노트북에서는 배터리에도 반영된다. `entry_of`는 동기·비동기 경로가 공유하므로 한 번만 고치면 둘 다 낫는다.

---

### P-4 🟡 P2 · 배치마다 전체를 다시 만든다 (lister 도입의 부작용)

**위치**: `nexa-dlg/src/lib.rs:881-910`(`poll_loaders`) → `:1229`(`refresh_grid`)

```rust
Some(ListMsg::Batch(v)) => { self.entries.extend(v); got_batch = true; }
// ...
if got_batch || finished.is_some() { self.refresh_grid(sel); }   // ← 누적 전체를 재구성
```

`refresh_grid`는 매번 **누적분 전체**를 재정렬(`sort_by`) → 전량 깊은 복사(`.cloned()`, 1236행) → 전량 `make_row` → **`TreeGrid`를 통째로 새로 생성**한다.

4,899개 = 20배치, `MAX_MSGS = 8`이니 약 3회 재구성 — 누적 **약 11,000행 분량**을 만들고 버린다(필요량의 2.2배). P-1과 겹쳐 로딩 중 프레임이 가장 무겁다.

**수정안**(택1, 위가 간단)

- 배치 도착 시에는 `entries`에 **붙이기만** 하고, 재구성은 `Done`에서 1회 + 로딩 중에는 **일정 간격**(예: 150 ms)으로만.
- 또는 `refresh_grid`를 증분화 — 새 배치분만 `make_row` 해 `nodes`에 append(정렬은 `Done` 뒤 1회).

같이 처리할 것: `refresh_grid`의 `.cloned()`(1236행) — `top`을 `Vec<Entry>` 복사 대신 `Vec<&Entry>` 또는 인덱스로.

---

### P-5 🟡 P2 · 설계한 계층이 없다 (구조 · 별건)

[nexa-ui 20 §1](../../nexa-ui/docs/20-file-management-and-dialogs.md)이 설계한 중간 계층이 만들어지지 않았다.

| 계층 | 설계 | 현재 |
|---|---|---|
| `nexa-ctl/controls/file/` | `PathBar` · `PlacesList` · `FileList` · `FileTree` · `NameBox` · `FilterCombo` | **디렉터리 없음** |
| `nexa-dlg` | `Dialog` 프레임 · `FilePicker` · `MessageBox` · `Prompt` · `Progress` | `lib.rs` 1개 · **2,174줄** · `FilePicker` **47필드 82함수** |
| `nexa-fs` | `places`·`listing`·`sort`·`ops`·`watch`·`kind`·`path` 7모듈 | `lib.rs` 938 + `shell.rs` 732 + `lister.rs` 284 |

실질적 영향은 **재사용 불가**다. dir2·clip·beep이나 nexa-sql 오브젝트 탐색기가 `FileList`만 가져다 쓸 수 없다 — 20 §2가 *"nexa-dir2가 `nexa-fs` + `FileList`의 최대 소비자가 된다"*고 한 목표가 막힌다. 20 §0 원칙 3(*"새 기능은 아래층을 고치지 않고 위층에서 조합"*)도 지켜지지 않았다.

**지금 당장 성능 문제는 아니다.** P-1~P-4를 먼저 하고, 추출은 기능이 안정된 뒤 별도 작업으로 잡는다.

---

## 3. 수정 순서와 기대 효과

| 순위 | 항목 | 저장소 · 파일 | 효과 | 규모 |
|:--:|---|---|---|:--:|
| **1** | **P-1** `rows()` 캐시 | `nexa-ui` `nexa-ctl/src/controls/tree.rs:223` | 4,899행 **8.58 ms → ~0** · 초당 122 MB 할당 제거 | 소 |
| **2** | **P-2** 첫 가시 행부터 순회 | `nexa-ui` `tree.rs:498,738` + `nexa-sql` `explorer.rs:987` | 20,000행에서 선형 비용 제거 | 소 |
| **3** | **P-3** 링크일 때만 경로 stat | `nexa-ui` `nexa-fs/src/lib.rs:143,187,229` | System32 **122 ms → 2.3 ms (53×)** | 소 |
| 4 | **P-4** 배치 재구성 완화 | `nexa-ui` `nexa-dlg/src/lib.rs:881,1229` | 로딩 중 중복 작업 2.2배 → 1배 | 중 |
| 5 | **P-5** `controls/file/*` 추출 | `nexa-ui` 전반 | 재사용·유지보수 | 대 |

**1~3만 하면 실사용 범위에서 끝난다.** 셋 다 국소 변경이고 서로 독립이다.

---

## 4. 인수인계 체크리스트 (작업 세션)

- [ ] **P-1** — `flatten()` 캐시 + 무효화. ⚠️ `pub roots` 직접 변경 경로(`lazy_load_places`)를 반드시 포함할 것. 무효화 누락 = 조용한 표시 버그
- [ ] **P-1 회귀 테스트** — `tree.rs` 기존 테스트(`flatten_respects_expansion` 836 · `collapse_hides_children` 842)가 캐시 무효화를 덮는지 확인. 덮지 않으면 "toggle 뒤 `rows()` 변화" 테스트 1개 추가
- [ ] **P-2** — TreeGrid(738)·TreeView(498) 양쪽. `explorer.rs`는 nexa-sql 커밋으로 분리
- [ ] **P-3** — `entry_of` 하나만 고치면 동기·워커 양쪽에 적용됨. **링크 폴더 진입이 되는지 실기 확인**(정확성이 이 코드의 존재 이유)
- [ ] 저장소가 둘이므로 **커밋도 둘**(`nexa-ui` · `nexa-sql`). push 전 `scripts/check-3os.sh`
- [ ] 문서 트랜잭션(journal → DEVLOG → STATUS → TODO)은 [16](16-doc-git-conventions.md) 규약대로
- [ ] 수정 뒤 §5로 재측정해 이 문서의 표를 **실측값으로 갱신**

### TODO에 붙일 줄 (복사용 · 이 문서는 `TODO.md`를 고치지 않았다)

```
| **T-82** | P0 | 소 | 트리 `rows()` 평탄화 캐시 — 4,899행 8.58ms/프레임 → ~0([37 P-1](37-file-picker-performance.md)) | — | ☐ |
| **T-83** | P0 | 소 | 트리·탐색기 페인트를 첫 가시 행부터(TreeGrid·TreeView·explorer)([37 P-2](37-file-picker-performance.md)) | — | ☐ |
| **T-84** | P1 | 소 | `nexa-fs::entry_of` — 링크일 때만 경로 stat(53× · [37 P-3](37-file-picker-performance.md)) | — | ☐ |
| **T-85** | P2 | 중 | 배치 도착마다 전체 재구성 완화 · `refresh_grid` 사본 제거([37 P-4](37-file-picker-performance.md)) | T-82 | ☐ |
| **T-86** | P2 | 대 | `nexa-ctl/controls/file/*` 추출 — 20 §1 계층 복원([37 P-5](37-file-picker-performance.md)) | — | ☐ |
```

> `TODO.md`의 마지막 번호가 **T-81**임을 확인했다(09-15 기준). 그 사이 번호가 늘었으면 조정할 것.

---

## 5. 재현 — 벤치 절차

조사 세션은 **프로젝트 소스를 고치지 않고** 동일 알고리즘을 복사한 독립 벤치로 측정했다. 수정 뒤 같은 방법으로 재측정한다.

### 5-1. `flatten()` 비용·메모리 (P-1 검증)

`tree.rs`의 `TreeNode`/`FlatRow`/`walk`를 그대로 복사하고, 파일 대화상자와 같은 셀 구성(보이는 3열 + 숨은 3: 경로·`d`/`f`·확장자)으로 N행 모델을 만들어 `flatten()`을 반복 측정한다. 메모리는 counting global allocator로 잰다.

```rust
// 핵심 루프 — tree.rs 원본과 동일
fn walk(nodes: &[TreeNode], path: &mut Vec<usize>, depth: usize, out: &mut Vec<FlatRow>) {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        out.push(FlatRow {
            path: path.clone(), depth, label: n.label.clone(),
            cells: n.cells.clone(), has_children: !n.children.is_empty(),
            expanded: n.expanded, image: n.image.clone(),
        });
        if n.expanded { walk(&n.children, path, depth + 1, out); }
        path.pop();
    }
}
```

**판정 기준**: 캐시 적용 후 4,899행 `rows()` 2회차 호출이 **1 µs 미만**(`Rc` 복사만)이어야 한다.

### 5-2. `fs::metadata` 방식 비교 (P-3 검증)

같은 폴더에 대해 ① `fs::metadata(de.path())` ② `DirEntry::metadata()`(링크만 ①로 폴백) 두 방식을 **양쪽 다 워밍한 뒤 순서를 바꿔 2회씩** 재고 평균한다.

> ⚠️ 순서 편향에 주의. 워밍 없이 ①을 먼저 재면 306배가 나오지만, 공정하게 재면 **53배**다. 이 문서의 표는 공정 측정값이다.

**대상 폴더**: 홈 · `C:\Windows` · `C:\Windows\System32` · `target/debug/deps`.

### 5-3. 벤치 파일

이번 세션 벤치는 조사용 임시 디렉터리에 있다(세션 종료 시 사라질 수 있음).

```
<scratchpad>/flatbench.rs    # 5-1 시간
<scratchpad>/membench.rs     # 5-1 메모리(counting allocator)
<scratchpad>/fsbench3.rs     # 5-2 metadata 비교
빌드: rustc -O -o x.exe x.rs
```

영구 보관이 필요하면 `nexa-ui`에 `benches/`로 옮기는 것을 검토한다(현재 두 저장소 모두 벤치 하니스 없음).

---

## 6. 실측 — 프로세스 메모리 (릴리스 · 09-15)

> 사용자 요청: *"전체 메모리 사용량이 과하게 많은 것 같은데 점검해줘 · release 기준 실행 후 시점별 모니터링."*
> `target/release/nexa-sql.exe` 실행 · 0.25~1초 간격 샘플링 · 사용자가 GUI 조작.

### 6-1. 시점별

| 시점 | Working Set | **Private** | 핸들 | GDI | 스레드 |
|---|---:|---:|---:|---:|---:|
| 기동 직후 (t=2.5s) | 28.3 MB | **8.44 MB** | 251 | 29 | 14 |
| 유휴 안정 (t=60~84s) | 28.4 MB | **8.39 MB** | 249 | 19 | 11 |
| 파일 창 열림 | 36.1 MB | **11.88 MB** | 353 | 65 | 15 |
| 큰 폴더 표시 중 | 36.3 MB | **11.96 MB** | 321 | 65 | 9 |
| 닫은 뒤 안정 | 34.1 MB | **9.67 MB** | 315 | 55 | 6 |

### 6-2. 결론 — 과하지 않다

**유휴 사설 메모리 8.4 MB로 형제 앱 중 가장 가볍다.**

| 앱 | Working Set | Private |
|---|---:|---:|
| **nexa-sql** | 28.4 MB | **8.4 MB** |
| nexa-beep | 20.8 MB | 8.4 MB |
| nexa-clip | 31.9 MB | 39.6 MB |
| NexaDir | 33.1 MB | 55.4 MB |

★ **작업 관리자 착시에 주의.** Working Set 28 MB와 Private 8.4 MB의 차이 약 20 MB는 **파일 기반 페이지**다 — `nexa-font`가 `Box::leak(Mmap)`으로 폰트를 매핑하고(`nexa-font/src/lib.rs:161` `map_font`), 여기에 exe·시스템 DLL이 더해진다. 물리 페이지를 다른 프로세스와 공유하고 OS가 언제든 회수하므로 **앱이 소유한 메모리가 아니다.** "메모리가 많다"는 관찰은 대개 이 숫자를 본 것이다.

### 6-3. 파일 창 비용과 회수

- **첫 열기 = +3.5 MB · 핸들 +100 · GDI +46 · 스레드 +4**(워커)
- **닫으면 2.3 MB 회수 · 스레드 정상 종료**(11→6)
- **첫 열기 뒤 핸들 ~314 · GDI ~56에서 고정** — 유휴 기준선(249/19)으로는 안 돌아온다

⚠️ **이것은 누수가 아니다.** 열고 닫기를 **3회 반복**해도 핸들 314 · GDI 56에서 **완전히 평평**했다(사이클당 증가 0). 첫 열기 때 한 번 오르는 **일회성 초기화 비용**이다. 사설 메모리만 사이클당 약 +0.1 MB 미세 상승(3회에 9.79 → 10.12 MB) — 아이콘 캐시·`probes` 맵·recent 경로 같은 캐시 성장 범위로, 장시간 관찰 가치는 있으나 현재 수준은 문제가 아니다.

### 6-4. ⚠️ 아직 못 잰 것 — 최악 케이스

측정 당시 파일 대화상자의 **기본 확장자 필터가 SQL 계열**(`file_win.rs:84` `sql_filters()` 첫 항목 = `sql·pls·plb·pks·pkb·prc·fnc·trg`)이라, System32의 파일 4,899개가 대부분 걸러졌다. **§2의 4,899행 수치가 실제로 재현된 상태가 아니다.**

**남은 검증**: 필터를 **"모든 파일(`*.*`)"로 바꾸고** System32에 진입해
- 사설 메모리 증가분(§2 추정 = TreeNode 2.01 MB + entries 약 0.9 MB)
- 스크롤·호버 중 체감 지연(P-1·P-2가 여기서 드러난다)

을 확인한다. P-1·P-2 수정 전후로 각각 재면 효과가 바로 보인다.

### 6-5. 누수 점검 결과 — 09-15 (개발 세션 · 코드 감사 + 자동 계측)

> 사용자 요청: *"닫은 뒤 Private +1.28 MB · 핸들 +66 · GDI +36이 안 돌아온다 → 누수 지점을 찾아 개선."*

**감사(짝 확인)** — 전부 해제 짝이 맞는다: winit `Window` Drop = `DestroyWindow` 메시지 · `Icon` = `RaiiIcon`/`DestroyIcon` · softbuffer `Surface` Drop = `DeleteObject`(DIB)·`DeleteDC`·`ReleaseDC` · nexa-fs `icon_to_rgba` = `GetIconInfo` 비트맵 2개 `DeleteObject` + `DestroyIcon` + `GetDC`/`ReleaseDC` · `lister` = `ReadDir` Drop(FindClose)·`JoinHandle` Drop(CloseHandle)·취소 플래그 · `FilePicker` Drop = 로더·프로브·아이콘 사본 전부.

**계측 1 — 맨 프로세스에서 셸만**(`cargo test -p nexa-fs resource -- --ignored --nocapture` · 워커 생성→확장자 20종+경로 2종 조회→워커 종료 = 1주기):

| | GDI | 핸들 |
|---|---:|---:|
| 셸 호출 전 | 0 | 140 |
| 1주기 뒤 | 40 | 277 |
| 2~5주기 뒤 | 40 | 277 |

→ 사용자가 본 **GDI +36 · 핸들 +66은 `SHGetFileInfoW` 첫 호출의 일회성 초기화**(시스템 이미지 리스트 · shell32/windows.storage 로드 · COM)다. 워커가 `CoUninitialize`를 하든 안 하든 같다(OS가 스레드 종료 때 아파트먼트를 정리). 회수할 방법이 없고 늘지도 않는다.

**계측 2 — 실제 앱 12주기**(`scripts/memcycle.ps1 -Cycles 12 -SettleMs 3000` · 수정 뒤 릴리스 · Ctrl+O → Esc 반복 · 3초 안정 뒤 샘플):

| 시점 | Private | 핸들 | GDI | USER | 스레드 |
|---|---:|---:|---:|---:|---:|
| 기동 | 8,572 KB | 253 | 29 | 24 | 14 |
| 1회 닫은 뒤 | 7,352 KB | 251 | 19 | 19 | 14 |
| 2회 닫은 뒤 | 9,972 KB | 327 | 55 | 19 | 14 |
| 5회 닫은 뒤 | 10,108 KB | 327 | 55 | 19 | 14 |
| 8회 닫은 뒤 | 10,108 KB | 327 | 55 | 19 | 14 |
| 12회 닫은 뒤 | 10,016 KB | 325 | 55 | 19 | 11 |

(1회는 창이 뜨기 전에 키가 들어가 열리지 않았다 = 기준선.) **2회 이후 핸들·GDI·USER 완전 평평 · Private는 9,972~10,108 KB 사이를 오르내림(11주기 합 +44 KB · 단조 증가 아님 = 할당자 잡음)** — 선형 누수 없음. 수정 전 같은 측정에서 닫은 뒤 핸들 353이던 것이 327로 내려왔다(워커 스레드·COM 아파트먼트를 닫는 즉시 거둔 몫 = 26). Private의 일회성 +2.6 MB = 셸 DLL 힙 + 아이콘 RGBA 캐시(≤512 · 16px 1 KB/32px 4 KB) + 글리프·도형 캐시.

**수정(작지만 규약)**: ① 아이콘 워커의 COM 아파트먼트를 스레드 수명과 묶음(`ComApartment` 진입/해제 짝 — 이전엔 초기화만). ② `IconService::release_worker()` — `FilePicker` Drop에서 호출 → **대화상자를 닫는 즉시 워커 스레드 종료**(30초 유휴 대기 없음 · 캐시는 유지). ③ 메인 창 `z_order`에서 닫힌 모달 창의 `WindowId` 제거(열 때마다 새 id → 한 칸씩 자라던 것). ④ 계측 도구 = nexa-fs `resource_tests`(ignored) + `scripts/memcycle.ps1`.

**남는 것(회수 불가 · 문제 아님)**: 셸 일회성 초기화(GDI ≈+36 · 핸들 ≈+100) · 아이콘 캐시 ≤2 MB 상한.

---

## 7. 원장 반영

- **[26 §8 네트워크 부하](26-performance-architecture.md)**와 같은 성격의 **로컬 I/O·페인트 부하 원장**이 없다. 이 문서 §2 표를 그 자리로 쓰고, **디렉터리를 읽거나 행 목록을 만드는 코드를 추가·변경하면 여기를 갱신**한다.
- **[30 아키텍처 패턴](30-architecture-patterns.md)** 체크리스트에 한 줄 추가 후보:
  > *"목록·트리 부품을 만들 때 ① 평탄화 결과에 캐시가 있는가 ② 페인트가 가시 범위만 도는가 ③ 디렉터리 메타는 `DirEntry` 캐시를 쓰는가."*

  세 번 반복된 결함이다 — 탐색기 `screen_rows` · `TreeGrid::rows` · `TreeView::rows`.
