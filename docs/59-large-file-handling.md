# 59 · 대용량 파일 처리 — 다른 편집기 비교 · 이번에 고친 것 · 개선 제안 (사용자 요청 09-19)

> **요구**: ① 성능 향상을 위해 고칠 곳을 정리하고 설계 변경·최적화 진행 ② 메모리 회수(큰 파일 닫기 · 접속 해제 · 결과 폐기)를 전수 점검하고 "즉시 1회 + 주기" 회수 ③ 대용량 파일의 편집·관리 방식을 다른 편집기와 비교해 개선안 제안.
> **상태**: ①② ✅ 구현(09-19 · journal 82차) · ③ 📐 제안(§4 · **결정 대기 D-125~D-128**). 관련: [55 입력 지연](55-editor-input-latency.md) · [26 §7-3·7-4 메모리](26-performance-architecture.md) · [39 부하원](39-resource-governance.md).

## 0. 결론

1. **측정으로 찾은 병목 다섯**을 고쳤다 — 전부 "매 프레임/매 키마다 본문 전체를 다시 훑는" 종류였다. 4만 줄·3.6 MB 스크립트(전 기능 켬) 기준: 유휴 그리기 **78 → 3.3 ms** · 글자 입력 **209 → 16 ms** · 앱 유휴 CPU **812 → 188 ms/6초**.
2. **가장 큰 메모리 위험은 되돌리기였다** — 묶음마다 본문 전체(`Vec<char>` = 4 B/글자)를 복사했고 상한이 1,000개라, 100 KB 스크립트도 오래 편집하면 수백 MB까지 갈 수 있었다. → **전체 복사 1개 + 차이만** 저장.
3. **회수는 이미 대부분 잘 되고 있었다**(큰 탭 닫기 41.5 → 10.5 MB · 10만 행 결과 교체 41.1 → 10.6 MB — 기준선까지). 여기에 힙 → OS 정리(즉시 1회 + 유휴 주기)와 보이지 않는 탭의 캐시 해제를 더했다.
4. 남은 구조적 비용은 **버퍼 표현**이다: `Vec<char>`(4 B/글자) + 저장 기준 `String` + 줄 변경 기준선 = 파일의 ≈ 6.6배. 주류 편집기는 1.0~1.1배다. → §4 2단계(UTF-8 갭 버퍼 + 줄 시작 표) 제안.
5. SQL 클라이언트다운 해법은 "큰 스크립트를 **열지 않고 실행**"이다(DataGrip · DBeaver 네이티브 실행 · sqlcmd · SQLcl 모두 이 길) → §4 3단계.

## 1. 이번에 고친 것(측정 → 원인 → 조치)

측정 도구: `nexa-ctl examples/bench_editor <줄 수> <기능>`(기능별 페인트·입력 비용 · 09-19 확장) · 앱은 `NSQL_TRACE_FRAMES`.

| # | 증상(4만 줄 · 3.6 MB) | 원인 | 조치 | 결과 |
|---|---|---|---|---|
| P1 | 구문 강조를 켜면 그리기 14 → **101 ms**(캐럿이 파일 끝쪽일 때) | 매 프레임 **0번 줄부터 화면 첫 줄까지** 다시 토큰화(블록 주석 상태를 이어 오려고) | 줄 시작 상태 캐시 `HlStateCache` — 줄 해시로 "앞에서부터 같은 줄 수"를 알아내 **바뀐 줄부터만** · 필요한 줄까지만(지연) | 3.3 ms |
| P2 | 글자 하나 = **200 ms**(기능을 다 꺼도) | 본문이 바뀔 때마다 **전 줄의 폭을 폰트 엔진으로** 다시 잼(가로 스크롤 범위용) | 행별 폭 캐시 `RowWidthCache` — 앞·뒤의 같은 행은 옛 폭, **바뀐 가운데 행만** 다시 잼 | 키당 1행 |
| P3 | 입력 처리만 **24 ms** | 자동 닫기·내어쓰기·입력 가능 길이 계산이 키마다 본문 전체를 `String`으로 모았다가 `Vec<char>`로 다시 풂(10여 곳) | 편집 버퍼 슬라이스를 직접 읽음(앞·뒤 글자 · 현재 줄만) | 0.1 ms |
| P4 | 매 프레임 본문 `String` 재조립(3.6 MB) + 줄 나누기 + 줄 수 세기 + 캐럿 줄 세기 | 그리기가 본문을 `&str` 줄로 필요로 함 | 세대(`edit.rev`)별 `MlTextCache`(본문 문자열 + 줄 표 한 번에 생성) · 캐럿 줄 = 이분 탐색 · 줄끝 표시는 버퍼를 빌림 | 편집 없는 프레임 = 0 |
| P5 | 되돌리기 = 묶음마다 본문 전체 복사(3.6 MB 파일 = 단어마다 14.6 MB · 상한 1,000) | `Snap { buf: Vec<char> }` | `SnapBuf::{Full, Delta}` — 맨 위만 전체, 새 묶음을 기록하는 순간 앞의 것을 **차이**로 접음 · 다시 실행 스택은 늘 차이 | 전체 1개 + 친 만큼 |
| P6 | 세션 창이 열려 있으면 유휴 CPU 90%([26 §7-3](26-performance-architecture.md)) | 그린 직후 다시 그리기를 요청하는 고리 | 그린 뒤에는 요청하지 않음 | 5,438 → 16 ms/6초 |

비용: 캐시가 본문의 UTF-8 사본(파일 크기) + 행당 28 B(해시·폭·상태)를 든다 — 3.6 MB 파일에서 +6.6 MB. 보이지 않는 탭의 캐시는 회수 때 놓는다(§2).

## 2. 메모리 회수 — 점검 결과와 "즉시 1회 + 주기"

### 2-1. 전수 점검(무엇이 언제 버려지는가)

| 자원 | 놓는 시점 | 점검 결과 |
|---|---|---|
| 편집기 탭(본문·히스토리·캐시·구문) | 탭 닫기 | ✅ 즉시(벡터에서 제거) · 마지막 탭은 비우기만 — 버퍼를 새로 만들어 옛 용량도 버려진다 |
| 탭별 결과 패널(그리드·텍스트 보기·페치 버퍼) | 탭 닫기(`panels.retain`) · 새 실행(교체) · 결과 탭 상한(`evict`) | ✅ |
| 외부 변경 상태 · 탭-세션 묶임 · 트랜잭션 배지 | 틱에서 살아 있는 탭 id로 정리 | ✅ |
| 세션(워커 스레드 · 드라이버 핸들) | 접속 해제 · 유휴 닫기 · 탭 닫기(전용) | ✅ `reap_sessions` |
| 탐색기 메타 세션 | 그 서버의 세션 0 → 접속만 닫고 **트리는 유지**(설계 52 §2-2) · 유휴 회수 | ✅(의도) |
| **접속 해제 뒤의 결과** | 남긴다(09-18 결정 — 보기용) | ⚠️ 설계상 유지 → 새 설정 `mem.release_results_on_disconnect`(기본 끔)로 선택 |
| 보조 창 프레임버퍼 | 창 닫기 | ✅(26 §7-2 ④) |
| **되돌리기 히스토리** | 탭 닫기 · `set_text` | 🔧 P5(전체 복사 누적) 수정 |
| **보이지 않는 탭의 그리기 캐시**(이번에 생김) · 미니맵 픽셀 | — | 🔧 회수 때 `TextBox::release_caches` |
| **힙 → OS** | 러스트는 값이 버려질 때 힙에 돌려줄 뿐 | 🔧 `memtrim.rs` |

### 2-2. 회수 정책(`memtrim.rs` · `main.rs mem_*`)

- **즉시 1회**(`mem.trim_on_release` 기본 켬): 탭 수가 줄었을 때 · 마지막 탭을 비웠을 때 · 실행이 끝났을 때(앞선 결과를 놓음) · 접속이 끊겼을 때 → **1초 뒤**(해제가 끝나고 · 연달아 일어나도 한 번) 회수.
- **주기**(`mem.trim_secs` 기본 300 · 0 = 끔): 입력 없음 5초 + 실행 중 세션 없음일 때만. 바쁘면 10초 뒤 다시 본다.
- **회수 한 번** = ① 보이지 않는 탭의 그리기 캐시 해제 ② 힙 정리 — Windows `HeapSetInformation(HeapOptimizeResources)` + 모든 힙 `HeapCompact` · Linux `malloc_trim(0)` · macOS `malloc_zone_pressure_relief`. **워킹셋 트림은 하지 않는다**(26 §7-2 ③ — 숫자만 줄고 복귀 때 느려진다).
- 수동: 팔레트 **"View: Release Unused Memory Now"**(전후 Private를 상태줄에). 개발자 모드(`load` 층)에는 회수마다 전후·걸린 시간이 남는다 · `NSQL_TRACE_MEM`.

### 2-3. 실측(Release · Private Bytes · 격리 설정 폴더 · `@after:` 기동 명령)

| 시나리오 | 사용 중 | 놓은 뒤(회수 켬) | 놓은 뒤(회수 끔) |
|---|---:|---:|---:|
| 3.6 MB 스크립트 탭 닫기 | 41.5 MB | **10.5 MB** | 10.8 MB |
| 10만 행 × 5열 결과 → 1행 결과로 교체 | 41.1 MB | **10.6 MB** | 11.5 MB |
| 10만 행 결과 → 접속 해제 | 41.1 MB | 40.9 MB(결과 유지 — 설계) | 41.3 MB |

읽을 점: Windows 힙은 큰 조각을 이미 잘 돌려준다(기준선 10.5 MB 회복). 힙 정리의 추가 효과는 0.3~0.9 MB — 작은 조각이 많이 남는 장시간 세션을 위한 안전망으로 본다.

## 3. 다른 편집기는 큰 파일을 어떻게 다루나(09-19 조사)

| 편집기 | 버퍼 구조 | 큰 파일 기준 | 기준을 넘으면 끄는 것 | 메모리 |
|---|---|---|---|---|
| **VS Code** | piece tree(64 KB 조각 · 조각별 줄 시작 표 · 레드블랙 트리에 길이·줄 수 요약) | 20 MB 또는 30만 줄 · 50 MB부터 확장 동기화 없음 · 1 GB부터 열기 전 확인 | 토큰화 · 줄 접기(wrap) · 폴딩 · 코드렌즈 · 단어 강조 · sticky scroll — 알림 + "강제로 켜기" 버튼 · 2만 자 넘는 줄은 토큰화 안 함 | ≈ 파일 크기(재구현 전 = 줄마다 객체 → 35 MB 파일이 600 MB) |
| **Sublime Text** | 전체 적재(토큰 덩어리) | 4 MB | 구문 강조 | 수 배 |
| **Zed** | 로프(SumTree · 128 B 조각 · 요약에 UTF-8/UTF-16 길이·줄·최장 행) | — | — | 로프 |
| **Helix / ropey** | 로프(UTF-8 · ≈ 1 KB 노드) | 파서 500 ms 제한 | tree-sitter | ≈ 1.1배 · 100 MB에서 초당 180~330만 삽입 |
| **JetBrains · DataGrip** | — | 2.5 MB(인텔리센스) · 20 MB(내용 적재) | 코드 인사이트 · 20 MB 넘으면 **일부만 표시** · DataGrip은 **열지 않고 실행** | 힙 |
| **Notepad++ · Scintilla** | 갭 버퍼 + 줄 시작 분할 표 · 글자+스타일 바이트 | 설정 가능(MB · 예전 200 MB 고정) | 줄 접기 · 자동 완성 · 스마트 강조 · 괄호 짝 · URL · 구문 — **항목별 토글** | ≈ 2배 · 스타일은 **보이는 끝까지만 지연 계산**(`end styled`) · 나머지는 유휴 시간에 |
| **Vim** | 스왑 파일 기반 블록 트리 | 플러그인 20 MB · 3,000열 · 다시 그리기 2초 | 구문 · 되돌리기 · 스왑 | 디스크 |
| **Emacs** | 갭 버퍼 | 10 MB에서 경고 · 1만 자 줄(so-long) | "literal" 방문 = 비싼 기능 끔 · vlf = 구간 단위 보기/편집 | 1배 |
| **EmEditor** | 디스크 임시 파일 | 300 MB | 여러 줄 주석 강조 · 줄 접기 — 읽는 중에도 볼 수 있고 취소 가능 · **일부만 열기** | 디스크 |
| **DBeaver** | Eclipse 문서 | 2 MB(SQL 파서 끔) · 100문장(결과 페치 안 함 물음) | 파서 · 서비스 · 페치 — **네이티브 실행**(psql/sqlplus) · 50 MB 스크립트에서 OOM 보고 | 힙 |
| **SSMS · SQL Developer** | — | — | 공식 안내 = `sqlcmd -i` / SQLcl `@file` | — |

갭 버퍼 vs 로프(1 GB 실측 · coredumped.dev): 메모리 오버헤드 ≈ 0 vs 5~15% · 갭 이동 1 GB = 22 ms · 검색 35 ms vs ≈ 250 ms · 다중 커서는 커서 간격이 1 KB를 넘으면 로프가 유리.

## 4. 제안(3단계 · 결정 D-125~D-128)

### 1단계 — 버퍼 구조를 두고 하는 것(✅ 대부분 이번에 함)

- ✅ 세대(`rev`)로 묶은 캐시 · 보이는 곳만 계산 · 구문 상태 지연 계산(Scintilla `end styled`와 같은 생각) · 본문 전체 `String` 재조립 제거 · 되돌리기 차이 저장 · 임시 할당 회수.
- ☐ **큰 파일 모드**(VS Code·Notepad++식 · 설정 레지스트리 · 바이트와 줄 수 둘 다):

  | 단계 | 기준(안) | 끄거나 줄이는 것 |
  |---|---|---|
  | L1 | 5 MB 또는 10만 줄 | 미니맵 = 간격 표본 · 선택어 강조 끔 · 괄호 짝 표 = 화면 ±N줄 · 줄 변경 기준선 = 줄 해시만(5 MB → 320 KB) |
  | L2 | 20 MB 또는 30만 줄 | 구문 강조 · 줄 접기 · 줄 변경 표시 · 자동 병합(58) 끔 · 2만 자 넘는 줄은 토큰화 안 함 |

  상태줄에 **"큰 파일(L2)"** 표식 — 누르면 기능별 토글 + "강제로 켜기"(VS Code). → **D-125**
- ☐ 저장 기준 `String`을 없애고 되돌리기 스택의 "저장 지점" 표식으로 대체(−파일 크기) — 더러움 판정·외부 변경 병합(58)의 base는 줄 해시 + 필요할 때 디스크에서 다시 읽기.

### 2단계 — 버퍼 표현 교체(설계 변경 · **D-126**)

| | 갭 버퍼(UTF-8) | piece table/tree | 로프 |
|---|---|---|---|
| 메모리 | 1.0배 + 갭 | ≈ 1.0배 + 추가 버퍼(원본을 저장 기준·기준선으로 겸용 · mmap 가능) | ≈ 1.1배 |
| 가까운 편집 | O(1) | O(log p) | O(log n) |
| 먼 곳으로 점프 | memmove(100 MB ≈ 2 ms) | O(log p) | O(log n) |
| 다중 커서 | 오프셋 순서로 한 번 쓸기(100 MB ≤ 2 ms) | 좋음 | 가장 좋음 |
| 줄 찾기 | 별도 줄 시작 표 | 조각별 줄 시작 + 트리 합계 | 요약 |
| 검색 | 연속 메모리 — 가장 빠름 | 조각 단위 | 조각 단위(≈ 7배 느림) |
| 자체 구현 규모(어림) | 400~700줄 | 1,200~2,000줄 | 1,500~2,500줄 |

**권장 = UTF-8 `Vec<u8>` 갭 버퍼 + Scintilla식 줄 시작 표**(줄 시작도 갭 버퍼 · "계단" 지연 오프셋이라 편집 O(1) · 줄→오프셋 O(1)). 이유: 셋 중 가장 작고 · Scintilla·Emacs가 증명했고 · 검색이 가장 빠르고 · 3.6 MB 파일이 24 MB → ≈ 4 MB가 되며 · 우리 상한(100 MB)에서 최악의 갭 이동이 수 ms다. 버퍼 접근을 포트(`slice(range)` · `line_start(n)` · `line_of(off)` · `replace(range, &str)`) 뒤에 두면 나중에 100 MB 넘는 mmap 파일·백그라운드 스냅샷이 필요할 때 piece tree로 바꿀 수 있다. 로프는 파서 스레드·협업을 넣을 때만 값을 한다. 커서·열은 바이트 오프셋으로 두고 글자·칸 변환은 보이는 줄에서만.

되돌리기 = 연산 기록(Scintilla) `{오프셋 · 지운 글 · 넣은 글 · 앞뒤 커서}` · 묶음 단위 · 인접 타이핑 병합. **메모리 상한**(→ **D-127**): 개수(`editor.undo_max`)가 아니라 **바이트 예산** `max(16 MB, min(파일 × 2, 256 MB))` · 오래된 묶음부터 버림 · 저장 지점이 버려지면 "영구 더러움" · 예산을 넘는 단일 연산(L2의 전체 바꾸기)은 "되돌릴 수 없습니다 — 계속?".

영향 범위: nexa-ctl `EditState`(캐럿·선택·다중 커서가 char 인덱스) · `TextBox` 전반 · nexa-sql의 char 인덱스 호출부(찾기·실행 범위·오류 줄). 큰 작업이라 **T-142로 분리**하고 포트 도입 → 구현 교체 → 호출부 이관의 3보로 나눈다.

### 3단계 — 큰 SQL 스크립트 관리(SQL 클라이언트 고유 · **D-128**)

1. **디스크에서 바로 실행**(GUI "File ▸ Run File…" · CLI `nsql run`은 이미 파일 기반): 64 KB 블록을 읽는 **스트리밍 문장 분할기**(따옴표·주석·`$$`·`/`·`GO`·`DELIMITER` 상태를 블록 경계 너머로 유지) · 진행(바이트·문장 수) · ■ = 기존 `CancelHandle` · 오류 시 정책(멈춤/계속) · N문장 넘으면 결과 페치를 기본으로 끔(DBeaver 100문장).
2. **열기 대화상자**(기준 20~50 MB): 열기 / **읽기 전용으로 열기(L2)** / **일부만 보기**(바이트 구간 · 앞·뒤 N MB) / **열지 않고 실행** / 취소(EmEditor · vlf · JetBrains).
3. **비동기 적재**: 워커 스레드 · 첫 화면 먼저 · 취소 가능한 진행 막 · 줄 표를 읽으면서 만든다(사본 없이 · Scintilla `ILoader`).
4. **인코딩**: BOM + 앞·뒤 64 KB 표본으로 판정 · 스트림으로 UTF-8 변환 · 손실이면 경고.
5. **읽기 전용 뷰의 따라가기(tail)**: [58](58-external-change-policy.md)의 서명 확인 재사용(크기·시각 + 앞·뒤 블록 해시) · 폴링 스레드 없음.
6. **경고**: 되돌릴 수 없는 연산 · 본문 전체를 만드는 연산(서식 정리) · 큰 파일의 외부 변경.

### 결정(사용자 확인)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-125 | 큰 파일 모드 | ① L1 5 MB/10만 줄 · L2 20 MB/30만 줄 · 상태줄 표식 + 기능별 토글 ② L2만 ③ 없음(캐시 최적화로 충분) | **①** |
| D-126 | 버퍼 표현 교체 | ① UTF-8 갭 버퍼 + 줄 시작 표(포트 뒤에) ② piece tree ③ 로프 ④ 지금 구조 유지 | **①**(T-142 · 3보로) |
| D-127 | 되돌리기 상한 | ① 바이트 예산(`max(16 MB, min(파일×2, 256 MB))`) ② 개수 상한 유지(지금 = 차이 저장이라 실제 위험은 작음) ③ 둘 다 | **③** |
| D-128 | 큰 스크립트 관리 | ① 디스크에서 실행 + 열기 대화상자(읽기 전용·일부 보기) + 비동기 적재 ② 디스크에서 실행만 먼저 ③ 보류 | **②**(가장 값이 큰 것부터) |

## 출처

- VS Code: [Text Buffer Reimplementation](https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation) · [textModel.ts 기준값](https://github.com/microsoft/vscode/blob/main/src/vs/editor/common/model/textModel.ts) · [largeFileOptimizations.ts](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/codeEditor/browser/largeFileOptimizations.ts)
- Zed: [Rope & SumTree](https://zed.dev/blog/zed-decoded-rope-sumtree) · Xi: [Rope science](https://xi-editor.io/docs/rope_science_01.html) · ropey: [README](https://github.com/cessen/ropey/blob/master/README.md)
- JetBrains: [Tuning the IDE](https://www.jetbrains.com/help/idea/tuning-the-ide.html) · DataGrip: [Run SQL files](https://www.jetbrains.com/help/datagrip/run-sql-files.html)
- Notepad++: [Preferences — Large File Restriction](https://npp-user-manual.org/docs/preferences/) · Scintilla: [ScintillaDoc](https://www.scintilla.org/ScintillaDoc.html)(`SCI_SETIDLESTYLING` · `SC_DOCUMENTOPTION_STYLES_NONE` · `ILoader`)
- Vim: [options](https://github.com/vim/vim/blob/master/runtime/doc/options.txt) · [LargeFile](https://github.com/vim-scripts/LargeFile) · Emacs: [Visiting](https://www.gnu.org/software/emacs/manual/html_node/emacs/Visiting.html) · [so-long](https://elpa.gnu.org/packages/so-long.html) · [vlf](https://elpa.gnu.org/packages/vlf.html)
- EmEditor: [Large file support](https://www.emeditor.com/text-editor-features/large-file-support/files-up-to-248gb/) · [Large File Controller](https://www.emeditor.com/text-editor-features/large-file-support/large-file-controller/) · UltraEdit: [Large file handling](https://www.ultraedit.com/support/tutorials-power-tips/ultraedit/large-file-handling/) · klogg: [문서](https://github.com/variar/klogg/blob/master/DOCUMENTATION.md)
- DBeaver: [SQL Execution](https://dbeaver.com/docs/dbeaver/SQL-Execution/) · [#37660 50 MB OOM](https://github.com/dbeaver/dbeaver/issues/37660) · SSMS: [sqlcmd 안내](https://learn.microsoft.com/en-us/troubleshoot/sql/ssms/exception-you-execute-query)
- 자료구조 비교: [Text showdown: gap buffers vs ropes](https://coredumped.dev/2023/08/09/text-showdown-gap-buffers-vs-ropes/) · [Text editor data structures](https://cdacamar.github.io/data%20structures/algorithms/benchmarking/text%20editors/c++/editor-data-structures/)
