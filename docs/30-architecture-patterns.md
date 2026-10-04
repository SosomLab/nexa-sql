# 30. 아키텍처 패턴 원장 — 확장점(IoC/주입) · 재사용 부품 · 관리 범위 (2026-09-15)

> **상태**: ✅ 원칙 확정(사용자 09-14~15) · 🚧 적용 로드맵(T-65~T-68). 상위 = [01 아키텍처](01-architecture.md)(4계층 · DR-7) · [22 드라이버 확장](22-driver-extensions.md)(DR-23·24) · [09 편집기·패키지](09-editor-and-packages.md)(DR-5·14). 이 문서는 **"어떤 기법을 어디에 쓰는가"의 원장**이다 — 새 기능은 여기 등재된 확장점·부품을 먼저 찾고, 없으면 여기에 추가한 뒤 만든다.

## 0. 사용자 요구(원문 요약 · 09-14~15)

1. 디자인 패턴의 효용을 최대화 — **일관성·효율성·관리성·재사용성**을 올리고, **관리되는 범위 안에서** 개발한다.
2. 플러그인·DB 드라이버처럼 뒤에 바뀌거나 버전이 갈리는 기능은 **IoC/주입(Injection)** 으로 **동적 배치** — 호환성·확장성·안정성.
3. 새 기법·기술·서비스가 나와도 **간단한 설정 + I/O 규약 준수**만으로 확장되게.
4. 구현 값은 하드코딩하지 말고 설정으로 — 자주 바꾸지 않는 값은 **비노출 설정**.
5. 네트워크를 만드는 코드는 상한·큐가 있고 원장([26 §8](26-performance-architecture.md))으로 관리.

## 1. 확장점 모델 — "포트 + 레지스트리 + 설정 선택"

모든 확장점은 같은 세 조각으로 만든다. 조각이 하나라도 빠지면 확장점이 아니라 하드코딩이다.

| 조각 | 뜻 | 규칙 |
|---|---|---|
| **포트(trait)** | 허브(`nsql-core`)가 정의하는 I/O 규약 · 구현체는 규약만 안다 | 포트는 허브에만 · 구현 crate는 포트만 의존(DR-7) · 규약 버전을 갖는다 |
| **레지스트리** | 구현체를 **이름/키로 찾는 표** · 부팅 시 또는 요청 시 조립 | 한 확장점에 레지스트리 하나 · 조회 실패는 명시 오류(패닉 금지) |
| **설정 선택** | 어떤 구현을 쓸지는 **설정/프로필/파일**이 고른다 · 코드는 고르지 않는다 | 키는 `nsql-settings::REGISTRY`에만 · 기본값은 설정에 · 비노출 가능 |

주입 방식은 Rust답게 **생성자 주입**(`Box<dyn Port>` · `Opener` 클로저)과 **레지스트리 조회**로 한다 — 전역 컨테이너·리플렉션은 쓰지 않는다. 동적 배치가 필요한 곳(드라이버·플러그인)은 **프로세스/WASM 경계**로 격리해 버전을 나란히 둔다(SxS · [22 §4-2](22-driver-extensions.md)).

### 1-1. 확장점 현황

| 확장점 | 포트(규약) | 레지스트리 | 설정 선택 | 동적 배치 | 상태 |
|---|---|---|---|---|---|
| **DB 드라이버** | `nsql_core::Session`·`Catalog` · `Opener` 클로저 | `nsql-drivers::open`(방언 → 어댑터 · feature) | 접속 문자열 `scheme://` · 프로필 `dialect` · `NSQL_ORACLE_CLIENT_DIR` | 내장 3종 · **RPC 드라이버**(stdio JSON-RPC 프로세스 · GitHub Releases · SxS) = T-27~30 | 내장 ✅ · RPC 📐 |
| **편집기 구문/패키지** | `.nexa-syntax`(Sublime 형식) · `Highlighter` trait | `syntax.rs` 레지스트리(확장자 → 규격 · `Packages/*/` 스캔·덮어쓰기) | `Set Syntax` 팔레트 · 확장자 기본 | 파일 드롭인(재시작 없이 스캔) · WASM 플러그인(DR-14) 📐 | 파일 ✅ · WASM 📐 |
| **로그 표현** | `nsql_log::LogFormat` trait | `nsql_log::formatter(name)` | `log.format` | 내장 3종 | ✅ |
| **접속 프로필 저장소** | `Vault`(봉투 형식 · 기기 키) | 단일(사용자 폴더) | `NSQL_HOME` | — | ✅ · 원격/팀 저장소는 [25](25-license-tiers-and-server.md) |
| **UI 컨트롤·토큰** | nexa-ctl `Control/Widget` trait · `tokens::*` 전역 | 컨트롤은 타입 · 토큰은 setter | `ui.*`(hover 색·페이드·스크롤바 지연) | 라이브러리(path 의존) | ✅ |
| **라이선스** | `nexa-license::check(Feature)` 순수 함수 | 기능 게이트 1곳/진입점 | 라이선스 파일 | 서명 알고리즘 포트(ed25519/p256 · D-40) | 📐 T-32 |
| **명령/단축키** | 팔레트 명령 id 문자열 · 메뉴 정의 | `open_palette` 목록 · 메뉴 트리 | 단축키 설정(T-53) | — | 명령 ✅ · 키맵 📐 |
| **설정** | `SettingKind` · `Entry`(키·범위·기본·i18n) | `nsql-settings::REGISTRY`(단일 원천) · `HIDDEN` | `settings.conf` · `nsql config` · 설정 화면(T-39) | — | ✅ |

### 1-2. 새 확장점을 만들 때의 체크리스트
1. 포트가 허브에 있고, 구현 crate가 허브만 의존하는가(역방향 의존 금지).
2. 레지스트리 하나 · 이름/키가 문서화됐는가 · 조회 실패 경로가 있는가.
3. 선택은 설정/프로필이 하는가 · 기본값이 레지스트리에 있는가 · 비노출이 맞는가.
4. 버전이 갈릴 수 있는가 → SxS 보관 · 규약 버전 필드 · 구버전 공존.
5. 실패가 앱 전체로 번지지 않는가 → 프로세스/WASM/스레드 경계 · `catch_unwind` · 오류 비확산([28](28-object-explorer.md)).
6. 네트워크를 만들면 [26 §8](26-performance-architecture.md) 표에 상한을 등재했는가.
7. 자원(스레드·소켓·디스크·프레임·캐시)을 쓰면 [39 §3](39-resource-governance.md) 부하원으로 등재하고 설정 키(`Entry.perf`)를 붙였는가 · 39 §2 기준 어느 행에 걸리는가.

## 2. 재사용 부품 원장 — 한 번 만든 기법은 여기 등재하고 다시 쓴다

| 부품 | 위치 | 문제 → 기법 | 쓰는 곳 |
|---|---|---|---|
| **점-도형 판정 `shape`**(09-28 · docs/93) | nexa-ctl `shape.rs` | 아이콘 래스터의 "점이 도형 안인가": `seg_dist`·`stroke`·`rect`·`rrect`·`disc`·`ring`·`ellipse`·`tri`·`poly`·`polys_evenodd/nonzero` — 세 곳에 복사돼 있던 식을 한 벌로 | nexa-ctl 글리프 · nexa-sql 탐색기 아이콘(`exp_icons`) · 툴바 아이콘(`toolicons`) |
| **한 크기 글꼴 설정 `FontPrefs::with_base(_status)`**(09-28) | nexa-ctl `theme.rs` | 창마다 `SlotFont{size,bold:false,italic:false}`를 손으로 조립하던 것 → 생성 함수(Factory) · `SlotFont::plain` | nexa-sql 보조 창·패널 27곳 |
| **보조 창 입력 변환 `input::text_key_event`**(09-28) | nexa-sql `input.rs` | winit 키 → 컨트롤 입력(Adapter) · 묶음 `TextKeys::{Line, Multi, MultiTab}` · 판정 본체 `text_key_of`(순수 · 시험) | 입력·라이선스·트랜잭션 로그·변수·가져오기·SQL 미리보기 창 |
| **프레임 버퍼 `Presenter::frame(size)`**(09-28) | nexa-sql `present.rs` | 크기 0 거르기 → resize → buffer_mut 상용구를 한 호출로(`None` = 이번 프레임 건너뜀) | 보조 창 11곳 |
| **보조 창 가운데 배치 `wingeom::centered_over`**(09-28) | nexa-sql `wingeom.rs` | 주인 창 위 가로 가운데 · 세로 1/k 지점(주인 배율로 폭 환산) | About·가져오기·입력·라이선스·SQL 미리보기 창 |
| **App 기능 모듈 `app/`**(09-28 · docs/93 §5) | nexa-sql `app/*.rs` | 거대 객체 분할: 상태 = `App` 한 곳(main.rs) · 동작 = 기능 파일 26 · 입력 라우팅 = 책임 연쇄 고리 메서드 `route_<패널>` · 창 사건 = `aux_window_event` 분배 | 새 동작은 해당 기능 파일에 |
| **플래시 메시지 `Flash`**(09-27) | nexa-ctl `controls/flash.rs` | 클릭 복사 "복사됨" 같은 짧은 알림: `show(text, tone, hold_ms, fade_ms)` → 앵커(가리면 안 되는 사각형)의 **우상단이 메시지의 좌하단**(오른쪽 모자라면 왼쪽으로 · 위 모자라면 아래 · 호스트 안 · 앵커 안 덮음) · 배경 상자(색조 + 테두리) · **유지 시간 뒤 배경색으로 녹아 사라짐**(제곱 감속) · 창 맨 마지막에 그려 최상위 Z-order · 타이머·큐 0 = `paint`가 `true`인 동안 호스트가 다시 그림 · 시간 = 설정 `ui.flash_hold_ms`(2000) · `ui.flash_ms`(페이드 3000) | 라이선스 창 이메일 링크(T-34) · 앞으로 링크·복사 버튼 전부 |
| **gridedit**(편집 가능한 그리드 핵심 · 09-26) | nexa-ctl `gridedit` | 어느 그리드든 편집: 셀 명세 검증(`CellSpec`) · 변경 집합 덧그리기+되돌리기(`ChangeSet` · 원본 불변) · 붙여넣기 행렬/자동 확장(`paste`) · 날짜 24형식(`datetime`) · 살아 있는 편집기 1개(`LiveEditor`) · 키맵 — 값은 `Option<String>` · DBMS·그리기 의존 0 | nexa-sql 결과 그리드(87) · 후보 = 설정 표·접속 목록·파일 이름 바꾸기 |
| **TypeAhead + hangul::Composer** | nexa-ctl `typeahead` · `hangul` | 목록/트리 앞글자 점프(nexa-beep 이식): 버퍼+조합+타임아웃은 부품 · 매칭은 라벨 함수 · 필터·HUD 위치 설정 · Windows 한/영은 호스트가 토글해 `jamo_from_qwerty` | 오브젝트 탐색기(09-19) · (후보) 파일 대화상자 트리 · 설정 창 트리 · 팔레트 |
| **contrast_order** | nexa-ctl `theme` | 순환 팔레트를 이웃끼리 가장 잘 구별되게 배열(보색·색 온도·밝기 · 첫 색 고정) — 깊이 색·차트 계열색처럼 "차례로 쓰는 색 목록"에 재사용 · `Theme.rainbow`는 이미 이 순서 | Rainbow Pairs(`ext.rainbow_pairs.contrast_order`) |
| **merge3 + replace_all_undoable** | nexa-ctl `merge3` · `TextBox` | 줄 단위 3-way 병합(의존 0 · 겹침 수·가져온 줄 범위) + 본문 전체를 되돌리기 한 단계로 바꾸기(캐럿 유지) — 외부 변경 반영 · 포매터 결과 적용 같은 "밖에서 온 새 본문"에 재사용 | 외부 파일 변경(58) |
| **StatWatch / FileSig** | nexa-fs `watch` | OS 와처 없는 파일 변경 감지(서명 비교 → 안정 대기 → 내용 해시) · 전용 스레드 · 요청 합침 — 언제 확인할지는 호스트가 정한다 | 외부 파일 변경(58) |
| **memtrim** | nexa-sql `memtrim.rs` | 힙 → OS 정리(3-OS) + 사용량 읽기 — "큰 것을 놓은 뒤 1회 + 유휴 주기" 정책은 호스트 · 워킹셋 트림은 하지 않는다 | 메모리 회수(59 §2) |
| **PreparedText + set_prepared** | nexa-ctl `TextBox` | 큰 본문을 **UI 스레드 밖에서** 준비(글자 버퍼 + 문자열 + 줄 표 · `Send`)해 복사 없이 옮겨 넣기 — 적재·재로드·생성된 큰 글 | 큰 파일 적재(`fileload.rs`) |
| **TextBuf**(UTF-8 갭 버퍼 + 줄 표 + 변경 기록) | nexa-ctl `edit::TextBuf` · `CharSeq` | 글자 인덱스로 말하는 큰 본문 버퍼 — 글자당 1~3 B · 편집 O(편집) · 줄 ↔ 글자 O(log) · `LineChange`로 줄별 캐시를 바뀐 줄만 갱신 · `find`/`eq_str`/`content_hash`는 본문을 문자열로 만들지 않는다 → [59 §6](59-large-file-handling.md) | 편집기 · (후보) 로그 창 본문 |
| **line_edits**(최소 줄 편집) | nexa-ctl `merge3` | 두 본문 → 바뀐 줄들만의 편집 목록(글자 좌표 · 오름차순) — 재로드·서식 정리를 "바뀐 줄만" 기록 | 외부 변경 반영 |
| **되돌리기 기록 파일**(`export_history`/`import_history` + `undofile`) | nexa-ctl `EditState` · nexa-sql `undofile.rs` | 저장할 때 쓰고 · 열 때 본문 길이·해시가 같으면 복원 · 어긋나면 통째로 버림 · 폴더는 인자로(테스트가 실제 설정을 안 건드린다) | 파일 탭 |
| **연산 기록 되돌리기**(`replace_many` · 묶음 · 저장 지점 · 예산 · 읽기 전용) | nexa-ctl `EditState` | 되돌리기 = 역연산 맞바꾸기(저장 = 지운 글자만) · 여러 곳 = 한 단계·한 번 훑기 · 더러움 O(1) · 바이트 예산 · 단일 변경 통로 → [60](60-undo-redo-redesign.md) | 찾아 바꾸기 · 외부 변경 반영 · 큰 파일 탭 |
| **fileload**(자리 탭 · 진행 막 · 취소) | nexa-sql `fileload.rs` + `Editors::begin_load_tab`/`fill_loaded` | "오래 걸리는 일을 **탭 하나에 가두고** 나머지는 그대로" — 진행 상태 = 원자값 · 지연 표시 · 100% 프레임 뒤 교체 · 명령 문지기는 순수 함수 | 큰 파일 열기 · (후보) 큰 결과 내보내기 미리보기 |
| **세대 + 줄 해시 캐시** | nexa-ctl `TextBox`(`MlTextCache`·`RowWidthCache`·`HlStateCache`) | 본문에서 파생되는 줄 단위 산출물은 (세대, 행 해시)로 묶어 **바뀐 행만** 다시 계산 — 새 줄 단위 기능(진단 표시 · 폴딩)을 넣을 때 같은 틀로 | 큰 파일 성능(59 §1) |
| **SoftStep 선택 되돌리기**(09-22) | nexa-ctl `EditState` | 선택·캐럿 변화를 되돌리기 줄에 한 단계로(스냅샷 · 편집 표식 · Shift 연속 합침 · 상한 500) — Sublime soft undo/redo · 새 선택 변경 API는 `note_sel`을 부른다 | journal 09-22 §65 |
| **win-func-check**(09-22) | nexa-sql `scripts/win-func-check.ps1` | 기능 점검 자동화 = 시나리오 표(격리 설정 · 기동 명령 · 기대) → 앱 기동 · 전 창 캡처 · 생존/패닉 자동 판정 · 캡처 판독 — 새 기능마다 시나리오 한 줄(키 입력이 필요한 것은 명령 id 직접 호출 또는 사용자 실기) | journal 09-22 §62 |
| **nsql-bookmarks 코어**(09-22) | `crates/nsql-bookmarks` | 문서 안 자리의 모델·줄 보정·재탐색(Dice)·JSON — 본문을 `&[&str]`로 받는 순수 함수(GUI·CLI 공용 · 시계·파일은 호출자) | 69 · journal 09-22 §70 |
| **is_outside_click**(09-22) | nexa-ctl `ContextMenu` | 열린 팝업의 바깥 좌/우 클릭 판정 — `on_event`는 바깥 클릭을 닫고 **소비**로 보고하므로 컨테이너는 이 판정을 `on_event` 전에 재 두고 `consumed && !outside`로 "닫고 통과"를 구현한다(편집기 탭·결과 탭·탐색기·그리드 공통 · 새 컨텍스트 메뉴 컨테이너도 같은 꼴) | journal 09-22 §58 |
| **MenuEntry::Sub**(09-22) | nexa-ctl `MenuBar`(pulldown) | 풀다운 하위 메뉴 한 단계(`›` · hover/→/Enter 펼침 · ←/Esc 접힘 · 표면 밖이면 왼쪽 뒤집기 → `nudge_into`) — 메뉴가 길어지면 항목을 늘리지 말고 그룹으로 접는다(Edit 메뉴 6그룹) | journal 09-22 §60 |
| **fallback_file_icon**(09-22) | nexa-ctl `controls` | 셸 아이콘이 없거나 아직 안 온 파일/폴더의 16px 자체 그림 — nexa-dlg 파일 대화상자 + nexa-sql 프로젝트 탐색기(둘째 사용처에서 승격). OS 아이콘은 nexa-fs `IconService` |
| **TreeGrid 선택 없음**(09-22) | nexa-ctl `TreeGrid` | `clear_selection`/`has_selection` — 빈 곳 클릭 = 강조 0 · 캐럿 행은 남겨 키 이동 기준 · 파일 대화상자 세 모드 공통 |
| **FilterBar**(09-22) | nexa-sql `filterbar.rs` | 필터 틀 = 텍스트박스 + 안쪽 토글 Aa·ab·(.*)(+Path) + 오른쪽 부가 토글 · 매칭(`matches`) · 위·아래 여백 상수 `GAP_Y` — 프로젝트·북마크·확장 패널(검색 패널은 자체 배치 · 토글 부품 공용) |
| **SearchHistory + Recall**(09-23 · journal §119) | nexa-sql `search_history.rs` | 검색 상자별 최근 검색어 — `SearchHistory`(상자 이름 → 최근순 · 중복 없음 · 상한 `search.history_max` · 전역 한 파일 `search-history.json` · nsql-settings `json::dump/parse`) + `Recall`(상자 하나의 ↑/↓ 되부르기 · 끝에서 치던 글 복귀 · `TextBox::text_rev`로 타이핑 감지) · `SharedHistory = Rc<RefCell<…>>` 하나를 앱이 만들어 상자를 가진 곳(찾기 · 파일 검색 · FilterBar 4곳 · 설정 창)에 건넨다 — 검색어를 받는 상자를 새로 만들면 `Recall::new("<이름>")` + `on_key`/`commit` 두 줄로 잇는다 |
| **ParWalk**(09-23 · journal §120) | nexa-sql `parwalk.rs` | 병렬 폴더 열거 — 코디네이터 1 + 워커 N(`project.scan_threads`) · 폴더 단위 작업 큐(`Mutex<VecDeque>+Condvar` · 비고 일하는 워커 0 = 종료) · `nexa_fs::list_opts` 규칙 그대로 · **채널 하나**(도착 순 = 병합 로그 · 부모 메시지 먼저) · 취소 = 플래그/수신자 버림 · 수신 쪽은 틱마다 시간 예산으로 합친다(프로젝트 탐색기 필터) — 둘째 사용처가 생기면 nsql-search `walk`(무시 규칙 있는 쪽)와 합칠 후보 |
| **CopyBtn**(09-23) | nexa-sql `copybtn.rs` | 복사 버튼 = 상자 + 아이콘 · `press(now)` → 눌림(120 ms · 안으로) → 체크(`mi_check` · 녹색 `theme.ok`) → `ui.copy_feedback_ms` 뒤 복귀 · `next_tick(now)`로 담는 쪽 tick에 얹힘(자체 타이머 0) — 실행 카드 · 접속 창 파일 이름 복사(복사 기능을 가진 버튼은 전부 이 부품) |
| **dbms_icons**(09-22 · 10-04 PNG 교체) | nexa-sql `dbms_icons.rs` + `build.rs` + `assets/dbms/{light,dark}/{64,20}/<id>.png`(173종 · 고지 = `assets/dbms/NOTICE.md`) | DBMS 아이콘 표 — `build.rs`가 폴더를 훑어 `OUT_DIR/dbms_icons_table.rs`(id 정렬 · `include_bytes` · 네 칸 하나라도 없으면 빌드 실패) · `pick(방언, 힌트) -> id`(힌트 → 방언 → ODBC·미접속이면 5자 이상 낱말 = id → odbc/generic · 옛 이름 이주) · `image(id, dark, Src::Large\|Small, px)` = PNG 지연 디코드(스레드 로컬 캐시) + 면적 평균 축소 / 2×2 보간 확대 — 탐색기 서버 머리글·루트(64 축소) · 묶음 모드 연결 행(20) · 접속 창·세션 창도 같은 것을 쓸 수 있다 · (09-22판 = SVG 틀 + 라벨 13종 · 폐기) |
| **order — 순서/표시 모델 + 편집 창**(10-04) | nexa-ctl `order.rs`(부품 · 순수) + nexa-sql `order_win.rs` · `statusbar.rs` | 문법 `블록:vis[자식:vis,…]\|…` · `parse`/`serialize`/`normalize`/`default_order`/`visible_blocks`(기본 숨김 표 = 인자) — nexa-dir3 툴바 조정에 이은 **두 번째 사용처라 부품으로 올림** · 편집 창 = 체크·▲▼·Ctrl/⌘+↑↓·끌기·Space·Esc·Reset · 잠금 항목 · 그룹(자식) 2단 · 설정 키 하나(`statusbar.layout`)에 저장 · 10-04 툴바 = 가상 키 `toolbar.order`(어댑터가 `toolbar.layout` + `toolbar.hidden`으로 나눠 저장 · 그룹 잠금 · `child_move=false`) — 사용처 = 상태바 · 툴바 |
| **ellipsize_middle + show_full**(09-22) | nexa-ctl `draw` | 긴 경로·라벨의 가운데 `…` 축약(접두사 폭 표 · 앞 ≈ 뒤) + 전역 "전체 보기" 스위치(Alt 동안) — 풀다운·우클릭 메뉴·팔레트·검색 결과 공통 · 새로 경로를 보이는 곳은 이 부품을 쓴다 | journal 09-22 §38 |
| **IntentFade / HoverFade / FadeSpeed** | nexa-ctl `tokens` | 지나가는 대상의 hover 비용 0 · 마지막 의도만 · 속도 속성 2단 | 그리드 행 · 목록 행 · 콤보 항목 · 버튼 · 텍스트박스 |
| **hover/눌림 색 · 페이드 ms · 스크롤바 지연 전역 setter** | nexa-ctl `tokens` · `scroll` | "설정 한 번 = 전 컨트롤 즉시"(핫스왑) | `ui.*` 설정 |
| **ScrollBars(축별)** | nexa-ctl `scroll` | 오버레이 · 축 독립 · 상태 상수 크기 · `set_fast_scroll` | 편집기 · 그리드 · 로그 · 목록 · 상태 메시지 |
| **FastScroll · ScrollAccel · SpeedHud**(09-30) | nexa-ctl `scroll` | 고속 스크롤 = 전역 설정(핫스왑 · `set_fast_scroll`) + 가속기(같은 방향 사건이 window 안에 이어지면 step번마다 배수 +1 · 상한 max · 사건마다 `factor` 하나 · 큐·타이머·관성 0) + 속도 HUD(×N 캡슐 · hold 뒤 fade · 9자리) · `ScrollBars`가 셋을 품는다(`note_fast` · `set_fast_override`) | 편집기 · 결과(모든 보기) · 객체 탐색기 · 검색 결과 · 프로젝트 · 북마크 · 아웃라인 · 확장 패널(직접) · 설정 `scroll.*` 7키 |
| **FilterQuery**(09-30) | nsql-core `filterq` + nexa-sql `filterbar::Matcher.query`/`NodeFacts` | 필터 질의 언어(`키:값` 술어 · AND/OR/NOT · 괄호 · 따옴표 · 용량 단위) · `Facts` 포트로 이름·종류·용량 | 모든 필터 상자(객체 탐색기 size/type · 프로젝트 size/ext/path · 그 밖 이름) · docs/98 |
| **LatestIntent<T>**(09-30) | nexa-ctl `tokens` | "마지막 사건만 뜻 있는" 지연 동작의 표준: 사건 = 목표 덮어쓰기(큐 0) · `tick`이 delay 동안 머문 목표만 확정 · 바뀌면 앞 의도 폐기 · 표시 때 목표 유효성 재확인 | 탐색기 노드 툴팁(확정 = (칸, 노드) · 커서 아래 재확인) · 후보 = 링크 툴팁 · 그리드 표식 툴팁 · 검색어 확정 · docs/99 |
| **PointerCapture**(09-30) | nexa-sql `app/input.rs` `press_capture`/`area_at`/`dispatch_captured` | 누른 영역이 MouseUp까지 영역 밖 이동·놓임을 받는다(드래그 선택 컨트롤의 기본 처리 · 커서 아래 라우팅의 예외) | 편집기·그리드·탐색기·패널 7 · 찾기 · 상세 |
| **DropClick**(09-30) | nexa-ctl `toolbar` | 드롭다운 항목 ▾ 클릭 동작 속성: `Same`(기본 · 본체와 같은 id) · `Separate`(`id#drop`) | 결과 보기 모드 버튼 · 탭 연결정보 |
| **ContextMenu(id만) · EditMenu** | nexa-ctl | 실행은 호스트 몫 · 컨트롤은 무엇을 할지 모른다 | 목록 · 콤보 · 입력란 |
| **TimeoutButton(경고 톤 · 두 줄 · 잔여 표시 옵션)** | nexa-ctl | 파괴적 2단 확인 · 시간 주입(결정적 테스트) | Delete |
| **ColorPanel** | nexa-ctl | HSV+A · 프리셋 · 최근 · 실시간 보고 | 색 설정 창 |
| **Control::set_enabled · clear_transient · own_focus 패턴** | nexa-ctl · `connect.rs` | 포커스 ≤1 · 가려진 컨트롤 일시 상태 비움 | 모든 창 |
| **ProbePolicy · ProbeHub(요청당 스레드 · 상한)** | `probe.rs` | 주기·지수 백오프·즉시 확인·병렬·격리 | 신호등 |
| **Attempt 큐(상한 + FIFO)** | `main.rs` | Test/Connect 동시 상한 · 순서 보장 | 접속 창 |
| **Runner + Opener + catch_unwind 워커** | `nsql-run` · `worker.rs` | 세션 소유 스레드 · 패닉 격리 · 빠른 판정 | 실행 |
| **owned_by(소유 창) · rehover · wheel_event** | `winfocus.rs` · `input.rs` | 작업표시줄 1 · 모달 닫힘 뒤 hover · 스크롤 방향 한 곳 | 3창 |
| **Timeline(Stage)** | `nsql-core` | 각 층이 자기 단계만 · 표시 시점 포맷 | `--timing` · 상태줄 |
| **ConnTuning + HIDDEN 설정** | `conn_win.rs` · `nsql-settings` | 구현 상수 → 설정(비노출) 주입 | 접속 창 |
| **RowSource 포트 · ResultData(Arc 세그먼트) · View(인덱스 투영)** | `nsql-core` · `nsql-io` | 결과 **한 세트** · 형식별 사본 0 · 렌더러 하나(`render_block`/`generate_src`) · NULL 글자 설정 1 | 그리드 · 텍스트 보기 7종 · 복사 · SQL 복사 · CLI(DR-33) |
| **ToolDock · ToolGroup · DockLayout · ToolItem::separator + ToolFloatWin** | nexa-ctl `tooldock` · `toolfloat.rs` | 목적별 그룹(아이콘·구분자 계층) · 그립 드래그 순서 · 떼어 내기 = 액션만(창은 호스트) · 배치 문자열 1키 · 툴바 소유 한 곳 | 상단 툴바(09-17) · 결과 도구줄 후보 |

| **타이밍 두 벌 — 트레일링 스로틀 / 트레일링 디바운스**(사용자 09-28 용어 확정) | `app/project.rs` `project_autosave_tick`(스로틀 + 주기 안전망) · `persist_settings`·저장 큐(디바운스 · [70 §3](70-autosave-and-restore.md)) | 잦은 요청을 1회로 접기 — **스로틀** = 창의 기준이 **첫** 사건 · 창이 닫힐 때 쌓인 것을 한 번에(코얼레싱·배치 · 프로젝트/파일 저장) · **디바운스** = 창의 기준이 **마지막** 사건 · 마지막 값이 전체를 대변(latest-wins · 설정 저장 · 잦은 호출) · 새 타이머를 만들 때 둘 중 무엇인지 먼저 정한다([70 §7-1](70-autosave-and-restore.md)) | 프로젝트 자동 저장 · 설정 저장 · 저장 큐 |

규칙: 같은 문제를 두 번째 만나면 **부품으로 올린다**(nexa-ui면 공용). 창마다 복사한 코드는 이 표의 후보다.

## 3. 관리 범위 — 지금 코드에서 벗어나 있는 것(개선 로드맵)

| # | 증상 | 기법 | 작업 |
|---|---|---|---|
| A | 창 3개(접속·로그·색)가 softbuffer/RasterCtx/이벤트 변환(`to_input`)/틱을 **복사**해 갖고 있다(≈300줄×3) | **WindowHost** 부품(nexa-clip 창 골격 이식): 창 생성·표면·배율·입력 변환·틱·페인트 프레임을 한 타입으로, 창은 `layout/route/paint`만 구현 | **T-65** |
| B | `conn_win.rs` 2.5k줄 — 목록 모델(정렬·필터·열)·행 버튼·폼·프로브 표시·메뉴가 한 파일 | 목록을 **nexa-grid `ConnectionGrid`**(U-3 · dir2 rows 이식)로 · 프로브 표시는 `RowSource` 장식 · 창은 조립만 | T-31b·U-3 |
| C | 확장점 중 드라이버 RPC·WASM 플러그인이 설계만 있음 | [22](22-driver-extensions.md) T-27~30 · 규약 버전 필드부터 | T-27 |
| D | 접속 창 조정값은 설정화됐지만 편집기·그리드·로그 창 상수는 아직 코드 | `EditorTuning/GridTuning`으로 같은 방식(비노출) | **T-66** |
| E | 명령 id·단축키·메뉴가 문자열로 흩어져 있음 | **Command 레지스트리**(id · 라벨 Msg · 기본 키 · 핸들러) 하나 → 메뉴·팔레트·툴바·키맵이 같은 표를 읽는다 | **T-67**(T-53 선행) |
| F | 워커 `Cmd/ConnOutcome/RunEvent` 셋이 각각 자람 | 실행 계층 **메시지 규약 문서화**(버전 · 큐/스레드 소유권 표) — RPC 드라이버 규약과 맞춘다 | **T-68** |

우선순위: A → E → D(작은 것) → B(nexa-grid와 함께) → C.

## 4. 결정(관례 · 사용자 확인 불요)
- IoC = **생성자 주입 + 레지스트리** · 전역 컨테이너/리플렉션 없음(DR-3 외부 crate 0 · 단일 바이너리 DR-1과 조화).
- 동적 배치 경계 = **프로세스(stdio RPC)·WASM·스레드** 셋만 — 네이티브 dylib 플러그인은 쓰지 않는다(ABI·안정성).
- 규약에는 **버전 필드**를 둔다(드라이버 매니페스트 · `.nexa-syntax` · 라이선스 파일 · 설정 키 rename 표).
- 구현 상수는 **설정 레지스트리**로 · 자주 바꾸지 않으면 `HIDDEN`(`nsql config list all`).
