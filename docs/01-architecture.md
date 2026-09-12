# 01 · 아키텍처 — 계층 독립 · 포트 연계

> 사용자 요구(09-12): *"UI(IDE) 파트와 Core 기능, DBMS 연결 등 네트워크 기능, 네트워크를 통한 DBMS 연결 어댑터 등 아키텍처가 **독립적이면서 연계**되도록."* + *"DBMS별 차이를 극복하면서 단일 앱으로 가능한가"*.
> 답: **단일 앱 + 4계층 + 포트(trait) 연결**. 계층은 서로를 모르고, 허브(`nsql-core`)의 타입과 트레이트로만 말한다. 각 계층은 화면·네트워크 없이 `cargo test`로 검증된다.

## 1. 4계층과 의존 방향

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ ④ UI (IDE)          nexa-sql(bin)  ·  nsql-cli(bin `nsql`)                    │
│    nexa-ui(공용): nexa-gfx · nexa-ctl · nexa-edit(편집기) · nexa-grid · dock  │
│    화면 S1~Sn · 패키지 로더 · 키맵 · 설정(nexa-conf)                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ③ Core              nsql-core(허브 · 의존 0) · nsql-script(엔진) · nsql-io    │
│    Dialect·Value·ResultSet·Session/Catalog 포트 · 스크립트/세션 변수 · export/import/bulk │
│    nsql-catalog(오브젝트 브라우저 모델) · nsql-history · nsql-license            │
├──────────────────────────────────────────────────────────────────────────────┤
│ ② Network           nsql-net                                                  │
│    TCP/TLS(rustls) · SSH 터널 · 프록시 · keepalive · 취소 토큰 · 타임아웃 ·      │
│    연결 풀 · 자격 증명(OS 키체인) — DBMS 프로토콜을 모른다                        │
├──────────────────────────────────────────────────────────────────────────────┤
│ ① Adapters          nsql-driver-oracle · -mssql · -postgres · -mysql · -sqlite · -odbc · -rpc(플러그인) │
│    각 DBMS 프로토콜/클라이언트를 `Session`·`Catalog`·`Bulk` 포트로 번역            │
└──────────────────────────────────────────────────────────────────────────────┘
      의존 방향:  ④ → ③ ← ①      ① → ②      (③은 ①②④를 모른다 · ④는 ①②를 모른다)
```

- **UI는 드라이버를 모른다**: UI가 아는 것은 `Box<dyn Session>`뿐. 드라이버 교체(예: kubo `oracle` → 공식 `oracledb`)가 UI 코드 0줄 변경.
- **Core는 네트워크를 모른다**: 엔진은 `Action`을 내고 결과를 `absorb`할 뿐. 그래서 `nsql plan`이 DB 없이 돈다.
- **Adapters만 외부 크레이트를 안다**: 드라이버 크레이트(DR-3 예외)는 어댑터 안에 갇힌다. 공개 시그니처에 외부 타입 0(clip DR-8 · nexa-ctl 규칙과 동일).
- **Network는 프로토콜을 모른다**: 어댑터가 `nsql-net`의 스트림(TCP/TLS/SSH)을 받아 그 위에 TDS·TNS·PG 프로토콜을 얹는다. 순수 Rust 드라이버는 이 경로, ODPI-C/ODBC처럼 클라이언트 라이브러리가 소켓을 소유하는 어댑터는 `nsql-net`의 **터널·자격 증명만** 쓴다.

## 2. 포트 (허브 `nsql-core`가 정의)

| 포트 | 구현자 | 소비자 | 지금 |
|---|---|---|---|
| `Session` — `execute(ExecRequest)→ExecResult` · `fetch_cursor` · `commit/rollback` | 어댑터 | 엔진 호스트(UI·CLI) | ✅ 정의됨 |
| `Catalog` — 스키마/오브젝트 열거 · DDL 추출 · describe | 어댑터(+`nexa-dialect.json` 질의 템플릿) | 오브젝트 브라우저 · 자동완성 · CLI `desc` | ☐ |
| `Bulk` — 배열 바인딩/COPY/BulkLoad 스트림 | 어댑터 | `nsql-io` | ☐ |
| `Transport` — 스트림 열기(TCP/TLS/SSH) · 취소 · 자격 증명 | `nsql-net` | 어댑터 | ☐ |
| `Widget`/`DrawCtx` — 그리기 어휘 | `nexa-ui` | 화면 | ✅ (nexa-ui) |

## 3. 실행 흐름 (한 문장 실행)

```text
편집기(④) ─텍스트─▶ split_script/Engine::plan(③) ─Action::Execute(Prepared)─▶ 워커 스레드
   ▲                                                                           │ Session::execute(①) ─ Transport(②) ─ DBMS
   └─────── ResultSet/ExecResult(③ 타입) ◀── 채널 ◀────────────────────────────┘
그리드(④)는 ResultSet을 그린다 · Engine::absorb(③)가 OUT 값을 변수 저장소에 넣는다
```
UI 스레드는 기다리지 않는다(clip DR-41). 취소는 `nsql-net`의 취소 토큰 → 어댑터가 프로토콜 취소(Oracle `OCIBreak` · TDS Attention · PG `CancelRequest`)로 번역.

## 4. 왜 DBMS별 앱이 아니라 단일 앱인가 (D-1 답)

| 관점 | 단일 앱 + 방언 계층 | DBMS별 앱 |
|---|---|---|
| 차이의 실체 | 바인드 문법 · 배치 규칙 · 카탈로그 질의 · 타입 매핑 = **데이터(방언 정의) + 얇은 재작성**([08 §4](08-session-variables.md)) | 같은 편집기·그리드·설정을 N번 |
| 사용자 | 한 도구로 Oracle↔MSSQL 오가며 **같은 변수·같은 키맵** | 도구마다 다른 관습 |
| 위험 | 방언 정의가 커지면 "최소공배수" 함정 → **1급 방언(Oracle·MSSQL)은 전용 기능을 패키지로 깊게**(Golden·PL/SQL Developer 수준), 2·3급은 공통 기능만 | 유지보수 N배 |
| 결론 | ★ **단일 앱** — 단, 배포는 필요시 "Oracle 에디션"처럼 **패키지 묶음만 다르게** 낼 수 있다(코드는 하나) | — |

## 5. 크레이트 현황 (2026-09-12 2차)

| 크레이트 | 계층 | 상태 | 테스트 |
|---|---|---|---|
| `nsql-core` | ③ 허브 | ✅ Dialect·Value·VarType·`Session`(set_option·describe) | 4 |
| `nsql-script` | ③ 엔진 | ✅ 오프셋 기반 분리·명령·바인드·방언·엔진·CONNECT | 34 |
| `nsql-io` | ③ | ✅ export 6형식 · CSV 파서 · CJK 폭 | 5 |
| `nsql-run` | ③ 오케스트레이션 | ✅ Action 실행 · OUT 흡수 · 이벤트 · 프롬프트 | 3 |
| `nsql-driver-sqlite` | ① | ✅ 실DB | 2 |
| `nsql-driver-oracle` | ① | ✅ 실서버 검증(integration) | 2(통합) |
| `nsql-driver-mssql` | ① | ✅ 실서버 검증(integration) · 배치 라우팅 | 3 + 2(통합) |
| `nsql-drivers` | ①→④ 경계 | ✅ 레지스트리(feature) | 1 |
| `nsql-cli`(`nsql`) | ④ | ✅ plan/run/shell/export | — |
| `nexa-sql` | ④ | ✅ 최소 창(TextBox·자체 그리드·워커) · ⏳ 실기 | — |
| `nexa-ui/*` | ④ 공용 | ✅ gfx·ctl·conf·**font** | 194 |
| `nsql-net` · `nexa-edit` · `nexa-grid` · `nsql-catalog` · 세션/캐시 | ②④③ | ☐ [17](17-editor-incremental-plan.md) [18](18-session-and-projects.md) [19](19-compare-git-and-object-history.md) | — |
