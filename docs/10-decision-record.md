# 10 · 결정 기록 (DR) · 권장 확정 대기 (DP) · 열린 결정 (D)

> 확정은 DR, 내가 낸 결론으로 사용자 확인이 남은 것은 DP, 미정은 D. 변경 시 과거를 지우지 않고 새 항목으로 정정. **최신 갱신 2026-09-13.**

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
| **DR-27** | ★ **배포 = 설치본만 · 포터블 없음** — 목적별 실행파일(GUI `nexa-sql` · CLI `nsql` · 드라이버 프로세스) · Rust 코어는 정적 링크 · OS 런타임/드라이버 라이브러리는 공유 lib으로 별도 위치 · macOS `.app`(Universal 2 · Frameworks/@rpath · Application Support) · Windows MSI · Linux deb/rpm · 사용자 데이터는 OS 사용자 폴더(`NSQL_HOME`은 개발용) | 사용자 09-15 *"포터블 배포는 하지 않을 것 · 목적별 실행파일 분리 · 공유/정적 라이브러리 별도 구성 · 설치본 · macOS 특징 고려"* · [33](33-distribution-and-packaging.md) | ✅ 09-15(설계 · 구현 T-72) |
| **DR-28** | **PostgreSQL = 내장 드라이버 1급 지원**(`nsql-driver-pg` · rust-postgres 동기 · DR-3 예외 원장) — Oracle·MSSQL·SQLite와 같은 엔진 관용(세션 변수 · SELECT INTO 별칭 · CALL OUT) · 카탈로그·탐색기·CLI 동등 | 사용자 09-15 *"postgresql까지 지원 범위를 확대"* | ✅ 09-15(matrixdb2 실서버) |
| **DR-29** | **내장 드라이버는 정적 링크 유지 · 확장 드라이버는 in-process 동적 라이브러리(C ABI cdylib · 지연 로드)** — 실측(09-15 release): CLI 기동 ~20ms · GUI 사설 메모리 8.5MB · 드라이버 전역 초기화 0(tokio 런타임·ODPI-C는 첫 접속 시) → 정적 링크는 기동·메모리·속도 비용이 없다(코드는 요구 페이징 · 안 쓰는 드라이버 페이지는 적재되지 않음). 파일 분리가 필요한 경우(GitHub 다운로드·SxS Instant Client·재설치 없는 갱신)만 **같은 프로세스 안에서 `dlopen`하는 cdylib**(IPC 0 · 직접 호출) — DR-24의 stdio JSON-RPC **프로세스**는 격리가 꼭 필요한 선택 모드로 강등(같은 ABI를 프로세스 호스트가 감싼다) | 사용자 09-15 *"로딩 시간·불필요한 메모리·속도 지연 없는 구조"* · *"exe 분리가 아니라 동적 라이브러리/플러그인"* · *"분리가 필요없는 수준이면 지금도 괜찮아"* · [22 §0](22-driver-extensions.md) | ✅ 09-15(구조 확정 · cdylib 구현 = T-27 개정) |

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
| D-49 | Windows 설치기 — MSI(WiX · 조용한 설치·GPO · 권장) vs NSIS(nexa-clip 보유)([33 §4](33-distribution-and-packaging.md)) |
| D-50 | macOS CLI 노출 — pkg 설치 스크립트로 `/usr/local/bin/nsql` 링크(권장) vs Homebrew만 |
| D-51 | 트랜잭션 UX([34](34-transaction-ux.md)) — 수동 커밋 단위 = 탭 세션 · 모드 계층 전역→프로필→탭 · 표시 3층(탭 배지 ●n · 상태줄 세그먼트+팝업 · 툴바 Commit 배지) · 잃는 순간만 모달 · 오래된 미커밋 빨강 — 권장안 확정 요청 |
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
| **D-42** | 기본 결과 상한 — 1,000행 + "더 가져오기"(권장) / 500 / 무제한([26 §6](26-performance-architecture.md)) |
| **D-43** | 추가 페치 — 서버 커서 유지 + OFFSET 폴백(권장) / 항상 재질의(DataGrip식) |
| **D-44** | 타이밍 로그 파일 기본 — 끔(`NSQL_LOG=timing`으로 켬 · 권장) / 켬 |
| **D-45** | `CONNECT` 동사 — ⓐ 우리 클라이언트 명령 하나(SQL*Plus 상위 호환 · 별칭 `\c`·`:connect` · 권장) / ⓑ 네이티브·래퍼 분리 / ⓒ 접두 필수([27 §6](27-cli-conventions.md)) |
| **D-46** | 탐색기 메타 세션 — 프로필당 별도 접속(권장) / 편집기 세션 공유 · 설정 `explorer.session`([28 §6](28-object-explorer.md)) |
| **D-47** | 세션 공유 시 편집기 실행 중 메타 요청 — 대기(권장) / 거부 |
| **D-40** | ★ `nexa-license` 서명 알고리즘 **포트화** — `alg=ed25519`(dalek 2.x · beep/clip/sql 기본) + `alg=p256`(dir2 · Windows CNG 인박스 · 외부 crate 0 유지) · 루트 키 2개 · 발급기 양쪽 서명([25 §10-2 #4](25-license-tiers-and-server.md)) — 대안 = dir2 제외(단일 Ed25519) |
| ~~D-39~~ → DR-26 | `nexa-license` 라이브러리 가시성 — 공개(권장 · CI 토큰 불요 · 계열 재사용) / 비공개(CI에 fine-grained PAT) — 서버 저장소는 비공개 확정(DR-25) |
| **D-18** | 기기 키(`device.key`) OS 비밀 저장 결합 — macOS Keychain · Linux Secret Service(현재 0600 평문 · Windows는 DPAPI ✅). 결합 시 키 파일만 교체, 프로필 재암호화 불요([21 §5](21-connection-profiles.md)) |

## 4. 외부 crate 원장 (추가 시 건별 기록)

| crate | 계층·크레이트 | 사유 | 라이선스 | 상태 |
|---|---|---|---|---|
| `ab_glyph` | ④ nexa-gfx | 계승 | Apache-2.0 | ✅ |
| `oracle` 0.6 | ① driver-oracle | DP-3 | UPL/Apache | ☐ M1 |
| `tiberius-ng` | ① driver-mssql | DP-4 | MIT/Apache | ☐ M1 |
| `rustls` (+webpki-roots) | ② nsql-net | TLS | MIT/Apache/ISC | ☐ M1 |
| `russh` | ② nsql-net | SSH 터널 | Apache-2.0 | ☐ M2 |
| `rustybuzz` | ④ nexa-gfx | DP-6 셰이핑 | MIT | ☐ M2 |
| `syntect` | ④ nexa-edit | `.sublime-syntax` | MIT | ☐ M3 |
| `wasmi` | ④ 패키지 | DP-7 | MIT/Apache | ☐ M6 |
| `postgres` 0.19(동기 · tokio-postgres 위) | ① driver-pg | DR-28 · DR-3 예외 | MIT/Apache | ✅ 09-15 |
| `tracing` 0.1 | ① driver-mssql | tiberius Info 토큰(PRINT) 캡처 — 이미 전이 의존 | MIT | ✅ 09-15 |
| `mysql_async` · `rusqlite(bundled)` · `odbc-api` | ① | DP-2 | MIT/Apache | rusqlite ✅ · 나머지 ☐ M4 |
| `chacha20poly1305` 0.10 · `sha2` 0.10 · `getrandom` 0.2 | Core 옆 `nsql-vault` | DR-22 비밀번호 봉투 AEAD · 키 KDF · OS 난수 — ★ **암호화 자체 구현 금지 부류**, nexa-clip `nclip-store` 원장과 동일 판(RustCrypto) | MIT/Apache-2.0 | ✅ 09-13 |
