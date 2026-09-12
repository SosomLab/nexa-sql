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
| ☐ | `nsql-net` TCP/TLS · 취소 토큰 |
| ☐ | `nsql-driver-oracle`(kubo `oracle`) — REF CURSOR OUT · DBMS_OUTPUT |
| ☐ | `nsql-driver-mssql`(tiberius-ng) — `sp_executesql` OUTPUT |
| ☐ | `nsql run/shell` · `export csv/json` · 세션 변수 실기 |

## M2 — GUI 관통
| 상태 | 항목 |
|:--:|---|
| 📐 | `nexa-edit` 1단계(Buffer·Layout·View·Selections·Keymap·IME) — [09 §3](09-editor-and-packages.md) |
| ☐ | `nexa-grid` 가상화 · dock/tab(nexa-ui U-2) · 연결 프로필 |

## M3~M6 — [02](02-roadmap.md)
