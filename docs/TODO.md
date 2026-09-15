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
| **T-7** | P1 | 중 | `nsql shell` — ✅ 최소(줄 누적 실행) · ☐ 줄 편집·히스토리·자동완성 | — | 🚧 |
| **T-8** | P1 | 중 | `nsql-io` export — ✅ 6형식 · ☐ 스트리밍(행 단위 fetch) · xlsx/parquet | — | 🚧 |
| **T-9** | P1 | 소 | 엔진 보강 — `SET SERVEROUTPUT` 폴링 Action · `SPOOL` · `@`/`@@` 로드(호스트) · 엄격 모드(암묵 변수 금지) | — | ☐ |
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
| **T-62** | P1 | 소 | 아이콘 배포 — Windows 실행 파일 리소스(.rc · 외부 crate 0 = `rc.exe`/`windres` 빌드 단계) · NSIS · macOS `.app`/`.icns` · Linux `.desktop` + hicolor PNG(clip `install_launcher` 이식) | 릴리스 파이프라인 | ☐ |

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
| **T-54** | P1 | 중 | `session.mode` 실체 — 편집기 탭 다중화 시 per-editor 세션(워커 세션 맵 · CONNECT 범위 경고) · **한 탭의 접속 장애가 다른 탭에 번지지 않게(사용자 09-14 14차 · 워커 패닉 격리·빠른 판정은 ✅, 세션 분리는 여기)** · 로그 창 필터/검색/지우기 · 로그 창 Grid = nexa-grid(G-3 뒤) | E-3 D-45 | ☐ |
| **T-48** | P0 | 대 | ✅ 09-15 1차 상한 = `grid.max_rows` 200(Oracle/SQLite 조기 중단 · `RunEvent::ResultSet.more` · `--max-rows`) · 잔여 = 페치 모델 — 상한(D-42)·배치 스트리밍·"더 가져오기"(Navigate · D-43) · 예산 경고 · 실행 히스토리 패널 · 메타만(EXPLAIN/COUNT) | D-42 D-43 | 🚧 |
| **T-49** | P1 | 소 | `DBMS_OUTPUT.GET_LINES` 배열 회수 · 실행 중 주기 폴링 옵션 · UNLIMITED 안내 | — | ☐ |
| **T-50** | P0 | 대 | 컬럼 지향 결과 저장소 + 페인트 할당 0 + 폭 캐시 — **nexa-grid(U-3 · dir2 `rows.rs` 이식 · [nexa-ui 21 그리드 계열](../../nexa-ui/docs/21-grid-family.md))** 와 함께 · 결과 그리드는 속도·메모리 최우선(사용자 09-14) | nexa-ui U-3 | ☐ |
| **T-51** | P1 | 중 | D-41 반영 — 옵션 파서 재정리(별칭·충돌 안내) · `-e` · `-v` · `-b` · `-W`/`NSQL_PASSWORD` · `-S host,port` · 진영별 `--help` 예시 | D-41 | ☐ |
| **T-55** | P0 | 중 | 편집기 탭 — TabBar(nexa-ctl 이식 U-2) 배선 · 탭별 버퍼 · `tabs.rows`(single ◀▶·드래그 / multi) · `tabs.tooltip`(TabInfo 카드 · 구문) · ☐ `session.mode` per-editor 마커(T-54) | U-2 T-54 | ✅ 09-14 |
| **T-57** | P0 | 대 | **인텔리전스**([29 §6](29-editor-syntax-palette-statusbar.md)) — 끄면 훅 0 · `intel` 워커(세대 번호·디바운스 120ms·예산 30ms) · **alias→컬럼**(캐럿 문장 스캔 · Catalog 캐시) · 방언별 시스템 오브젝트/내장 함수 데이터 패키지(`Packages/Intel/*.nexa-intel`) · 시그니처 힌트 · 팝업 ≤12행 · `TokenKind::Builtin` · `nsql shell` Tab | T-56 E-2 | ☐ |
| **T-58** | P0 | 대 | **다중 커서·정규식 찾기**([29 §7](29-editor-syntax-palette-statusbar.md)) — 리전 벡터(정렬·병합) · `Ctrl+D`/`Alt+F3`/`Ctrl+⇧L`/컬럼 선택 · 찾기 패널 정규식 → `Alt+Enter` 전부 선택 · `Ctrl+H` `$1` · 뒤→앞 적용 · 트랜잭션 1 = Undo 1 | E-2 T-59 | ☐ |
| **T-59** | P1 | 중 | 자체 정규식 엔진(Thompson NFA · 캡처 · `\b` · 옵션 · 외부 crate 0) → 찾기 · `.sublime-syntax` 컨텍스트 호환 · 강조 **행 시작 상태 캐시**(큰 파일) | — | ☐ |
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
| **T-72** | P1 | 대 | **배포 파이프라인**([33](33-distribution-and-packaging.md) · DR-27) — `packaging/{windows,macos,linux}` 스테이징 · MSI/pkg+dmg(Universal 2 · Frameworks)/deb·rpm · `release.yml` 3-OS · sha256 · `--smoke`·제거 검증 · 서명 자리 | T-62 T-10 D-49 D-50 | ☐ |
| **T-73** | P1 | 중 | ✅ 09-15 1차(`findbar.rs` · Ctrl+F/H · F3 · Aa · Replace/All · 순환) · 잔여 = 정규식(T-59) · 전체 일치 하이라이트 · 단어 단위 · 편집기 **찾기/바꾸기**(Ctrl+F/H · 대소문자·정규식 T-59·전체 바꾸기 · 결과 하이라이트) — nexa-ctl TextBox 검색 API | T-58 T-59 | 🚧 |
| **T-74** | P1 | 중 | ✅ 09-15 20차 1차(Open/Save/Save As · 최근 파일 · 끌어놓기 · `*` 더러움 · 닫기 2단 · CRLF) — 잔여 = 외부 변경 X-1(docs/15) · 프로젝트 폴더 · export 경로 · **파일 열기/저장**(File ▸ Open/Save/Save As · 최근 파일 · 외부 변경 X-1) — nexa-ui 자체 파일 대화상자([nexa-ui 20](../../nexa-ui/docs/20-file-management-and-dialogs.md) F-6) 선행 | nexa-ui 20 | 🚧 |
| **T-75** | P2 | 소 | 그리드 복사 확장 — Copy as JSON/Markdown · 컬럼 헤더 우클릭(컬럼 복사·숨김) · 셀 편집기(nexa-ui 21 §3-2) | 09-15 3차 | ☐ |
| **T-76** | P2 | 소 | **settings.json 내장 편집기**(`settings.json_editor = builtin`) — 편집기 탭에서 열고 저장(T-74 파일 저장) → 같은 감시 경로로 반영 · JSON 구문 강조 패키지 · 스키마 힌트(레지스트리 라벨/허용값) | T-74 T-17 | ☐ |
| **T-77** | P1 | 대 | **트랜잭션 UX**([34](34-transaction-ux.md) · D-51) — 탭별 `TxState` · 탭 배지 `●n`(경고→오래되면 빨강) · 상태줄 세그먼트+팝업(모드 전환·Commit·Rollback·대기 목록) · 툴바 Commit 배지/활성 · 닫기/해제/전환/종료 모달 · 방언별 암묵 커밋 · `tx.*` 설정 5키 · `nsql shell` `*n` | T-54 T-55 T-61 | 📐 |
| **T-64** | P2 | 중 | 설정 화면(T-39)에 색 선택기 연동 — `ColorPanel`을 hover/눌림 외 테마 주요 색에도 · `ui.fade_fast/slow` · `probe.*` · `input.scroll_natural` 노출 | T-39 | ☐ |
| **T-52** | P2 | 소 | 셸 명령 대응표(`\d` · `:r` · `.tables` → `DESC` · `@` · `SHOW TABLES`) | T-7 | ☐ |

## 4. M3+ (요약)
syntect `.sublime-syntax`(T-17) · 컬러스킴/스니펫/완성(T-18) · `Default` 패키지(T-19) · `Catalog` 포트 + 오브젝트 브라우저(T-20) · import/bulk 6방언(T-21) · 데이터 편집기 변경 SQL 미리보기(T-22) · MySQL/ODBC 드라이버(T-23 · PG ✅ 09-15 `nsql-driver-pg` · SQLite ✅) · WASM 플러그인 API(T-24) · 릴리스 파이프라인·서명(T-25) · 라이선스 키(T-26 → **T-32~36** [23](23-license-activation.md)).
| **T-78** | P1 | 중 | 구문 토큰 종류 확장(자료형·함수·바인드·구분자·명령) + 색 프리셋 `editor.color_preset`(nexa/dbeaver/golden · [35](35-editor-colors-reference.md)) · 선택 반투명 옵션(D-54) | D-53 D-54 | ☐ |
| **T-79** | P1 | 중 | 파일 인코딩 — 열기/저장 대화상자 하단 인코딩·줄끝 콤보(Auto/UTF-8/UTF-8 BOM/UTF-16/EUC-KR·CP949 표 내장) · 탭별 인코딩 기억 · 상태줄 세그먼트 · 3-OS 동일(자체 대화상자) | T-74 | ☐ |
| **T-80** | P2 | 소 | 그리드 SQL 복사의 키 = PK(카탈로그 `nsql-catalog` 연동 · 없으면 첫 컬럼) · 테이블 추정 개선(별칭·스키마) | T-56 | ☐ |

