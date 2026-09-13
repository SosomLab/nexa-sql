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
| D-6 | 라이선스 키 집행 시점(v1.0 전/후) · 오프라인 Ed25519 토큰(nexa-dir 17 설계 재사용) |
| D-8 | `similar`(비교·3-way 병합) 원장 등재 |
| D-9 | 로프 크레이트 `crop` vs `ropey` — E-1 착수 시 |
| ~~D-14~~ | → **DR-21**(Codespaces + DBMS별 Docker + Actions) |
| **D-15** | ★ **다음 우선순위** — nexa-edit E1~E5 / 세션 hot exit / 비교·git·오브젝트 캐시 / CLI 완성(import·bulk·PG/MySQL) 중 순서 (사용자 답 대기 · 권장 = E1~E5 → hot exit) |
| **D-16** | ★ **실기 OS 순서** — macOS 먼저 vs Windows 먼저(IME 구현 순서에 영향) (사용자 답 대기) |
| **D-17** | 오브젝트 시점 캐시·로컬 히스토리 기본 위치 — 앱 데이터 폴더(권장) vs 프로젝트 `.nexa/` (사용자 답 대기) |
| **D-19** | 드라이버 확장 저장소 구성 — 단일 `SosomLab/nexa-sql-drivers`(태그 접두) vs 확장별 저장소(권장 = 단일 + 서드파티 별도) · [22 §8](22-driver-extensions.md) |
| **D-20** | Oracle OCI 확장의 Instant Client 동봉(OTN 조건 · D-1 통합) vs 사용자 다운로드 안내 |
| **D-21** | 드라이버 갱신 확인 기본값(켬 권장 · 폐쇄망은 설정으로 끔) |
| **D-22** | 순수 Rust thin `oracledb` 채택 시점 — OUT 바인드·REF CURSOR가 공개 API에 들어오는 판(GitHub Discussions 문의 후보). 들어오면 Oracle 내장 드라이버 = thin, OCI(kubo)는 레거시 확장([22 §4-1](22-driver-extensions.md))으로 |
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
| `tokio-postgres` · `mysql_async` · `rusqlite(bundled)` · `odbc-api` | ① | DP-2 | MIT/Apache | ☐ M4 |
| `chacha20poly1305` 0.10 · `sha2` 0.10 · `getrandom` 0.2 | Core 옆 `nsql-vault` | DR-22 비밀번호 봉투 AEAD · 키 KDF · OS 난수 — ★ **암호화 자체 구현 금지 부류**, nexa-clip `nclip-store` 원장과 동일 판(RustCrypto) | MIT/Apache-2.0 | ✅ 09-13 |
