# 105. 운영체제별 빠른 파일 열거 조사 — Goto Anything·프로젝트 파일 색인

> 2026-10-07 · 협업 세션 조사(사용자 "운영체제별로 파일 탐색을 빠르고 효과적으로 수행할 수 있는 방법 Research 필요") · 배경 = Ctrl+P 프로젝트 폴더(Downloads 10,415 파일 · 101 폴더)에서 멈춘 듯 보임 · 관련 = [T-286](TODO.md) Goto Anything · `parwalk.rs` · 37(파일 선택기 성능) · 39 §3 · 72 · 위키 [Goto-Anything](wiki/Goto-Anything.md)
>
> 표기: **추정** = 출처가 직접 말하지 않은 것 · 수치는 출처 측정 조건 그대로(장비·캐시가 다르다).

## 0. 한 줄 결론

**10k 규모의 멈춤은 열거가 아니다**(이 PC 실측 = Downloads 10,415개를 **4~8 ms**에 다 읽음) — 병목은 받는 쪽(팔레트 O(N) · 개발 세션 수정 중)이고, 열거는 지금 구조(`parwalk` + std `read_dir`)로 **100k까지 충분**하다. 할 일 = ① 소비 쪽 증분·상위 K ② 60초마다 비우고 다시 읽는 캐시를 **교체식 + 변경 감시 증분**으로 ③ **제외 규칙**(`.git` · `target` · `node_modules` · `.gitignore`) ④ mac/Linux에서 항목마다 stat하는 `entry_of` 대신 **`file_type()`(d_type)만**. MFT·Spotlight·Windows Search 같은 OS 색인은 권한·가용성 문제로 **기본 경로에 쓰지 않는다**.

### 0-1. 이 PC 실측(Windows 11 · std `read_dir` · Release · 웜 캐시 · 2회 · 협업 세션 벤치 `walkbench`)

| 대상 | 파일 · 폴더 | 순차(`file_type`만) | 순차(+`metadata`) | 4스레드 | 8스레드 |
|---|---|---|---|---|---|
| `C:\Users\…\Downloads` | 10,415 · 101 | 7.5 ~ 7.8 ms | 6.9 ~ 7.3 ms | **3.8 ~ 4.0 ms** | 6.4 ~ 9.1 ms |
| `nexa-sql/target/debug` | 179,278 · 9,359 | 336 ~ 428 ms | 345 ~ 375 ms | **118 ~ 167 ms** | 127 ~ 172 ms |

- Windows는 `metadata()`를 더해도 비용이 같다(std가 `FindNextFileW`의 `WIN32_FIND_DATAW`를 그대로 씀 — §1).
- 4스레드가 8스레드보다 낫거나 같다(같은 디스크·`Mutex` 큐 경합 · 추정). `project.scan_threads` 기본 4는 맞는 값.
- **콜드 캐시는 재지 않았다**(파일 캐시 비우기 = 관리자 권한) — 콜드는 디스크 지연에 묶여 수 배~수십 배일 수 있다(추정).

## 1. OS API 비교

| OS | API | 무엇 | 장점 | 제약 | 우리에게 |
|---|---|---|---|---|---|
| Windows | `FindFirstFileExW` + `FindExInfoBasic` | 폴더 열거 · 짧은(8.3) 이름 생략 | 10k 파일 ~12 % 빠름(250 → 220 ms) [W3] | Win7+ | **std가 이미 씀**(플래그 0) [W5] |
| Windows | `FIND_FIRST_EX_LARGE_FETCH` | 큰 버퍼로 질의 | 네트워크 등 지연 큰 매체에서 이득 [W1][W2] | 로컬 디스크에선 이득 없음·작은 폴더는 느려짐 [W3] · UI 스레드·중도 중단에 부적합 [W2] | 네트워크 폴더 한정 선택 사항 |
| Windows | `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` / `NtQueryDirectoryFile` | 버퍼 단위 대량 반환 | Go 전환 = 시스템 호출 파일당 1 → 100개당 ~1 · ReadDir −31.6 % [W6] · 5,000개 폴더 콜드 5.2 ms vs Find* 14.5 ms [W4] | Vista+ · 직접 FFI | **2단계 후보**(어댑터 격리 · DR-3) |
| Windows | NTFS MFT `FSCTL_ENUM_USN_DATA` + USN 저널 | 볼륨 전체 레코드 · 변경 기록 | Everything = 25만 파일 ~5 s · 100만 ~1분 색인 · 꺼져 있어도 변경 안 놓침 [W8] | **볼륨 핸들 = 관리자 또는 서비스** [W8][W9] · NTFS(ReFS)만 · SMB 불가 [W7] · 경로 재구성 필요 | ❌ 기본 경로 아님(권한·설치 서비스) |
| Windows | Windows Search `SystemIndex`(OLE DB) | OS 색인 질의 | 색인된 곳은 즉시 | **색인 범위(crawl scope) 밖 = 결과 없음**(추정 · 개념은 [W10]) · 서비스 꺼짐 가능 | ❌ |
| macOS | `getattrlistbulk` | 폴더 1회 호출로 이름+속성 다량 | 40만 파일 521 ms vs `du`(readdir+lstat) 2.57 s ≈ 6.4× · 버퍼 128 KB [M2] | 이름만 필요하면 `readdir`가 같거나 빠름(APFS) [M3][M4] | 속성이 필요할 때만 · **이름+종류면 `readdir` d_type으로 충분** |
| macOS | FSEvents | 폴더 단위 변경 알림 · 지연 병합 · 이벤트 ID로 재생 | 꺼져 있던 동안 변경도 `sinceWhen`으로 받음 [M5][M6] · 재귀 기본 | 폴더 단위(파일 단위 = `FileEvents` 플래그 10.7+) [M7] · 병합 시 `MustScanSubDirs` = 그 하위 재열거 | **감시 증분에 적합**(CoreServices FFI) |
| macOS | Spotlight `mdfind`/`MDQuery` | OS 색인 | 색인된 곳은 즉시 | 사용자가 끌 수 있음(`mdutil`) [M8] · 점 파일·숨김 폴더 제외(2차 출처) [M10] · node_modules 등 제외 불가(포함 범위만) [M9] | ❌ |
| Linux | `getdents64` | 커널 폴더 읽기 | glibc 버퍼 = `st_blksize` 32 KiB~1 MiB · 수백만 항목 폴더는 큰 버퍼 이득 [L1] | — | std가 glibc `readdir` 경유 |
| Linux | `d_type` | 항목 종류(파일/폴더) | **`lstat` 생략** [L2] | 일부 FS = `DT_UNKNOWN` → stat 폴백 필요 [L2] | std `DirEntry::file_type()` = 대부분 무료 [L3] |
| Linux | inotify | 폴더 단위 감시 | 즉시 알림 | **비재귀 = 폴더마다 watch** · 기본 상한 `max_user_watches` 8,192(5.11+ = 메모리 1 % · 최대 1,048,576) [L5] · watch 1개 ≈ 1,080 B [L6] · 넘침 `IN_Q_OVERFLOW` [L4] · 네트워크 FS 이벤트 없음 | 감시 증분 — 상한 넘으면 폴링 폴백 |
| Linux | fanotify | 파일 시스템 전체 감시 | `FAN_MARK_FILESYSTEM` = 재귀 대안 [L8] | 5.13 전 = `CAP_SYS_ADMIN` · 이후 비특권도 mount/filesystem 표식 불가 [L7] | ❌ |
| Linux | `plocate`/`mlocate` DB | 주기 색인 | 2,700만 중 2개 검색 0.008 s [L9] | `updatedb` 주기(실시간 아님) [L10] · 설치 여부 불확실 | ❌ |

## 2. 도구들의 방식

| 도구 | 첫 열거 | 증분 유지 | 메모리 | 상한·제외 |
|---|---|---|---|---|
| ripgrep / `ignore` crate · `fd` | `WalkParallel` = 다중 스레드 · 작업 훔치기 스택(옛 판 = `Mutex<Vec>`) [T3] · d_type으로 stat 생략(추정 · std 동작에서) · fd = 400만 파일 0.86 s(웜) vs `find` 11~20 s [T4] | 없음(매번 다시 걸음) | 걷는 동안만 | 기본 = 숨김 · `.gitignore` · `.ignore` · 전역 gitignore · `.git/info/exclude` 적용 · 링크 안 따라감 [T2] |
| VS Code Quick Open | 질의 세션마다 `rg --files --hidden …` 1회(`files.exclude` · `search.exclude`를 `-g`로) [T5][T6] · 진행 512개 단위 · 퍼지 상위 N [T7] | **영속 파일 목록 없음**(세션 키 캐시 · 앞 질의를 좁힐 때 재사용) [T6][T7] · 파일 감시 = `@parcel/watcher` 재귀(별도 프로세스) [T10] | 세션 동안 · 결과 상한 20,000 [T8] | `files.exclude` 기본 `.git` `.svn` `.hg` `.jj` `.DS_Store` `Thumbs.db` [T9] · 감시 제외 `.git/objects/**` 등 [T9] · 최근 파일 섞기 `includeHistory` 기본 켬 [T11] |
| Sublime Text Goto Anything | 사이드바 폴더 "카탈로그"(파일 목록) [T13] | **파일 시스템 이벤트로 최신 유지** [T13] · 심볼 색인은 별도 저우선 백그라운드 [T12] | 추정 불가(공개 수치 없음) | `folder_exclude_patterns` = 사이드바·Goto·프로젝트 전체 동작에서 뺌 · `binary_file_patterns` = Goto·Find에서만 뺌 [T14] · `index_workers` · `index_exclude_gitignore` [T12] |
| Everything | MFT 직접(25만 ~5 s · 100만 ~1분) [W8] | USN 저널(꺼져 있던 동안도) [W8] | 100만 ≈ 100 MB RAM · 45 MB 디스크 [W8] · 조정 시 ~50 MB [T15] · 1.5 = 항목당 ~100 B [T16] | 관리자/서비스 필요 · FAT·네트워크 = 폴더 색인(재스캔) [W8] |
| JetBrains IntelliJ | VFS = 한 번이라도 요청된 파일의 영속 스냅숏(이름·길이·시각·속성) [T17] | 네이티브 워처 `fsnotifier`(3-OS) → 보고된 변경만 새로 고침 [T18] · Linux inotify 상한 넘으면 "동기화 느림" 경고 + 재귀 스캔 폴백 · 권장 `max_user_watches = 524288` [T19] | 1차 출처 수치 없음 | 제외 폴더(VFS 자체는 거르지 않음) [T17] |

**공통 결론**: 빠른 도구들은 **OS 색인이 아니라 자기 걷기 + 제외 규칙 + (선택) 감시 증분**이다. 디스크 영속 캐시는 JetBrains·Everything만(큰 상주 비용을 감수하는 전용 도구) — VS Code는 매번 rg로 다시 걷는다(10만 단위도 충분히 빠르다는 판단 · 추정).

## 3. 제안

### 3-1. 지금 코드의 위치(10-07 확인)

| 자리 | 지금 | 문제 |
|---|---|---|
| `parwalk.rs` | 워커 N(`project.scan_threads` 4 · 상한 16) · 폴더째 메시지 · `Mutex` 큐 | 제외 규칙 없음(`show_hidden` · `show_dot`뿐) |
| `nexa_fs::list_opts` → `entry_of` | 항목마다 `de.metadata()` | Windows = 무료 · **mac/Linux = 항목마다 `lstat`**(Goto는 이름·종류만 필요) |
| `app/goto.rs` `goto_refresh_files` | 60초 지나면 **`goto_files.clear()` 뒤 다시 걸음** · 끝나면 `sort()` | 재열거 동안 목록이 비거나 줄어듦 · 전체 재열거 반복 |
| `nexa_fs::watch` | 열린 파일의 stat 서명 비교(`StatWatch`) | **폴더 감시가 아님** — 증분 색인에 못 씀 |

### 3-2. 단계 1(코드만 · 의존 0 · 바로)

1. **교체식 캐시**: 새 목록을 따로 모아 끝나면 한 번에 바꿈(옛 목록은 그동안 그대로 보임) · 60초 만료는 "다음에 열 때 뒤에서 다시"로.
2. **제외 규칙**(기본값 제안 = D-259): 폴더 이름 `.git` `.svn` `.hg` `node_modules` `target` `.nsql` + 프로젝트의 `.gitignore`(최상위만 · 간이 글롭 · 2단계에서 하위 `.gitignore`) · 설정 `project.exclude`(글롭 목록 · 파일 검색·프로젝트 탐색기와 공용 여부 = D-260).
3. **종류만 읽는 열거**: `nexa_fs`에 `list_names`(이름 + `file_type()` · `DT_UNKNOWN`/링크만 stat) 추가 → Goto·파일 검색이 씀(탐색기 패널처럼 크기·시각이 필요한 곳은 그대로).
4. **상한**: `project.scan_max` 0(무제한) 유지 + Goto 쪽 상위 K(개발 세션 진행 중) · 깊이 상한 없음(제외 규칙이 대신).
5. **경로 저장 = 폴더 표 + 이름**(arena): `PathBuf` 개별 할당 대신 `(폴더 id, 이름)` — §4 메모리.

### 3-3. 단계 2(감시 증분 · 디스크 캐시 · OS별 어댑터)

1. **폴더 감시 포트** `DirWatch`(nexa-fs · 포트 + OS 어댑터 · DR-3 격리): Windows `ReadDirectoryChangesW`(재귀 플래그 · 버퍼 넘침 = 전체 재열거) · macOS FSEvents(`MustScanSubDirs` = 그 하위만 재열거 · `sinceWhen` 재생은 쓰지 않음 = SinceNow · 앱 재시작 사이 변경은 TTL/첫 열거가 덮는다) · Linux inotify(폴더마다 watch · 상한 접근 시 폴링 폴백 + 상태줄 안내 · JetBrains와 같은 처리). 이벤트 = "이 폴더 다시 읽기" 요청으로 바꿔 `parwalk`에 넣음(폴더 단위 재열거 = 단순·안전).
2. **디스크 캐시**(선택 · D-261): 위치 = 프로젝트 모드 `NSQL_HOME/cache/projects/<해시>.idx`(프로젝트 폴더를 더럽히지 않음 · 폴더 모드는 `.nsql/`) · 내용 = 폴더 표 + 이름 + 폴더 mtime · 열 때 = 캐시 즉시 표시 → 뒤에서 폴더 mtime 비교 재열거(바뀐 폴더만) · 10k~100k에서는 **이득이 작아 미룸 권장**(§0-1 = 100k도 ~150 ms · 추정).
3. **Windows 대량 열거**(선택 · D-262): `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` 어댑터 — 콜드·네트워크에서 이득(Go −31.6 % · VA 5,000개 2.8×) · 웜 로컬에선 이미 ms 단위라 우선순위 낮음 · `LARGE_FETCH`는 네트워크 경로일 때만.
4. **하지 않음**: MFT/USN(관리자) · Spotlight · Windows Search · plocate · fanotify — 가용성·권한이 보장되지 않는다.

## 4. 규모별 비용·메모리(추정)

| 규모 | 첫 열거(웜 · 4스레드) | 첫 열거(콜드) | 재열거(단계 2 감시 증분) | 목록 메모리 `PathBuf`(지금) | 폴더 표 + 이름(제안) |
|---|---|---|---|---|---|
| 10k | ~4 ms(실측) | 수십~수백 ms(추정) | 바뀐 폴더만 = ms 미만 | ~1~1.5 MB(경로 ~80 B + 할당 머리 · 추정) | ~0.3~0.5 MB |
| 100k | ~70~150 ms(179k 실측 118~167 ms에서 비례 · 추정) | 1~수 s(추정) | 같음 | ~10~15 MB | ~3~5 MB |
| 1M | ~1~1.5 s(추정) | 10 s 안팎(추정 · Everything MFT도 ~1분 [W8]) | 같음 · Linux inotify 폴더 수 > 상한 가능 | ~100~150 MB | ~30~50 MB(Everything ~50~100 MB/1M [T15][T16]와 같은 자릿수) |

- 퍼지 일치 비용은 별도(목록 N × 질의) — 상위 K + 증분(앞 질의 결과를 좁힘 = VS Code 방식 [T7])으로 막는다.
- 1M 규모는 **제외 규칙이 사실상 핵심**(`node_modules`·`target`이 대부분인 경우가 흔함 · 추정) · `project.scan_max`(> 0 = 메모리 보호)를 안내.

## 5. 결정·작업 후보

| # | 후보 | 권장 |
|---|---|---|
| **D-259** | 기본 제외 폴더 = `.git` `.svn` `.hg` `node_modules` `target` `.nsql` + 최상위 `.gitignore` | 채택(VS Code·ripgrep 기본과 같은 방향) |
| **D-260** | 제외 설정 `project.exclude` 하나를 Goto · 파일 검색 · 프로젝트 탐색기가 공용 | 공용(Sublime `folder_exclude_patterns`처럼 한 곳) · 탐색기 표시만 따로 끌 수 있게 |
| **D-261** | 디스크 색인 캐시 | 보류(100k까지 열거가 충분히 빠름 · 1M 사용자 생기면) |
| **D-262** | Windows `FileIdBothDirectoryInfo` 대량 열거 어댑터 | 보류(콜드·네트워크 측정 뒤) |
| **D-263** | 폴더 감시 포트 `DirWatch`(3-OS 어댑터 · 상한 넘으면 폴링) | 단계 2로 채택 · nexa-ui에 부품 |

| # | 작업 | 단계 |
|---|---|---|
| **T-289** | Goto 캐시 교체식(재열거 중 옛 목록 유지) · 60초 만료 = 다음 열 때 뒤에서 | 1 |
| **T-290** | 제외 규칙 `project.exclude` + 기본값(D-259) + 최상위 `.gitignore` 간이 해석 · `parwalk` 적용 | 1 |
| **T-291** | `nexa_fs::list_names`(이름 + `file_type()`만 · `DT_UNKNOWN`/링크만 stat) → Goto·파일 검색 | 1 |
| **T-292** | 목록 저장 = 폴더 표 + 이름(arena) · 퍼지 대상 문자열 공유 | 1 |
| **T-293** | nexa-ui `DirWatch` 포트 + 3-OS 어댑터(D-263) → "폴더 다시 읽기"를 `parwalk`에 · Windows ✅ bin50 · macOS ✅ 10-10(FSEvents · 실제 경로 매핑) · Linux inotify 남음 | 2 |
| **T-294** | 콜드 캐시·네트워크 폴더 측정(Windows 대기 목록 비우기 = 관리자 → 사용자 실행 또는 재부팅 직후) → D-262 판단 | 2 |

## 출처

- [W1] FindFirstFileExW (`FIND_FIRST_EX_LARGE_FETCH`) — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw · `FindExInfoBasic` — https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ne-minwinbase-findex_info_levels
- [W2] Raymond Chen, LARGE_FETCH 사용 지침 — https://devblogs.microsoft.com/oldnewthing/?p=2843
- [W3] 독립 측정(LARGE_FETCH 로컬 이득 없음 · FindExInfoBasic ~12 %) — https://www.delphitools.info/2013/11/25/efficient-file-enumeration/2/
- [W4] Visual Assist 5,000개 콜드 비교 — https://www.wholetomato.com/blog/how-to-query-file-attributes-50x-faster-on-windows/?amp=1
- [W5] Rust std Windows `readdir` 구현 — https://rust.googlesource.com/rust/+/HEAD/library/std/src/sys/fs/windows.rs · https://doc.rust-lang.org/stable/std/fs/fn.read_dir.html
- [W6] Go `GetFileInformationByHandleEx` 전환(−31.6 %) — https://go.googlesource.com/go/+/1951857ec07c1d491e1836770a647d3902934a67 · API — https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex
- [W7] `FSCTL_ENUM_USN_DATA` — https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data · https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-mft_enum_data_v1
- [W8] Everything FAQ — https://www.voidtools.com/faq/
- [W9] Everything 서비스 — https://voidtools.com/forum/viewtopic.php?p=21231
- [W10] Windows Search 질의 — https://learn.microsoft.com/en-us/windows/win32/search/-search-3x-wds-qryidx-searchqueryhelper
- [M1] `getattrlistbulk(2)` — https://opensource.apple.com/source/xnu/xnu-6153.61.1/bsd/man/man2/getattrlistbulk.2.auto.html
- [M2] dumac 측정 — https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos
- [M3] APFS 열거 비교(Tempelmann) — https://mjtsai.com/blog/?p=25067
- [M4] 같은 측정 수치 — https://blog.tempel.org/2019/04/
- [M5] FSEvents 개요 — https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/TechnologyOverview/TechnologyOverview.html
- [M6] FSEvents 사용 — https://developer.apple.com/library/mac/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html
- [M7] FileEvents 플래그 — https://github.com/zchee/go.fsevents/blob/master/INTERNALS.md
- [M8] Spotlight 끄기 — https://eclecticlight.co/2026/01/16/can-you-disable-spotlight-and-siri-in-macos-tahoe/
- [M9] Spotlight 제외 범위 — https://www.alfredapp.com/help/kb/node-modules/ · https://alexwlchan.net/2021/ignore-lots-of-folders-in-spotlight/
- [M10] Spotlight 숨김 파일(2차 출처) — https://opper.ai/ai-roundtable/questions/the-macos-finder-search-doesnt-include-folders-and-file-cfd6e29a
- [L1] getdents 버퍼 크기 — https://github.com/golang/go/issues/64597
- [L2] `readdir(3)` d_type — https://man7.org/linux/man-pages/man3/readdir.3.html
- [L3] Rust `DirEntry::file_type` — https://doc.rust-lang.org/std/fs/struct.DirEntry.html
- [L4] `inotify(7)` — https://man7.org/linux/man-pages/man7/inotify.7.html
- [L5] `max_user_watches` 기본값 변경(5.11) — https://code.amethyst.name/public-mirrors/linux/commit/92890123749bafc317bbfacbe0a62ce08d78efb7 · https://lkml.iu.edu/hypermail/linux/kernel/2010.3/08749.html
- [L6] VS Code Linux watch 비용 — https://code.visualstudio.com/docs/setup/linux
- [L7] `fanotify_init(2)` — https://man7.org/linux/man-pages/man2/fanotify_init.2.html
- [L8] `fanotify(7)` — https://man7.org/linux/man-pages/man7/fanotify.7.html
- [L9] plocate — https://plocate.sesse.net/
- [L10] plocate vs mlocate — https://www.linuxuprising.com/2021/09/plocate-is-much-faster-locate-drop-in.html
- [T2] `ignore::WalkBuilder` — https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html · `WalkParallel` — https://docs.rs/ignore/latest/ignore/struct.WalkParallel.html
- [T3] ripgrep 작업 훔치기 스택 — https://github.com/BurntSushi/ripgrep/commit/d938e955af7a39ac0245b1ce84924f7f4a2141ac
- [T4] fd 벤치 — https://github.com/sharkdp/fd
- [T5] VS Code Search Issues 위키 — https://github.com/microsoft/vscode/wiki/Search-Issues
- [T6] `ripgrepFileSearch.ts` · `fileSearch.ts` — https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/search/node/ripgrepFileSearch.ts · https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/search/node/fileSearch.ts
- [T7] `rawSearchService.ts` — https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/search/node/rawSearchService.ts
- [T8] `search.ts`(`DEFAULT_MAX_SEARCH_RESULTS`) — https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/search/common/search.ts
- [T9] `files.contribution.ts`(제외 기본값) — https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/contrib/files/browser/files.contribution.ts
- [T10] VS Code 파일 감시 내부 — https://github.com/microsoft/vscode/wiki/File-Watcher-Internals
- [T11] `search.contribution.ts` — https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/contrib/search/browser/search.contribution.ts
- [T12] Sublime 색인 — https://www.sublimetext.com/docs/indexing.html
- [T13] Sublime 폴더 카탈로그 — https://forum.sublimetext.com/t/disabled-fonder-scanning-with-index-files-false-not-working/24167
- [T14] Sublime 프로젝트 설정 — https://www.sublimetext.com/docs/projects.html
- [T15] Everything 1.4 메모리 조정 — https://voidtools.com/forum/viewtopic.php?p=31241
- [T16] Everything 1.5 메모리 — https://www.voidtools.com/forum/viewtopic.php?p=75831
- [T17] IntelliJ VFS — https://plugins.jetbrains.com/docs/intellij/virtual-file-system.html
- [T18] IntelliJ VFS(fsnotifier) — https://confluence.jetbrains.com/display/IDEADEV/IntelliJ+IDEA+Virtual+File+System
- [T19] IntelliJ inotify 상한 — https://intellij-support.jetbrains.com/hc/en-us/community/posts/115000013130/comments/115000032524

> 확인하지 못한 것: VS Code `search.followSymlinks` 기본값 · `NtQueryDirectoryFile` 버퍼 크기 효과의 1차 수치 · JetBrains VFS 파일당 메모리 · Spotlight 숨김 파일 동작(2차 출처뿐).
