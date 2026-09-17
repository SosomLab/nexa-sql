# TODO — 순차 백로그

> ID · 우선(P0~P2) · 규모(소/중/대) · 의존 · 상태(☐/🚧/✅/⏸). 목표순.

## 1. 결정 (코드보다 먼저)
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-1** | P0 | 소 | DP-1~10 사용자 확정 → DR 승격([10 §2](10-decision-record.md)) | — | ✅ 09-12(DR-10~20) |
| **T-2** | P0 | 소 | ★ D-15~D-17 사용자 답(다음 우선순위 · 실기 OS · 캐시 위치) → DR 승격 · D-14는 DR-21 | — | 🚧 |
| **T-2b** | P0 | 소 | `integration` 워크플로 green → 실서버 결함 3건 수정 · T-4/T-5 닫힘 | — | ✅ 09-12 |
| **T-2c** | P1 | 소 | 사내 Oracle(192.168.0.58/BISCM 19c) 실접속 — `nsql conn test biscm` · `it-oracle.sql` 통과 · GUI 창 접속 (win) · ☐ mac에서도 | T-4 | ✅ 09-13(win) |

## 2. M1 — CLI 관통
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-3** | P1 | 중 | `nsql-net` — 취소 토큰(OCIBreak·TDS Attention) · 타임아웃 · SSH 터널(어댑터가 소켓 소유 → 터널·자격증명만) | — | ☐ |
| **T-4** | P0 | 대 | `nsql-driver-oracle` — ✅ 구현 · ✅ **실서버 검증**(Oracle Free 23ai · integration) · ✅ Instant Client 경로(`NSQL_ORACLE_CLIENT_DIR`)·DPI-1047 안내 · ☐ 취소(OCIBreak) | — | ✅ 09-12 |
| **T-5** | P0 | 대 | `nsql-driver-mssql` — ✅ 구현 · ✅ **실서버 검증**(SQL Server 2022 · integration) · ☐ Entra/통합 인증 · 오류 줄 보정 | — | ✅ 09-12 |
| **T-6** | P0 | 중 | `nsql run` — 파일/stdin · `&1..&n` · `WHENEVER` · 형식 출력 | — | ✅ 09-12 |
| **T-7** | P1 | 중 | `nsql shell` — ✅ 최소(줄 누적 실행) · ✅ 09-16 별칭(T-52)·SPOOL·`@@` · ☐ 줄 편집·히스토리·자동완성 | — | 🚧 |
| **T-8** | P1 | 중 | `nsql-io` export — ✅ 6형식 · ☐ 스트리밍(행 단위 fetch) · xlsx/parquet | — | 🚧 |
| **T-9** | P1 | 소 | ✅ 09-16 48차 — `SPOOL` 포트(`nsql_run::Spool` · CLI tee · APPEND/CREATE/OFF · GUI는 미지원 안내) · `@`/`@@` 상대 경로·인자·복원·`.sql` 보완·깊이 32 · 엄격 모드 `script.strict`(HIDDEN) · `SHOW <kind>` · SERVEROUTPUT 폴링은 드라이버에 이미 있음(확인) | — | ✅ |
| **T-10** | P1 | 소 | `cargo-deny` 라이선스 게이트 · `THIRD-PARTY-NOTICES` | T-4 | ☐ |

## 3. M2 — GUI 관통
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-11** | P0 | 중 | 셰이핑 스파이크(DP-6) — `rustybuzz`로 한글 조합·CJK 폭 · `nexa-gfx` 통합 | T-1 | ☐ |
| **T-12** | P0 | 대 | `nexa-edit` Buffer·Transaction·History(soft undo) — 순수 로직·테스트 | — | ☐ |
| **T-13** | P0 | 대 | `nexa-edit` Layout/View/Selections(멀티커서·컬럼) · **IME preedit 오버레이** | T-11 T-12 | ☐ |
| **T-14** | P0 | 중 | `.sublime-keymap` 파서 + context 평가기 + Command 레지스트리 | T-12 | ☐ |
| **T-15** | P0 | 대 | `nexa-grid` 가상화(10만 행 · 컬럼 리사이즈/정렬/고정 · TSV 복사) | nexa-ui U-2 | ☐ |
| **T-16** | P0 | 중 | 창 하나 관통 — ✅ 최소(TextBox·자체 그리드·워커) · **T-16b** ✅ 연결 프로필(`nsql-vault` · 09-13 DR-22) · ✅ 클립보드(09-14 · Ctrl+C/X/V · 우클릭) · ☐ 프로필 목록·삭제 UI([22 §1](22-driver-extensions.md)) · **T-16c** 치환 변수 대화상자 · **T-16d** 세션 hot exit([18](18-session-and-projects.md)) | — | 🚧 |

## 3-1. 편집기 E1~E9([17](17-editor-incremental-plan.md)) · 세션([18](18-session-and-projects.md)) · 외부 변경([15](15-external-file-changes.md)) · 비교/git/오브젝트 캐시([19](19-compare-git-and-object-history.md))
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **E-1** | P0 | 대 | `nexa-edit` E1 Buffer(로프·인덱스·CP949·Transaction·History) | D-9 | ☐ |
| **E-2** | P0 | 중 | E2 Selection · E3 Commands+Keymap(Default 키맵 데이터) | E-1 | ☐ |
| **E-3** | P0 | 대 | E4 Display · E5 멀티커서 → **GUI 재정비 ①** | E-2 T-11 | ☐ |
| **S-1** | P0 | 중 | 세션 hot exit(유휴 2s/30s 원자 저장 · 복원) · 프로젝트/워크스페이스 파일 | T-16 | ☐ |
| **X-1** | P1 | 중 | 외부 변경 감지(워처·해시·fast-forward) · 비모달 배지 | S-1 | ☐ |
| **C-1** | P1 | 중 | 비교 뷰 2-pane(`similar`) · 스크립트 저장 시점 스냅샷 · git gutter | E-3 | ☐ |
| **O-1** | P1 | 대 | 오브젝트 시점 캐시(열기/컴파일 전후) · 복원 명령 | C-1 Catalog | ☐ |
| **X-2** | P2 | 대 | 3-pane 병합 | C-1 X-1 | ☐ |

## 3-2. 드라이버 확장 · 접속 대화상자([22](22-driver-extensions.md) · DR-23·24 · 사용자 09-13) — 순서 고정(프로토콜 → 보관 → 다운로드 → Oracle → GUI)
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-27** | P0 | 대 | **09-15 개정(DR-29)**: in-process cdylib C ABI(`nsql-abi` vtable+코덱 · 드라이버 `cdylib` 타깃 · 지연 `dlopen` 로더 · 외부 crate 0) — 프로세스 호스트는 같은 ABI 위 선택 모드 · 원안: `nsql-driver-rpc` — stdio JSON-RPC v1(프레임·Session 메서드·Value 인코딩) + 프록시 Session + 참조 구현 `nsql-driver-sqlite-rpc` | — | ☐ |
| **T-28** | P0 | 중 | `nsql-ext` — `drivers/<id>/<ver>/` 레이아웃 · `nexa-driver.json` · 최신 선택 규칙 · 참조 카운트 · 삭제/휴지통 · `nsql driver list/rm/prune/path` | T-27 | ☐ |
| **T-29** | P0 | 대 | GitHub 다운로드 — 색인 · 최신 태그 · HTTPS(rustls) · sha256 · Ed25519 · 원자적 설치 · `install/update/search` · 갱신 배지 · `--file` 오프라인 | T-28 D-19 | ☐ |
| **T-30** | P0 | 대 | Oracle OCI 확장 — kubo 드라이버를 RPC 프로세스로 · Instant Client 19/23 SxS · 프로필 `driver=` · ORA-28040 힌트 | T-29 D-20 | ☐ |
| **T-31** | P0 | 대 | GUI — DBeaver식 접속 대화상자 · **Golden식 로그인 리스트 = 그리드(사용자 09-14 · nexa-grid ConnectionGrid · [nexa-ui 21](../../nexa-ui/docs/21-grid-family.md))** · 드라이버 관리자(목록·Download·Update·Delete). 기본 패널은 ✅ 09-14(`connect.rs`) | T-28 · nexa-ui U-3 | 🚧 |
| **T-31b** | P1 | 중 | 접속 창 보강 — Import/Export(프로필 파일 · 비밀번호 제외 기본) · Help/Options · Read Only · Pin 열 · ~~목록 정렬/스크롤~~(✅ 09-14 14차 · 폭 조절·결합 정렬 포함) · ~~삭제 확인~~(✅ 09-14 TimeoutButton 2단) · **삭제 복구**(`.deleted/` 이동 · N일 보관 · 09-14 실수 삭제 사례) · 접속 그리드 = nexa-grid `ConnectionGrid`(G-4) | T-31 F-2 | ☐ |
| **T-62** | P1 | 소 | ✅ 09-16 49차 — Windows `.rc` + `crates/nexa-sql/build.rs`(rc.exe/windres 있을 때만 · 외부 crate 0) · macOS `.icns`(iconutil) · Linux `.desktop` + hicolor 8종 · `make-ico.py` | 릴리스 파이프라인 | ✅ |

## 3-3. 정품 인증 · 기능 게이트([23](23-license-activation.md) · 티어·서버·저장소 분리 [25](25-license-tiers-and-server.md) · 사용자 09-14) — 코드는 D-23·24·32·33 답 뒤 · **라이브러리 저장소(T-45)부터**
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-32** | P1 | 중 | `nsql-license` — 파일 형식 파서·정규화·Ed25519 검증·`Feature`/`check` · 3-OS 기기 ID · 테스트([23 §5](23-license-activation.md)) | D-24 D-26 | ☐ |
| **T-33** | P1 | 소 | `nsql license request/status/install/remove` · 종료 코드 4 | T-32 | ☐ |
| **T-34** | P1 | 중 | GUI 설정 라이선스 탭 · 상태줄 배지 · Denied 대화상자(요청 코드 복사) · i18n `license.*` | T-32 T-37 | ☐ |
| ~~T-35~~ | | | → **T-41**(`nexa-license-tool`은 서버 비공개 저장소의 두 번째 bin · [25 §9-1](25-license-tiers-and-server.md)) | | |
| **T-36** | P1 | 소 | 게이트 배선 — D-23 목록의 진입점 1곳씩 · 확장 manifest `requires` | T-32 D-23 | ☐ |
| **T-45** | P1 | 중 | ★ **`SosomLab/nexa-license` 저장소 생성**(형제 · path 의존) — format(자체 파서)·types(`Product`)·verify(**SigVerifier 포트** · ed25519 feature)·request(`NEXAREQ1`)·machine(계열 공통 기기 코드)·fs(경로 주입)·keys(ed25519·p256 루트)·테스트 픽스처 · 3-OS CI · 문자열 0(열거형만)([25 §9·§10](25-license-tiers-and-server.md)) — T-32의 검증·기기 ID·테스트는 여기로, `nsql-license`는 얇은 층 | D-24 D-26 D-40 | 🚧 09-14(저장소 생성 · 공개 · format/base32/Product 골격 + CI · 8588bc7 push) |
| **T-40** | P1 | 중 | `nexa-license` 확장 — `kind`·`machine.N`·org 키(`seats`·`seat_mode`·`server_key`) · 리스 2단 체인 검증 · `License.source` | T-45 D-32 D-33 | ☐ |
| **T-41** | P1 | 대 | **`SosomLab/nexa-license-server`(비공개)** — `nexa-licensed`(init/install/service · SQLite 좌석·리스·감사 · 최소 HTTP · named/device/concurrent · 관리 CLI+정적 페이지 · 3-OS 서비스) + `nexa-license-tool`(발급기 · 재발급 대장) | T-40 D-36~38 | ☐ |
| **T-42** | P1 | 중 | 클라이언트 리스 — `license.server`·`license.user` · 부팅 갱신·하트비트·release · 오프라인 리스 · 상태줄 배지 · `nsql license server/status/renew` | T-40 T-33 | ☐ |
| **T-44** | P2 | 소 | 관리자 설치 문서([25 §4-6](25-license-tiers-and-server.md)) · 100 동시 부하 · 만료·유예 실기 | T-41 T-42 | ☐ |
| **T-46** | P1 | 소 | 라이선스 백업·복원 — `nsql license export/import` · GUI 백업/복원 · **시작 시 알려진 위치 자동 복원**(사용자 폴더 → 기기 공용 → exe 옆) · Device는 기기 공용 폴더(`%ProgramData%` 등) · 라이선스 ID 복사([25 §11-3](25-license-tiers-and-server.md)) | T-32 | ☐ |

## 3-4. 앱 설정 · i18n · 테마(사용자 09-14)
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-37** | P0 | 중 | `nsql-i18n`(영어 기본 · 한국어) + `nsql-settings`(레지스트리 · `settings.conf`) · `nsql config` · GUI 전 문자열 · ☐ CLI 나머지(run/shell/export/conn) 문자열 카탈로그화 | — | ✅ 09-14 |
| **T-38** | P0 | 소 | 테마 System/Light/Dark(기본 System) — OS 판정 3-OS(`theme.rs`) · `Ctrl/⌘+⇧T` 순환 · `ThemeChanged` 추종 · ☐ mac/Linux 실기 | T-37 | ✅ 09-14(win) |
| **T-39** | P1 | 중 | ✅ 09-15 1차(`prefs_win.rs` · 검색 · 트리 · 카드 · Switch/Combo/TextBox · 색/단축키 보조 버튼 · 초기화 · 고급 · 즉시 반영 `apply_setting`) · 잔여 = 변경 바 애니메이션 · 키맵 카테고리 그리드 · 설정 화면 — VS Code식 좌 TOC + 검색 + 항목 카드 + 변경 바 + Reset([24 §2·§3](24-settings-and-vscode-analysis.md)) · **T-39b** 프로젝트 스코프 `.nexa/settings.conf` | 메뉴바 이식 · T-37 | 🚧 |

## 3-5. 성능 계측·경량 구조([26](26-performance-architecture.md)) · CLI 규약([27](27-cli-conventions.md)) · 사용자 09-14
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-46b** | P0 | 중 | ✅ 접속 흐름(공용 `from_parts`/`test_connection` · CLI `conn add/test` 필드 · GUI 접속 패널) · ✅ 계측 골격(`Stage/Timeline` · `--timing` · 푸터) | — | ✅ 09-14 |
| **T-47** | P1 | 소 | Send 스팬 · MSSQL/SQLite Execute·Fetch 분리 · **파일 로그 싱크**(`LogSink` · 배치 flush · 회전 · `NSQL_LOG`) · `--timing=json` | D-44 | ☐ |
| **T-54** | P1 | 중 | `session.mode` 실체 — 편집기 탭 다중화 시 per-editor 세션(워커 세션 맵 · CONNECT 범위 경고) · **한 탭의 접속 장애가 다른 탭에 번지지 않게(사용자 09-14 14차 · 워커 패닉 격리·빠른 판정은 ✅, 세션 분리는 여기)** · 로그 창 필터/지우기 ✅ 09-16 22차(종류·컬럼 · 우클릭 메뉴) · 검색 ☐ · 로그 창 Grid = nexa-grid(G-3 뒤) | E-3 D-45 | ☐ |
| **T-48** | P0 | 대 | ✅ 09-15 1차 상한 · ✅ 09-16 25차 OFFSET 폴백·도구줄·자동 페치 · **✅ 09-16 49차 a 서버 커서 유지**(`CursorHandle` · SQLite 세션 스레드 · Oracle 소유 ResultSet · PG DECLARE CURSOR(실서버 미검증) · MSSQL 폴백 · 러너 fetch_next/all/count · Navigate 스팬 · 자동 커밋 유예) · **✅ d CLI**(`cli.max_rows` · `\more/\all/\count/\pager` · `cli.auto_more`) · **✅ b 09-17 51차** 전체 조회 스트리밍(`query_stream` + `fetch_all` 배치 진행률 · ■/Esc 취소 · 예산 수신 중 판정 · MSSQL은 일괄) · **c** MSSQL 스트림 · `db.cursor_idle_secs` 실체 · PG 실서버 | 43 D-68~72 | 🚧 |
| **T-49** | P1 | 소 | `DBMS_OUTPUT.GET_LINES` 배열 회수 · 실행 중 주기 폴링 옵션 · UNLIMITED 안내 | — | ☐ |
| **T-50** | P0 | 대 | 컬럼 지향 결과 저장소 + 페인트 할당 0 + 폭 캐시 — **nexa-grid(U-3 · dir2 `rows.rs` 이식 · [nexa-ui 21 그리드 계열](../../nexa-ui/docs/21-grid-family.md))** 와 함께 · 결과 그리드는 속도·메모리 최우선(사용자 09-14) | nexa-ui U-3 | ☐ |
| **T-51** | P1 | 중 | D-41 반영 — 옵션 파서 재정리(별칭·충돌 안내) · `-e` · `-v` · `-b` · `-W`/`NSQL_PASSWORD` · `-S host,port` · 진영별 `--help` 예시 | D-41 | ☐ |
| **T-55** | P0 | 중 | 편집기 탭 — TabBar(nexa-ctl 이식 U-2) 배선 · 탭별 버퍼 · `tabs.rows`(single ◀▶·드래그 / multi) · `tabs.tooltip`(TabInfo 카드 · 구문) · ☐ `session.mode` per-editor 마커(T-54) | U-2 T-54 | ✅ 09-14 |
| **T-57** | P0 | 대 | **인텔리전스 = 메타 저장소 한 벌**([47](47-intellisense-metadata.md) 📐 확정 09-17 · D-79~D-86 · [29 §6](29-editor-syntax-palette-statusbar.md) UI) — ⓪ 설정 `intel.*`/`meta.*` 48키 등록(47 §8 · DBeaver 카테고리 · 종속 잠금) ✅ ① `nsql-run::meta` 저장소(인터닝 · exact 해시 · 버킷 정렬/이진 탐색 · `Arc<Snapshot>` · 디프 · LRU · 09-17 자율 배치 3) ② 카탈로그 벌크 컬럼·코멘트·워터마크(방언 4) ③ 탐색기 = 저장소 뷰 ④ 인텔 워커(문맥·alias·랭킹·예산) ⑤ 팝업 컨트롤 ⑥ hover 카드 ⑦ 디스크 캐시 ⑧ CLI Tab · 시스템 오브젝트 패키지 | T-56 E-2 28 | 🚧 |
| **T-58** | P0 | 대 | **다중 커서·정규식 찾기**([29 §7](29-editor-syntax-palette-statusbar.md)) — 리전 벡터(정렬·병합) · `Ctrl+D`/`Alt+F3`/`Ctrl+⇧L`/컬럼 선택 · 찾기 패널 정규식 → `Alt+Enter` 전부 선택 · `Ctrl+H` `$1` · 뒤→앞 적용 · 트랜잭션 1 = Undo 1 | E-2 T-59 | ☐ |
| ~~T-59~~ | — | — | ~~자체 정규식 엔진(Thompson NFA)~~ → **D-76 `regex` + `fancy-regex`로 대체(✅ 09-16 34차 · `rx.rs`)** · 잔여 = `.sublime-syntax` 컨텍스트 호환 · 강조 행 시작 상태 캐시(큰 파일) | — | ☐ |
| **T-60** | P2 | 소 | 서식 복사 — Linux 다중 형식(자체 Wayland/X11 클립보드 소유) · macOS osascript 실기 · 잘라내기 서식 옵션 | — | ☐ |
| **T-61** | P1 | 소 | 상태줄 — `statusbar.*` 표시 설정 · 세션 모드/Read-only 배지 · 인코딩·줄끝·탭 크기 세그먼트 · 클릭 동작(Goto line · 로그 창) · 좁을 때 왼쪽부터 숨김 | T-39 T-54 | ☐ |
| **T-56** | P0 | 대 | ✅ 09-15 1차(`nsql-catalog` 5방언 · `nsql cat` · DESC · `explorer.rs` 메타 세션 스레드 · 지연 1단계 · 노드별 ⚠ · 소스/SELECT 새 탭 · 우클릭) · 잔여 = 툴팁 카드 · 스레드 풀 ≤4 · 취소 · 자동 갱신 · nexa-grid `CatalogSource` · 필터 상자 · **오브젝트 탐색기**([28](28-object-explorer.md)) — `Catalog` 포트 · Oracle/MSSQL/SQLite `children` · `nsql-run::explorer`(메타 세션 · 스레드 풀 ≤4 · 노드 상태 · 캐시 · 취소 · catch_unwind) · `nsql cat` · 트리 그리드(nexa-grid `CatalogSource`) · 설정 4키(등재 ✅) · 자동 갱신(기본 off) | nexa-ui G-1~3 · D-46 | 🚧 |
| **T-53** | P1 | 중 | ✅ 09-15 1차(`keymap.rs` Sublime win/mac 기본 · `key.*` 20키 · `keys_win.rs` 캡처 창 · 충돌 배지 · Ctrl+Shift+E) · 잔여 = 검색 · 그룹 · **단축키 설정 화면** — KeymapGrid(그룹·이름·설명·단축키 · [지정]/[초기화] · 충돌 배지 · 검색) + `HotkeyCapture` 캡처 대화상자(적용 → 그리드 값·키맵 파일) — [nexa-ui 21 §3-1](../../nexa-ui/docs/21-grid-family.md) · 명령 레지스트리는 E3(T-14) | nexa-ui G-6 · T-14 | 🚧 |
| **T-63** | P2 | 소 | 프로브 부하 원칙(09-14 검토 · [26 §8](26-performance-architecture.md)) — ✅ 대상 집합 밖 항목 재예약 금지 · ✅ 슬롯 초과 깨우기 1s · ✅ Test/Connect 동시 상한+큐(`connect.max_concurrent`) · ☐ 호스트명 DNS 캐시(프로필 수백 개 대비) | 14차 검토 | 🚧 |
| **T-65** | P1 | 중 | **WindowHost** 부품 — 창 3개(접속·로그·색)가 복사한 softbuffer/RasterCtx/입력 변환/틱/페인트 프레임을 한 타입으로(nexa-clip 창 골격 이식) · 창은 `layout/route/paint`만 | [30 §3-A](30-architecture-patterns.md) | ☐ |
| **T-66** | P2 | 소 | 편집기·그리드·로그 창 상수 설정화(`EditorTuning/GridTuning` · 비노출) — 접속 창 `ConnTuning`과 같은 방식 | [30 §3-D](30-architecture-patterns.md) | ☐ |
| **T-67** | P1 | 중 | 🚧 09-15 씨앗 = `keymap::COMMANDS`(id·라벨·기본 코드) · 잔여 = 메뉴·팔레트·툴바가 같은 표를 읽기 · 핸들러 · **Command 레지스트리**(id · 라벨 Msg · 기본 단축키 · 핸들러) — 메뉴·팔레트·툴바·키맵(T-53)이 같은 표를 읽는다 | [30 §3-E](30-architecture-patterns.md) T-53 | 🚧 |
| **T-68** | P2 | 소 | 실행 계층 메시지 규약 문서화(`Cmd/ConnOutcome/RunEvent/TestResult` · 버전 · 큐/스레드 소유권 표) — RPC 드라이버 규약(T-27)과 정렬 | [30 §3-F](30-architecture-patterns.md) | ☐ |
| **T-69** | P1 | 중 | ✅ 09-15 19차 탭별 계층(상태줄 팝업 = 활성 탭 · 붙여넣기 탭→공백 · 탭 폭 렌더 탭마다) · ✅ 09-15 1차 전역 계층(`editor.tab_size`/`indent_spaces` · nexa-gfx 탭 폭 주입 · Tab=공백 · 변환 · 상태줄 세그먼트+팝업) · 잔여 = 문법/프로젝트/탭별 계층 · 감지 · Tab/Shift+Tab 줄 들여쓰기 · **들여쓰기 설정 계층**([31](31-indentation-settings.md)) — 전역 → 문법(`<Syntax>.nexa-settings`) → 프로젝트 → 현재 탭(메모리·세션) · 상태바 `Tab Size: 4`/`Spaces: 4` 세그먼트 + 팝업(공백/탭 · 폭 1~8 · 버퍼에서 감지 · 변환 · 문법/전역 기본으로 저장) · nexa-gfx 탭 폭 주입(4칸 고정 제거) · Tab/Shift+Tab 줄 들여쓰기 | 29 T-61 T-67 | 🚧 |
| **T-70** | P1 | 소 | PostgreSQL TLS(`sslmode=prefer/require` · rustls) · `postgres-rustls` 원장 · 접속 폼 SSL 토글 | nsql-driver-pg | ☐ |
| **T-71** | P1 | 중 | ✅ 09-15 1차(`oracle.live.*` · V$SESSION/로그 테이블 폴링 · 로그 창 `[live]`) · 잔여 = CLI `--live` · Live 세그먼트 · DBMS_PIPE/LONGOPS · **Oracle 라이브 로그 모니터**([32 §2](32-server-messages-and-live-log.md)) — 메타 세션 폴링(로그 테이블 · V$SESSION client_info/action · DBMS_PIPE · LONGOPS) · 로그 창 Live 세그먼트 · `oracle.live.*` 설정 · CLI `--live` · 실행 중에만 1s 이상 | 28 T-56 D-48 | 🚧 |
| **T-72** | P1 | 대 | ✅ 09-16 49차(DR-32 · 보조 에이전트) — `packaging/{macos,windows,linux}` + `lib.sh` · macOS `.app` Universal 2/pkg(postinstall `/usr/local/bin/nsql` 링크)/dmg 실기 생성 · WiX v4 MSI · deb/rpm · `release.yml`(태그 → 3-OS → 설치 스모크 → sha256 → Release 초안) · THIRD-PARTY-NOTICES 초안. 잔여 = 서명 키(DR-20) · winget/choco/brew 매니페스트 · zig glibc 2.17 · arm64 매트릭스 · 동봉 Packages 로더 · T-10 | T-62 T-10 DR-32 | ✅ |
| **T-73** | P1 | 중 | ✅ 09-15 1차(`findbar.rs` · Ctrl+F/H · F3 · Aa · Replace/All · 순환) · ✅ 정규식(D-76 34차) · ✅ 단어 단위 · ✅ 09-16 48차 **일치 전부 표시**(`set_find_marks` 반투명 · 열린 동안 갱신) — 편집기 **찾기/바꾸기** 완료 | T-58 T-59 | ✅ |
| **T-74** | P1 | 중 | ✅ 09-15 20차 1차(Open/Save/Save As · 최근 파일 · 끌어놓기 · `*` 더러움 · 닫기 2단 · CRLF) — 잔여 = 외부 변경 X-1(docs/15) · 프로젝트 폴더 · export 경로 · **파일 열기/저장**(File ▸ Open/Save/Save As · 최근 파일 · 외부 변경 X-1) — nexa-ui 자체 파일 대화상자([nexa-ui 20](../../nexa-ui/docs/20-file-management-and-dialogs.md) F-6) 선행 | nexa-ui 20 | 🚧 |
| **T-75** | P2 | 소 | 그리드 복사 확장 — Copy as JSON/Markdown · 컬럼 헤더 우클릭(컬럼 복사·숨김) · 셀 편집기(nexa-ui 21 §3-2) | 09-15 3차 | ☐ |
| **T-76** | P2 | 소 | 🚧 09-16 1차(builtin = 편집기 탭 + 저장 감시 반영) · 잔여 = **settings.json 내장 편집기**(`settings.json_editor = builtin`) — 편집기 탭에서 열고 저장(T-74 파일 저장) → 같은 감시 경로로 반영 · JSON 구문 강조 패키지 · 스키마 힌트(레지스트리 라벨/허용값) | T-74 T-17 | ☐ |
| **T-77** | P1 | 대 | ✅ 09-16 49차 1차(**공유 세션** · DR-30) — 대기 목록 + 탭 배지 `●n`/`⚠n` · 상태줄 세그먼트+팝업(모드 전환·Commit(n)·Rollback(n)·목록) · 툴바 Commit/Rollback(Material · 활성/색) · 닫기/해제/종료/전환 팝업 · Oracle/MySQL 암묵 커밋 · `tx.stale_min`·`tx.close_action`·`tx.badge`·`tx.smart_commit`. 잔여 = 탭별 세션(T-54) · 프로필 `autocommit=` · `nsql shell` `*n` · 트랜잭션 로그 창 필터 | T-54 T-55 T-61 | 🚧 |
| **T-64** | P2 | 중 | 설정 화면(T-39)에 색 선택기 연동 — `ColorPanel`을 hover/눌림 외 테마 주요 색에도 · `ui.fade_fast/slow` · `probe.*` · `input.scroll_natural` 노출 | T-39 | ☐ |
| **T-52** | P2 | 소 | ✅ 09-16 48차 — 셸 별칭 `\d \dt .tables \dv \d <n> .schema \i .read \c \?`(→ SHOW/DESC/@/CONNECT · `\?` 표 · `shell --help`) · [40 Step 11](40-cli-usage.md) | T-7 | ✅ |

## 4. M3+ (요약)
syntect `.sublime-syntax`(T-17) · 컬러스킴/스니펫/완성(T-18) · `Default` 패키지(T-19) · `Catalog` 포트 + 오브젝트 브라우저(T-20) · import/bulk 6방언(T-21) · 데이터 편집기 변경 SQL 미리보기(T-22) · MySQL/ODBC 드라이버(T-23 · PG ✅ 09-15 `nsql-driver-pg` · SQLite ✅) · WASM 플러그인 API(T-24) · 릴리스 파이프라인·서명(T-25) · 라이선스 키(T-26 → **T-32~36** [23](23-license-activation.md)).
| **T-78** | P1 | 중 | 구문 토큰 종류 확장(자료형·함수·바인드·구분자·명령) + 색 프리셋 `editor.color_preset`(nexa/dbeaver/golden · [35](35-editor-colors-reference.md)) · 선택 반투명 옵션(D-54) | D-53 D-54 | ☐ |
| **T-79** | P1 | 중 | 파일 인코딩 — 열기/저장 대화상자 하단 인코딩·줄끝 콤보(Auto/UTF-8/UTF-8 BOM/UTF-16/EUC-KR·CP949 표 내장) · 탭별 인코딩 기억 · 상태줄 세그먼트 · 3-OS 동일(자체 대화상자) | T-74 | ☐ |
| **T-80** | P2 | 소 | 그리드 SQL 복사의 키 = PK(카탈로그 `nsql-catalog` 연동 · 없으면 첫 컬럼) · 테이블 추정 개선(별칭·스키마) | T-56 | ☐ |
| **T-81** | P1 | 대 | **파일 검색 탭 + 프로젝트**([36](36-find-in-files-and-project.md)) — **✅ 09-16 49차 b `nsql-search` 엔진 + `nsql grep`**(병렬 열거 · 무시 규칙 · SWAR · 스트리밍 · 취소 · search_text · 28 테스트 · 21k파일 0.3~0.6s) · **✅ 09-17 50차 a 패널**(`search_panel.rs` · Ctrl+⇧F · Where · 열린 탭+폴더 스트리밍 · 결과 트리 · 클릭 이동 · 설정 4키) · 잔여 = c 프로젝트 파일 · d 바꾸기 미리보기/적용 · e 캐시·EUC-KR | 22차 활동 막대 · T-74 · T-59 | 🚧 |
| **T-82** | P0 | 소 | 트리 `rows()` 평탄화 캐시([37 P-1](37-file-picker-performance.md)) | — | ✅ 09-15 |
| **T-83** | P0 | 소 | 트리·탐색기 페인트를 첫 가시 행부터([37 P-2](37-file-picker-performance.md)) — 탐색기는 이미 `skip(first)`+행 캐시 · nexa-ctl TreeView/TreeGrid ✅ | — | ✅ 09-15 |
| **T-84** | P1 | 소 | `nexa-fs::entry_of` — 링크일 때만 경로 stat(53× · [37 P-3](37-file-picker-performance.md)) | — | ✅ 09-15 |
| **T-85** | P2 | 중 | 배치 도착마다 전체 재구성 완화([37 P-4](37-file-picker-performance.md)) — ✅ 150ms 간격 · 잔여 = `refresh_grid` 사본 제거(증분 append) | T-82 | 🚧 |
| **T-86** | P2 | 대 | `nexa-ctl/controls/file/*` 추출 — 20 §1 계층 복원([37 P-5](37-file-picker-performance.md)) = F-3 | — | ☐ |
| **T-87** | P2 | 소 | 우클릭 메뉴 항목 앞 아이콘(사용자 09-16 요청 · **대상 메뉴 미확인** — 편집기 탭/상태줄 Tab/접속 창 목록/결과 그리드 중) · 파일 대화상자 메뉴 방식(`CtxItem::with_icon`) | — | ⏳ 사용자 |
| **T-88** | P1 | 소 | **Windows 재검증**(09-16 맥에서 고친 것): 잉크 기준 세로 정렬(`text_center_y`) · 스플리터 · Material 아이콘 · 메뉴 배율 · 툴팁 다중 행 — 맥 세션 후반은 캡처를 못 봐 빌드/테스트로만 확인 | — | ☐ |
| **T-89** | P1 | 소 | ✅ 09-16 **줄끝 정책**([38](38-line-endings.md)) — `eol.rs`(다수결 판정·정규화·정책) · `file.eol_new`/`file.eol_save` · 상태줄 LF/CRLF 세그먼트+팝업 · 줄끝 변경 = 더러움. ✅ 09-16 48차 Edit ▸ 줄끝 3종 메뉴 · 잔여 = 다른 이름으로 저장 콤보(T-79) | — | ✅ |
| **T-90** | P1 | 대 | **자원 거버넌스**([39](39-resource-governance.md) · DR-31) — **✅ 09-16 49차 a** 레지스트리 `perf.mode`·`PERF` 원장 24키·`Settings::effective`·`nsql config list perf`·`nexa-sys`(D-60) · **✅ d** 상한 세터 4(log.max_lines · undo_max · glyph_cache · icon_cache) + main.rs 배선 · **✅ 09-17 52차 `perf.boost` 실행 속도 향상**(39 §4-6 · 강제+잠금 · `ui.menu_icons`/`ui.clipboard_probe`) · 잔여 = 상태줄 ⚡ · 설정 창 Performance 카드 · `nsql --perf` · **b** DB(`db.statement_timeout` cancel 포트 · 탐색기 숨김 = 세션 없음) · **c** CPU/GFX(`editor.highlight_max_kb` · `max_occurrences` · `ui.max_fps` · `ui.animations` · `caret_blink` · `file.os_icons` · `probe_chevrons` 배선) · **e** 진단·게이트 · **f** 후보 | DR-31 | 🚧 |
| **T-91** | P2 | 소 | 편집기 페인트의 O(본문) 항목 — `content_w` 전 줄 측정 · `logical_lines` 사본 · 강조 상태를 첫 가시 행까지 매 프레임 전달 → 변경 시에만 재계산(캐시) · [39 §3-3](39-resource-governance.md) 등재 | — | ☐ |
| **T-93** | P1 | 중 | ✅ 09-16 49차 — `results.rs` `ResultPanel{tabs}`(편집기 탭 쌍 · 활성 그리드만 그림 · swap) · Ctrl+\\ 새 결과 탭 · TabBar 단일 행(auto/always) · 우클릭(닫기/다른/오른쪽/고정/맨 앞·뒤/첫·마지막) · 상한 8 + 자동 정리 · 끄기 · **메모리 예산 D-72** · 설정 5키. 잔여 = 탭 이름 바꾸기 · Export 항목 · 커서 close 연동(T-48a 뒤) | T-48a T-48b | ✅ |
| **T-99** | P2 | 중 | ✅ 09-16 42차 **글리프 오토힌트 근사**(nexa-gfx `set_text_hint` · `ui.text_hint`) · ✅ 44차 잔여(곡선/대각선 피팅) = **Windows GDI 글리프 경로**(`ui.text_gdi` · D-77)로 해소 | 31차 | ✅ |
| **T-100** | P3 | 중 | ✅ 09-17 55차 **macOS CoreText**(nexa-gfx `coretext.rs` · 기본 켬 `OS_DEFAULTS`) · 잔여 = Linux FreeType · **macOS/Linux OS 래스터 경로**(CoreText `CTFontDrawGlyphs`/FreeType 힌팅) — D-77의 Windows GDI와 같은 포트(`Font::set_face_family` + `set_text_gdi`류 전역) · 내장 오토힌트로 충분하면 보류 | 44차 | ☐ |
| **T-102** | P1 | 대 | **최적화 패스**(사용자 09-17 · 전체 개발 뒤): 실행 속도 · 실행 파일 크기(release 프로필 · strip · LTO · 의존 감사) · 라이브러리 정적/동적 분리(33 §1) · 로딩 속도 · 점유 메모리 · 실행 후 회수(잠든 탭·캐시 해제) · **텍스트 보기 창 단위 렌더**(파생 `text_lines` 전체 실체화 → 보이는 블록만) · **결과 저장 컬럼형/아레나**(DR-33 후속 · 셀당 32B+힙 문자열) · `nsql-run::fetch_all` 세그먼트 스트리밍 — 기준 = [26](26-performance-architecture.md)·[39 §2](39-resource-governance.md) · **3-OS 점검**(리눅스 glibc/동적 lib 버전 실행 불가 전례 → 정적 링크·최소 glibc 명시 · Windows CRT · macOS 최소 버전) · 코드 수정 포함 | 26 39 33 | ☐ |
| **T-103** | P1 | 중 | **위키 + 맥 캡처 매뉴얼**(사용자 09-17): ✅ 1차 `examples/demo.sql` · `scripts/mac-capture.sh`(NSQL_HOME 격리 · PID 지정 · 9장) · ✅ 09-17 52차 **Demo 프로필 자동 생성**(내장 스크립트 · 최초 1회 팝업 · 도움말 메뉴 · [21 §5](21-connection-profiles.md)) · 잔여 = 스크립트 보완 3건(클립보드 타이핑 · 창 제목 캡처 · 인자 접속 탐색기 결함 수정) → 재캡처 → `docs/wiki/*.md` 10장 → `scripts/wiki-publish.sh` → wiki push | 40 | 🚧 |
| **T-104** | P2 | 소 | ✅ 09-17 52차 인자 접속 = Connect 버튼 경로(`last_spec` · 탐색기 접속 · `-c`/`--fill`/`--help` · [21 §6](21-connection-profiles.md)) · 잔여 = AppleScript 고속 keystroke 글자 유실 점검 | T-103 | 🚧 |
| **T-105** | P2 | 중 | **툴바 편집 UI**(사용자 09-17 "편집 기능은 할일에만"): 그룹 안 아이콘·구분자의 **표시 여부·순서** 편집(설정 창 카드 또는 우클릭 ▸ 편집…) · 플로팅 창을 도크 위로 끌어 붙이기 · 세로 도크 · 그룹 추가/이름 — 모델은 nexa-ctl `ToolGroup.items` 계층 그대로 · 저장은 `toolbar.layout` 확장 | 52차 도크 | ☐ |
| **T-106** | P1 | 중 | **포터블 배포 모드**(D-78 · 사용자 09-17): exe 옆 `data\` 감지 → `NSQL_HOME` · zip 채널(`packaging/`) · 기기 키/라이선스 기기 ID 정책 · 33 §0 개정 · DR 승격 | D-78 33 23 | ☐ |
| **T-107** | P1 | 중 | ✅ 09-17 52차 — **트랜잭션 로그 창**([44](44-transaction-log.md)): nsql-run `txlog`(TxEntry/TxRecord/TxLog 링·필터·`outcome_of` 표시 시점) · `txlog_win.rs` 모덜리스(검색 · 7열 · Tx 열 = 대기/커밋됨/롤백됨/암묵/자동/끊김 · 스위치 3 · 취소선) · 호스트 `tx_close(outcome)` · `txlog.max_entries`. **잔여(T-107b)**: `tx_pending` 원천을 TxLog로 교체 · ✅ 우클릭(복사·편집기로 · 09-17) · 중지(T-108) 연결 · 서버 롤백 판정(44 §4) | T-108 34 39 | ✅ |
| **T-108** | P1 | 대 | **실행 중 상태·중지**(사용자 09-17): ✅ 09-17 ① `Session::cancel_handle` 포트 + SQLite/PG/Oracle 구현 + 워커 `cancel_run` + 툴바/카드 ■ + "중지됨" 판정 · ✅ 배지 · ✅ 실행 상태 카드. **잔여**: ✅ 편집기 탭 실행 중 표시(▶ · 09-17) · 중지 시점까지 받은 행 표시(행 단위 · `ExecResult.interrupted` · 지금은 결과 비움) · ✅ MSSQL 취소(`mssql.encrypt=login` = Attention 세션 유지 · `required` = 소켓 종료+재접속) · 후속 = tiberius 포크로 TLS 안 Attention · 트랜잭션 로그 중지 결과 연결 · 카드 라이브 단계 이벤트 | 26 43 44 | 🚧 |
| **T-90e** | P2 | 소 | ✅ 09-17 `scripts/bench-boost.ps1` 이식 · 잔여 = · 접속 1개 열린 유휴 · 입력 지연/프레임 시간 · 155k행 전체 조회 뒤 회수 · [45](45-perf-boost-benchmark.md) 갱신 | 39 45 | ☐ |
| **T-110** | P1 | 중 | **미니맵 표시 확장**([46](46-minimap-features.md) · 사용자 09-17): ✅ 1순위(찾기 띠 · 오류 줄 · 뷰포트 always/hover+테두리 · 클릭 center/text · 09-17 자율 배치) → 2순위 = View 토글·키맵 `view.minimap` · 현재 문장 범위 · git 변경 줄 · 수정 줄 · 좁은 창 숨김 → 3순위 = 자동 숨김 페이드 · 배율 · 왼쪽 · 섹션 헤더. 구현 = nexa-ctl `MinimapMark` 한 벌 + `set_minimap_marks(kind, ranges)` | 46 39 | 🚧 |
| **T-112** | **P3(최하위)** | 중 | **실행 대기열**([43 §8](43-fetch-model-and-result-tabs.md) 설계 · 사용자 09-17): 편집기 실행 중 재실행 = 지금은 막힘(✅) → 나중에 큐(실행 1 + 대기 2 · 카드 3장 · 대기 취소 × · 취소 시 뒤 카드 당김 · 끝나면 다음을 **새 결과 탭(Ctrl+\\ 경로)**로 자동 실행 · 일괄 취소는 실행 중 제외 · 대기열 관리 팝업 · 설정 `run.queue_max`/`queue_new_tab`) | T-108 43 44 | ☐ |
| **T-115** | P2 | 소 | **Auto Indent 후속**([49 §5](49-auto-indent.md)): `bracketIndentNextLinePattern` · ✅ 줄 주석 뒤 제외(09-17) · `unIndentedLinePattern`(주석 전용 줄) · 문자열/주석 안 괄호 무시(강조 토큰) · 붙여넣기 재정렬(`reindent`) · `.tmPreferences` 읽기 · 다중 캐럿 증가 규칙 | 49 | ☐ |
| **T-116** | P2 | 소 | **일관성 후속**([43 §9-3](43-fetch-model-and-result-tabs.md)): ✅ `strict_all`(09-17) · 겹침 검사(offset−1 행 비교) 옵션 · CLI `\\more` 같은 규칙 | 43 | ☐ |
| **T-117** | P3 | 소 | 줄 변경 표시 후속(09-17): 기준선 = git HEAD 선택(Sublime `mini_diff: auto`) · 띠 클릭 → 그 줄 되돌리기/디프 팝업 · LCS 상한 초과 시 Myers/patience | 29 | ☐ |
| **T-118** | P1 | 대 | **확장 시스템**([50](50-extension-system.md) · D-87~D-90 결정 뒤): ① 매니저 **✅ 1차 09-17 맥 54차**(`extensions/manager.rs` · 저장소 = 이 저장소 `extensions/` 폴더(index.json + 폴더별 extension.json · README 명세 v1) · 팔레트 Sublime식 8명령 · sha256 검증 · SxS · data 설치/되감기 · 켜기/끄기 · 설정 그룹 Extensions + 분류 숨김 · 원격 = curl) · 잔여 = 서명 인덱스 · Upgrade 명령 · `requires` 자동 설치 · **동적 설정 등록**(extension.json `settings[]` → 레지스트리) · 잠든 탭 쌍 표 해제 · 성능 게이트 50 §11 측정 ② WASM 런타임(`wasmi` · 호스트 API · 능력 승인 · 샘플) ③ 결과 후처리·포매터 훅 ④ (선택) 프로세스 플러그인 + Python 러너 + 3-OS 샌드박스 | 22 09 30 | 📐 |
| **T-119** | P1 | 중 | ✅ 09-17 맥 54차 — **Rainbow Pairs 배선**([51 §11](51-rainbow-brackets.md)): `App.extensions` Registry · `rainbowpair.*` → 전 탭 `set_bracket_opts` · 키맵 Ctrl+Alt+, . [ ] · 편집 메뉴/우클릭 "괄호 이동 ▸" · 팔레트 · 끄기 = 효과 off. 잔여 = 실기 · 29/24 문서 표 · 색 목록 설정 창(색 창 재사용) → WASM export(T-118 ②) | 51 | ✅ |
| **T-120** | P3 | 소 | `scripts/check-3os.sh` 교차 타깃이 이 Windows 기기에서 실패(`ring` 빌드에 `x86_64-linux-gnu-gcc` 필요 · 환경 문제 · 09-17): `cargo-zigbuild` 또는 `cross` 도입 · 아니면 교차 빌드는 CI에만 두고 로컬은 `--quick` | — | ☐ |
| **T-113** | P2 | 중 | **로그 후속**([48 §5](48-logging-architecture.md)): 완전 지연 포맷(`LogRec` 32B · 텍스트는 표시 시점) · ✅ feature `devlog` 컴파일 타임 제거(09-17) · `trace` 수준 생산 지점 · `tx`/`meta` 층 · ✅ 텍스트 보기 변환 시간(09-17) | 48 39 | ☐ |
| **T-114** | P2 | 소 | **Sublime 커서 후속**([29 §7-1](29-editor-syntax-palette-statusbar.md)): ✅ Ctrl+M 괄호 짝 · ✅ Ctrl+Shift+M 괄호 안 확장(09-17) · Ctrl+Shift+Space 스코프 확장 · ✅ Ctrl+↑/↓ 스크롤만(09-17) · mac ⌘↑/↓ · 가운데 버튼 열 선택 | 29 | 🚧 |
| **T-111** | P3 | 소 | 색 설정 알파 통합(09-17): `editor.ruler_alpha`·`editor.whitespace_alpha`를 `#RRGGBBAA`로 흡수(마이그레이션 = 6자리 값 + alpha → 8자리) 뒤 키 제거 · 색 창 키 모드에서 최근 색 공유 확인 | 24 | ☐ |
| **T-109** | P2 | 소 | UI 글꼴 GDI 경로에서 가운뎃점 `·`(U+00B7)이 2×1px·전진 2px로 나옴(설정 창 `·→$`가 `→$`로 보임 · nexa-font 덤프 09-17) — 폴백 face 선택/전진 폭 점검 | nexa-font | ☐ |
| **T-101** | P2 | 소 | ✅ 09-16 48차 확인 — `--overflow` 기본값은 설정에서 읽어 `none`으로 찍힘(이미 수정돼 있었음 · `help.rs` 8행) | — | ✅ |
| **T-98** | P1 | 중 | ✅ 09-16 48차 — nexa-ui `EditCommand` 14종(줄 복제/삭제/합치기/이동 · 주석 토글 · 들여쓰기 ±·여러 줄 Tab · 줄 선택/나누기 · 캐럿 추가 ↑/↓ · 대소문자) + 키맵 2단 코드(`ctrl+k,ctrl+u`) + macOS `control+…` + `Ctrl+G` · 메뉴/팔레트 등재. 잔여 = `Ctrl+K,Ctrl+D` 건너뛰기 · `Ctrl+U` 소프트 되돌리기 · 캡처 창의 2단 코드 입력 | T-58 [29](29-editor-syntax-palette-statusbar.md) | ✅ |
| **T-97** | P2 | 중 | ✅ 09-16 49차 — nexa-ui `TextBox::set_minimap`(줄당 2px · 토큰 색 · 뷰포트 상자 · 창 한정 캐시 비트맵 · 클릭/드래그 · 테스트 5) + `editor.minimap`(off)/`editor.minimap_width`(80). 한계 = wrap 모드는 소프트 행 기준 | 32차 | ✅ |
| **T-96** | P2 | 소 | ✅ 09-16 48차 — `Ctrl+P`/⌘P Goto Anything(팔레트 재사용 · 열린 탭 ✓/`*`/경로 + 최근 파일 · 퍼지 · `:숫자` 줄 이동 · Tabs ▸ 탭 찾기…). 잔여 = 접속 표시 · (C) 탭바 ▾ 넘침 드롭다운 | 팔레트 | ✅ |
| **T-95** | P2 | 중 | 로그 창 본문 = **읽기 전용 편집기 뷰**(nexa-ctl 편집기 부품 공유) → Sublime **컬럼(블록) 선택**·다중 커서·찾기를 편집기와 같은 부품으로 · 지금의 자체 선택 코드 제거(사용자 09-16 24차) | T-57 [29](29-editor-syntax-palette-statusbar.md) | ☐ |
| **T-94** | P2 | 대 | **데이터 편집기**(DBeaver 결과 하단 행 추가/삭제/복제 · 셀 편집 · Save/Cancel) — 문장 생성 = [41](41-sql-copy-key-rules.md) 키 규칙 · 트랜잭션 [34](34-transaction-ux.md) · 별도 설계 문서 먼저 | T-93 41 34 | ☐ |
| **T-92** | P2 | 중 | DBeaver식 **가상 키**(테이블별 사용자 지정 키 · 프로필 저장 · [41 D-67](41-sql-copy-key-rules.md)) — 앞 3컬럼 경고가 잦으면 | — | ☐ |

