# TODO — 순차 백로그

> ID · 우선(P0~P2) · 규모(소/중/대) · 의존 · 상태(☐/🚧/✅/⏸). 목표순.

## 1. 결정 (코드보다 먼저)
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-1** | P0 | 소 | DP-1~10 사용자 확정 → DR 승격([10 §2](10-decision-record.md)) | — | ✅ 09-12(DR-10~20) |
| **T-2** | P0 | 소 | ★ D-15~D-17 사용자 답(다음 우선순위 · 실기 OS · 캐시 위치) → DR 승격 · D-14는 DR-21 | — | 🚧 |
| **T-2b** | P0 | 소 | `integration` 워크플로 green → 실서버 결함 3건 수정 · T-4/T-5 닫힘 | — | ✅ 09-12 |
| **T-2c** | P1 | 소 | 사내 Oracle(192.168.0.55/58) 실접속 — `sqlplus` · `nsql run`(사용자 네트워크에서만) | T-4 | ⏳ 사용자 |

## 2. M1 — CLI 관통
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **T-3** | P1 | 중 | `nsql-net` — 취소 토큰(OCIBreak·TDS Attention) · 타임아웃 · SSH 터널(어댑터가 소켓 소유 → 터널·자격증명만) | — | ☐ |
| **T-4** | P0 | 대 | `nsql-driver-oracle` — ✅ 구현 · ✅ **실서버 검증**(Oracle Free 23ai · integration) · ✅ Instant Client 경로(`NSQL_ORACLE_CLIENT_DIR`)·DPI-1047 안내 · ☐ 취소(OCIBreak) | — | ✅ 09-12 |
| **T-5** | P0 | 대 | `nsql-driver-mssql` — ✅ 구현 · ✅ **실서버 검증**(SQL Server 2022 · integration) · ☐ Entra/통합 인증 · 오류 줄 보정 | — | ✅ 09-12 |
| **T-6** | P0 | 중 | `nsql run` — 파일/stdin · `&1..&n` · `WHENEVER` · 형식 출력 | — | ✅ 09-12 |
| **T-7** | P1 | 중 | `nsql shell` — ✅ 최소(줄 누적 실행) · ☐ 줄 편집·히스토리·자동완성 | — | 🚧 |
| **T-8** | P1 | 중 | `nsql-io` export — ✅ 6형식 · ☐ 스트리밍(행 단위 fetch) · xlsx/parquet | — | 🚧 |
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
| **T-16** | P0 | 중 | 창 하나 관통 — ✅ 최소(TextBox·자체 그리드·워커) · **T-16b** 클립보드·컨텍스트 메뉴·연결 프로필(키체인) · **T-16c** 치환 변수 대화상자 · **T-16d** 세션 hot exit([18](18-session-and-projects.md)) | — | 🚧 |

## 3-1. 편집기 E1~E9([17](17-editor-incremental-plan.md)) · 세션([18](18-session-and-projects.md)) · 외부 변경([15](15-external-file-changes.md)) · 비교/git/오브젝트 캐시([19](19-compare-git-and-object-history.md))
| ID | 우선 | 규모 | 항목 | 의존 | 상태 |
|---|:--:|:--:|---|---|:--:|
| **E-1** | P0 | 대 | `nexa-edit` E1 Buffer(로프·인덱스·CP949·Transaction·History) | D-9 | ☐ |
| **E-2** | P0 | 중 | E2 Selection · E3 Commands+Keymap(Default 키맵 데이터) | E-1 | ☐ |
| **E-3** | P0 | 대 | E4 Display · E5 멀티커서 → **GUI 재정비 ①** | E-2 T-11 | ☐ |
| **S-1** | P0 | 중 | 세션 hot exit(유휴 2s/30s 원자 저장 · 복원) · 프로젝트/워크스페이스 파일 | T-16 | ☐ |
| **X-1** | P1 | 중 | 외부 변경 감지(워처·해시·fast-forward) · 비모달 배지 | S-1 | ☐ |
| **C-1** | P1 | 중 | 비교 뷰 2-pane(`similar`) · 스크립트 저장 시점 스냅샷 · git gutter | E-3 | ☐ |
| **O-1** | P1 | 대 | 오브젝트 시점 캐시(열기/컴파일 전후) · 복원 명령 | C-1 Catalog | ☐ |
| **X-2** | P2 | 대 | 3-pane 병합 | C-1 X-1 | ☐ |

## 4. M3+ (요약)
syntect `.sublime-syntax`(T-17) · 컬러스킴/스니펫/완성(T-18) · `Default` 패키지(T-19) · `Catalog` 포트 + 오브젝트 브라우저(T-20) · import/bulk 6방언(T-21) · 데이터 편집기 변경 SQL 미리보기(T-22) · PG/MySQL/SQLite/ODBC 드라이버(T-23) · WASM 플러그인 API(T-24) · 릴리스 파이프라인·서명(T-25) · 라이선스 키(T-26).
