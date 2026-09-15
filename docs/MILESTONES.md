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
| ✅ | **09-15**: Sublime 단축키 맵(win/mac · `key.*` · 캡처 창 T-53) · **오브젝트 탐색기 1차**(T-56 · `nsql-catalog` 공용 · 소스 열기 → 재컴파일 → Oracle 오류 보고) · 페치 상한 `grid.max_rows`(T-48 1차) · 실행 중 서버 메시지 실시간(MSSQL·PG · [32](32-server-messages-and-live-log.md)) · 편집기 undo/redo · 폰트 기본값(Golden) · 설정 트리(DBeaver) · **3차**: 그리드 셀 선택·복사(TSV/CSV/INSERT) · 트랜잭션(자동/수동 커밋 · Commit/Rollback) · 실행 계획(Explain · `nsql explain`) · 서버 SET 문 통과 · **4차**: Oracle 라이브 로그 모니터(V$SESSION/로그 테이블 폴링 · T-71 1차) · **5차**: 찾기/바꾸기 바(T-73 1차) · 자동 재접속
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
