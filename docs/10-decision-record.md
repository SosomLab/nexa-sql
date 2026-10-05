# 10 · 결정 기록 (DR) · 권장 확정 대기 (DP) · 열린 결정 (D)

> 확정은 DR, 내가 낸 결론으로 사용자 확인이 남은 것은 DP, 미정은 D. 변경 시 과거를 지우지 않고 새 항목으로 정정. **최신 갱신 2026-10-05**(§3-1 번호별 현황 신설 — §3 표는 그 전 기록 그대로 · 번호가 문서마다 겹친 것은 §3-1 머리말). **번호 규칙(10-05 확정 · [16 §2-9](16-doc-git-conventions.md))**: 이미 나간 번호는 바꾸지 않는다 · 겹친 번호는 `문서번호·D-nnn`(예 `25·D-44`)으로 부른다 · 새 번호는 **D-254부터**(발급 전 저장소 전체 grep) · 같은 내용 두 번호(D-8↔19의 D-12 · D-17↔19의 D-13)는 이 문서 쪽이 정본.

## 1. 확정 결정 (DR) — 사용자 발언에 근거

| # | 결정 | 근거 | 확정 |
|:--:|---|---|---|
| **DR-1** | **언어·스택 = Rust 올 네이티브** — Tauri/Electron/Qt 없이 `nexa-ui`(자체 래스터) 위에 그린다 | 사용자 *"Rust 기반 앱이 안정적으로 느껴짐"* + [06 Part B](06-rust-ecosystem.md) 평가(egui가 얻는 점수 = nexa-ui도 얻음 · 에디터 자체 구현 비용은 프레임워크 무관) + 계열 3제품 출시 실적 | ✅ 09-12 |
| **DR-2** | 라이선스 = **PolyForm Noncommercial 1.0.0**(영문 정본 + 한글본) — 영리 목적만 유료 | 사용자 요청 *"nexa-clip 등과 동일하게"* · [13](13-licensing.md) | ✅ 09-12 |
| **DR-3** | **외부 crate 기본 0 지향의 명시 예외 = DB 드라이버·셰이핑** — 어댑터 계층 안에 가두고 공개 시그니처에 외부 타입 0. 그 외 추가는 §4 원장 | 드라이버 없이는 제품이 없다(사용자 *"핵심 기능을 쓰려면 드라이버 구현이 필요"*) | ✅ 09-12 |
| **DR-4** | **공용 컨트롤을 먼저 라이브러리(`nexa-ui`)로 분리**하고 nexa-sql은 path 의존으로 소비 | 사용자 요청 *"컨트롤들을 별도 라이브러리로 묶어서 재사용 가능한 구조를 먼저"* · clip DR-18 | ✅ 09-12(추출 완료 · 189 테스트) |
| **DR-5** | **편집기 = Sublime Text 차용**(기본 편집 기능 · 명령 이름 · 키맵) · **확장 = Sublime 패키지 구조 차용**(데이터 패키지 + 플러그인) | 사용자 방침 09-12 · [09](09-editor-and-packages.md) | ✅ 09-12 |
| **DR-6** | **CLI `nsql`을 개발 범위에 포함** — 접속·스크립트 실행·export/import·bulk insert. GUI와 같은 코어 | 사용자 요청 09-12 · [11](11-cli.md) | ✅ 09-12 |
| **DR-7** | **4계층 아키텍처** — UI(IDE) / Core / Network / DBMS 어댑터가 독립, 허브 포트(trait)로만 연계 | 사용자 요구 09-12 · [01](01-architecture.md) | ✅ 09-12 |
| **DR-8** | **세션 변수는 클라이언트 메모리에 산다**(SQL*Plus 동일). 리터럴 대입은 DB 왕복 없이 로컬 · 방언 재작성으로 MSSQL 등에서도 다수 문장이 같은 변수를 공유 | 사용자 핵심 요구 · [08](08-session-variables.md) · 구현 33 테스트 | ✅ 09-12(구현) |
| **DR-9** | 문서·git 규약 = 계열 [16](16-doc-git-conventions.md) 그대로(push는 명시 요청 시에만 · `git add -A` 금지 · 3-OS 검사) | 계열 표준 | ✅ 계승 |
| **DR-10** | **단일 앱**(DBMS별 앱 아님) — 방언 데이터 + 얇은 재작성 · 1급 방언은 패키지로 깊게 · 배포만 에디션 분리 가능 | DP-1 → 사용자 *"나머진 한꺼번에 개발 진행"* 09-12 | ✅ 09-12 |
| **DR-11** | **지원 범위** 1급 Oracle·SQL Server → 2급 PG·MySQL·SQLite → 3급 국산 DBMS(ODBC) → 플러그인. NoSQL 범위 밖 | DP-2 | ✅ 09-12 |
| **DR-12** | Oracle 드라이버 = `oracle`(kubo · ODPI-C) 지금 → 공식 `oracledb` GA 시 어댑터 교체. **09-13 정정**: 교체 조건 = GA **+ OUT/IN OUT 바인드·REF CURSOR·DBMS_OUTPUT 회수 지원**(beta.3 스파이크: 19c 접속 157ms ✓ · OUT API 부재 ✗ — D-22) | DP-3 · 실기 09-13 | ✅ 09-12 · 정정 09-13 |
| **DR-13** | SQL Server 드라이버 = `tiberius` 계열 → Microsoft `mssql-tds` 성숙 시 재평가 | DP-4 | ✅ 09-12 |
| **DR-14** | 결과셋 = 자체 모델(Arrow는 export 어댑터에서만) | DP-5 | ✅ 09-12 |
| **DR-15** | 셰이핑 = `rustybuzz` 도입(원장 등재) — 우선은 nexa-gfx 글리프 폴백으로 한글을 그리고, 조합·합자는 셰이퍼 단계에서 | DP-6 | ✅ 09-12 |
| **DR-16** | 플러그인 = 데이터 패키지 → WASM(`wasmi`) → Lua는 수요 시 | DP-7 | ✅ 09-12 |
| **DR-17** | 예산 게이트 — 기동 < 1s · 유휴 RSS < 80MB · 10만 행 < 150MB · 바이너리 ≤ 30MB | DP-8 | ✅ 09-12 |
| **DR-18** | **CLI(M1)와 최소 GUI(M2 슬라이스)를 병행** — 같은 코어(`nsql-run`)를 둘이 소비 | DP-9 정정: 사용자 *"GUI 최소 기능 구현을 병행하면서 CLI 함께"* 09-12 | ✅ 09-12 |
| **DR-19** | ★ **한글 처리·고정폭 폰트 1급 지원** — 편집기·그리드는 고정폭(한글 고정폭 D2Coding 우선) + 한글 UI 본 폴백 · IME preedit 오버레이 | 사용자 09-12 *"한글 처리와 고정폭 폰트 등을 잘 지원"* · `nexa-font` | ✅ 09-12 |
| **DR-20** | **코드 서명은 별도 요청 시 별도 진행** — 지금은 무서명(계열 v1과 동일) | DP-10 → 사용자 09-12 | ✅ 보류 확정 |
| **DR-21** | **실서버 검증 = GitHub Codespaces devcontainer + 각 DBMS별 Docker 컨테이너 + Actions 통합 워크플로** — 로컬 Docker Desktop 의존 없음 | D-14 → 사용자 09-12 *"codespaces … 각 DBMS별 docker"* · [20](20-testing-codespaces.md) | ✅ 09-12 |
| **DR-23** | ★ **GUI 접속 = DBeaver식 접속 설정 대화상자**(좌측 트리 · Main/Advanced/Driver properties · Host/URL · 인증 · Save password · Test Connection · Driver Settings/Download) **+ Golden식 로그인 리스트**(프로필 목록 · Pin · 더블클릭 접속) | 사용자 09-13 스크린샷 3장 *"다양한 DB를 지원하려면 DBeaver 형태가 맞다 · 1번은 Golden"* · [22 §1](22-driver-extensions.md) | ✅ 09-13(설계) |
| **DR-24** | ★ **드라이버 확장 = GitHub Releases에서 내려받는 stdio JSON-RPC 프로세스** — 기본은 **최신 non-prerelease**, 버전 지정 시 **SxS 다중 버전 보관**(`drivers/<id>/<ver>/`) · 프로필 `driver=<id>@<ver>` 고정 · 관리자(목록·설치·갱신·**삭제** · 참조 프로필 보호) · sha256 + Ed25519 검증 필수. 순수 Rust 드라이버는 내장 유지. 근거: Oracle 클라이언트 ↔ 서버 지원 매트릭스(23ai ← 19c/21c/23ai · 11.2.0.4/12.1 → 19c까지) + ODPI-C 프로세스당 클라이언트 1개 | 사용자 09-13 *"GitHub에서 다운로드해서 확장 사용"* · *"최신 버전이 다운로드"* · *"SxS처럼 여러 버전 · 목록 보고 삭제"* · [22](22-driver-extensions.md) | ✅ 09-13(설계 · 구현 T-27~T-31) |
| **DR-22** | ★ **연결 프로필 = 사용자 설정 폴더 파일 저장소(`nsql-vault`)** — 비밀번호만 ChaCha20-Poly1305 봉투(도메인 = 프로필 이름) · 기기 키는 Windows DPAPI, 그 외 0600 · **CLI·GUI·여러 인스턴스가 같은 폴더 공유**(첫 실행 경합은 `create_new`로 단일 키) · OS 키체인은 기기 키 보호 후속(D-18) | 사용자 09-13 *"사용자 폴더에 접속 정보를 암호화해서 저장 · 연결 시 재사용"* · *"몇 개의 Instance를 실행하든 저장된 암호를 함께 사용"* · [21](21-connection-profiles.md) · D-2 닫힘 | ✅ 09-13 |
| **DR-25** | ★ **인증 서버는 별도 비공개 저장소(`SosomLab/nexa-license-server` · kiros33 계정) · 앱·서버 공유 기능은 형제 라이브러리 `SosomLab/nexa-license`로 분리(path 의존 · nexa-ui 방식)** — 라이브러리는 형식·서명·검증·요청 코드·기기 ID·프로토콜만, `Feature`·UI·저장 정책은 앱이 주입 | 사용자 09-14 2차 *"차후 인증서버는 별도 Repository · 공유 기능은 별도 라이브러리로 분리"* · [25 §9](25-license-tiers-and-server.md) | ✅ 09-14 |
| **DR-26** | ★ **라이선스 티어·서버 운영 확정(D-32~39 권장안 그대로)** — Team(1~5) = 사용자 파일 묶음 기본 + 서버 선택 · Org 좌석 = named 기본 + concurrent 옵션(×2) · 가격 비율 Device 1.0 / User 1.3 / Team 0.9×좌석 / Org 연 0.5×좌석 · 재발급 셀프 연 5회 · 사용자 식별 = OS 로그인명(+`license.user`) · 서버 단일+백업 · 리스 TTL 7일·오프라인 30일·비활성 회수 30일·유예 30일 · **`nexa-license` 라이브러리 = 공개 저장소**(`SosomLab/nexa-license` 생성 09-14) · 서버 저장소 비공개 | 사용자 09-14 *"추천대로 진행할께 · 공개가 적합하다면 공개 처리"* · [25](25-license-tiers-and-server.md) | ✅ 09-14 |
| **DR-27** | ★ **배포 = 설치본만 · 포터블 없음**(09-17 사용자 "포터블도 고려" → **D-78**에서 재검토 · 그 전까지 상태 저장은 전부 `NSQL_HOME` 규약) — 목적별 실행파일(GUI `nexa-sql` · CLI `nsql` · 드라이버 프로세스) · Rust 코어는 정적 링크 · OS 런타임/드라이버 라이브러리는 공유 lib으로 별도 위치 · macOS `.app`(Universal 2 · Frameworks/@rpath · Application Support) · Windows MSI · Linux deb/rpm · 사용자 데이터는 OS 사용자 폴더(`NSQL_HOME`은 개발용) | 사용자 09-15 *"포터블 배포는 하지 않을 것 · 목적별 실행파일 분리 · 공유/정적 라이브러리 별도 구성 · 설치본 · macOS 특징 고려"* · [33](33-distribution-and-packaging.md) | ✅ 09-15(설계 · 구현 T-72) |
| **DR-28** | **PostgreSQL = 내장 드라이버 1급 지원**(`nsql-driver-pg` · rust-postgres 동기 · DR-3 예외 원장) — Oracle·MSSQL·SQLite와 같은 엔진 관용(세션 변수 · SELECT INTO 별칭 · CALL OUT) · 카탈로그·탐색기·CLI 동등 | 사용자 09-15 *"postgresql까지 지원 범위를 확대"* | ✅ 09-15(matrixdb2 실서버) |
| **DR-29** | **내장 드라이버는 정적 링크 유지 · 확장 드라이버는 in-process 동적 라이브러리(C ABI cdylib · 지연 로드)** — 실측(09-15 release): CLI 기동 ~20ms · GUI 사설 메모리 8.5MB · 드라이버 전역 초기화 0(tokio 런타임·ODPI-C는 첫 접속 시) → 정적 링크는 기동·메모리·속도 비용이 없다(코드는 요구 페이징 · 안 쓰는 드라이버 페이지는 적재되지 않음). 파일 분리가 필요한 경우(GitHub 다운로드·SxS Instant Client·재설치 없는 갱신)만 **같은 프로세스 안에서 `dlopen`하는 cdylib**(IPC 0 · 직접 호출) — DR-24의 stdio JSON-RPC **프로세스**는 격리가 꼭 필요한 선택 모드로 강등(같은 ABI를 프로세스 호스트가 감싼다) | 사용자 09-15 *"로딩 시간·불필요한 메모리·속도 지연 없는 구조"* · *"exe 분리가 아니라 동적 라이브러리/플러그인"* · *"분리가 필요없는 수준이면 지금도 괜찮아"* · [22 §0](22-driver-extensions.md) | ✅ 09-15(구조 확정 · cdylib 구현 = T-27 개정) |

| **DR-30** | **트랜잭션 UX**([34](34-transaction-ux.md) 권장안 그대로) — 수동 커밋 단위 = 탭 세션 · 모드 계층 전역 → 프로필 → 탭 · 표시 3층(탭 배지 `●n` · 상태줄 세그먼트+팝업 · 툴바 Commit 배지) · 잃는 순간만 모달 · 오래된 미커밋 빨강(`tx.stale_min`) · `tx.smart_commit` 기본 off(D-52) | 사용자 09-16(일괄 진행 질의에 "권장안으로 확정") · 종전 D-51 | ✅ 09-16 · 구현 = T-77 |
| **DR-31** | **자원 거버너**([39](39-resource-governance.md)) — `perf.mode` 기본 **full** + 배터리/원격 세션이면 상태줄 1회 안내(D-58) · 개별 키를 바꾸면 표시 **custom**(D-59) · OS 신호 모듈 = 새 크레이트 **`nexa-sys`**(nexa-ui · D-60) · `db.statement_timeout` 기본 **0**(D-61) | 사용자 09-16 · 종전 D-58~61 | ✅ 09-16 · 구현 = T-90(a/d 1차 ✅) |
| **DR-32** | **설치기** — Windows **MSI(WiX)**(조용한 설치·GPO · D-49) · macOS `.pkg` 설치 스크립트가 **`/usr/local/bin/nsql` 링크**(D-50) | 사용자 09-16 · 종전 D-49/50 · [33 §2](33-distribution-and-packaging.md) | ✅ 09-16 · 구현 = T-72 |
| **DR-33** | ★ **결과 데이터 = 한 세트 + 뷰 투영** — 행 단위 `Vec<Value>` 덩어리(`ResultSet` · 드라이버·러너·CLI) · GUI는 **`ResultData`**(페치 세그먼트 `Arc` · 덧붙이기/공유 복사 0) · **`View`**(행 순서·열 부분집합 인덱스) · 렌더러(nsql-io)·그리드·복사·정렬은 **`RowSource` 포트 하나**로 읽는다 · NULL 글자 = `grid.null_text`/`cli.null_text` 한 설정 · 컬럼형/아레나 저장은 T-102 후보 | 사용자 09-17 *"포맷과 상관없이 데이터는 1세트로 관리하고 뷰에 맞춰서"* · 종전 D-2 잔여 | ✅ 09-17 52차 · [journal](journal/2026-09-17.md) |
| **DR-34** | ★ **세션 컨텍스트 = 통제의 단위**([52](52-session-modes.md)) — 기본 사상 *1 인스턴스 · 1 서버 · 1 계정* · `Sess` = 워커 + 실행·트랜잭션 상태 전부(활성 탭의 세션을 `self.sess`로 맞바꿈) · **한 세션 = 한 번에 한 작업**(`Sess::blocked` · 실행·Explain·페치·건수·키 조회·Commit/Rollback·접속을 문지기 `gate_open` 하나로) · **공유 연결 N개**(접속 창 Connect = 기존 유지 + 추가 · 같은 서버·DB·계정 중복 금지 · 탭은 처음 실행한 연결에 묶임 · 툴바 Disconnect 드롭다운) · **전용 세션** = 편집기 `CONNECT <프로필\|"접속 문자열">` · `DISCONNECT`/탭 표식 메뉴로 공유 복귀 · **개별 모드** `session.mode=per-editor`(처음 활성화될 때 접속 · 해제한 탭은 실행 불가) · 유휴 닫기(전용만 30분 · 열린 트랜잭션·SQLite 제외 · 주기 핑 없음) · 분기 판단은 순수 함수 + MC/DC 테스트 | 사용자 09-18 *"공유/개별 병행 · 실행 진입점 통합 block · connect 명령으로 Private 세션 · 공유 연결은 여러 개 · 기존 연결은 명시적으로 해제하지 않으면 유지"* | ✅ 09-18 56차 · D-96~108 확정·구현 |

## 2. 권장 확정 대기 (DP)

> 09-12 DP-1~9 → DR-10~18로 승격(사용자 *"나머진 한꺼번에 개발 진행"*). DP-10(서명) → DR-20 보류. 현재 대기 항목 없음.

## 3. 열린 결정 (D)

| # | 내용 |
|:--:|---|
| D-1 | Instant Client를 설치본에 동봉할 것인가(OTN 재배포 조건) vs 사용자 다운로드 안내 — 공식 thin GA면 소멸 |
| ~~D-2~~ | → **DR-22**(파일 저장소 + 봉투 · Windows DPAPI). 잔여 = **D-18** macOS Keychain · Linux Secret Service로 기기 키 보호 |
| D-3 | `nexa-edit`를 `nexa-ui`에 둘지 별도 저장소로 둘지(SDK MIT 분리 가능성 · nexa-ui D-1) |
| D-4 | 한글 IME preedit 오버레이의 3-OS 구현 순서(Windows 먼저 — 사용자 실무 OS 확인 필요) |
| D-5 | MSSQL REFCURSOR 대체 표현(결과 집합 탭) · `SESSION_CONTEXT` 옵션 노출 여부 |
| ~~D-6~~ | → **D-23~D-31**로 세분([23](23-license-activation.md) · 09-14) |
| D-8 | `similar`(비교·3-way 병합) 원장 등재 |
| D-48 | Oracle 라이브 로그 기본 소스 — 자율 트랜잭션 로그 테이블(현장 관행 · 권장) vs V$SESSION client_info(코드 1줄 · 권한) — 권장 = 테이블 기본 + 세션 세그먼트 병행([32 §2](32-server-messages-and-live-log.md)) |
| ~~D-49~~ | → **DR-32**(MSI · 09-16) |
| ~~D-50~~ | → **DR-32**(pkg 스크립트 `/usr/local/bin/nsql` 링크 · 09-16) |
| ~~D-51~~ | → **DR-30**(권장안 확정 · 09-16) |
| D-62~66 | ✅ 09-16 결과 → SQL 키 규칙([41 §2](41-sql-copy-key-rules.md)): PK → 첫 유니크 → 앞 3컬럼 + 경고 1회 · 설정 `sql.key_mode` pk/all · 경고 = CLI 주석/GUI 상태줄+로그 · 테이블 미추정 = `T` · 키 조회 = 필요 시 1회 캐시 |
| D-67 | 가상 키(테이블별 사용자 지정 · T-92) — 앞 3컬럼 경고가 잦으면 도입 |
| D-77 | ✅ 09-16 **글자 래스터 = Windows는 OS(GDI) ClearType 글리프**([journal 44·46차](journal/2026-09-16.md)): nexa-gfx `gdi.rs` — 32bpp DIB에 `CLEARTYPE_QUALITY`로 그려 **채널별(R·G·B) 커버리지** + 정수 전진 폭 + **진짜 볼드 face**(Windows·Golden과 같은 래스터 · Win32 수동 extern으로 crate 0) · 설정 `ui.text_gdi`(기본 켬) · 못 여는 face·비BMP·다른 OS = 내장 ab_glyph + 오토힌트(T-99). 회색 `GGO_GRAY8`은 맑은 고딕의 얇은 세로 획을 지워(46차 `닫`) 폐기 · macOS/Linux OS 경로 = T-100 |
| D-78 | **포터블 배포 모드**(사용자 09-17 "큰 방향성에서는 포터블 배포도 고려") — DR-27 "설치본만"을 연다. 안: exe 옆 `data\`(또는 `NSQL_HOME`)가 있으면 그 폴더 = 사용자 폴더(설정·프로필·툴바 배치·최근 파일 한 트리 · 코드 경로는 지금과 동일) · 자동 갱신·OS 등록 없음 · zip 채널 추가 · Oracle Instant Client는 사용자 몫 · [33 §0](33-distribution-and-packaging.md). 결정 대기 = 채널 이름 · 기기 키(DPAPI는 기기 종속 → 포터블은 파일 키?) · 라이선스 기기 ID(23) |
| D-79 | **인텔리센스 트리거**(사용자 09-17 · [47 §7](47-intellisense-metadata.md)) — 자동(`.` 즉시 · 식별자 2자 + 250ms) + Ctrl+Space 즉시 · 전부 설정(`intel.auto_activation`·`delay_ms`·`trigger_chars`·`activate_on_typing`) | ✅ 09-17 사용자 |
| D-80 | **FROM 뒤 후보 범위** — 현재 스키마 관계 + 스키마 이름 + 시노님 · `SCHEMA.` 뒤는 그 스키마 · `intel.from_scope`(current/schemas_first/current_and_schemas/all) | ✅ 09-17 사용자 |
| D-81 | **메타 선반입 범위** — 현재 스키마(객체 → 벌크 컬럼) + 탐색기에서 펼친/문장에 나온 스키마 · 전 스키마 유휴 순차는 `meta.prefetch=all` | ✅ 09-17 사용자 |
| D-82 | **메타 갱신** — 수동 새로고침 + 실행한 DDL 감지 + 워터마크 주기(유휴 · 300s · 0 = 끔) → 버킷 디프로 변경분만 · `meta.refresh_on_ddl`/`refresh_secs`/`refresh_scope` · `explorer.*` 갱신 키는 `meta.*`로 통합 | ✅ 09-17 사용자 |
| D-83 | **코멘트** — 객체·컬럼 카탈로그 질의에 항상 함께(`meta.comments=always`) · 표시 `intel.show_comments` | ✅ 09-17 사용자 |
| D-84 | **hover 툴팁** — 테이블/뷰 + 컬럼(alias) + 프로시저/함수 · 300ms · 같은 저장소(별도 데이터 없음) · `intel.hover`/`hover_scope`/`hover_delay_ms` | ✅ 09-17 사용자 |
| D-85 | **메타 디스크 캐시** — 프로필별 `.nmeta` + 워터마크 검증 · `meta.disk_cache`/`disk_cache_max_mb` | ✅ 09-17 사용자 |
| D-86 | **후보 매칭** — 정확 > 접두(이진 탐색) > 단어 경계 부분 > 약어 퍼지(접두 부족 시 · 예산 안) · `intel.match`/`match_case` | ✅ 09-17 사용자 |
| **D-133** | ✅ **09-24 기본값 = `iosurface`**(100차 §178 · Debug(의존 최적화) 프레임 48 → 13.9 ms · 실패 시 softbuffer 폴백 · Apple Silicon 미확인) · 🚧 09-21 **② 구현**(nexa-sys `layer_present` · `present.rs` · 설정 `gfx.mac_present` = softbuffer 기본 / iosurface · present 36.2 → 2.9 ms · 프레임 51 → 16 ms · [62 §2-1](62-macos-input-and-present.md)) · ⏳ 09-20 맥 화면 내보내기([62 §2](62-macos-input-and-present.md)) — present 37 ms = CoreAnimation의 CPU 색 변환 · ① softbuffer 디스플레이 색 공간 패치(45.8 → 19.6 ms · 색 약간 진해짐) ② 자체 IOSurface 내보내기(색 정확 · 복사 0 · 권장 · T-136 D6과 함께) ③ 그대로 |
| **D-134** | ⏳ 09-20 `input.hangul_compose` 기본값 — auto(구현 · macOS + 한글 입력 소스일 때 앱 조합) / system |
| **D-135~138** | ✅ 09-21 **변수 관리**([63 §7](63-variable-management.md) · 사용자 전부 권장안): **D-135 변수 표 = 계층**(탭 표가 기본 · 일부러 올린 값만 같은 연결의 탭들이 공유 · 읽기 전용 프로필 층 · 값은 `CONNECT`를 넘어 살고 커서만 무효) · **D-136 보존 = hot exit와 함께**(비밀·커서 제외) + `VAR`/`DEFINE` 스크립트로 내보내기·가져오기 · **D-137 값 없는 바인드 읽기 = 실행당 한 번 묻기**(빠진 바인드 + `&`를 한 격자에서 · 대입 대상은 종전처럼 자동 생성 · 엄격 모드 = 오류) · **D-138 스크립트 전체 실행의 조회 여러 개 = 설정, 기본 문장마다 결과 탭**(`grid.result_per_statement`) | ✅ 09-21 사용자 |
| **D-144** | ✅ 09-28(사용자) 탐색기 서버 노드 **연결 해제 = 항상 고르기**(`explorer.disconnect_pick` 기본 `always` · 연결이 하나여도 모두/이 연결을 확인) — 옛 기본 `auto`는 새 기본으로 | 확정 |
| **D-145** | ✅ 09-28(사용자 "라이선스가 없어도 모든 기능이 동작하는 것이 기본 · 개인 사용은 전 기능 오픈 · 차후 일부 제한 가능") **라이선스 기능 게이트 기본 끔** — 숨은 `license.gates`(옛 `license.gates_dev`)를 켤 때만 Debug·Release 동일 적용 · 게이트 12곳·`Feature` 표는 유지(D-43~46은 "켰을 때"의 규칙) · 종전 "Release 늘 켬"은 폐기([journal 09-28 §8-1](journal/2026-09-28.md)) | 확정 |
| **D-146** | ⏳ 09-28 설정 키 이름 2차 rename([94 §6-2](94-settings-key-naming-and-location.md) · `conn.`→`login.` · `sql.`→`grid.` · 단위 접미 `ui.fade_*_ms` · `editor.tab_accent_color`) + 카테고리 쪼갬(78 §3 · Session → 세션/트랜잭션 보호/스크립트·변수) — 권장 = 둘 다(T-250 · T-186) · 1차 4키는 09-28 적용 | 권장안 |
| D-139~143 | ⏳ 09-21 변수 관리 부속(권장안으로 진행 · 이의 있으면 바꿈): D-139 `SELECT … INTO` 0행·여러 행 = Oracle식 오류(`vars.into_policy`) · D-140 비밀 값 = `ACCEPT HIDE` + 이름 규칙 → 가림·기록/저장 안 함 · D-141 `&` = Oracle 방언 탭만 기본 켬 · 문자열 안은 선택 · D-142 이름 대소문자 무시(표기 보존) · SQLite `:x @x $x` = 한 변수 · D-143 서버에 비추기(SESSION_CONTEXT 등) = 뒤로 | 권장안 |
| (번호 주의) | ★ **D-100~110은 두 번 쓰였다**(맥 09-18과 Windows 09-19 세션이 서로 모르고 같은 구간을 씀 · 09-21 정리): **52·D-96~108**(세션 모드) · **53·D-109~114**(접속 생존) ↔ **56·D-100~105**(수동 커밋 잠금 방지) · **57·D-106~110**(탐색기 갱신). 기록(journal·DEVLOG·커밋)은 역사라 고치지 않는다 — 이 구간을 가리킬 때는 **문서 번호를 앞에**(`56·D-103`). 새 결정은 **이 표의 최댓값 + 1**에서 시작하고(지금 D-135~), 번호를 잡기 전에 `git pull` 뒤 이 표를 먼저 본다([16](16-doc-git-conventions.md)) | 규칙 |
| D-129~132 | ✅ 09-20 **되돌리기 후속**([60 §7](60-undo-redo-redesign.md) · 사용자 "추천한 대로 · 선행 필요 의견 동의 · T-142 선행" → 구현 84차): D-129 기록 파일 = Vim `undofile` 방식(저장할 때 쓰고 · 열 때 본문 해시가 같으면 복원 · `editor.undo_persist` on) · D-130 거대 편집 = 지우는 동작을 3초 안에 되풀이해야 진행 + 히스토리 비움(`editor.undo_giant_mb` 32 · 권장 "L2에서만" → 바이트 기준으로 어느 탭에나) · D-131 쉬었다 치면 새 묶음 1,500 ms · D-132 재로드 = 최소 줄 편집(`merge3::line_edits`) |
| D-125~128 | ✅ 09-19~20 **대용량 파일**([59 §5](59-large-file-handling.md) · 사용자 "구현해줘 · 충돌/조정은 효과적인 방향으로" → 권장안): D-125 큰 파일 모드 L1 5 MB/10만 줄 · L2 20 MB/30만 줄 ✅ · D-127 되돌리기 = 개수 + **바이트 예산** ✅([60](60-undo-redo-redesign.md)) · D-128 열기 선택(그대로/앞부분만/읽기 전용/열지 않고 실행) + 비동기 적재 + 탭 격리 진행 막 ✅(스트리밍 분할기 제외) · **D-126 버퍼 = UTF-8 갭 버퍼 + 줄 시작 표 ✅ 84차**([59 §6](59-large-file-handling.md) · nexa-ctl `TextBuf` · 좌표는 글자 인덱스 유지 — 65 MB 상주 348 → 87 MB · 입력 190 → 3 ms) |
| D-119~124 | ✅ 09-19 **외부 파일 변경**([58](58-external-change-policy.md) · 전부 권장안 · 구현 T-140): 고친 탭도 겹침 0이면 묻지 않고 병합(안전 장치 다섯) · 비모달 띠 + 저장 2단 확인 · 파일당 띠 하나 + "유지"는 디스크 서명에 묶기 · 감지 = 활성화·탭 전환·저장 직전 + 보이는 탭 폴링(비활성 0) · stat 서명 + 해시(의존 0 · 포트만 열림) · 병합 자체 구현 |
| D-106~110 | ✅ 09-19 **객체 탐색기 갱신**([57](57-explorer-refresh-after-ddl.md) · 전부 권장안 · 구현 T-138): 실행한 DDL → 그 폴더만 디프 · 트랜잭션 DDL 수동 커밋 = 커밋 때 · 유휴 워터마크 300초 바뀐 스키마만 · "객체 없음" 신호 · 새 객체 2초 강조 |
| D-100~105 | ✅ 09-19 **수동 커밋 잠금 방지**([56](56-manual-commit-lock-prevention.md) · 구현 T-137): 읽기 트랜잭션 자동 종료 · ROLLBACK · **30분 뒤 카운트다운 자동 롤백**(사용자 선택 — 권장은 경고만) · 막힘 감지 30초 · 서버 파라미터 0 · **L1~L4 한 번에**(사용자 선택) · 2차 = 접속 유형 개발/시험/운영(값·UI는 가정 — 확인 필요) |
| D-76 | ✅ 09-16 **정규식 엔진 = `regex` + `fancy-regex`**(찾기·바꾸기·파일 찾기/바꾸기 · [journal 32차](journal/2026-09-16.md)): 선형 시간 RE2식 코어 + lookaround/역참조는 fancy만 백트래킹(시간 상한) · 순수 Rust(DR-3) · `$1` 치환 규약 |
| D-68~72 | ✅ 09-16 페치 모델·결과 탭([43 §0](43-fetch-model-and-result-tabs.md)): 상한 = **대화형만**(GUI 그리드 · `nsql shell` · run/export/파이프 무제한) · 기본 **200**(전역 → 탭 로컬 · D-42 답) · 추가 페치 = **서버 커서 유지 + OFFSET 폴백**(D-43 답 · 세션당 커서 1) · 결과 탭 = 편집기 탭 ↔ 패널(Ctrl+Enter 교체 · Ctrl+\\ 추가) · 메모리 예산 `grid.memory_budget_mb` 256(탭 합계) |
| ~~D-58~~ | → **DR-31**(`perf.mode` 기본 full + 1회 안내 · 09-16) |
| ~~D-59~~ | → **DR-31**(custom 표시 · 09-16) |
| ~~D-60~~ | → **DR-31**(`nexa-sys` 크레이트 · 09-16) |
| ~~D-61~~ | → **DR-31**(기본 0 · 09-16) |
| D-52 | `tx.smart_commit`(자동 모드라도 DML 실행 시 그 탭을 수동으로 · DBeaver 기본) — 권장 기본 off(자동 커밋 기본 유지) |
| D-9 | 로프 크레이트 `crop` vs `ropey` — E-1 착수 시 |
| ~~D-14~~ | → **DR-21**(Codespaces + DBMS별 Docker + Actions) |
| **D-15** | ★ **다음 우선순위** — nexa-edit E1~E5 / 세션 hot exit / 비교·git·오브젝트 캐시 / CLI 완성(import·bulk·PG/MySQL) 중 순서 (사용자 답 대기 · 권장 = E1~E5 → hot exit) |
| **D-16** | ★ **실기 OS 순서** — macOS 먼저 vs Windows 먼저(IME 구현 순서에 영향) (사용자 답 대기) |
| **D-17** | 오브젝트 시점 캐시·로컬 히스토리 기본 위치 — 앱 데이터 폴더(권장) vs 프로젝트 `.nexa/` (사용자 답 대기) |
| **D-19** | 드라이버 확장 저장소 구성 — 단일 `SosomLab/nexa-sql-drivers`(태그 접두) vs 확장별 저장소(권장 = 단일 + 서드파티 별도) · [22 §8](22-driver-extensions.md) |
| **D-20** | Oracle OCI 확장의 Instant Client 동봉(OTN 조건 · D-1 통합) vs 사용자 다운로드 안내 |
| **D-21** | 드라이버 갱신 확인 기본값(켬 권장 · 폐쇄망은 설정으로 끔) |
| **D-22** | 순수 Rust thin `oracledb` 채택 시점 — OUT 바인드·REF CURSOR가 공개 API에 들어오는 판(GitHub Discussions 문의 후보). 들어오면 Oracle 내장 드라이버 = thin, OCI(kubo)는 레거시 확장([22 §4-1](22-driver-extensions.md))으로 |
| **D-23** | ★ **정품 인증 — 게이트 기능 목록·무료 범위**([23 §4-3](23-license-activation.md) 권장안: xlsx/parquet export · SSH 터널 · 드라이버 확장 · 다중 접속 · 비교 = Pro / 나머지 무료). ⚠️ [13 §3](13-licensing.md) "무료 제한 없음" 권장을 **정정**하는 결정(사용자 09-14 요청) |
| **D-24** | ★ 기기 묶음 원천 — ⓐ OS 기기 식별자 해시(권장) ⓑ `device.key` 무작위 ⓒ 결합([23 §6](23-license-activation.md)) |
| **D-25** | ★ 라이선스 모델 — 영구+업데이트 1년(권장) vs 구독 · 기기 수 · 등급 이름 → **09-14 2차 구체화: 4단(Device 1대 · User 5대 · Team 1~5석 · Organization 6+ 서버) · 개인 영구/조직 구독 · 가격 비율 = D-34**([25 §2](25-license-tiers-and-server.md)) |
| **D-26** | 라이선스 파일 형식 — 자체 key=value 봉투(권장 · JSON 의존 0) vs PASETO v4.public 엄수(13 §3 문구) |
| **D-27** | 발급 비밀키 보관(발급 PC DPAPI 봉투 + 오프라인 백업) · 공개키 2개 내장 무중단 회전 |
| **D-28** | 14일 체험 파일 도입 여부(권장 = 있음 · 기기당 1회) |
| **D-29** | 온라인 2차(Cloudflare Workers 발급 자동화) 착수 시점(권장 = v1.0 이후 · 수동 이메일로 시작) |
| **D-30** | 집행 시점(= D-6 구체화) — v1.0부터 게이트 활성(권장) · 그 전 판은 배지만 |
| ~~D-31~~ | → 사용자 09-14 2차: 조직 티어는 **사내 인증 서버**로 — 세부는 **D-32~D-39** |
| ~~D-32~~ → DR-26 | ★ Team(1~5)의 서버 — 파일 묶음 기본·서버 선택(권장) / 서버 필수 / 서버 미제공 |
| ~~D-33~~ → DR-26 | ★ Organization 좌석 모드 — named만 / named + concurrent 옵션(권장 · ×2) / concurrent만 |
| ~~D-34~~ → DR-26 | ★ 가격 비율 — Device 1.0 · User 1.3 · Team 0.9×좌석 · Org 연 0.5×좌석 · floating ×2([25 §2](25-license-tiers-and-server.md)) |
| ~~D-35~~ → DR-26 | User 기기 5대 재발급 셀프 서비스 횟수(연 5회 권장) |
| ~~D-36~~ → DR-26 | 조직 사용자 식별 — OS 로그인명(권장) / 이메일 / LDAP·AD(후속) |
| ~~D-37~~ → DR-26 | 서버 가용성 — 단일 + 백업(권장) / 2노드 |
| ~~D-38~~ → DR-26 | 리스 TTL 7일 · 오프라인 리스 30일 · 비활성 회수 30일 · 유예 30일(권장값) |
| **D-41** | ★ CLI 짧은 옵션 진영 — ⓐ psql 기본(`-p` 포트 · `-d` DB · 방언 `-T` · mysql/sqlcmd 별칭 · 권장) / ⓑ 현행(`-d` 방언 · `-p` 비번) / ⓒ 긴 옵션만([27 §3](27-cli-conventions.md)) |
| ~~D-42~~ | → **D-69**(200행 · 사용자 09-16 · [43](43-fetch-model-and-result-tabs.md)) |
| ~~D-43~~ | → **D-70**(커서 유지 + OFFSET 폴백 확정 · [43 §3](43-fetch-model-and-result-tabs.md)) |
| **D-44** | 타이밍 로그 파일 기본 — 끔(`NSQL_LOG=timing`으로 켬 · 권장) / 켬 |
| **D-45** | `CONNECT` 동사 — ⓐ 우리 클라이언트 명령 하나(SQL*Plus 상위 호환 · 별칭 `\c`·`:connect` · 권장) / ⓑ 네이티브·래퍼 분리 / ⓒ 접두 필수([27 §6](27-cli-conventions.md)) |
| **D-46** | 탐색기 메타 세션 — 프로필당 별도 접속(권장) / 편집기 세션 공유 · 설정 `explorer.session`([28 §6](28-object-explorer.md)) |
| **D-47** | 세션 공유 시 편집기 실행 중 메타 요청 — 대기(권장) / 거부 |
| ~~D-96~108~~ | 세션 모드([52 §10](52-session-modes.md)) — ✅ 09-18 권장안 확정·구현(사용자 *"결정에 따른 개발도 모두 진행"*): 전용 = 탭 속성 · `CONNECT` 앞 문장 거부 · 활성화 시 접속 · 공유 N 추가(상한 8) · 세션 상태 있는 세션은 유휴 닫기 제외 · 뒤에서 끝난 실행 ✓/✗ · 트랜잭션 로그 세션 열 · 서버별 탐색기(세션 ≥1 유지) · 같은 서버 = 계정까지 · 끊긴 묶음은 표식으로 |
| **D-117** | 탐색기 루트에 참조 탭 수 표시([54 §8](54-connection-model-and-disconnect.md)) — 권장: 루트 오른쪽 흐린 `· 3 tabs` |
| **D-118** | 툴바 Disconnect `SharedAsk` 팝업 기본 항목 — 권장: 첫 항목 "N개 탭 모두 해제"(DBeaver 의미) |
| **D-115** | 로그인 필수 항목([22 §10](22-driver-extensions.md)) — 비밀번호 없는 접속(PG `trust` · Oracle OS 인증)을 허용하는 스위치를 둘 것인가 — 권장: **두지 않음**(인라인 규칙 52 §4와 같게 비밀번호 필수 · 대신 T-132 변수/모달) |
| **D-116** | 필수 표기 — 라벨 `*`(권장 · Azure Data Studio식 · 방언마다 필수가 달라 명확) / 선택 칸 "(optional)"만(GOV.UK식) / 둘 다 |
| ~~D-109~114~~ | 접속 생존 관리([53 §6](53-connection-liveness.md)) — ✅ 09-18 권장안 확정·구현(사용자 *"제안 방식으로 구현"*): 마지막 성공 60초 뒤 판정 · TCP keepalive 60초 · Oracle 호출 상한 기본 끔 · 다음 실행 때 자동 재접속 · Broken은 표식으로 남김 · OS 신호는 T-130 |
| **D-40** | ★ `nexa-license` 서명 알고리즘 **포트화** — `alg=ed25519`(dalek 2.x · beep/clip/sql 기본) + `alg=p256`(dir2 · Windows CNG 인박스 · 외부 crate 0 유지) · 루트 키 2개 · 발급기 양쪽 서명([25 §10-2 #4](25-license-tiers-and-server.md)) — 대안 = dir2 제외(단일 Ed25519) |
| ~~D-39~~ → DR-26 | `nexa-license` 라이브러리 가시성 — 공개(권장 · CI 토큰 불요 · 계열 재사용) / 비공개(CI에 fine-grained PAT) — 서버 저장소는 비공개 확정(DR-25) |
| **D-18** | 기기 키(`device.key`) OS 비밀 저장 결합 — macOS Keychain · Linux Secret Service(현재 0600 평문 · Windows는 DPAPI ✅). 결합 시 키 파일만 교체, 프로필 재암호화 불요([21 §5](21-connection-profiles.md)) |

## 3-1. 번호별 현황 — 2026-10-05 정합(다른 문서 기준 · 위 §3 표의 보충)

> 위 §3 표는 09-13~09-28에 적은 그대로 둔다(과거를 지우지 않는다). 이 절은 **저장소 문서 전체**(CLAUDE.md · STATUS · journal · 설계 문서)를 훑어 번호마다 지금 상태를 한 줄로 적은 것이다 — §3에 행이 없던 번호(D-87~90 · D-147 이후 · L-1~L-8 등)도 여기에 채웠다. **문서 근거만**(코드 미확인).
> 상태: ✅ 확정 = 문서에 사용자 확정·"진행" 명시 · 🚧 권장안 진행 = 권장안대로 구현됐으나 사용자 확정 문구 없음 · ⏳ 대기 = 결정 대기 · ❓ 미확인 = 정의나 상태를 찾지 못함. 건수 = 확정 90 · 권장안 진행 77 · 대기 73 · 미확인 2(줄 수 기준).
> ⚠ **번호 충돌**: 같은 번호가 문서마다 다른 뜻으로 쓰인 경우가 있다(D-41~48 = 27/26 ↔ 25 §13-6 라이선스 · D-100~110 · D-143~155 · D-184~186 · D-197~209 · D-219 · L-1~8 = 25 §12-7 ↔ 39 §6 등). 그런 번호는 `D-147[67]`처럼 **정의 문서 번호를 대괄호로** 붙여 줄을 나눴다. 번호는 다시 매기지 않는다(위 번호 규칙 · 10-05).
> 근거의 줄 번호는 10-05 조사 시점 기준이다(STATUS · TODO는 그 뒤 밀릴 수 있다).

| 번호 | 내용 | 상태 | 근거 |
|---|---|---|---|
| D-1 | Oracle Instant Client 설치본 동봉 vs 사용자 다운로드 안내(OTN 재배포 조건) — 답 없음 · D-20과 통합 대상 | ⏳ 대기 | 10-decision-record.md:53 §3 · 22-driver-extensions.md:171 §8 · 64-dbms-clients-and-driver-packaging.md:116 |
| D-3 | `nexa-edit`를 nexa-ui에 둘지 별도 저장소로 둘지 — 답 없음(편집 버퍼는 nexa-ctl `TextBuf`로 구현됨 · TODO:38 "대체됨 → T-142") | ⏳ 대기 | 10-decision-record.md:55 §3 · TODO.md:38 |
| D-4 | 한글 IME preedit 오버레이 3-OS 구현 순서(Windows 먼저) — 답을 적은 문서 없음 | ⏳ 대기 | 10-decision-record.md:56 §3 |
| D-5 | MSSQL REFCURSOR 대체 표현(결과 집합 탭) · `SESSION_CONTEXT` 옵션 노출 — 답 없음 | ⏳ 대기 | 10-decision-record.md:57 §3 · 43-fetch-model-and-result-tabs.md:134 |
| D-7 | nexa-sql 문서에 정의 없음 — 형제 저장소 nexa-ui의 D-7(휴지통)로만 언급 | ❓ 미확인 | STATUS.md:707 · journal/2026-09-14.md:159 |
| D-8 | `similar` crate(비교·3-way 병합) 원장 등재 — 답 없음(3-way 병합은 58·D-124 ✅로 자체 구현 `merge3` · 의존 0) | ⏳ 대기 | 10-decision-record.md:59 §3 · 15-external-file-changes.md:47 · 58-external-change-policy.md:130 |
| D-9 | 로프 crate `crop` vs `ropey` — 소멸: 버퍼 = UTF-8 갭 버퍼 `TextBuf`(59·D-126 ✅ · T-142)로 대체 (09-20) | ✅ 확정 | 10-decision-record.md:96 §3 · TODO.md:38 · 59-large-file-handling.md:128 |
| D-10 | hot exit 저널(델타 로그) 도입 시점 — 1차는 스냅샷만(문서 18 안의 열린 결정) | ⏳ 대기 | 18-session-and-projects.md:37 §4 |
| D-11 | 세션 변수 값 저장 기본값(끔 권장) — 문서 18 안의 열린 결정(뒤에 63·D-136 ✅ 파일별 보존이 같은 주제를 정함) | ⏳ 대기 | 18-session-and-projects.md:38 §4 · 63-variable-management.md:142 |
| D-12 | `similar` 원장 등재(비교·병합 공용) — 문서 19의 결정 후보 · 결정 기록 D-8과 같은 내용 | ⏳ 대기 | 19-compare-git-and-object-history.md:62 §5 |
| D-13 | 오브젝트 캐시 기본 위치(앱 데이터 vs 프로젝트 `.nexa/`)와 상한 — 문서 19의 결정 후보 · 결정 기록 D-17과 같은 내용 | ⏳ 대기 | 19-compare-git-and-object-history.md:63 §5 |
| D-15 | 다음 우선순위(nexa-edit E1~E5 / hot exit / 비교·git / CLI 완성) — 사용자 답 대기인 채 | ⏳ 대기 | 10-decision-record.md:98 §3 · STATUS.md:769 |
| D-16 | 실기 OS 순서(macOS 먼저 vs Windows 먼저) — 사용자 답 대기인 채 | ⏳ 대기 | 10-decision-record.md:99 §3 · journal/2026-09-12.md:35 |
| D-17 | 오브젝트 시점 캐시·로컬 히스토리 기본 위치(앱 데이터 폴더 권장) — 사용자 답 대기인 채 | ⏳ 대기 | 10-decision-record.md:100 §3 · journal/2026-09-12.md:35 |
| D-18 | 기기 키 `device.key`의 macOS Keychain · Linux Secret Service 결합 — 후속(현재 0600 평문) | ⏳ 대기 | 10-decision-record.md:136 §3 · 21-connection-profiles.md:52 §5 |
| D-19 | 드라이버 확장 저장소 구성 — 단일 `nexa-sql-drivers` + 서드파티 별도(권장) · 답 없음 | ⏳ 대기 | 22-driver-extensions.md:170 §8 · TODO.md:52 |
| D-20 | Oracle OCI 확장의 Instant Client 동봉 여부(D-1과 통합) — OTN 조건 확인 후 | ⏳ 대기 | 22-driver-extensions.md:171 §8 · TODO.md:53 |
| D-21 | 드라이버 갱신 확인 기본값(켬 권장 · 폐쇄망은 설정으로 끔) — 답 없음 | ⏳ 대기 | 22-driver-extensions.md:172 §8 |
| D-22 | 순수 Rust thin `oracledb` 채택 시점(OUT 바인드·REF CURSOR 공개 API 조건) — 09-21 재스파이크 가치 있음(T-159) | ⏳ 대기 | 10-decision-record.md:104 §3 · 22-driver-extensions.md:96 · 64-dbms-clients-and-driver-packaging.md:137 |
| D-23 | 정품 인증 게이트 기능 목록·무료 범위 — 게이트 표는 25 §13-3(25·D-41)으로 채택·12곳 배선, 기본은 끔(D-145) (문서 불일치: 23:5·CLAUDE DR-25 행은 답 대기 · 25 §13-6은 09-27 확정) | 🚧 권장안 진행 | 23-license-activation.md:235 §6 · 25-license-tiers-and-server.md:546 §13-6 · 23-license-activation.md:255 |
| D-24 | 기기 묶음 원천 — ⓐ OS 기기 식별자 해시(권장)로 nexa-license `machine` 구현 · 사용자 확정 문구 없음 | 🚧 권장안 진행 | 23-license-activation.md:236 §6 · TODO.md:61 |
| D-25 | 라이선스 모델 — 영구+업데이트 1년(권장) · 09-14 4단 티어로 구체화(티어·가격은 DR-26 · 기간은 L-4) · D-25 자체 확정 문구 없음 | 🚧 권장안 진행 | 23-license-activation.md:237 §6 · 10-decision-record.md:107 · 25-license-tiers-and-server.md:453 |
| D-26 | 라이선스 파일 형식 — 자체 key=value 봉투(권장)로 nexa-license 구현 · 확정 문구 없음 | 🚧 권장안 진행 | 23-license-activation.md:238 §6 · TODO.md:61 |
| D-27 | 발급 비밀키 보관(봉투 + 오프라인 백업) · 공개키 2개 회전 — 발급기 봉투 `nxk1` 구현 · 봉투 방식은 L-1 확정 | 🚧 권장안 진행 | 23-license-activation.md:239 §6 · 23-license-activation.md:254 · 25-license-tiers-and-server.md:450 |
| D-28 | 14일 체험 파일 도입(기기당 1회) — 기본 기간 L-4(09-27 확정)에 "체험 14일(D-28)"로 반영 · D-28 자체 확정 문구 없음 | 🚧 권장안 진행 | 23-license-activation.md:240 §6 · 25-license-tiers-and-server.md:453 |
| D-29 | 온라인 2차(발급 자동화) 착수 시점 — v1.0 이후 · 수동 이메일로 시작(전달 채널은 L-5 확정) | 🚧 권장안 진행 | 23-license-activation.md:241 §6 · 25-license-tiers-and-server.md:434 · 25-license-tiers-and-server.md:454 |
| D-30 | 게이트 집행 시점(v1.0부터 활성 권장) — 답 없음 · 09-28 D-145로 게이트 기본 끔이 됨 | ⏳ 대기 | 23-license-activation.md:242 §6 · 23-license-activation.md:6 |
| D-40 | `nexa-license` 서명 알고리즘 포트화(ed25519 + p256) — 답 없음(09-27 구현은 ed25519 · keys) | ⏳ 대기 | 10-decision-record.md:134 §3 · 25-license-tiers-and-server.md:311 §10 · TODO.md:66 |
| D-41[27] | CLI 짧은 옵션 진영 — ⓐ psql 기본(권장) / ⓑ 현행 / ⓒ 긴 옵션만 — 답 없음(T-51 ☐) | ⏳ 대기 | 27-cli-conventions.md:90 §3 · 27-cli-conventions.md:104 · 10-decision-record.md:121 |
| D-41[25] | 라이선스 게이트 표 §13-3 확정 — 상한식(연결 2/2 · 탭 5 · 2 MB · 결과 탭 2 · 창 1) 도입 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:546 §13-6 · journal/2026-09-27.md:205 · STATUS.md:220 |
| D-42[25] | Org 전용 기능 우선순위 — 감사 로그 → 중앙 프로필 → 정책 강제 → 좌석 서버 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:547 §13-6 · journal/2026-09-27.md:205 |
| D-43[25] | 잠긴 기능 UX — 메뉴에 남기고 "Pro · 대체: …" 안내 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:548 §13-6 · journal/2026-09-27.md:205 |
| D-44[26] | 타이밍 로그 파일 기본 — 끔(권장 · `NSQL_LOG=timing`으로 켬) — 답 없음(파일 로그 싱크 ☐) | ⏳ 대기 | 26-performance-architecture.md:128 §6 · TODO.md:84 · 10-decision-record.md:124 |
| D-44[25] | 무료 배지 "Non-commercial use only" 상시 표시 · 기업 감지 없음(전화홈 0) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:549 §13-6 · journal/2026-09-27.md:172 · journal/2026-10-04.md:108 |
| D-45[27] | `CONNECT` 동사 — ⓐ 우리 클라이언트 명령 하나(권장) · 52·D-98 ✅(접두 강제 없음 · `:connect`만)로 구현 · D-45 자체 확정 문구 없음 | 🚧 권장안 진행 | 27-cli-conventions.md:141 §6 · 52-session-modes.md:309 §10 |
| D-45[25] | 체험(14일)은 Pro만 · Org는 평가 라이선스 별도 발급 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:550 §13-6 · journal/2026-09-27.md:205 |
| D-46[28] | 탐색기 메타 세션 — 프로필당 별도 접속(권장) · 1차 = separate 고정으로 구현 | 🚧 권장안 진행 | 28-object-explorer.md:154 §6 · 28-object-explorer.md:5 · 52-session-modes.md:115 |
| D-46[25] | 상한식 수치 — 동시 연결 2/2 · 편집 탭 5 · 2 MB · 결과 탭 2 · 창 1(출시 뒤 조정 가능) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:551 §13-6 · journal/2026-09-27.md:205 |
| D-47[28] | 세션 공유 불가피 시 편집기 실행 중 메타 요청 = 대기(큐 · 권장) — 1차 separate 고정 · SQLite `:memory:`는 짧은 읽기만 | 🚧 권장안 진행 | 28-object-explorer.md:155 §6 · 28-object-explorer.md:110 |
| D-47[25] | 등록 안내 카드 주기 12 h · 광고 슬롯은 테스트 전용(Release = `none` 고정) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:553 §13-6 · journal/2026-09-27.md:205 |
| D-48[32] | Oracle 라이브 로그 기본 소스 — 권장 = 테이블 기본 + 세션 세그먼트 병행 · 1차 구현 기본 = session · 답 없음 | ⏳ 대기 | 32-server-messages-and-live-log.md:55 §2 · 32-server-messages-and-live-log.md:5 · 10-decision-record.md:60 |
| D-48[25] | 라이선스 호환 원칙 — 발급 파일 불변 · `features=*`는 이후 기능 포함 · 큰 개편 = `nxl2` + 재발급 프로세스 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:552 §13-6 · journal/2026-09-27.md:31 |
| D-52 | `tx.smart_commit` 기본 off(자동 커밋 기본 유지 · 권장) — 설정 키는 off로 존재 · 확정 문구 없음 | 🚧 권장안 진행 | 34-transaction-ux.md:111 §4 · 34-transaction-ux.md:88 · 10-decision-record.md:95 |
| D-53 | 기본 구문 팔레트 — ⓒ 지금 값 유지 + 프리셋 3종 `editor.color_preset`(권장) — 결정 대기 | ⏳ 대기 | 35-editor-colors-reference.md:132 §4 · journal/2026-09-16.md:94 · TODO.md:117 |
| D-54 | 선택 렌더링 — 불투명 기본 + Golden식 반투명 워시는 설정으로(권장) — 미정 | ⏳ 대기 | 35-editor-colors-reference.md:133 §4 · 39-resource-governance.md:160 |
| D-55 | 파일 검색 기본 무시 규칙에 `.gitignore` 자동 적용(끄는 토글) — 옵션으로 구현 · 확정 문구 없음 | 🚧 권장안 진행 | 36-find-in-files-and-project.md:97 §6 · 67-project-workspace.md:116 |
| D-56 | 메모리 탭 본문 저장 — 프로젝트 파일 평문 vs 별도 `tabs/<id>.txt`(권장) — 답 없음(뒤에 67·D-144 · 70이 같은 주제를 다시 다룸) | ⏳ 대기 | 36-find-in-files-and-project.md:98 §6 · journal/2026-09-16.md:94 |
| D-57 | 기본 프로젝트(워크스페이스) 자동 생성으로 hot exit 켬 — 채택 (09-22) | ✅ 확정 | 36-find-in-files-and-project.md:99 §6 · 67-project-workspace.md:53 |
| D-63 | 설정 `sql.key_mode` = `pk`(기본) \ ¦ `all` — GUI Copy SQL · CLI 공통 (09-16 · 결정 기록 D-62~66 범위 행에 포함) | ✅ 확정 | 41-sql-copy-key-rules.md:28 §2 · 10-decision-record.md:64 |
| D-64 | 키 대체 경고는 요청당 1회 — CLI = 결과 끝 SQL 주석 · GUI = 상태줄+로그 (09-16 · 범위 행 포함) | ✅ 확정 | 41-sql-copy-key-rules.md:29 §2 · 10-decision-record.md:64 |
| D-65 | 테이블 추정 실패 = `T` + 경고 · INSERT는 키 경고 없음 (09-16 · 범위 행 포함) | ✅ 확정 | 41-sql-copy-key-rules.md:30 §2 · 10-decision-record.md:64 |
| D-66 | 키 조회 = 필요할 때 1회(테이블당 캐시 · 접속마다 초기화) (09-16 · 범위 행 포함) | ✅ 확정 | 41-sql-copy-key-rules.md:31 §2 · 10-decision-record.md:64 |
| D-67 | DBeaver식 가상 키(테이블별 사용자 지정 · T-92) — 3컬럼 경고가 잦으면 도입 · 후속 | ⏳ 대기 | 41-sql-copy-key-rules.md:32 §2 · TODO.md:321 |
| D-71 | 결과 탭 = 편집기 탭 ↔ 결과 패널(탭 여러 개) · Ctrl+Enter 교체 · Ctrl+\ 새 탭 (09-16 · 결정 기록 D-68~72 범위 행에 포함) | ✅ 확정 | 43-fetch-model-and-result-tabs.md:13 §0 · 10-decision-record.md:90 |
| D-72 | 메모리 예산 `grid.memory_budget_mb` 1024 · 탭마다 독립(09-17 사용자 정정) (09-16 · 범위 행 포함) | ✅ 확정 | 43-fetch-model-and-result-tabs.md:14 §0 · 10-decision-record.md:90 |
| D-73 | `grid.result_tabs` 켬(기본)/끔 — 끄면 활성 탭 외 즉시 해제 · T-93으로 구현 · 확정 문구 없음 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:166 §4-3a · journal/2026-09-16.md:506 |
| D-74 | 결과 탭 바 표시 `grid.result_tabbar` = auto(2개 이상일 때만 · 기본)/always · T-93 구현 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:167 §4-3a · journal/2026-09-16.md:506 |
| D-75 | 결과 탭 바 = 단일 행 고정 + 우클릭 메뉴(닫기·고정·이름·이동) · T-93 구현 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:168 §4-3a · journal/2026-09-16.md:506 |
| D-78 | 포터블 배포 모드(exe 옆 `data\` = 사용자 폴더 · zip 채널) — 재검토 중 · T-106 ☐ | ⏳ 대기 | 10-decision-record.md:67 §3 · 33-distribution-and-packaging.md:11 · MILESTONES.md:66 |
| D-88 | 확장 Python 지원 범위 — ② 사용자 python3 + 격리 프로세스(권장) — 답 대기 | ⏳ 대기 | 50-extension-system.md:98 §6 · 68-extension-developer-api.md:147 |
| D-89 | 확장 인덱스 서명·출처 — ② 공식 + 사용자 추가 URL(서명 필수 · 권장) — 답 대기 · 미구현 | ⏳ 대기 | 50-extension-system.md:99 §6 · 75-extension-sdk-and-dynamic-loading.md:144 |
| D-90 | 확장 능력 승인 시점 — ③ 설치 때 표시 + 위험 능력은 첫 사용 때 재확인(권장) — 답 대기 · 미구현 | ⏳ 대기 | 50-extension-system.md:100 §6 · 75-extension-sdk-and-dynamic-loading.md:144 |
| D-91 | 레인보우 괄호 구현 형태 — ① 코어 + in-process 플러그인 층 (09-17) | ✅ 확정 | 51-rainbow-brackets.md:173 §7 · 51-rainbow-brackets.md:181 |
| D-92 | 쌍 집합 — ① `()[]{}` + 인용부호 · `<>` 제외 (09-17) | ✅ 확정 | 51-rainbow-brackets.md:174 §7 · 51-rainbow-brackets.md:181 |
| D-93 | 괄호 이동 키 — ① Ctrl+Alt+, / .(형제) · Ctrl+Alt+[ / ](상위/하위) (09-17) | ✅ 확정 | 51-rainbow-brackets.md:175 §7 · 51-rainbow-brackets.md:181 |
| D-94 | 1순위 범위 — ② 색+짝 없음+현재 쌍+이동 메뉴+자동 닫기/감싸기 (09-17) | ✅ 확정 | 51-rainbow-brackets.md:176 §7 · 51-rainbow-brackets.md:181 |
| D-95 | 괄호 팔레트 — ① 테마 기본 6색(사용자 색·종류별 풀은 설정 옵션) (09-17) | ✅ 확정 | 51-rainbow-brackets.md:177 §7 · 51-rainbow-brackets.md:181 |
| D-97 | 개별 모드의 접속 창 Connect = 활성 탭 세션 + 새 탭 기본 스펙 갱신 (09-18 · 결정 기록 D-96~108 범위 행에 포함) | ✅ 확정 | 52-session-modes.md:308 §10 · 10-decision-record.md:128 |
| D-98 | `CONNECT` 접두 강제 없음 · `:connect`/`:disconnect`만(psql `\c`는 T-52) (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:309 §10 · 10-decision-record.md:128 |
| D-99 | 공유 탭에서 `CONNECT` 앞에 서버로 갈 문장이 있으면 실행 거부 + 안내 (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:310 §10 · 10-decision-record.md:128 |
| D-101[52] | 공유 연결 N = 추가(상한 8 · 사용자 09-18) (09-18 · 범위 행 D-96~108 포함) | ✅ 확정 | 52-session-modes.md:312 §10 · journal/2026-09-18.md:29 |
| D-101[56] | 읽기 트랜잭션 자동 종료 동작 = ROLLBACK (09-19 · 범위 행 D-100~105 포함) | ✅ 확정 | 56-manual-commit-lock-prevention.md:140 §8 · journal/2026-09-19.md:159 |
| D-102[52] | 유휴 닫기 = 전용 세션만 30분 · 공유는 설정 (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:313 §10 · 10-decision-record.md:128 |
| D-102[56] | L2 기본 = 30분 뒤 카운트다운 자동 롤백(사용자 선택 ② · 권장은 경고만) (09-19 · 범위 행 포함) | ✅ 확정 | 56-manual-commit-lock-prevention.md:141 §8 · journal/2026-09-19.md:159 |
| D-104[52] | 뒤에서 끝난 실행 = 탭 제목 앞 ✓/✗ (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:315 §10 · journal/2026-09-18.md:38 |
| D-104[56] | L4 서버 타임아웃 기본 = 0(안 보냄) (09-19 · 범위 행 포함) | ✅ 확정 | 56-manual-commit-lock-prevention.md:143 §8 · journal/2026-09-19.md:159 |
| D-105[52] | 트랜잭션 로그 = 전 세션 합본 + "세션" 열(세션 ≥2일 때만) (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:316 §10 · journal/2026-09-18.md:38 |
| D-105[56] | 잠금 방지 범위 = L1~L4 한 번에(사용자 선택 ① · 권장은 L1+L2 먼저) (09-19 · 범위 행 포함) | ✅ 확정 | 56-manual-commit-lock-prevention.md:144 §8 · journal/2026-09-19.md:159 |
| D-107[52] | "같은 서버" = 방언·호스트·포트·DB·계정·역할까지 같을 때 (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:318 §10 · 10-decision-record.md:128 |
| D-107[57] | 트랜잭션 DDL 방언의 수동 커밋 = 커밋 때 탐색기 갱신 (09-19 · 범위 행 D-106~110 포함) | ✅ 확정 | 57-explorer-refresh-after-ddl.md:78 §6 · 10-decision-record.md:87 |
| D-108[52] | 묶인 탭의 연결이 끊기면 끊김 표식으로 남김(자동 이동 없음) (09-18 · 범위 행 포함) | ✅ 확정 | 52-session-modes.md:319 §10 · 10-decision-record.md:128 |
| D-108[57] | 탐색기 유휴 주기 갱신 = 300초 · 바뀐 스키마만 (09-19 · 범위 행 포함) | ✅ 확정 | 57-explorer-refresh-after-ddl.md:79 §6 · 10-decision-record.md:87 |
| D-110[53] | TCP keepalive 켬(60초 · PG·MSSQL · `net.keepalive_secs`) (09-18 · 범위 행 D-109~114 포함) | ✅ 확정 | 53-connection-liveness.md:111 §6 · 53-connection-liveness.md:4 |
| D-110[57] | 새 객체 표시 = 2초 옅은 강조(선택 이동 없음) (09-19 · 범위 행 D-106~110 포함) | ✅ 확정 | 57-explorer-refresh-after-ddl.md:81 §6 · 10-decision-record.md:87 |
| D-111 | Oracle 호출 타임아웃 기본 끔(`session.call_timeout_secs` 0 · 키는 둠) (09-18 · 범위 행 포함) | ✅ 확정 | 53-connection-liveness.md:112 §6 · 53-connection-liveness.md:127 |
| D-112 | Broken 뒤 재접속 = 다음 실행 때 자동 + 세션 상태 소실 안내(미커밋이 있으면 묻기) (09-18 · 범위 행 포함) | ✅ 확정 | 53-connection-liveness.md:113 §6 · 53-connection-liveness.md:124 |
| D-113 | OS 네트워크 변경 신호(`nexa-sys`) 넣음 — 구현은 T-130 잔여(지금은 `probe.stale_secs`가 대신) (09-18 · 범위 행 포함) | ✅ 확정 | 53-connection-liveness.md:114 §6 · 53-connection-liveness.md:130 |
| D-114 | Broken 세션 = 끊김 표식·빨강 플러그로 남김(즉시 해제 아님) (09-18 · 범위 행 포함) | ✅ 확정 | 53-connection-liveness.md:115 §6 · 53-connection-liveness.md:123 |
| D-115 | 비밀번호 없는 접속(PG `trust` · Oracle OS 인증) 허용 스위치 — 권장 = 두지 않음 · 확인 대기 | ⏳ 대기 | 22-driver-extensions.md:238 §10 · STATUS.md:355 · 61-core-design-and-working-rules.md:321 |
| D-116 | 로그인 필수 표기 = 라벨 `*`(권장) — T-133으로 `*`·경고 띠 구현 · 문서 표기는 확인 대기 | 🚧 권장안 진행 | 22-driver-extensions.md:238 §10 · CLAUDE.md:25 · STATUS.md:355 |
| D-117 | 탐색기 루트에 참조 탭 수 표시(`· 3 tabs` 권장) — 답을 받으면 구현 · TODO ☐ | ⏳ 대기 | 54-connection-model-and-disconnect.md:93 §8 · TODO.md:308 |
| D-118 | 툴바 Disconnect `SharedAsk` 팝업 기본 항목 = "모두 해제"(권장) — 결정 대기 | ⏳ 대기 | 54-connection-model-and-disconnect.md:94 §8 · 61-core-design-and-working-rules.md:321 |
| D-120 | 외부 변경 확인 UI = 비모달 띠 + 저장 때만 확인(구현은 "3초 안에 한 번 더" 관례) (09-19 · 결정 기록 D-119~124 범위 행에 포함) | ✅ 확정 | 58-external-change-policy.md:126 §4 · 58-external-change-policy.md:151 |
| D-121 | 여러 번 바뀔 때 = 파일당 띠 하나 갱신 + "유지"는 디스크 서명에 묶기 (09-19 · 범위 행 포함) | ✅ 확정 | 58-external-change-policy.md:127 §4 · 58-external-change-policy.md:4 |
| D-122 | 감지 시점 = 활성화·탭 전환·저장 직전 + 활성 창의 보이는 탭 2초 폴링 (09-19 · 범위 행 포함) | ✅ 확정 | 58-external-change-policy.md:128 §4 · 58-external-change-policy.md:4 |
| D-123 | 감시 방식 = stat 서명 + 해시 · 전용 스레드 · 의존 0(포트만 열어 둠) (09-19 · 범위 행 포함) | ✅ 확정 | 58-external-change-policy.md:129 §4 · 58-external-change-policy.md:4 |
| D-124 | 3-way 병합 = 자체 구현(`diff_lines` 재사용 · 의존 0 · `diffy` 불채택) (09-19 · 범위 행 포함) | ✅ 확정 | 58-external-change-policy.md:130 §4 · journal/2026-09-19.md:196 |
| D-134 | `input.hangul_compose` 기본값 = auto(권장 · 구현됨) — 사용자 확인 대기로 남음 | 🚧 권장안 진행 | 62-macos-input-and-present.md:116 §6 · 61-core-design-and-working-rules.md:321 · TODO.md:303 |
| D-139 | `SELECT … INTO` 0행·여러 행 = Oracle식 오류(`vars.into_policy`) — 구현 · 권장안으로 진행 | 🚧 권장안 진행 | 63-variable-management.md:145 §7 · 63-variable-management.md:137 · journal/2026-09-21.md:50 |
| D-140 | 비밀 값 = `ACCEPT HIDE` + 이름 규칙 → 가림·기록/저장 안 함 — 이름 규칙 구현 | 🚧 권장안 진행 | 63-variable-management.md:146 §7 · 63-variable-management.md:116 |
| D-141 | `&` 치환 = Oracle 방언 탭에서만 기본 켬 · 문자열 안은 선택 · 상태줄 토글 | 🚧 권장안 진행 | 63-variable-management.md:147 §7 · 63-variable-management.md:137 |
| D-142 | 변수 이름 대소문자 무시(처음 쓴 표기 보존) · SQLite `:x @x $x` = 한 변수 — 구현 | 🚧 권장안 진행 | 63-variable-management.md:148 §7 · journal/2026-09-21.md:39 |
| D-143[63] | 변수를 서버에 비추기(SESSION_CONTEXT · `set_config` · `SET @v`) = 변수별 선택 — "뒤로" 미룸 | ⏳ 대기 | 63-variable-management.md:149 §7 · 10-decision-record.md:82 |
| D-143[64] | Instant Client 내려받기 도우미 · 경량 배포판("D-143 후보"로만 언급 · 재배포 조건 확인 뒤) | ⏳ 대기 | 64-dbms-clients-and-driver-packaging.md:116 · journal/2026-09-21.md:250 · TODO.md:281 |
| D-144[67] | 미저장 본문 위치 — ② 사용자 폴더 `backups/` 스냅숏(권장) · 권장안으로 착수(사용자 확인 대기) | 🚧 권장안 진행 | 67-project-workspace.md:148 §6 · journal/2026-09-22.md:225 · TODO.md:288 |
| D-145[67] | 주기 저장 방식 — ③ 하이브리드(1차는 본문 스냅숏만) · 권장안으로 착수(사용자 확인 대기) | 🚧 권장안 진행 | 67-project-workspace.md:149 §6 · journal/2026-09-22.md:225 |
| D-146[94] | 설정 키 2차 rename(`conn.`→`login.` 등 11쌍) + 카테고리 쪼갬 5 — T-250으로 09-29 적용 (문서 불일치: 결정 기록·journal 09-28:99는 대기 · 94 §6-2는 적용 ✅) | 🚧 권장안 진행 | 94-settings-key-naming-and-location.md:96 §6-2 · journal/2026-09-28.md:224 · 10-decision-record.md:81 |
| D-146[67] | 프로젝트 복원 UX — ① 묵시 + 안내(묻지 않음) · 권장안으로 착수(사용자 확인 대기) | 🚧 권장안 진행 | 67-project-workspace.md:150 §6 · 70-autosave-and-restore.md:112 · journal/2026-09-22.md:225 |
| D-147[67] | 프로젝트 탐색기 배치 — ① VS Code식 두 패널 + sash · 권장안으로 착수(사용자 확인 대기) | 🚧 권장안 진행 | 67-project-workspace.md:151 §6 · journal/2026-09-22.md:225 |
| D-147[95] | 포맷터 Basic 콤마 기본 = 줄 앞 — 기본값으로 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 · STATUS.md:119 |
| D-148[67] | 프로젝트 없는 상태 — ① 기본 워크스페이스로 hot exit 유지 · 권장안으로 착수 | 🚧 권장안 진행 | 67-project-workspace.md:152 §6 · 70-autosave-and-restore.md:47 |
| D-148[95] | 포맷터 들여쓰기 = 탭 · 폭 4 — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-149[67] | 프로젝트 파일의 접속 정보 — ① 없음(탭 표식만) · 권장안으로 착수 | 🚧 권장안 진행 | 67-project-workspace.md:153 §6 · journal/2026-09-22.md:225 |
| D-149[95] | 포맷터 시드 = Basic 끔 / kiros33 켬 — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-150[68] | 확장 API 1차 범위 — ① Rainbow Pairs를 WASM으로 옮기는 데 필요한 API만(권장) — 답 대기 | ⏳ 대기 | 68-extension-developer-api.md:141 §7 · TODO.md:291 |
| D-150[95] | 포맷 단축키 = Shift+Alt+F — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-151[68] | Rainbow Pairs 최종 형태 — ② 앱 동봉(`bundled`) + 저장소 갱신(권장) — 답 대기 | ⏳ 대기 | 68-extension-developer-api.md:142 §7 · TODO.md:291 |
| D-151[95] | 포맷 범위 = 선택 > 문서 — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-152[68] | 확장 동적 해제 기본값 — ① 유휴 300초 뒤 내림(향상 모드는 즉시 · 권장) — 답 대기 | ⏳ 대기 | 68-extension-developer-api.md:143 §7 · TODO.md:291 |
| D-152[95] | 포맷 미리보기 = 읽기 전용 탭(설정 변경 즉시 갱신) — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-153[68] | 확장 활성화 모델 — ① VS Code식 활성화 이벤트(선언 기여 + 지연 적재 · 권장) — 답 대기 | ⏳ 대기 | 68-extension-developer-api.md:144 §7 · TODO.md:291 |
| D-153[95] | 확장 포맷터 id `sql-formatter-kiros33` · 설정 접두 `sqlfmt.` — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-154[68] | 확장 SDK 위치·언어 — ① 형제 저장소 `nexa-ext-sdk`(Rust + WIT · 권장) — 답 대기(75 구현은 저장소 안 `extensions/sdk` · WIT 없음) | ⏳ 대기 | 68-extension-developer-api.md:145 §7 · 75-extension-sdk-and-dynamic-loading.md:154 · TODO.md:291 |
| D-154[95] | `sqlfmt.strict` 기본 켬 — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-155[68] | 확장 서명·저장소 신뢰 — ① 공식 인덱스만 서명 · 개인 저장소는 경고(권장) — 답 대기 | ⏳ 대기 | 68-extension-developer-api.md:146 §7 · TODO.md:291 |
| D-155[95] | 포맷터 v1 밖 = 별칭 자동 부여·테이블 주석·PL/SQL 블록·방언 치환(T-255) — 고지 후 진행 | 🚧 권장안 진행 | journal/2026-09-28.md:190 §11 · journal/2026-09-28.md:189 |
| D-156 | 북마크 대상 = 파일 위치 + 객체 DDL 편집창의 위치(객체 자체는 제외 · 3차 정정) (09-22) | ✅ 확정 | 69-bookmarks.md:558 §8 · 69-bookmarks.md:4 |
| D-157 | 북마크 저장 = 하이브리드(프로젝트 워크스페이스 파일 · 없으면 기본 워크스페이스) (09-22) | ✅ 확정 | 69-bookmarks.md:559 §8 · 69-bookmarks.md:4 |
| D-158 | 파일 위치 앵커 = 줄번호 + 내용 앵커로 재탐색 (09-22) | ✅ 확정 | 69-bookmarks.md:560 §8 · 69-bookmarks.md:4 |
| D-159 | 객체 문서 식별 = 접속 좌표 + 스키마·종류·이름(계정 제외 · 서버가 다르면 다른 북마크) (09-22) | ✅ 확정 | 69-bookmarks.md:561 §8 · 69-bookmarks.md:4 |
| D-160 | 끊어진 북마크 = 회색 보관 + `bookmark.stale_days`(30) 뒤 자동 제거 · 부활 시도 (09-22) | ✅ 확정 | 69-bookmarks.md:562 §8 · 69-bookmarks.md:4 |
| D-161 | 북마크 1차 UX 범위 = 전부(이름·메모 + 니모닉 + 그룹 + 패널) (09-22) | ✅ 확정 | 69-bookmarks.md:563 §8 · 69-bookmarks.md:4 |
| D-162 | 니모닉 = 0–9 · 문서별 유일 (09-22) | ✅ 확정 | 69-bookmarks.md:564 §8 · 69-bookmarks.md:4 |
| D-163 | 이동·이름 변경 = 앱 안 자동 추적 · 앱 밖은 내용 해시로 "이어 붙일까요" 제안 (09-22) | ✅ 확정 | 69-bookmarks.md:565 §8 · 69-bookmarks.md:4 |
| D-164 | 팀 공유 = 공용·개인 2단(공용 = 프로젝트 파일 · 기본은 개인) (09-22) | ✅ 확정 | 69-bookmarks.md:566 §8 · 69-bookmarks.md:4 |
| D-165 | 공용 파일의 서버 표기 = 프로필 이름만(호스트·포트·DB·계정 제외) (09-22) | ✅ 확정 | 69-bookmarks.md:567 §8 · 69-bookmarks.md:4 |
| D-166 | DDL 북마크 복원 = 목록에 남기고 클릭할 때 다시 조회 · 유일성 = 서버+객체 경로+유형 (09-22) | ✅ 확정 | 69-bookmarks.md:568 §8 · 69-bookmarks.md:4 |
| D-167 | DDL을 파일로 저장 = 양쪽 다 유지(복사 · `origin` 기억) (09-22) | ✅ 확정 | 69-bookmarks.md:569 §8 · 69-bookmarks.md:4 |
| D-168 | 이름 없는 탭 = 워크스페이스 탭 id로 동일하게 지원 (09-22) | ✅ 확정 | 69-bookmarks.md:570 §8 · 69-bookmarks.md:4 |
| D-169 | 북마크 화면 표시 = 거터 아이콘 · 미니맵 틱 · 인라인 라벨 · 상황줄/배지 넷 다 (09-22) | ✅ 확정 | 69-bookmarks.md:571 §8 · 69-bookmarks.md:4 |
| D-170 | CLI `nsql bookmark list \ ¦ add \ ¦ rm \ ¦ prune` (09-22) | ✅ 확정 | 69-bookmarks.md:572 §8 · journal/2026-09-22.md:485 |
| D-171 | 재배치 판정 = 정확 일치 실패 시 유사도(Sørensen–Dice ≥ 0.6)로 잇기(설계 판단 · 사용자 확인 대상 아님) (09-22) | ✅ 확정 | 69-bookmarks.md:573 §8 · journal/2026-09-22.md:198 |
| D-172 | 북마크 패널 묶음 = 그룹 → 문서 → 항목(문서·평면 전환 가능) (09-22) | ✅ 확정 | 69-bookmarks.md:574 §8 · journal/2026-09-22.md:199 |
| D-173 | 목록 좌클릭 = 미리보기 + 포커스는 목록(더블클릭·Enter = 이동) (09-22) | ✅ 확정 | 69-bookmarks.md:575 §8 · journal/2026-09-22.md:199 |
| D-174 | 그룹 소속 = 한 북마크는 그룹 하나 (09-22) | ✅ 확정 | 69-bookmarks.md:576 §8 · journal/2026-09-22.md:199 |
| D-175 | 설정 자리 = 새 그룹 `Bookmarks` + 분류 셋 (09-22) | ✅ 확정 | 69-bookmarks.md:577 §8 · 69-bookmarks.md:583 |
| D-176 | 미리보기는 절대 접속하지 않는다(설계 판단 · C-4) (09-22) | ✅ 확정 | 69-bookmarks.md:578 §8 · journal/2026-09-22.md:199 |
| D-177 | 제거 되돌리기 = 5초 [실행 취소] 토스트(`bookmark.undo_secs`) · 전체 지우기는 두 번 확인(설계 판단) (09-22) | ✅ 확정 | 69-bookmarks.md:579 §8 · journal/2026-09-22.md:199 |
| D-178 | 참조 파일 백업 판 수 — ① 최신 1개(권장) — 사용자 확인 대기 | ⏳ 대기 | 70-autosave-and-restore.md:166 §8 · TODO.md:288 · CLAUDE.md:17 |
| D-179 | 캐럿·스크롤을 워크스페이스에 쓰는 시점 — ② 탭 전환·포커스 잃음·5초 유휴·종료(권장) — 확인 대기 | ⏳ 대기 | 70-autosave-and-restore.md:167 §8 · TODO.md:288 |
| D-180 | 복원 때 백업 본문 vs 디스크가 다를 때 — ① 묵시로 백업 올리고 띠(권장) · 미저장 스냅숏 복원에 ① 적용 · 사용자 확인은 대기 | 🚧 권장안 진행 | 70-autosave-and-restore.md:168 §8 · journal/2026-09-22.md:582 · TODO.md:288 |
| D-181 | `project.hot_exit` 기본 — ① `always`(권장) — 확인 대기 | ⏳ 대기 | 70-autosave-and-restore.md:169 §8 · TODO.md:288 |
| D-182 | 100개 `Save All` 진행 표시 — ① 상태줄 카운트만(권장) — 확인 대기 | ⏳ 대기 | 70-autosave-and-restore.md:170 §8 · TODO.md:288 |
| D-183 | 백업 폴더 위치 — ① 설정 폴더 `backups/`(권장) — 확인 대기 | ⏳ 대기 | 70-autosave-and-restore.md:171 §8 · TODO.md:288 |
| D-184[43] | 전체 조회의 커서 경로도 실행 카드 표시 = 예 — 권장안 적용 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:314 §11-4 · journal/2026-09-22.md:318 |
| D-184[63] | 바인드 식의 "사용 시" 재계산 = 의존 추적(dirty) — 09-23 최소판 구현(`VarStore.formulas` + `Action::Replan`) (09-23) | ✅ 확정 | 63-variable-management.md:198 §9 · journal/2026-09-22.md:535 |
| D-185[43] | Count 실행 카드 = 예 — 권장안 적용 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:314 §11-4 · journal/2026-09-22.md:318 |
| D-185[63] | 변수별 확장 시점 문법(`DEFINE a == …`) = 보류(설정 `vars.expand_at`만) | ⏳ 대기 | 63-variable-management.md:199 §9 · 63-variable-management.md:194 |
| D-186[43] | 키 조회 실행 카드 = 없음(로그만) — 권장안 적용 | 🚧 권장안 진행 | 43-fetch-model-and-result-tabs.md:314 §11-4 · journal/2026-09-22.md:318 |
| D-186[73] | 프로젝트 경로 앵커 표기 = `${name}/…` 문자열 접두(권장) — 결정 대기 | ⏳ 대기 | 73-project-path-portability.md:141 §6 · TODO.md:276 · STATUS.md:247 |
| D-187 | 등록 폴더 참조 = 이름(`${folder:이름}` · 권장) — 결정 대기 | ⏳ 대기 | 73-project-path-portability.md:142 §6 · TODO.md:276 |
| D-188 | 절대 경로가 남을 때 = 경고 로그 + 저장(`project.warn_absolute` · 권장) — 결정 대기 | ⏳ 대기 | 73-project-path-portability.md:143 §6 · TODO.md:276 |
| D-189 | 못 찾은 탭 = 자리 탭(찾아보기/변수 정의/닫기 · 권장) — 결정 대기 | ⏳ 대기 | 73-project-path-portability.md:144 §6 · TODO.md:276 |
| D-190 | 프로젝트 파일 구조 = 한 파일 v2 절 나누기 + `project.share_workspace`(권장) — 결정 대기 | ⏳ 대기 | 73-project-path-portability.md:145 §6 · TODO.md:276 |
| D-191 | 프로젝트 모드의 변수 = 프로젝트 파일 탭 블록(파일 `vars/` 안 씀 · 권장) — 결정 대기 | ⏳ 대기 | 74-vars-persistence-identity.md:120 §6 · TODO.md:170 · journal/2026-09-22.md:689 |
| D-192 | 파일·폴더 모드 1차 열쇠 = 경로 유지 + 보조 = 파일시스템 id(권장) — 결정 대기 | ⏳ 대기 | 74-vars-persistence-identity.md:121 §6 · TODO.md:170 |
| D-193 | 지문 불일치 후보 = 제시만(자동 적용 안 함 · 권장) — 결정 대기 | ⏳ 대기 | 74-vars-persistence-identity.md:122 §6 · TODO.md:170 |
| D-194 | 문서 id 주석(`-- nsql:doc …`) = 옵트인(권장) — 결정 대기 | ⏳ 대기 | 74-vars-persistence-identity.md:123 §6 · TODO.md:170 |
| D-195 | Save As 변수 이관 = 복사(이름 없는 탭 → 파일은 이동 · 권장) — 결정 대기 | ⏳ 대기 | 74-vars-persistence-identity.md:124 §6 · TODO.md:170 |
| D-196 | 프로젝트 필터 이름 열거 상한(기본 50,000 + "계속 열거") 제안 — 폐기 · 워커 병렬 열거·`project.scan_max` 0 = 무제한 설계로 대체 (09-23) | ✅ 확정 | journal/2026-09-22.md:730 §114 · journal/2026-09-22.md:772 · journal/2026-09-22.md:777 |
| D-197[75] | WASM 확장이 내장 확장과 같은 id면 WASM이 대체 · 내장은 폴백 (09-23) | ✅ 확정 | 75-extension-sdk-and-dynamic-loading.md:153 §8 · journal/2026-09-22.md:738 |
| D-197[77] | `ObjectRef`에 접속 좌표를 넣는 방식(북마크 69 §2-2 재사용 vs 세션 id) — 결정 대기 | ⏳ 대기 | 77-data-workbench-architecture.md:119 · journal/2026-09-22.md:835 |
| D-198[75] | 확장 ABI v1 = 버퍼+JSON(WIT 없음) · 게스트 상태 없음 · 편집기 조작은 op 큐 (09-23) | ✅ 확정 | 75-extension-sdk-and-dynamic-loading.md:154 §8 · STATUS.md:243 |
| D-198[77] | 키 없는 테이블의 편집 = "전체 열 = 값" 허용(1행 확인 뒤) — 77은 결정 대기 · 87에서 3급(전 열 비교 + 사전 COUNT=1)으로 구현 | 🚧 권장안 진행 | 77-data-workbench-architecture.md:119 · 87-grid-data-editing.md:153 · 87-grid-data-editing.md:193 |
| D-199[75] | 확장 배포 채널 = 저장소 폴더(기본) + Releases 자산(`files[].url`) · 둘 다 sha256 필수 (09-23) | ✅ 확정 | 75-extension-sdk-and-dynamic-loading.md:155 §8 · journal/2026-09-22.md:738 |
| D-199[77] | Excel COM용 `windows` crate 도입(명시 예외 원장 · Windows 전용 feature) — 결정 대기 · T-183 ☐ | ⏳ 대기 | 77-data-workbench-architecture.md:119 · TODO.md:274 |
| D-200[76] | 인텔리센스 1차 후보 출처 = alias 컬럼 · 문장 alias · 스키마 객체 · 문서 심볼 · 키워드 · 문서 단어 — 사용자 확인 없이 진행 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:91 §8 · 76-intellisense-and-outline.md:13 |
| D-200[77] | XLSX writer 자체 구현(zip deflate 포함 · 의존 0) vs 외부 crate — 결정 대기 · T-183 ☐ | ⏳ 대기 | 77-data-workbench-architecture.md:119 · TODO.md:274 |
| D-201[76] | 인텔리센스 팝업 부품 = `ContextMenu` 재사용(새 컨트롤 0) — 사용자 확인 없이 진행 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:92 §8 · journal/2026-09-22.md:747 |
| D-201[83] | 탐색기 Databases 층 = 2차(T-204) · 1차는 접속 DB 하나 — 2차는 문서 101(T-270)로 진행 | 🚧 권장안 진행 | 83-object-explorer-dbms-trees-and-generate-sql.md:78 §5 · 101-mssql-ssms-explorer-databases-layer.md:4 |
| D-202[76] | 후보 계산 = UI 스레드 스냅샷 조회(예산 `intel.budget_ms`) · 워커는 예산 초과 실측 뒤 — 사용자 확인 없이 진행 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:93 §8 · 39-resource-governance.md:216 |
| D-202[83] | 탐색기 Package Bodies 폴더 제거(DBeaver 동일) · 본문은 패키지 노드 소스/DDL — 권장안 | 🚧 권장안 진행 | 83-object-explorer-dbms-trees-and-generate-sql.md:79 §5 |
| D-203[76] | 아웃라인 캐시 열쇠 = 본문 세대 · 유휴 재계산 · L1+ 끔 — 사용자 확인 없이 진행 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:94 §8 · journal/2026-09-22.md:747 |
| D-203[83] | Generate SQL 바인드 표기 = `:컬럼`(엔진 재작성) — 구현 | 🚧 권장안 진행 | 83-object-explorer-dbms-trees-and-generate-sql.md:80 §5 · journal/2026-09-22.md:1041 |
| D-204[76] | Goto Symbol = 팔레트(Ctrl+R) · 아웃라인 = 활동 막대 패널 — 사용자 확인 없이 진행 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:95 §8 · 76-intellisense-and-outline.md:13 |
| D-204[54] | 해제된 연결 행 = 제거(기본) · `explorer.keep_offline`로 유지(사용자 09-25 지시가 09-18 설계보다 나중) (09-25) | ✅ 확정 | 54-connection-model-and-disconnect.md:125 §9 · 54-connection-model-and-disconnect.md:113 · journal/2026-09-22.md:1044 |
| D-205[54] | 서버 헤더 해제 방식 `explorer.disconnect_pick` = auto 기본 — 이후 D-144(09-28 ✅)로 기본 always | 🚧 권장안 진행 | 54-connection-model-and-disconnect.md:125 §9 · journal/2026-09-22.md:1044 · journal/2026-09-28.md:102 |
| D-205[76] | 완성 팝업 페이지 로딩의 끝 항목 = 비활성 안내 한 줄("N개 더") — T-196 구현 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:186 §13 · journal/2026-09-22.md:975 |
| D-206[76] | 완성 팝업 End = 전부 붙인 뒤 마지막(상한 `max_total`) — T-196 구현 | 🚧 권장안 진행 | 76-intellisense-and-outline.md:186 §13 · journal/2026-09-22.md:995 |
| D-206[63] | 변수 층 우선순위 = tab > shared > global > fixed (✅ · 날짜 표기 없음 · 100차 §194) | ✅ 확정 | 63-variable-management.md:238 §11 |
| D-207[63] | 글로벌 변수 자동 저장 = `vars.global_persist` 기본 켬 (✅ · 날짜 표기 없음 · 100차 §194) | ✅ 확정 | 63-variable-management.md:238 §11 |
| D-207[80] | 메모리 창 = 모델리스 + 최상위 스위치(사용자 · 모달 취소) · 메인 소유 (09-24) | ✅ 확정 | 80-memory-monitor.md:51 §6 · journal/2026-09-22.md:977 |
| D-208[54] | 같은 탭 `CONNECT` 두 번 = 앞 연결이 사라짐 — ① 모델 유지 + 로그 안내(권장) · 결정 대기 | ⏳ 대기 | 54-connection-model-and-disconnect.md:155 §10-5 · journal/2026-09-22.md:1051 |
| D-208[80] | 상태줄 메모리 숫자 = 풋프린트(mac) / Private Bytes(Win) / resident−shared(Linux) — 구현 | 🚧 권장안 진행 | 80-memory-monitor.md:52 §6 · journal/2026-09-22.md:977 |
| D-209[80] | 메모리 데이터 카테고리는 어림 · 차이는 "기타" · 할당자 후킹 안 함 — 구현 | 🚧 권장안 진행 | 80-memory-monitor.md:53 §6 · journal/2026-09-22.md:977 |
| D-209[85] | `explorer.index_idle_ms` 기본 5000 → 250 · `index_prefetch` = 접속 직후 L1 채움 — 한 줄 고지 | 🚧 권장안 진행 | 85-metadata-layers.md:92 · 85-metadata-layers.md:85 · journal/2026-09-22.md:1066 |
| D-210 | 그리드 편집 빈 문자열 커밋 기본 = NULL(`grid.edit_empty`) — 결정 대기로 표기(설정 표에는 기본 NULL로 설계) | ⏳ 대기 | 87-grid-data-editing.md:182 §9 · 87-grid-data-editing.md:70 · journal/2026-09-22.md:1106 |
| D-211 | 복제 행의 키 열 = 비움 vs 원본 값 복사 — 결정 대기로 표기(설정 `grid.edit_dup_keys` on으로 설계) | ⏳ 대기 | 87-grid-data-editing.md:182 §9 · 87-grid-data-editing.md:164 |
| D-212 | 적용 뒤 기본 = 재조회 vs 로컬 반영 — 결정 대기로 표기 · 이후 D-219로 `grid.edit_refresh=rows` 기본(한 줄 고지) | 🚧 권장안 진행 | 87-grid-data-editing.md:182 §9 · 87-grid-data-editing.md:271 |
| D-213 | LOB 인라인 편집 허용 범위(CLOB 텍스트만 · BLOB은 파일 넣기) — 결정 대기 | ⏳ 대기 | 87-grid-data-editing.md:182 §9 · journal/2026-09-22.md:1106 |
| D-214 | 키 열이 결과에 없을 때 = 숨은 키 열 자동 주입(재조회 1회 · `grid.edit_hidden_keys`) — T-231 구현 · 이후 D-225로 기본 off | 🚧 권장안 진행 | 87-grid-data-editing.md:324 §13-6 · TODO.md:216 · 87-grid-data-editing.md:365 |
| D-215 | 키 값 수정 = 허용 + 변경 집합 안 중복 사전 검사 + 키 변경 UPDATE 먼저 — 구현 | 🚧 권장안 진행 | 87-grid-data-editing.md:325 §13-6 · 87-grid-data-editing.md:339 · journal/2026-09-22.md:1136 |
| D-216 | 전 열 비교의 제외 열 규칙(LOB·실수·긴 문자·xml/json·공간형) + 사전 건수 검사 기본 on — 구현 | 🚧 권장안 진행 | 87-grid-data-editing.md:326 §13-6 · 87-grid-data-editing.md:336 |
| D-217 | PG ctid 채택 시 적용 뒤 행 재조회 필수 · xmin 동시성 기본 on — 구현(0행이면 전체 재조회 폴백) | 🚧 권장안 진행 | 87-grid-data-editing.md:327 §13-6 · 87-grid-data-editing.md:272 |
| D-218 | SQL Server 힙 = `%%physloc%%` 미사용(3급/읽기 전용) — 구현 | 🚧 권장안 진행 | 87-grid-data-editing.md:328 §13-6 · 87-grid-data-editing.md:337 |
| D-219 | `grid.edit_refresh` 기본값 = `rows`(바뀐 행만 제자리 재조회) — 기본값 변경 한 줄 고지 (journal 09-22:1134는 같은 번호를 "단어 이동 스위치 묶을지"에도 씀) | 🚧 권장안 진행 | 87-grid-data-editing.md:264 §12-4-a · journal/2026-09-22.md:1146 · CLAUDE.md:14 |
| D-220 | 벌크 적재 = INSERT 전용(UPDATE/DELETE는 87 §14 규칙 유지) — 권장 ✅ · 한 줄 고지로 진행 | 🚧 권장안 진행 | 89-bulk-io-review.md:150 §3-5 · 89-bulk-io-review.md:144 · journal/2026-09-22.md:1174 |
| D-221 | SQL Server bulk 기본 `CHECK_CONSTRAINTS` on · `FIRE_TRIGGERS` off — 한 줄 고지로 진행 | 🚧 권장안 진행 | 89-bulk-io-review.md:151 §3-5 · 89-bulk-io-review.md:144 |
| D-222 | PG COPY 형식 — 제안은 binary 기본 · 구현은 text(binary는 후속) — 한 줄 고지로 진행 | 🚧 권장안 진행 | 89-bulk-io-review.md:152 §3-5 · 89-bulk-io-review.md:144 |
| D-223 | 벌크 커밋 간격 기본 10,000행(COPY는 문장 원자) — 한 줄 고지로 진행 | 🚧 권장안 진행 | 89-bulk-io-review.md:153 §3-5 · 89-bulk-io-review.md:144 |
| D-224 | 위험 옵션(NOLOGGING/APPEND · 제약 끄기)은 기본 끔(`bulk.append_hint`) — 한 줄 고지로 진행 | 🚧 권장안 진행 | 89-bulk-io-review.md:154 §3-5 · 89-bulk-io-review.md:144 |
| D-225 | 그리드 편집 재조회는 기본 안 함 — `grid.edit_hidden_keys`·`grid.edit_rowid` 기본 off · 필요 시 ⚿ 행 식별 열 가져오기 (09-27) | ✅ 확정 | 87-grid-data-editing.md:362 §15-1 · journal/2026-09-26.md:195 · CLAUDE.md:17 |
| D-226 | 그리드 적용 사전 검사 범위 `grid.edit_precheck`(1·2급 생략 · 3급만 유지 권장) — 결정 대기 | ⏳ 대기 | journal/2026-09-26.md:193 |
| D-227 | 분석 지연 기법(난독화·안티디버깅·자기 무결성·문자열 암호화) 도입하지 않음 (09-27) (문서 불일치: 92 §5 제목은 결정 대기 · journal·STATUS·CLAUDE는 확정) | ✅ 확정 | 92-digital-asset-protection.md:87 §5 · journal/2026-09-27.md:205 · STATUS.md:220 |
| D-228 | 로컬 보강 넷(R1~R4) 채택 — 앱 층에 포함 (09-27) | ✅ 확정 | 92-digital-asset-protection.md:88 §5 · journal/2026-09-27.md:205 · STATUS.md:220 |
| D-229 | 서버 검증 착수 시점 = Org 첫 고객 확정 뒤(서버 보류) (09-27) | ✅ 확정 | 92-digital-asset-protection.md:89 §5 · journal/2026-09-27.md:205 · journal/2026-09-27.md:284 |
| D-230 | 코드 서명 필수 승격 = 1.0 전 재검토(지금은 별도 요청 유지) (09-27) | ✅ 확정 | 92-digital-asset-protection.md:90 §5 · journal/2026-09-27.md:205 · CLAUDE.md:13 |
| D-231 | 서버 전까지 `kind=org` 파일 단독 설치 = 거부 + 안내(권장) — 결정 대기 | ⏳ 대기 | 25-license-tiers-and-server.md:154 §4-4 · journal/2026-09-27.md:287 · STATUS.md:196 |
| D-232~D-238 | 저장소 문서 어디에도 나오지 않는 번호(건너뜀) | ❓ 미확인 | (출현 없음) |
| D-239 | `output.serveroutput` 기본 켬(Oracle 새 세션마다 `SET SERVEROUTPUT ON`) — 한 줄 고지 | 🚧 권장안 진행 | journal/2026-09-30.md:23 · 100-object-source-run-and-output-tab.md:84 · 39-resource-governance.md:68 |
| D-240 | 객체 삭제 백업 폴더 `<설정>/backups/drop/` + 보관 `project.backup_days` 공용 — 권장안 적용 · 사용자 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · STATUS.md:59 · journal/2026-10-02.md:10 |
| D-241 | DROP 문 = 옵션 없는 보수형(CASCADE/PURGE 없음) — 권장안 적용 · 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · 28-object-explorer.md:197 · journal/2026-10-02.md:10 |
| D-242 | 삭제 세션 = 그 서버의 접속된 세션(없으면 "먼저 접속") — 권장안 적용 · 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · 28-object-explorer.md:197 · journal/2026-10-02.md:10 |
| D-243 | 접속 유형 지정 = 세션 + 저장 프로필 → 정정: 세션 메뉴·`CONNTYPE`은 임시(`env_temp` · 프로필 저장 안 함) — 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · journal/2026-09-30.md:102 · journal/2026-10-02.md:10 |
| D-244 | 삭제 전 백업 DDL = Generate SQL ▸ DDL 전체(full · qualified) — 권장안 적용 · 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · 28-object-explorer.md:195 · journal/2026-10-02.md:10 |
| D-245 | 삭제 확인 타임아웃 = `login.delete_confirm_ms` — 권장안 적용 · 확인 대기 | ⏳ 대기 | journal/2026-09-30.md:89 §9 · 28-object-explorer.md:194 · journal/2026-10-02.md:10 |
| D-246 | SQL Server 트리 골격 = SSMS(사용자 명시 · `explorer.mssql_tree = ssms\ ¦ schema`) — 권장안으로 진행 · 사용자 확인 대기 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:94 §6 · journal/2026-09-30.md:127 |
| D-247 | SSMS 폴더 중 다이어그램·Service Broker·스냅샷·PolyBase·Always On·관리·XEvent 생략 — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:95 §6 · journal/2026-09-30.md:127 |
| D-248 | 현재 DB 추적 = 러너 `USE` 파싱(왕복 0) + 접속 1회 `DB_NAME()` — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:96 §6 · journal/2026-09-30.md:129 |
| D-249 | 다른 DB 객체의 완성 = 1차 현재 DB만 · 3부 접두 완성은 2차 — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:97 §6 · journal/2026-09-30.md:128 |
| D-250 | 현재 DB가 바뀌면 메타 = 비우고 다시(L1 → L2) — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:98 §6 · journal/2026-09-30.md:129 |
| D-251 | 시스템 DB = 하위 폴더로 분리(`explorer.mssql_system_dbs`로 숨김 가능) — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:99 §6 · journal/2026-09-30.md:127 |
| D-252 | 연결된 서버 수정 = Open script(재생성 스크립트 · 대화상자 없음) — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:100 §6 |
| D-253 | 서버 헤더 = `호스트:포트 (제품 버전 - 로그인)` · 전 방언 같은 틀 — 권장안으로 진행 | 🚧 권장안 진행 | 101-mssql-ssms-explorer-databases-layer.md:101 §6 · journal/2026-09-30.md:130 |
| D-254 | SQL Server 선언 없는 변수의 서버 타입 보존 — 처음 값을 받는 `Auto` 변수를 `NVARCHAR(4000)`이 아니라 **`sql_variant`**로 선언 + 꼬리 행 `[이름$type]`으로 타입 복원(T-162 ⑤) · 기본값 변경(한 줄 고지) · 한계 = `(n)varchar(max)`·`xml`·`text` 값은 못 받음(종전 = 4000자 절단 · 지금 = 서버 오류) → `VARIABLE v VARCHAR2(8000)` 선언으로 우회 · 번호 = 16 §2-9 규칙 첫 적용(저장소 전체 grep 뒤 D-254) (10-05) | 🚧 권장안 진행 | journal/2026-10-05.md §17 · 63 §3-1 · TODO T-162 |
| L-1 | 발급 PC OS와 봉투 방식 = ⓐ(3-OS 동일) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:450 §12-7 · 25-license-tiers-and-server.md:446 · journal/2026-09-27.md:205 |
| L-2 | tier 프리셋 = §11-3 게이트 표(25·D-41)로 고정 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:451 §12-7 · journal/2026-09-27.md:205 |
| L-3 | 라이선스 ID 체계 `NSL-2026-000001` · 대장은 발급 PC 로컬(비공개 저장소에도 안 올림) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:452 §12-7 · journal/2026-09-27.md:205 |
| L-4 | 기본 기간 = updates 1년 · 체험 14일 · Org 구독 1년 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:453 §12-7 · journal/2026-09-27.md:205 |
| L-5 | 전달 채널 = 이메일 수동 + `mail.txt` 템플릿(한/영) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:454 §12-7 · journal/2026-09-27.md:205 |
| L-6 | 재발급 정책 = 연 5회 · 기기 교체 시 옛 기기 제거 (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:455 §12-7 · journal/2026-09-27.md:205 |
| L-7 | `nexa-license-server` 저장소 생성 시점 — "서버 보류로 대체"(발급기는 nexa-license 워크스페이스에) (09-27) | ✅ 확정 | 25-license-tiers-and-server.md:456 §12-7 · 25-license-tiers-and-server.md:446 · journal/2026-09-27.md:58 |
| L-8 | 서명 코드 위치(25 §11-1 "검증 전용" vs §12-1 `issuer` feature) — 결정 대기(권장 = §11-1 유지 · CLAUDE.md:13은 nexa-license `issuer` = `sign`으로 구현됐다고 적음) | ⏳ 대기 | journal/2026-09-27.md:82 · STATUS.md:214 · journal/2026-09-27.md:115 |

## 4. 외부 crate 원장 (추가 시 건별 기록)

| crate | 계층·크레이트 | 사유 | 라이선스 | 상태 |
|---|---|---|---|---|
| `ab_glyph` | ④ nexa-gfx | 계승 | Apache-2.0 | ✅ |
| `fancy-regex` 0.14(+ `regex` 1) | ① nexa-sql `rx.rs`(찾기/바꾸기 · 파일 찾기 · 패턴 기능 공용) | **D-76** — 선형 시간 코어 + lookaround/역참조만 백트래킹 · 순수 Rust(DR-3 예외) · 자체 NFA(T-59) 대체 | MIT | ✅ 09-16 |
| `x11rb` 0.13(winit의 기존 의존 · RustConnection · 기본 기능만) | ① nexa-sql `winfocus.rs`(Linux X11 `WM_TRANSIENT_FOR` + `_NET_WM_STATE_MODAL` — 모달을 메인에 붙임) · ② `clipboard_x11.rs`(09-26 101차 · `CLIPBOARD` selection을 앱이 직접 소유/조회 — 종전 `wl-copy`/`xclip`/`xsel` 외부 프로그램 의존을 걷어냄 · INCR · 설정 `clipboard.x11_native`) | 09-22 91차 — winit 0.30 Wayland/X11 API가 부모 창·창 올리기를 못 준다 · 새 코드 0바이트(이미 링크됨) · Linux 전용 target 의존 | MIT/Apache | ✅ 09-22 |
| `encoding_rs` 0.8 | ① nexa-sql(파일 인코딩 · EUC-KR·Shift_JIS·EUC-JP·GB18030·Big5·CP1252) | DR-3 예외 — 순수 Rust · Firefox 코덱 · 표를 직접 만들 이유 없음(09-16) | Apache-2.0/MIT | ✅ 09-16 |
| `oracle` 0.6 | ① driver-oracle | DP-3 | UPL/Apache | ✅(10-05 정합 · `nsql-driver-oracle/Cargo.toml` 확인) |
| `tiberius` 0.12(계획 때 이름 = `tiberius-ng` · features `tds73`·`rustls`·`chrono`) | ① driver-mssql | DP-4 | MIT/Apache | ✅(10-05 정합 · `nsql-driver-mssql/Cargo.toml` 확인) |
| `rustls` (+webpki-roots) | ② nsql-net | TLS | MIT/Apache/ISC | ☐ 직접 의존 없음(10-05 정합 — tiberius의 `rustls` feature로만 전이 · `nsql-net` 크레이트 없음) |
| `russh` | ② nsql-net | SSH 터널 | Apache-2.0 | ☐ M2 |
| `rustybuzz` | ④ nexa-gfx | DP-6 셰이핑 | MIT | ☐ 미도입(10-05 정합 — 두 저장소 `Cargo.toml`에 없음 · 한글 = D-77 GDI 글리프·앱 조합 경로 · 도입 계획이 살아 있는지는 미확인) |
| `syntect` | ④ nexa-edit | `.sublime-syntax` | MIT | ☐ M3 |
| `wasmi` 1.1 | ④ nexa-sql `extensions/wasm.rs` | DR-16 · D-87 ① · [75](75-extension-sdk-and-dynamic-loading.md)(인터프리터 · 연료/메모리 상한 내장 · nexa-dir2와 같은 판) | MIT/Apache | ✅ 09-23 |
| `postgres` 0.19(동기 · tokio-postgres 위) | ① driver-pg | DR-28 · DR-3 예외 | MIT/Apache | ✅ 09-15 |
| `tracing` 0.1 | ① driver-mssql | tiberius Info 토큰(PRINT) 캡처 — 이미 전이 의존 | MIT | ✅ 09-15 |
| `mysql_async` · `rusqlite(bundled)` · `odbc-api` | ① | DP-2 | MIT/Apache | rusqlite ✅ · 나머지 ☐ M4 |
| `chacha20poly1305` 0.10 · `sha2` 0.10 · `getrandom` 0.2 | Core 옆 `nsql-vault` | DR-22 비밀번호 봉투 AEAD · 키 KDF · OS 난수 — ★ **암호화 자체 구현 금지 부류**, nexa-clip `nclip-store` 원장과 동일 판(RustCrypto) | MIT/Apache-2.0 | ✅ 09-13 |
| `objc2` 0.5 · `objc2-app-kit` 0.2 · `objc2-foundation` 0.2(**macOS 타깃 한정**) | ④ nexa-sql `icon.rs` | Dock 아이콘(`NSApplication.setApplicationIconImage` · 번들 없는 개발 실행) — winit과 **같은 판**(Cargo.lock 동일 항목 · 트리 중복 0) · nexa-clip `nclip-plat` 선례 | MIT | ✅ 09-16 |
