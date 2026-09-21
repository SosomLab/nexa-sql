# MILESTONES — 기능·목적 관점 현황

> ✅ 완료 / 🚧 진행 / 📐 설계만 / ☐ 미착수. 목표순. 로드맵 = [02](02-roadmap.md).

## M0 — 조사·결정·골격
| 상태 | 항목 | 근거 |
|:--:|---|---|
| ✅ | 경쟁 조사(크로스플랫폼·Oracle·MSSQL) | [03](03-competitive-landscape.md) [04](04-oracle-tools.md) [05](05-mssql-tools.md) |
| ✅ | Rust 생태계(드라이버·GUI·배포) · 편집기 조사 | [06](06-rust-ecosystem.md) [07](07-editor-research.md) |
| ✅ | 언어·플랫폼·아키텍처·범위 결정(DR-1~9 · DP-1~10) | [10](10-decision-record.md) [01](01-architecture.md) |
| ✅ | 공용 UI 라이브러리 `nexa-ui` 추출(189 테스트) | `../nexa-ui` |
| ✅ | ★ 세션 변수 엔진 `nsql-script`(37 테스트) + `nsql plan` | [08](08-session-variables.md) |
| ✅ | 편집기·패키지 설계 · CLI 설계 · 라이선스 | [09](09-editor-and-packages.md) [11](11-cli.md) [13](13-licensing.md) |
| ✅ | 기반 보고서 | [00](00-foundation-report.md) |
| ☐ | DP 확정 → DR 승격 | — |

## M1 — CLI 관통 (Oracle · MSSQL 실접속)
| 상태 | 항목 |
|:--:|---|
| 🚧 | `nsql-net` TCP/TLS · 취소 토큰 — 드라이버가 소켓을 소유하는 구조라 M1은 어댑터 내부 · 취소는 T-3b |
| ✅ | `nsql-driver-oracle`(kubo) — 이름 바인드·REF CURSOR·DBMS_OUTPUT · **실서버 검증(Oracle Free 23ai)** |
| ✅ | `nsql-driver-mssql`(tiberius) — DECLARE+트레일러 OUT 회수 · 배치 라우팅 · **실서버 검증(SQL Server 2022)** |
| ✅ | `nsql-driver-sqlite` — 실DB 검증 어댑터 |
| ✅ | `nsql run/shell/export`(grid/csv/tsv/json/jsonl/insert) · 세션 변수 실기(SQLite · **Oracle · MSSQL**) |
| ✅ | **연결 프로필 `nsql conn` · `-c <이름>` · `CONNECT <이름>`** — `nsql-vault` 봉투 · DPAPI 기기 키 · 여러 인스턴스 공유(DR-22 · 09-13) |
| ☐ | `import`/`bulk`(M4) |

## M2 — GUI 관통
| 상태 | 항목 |
|:--:|---|
| ✅ | **최소 창** — 접속·편집기(TextBox 다중행·IME)·그리드·상태줄·워커 · 영역별 폰트(UI/고정폭) |
| ✅ | 창 실기(09-13 win · 자동 구동) — 프로필 접속 · 편집 · F5 실행 · 그리드 · 재그리기 루프 수정 |
| ⏳ | 창 실기(사용자) — 한글 IME 입력 · 그리드 스크롤 · 창 둘 프로필 공유 |
| 📐 | `nexa-edit` E1~E5([17](17-editor-incremental-plan.md)) — TextBox 대체 |
| ✅ | 연결 프로필 저장·이름 접속(이름 칸 + Save · T-16b 일부 · 09-13) |
| ✅ | Golden식 접속 창(09-14 13·14차) — 로그인 목록(신호등·Test/Connect 행 버튼·Password 열·정렬/폭/DnD/스크롤·우클릭 메뉴·Delete 2단·삭제 뒤 인접 선택) · 상세 폼(편집 메뉴·워드랩 상태·세션 비밀번호) · 프로브 정책(주기·지수 백오프·즉시 재확인·병렬 스레드) · 테스트 스레드 분리 · 소유 창(작업표시줄 1) · 색 설정 창(`ColorPanel`) · hover/포커스/팝업 UX 규칙(CLAUDE.md §3) |
| ✅ | **09-15**: Sublime 단축키 맵(win/mac · `key.*` · 캡처 창 T-53) · **오브젝트 탐색기 1차**(T-56 · `nsql-catalog` 공용 · 소스 열기 → 재컴파일 → Oracle 오류 보고) · 페치 상한 `grid.max_rows`(T-48 1차) · 실행 중 서버 메시지 실시간(MSSQL·PG · [32](32-server-messages-and-live-log.md)) · 편집기 undo/redo · 폰트 기본값(Golden) · 설정 트리(DBeaver) · **3차**: 그리드 셀 선택·복사(TSV/CSV/INSERT) · 트랜잭션(자동/수동 커밋 · Commit/Rollback) · 실행 계획(Explain · `nsql explain`) · 서버 SET 문 통과 · **4차**: Oracle 라이브 로그 모니터(V$SESSION/로그 테이블 폴링 · T-71 1차) · **5차**: 찾기/바꾸기 바(T-73 1차) · 자동 재접속 · **8차**: 환경 설정 창(T-39 1차 · 검색/트리/카드) · 접속 해제 툴바 · 메뉴 글꼴 설정 · 탐색기 셰브론 · **10차**: 설정 JSON 편집(외부 프로그램 + 저장 감시 반영 · CLI export/import-json) · 설정 창 스플리터 · 탐색기 글꼴 · **11~17차**: 접속 창 모달(보조 창 포함) · 탐색기 셰브론 90°/시각 중심/호버 페이드/로딩 애니메이션/아이콘 16종(`explorer.icons`) · 설계 [34 트랜잭션 UX](34-transaction-ux.md)
| ✅ | **09-18**: ★ **세션 컨텍스트 DR-34**([52](52-session-modes.md)) — 공유 연결 N(추가 접속 · 상한 8 · Disconnect ▾) · 전용 세션 `CONNECT <프로필\|"접속 문자열">`/`DISCONNECT` · 탭 표식 메뉴 · 개별 모드 · **실행 통제 단일화**(`gate_open`) · 서버별 탐색기(세션 ≥1이면 유지) · 세션 상태 있는 세션은 유휴 닫기 제외 · 트랜잭션 로그 세션 열 · 판정 순수 함수 + MC/DC · ⏳ 실기 T-125 |
| ✅ | **09-19~20**(win 67~84차): 수동 커밋 잠금 방지 L1~L4([56](56-manual-commit-lock-prevention.md)) · 확장 패널 + 확장 뷰 탭([50 §14](50-extension-system.md)) · 객체 탐색기 갱신(DDL 뒤 그 폴더만 · 유휴 워터마크 · 우클릭 ▸ 계층형 새로 고침 · [57](57-explorer-refresh-after-ddl.md)) · 외부 파일 변경(조용한 재로드 · 겹침 0 병합 · [58](58-external-change-policy.md)) · 접속 유형 개발/시험/운영 · 메모리 회수 · ★ **대용량 파일**([59](59-large-file-handling.md) — 큰 파일 모드 · 열기 선택 · 디스크에서 실행 · 탭 격리 적재 · **편집 버퍼 `TextBuf`**: 65 MB 상주 560 → 87 MB · 입력 190 → 3 ms) · ★ **되돌리기**([60](60-undo-redo-redesign.md) — 연산 기록 · 바이트 예산 · 쉬었다 치면 새 묶음 · 재로드 = 최소 줄 편집 · 거대 편집 2단 확인 · 재시작 뒤에도 남는 기록) |
| 🚧 | **09-20~21**(mac 86~88차): ★ 맥 한글 입력(앱 조합 · [62](62-macos-input-and-present.md)) · 분할기 방언 결함 · 전 객체 유형 DDL 자동 시험 · 맥 화면 내보내기 IOSurface(선택 · present 36 → 2.9 ms · D-133) · ★ **T-146 수동 커밋이 PG·SQLite·MSSQL·MySQL에서 실제로는 자동 커밋이던 결함** · ★ **변수 관리**([63](63-variable-management.md) — REF CURSOR 자동 표시 · 서명 추론 · 문장/커서마다 결과 탭 · 계층형 변수 표 · 실행당 한 번 입력 창 · 파일별 보존 · 변수 창 · `SELECT … INTO` 행 수 정책 · 실서버 통합 9/9) — 잔여 = PG refcursor · `Caps` 포트 · MacroStore |
| 🚧 | **09-21**(win 89차): Windows 종합 점검(메모리·CPU·기동·릭 A/B = 회귀 없음) · 창 화면 밖 배치 수정 · T-155 빈 `COMMIT` 제거 · ★ T-150 PG refcursor ✅ · T-151 PG 서명 → OUT 받기 ✅ · ★ **T-152 `Caps` 포트 ✅** · T-153 `ACCEPT`·CLI `-v`·미정의 바인드 경고 ✅ · **후반**: T-151 ✅(실서버 Oracle·SQL Server — DATE 시각 잘림 · `OUTPUT` 누락 · `NVARCHAR(2)` 잘림 결함 셋) · T-153 `COLUMN NEW_VALUE`·시스템 변수·`${v:형식}`·값 상한 · 향상 모드 표 +2 · 부하원 스위치 +2 · 사용자 확인표 U-1~U-8 · **끝**: 결과 1개에도 탭 영역(`grid.result_tabbar_single` · 다중 탭 종속) · 다중 탭 끄기 = 향상 모드 · 큰 파일 단계별 확장 효과(L1)·구문 강조(L2) 제한 · `${env:…}` 환경 변수 · **마무리 묶음**: 결과 탭 `결과N`·이름 바꾸기 · 메뉴·툴팁·드롭다운 잘림 전수 보완(공용 부품 안전망 + 코딩 규칙) · ★ 설정 ▸ DBMS(Oracle Instant Client 자동 탐지/직접 지정 · 읽기 전용 파생 정보) · 📐 64 드라이버 내장 ↔ 확장 검토 · **89차 후반(추가 7~20)**: 설정 ▸ DBMS(Oracle Instant Client 자동 탐지/직접 지정 · 폴더 고르기 · [64](64-dbms-clients-and-driver-packaging.md)) · 🔧 **객체 탐색기 우클릭**이 실제 입력으로는 닿지 않던 결함 둘(라우팅 · 동작 큐) + 메뉴 = 창 위·대상 행 아래/위 · 접힌 노드는 새로 고침으로 펼치지 않음 · 오프라인 새로 고침 = "접속 안 됨" 안내 · ★ **일회성 비밀번호 + 세션 자격 금고**(`user@host` = 입력 창 · `user:@host` = 빈 비밀번호 명시 · `nsql_core::Secret` 0 덮어쓰기 · 실행 중 난수 키 봉투로 재사용 · 거부되면 폐기 후 다시 묻기 · 종료 때 키까지 지움 · [21 §7](21-connection-profiles.md)) · 같은 서버 `CONNECT` 규칙(자격이 바뀌면 기존 접속을 먼저 닫고 재접속 · 프로필 이름은 최종 접속 정보로 비교 · `RunEvent::ConnectionClosed`) · 입력 창 Enter가 편집기로 새던 결함 · 자체 시험 도구(`ui.click/rclick/move` · `NSQL_NO_ACTIVATE`) — nexa-ui 359 · nexa-sql 402 · **push 뒤(추가 21~23)**: SQL Server·PG 결과 머리줄 = 변수 이름(바인드만 있는 SELECT 항목) · 읽기 전용 숫자 바인드 타입 · CLI도 비밀번호 자리가 없으면 서버에 가지 않음 · 저장하지 않은 탭 닫기 = 저장 여부를 묻는다(`editor.close_unsaved`) — nexa-sql 403 |
| 🔧 | CI `integration`(PostgreSQL) `pg_read_only_transaction_ends_in_manual_mode` 실패 — 78~79차부터 · 원인 미확인(T-146) |
| ☐ | `nexa-grid` · dock/tab(nexa-ui U-2) · 클립보드·컨텍스트 메뉴(T-16b) · 치환 변수 대화상자(T-16c) · DBeaver식 접속 대화상자·프로필 목록([22 §1](22-driver-extensions.md)) |

## 드라이버 — 내장 4종
| 상태 | 항목 |
|:--:|---|
| ✅ | Oracle(ODPI-C · 19c 실서버) · SQL Server(TDS · PRINT 실시간) · **PostgreSQL(09-15 · rust-postgres · matrixdb2 실서버)** · SQLite |
| ☐ | MySQL · ODBC(T-23) · PG TLS(T-70) |

## 배포 — [33](33-distribution-and-packaging.md) (DR-27 · 09-15)
| 상태 | 항목 |
|:--:|---|
| 📐 | 설치본만(MSI · pkg/dmg · deb/rpm) · 목적별 exe · 공유 lib 분리 · macOS `.app` Universal 2 · 파이프라인 T-72 · 아이콘 T-62 |

## 드라이버 확장 · 접속 대화상자 — [22](22-driver-extensions.md) (DR-23·24 · 09-13)
| 상태 | 항목 |
|:--:|---|
| 📐 | stdio JSON-RPC 드라이버 프로세스(T-27) · `drivers/<id>/<ver>/` SxS 보관·관리 CLI(T-28) · GitHub 최신 다운로드·서명 검증(T-29) · Oracle OCI 확장 Instant Client 19/23(T-30) · DBeaver식 접속 대화상자·드라이버 관리자(T-31) |

## M3~M6 — [02](02-roadmap.md)
