# 128 — GUI 크레이트 시범 분할 설계안(확장 · AI) — 초안

> 📐 **설계안 초안(11-23 · 협업 작성 · 코드 변경 없음)** — D-490 로컬 ③ "nexa-sql 크레이트 분할 = 설계안부터"(사용자 11-21) · 11-23 사용자 답 "설계안부터"(journal 11-22 §16 #14 · §22). 결정 전 문서다 — 진행 여부·순서는 사용자가 정한다.
> 근거 = 11-23 main(b1a76cd0) 소스를 읽기만 해서 센 수(grep · wc) · 빌드 시간은 새로 재지 않았다(개발이 소스를 고치는 중이라 cargo를 쓰지 않음 → [journal 11-21 §31](journal/2026-11-21.md) B-2 측정값으로 갈음).

## 0. 결론 다섯 줄

1. **확장(extensions/) 쪽은 거의 그대로 뗄 수 있다** — `crates/nexa-sql/src/extensions/` 5파일 3,991줄은 `crate::` 참조가 **0**이다(`super::`와 외부 크레이트 `nsql_settings` · `nsql_i18n` · `nexa_ctl` · `nsql_seq` · `wasmi`만 쓴다). 남는 일은 가시성(`pub(crate)` → `pub`)과 `App` 쪽 사용 자리 고치기(약 84곳)다.
2. **AI(ai/) 쪽은 먼저 끊을 것이 7모듈 · 약 25곳** — `ai/` 19파일 15,624줄이 nexa-sql 안의 도우미 7개(`toolicons` 11 · `clipboard` 3 · `png` 2 · `menu_icons` 2 · `palette` · `sessions` · `ext_panel` 각 1)를 부른다. 이 도우미들을 아래 크레이트로 내리거나 포트로 넘긴 뒤에야 뗄 수 있다.
3. **`impl App` 접착 코드는 nexa-sql에 남는다** — `app/extensions.rs` 2,752줄 · `app/ai.rs` 6,332줄 · `app/ai_editor.rs` 876줄(합 9,960줄)은 `App` 필드를 직접 만지므로 옮기지 않는다. 그래서 떼어 내는 몫은 nexa-sql 196,263줄 중 **확장 2.0 % · AI 8.0 %**뿐이다.
4. **빌드 이득은 작을 것으로 본다(추정 · 미측정)** — nexa-sql을 고칠 때 다시 컴파일하지 않게 되는 몫이 약 10 %다. 11-21 측정의 "실제 편집 증분 13.2 s"에 대면 **편집마다 1~1.5 s 안팎**이다. 반대로 AI·확장 쪽을 고치면 그 크레이트와 nexa-sql 둘 다 다시 컴파일된다. 효과는 시범 뒤 `cargo build --timings`로 재야 한다.
5. **추천 = 확장 쪽만 먼저 시범으로 뗀다**(경계가 이미 깨끗해 위험이 작고, 분할 절차와 측정 방법을 시험하는 값이 있다). 측정 이득이 1 s 미만이면 AI 쪽은 하지 않는다 — 이득에 비해 고치는 자리(가시성 · 도우미 이전 · 시험 위치)가 많다.

## 1. 지금 구조(11-23 · 줄 수)

| 묶음 | 파일 | 줄 | 바깥(nexa-sql 안) 참조 |
|---|---|---|---|
| 확장 본체 | `extensions/` mod · wasm · manager · insert_sequence · rainbow_pairs | 3,991 | `crate::` **0** |
| 확장 접착 | `app/extensions.rs` | 2,752 | `impl App` |
| AI 본체 | `ai/` 19파일(view 4,977 · mod 1,323 · guard 1,280 · host 1,234 · panel 1,166 · claude_api 1,038 · tools 844 · codex 739 · wif 619 · picker 526 · mcp 460 · claude_cli 364 · e2e 307 · skills 201 · hooks 159 · diff 114 · agents 100 · 그 밖) | 15,624 | `crate::` 7모듈 · 약 25곳 |
| AI 접착 | `app/ai.rs` · `app/ai_editor.rs` | 7,208 | `impl App` |
| nexa-sql 전체 | `crates/nexa-sql/src/**/*.rs` | 196,263 | |

## 2. 의존 방향과 끊어야 할 참조

### 2-1. 확장

- **안 → 밖**: 없음(`crate::` 0). 외부 = `nsql_settings` 7 · `nsql_i18n` 2 · `nexa_ctl` 2 · `nsql_seq` 1 · `wasmi` 1 · `std`.
- **밖 → 안**(`extensions::` 쓰는 자리): `app/extensions.rs` 62 · `main.rs` 13 · `app/ai_editor.rs` 4 · `app/format.rs` 2 · `app/startup_cmd.rs` 1 · `app/insseq.rs` 1 · `app/ai.rs` 1 = **약 84곳**.
- `App` 필드 = `extensions: Registry` · `ext_catalog` · `ext_staged` · `ext_held` · `ext_ev: ExtEvents` · `ext_meta_cache`(main.rs:670-681) — 형식만 새 크레이트 경로로 바뀐다.
- 이미 포트가 있다: `EditorOps`(편집기 조작) · `Extension` · `ExtensionEffect` · `Command` · `MenuContribution`(extensions/mod.rs).

### 2-2. AI

- **안 → 밖**(끊을 것):

| 모듈 | 자리 | 쓰는 것 | 처리 후보 |
|---|---|---|---|
| `toolicons` | 11 | `mi_add` · `mi_key` · `mi_check` · `mi_copy` · `mi_edit` · `mi_run` · `mi_swap` · `mi_table` · `mi_text` | 아이콘 함수를 작은 공용 크레이트로(nexa-ui 쪽 아이콘 묶음과 합칠지 = 개발 판단) |
| `clipboard` | 3 | `write_text` · `read_text` | 공용 크레이트로 내림(GUI 여러 곳이 씀) |
| `png` | 2 | `encode` · `decode` | 공용 크레이트로 내림 |
| `menu_icons` | 2 | `open_at` | 포트(메뉴 열기 콜백) 또는 공용 |
| `palette` | 1 | `fuzzy_score_lower` | `nsql-search`(이미 있음)로 옮길 후보 |
| `sessions` | 1 | `arm_ratio` | 순수 함수 → 공용 |
| `ext_panel` | 1 | `ellipsize_end` | 그리기 도우미 → nexa-ctl 쪽 후보 |

- **밖 → 안**: `app/ai.rs` 128 · `app/ai_editor.rs` 15 · `main.rs` 6 · `app/menus.rs` 5 · `app/toolbar.rs` 2 · `app/paint.rs` 2 · `explorer.rs` · `app/run.rs` · `app/events.rs` 각 1 = **약 161곳**.
- `App` 필드 = `ai_panel: ai::panel::AiPanel` · `ai_view: ai::view::AiView` · `ai: app::ai::AiState`(main.rs:711-713).
- 외부 = `nexa_ctl` 17 · `nsql_settings` 8 · `nsql_i18n` 6 · `nsql_ext` 1 · `nsql_core` 1 · `std` — AI 크레이트는 UI 크레이트(`nexa-ctl`)에 의존한다(DR-7 계층 = UI 쪽 크레이트).

## 3. 포트(trait)로 뽑을 경계 후보

- **확장**: 새로 만들 것 없음 — `EditorOps` · `Extension`이 이미 경계다. `Registry`의 설정 읽기(`nsql_settings`)는 그대로 둔다.
- **AI**: ① `AiHost`(이미 `ai/host.rs`) = 앱에 묻는 것(편집기 본문 · 세션 · 실행)의 포트 — 접착 코드가 구현 ② 아이콘·클립보드·메뉴 열기를 한 포트(`UiServices` 같은 이름)로 묶어 넘기는 안 vs 공용 크레이트로 내리는 안 — 공용 크레이트가 단순하다(함수 9 + 4).

## 4. 증분 빌드 몫(추정)

- 11-21 §31 B-2 C0(지금 설정) = **실제 편집 증분 13.2 s**(Debug · nexa-sql 한 크레이트).
- 떼어 낸 뒤 nexa-sql만 고치는 경우 = 확장·AI 코드(10 %)를 다시 컴파일하지 않음 → 줄 수 비례로 **약 1.3 s 절감 추정**(rustc 증분이 이미 바뀌지 않은 함수를 재사용하므로 실제는 더 작을 수 있다).
- AI·확장 쪽을 고치는 경우 = 그 크레이트 + nexa-sql(의존 쪽) 둘 다 다시 컴파일 → 지금보다 느려질 수 있다.
- 링크 시간은 그대로(exe 하나).
- **측정 방법(시범 때)** = 분할 전후로 `cargo build --timings -p nexa-sql`(nexa-sql 파일 하나 touch · AI/확장 파일 하나 touch · 각 3회 중앙값) · 결과 = 이 문서 §4에 표로.

## 5. 위험과 순서(docs/93 절차를 그대로)

| 순서 | 할 일 | 위험 |
|---|---|---|
| 1 | 원복 태그 · 기준선(시험 수 · clippy 0 · V1 all · `--timings` 전 측정) | — |
| 2 | **확장**: `crates/nsql-ext-host`(이름은 개발 판단) 새 크레이트 · `extensions/` 이동 · `pub` 정리 · nexa-sql 사용 자리 경로 바꿈 | 낮음 — `crate::` 0 · 시험이 모듈 안에 있으면 함께 이동 |
| 3 | 검증(V2 + V1 color-list · ext-diff · 확장 관련) · `--timings` 후 측정 | 낮음 |
| 4 | 측정 이득 판단 → AI 진행 여부(사용자) | — |
| 5 | (진행 시) AI 도우미 7개를 공용 크레이트·nsql-search·nexa-ctl로 먼저 이동 → 기존 시험 통과 | 중간 — 도우미 이동이 다른 화면 코드에도 퍼진다 |
| 6 | (진행 시) `crates/nsql-ai`(이름 = 개발) 이동 · `AiHost` 경계 · e2e 시험 자리 | 중간 — `ai/e2e.rs` · 실서버 CLI 연동 시험 경로 · 기능 플래그 |

- 공통 위험: `pub(crate)`이 `pub`이 되면서 경계가 넓어짐(공개 항목 수를 기준선과 비교) · `Msg` 키(i18n)는 이미 별도 크레이트라 영향 없음 · 설정 키(`REGISTRY`)는 이미 `nsql-settings`라 영향 없음 · `#[cfg(test)]` 도우미 공유.
- 하지 말아야 할 경우: 측정 이득이 1 s 미만이고 개발 중인 기능(T-105 · T-172 P3 · O-1)과 같은 파일을 건드리는 시기 — 충돌 비용이 이득보다 크다.

## 6. 결정 후보(사용자)

- ① 확장 쪽 시범 분할 진행 여부(추천 = 진행 · 개발 일정이 한가할 때) ② 측정 기준(편집 증분 절감 1 s 이상이면 AI도 진행 · 추천) ③ 새 크레이트 이름(개발 판단으로 둘지).
