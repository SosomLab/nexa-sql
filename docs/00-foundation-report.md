# 00 · Nexa SQL 기반 보고서 — 언어 · 플랫폼 · 아키텍처 · 지원 범위 · 조사 종합

> 작성 2026-09-12 · 대상 독자 = 프로젝트 오너(사용자). 이 문서는 **첫날의 모든 조사·평가·결정·산출물을 한 장으로 잇는 허브**다. 세부는 각 문서로 링크한다.
> 상태 요약: 조사 5건 완료 · 공용 UI 라이브러리 추출 완료(189 테스트) · 세션 변수 엔진 구현(37 테스트) · `nsql plan` 동작 · 사용자 확인 대기 DP 10건([10 §2](10-decision-record.md)).

---

## 1. 요청과 답 (한눈에)

| 사용자 질문/요청 | 답 · 근거 | 문서 |
|---|---|---|
| Rust 기반 앱이 안정적으로 느껴지는데 분석/평가해 달라 | ★ **Rust 올 네이티브가 맞다**(§2). 단 "편집기와 셰이핑"이 최대 비용이며 이는 어떤 프레임워크를 써도 같다 | [06](06-rust-ecosystem.md) |
| 개발 언어 · 플랫폼 · 아키텍처 · 지원 범위 결정 | Rust / 3-OS(Win·mac·Linux) 동일 화면 / **4계층 + 포트** / Oracle·MSSQL 1급 → PG·MySQL·SQLite 2급 → 국산 DBMS ODBC 3급 | [01](01-architecture.md) · [10](10-decision-record.md) |
| DBeaver 같은 크로스플랫폼 앱 · OS별 대표 앱 조사 | 크로스플랫폼 14종 + Rust/Tauri 신흥 7종 · Oracle 12종 · MSSQL 18종 조사. 공백 = **"DBeaver 커버리지 × TablePlus 가벼움 + Oracle/국산 DBMS + DBA 워크플로"** | [03](03-competitive-landscape.md) [04](04-oracle-tools.md) [05](05-mssql-tools.md) |
| Golden · PL/Edit · Orange · SSMS 등 | Golden 8.4 현역($60 · 13MB · OCI 직접) · Orange 7/8 **아직 판매 중**(2026-01 게시) · SSMS 22는 VS 2026 셸(20~50GB) · Azure Data Studio 2026-02-28 은퇴 | [04](04-oracle-tools.md) [05](05-mssql-tools.md) |
| DBMS별 차이 극복 + 단일 앱 가능? DBMS별 앱? | ★ **단일 앱**. 차이는 "방언 데이터 + 얇은 재작성"으로 흡수됨을 코드로 실증 | [01 §4](01-architecture.md) [08](08-session-variables.md) |
| sqlplus 변수(선언·재사용·refcursor)·`connect` 인에디터 · SQL Server에서도 변수 공유 | ★ **구현됨**(`nsql-script`): 변수는 클라이언트에 산다 · 리터럴 대입은 로컬 · MSSQL은 `sp_executesql` OUTPUT + `SELECT @X = …` 변환 + `DECLARE` 프리펜드 폴백 | [08](08-session-variables.md) |
| 컨트롤을 별도 라이브러리로 먼저 | ★ **`../nexa-ui` 생성·커밋** — clip의 gfx/ctl/conf 이관, 189 테스트 green | [nexa-ui/CLAUDE.md](../../nexa-ui/CLAUDE.md) |
| 편집기 = Sublime 차용 · 패키지로 확장 · VS Code/IntelliJ 등 기능 목록 | 39항목 체크리스트(P0/P1/P2) · Sublime 패키지 파일 형식 **그대로 채택**(syntect) · 플러그인 = WASM(dir2 선례) | [07](07-editor-research.md) [09](09-editor-and-packages.md) |
| CLI 도구 · export/import · bulk insert · 드라이버 | `nsql` 명령 체계 · 방언별 네이티브 bulk 경로 · 드라이버 순위표 · **CLI 관통(M1)을 GUI보다 먼저** | [11](11-cli.md) [02](02-roadmap.md) |
| UI · Core · Network · 어댑터 독립·연계 | 4계층 의존 규칙(④→③←①, ①→②) · 포트 5종 | [01](01-architecture.md) |
| 영리 목적만 유료 라이선스 | PolyForm NC 1.0.0 영문 정본 + 한글본 적용(양 저장소) | [13](13-licensing.md) |

## 2. 언어·스택 평가 — 왜 Rust 올 네이티브인가

**후보 4개**를 [06 Part B](06-rust-ecosystem.md)의 6개 기준(경량·IME·에디터·도킹·3-OS·팀 적합)으로 채점하면 egui 24 · Tauri 23 · GPUI 23 · iced 22 · Slint 19다. 그런데 **egui가 점수를 얻는 항목은 전부 `nexa-ui`(자체 래스터·컨트롤 17종·디자인 토큰)도 얻는다** — 계열이 이미 3제품(clip·beep·dir2)으로 증명한 층이다. 반대로 Tauri가 이기는 항목(Monaco 에디터·웹 그리드)은 "3개 웹뷰 엔진 CSS 불일치"(QoreDB 저자)와 10만 행 그리드에서 150MB 초과라는 대가가 있고, 사용자 판단(*"Rust 기반이 안정적"*)과도 어긋난다.

| 판단 | 내용 |
|---|---|
| ✅ 강점 | 단일 바이너리·기동 <1s·낮은 RSS(TablePlus급) · 3-OS 동일 화면(계열 DR-1) · 드라이버 생태계가 2026년에 성숙: **Oracle 공식 순수 Rust thin 드라이버 베타(2026-08)** · Microsoft `mssql-tds`(2026-09-10) · `tiberius` 유지보수 재개 · PG/MySQL/SQLite 안정 |
| ⚠️ 비용 | ① **편집기**(멀티커서·구문·완성·IME) 자체 구현 — 프레임워크 무관하게 필요 · 최고 난이도(사용자 인식과 일치) ② **텍스트 셰이핑**(한글 조합·CJK 폭) 부재 → 크레이트 1개 도입(DP-6) ③ 도킹·가상화 그리드는 dir2에 원형이 있으나 이식 필요 |
| ❌ 기각 | Electron/Tauri(경량·팀 성향 충돌) · Qt(라이선스·팀 기피) · Java(DBeaver의 약점 그 자체) |
| 결론 | **DR-1 Rust 올 네이티브 · `nexa-ui` 확장**. 리스크는 "편집기 늪" 하나이며 Helix 구조 고정 + 단계 테스트 + P0 외 기능 패키지化로 관리([09 §5](09-editor-and-packages.md)) |

## 3. 시장에서의 자리 (조사 종합)

- **크로스플랫폼**([03](03-competitive-landscape.md)): DBeaver(Java · 0.5~2GB+) · DataGrip(Java · 구독) · TablePlus(네이티브 3벌 · 경량) · Beekeeper/DbGate(Electron). Rust/Tauri 신흥(Tabularis 4.9k★ · QoreDB 30MB)은 **Oracle을 못 다룬다**. ADS 은퇴로 MSSQL 크로스플랫폼은 VS Code 확장뿐.
- **Oracle**([04](04-oracle-tools.md)): 사용자가 호감을 가진 Golden(단일 EXE · OCI 직접 · SQL*Plus 관습)과 PL/SQL Developer(SQL/Test/Command Window 3분할 · 바인드 스캔 그리드)의 미덕이 곧 우리 사양. Orange는 여전히 판매 중이나 32bit 종속·Windows 전용. **SQLcl이 남긴 SQL*Plus 부분집합**이 우리 구현 범위의 기준.
- **MSSQL**([05](05-mssql-tools.md)): SSMS는 "덩치만 빼면 좋다"는 사용자 평 그대로(VS 셸 · Windows 전용). macOS/Linux에서 **가볍고 스크립트 친화적(SQLCMD 호환 + 세션 변수)** 인 도구는 없다.
- **국산 DBMS**: 웨어밸리·체커·티맥스 모두 Windows 네이티브 주력 → macOS/Linux 공백.
- ★ **포지셔닝 한 줄**: *"Golden의 가벼움과 SQL*Plus 관습 · SSMS/PL/SQL Developer의 DBA 워크플로 · DBeaver의 커버리지를, 3-OS 한 벌 화면과 한 개의 CLI로."*

## 4. 아키텍처 요약 ([01](01-architecture.md))

```text
④ UI(IDE)  nexa-sql · nsql(CLI) · nexa-ui(gfx·ctl·edit·grid·dock)
③ Core     nsql-core(허브 · 포트) · nsql-script(세션 변수 엔진 ✅) · nsql-io(export/import/bulk)
② Network  nsql-net(TCP/TLS/SSH · 취소 · 풀 · 키체인)
① Adapters nsql-driver-{oracle,mssql,postgres,mysql,sqlite,odbc,rpc}
의존: ④→③←①, ①→② · Core는 화면·네트워크를 모른다 → `nsql plan`이 DB 없이 돈다
```

## 5. 지원 범위 (DP-2)

| 급 | DBMS | 드라이버 | 깊이 |
|---|---|---|---|
| 1급 | **Oracle** · **SQL Server** | `oracle`(ODPI-C) → 공식 thin · `tiberius-ng` → `mssql-tds` | PL/SQL·T-SQL 전용 패키지 · 세션 변수 · 실행계획 · 세션 모니터 · bulk |
| 2급 | PostgreSQL · MySQL/MariaDB · SQLite | 순수 Rust | 편집·그리드·export/import·bulk |
| 3급 | Tibero · Altibase · CUBRID · 기타 | `odbc-api` | 공통 기능 |
| 확장 | JDBC 전용 등 | stdio JSON-RPC 플러그인 | 언어 무관 |

## 6. 오늘 만든 것 (산출물)

| 저장소 | 산출 | 검증 |
|---|---|---|
| `../nexa-ui` (신규 · 커밋 `d182eee`) | `nexa-gfx`(745) · `nexa-ctl`(12,910 · 컨트롤 17종) · `nexa-conf`(599) · 문서 골격 · CI | fmt ✓ · clippy `-D warnings` ✓ · **189 테스트** |
| `nexa-sql` (신규) | `nsql-core`(포트·값 모델) · `nsql-script`(분리·명령·바인드·방언·엔진·CONNECT · 2,400 LOC) · `nsql`(plan) · `nexa-sql`(자리) · 문서 13건 · 라이선스 | fmt ✓ · clippy ✓ · **37 테스트** · `examples/golden-session-vars.sql` Oracle/MSSQL dry-run ✓ |

`nsql plan --dialect mssql examples/golden-session-vars.sql` 발췌:
```text
[1] EXEC :V_PRG_NM := '…'      → LOCAL :V_PRG_NM = N'SP_M4P_MPO_M4E_CREATE_BSY'   (DB 왕복 0)
[4] EXEC SELECT … INTO :V_…    → SELECT @V_PROJECT_CD = A.PROJECT_CD, … FROM M4S_O301010 A …  (InOut ×5 · OUTPUT 회수)
[5] SELECT :V_PROJECT_CD, …    → SELECT @V_PROJECT_CD, … FROM DUAL   · V_PRG_NM Varchar2(25) In = N'SP_…'
[6] connect user/pass@…        → CONNECT user/***@192.168.1.1:1521/db
```

## 7. 사용자 확인이 필요한 결정 (DP · [10 §2](10-decision-record.md))

1. DP-1 단일 앱 · 2. DP-2 지원 범위 순위 · 3. DP-3/4 Oracle·MSSQL 드라이버 선택 · 4. DP-5 결과셋 모델(자체 컬럼형) · 5. DP-6 셰이핑 크레이트(`rustybuzz`) · 6. DP-7 플러그인 런타임(WASM) · 7. DP-8 예산 수치 · 8. **DP-9 CLI 먼저 관통** · 9. DP-10 서명(법인 여부).

## 8. 다음 단계 ([02](02-roadmap.md) M1)

`nsql-net`(TCP/TLS) → `nsql-driver-oracle` + `nsql-driver-mssql` → `nsql run/shell` 실접속 → 세션 변수 실기(REFCURSOR `PRINT` · `sp_executesql` OUTPUT) → `export csv/json`. 병행: `nexa-ui` U-2(dock/tab 이식) · D-3/DP-6 셰이핑 스파이크.
