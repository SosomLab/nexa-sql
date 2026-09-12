# 04 · 경쟁 제품 조사 ② — Oracle 특화 클라이언트 · SQL*Plus 스크립트 의미론

> 작성 2026-09-12 · 웹 조사(에이전트) · 일부 벤더 페이지(Quest·Allround·Oracle 일부)는 403이라 2차 출처 사용. 미확인 항목은 "확인 필요".
> ★ §8이 [08 세션 변수 설계](08-session-variables.md)의 Oracle 측 기준이다.

---

## 1. Benthic Software — Golden / PLEdit

| 항목 | 내용 |
|---|---|
| 한줄 요약 | 1996년부터 이어진 초경량 Oracle 전용 Windows 쿼리/스크립트 도구. SQL*Plus 스크립트 관습을 그대로 흡수한 것이 핵심 가치 |
| 기술 스택 | 네이티브 Win32/x64 단일 EXE(Golden32.exe / Golden8_64bit.exe), 설치 13~15MB, 외부 런타임 없음, **OCI.DLL 직접 로드**(레지스트리로 Oracle Home 탐지 또는 `oci.dll` 경로 직접 지정, Unicode OCI). 개발 언어 Delphi/VCL 추정 — 확인 필요 ([FAQ](https://www.benthicsoftware.com/faq.html)) |
| OS | Windows 7 이상, Terminal Server/Citrix. 클라이언트↔Golden 비트 일치 필요. Portable 모드(8.4) |
| 라이선스·가격 | Golden 8: $60/1user(5~19 $54, 20+ $48), 업그레이드 $45. PLEdit 7: $40. GoldView 4: $30. 30일 트라이얼, 오프라인 등록 ([golden](https://www.benthicsoftware.com/golden.html), [pledit](https://www.benthicsoftware.com/pledit.html)) |
| 현황 | 활발. Golden 8.4 Build 846 (2026-07-12), 8.3(2026-01-27)에서 Oracle 26ai 지원. PLEdit 7.5 Build 750 (2026-03-09). GoldSqall 2.4(멀티DB판) ([news](https://www.benthicsoftware.com/news.html)) |

**SQL*Plus 호환 스크립트 기능**
- `EXEC`, `DESC`, `CONNECT` 문 지원("Supports EXEC, DESC and CONNECT statements").
- **바인드 변수**: `:NAME` 형식, REFCURSOR 포함. 2004년판 매뉴얼 원문: *"Bind variables are names starting with a colon (i.e. :RETVAL), and the contents of bind variables are displayed when the PRINT command is used."* / *"Bind variables can be used with the EXEC command to pass arguments and return values including refcursors."*
- ★ `VARIABLE name TYPE` 선언이 필수인지, `:name`을 만나면 값/타입을 **프롬프트**하는지(GoldSqall `Set bindprompting on/off` 옵션이 시사)는 확인 필요. 사용자 실사용 예(`EXEC :V_PRG_NM := '...'` 후 선언 없이 `SELECT :V_PRG_NM FROM DUAL`)로 보아 **선언 없는 암묵 생성 + 대입으로 타입 추론**이 Golden의 동작으로 추정 → nexa-sql도 이 관용을 채택([08 §3](08-session-variables.md)).
- 치환 변수: `&var` 프롬프트, `DEFINE`, 8.1부터 치환 상수(탭 이름·날짜 등).
- `SET`: `FETCHROWS`, `transpose on/off`, `bindprompting on/off` 등 도구 고유. `SPOOL` + 자체 `OUTPUT`, `DBMS_OUTPUT`, `Clear dboutput`.
- 스크립트 호출 + 파라미터(치환 변수로 변환), 커맨드라인 `Golden8_64bit.exe -u:"login" -a:"script.sql" -x -s`.
- 그 외: `SHOW CREATE <object>`, `SHOW PARAMETERS`, `fetch all;`, `export excel|json`, Explain Plan, 타이밍, **`/*-*/`·`/*+*/` 접두어로 로컬 파싱 우회/서버 처리 강제**.
- 실행 모델: 커서 문장 실행 vs "Run script"(스크립트 모드).
- **PLEdit**: 프로시저/패키지/트리거/뷰 편집·컴파일, 오류 위치 이동, 멀티캐럿, SQLBuilder. **디버거 없음**.
- 약점: Windows 전용, DBA 기능 없음, 온라인 문서 부재, 구식 UI.
- ★ 참고할 점: "OCI 직접 + 단일 실행 파일 + SQL*Plus 관습 수용"이라는 포지셔닝 자체. `/*-*/` 로컬 파서 우회, 스크립트 인자→치환 변수, 커맨드라인 자동화.

## 2. Orange for Oracle (웨어밸리 WareValley)

| 항목 | 내용 |
|---|---|
| 한줄 요약 | 2001년 출시된 한국산 Oracle 클라이언트. 가볍고 한국어 UX로 국내 SI/금융권 표준 |
| 역사 | 1.0(2001) → Orange 2010(DB2/Sybase/MS SQL/Altibase/Tibero 확장) → 6.0 → 7.0(약 2015, "20만 사용자") → **Orange 8.0**(다운로드 목록 2026-01-13, Standard/DBA UNICODE, 855MB) + Orange Ade 3.0(ODBC/JDBC 멀티DBMS) + Trusted Orange ([download-list](https://www.warevalley.com/ko/support/download-list), [sharedit](https://www.sharedit.co.kr/products/104)) |
| 기술 스택 | 네이티브 Windows GUI, Oracle Client + `tnsnames.ora` 경유 OCI. **7.0은 32bit Oracle Client만 지원**(2020-10 사례) ([hoing.io](https://www.hoing.io/archives/153)) |
| 라이선스·가격 | 영구. V7 Standard ₩1,455,300(리셀러가), DBA·Reorg 견적. 30일 트라이얼 |
| ★ 2026 판매 현황 | **여전히 판매·다운로드 중**(사용자 인식 "판매 안 되는 것 같다"와 다름). warevalley.com에 Orange 7.0(Standard/DBA/Reorg), for Tibero/Altibase/SQL Server/DB2, Orange 8.0이 2026-01-13자 게시. 단종 공지 없음 |
| Oracle 특화 강점 | SQL Tool, PL/SQL Tool, Plan Tool, Schema Browser, Description Tool, Table Editor, Export/Import, Loader, SQL↔코드 변환. DBA: Session/SQL/ASH Monitor, Tuning Advisor, Lock/Space. Reorg |
| 사랑받은 이유 | "서버 부담 없이 16개 이상 인스턴스 동시 실행", 가벼움, 한국어 UI, 국내 표준 |
| 약점 | 32bit 종속(7.0), 확장성 부족, 콘솔이 SQL*Plus 문법과 다름, 폐쇄적 문서 |
| 참고할 점 | 다중 인스턴스 경량성, 한국어 Description/Plan Tool UX |

## 3. Oracle SQL Developer / SQL Developer for VS Code / SQLcl

| 항목 | 내용 |
|---|---|
| 한줄 요약 | Oracle 공식 무료 IDE. **데스크톱(Java Swing)은 24.3.1로 기능 개발 종료**, 신기능은 VS Code 확장, 스크립트 엔진은 SQLcl |
| 데스크톱 | 24.3.1(2024-12) 마지막 기능 릴리스. Jeff Smith 2026-02: "no new features are planned", 24.3 지원 **2027-04까지** ([thatjeffsmith](https://www.thatjeffsmith.com/archive/2026/02/oracle-sql-developer-24-3-support-extended/)) |
| VS Code 확장 | 26.2(2026-07). 연결 폴더, 차트, 테이블 편집, CSV 로드, SQL Notebook, DBA 기능 추가 중 ([docs](https://docs.oracle.com/en/database/oracle/sql-developer-vscode/index.html)) |
| SQLcl | 26.2. **Java 17/21 필요**. SQL*Plus 호환: `VARIABLE`, `PRINT`, `EXEC`, `DEFINE`, `ACCEPT`, `SET`, `SHOW`, `CONNECT`, `SPOOL`, `@/@@/START`, `DESC`, `COLUMN`(`NEW_VALUE`만), `BREAK/COMPUTE`, `WHENEVER`, `HOST`. 전용: `SET SQLFORMAT {csv|html|xml|json|ansiconsole|insert|loader|...}` + `/*csv*/` 인라인 힌트, `ALIAS`, `INFO[+]`, `DDL`, `CTAS`, `LOAD`, `REPEAT`, `LIQUIBASE`, `MCP`, `AI`. 미지원: `REPHEADER/REPFOOTER`, `SET SQLTERMINATOR/MARKUP/RECSEP` ([working-sqlcl 26.2](https://docs.oracle.com/en/database/oracle/sql-developer-command-line/26.2/sqcug/working-sqlcl.html), [relnotes](https://www.oracle.com/tools/sqlcl/sqlcl-relnotes-26.2.html)) |
| 강점 | 무료·크로스플랫폼(JDBC thin), 최신 기능 즉시 반영, PL/SQL 디버거, SQLcl 스크립트 완전 호환 |
| 약점 | Java 무게, 데스크톱 유지보수 모드, VS Code판 기능 격차 |
| ★ 참고할 점 | SQLcl의 `SET SQLFORMAT` + `/*json*/` 힌트, **`COLUMN NEW_VALUE`만 남기고 리포트 서식을 버린 취사선택 목록**이 nexa-sql "구현할 SQL*Plus 부분집합"의 기준으로 적합 |

## 4. Quest Toad for Oracle

| 항목 | 내용 |
|---|---|
| 한줄 요약 | 가장 무겁고 가장 비싼 Oracle IDE의 사실상 표준 |
| 기술 스택 | Windows 네이티브(Delphi로 알려짐 — 확인 필요), 64bit, **Instant Client 19.3 + 23.7 번들**. 2026 R1 ([release notes](https://support.quest.com/technical-documents/toad-for-oracle/2026-r1/release-notes/)) |
| 가격(구독) | Base $625, Professional $983, Pro DB Admin $1,601, Xpert Plus $2,166, Dev Plus $2,472, DBA Plus $4,994 /user/yr ([trustradius](https://www.trustradius.com/products/toad-database-developer-tools/pricing)) |
| 에디션 | Base: Editor+디버거, Schema Browser, Session Browser, 자동화, utPLSQL. Pro: +Code Analysis. Xpert: +SQL Optimizer. DB Admin: Health Check, 비교, AWR |
| 약점 | 무거움, Windows 전용, 가격 |
| 참고할 점 | Schema Browser 탭 구조, Session Browser, "as script vs statement" 실행 구분, 바인드 변수 프롬프트 다이얼로그 |

## 5. Allround Automations PL/SQL Developer

| 항목 | 내용 |
|---|---|
| 한줄 요약 | PL/SQL 개발자용 실용주의 IDE. 저렴·빠름, Test/Command Window가 SQL*Plus 흐름을 GUI로 잘 옮김 |
| 기술 스택 | Windows 네이티브(Delphi 추정 — 확인 필요), OCI 필수(Instant Client 가능), 플러그인 |
| 현황·가격 | 16.0.8 (2025-11-24). 약 $238 ([componentsource](https://www.componentsource.com/product/pl-sql-developer)) |
| ★ 핵심 창 | **SQL Window**(`&subst`와 `:bind` 모두, 바인드 기본 타입 문자열), **Test Window**(익명 블록, 소스에서 바인드 변수 자동 탐지 → 값 입력 그리드, 디버거/프로파일러), **Command Window**(SQL*Plus 에뮬레이션), Program Window, Session Monitor, Object Browser, Beautifier, Query Builder ([setup guide](https://www.williamrobertson.net/documents/plsqldeveloper-setup-1.html)) |
| 약점 | Windows 전용, UI 노후, DBA 약함 |
| ★ 참고할 점 | "SQL Window(문장)/Test Window(블록+바인드)/Command Window(SQL*Plus)" 3분할과, 바인드 변수를 소스에서 스캔해 값 입력 그리드를 띄우는 UX |

## 6. 기타 도구

| 도구 | 벤더/현황 | 스택·OS | 가격 | 비고 |
|---|---|---|---|---|
| **SQL*Plus** | Oracle 동봉 | C/OCI, 전 OS | 무료 | 스크립트 의미론 기준(§8) |
| **TOra** | **휴면**(v3.2 2017) | C++/Qt | GPL2 | ([tags](https://github.com/tora-tool/tora/tags)) |
| **SQLTools** | 오픈소스, **2.0 build 37 2026-08-24** | Win32, Oracle Client 12c/19c | GPLv3 | 초경량(6MB), connect·변수 등 기본 SQL*Plus 명령 ([sf](https://sourceforge.net/projects/sqlt/)) |
| **KeepTool Hora** | 16.2.3(2025-09) | Delphi, Windows | Free/Pro $1,057/Enterprise | ([keeptool](https://keeptool.com/en/keeptool-products/)) |
| **RazorSQL** | 11.0.3 | Java, 전 OS | $129 | 멀티DB, Oracle 특화 약함 |
| **Aqua Data Studio** | Idera 26.0 | Java | ~$499/yr | 26.0에서 Oracle SSPI |
| **SQLGate** | **㈜체커(Chequer)** — 2017 앤트위즈에서 양수. for Oracle Developer **10.3.4.1(2026-03-04)**, V10부터 64bit | Windows 네이티브 | 무료판(에디터 1창·세션 1) / Standard·Premium 구독 / Enterprise 영구 / Indie ₩300,000/30개월 / 학생 무료 | 8종 DBMS(Oracle, SQL Server, Tibero, DB2, MySQL, MariaDB, PostgreSQL, CUBRID). PL/SQL 디버거, 실행계획 플로차트 ([version-history](https://www.sqlgate.com/product/version-history?language=ko), [pricing](https://www.sqlgate.com/pricing/subscription?language=ko)) |
| **dbForge Studio for Oracle** | Devart 2026.1 | .NET/Windows | Express 무료 / $169.95~$429.95 | 디버거, 비교, Profiler, AI |
| **Tibero 클라이언트** | TmaxTibero. tbAdmin → **Tibero Studio** 3.4.0(2026-04) | Java 추정 | 제품 내 제공 | Orange for Tibero, SQLGate for Tibero도 존재 |
| "OpenTOAD" | 존재 근거 없음 — 확인 필요 | | | |

**2026 국내 DB 툴 벤더**: 웨어밸리(Orange 7/8 판매 중), 체커(SQLGate, Oracle판 가장 활발), 티맥스티베로(Tibero Studio). 세 곳 모두 **Windows 네이티브 주력** → ★ macOS/Linux 국산 DBMS 클라이언트는 공백.

## 7. 비교표

| 도구 | 스택 | OS | 접속 | 가격(1user) | 설치 | SQL*Plus 스크립트 호환 | Oracle 특화 |
|---|---|---|---|---|---|---|---|
| Golden 8.4 | 네이티브 | Win | OCI 직접 | $60 영구 | 13~15MB | 높음 | ★★★★ |
| Orange 7/8 | 네이티브 | Win | OCI(32bit) | ₩1.46M 영구 | 800MB+ | 낮음(자체 콘솔) | ★★★★ |
| SQL Developer 24.3.1 | Java Swing | 전 OS | JDBC thin | 무료 | ~500MB | 중 | ★★★★★ |
| SQLDev VS Code + SQLcl 26.2 | TS/Java 17 | 전 OS | JDBC | 무료 | 수백MB | 매우 높음 | ★★★★★ |
| Toad 2026 R1 | 네이티브 | Win | OCI(IC 번들) | $625~$4,994/yr | 수백MB | 중~높음 | ★★★★★ |
| PL/SQL Developer 16 | 네이티브 | Win | OCI | ~$238 | ~50MB | 높음 | ★★★★★ |
| SQLGate 10.3 | 네이티브 | Win | OCI 64bit | 무료~구독 | — | 낮음 | ★★★★ |
| dbForge 2026.1 | .NET | Win | — | 무료~$430/yr | 136MB | 중 | ★★★ |
| SQLTools 2.0 | Win32 | Win | OCI | 무료 | 6MB | 중 | ★★★ |

## 8. ★ SQL*Plus 세션 변수/스크립트 기능 정리 (nexa-sql 구현 기준)

출처: SQL*Plus User's Guide 19c — [VARIABLE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/VARIABLE.html), [EXECUTE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/EXECUTE.html), [PRINT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/PRINT.html), [CONNECT](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/CONNECT.html), [SPOOL](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/SPOOL.html), [COLUMN](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/COLUMN.html), [SHOW](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/SHOW.html), [DESCRIBE](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/DESCRIBE.html), [슬래시](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/slash.html), [@@](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/double-at-sign.html), [SET 요약](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/SET-system-variable-summary.html), [스크립트](https://docs.oracle.com/en/database/oracle/oracle-database/19/sqpug/using-scripts-in-SQL-Plus.html), [치환 변수(23ai)](https://docs.oracle.com/en/database/oracle/oracle-database/23/sqpug/using-substitution-variables-sqlplus.html)

### 8.1 바인드 변수 (client-side, 세션 수명)
- 선언: `VAR[IABLE] [name [type [= value]]]`. 타입: `NUMBER`, `CHAR[(n [CHAR|BYTE])]`, `NCHAR[(n)]`, `VARCHAR2(n [CHAR|BYTE])`(기본 4000, EXTENDED면 32K), `NVARCHAR2(n)`, `CLOB`, `NCLOB`, `REFCURSOR`, `BINARY_FLOAT`, `BINARY_DOUBLE`. BLOB/BOOLEAN/VECTOR는 19c 문서에 없음. 인자 없는 `VARIABLE`은 목록, `VARIABLE name`은 단일 변수 타입 표시. `VAR abc NUMBER=123`, `VAR xyz VARCHAR2(10)='test'` 초기값 가능.
- 참조: SQL·PL/SQL 본문에서 `:name`. 블록 안 대입 `:ret_val := 4;`, `SELECT sal INTO :salary FROM emp ...;`, `OPEN :cv FOR SELECT ...;`, 프로시저 인자 `EXEC pkg.proc(:cv)`, 함수 반환 `EXEC :rc := fn`. SQL 문 입력 바인드(`INSERT ... VALUES(:abc)`, `SELECT :abc FROM dual`). SQL*Plus 자체 명령(`SET`, `COLUMN`)에서는 불가 → 치환 변수.
- `EXEC[UTE] statement`: 한 개의 PL/SQL 문을 `BEGIN statement; END;`로 감싸 실행. 줄 연속 `-`.
- `PRINT [name ...]`: 현재값 출력(없으면 전부). REFCURSOR는 **한 번만** PRINT 가능. `SET AUTOPRINT ON`이면 EXEC 직후 참조 변수 자동 출력.
- CLOB/NCLOB 바인드는 임시 LOB → `DBMS_LOB.FREETEMPORARY` 권장.

### 8.2 ★ 지속성 메커니즘 — nexa-sql이 복제해야 할 부분
- 문서상 *"Bind variables persist throughout the SQL*Plus session"*. **서버에는 아무 상태도 없다.** SQL*Plus는 클라이언트 메모리에 `{이름, 타입, 길이, 값 버퍼, (REFCURSOR면 OCI 문장 핸들)}` 테이블을 유지하고, **매 실행마다** 문장 텍스트를 스캔해 `:name`을 찾아 해당 버퍼를 `OCIBindByName`으로 IN/OUT 바인딩한 뒤 `OCIStmtExecute`한다 ([OCI 바인딩](https://docs.oracle.com/en/database/oracle/oracle-database/19/lnoci/binding-and-defining-in-oci.html)). 실행 후 OUT 버퍼에 남은 값이 곧 다음 문장의 IN 값이 되고 `PRINT`는 이 버퍼를 읽는다. REFCURSOR는 커서 핸들을 바인딩(`SQLT_RSET`)해 `PRINT` 시 fetch → 1회만 출력.
- `CONNECT`는 현재 트랜잭션을 커밋하고 재접속. 치환 변수는 CONNECT를 넘어 유지된다고 명시. 바인드 변수도 클라이언트 객체라 이름·값은 유지되나 열린 REFCURSOR 핸들은 무효화되는 것으로 알려짐 — 확인 필요. ★ nexa-sql은 **"세션 객체(연결) ↔ 바인드 테이블(워크시트 컨텍스트)"을 분리**해 설계.
- 치환 변수와의 차이: `&`/`&&`/`DEFINE`/`ACCEPT`/`COLUMN ... NEW_VALUE`는 **문장 전송 전 텍스트 치환**(클라이언트 전처리, 최대 2048개·240바이트, 단일 전역 네임스페이스). `&var`는 매번 프롬프트, `&&var`는 첫 입력을 DEFINE으로 저장. `SET DEFINE OFF/ON/char`, `SET SCAN`, `SET VERIFY`, `SET CONCAT .`, `SET ESCAPE \`. 바인드 변수는 서버로 실제 바인딩.

### 8.3 접속·스크립트·터미네이터
- `CONN[ECT] [{logon | / | proxy} [AS {SYSDBA|SYSOPER|...}] [edition=v]]`, logon = `user[/pw][@connect_identifier]`, EZConnect `host:port/service`, `proxyuser[user]/pw`, `/@wallet_alias`. 비밀번호 생략 시 프롬프트.
- `@file [args]` / `START`: 인자는 `&1 &2 …`. `@@file`은 **호출 스크립트와 같은 디렉터리** 기준. 스크립트가 바꾼 SET/COLUMN은 종료 후 유지.
- `;`는 SQL 문 종료. `/`는 **단독 행**에서 버퍼(직전 SQL 또는 PL/SQL 블록) 실행 — PL/SQL 블록 종결자. `SET SQLBLANKLINES ON`이면 문장 중 빈 줄 허용.
- `PROMPT`, `ACCEPT var [NUMBER|CHAR|DATE] [PROMPT 'text'] [HIDE]`, `PAUSE`, `CLEAR`.

### 8.4 SET / SHOW / DESC / SPOOL / COLUMN
- `SET SERVEROUT[PUT] {ON|OFF} [SIZE n|UNL] [FORMAT WRAPPED|WORD_WRAPPED|TRUNCATED]` — ON이면 실행 후 `DBMS_OUTPUT.GET_LINES` 폴링. 기본 OFF.
- `SET AUTOT[RACE] {ON|OFF|TRACEONLY} [EXPLAIN] [STATISTICS]` — STATISTICS는 **두 번째 세션**으로 V$ 수집, PLUSTRACE 롤.
- `SET TIMING`, `SET AUTOPRINT`, `SET FEEDBACK`, `SET LINESIZE/PAGESIZE`, `SET HEADING`, `SET TERMOUT`, `SET ECHO`, `SET AUTOCOMMIT`, `SET TRIMSPOOL`, `SET NULL text`, `SET LONG n`(기본 80), `SET ARRAYSIZE 1~5000`(기본 15), `SET DEFINE/VERIFY/SCAN/ESCAPE`.
- `SHO[W] {ALL|ERRORS [type name]|USER|PARAMETERS [name]|RELEASE|SGA|PDBS|CON_NAME|RECYCLEBIN|변수}` — `SHOW ERRORS`는 PL/SQL 편집기 필수.
- `DESC[RIBE] [schema.]object[@dblink]` — 테이블은 Name·Null?·Type, 프로시저는 인자·In/Out·Default.
- `SPO[OL] [file [CREATE|REPLACE|APPEND] | OFF | OUT]` — 기본 `.LST`.
- `COL[UMN] col [FORMAT A20|9,990.99] [HEADING] [NEW_VALUE var] [NOPRINT] [WRAP|TRUNCATED] [NULL text]` — SQLcl은 `NEW_VALUE`만 구현 → GUI 도구의 현실적 상한.

### 8.5 nexa-sql 구현 체크리스트
1. 세션별 바인드 테이블: `VARIABLE` 파서(타입/길이/초기값), `:name` 스캔 → 매 실행 바인딩(IN/OUT), OUT 값 회수, REFCURSOR 핸들 보관·1회 fetch.
2. `EXEC` → `BEGIN … END;` 래핑, `/` 단독 행 블록 종결, `;` vs `/` 규칙, `SET SQLBLANKLINES`.
3. 치환 변수 전처리기(`&`, `&&`, `DEFINE/UNDEFINE/ACCEPT`, `&1..&n`, `SET DEFINE/VERIFY/SCAN/ESCAPE/CONCAT`) — 서버 전송 전 단계로 분리.
4. `CONNECT` 인에디터 재접속(커밋 → 재연결 → 치환 변수 유지, 바인드 테이블 유지 정책).
5. `@/@@` 상대경로, `SET SERVEROUTPUT`(GET_LINES 폴링), `SET TIMING`, `SET AUTOTRACE`(2차 세션), `SPOOL`, `SHOW ERRORS/USER/PARAMETERS`, `DESC`, `COLUMN FORMAT/HEADING/NEW_VALUE`.
6. Golden `/*-*/` 로컬 파서 우회 접두어, SQLcl `/*csv*/` 인라인 포맷 힌트 — 검증된 관용구로 채택 검토.
