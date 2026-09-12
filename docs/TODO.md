# TODO — 순차 백로그

> ID · 우선(P0~P2) · 규모(소/중/대) · 의존 · 상태(☐/🚧/✅/⏸). 목표순.

## 1. 결정 (코드보다 먼저)
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-1** | P0 | 소 | DP-1~10 사용자 확정 → DR 승격([10 §2](10-decision-record.md)) | — | ☐ |
| **T-2** | P0 | 소 | D-4 사용자 실무 OS 확인(IME 구현 순서) · DP-10 법인 여부 | — | ☐ |

## 2. M1 — CLI 관통
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-3** | P0 | 중 | `nsql-net` — TCP/TLS(rustls) 스트림 · 취소 토큰 · 타임아웃 | T-1 | ☐ |
| **T-4** | P0 | 대 | `nsql-driver-oracle` — `Session` 구현 · 이름 바인드 IN/InOut · REF CURSOR OUT → `fetch_cursor` · DBMS_OUTPUT 폴링 · Instant Client 경로 설정 | T-3 | ☐ |
| **T-5** | P0 | 대 | `nsql-driver-mssql` — `sp_executesql` + OUTPUT 파라미터 회수 · `DeclarePrepend` 모드 · GO 배치 · 오류 줄 보정(`line_offset`) | T-3 | ☐ |
| **T-6** | P0 | 중 | `nsql run` — 파일/stdin · `&1..&n` · `WHENEVER` 종료 코드 · `SET SQLFORMAT` 텍스트 출력(grid/csv/json) | T-4 | ☐ |
| **T-7** | P1 | 중 | `nsql shell` — 줄 편집·히스토리·`PRINT`/`VARIABLE` 대화 | T-6 | ☐ |
| **T-8** | P1 | 중 | `nsql-io` export(csv/tsv/json/jsonl/insert) 스트리밍 | T-6 | ☐ |
| **T-9** | P1 | 소 | 엔진 보강 — `SET SERVEROUTPUT` 폴링 Action · `SPOOL` · `@`/`@@` 로드(호스트) · 엄격 모드(암묵 변수 금지) | — | ☐ |
| **T-10** | P1 | 소 | `cargo-deny` 라이선스 게이트 · `THIRD-PARTY-NOTICES` | T-4 | ☐ |

## 3. M2 — GUI 관통
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-11** | P0 | 중 | 셰이핑 스파이크(DP-6) — `rustybuzz`로 한글 조합·CJK 폭 · `nexa-gfx` 통합 | T-1 | ☐ |
| **T-12** | P0 | 대 | `nexa-edit` Buffer·Transaction·History(soft undo) — 순수 로직·테스트 | — | ☐ |
| **T-13** | P0 | 대 | `nexa-edit` Layout/View/Selections(멀티커서·컬럼) · **IME preedit 오버레이** | T-11 T-12 | ☐ |
| **T-14** | P0 | 중 | `.sublime-keymap` 파서 + context 평가기 + Command 레지스트리 | T-12 | ☐ |
| **T-15** | P0 | 대 | `nexa-grid` 가상화(10만 행 · 컬럼 리사이즈/정렬/고정 · TSV 복사) | nexa-ui U-2 | ☐ |
| **T-16** | P0 | 중 | 창 하나 관통: 편집기 → 엔진 → 워커 → 그리드 · 연결 프로필(키체인 D-2) | T-13 T-15 | ☐ |

## 4. M3+ (요약)
syntect `.sublime-syntax`(T-17) · 컬러스킴/스니펫/완성(T-18) · `Default` 패키지(T-19) · `Catalog` 포트 + 오브젝트 브라우저(T-20) · import/bulk 6방언(T-21) · 데이터 편집기 변경 SQL 미리보기(T-22) · PG/MySQL/SQLite/ODBC 드라이버(T-23) · WASM 플러그인 API(T-24) · 릴리스 파이프라인·서명(T-25) · 라이선스 키(T-26).
