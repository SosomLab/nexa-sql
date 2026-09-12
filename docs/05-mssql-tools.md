# 05 · 경쟁 제품 조사 ③ — Microsoft SQL Server 클라이언트 · 세션 변수 우회 설계

> 작성 2026-09-12 · 웹 조사(에이전트) · Microsoft Learn·GitHub·벤더 사이트 기준. 미확인 항목은 "확인 필요".
> ★ §7이 nexa-sql 핵심 설계([08](08-session-variables.md))의 SQL Server 측 근거다.

---

## 1. SQL Server Management Studio (SSMS)

| 항목 | 내용 |
|---|---|
| 한줄 요약 | Microsoft 공식 GUI 관리 도구. 2026년 현재 **SSMS 22**가 유일한 Full support 버전 |
| 최신 버전 | **22.10.0 (2026-09-08)**. 22.0.0 GA는 2025-11-11 ([Release notes](https://learn.microsoft.com/en-us/ssms/release-notes-22)) |
| 기술 스택 | **Visual Studio 2026 (18.x) Isolated Shell** 기반 (.NET Framework 4.8, WPF/WinForms). 22.10.0은 VS 18.10.0 셸. ※ VS 2022(17.x) 셸을 쓴 것은 SSMS 21이며, SSMS 22부터 18.x 셸로 전환 |
| OS | Windows 11 / Windows Server 2019·2022·2025, **64-bit(x64·Arm64) 전용**. Windows 10은 지원 목록에서 제외 ([System requirements](https://learn.microsoft.com/en-us/ssms/system-requirements)) |
| 무게 | RAM 최소 4 GB / 권장 16 GB, 디스크 "일반 설치 20–50 GB"(VS Installer 워크로드 포함). 정확한 다운로드 GB 수치는 확인 필요 |
| 라이선스 | 무료. GitHub Copilot 기능은 Copilot 구독(Free 포함) 필요 |
| 지원 정책 | Modern Lifecycle — 최신 버전만 Full support ([Support policy](https://learn.microsoft.com/en-us/ssms/support-policy)) |

**주요 기능**: Object Explorer, Query Editor(IntelliSense), Activity Monitor, 실행 계획(Actual/Estimated), XEvent Profiler / SQL Server Profiler, SQL Server Agent 잡, Maintenance Plans, SQLCMD 모드, Results Grid→Excel/JSON/Markdown/XML 저장(22.4.1+).
**2026 신규**: GitHub Copilot Chat·Code completion GA(22.4.1), Copilot **Agent Mode** preview(22.7.0, 기본 read-only), ScriptDOM 기반 **SQL Formatter** preview(22.7.0), **Schema Compare** preview(22.7.0), Database DevOps(22.9.0). ([TechCommunity GA 공지](https://techcommunity.microsoft.com/blog/sqlserver/sql-server-management-studio-ssms-22-is-now-generally-available-ga/4469003))

**강점**: Agent/Maintenance Plan/보안 관리 등 DBA 기능의 유일한 공식 홈. **약점**: Windows 전용, 무겁고(VS 셸) 설치가 큼, 22.7.1까지 대용량 스크립트 편집기 메모리 폭주 버그. **참고할 점**: Microsoft 스스로 "SQL Agent·전체 관리는 SSMS, 개발은 VS Code"로 역할을 분리 중 → 사용자 평 *"덩치만 빼면 SSMS가 좋다"* 의 정확한 지점.

### 1-1. SQLCMD 모드 정밀 정리
근거: [SSMS SQLCMD 모드](https://learn.microsoft.com/en-us/ssms/scripting/sqlcmd-scripts-query-editor), [sqlcmd commands](https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-commands), [sqlcmd utility](https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-utility)

- 활성화: Query ▸ SQLCMD Mode. 켜면 **IntelliSense와 T-SQL 디버거가 꺼짐**. 명령은 행 첫머리, 한 행에 하나, `GO` 외 모두 `:` 접두.
- `:setvar <var> ["value"]` — 스크립팅 변수 정의. 공백 불가, 값 미지정 시 삭제. 우선순위: `:setvar` > 환경변수.
- `$(var)` — **실행 전 텍스트 치환**(클라이언트 측 문자열 대체). 식별자에도 쓸 수 있으나 타입 안전성·injection 방어가 없음. `-x`로 비활성화.
- `:connect server[\instance] [-l timeout] [-U user [-P password]] [-N] [-G auth]` — 현재 연결을 닫고 재연결. **암시적 배치 구분자가 아니므로** `GO` 없이 여러 `:connect`를 쓰면 마지막 서버로 모두 실행.
- `:r <filename>` — 파일 내용을 문장 캐시에 파싱(중첩 가능).
- `:on error [exit|ignore]` · `:exit(query)` · `RAISERROR(...,127)`.
- `:out`, `:error`, `:perftrace` — 출력 리디렉션. `!! <cmd>` — OS 명령.
- `GO [n]` — 배치 종료 + n회 반복. 공식 문구: *"You can't declare a variable more than once in a single batch."*
- SSMS는 SqlClient, 명령줄 sqlcmd는 ODBC/go-mssqldb를 쓰므로 기본 SET 옵션이 달라 동일 스크립트 동작이 다를 수 있음.
- ★ **핵심 제약**: T-SQL 지역변수 `DECLARE @v`의 범위는 *"the batch or stored procedure in which it is declared"* — `GO`를 넘으면 `Msg 137 Must declare the scalar variable`. ([Variables](https://learn.microsoft.com/en-us/sql/t-sql/language-elements/variables-transact-sql)) SQLCMD 변수는 이 문제를 **텍스트 치환**으로만 우회.

---

## 2. Azure Data Studio (ADS) → MSSQL extension for VS Code

- **ADS는 2026-02-28 은퇴 확정** ([What's happening with ADS](https://learn.microsoft.com/en-us/sql/tools/whats-happening-azure-data-studio), [DevBlog](https://devblogs.microsoft.com/azure-sql/azure-data-studio-retirement/)).
- 대체: **MSSQL extension for Visual Studio Code** (`ms-mssql.mssql`). Windows/macOS/Linux. TypeScript(VS Code) + .NET `SqlToolsService`.
- GA 기능: Connection dialog, Object Explorer(필터), Query results(정렬·JSON/Excel/CSV), Query plan visualizer, Table designer, **Schema Designer**, **Schema Compare**, **Local SQL Server containers**, View/Edit data, DACPAC/BACPAC, DB 관리, Flat file import, **Query Profiler(XEvents)**, SQL notebooks, Copilot Chat + Agent Mode. ([MSSQL extension overview](https://learn.microsoft.com/en-us/sql/tools/visual-studio-code-extensions/mssql/mssql-extension-visual-studio-code))
- 미이전: **SQL Server Agent → SSMS 전용**, Profiler → XEvent, PostgreSQL/MySQL → 별도 확장.
- 강점: 무료·크로스플랫폼·공식·Copilot. 약점: DBA 기능 부족(Agent 없음), Electron 메모리, 결과 그리드 단순. SQLCMD 모드 지원 여부는 확인 필요.

---

## 3. 명령줄 도구

| 도구 | 상태·버전 | 스택 | 비고 |
|---|---|---|---|
| **sqlcmd (Go)** — [microsoft/go-sqlcmd](https://github.com/microsoft/go-sqlcmd) | **v1.10.0 (2025-03-03)**, 3-OS/컨테이너 | Go + go-mssqldb | 공식 권장 변형. Entra ID 광범위, TDS 8.0 strict, `sqlcmd create mssql`. ODBC판과 `-R`·`-I`·`-i` 동작 차이 |
| **sqlcmd (ODBC)** | `mssql-tools18` 동봉 | C++ + ODBC Driver 18 | Linux/macOS 통합 인증은 Kerberos 구성 필요 |
| **mssql-cli** — [dbcli/mssql-cli](https://github.com/dbcli/mssql-cli) | **deprecated** | Python | 자동완성 등 대화형 기능은 go-sqlcmd에 동등물 없음 |
| **usql** — [xo/usql](https://github.com/xo/usql) | v0.21.x 활발 | Go | psql 스타일 범용 CLI, 변수·백슬래시 명령·DB 간 복사 |

---

## 4. 서드파티 도구

| 도구 | 한줄 요약 | 스택/OS | 가격 | 강점 | 약점·참고 |
|---|---|---|---|---|---|
| **Redgate SQL Prompt / Toolbelt Essentials** | SSMS·VS 애드인(IntelliSense·포매팅·리팩터링) | .NET, Windows | SQL Prompt 약 **$369/user/yr**(확인 필요), Toolbelt Essentials 약 **$1,495** ([Redgate](https://www.red-gate.com/products/sql-prompt/pricing/)) | 업계 표준 포매터·스니펫 | SSMS 종속, 고가 |
| **ApexSQL (Quest)** | **Refactor·Complete·Fundamentals 2025-12-31 지원 종료**, Toad로 이전 유도 ([Quest 공지](https://support.quest.com/product-notification/noti-00001585)) | .NET, Windows | — | — | 신규 채택 비권장 |
| **dbForge Studio for SQL Server (Devart)** | SSMS 대체급 IDE(디버거·스키마/데이터 비교) | **.NET 4.7.2 WinForms, Windows 전용**; mac/Linux는 CrossOver ([Devart](https://docs.devart.com/studio-for-sql-server/getting-started/how-to-install-dbforge-studio-linux-mac.html)) | 구독 $229.95~$479.95/yr, 영구 $449.95~$949.95 | SSMS보다 풍부한 개발 기능 | 네이티브 크로스플랫폼 아님 |
| **DataGrip** | 다중 DB IDE | JVM, 3-OS | 약 $229/user/yr | 최고 코드 인텔리전스 | JVM 메모리, MSSQL 관리 기능 미약 |
| **DBeaver** | 오픈소스 범용 | JVM, mssql-jdbc | CE 무료 | ER·Kerberos/Azure SSO | 무겁고 macOS AD 인증 트러블 다수 |
| **TablePlus** | 경량 네이티브 | Swift(mac)/네이티브 | $99~129 | 빠르고 가벼움 | 실행계획·관리 거의 없음 |
| **SQLPro for MSSQL (Hankinsoft)** | macOS 전용 네이티브 | Swift/Cocoa | $9.99/월 또는 라이프타임(확인 필요) | 가장 가벼운 Mac MSSQL GUI | 기능 최소 |
| **Aqua Data Studio (Idera)** | 범용 IDE | JVM | 약 $499/yr | 시각적 분석·ER | 무겁고 고가 |
| **Navicat for SQL Server** | 상용 GUI | Qt, 3-OS | 영구(확인 필요) | 데이터 전송·스케줄 | MSSQL 특화 기능 적음 |
| **HeidiSQL** | 경량 Windows GUI | Delphi | 무료(GPL) v12.21 | 수십 MB, MSSQL 네이티브 | Windows 전용 |
| **SSMSBoost** | SSMS 애드인 | .NET | Community 무료 / Pro $195 | Grid 도구·연결 색상 | SSMS 종속 |
| **SSMS Tools Pack 7.0** | SSMS 애드인(쿼리 히스토리·스니펫) | .NET, SSMS 22.7+ | 영구 €40~ ([Licensing](https://www.ssmstoolspack.com/Licensing)) | 저렴 | SSMS 종속 |
| **Toad for SQL Server (Quest)** | 관리·개발 IDE, **2026 R1/R2** | .NET, Windows | 견적(확인 필요) | 데이터 검색·자동화 | Windows 전용 |
| **SolarWinds Plan Explorer** | 실행계획 분석 무료 도구 **v2026.2** ([SolarWinds](https://www.solarwinds.com/free-tools/plan-explorer)) | .NET, Windows | 무료 | SSMS보다 우수한 플랜 시각화 | Windows 전용 |
| **sp_whoisactive / First Responder Kit** | T-SQL 스크립트 생태계 | T-SQL | 무료(MIT) | 2026부터 연 1회 버전 | 서버 측 프로시저 |

---

## 5. macOS/Linux 사용자의 실제 선택 (2026)

Reddit/HN 원문 스레드는 직접 확보하지 못함(**확인 필요**). 2026년 비교 자료([Devart](https://www.devart.com/dbforge/ssms-alternatives.html), [Beekeeper](https://www.beekeeperstudio.io/blog/sql-server-management-studio-alternatives-free), [DbVisualizer](https://www.dbvis.com/thetable/azure-data-studio-alternatives-after-its-retirement/), [Jam SQL](https://jamsql.com/blog/ssms-alternatives-mac/))의 컨센서스:

1. **VS Code + MSSQL extension** — ADS 은퇴 이후 기본 선택.
2. **DBeaver CE** — 무료 범용 1순위. **DataGrip** — 유료 개발자 1순위.
3. **TablePlus / SQLPro Studio / Beekeeper Studio** — "가볍고 빠른 네이티브" 수요. 신흥 **Jam SQL Studio**(Apple Silicon 네이티브)도 언급(오픈소스 여부 확인 필요).
4. 관리 작업(Agent, 보안)이 필요하면 결국 **Windows VM/RDP + SSMS**.

★ "SSMS만큼의 DBA 기능을 가진 경량 크로스플랫폼 도구는 없다"는 공백이 존재.

---

## 6. 프로토콜·드라이버

- **TDS**: [MS-TDS](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/893fcc7e-8a39-4b3c-815a-773b7b982c50). **TDS 8.0**(SQL Server 2022+)은 TLS 1.3 핸드셰이크가 TDS 앞에 오며 `Encrypt=strict` ([TDS 8.0](https://learn.microsoft.com/en-us/sql/relational-databases/security/networking/tds-8)).
- **Rust `tiberius`** ([crates.io](https://crates.io/crates/tiberius)): 0.12.3, **마지막 릴리스 2024-07-19**, 사실상 무보수. TLS(native-tls/rustls), `winauth`(Windows SSPI), `integrated-auth-gssapi`(Kerberos), AAD 토큰, `BulkLoadRequest`. **MARS 미지원**. TDS 8.0 strict 미해결.
- 후속 포크(2026): **`tiberius-ng`** — 2026-08-31 발표, drop-in, TDS 8.0 strict, MultiSubnetFailover, 쿼리 취소, SSPI/NTLM/Kerberos/Entra ([Rust 포럼](https://users.rust-lang.org/t/tiberius-ng-a-maintained-continuation-of-the-tiberius-sql-server-driver/142157), [repo](https://github.com/MattJackson/tiberius-ng)). **`mssql` 1.0.2**(2026-09-01, [mssql-rust](https://github.com/mssql-rust/mssql-rust)) — 역시 포크. **`mssql-client` 0.20**([praxiomlabs](https://github.com/praxiomlabs/rust-mssql-driver)) — 신규 구현, TDS 7.3–8.0, 풀링, Entra/Kerberos, 단일 유지보수자.
- **Microsoft 공식 Rust TDS**: [microsoft/mssql-rs](https://github.com/microsoft/mssql-rs) — `mssql-tds` 코어, **실험/프리뷰**, 외부 PR 미수용. 향후 참조 구현 후보.
- **`odbc-api`**: unixODBC + ODBC Driver 18 경유. 통합 인증·Entra를 드라이버가 처리하나 설치 의존.
- **Entra ID 요건**: TDS `FEDAUTH` feature ext + 액세스 토큰(scope `https://database.windows.net/.default`) — Rust는 `azure_identity`로 토큰을 얻어 `AuthMethod::aad_token` 전달.

---

## 7. ★ SQL Server에서 세션 변수 공유가 안 되는 이유와 우회 방법

### 원인
- 클라이언트가 `GO`를 만나면 그 앞까지를 **하나의 배치(batch)** 로 전송. `GO`는 T-SQL 문이 아니라 도구 명령.
- 지역변수 범위 = *선언된 배치 또는 프로시저의 끝까지*. `sp_executesql` 중첩 배치도 상위 변수를 못 본다 ([Variables](https://learn.microsoft.com/en-us/sql/t-sql/language-elements/variables-transact-sql), [sp_executesql](https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/sp-executesql-transact-sql)).
- 즉 SQL*Plus의 `VARIABLE x NUMBER` + `:x` 같은 **세션 수명 바인드 변수 개념이 T-SQL에는 없다**.

### 서버가 제공하는 세션 범위 저장소
| 수단 | 범위 | 용량/타입 | 참고 |
|---|---|---|---|
| `SET CONTEXT_INFO` / `CONTEXT_INFO()` | 세션 | **최대 128 bytes binary** | UDF 내 사용 불가 ([SET CONTEXT_INFO](https://learn.microsoft.com/en-us/sql/t-sql/statements/set-context-info-transact-sql)) |
| `sp_set_session_context` / `SESSION_CONTEXT(N'key')` | 세션 | 값 **sql_variant 최대 8,000 bytes**, **세션 합계 1 MB** | SQL Server 2016+, `@read_only=1`, MARS 배치 간 가시성 제한 ([sp_set_session_context](https://learn.microsoft.com/en-us/sql/relational-databases/system-stored-procedures/sp-set-session-context-transact-sql)) |
| 임시 테이블 `#t` | 세션 | tempdb | 배치를 넘어 존속 |
| 전역 임시 테이블 `##t` | 모든 세션 | tempdb | 변수 저장소로 부적합 |
| `sp_executesql` 파라미터 | 단일 호출 | 모든 타입, `OUTPUT`, TVP | RPC 전달 시 타입 보존, 플랜 재사용 |

### 클라이언트가 SQL*Plus식 바인드 변수를 흉내내는 3가지 설계

**(a) 클라이언트 변수 저장소 + `sp_executesql`/RPC 파라미터 주입**
- 클라이언트가 `:name` 토큰을 파싱해 배치를 `@stmt`로, 변수 목록을 `@params`로 만들어 RPC 호출. 값이 바뀐 `OUTPUT` 파라미터는 RETURNVALUE 토큰으로 돌아와 저장소에 반영 → SQL*Plus `VARIABLE`/`PRINT` 의미론과 가장 가까움.
- 장점: **타입 충실도 최고**, injection 안전, 플랜 캐시 재사용, `OUTPUT` 양방향.
- 단점: `sp_executesql` 안의 `USE db`·`SET` 옵션이 호출 종료 시 원복; `CREATE PROCEDURE` 등 "배치 첫 문장" DDL은 래핑 불가(별도 경로); 커서는 배치 밖에서 fetch 불가; 오류 줄번호 어긋남; `GO n`은 클라이언트 몫.

**(b) 각 배치 앞에 `DECLARE @v <type> = <value>;` 자동 prepend**
- 저장소 값을 리터럴로 직렬화해 배치 앞에 붙임. 끝에 `SELECT @v AS [__var_v]`로 회수 가능.
- 장점: 원본 T-SQL 거의 무변경(DDL·`USE`·`SET` 그대로), 구현 단순, SQLCMD와 비슷한 경험.
- 단점: 리터럴 직렬화 타입 손실 위험(float·datetimeoffset·nvarchar(max)·varbinary), **플랜 캐시 오염**, 줄번호 오프셋 보정 필요, 사용자가 같은 이름을 `DECLARE`하면 중복 선언 오류.

**(c) `SESSION_CONTEXT` 사용**
- `EXEC sp_set_session_context N'v', @value` 저장 후 후속 배치에서 `CAST(SESSION_CONTEXT(N'v') AS int)`. 클라이언트가 `:v`를 CAST 식으로 재작성.
- 장점: 서버 측 진짜 세션 상태 — 프로시저·트리거·RLS 조건에서도 보임.
- 단점: `sql_variant`라 매번 CAST(타입 힌트를 클라이언트가 기억), 8 KB·1 MB 제한, 연결 풀 `sp_reset_connection` 초기화 확인 필요, 2016 미만은 128 B `CONTEXT_INFO`만.

**★ 설계 권고**: 기본 **(a)** 로 타입·안전성 확보, `sp_executesql`로 감쌀 수 없는 배치(첫 문장 DDL·`USE`·지속 `SET`)에만 **(b)** 폴백. 서버 측 코드에서도 변수를 봐야 하면 **(c)** 를 명시적 옵션. 행 집합을 다음 배치로 넘기는 요구는 `#temp` 안내가 현실적. → 구현 = `nsql-script` `Dialect::Mssql`([08](08-session-variables.md)).

---

### 종합 비교표

| 도구 | 플랫폼 | 무게 | 가격 | DBA 기능 | 개발 기능 | 2026 상태 |
|---|---|---|---|---|---|---|
| SSMS 22 | Win x64/Arm64 | 매우 무거움(VS 18 셸, 4–16 GB RAM) | 무료 | ★★★★★ | ★★★★ | 활발 |
| VS Code + MSSQL | 3-OS | 중(Electron) | 무료 | ★★★ | ★★★★ | ADS 후계, 월간 릴리스 |
| go-sqlcmd | 전 플랫폼 | 매우 가벼움 | 무료 | ★★ | ★★ | v1.10.0 |
| DBeaver | 전 플랫폼 | 무거움(JVM) | 무료/유료 | ★★ | ★★★★ | 활발 |
| DataGrip | 전 플랫폼 | 무거움(JVM) | $229/yr | ★ | ★★★★★ | 활발 |
| TablePlus / SQLPro | 네이티브 | 가벼움 | $99–129 / $9.99 | ★ | ★★ | 활발 |
| dbForge Studio | Win(CrossOver) | 중 | $229–479/yr | ★★★ | ★★★★★ | 2026.1 |
| Toad for SQL Server | Win | 중 | 견적 | ★★★★ | ★★★★ | 2026 R2 |
| Plan Explorer | Win | 가벼움 | 무료 | ★★(플랜) | — | 2026.2 |
| ADS | — | — | — | — | — | **은퇴(2026-02-28)** |

**요약 결론**: 2026년 MSSQL 클라이언트 지형은 "Windows·무거움·완전한 관리(SSMS 22)" 대 "크로스플랫폼·개발 중심(VS Code MSSQL, DBeaver/DataGrip)"으로 양분. macOS/Linux에서 **가볍고 스크립트 친화적(SQLCMD 호환 + 세션 변수)** 도구는 go-sqlcmd·usql뿐이고 둘 다 SQL*Plus식 바인드 변수는 없다. Rust 드라이버는 `tiberius-ng`(현 시점 가장 현실적) 또는 향후 `microsoft/mssql-rs`를 주시, 세션 변수는 `sp_executesql` RPC 파라미터 방식을 1차로 채택.
