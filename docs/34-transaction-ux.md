# 34 · 트랜잭션 UX — 자동 커밋 기본 · 수동 커밋 시 편집기 탭별 상태 표시

> **요청**(사용자 09-15): *"트랜잭션 기본은 자동 Commit, 설정으로 수동 Commit이면 각 Editor 창별로 트랜잭션 상태를 표시 — 어떤 방식이 쉽게 식별되고 관리가 좋은가."*
> **선행**: [18 세션·프로젝트](18-session-and-projects.md) · [26 §3](26-performance-architecture.md)(실행 계층) · [29 §4 상태줄](29-editor-syntax-palette-statusbar.md) · T-54 `session.mode`(탭별 세션) · 09-15 3차 트랜잭션 1차(전역 `session.autocommit` · Commit/Rollback · 상태줄 Auto/Manual ●).
> **상태**: 📐 설계 · 결정 **D-51** · 작업 **T-77**(T-54 위에).

---

## 0. 원칙 다섯 줄

1. **자동 커밋이 기본**(DBeaver·DataGrip과 같음). 수동은 사용자가 켠 것이므로 "지금 무엇이 커밋되지 않았는가"를 **보지 않아도 알게** 한다.
2. **트랜잭션의 단위 = 세션 = 편집기 탭**(T-54 `session.mode = per-editor`). 공유 세션(`shared`)이면 탭별 표시는 의미가 없으니 상태줄 하나만.
3. 표시는 **세 층이 같은 사실을 말한다**: 탭 배지(어느 탭인가) · 상태줄 세그먼트(지금 탭의 상세) · 툴바 Commit/Rollback(행동). 층마다 다른 숫자가 보이면 실패.
4. **잃을 수 있는 순간에만 묻는다**: 미커밋 탭 닫기 · 접속 해제 · 다른 서버로 접속 · 자동 커밋으로 전환 · 앱 종료. 그 밖엔 방해하지 않는다.
5. 서버가 몰래 커밋하는 경우(Oracle DDL 암묵 커밋 · `COMMIT`/`ROLLBACK` 문장 직접 실행)도 **상태를 따라간다** — 표시가 거짓말하지 않는다.

---

## 1. 다른 도구는 어떻게 하나

| 도구 | 모드 전환 | 미커밋 표시 | 행동 | 우리가 가져올 것 |
|---|---|---|---|---|
| DBeaver | 툴바 "Auto/Manual" 토글(접속 단위) · 스마트 커밋(DML이면 수동) | 툴바 커밋 버튼에 **대기 문장 수** · 접속 단위 | Commit · Rollback · 트랜잭션 로그 창 | 대기 수 배지 · 트랜잭션 로그(어떤 문장이 대기 중인가) |
| DataGrip | 툴바 "Tx: Auto ▾ / Manual"(콘솔 단위) | 콘솔 탭에 **●**(미커밋) · 커밋 버튼 활성 | Commit · Rollback · 닫을 때 확인 | 탭 ● · 콘솔(=탭) 단위 |
| TablePlus | 항상 수동(데이터 편집) | 상단 **"n changes · Commit / Discard" 바** | Ctrl+S 커밋 | 색 있는 바(잃기 쉬운 순간 강조) |
| PL/SQL Developer · Toad | 툴바 Commit/Rollback + 상태줄 "Transaction pending" | 툴바 버튼 활성/비활성 | 닫을 때 확인 | 상태줄 문구 |
| SQL*Plus | `SET AUTOCOMMIT` | 없음(EXIT 시 커밋) | — | 스크립트 `SET AUTOCOMMIT` 우선 |

공통점: **모드는 접속/콘솔 단위 · 미커밋은 "점 하나 + 숫자" · 행동은 항상 같은 자리(툴바)** · 닫을 때만 묻는다.

---

## 2. 권장 설계

### 2-1. 세 층 표시(같은 사실)

```
탭 바:   Script_1   Script_2 ●3   Script_3          ← 탭 배지 = 미커밋 DML 문장 수(수동 모드 · >0일 때만)
툴바:    [▷][▶] … [✓ Commit 3][↶ Rollback]           ← 활성 탭의 대기 수 · 0이면 비활성(회색)
상태줄:  … | Manual ● 3 pending · 12:03 |            ← 활성 탭의 상세(클릭 = 팝업)
```

| 층 | 무엇 | 언제 | 색·모양 |
|---|---|---|---|
| **탭 배지** | `●n`(n = 커밋되지 않은 DML/DDL 문장 수) | 수동 모드 && n>0. 자동 모드·0이면 없음 | 경고색(`warn` · 주황) · 오래되면(`tx.stale_min` 기본 10분) `danger`(빨강) — "잊은 트랜잭션" |
| **상태줄 세그먼트** | `Auto-commit` / `Manual` / `Manual ● n pending · 시작 hh:mm` | 항상(활성 탭 기준) | 클릭 = 팝업: 이 탭 Auto/Manual 전환 · Commit · Rollback · 대기 문장 목록(트랜잭션 로그) |
| **툴바** | Commit(대기 수 배지) · Rollback | 항상 · n=0이면 비활성 | 09-15 접속 해제 버튼과 같은 `enabled` 규칙 |
| **탭 툴팁 카드** | 접속 · 모드 · 대기 n · 첫 DML 시각 | hover | 기존 탭 툴팁 카드에 두 줄 추가 |

### 2-2. 모드 = 전역 기본 + 탭별 재정의(들여쓰기 계층 [31](31-indentation-settings.md)과 같은 꼴)

| 계층 | 원천 | 바뀌는 때 |
|---|---|---|
| 전역 | `session.autocommit`(기본 on) | 설정 창 · JSON |
| 접속 프로필 | `autocommit=` 필드(선택 · 운영 서버는 수동 강제) | 접속 창 상세 |
| **탭** | 상태줄 팝업 토글 · 스크립트 `SET AUTOCOMMIT ON/OFF`(그 탭 세션에만) | 세션 동안(영속 X · hot exit 세션 파일엔 저장) |

전환 규칙: 수동→자동으로 바꾸는데 대기 n>0이면 **묻는다**(Commit and switch / Rollback and switch / Cancel).

### 2-3. 상태 기계(탭 = 세션 하나)

```
Auto ──(수동 켬)──▶ Manual·clean(n=0)
Manual·clean ──(DML 성공)──▶ Manual·dirty(n+1 · first_at 기록)
Manual·dirty ──(Commit / COMMIT 문장 / Oracle DDL 암묵 커밋 / 자동 전환+Commit)──▶ Manual·clean
Manual·dirty ──(Rollback / ROLLBACK 문장 / 접속 해제·재접속 실패)──▶ Manual·clean (로그에 "rolled back n")
Manual·dirty ──(실행 오류)──▶ 그대로(서버가 문장만 롤백 · Oracle) — 표시 유지
```

n 세는 규칙: `RunEvent::Done{rows_affected: Some(_)}`인 DML/DDL 항목 · 방언별 암묵 커밋 = Oracle DDL(`CREATE/ALTER/DROP/TRUNCATE/GRANT`) 뒤 n=0 · MSSQL은 DDL도 트랜잭션 안 · PG도 안 · SQLite 자동커밋 문장 단위.

### 2-4. 묻는 순간(모달 · 09-15 접속 창과 같은 모달 부품)

| 순간 | 선택지 |
|---|---|
| 미커밋 탭 닫기(Ctrl+W · ×) | Commit · Rollback · Cancel |
| 접속 해제 / 다른 서버 접속(탭 세션) | 같음 |
| 자동 커밋으로 전환 | Commit and switch · Rollback and switch · Cancel |
| 앱 종료 | 탭별 목록 + Commit all · Rollback all · Cancel |
| 커밋 없이 `tx.stale_min` 경과 | 묻지 않고 배지 빨강 + 상태줄 "pending 25 min"(로그 창 1줄) |

### 2-5. 설정(레지스트리 · Session 카테고리)

| 키 | 기본 | 뜻 |
|---|---|---|
| `session.autocommit` | on | 전역 기본(1차 ✅) |
| `session.mode` | shared | `per-editor`면 탭별 세션·트랜잭션(T-54) |
| `tx.smart_commit` | off | DBeaver식: 자동 모드라도 **DML이 실행되면 그 탭을 수동으로** 전환(잊은 트랜잭션 방지 반대편 — 명시 커밋 습관) |
| `tx.stale_min` | 10 | 미커밋이 이 시간 넘으면 배지 빨강 |
| `tx.close_action` | ask | 미커밋 탭 닫기: ask / commit / rollback |
| `tx.badge` | count | 탭 배지 표시: count / dot / off |

---

## 3. 왜 이 조합인가(대안 비교)

| 방식 | 식별 | 관리 | 판정 |
|---|---|---|---|
| 상태줄만(1차 현 상태) | 활성 탭만 보인다 · 다른 탭의 미커밋을 잊는다 | 행동 자리 없음 | 부족 |
| 탭 배지만 | 어느 탭인지 즉시 · 상세(언제부터·몇 개) 없음 | 행동 자리 없음 | 부족 |
| 편집기 안 상단 바(TablePlus) | 눈에 띄지만 편집 영역을 먹고 탭마다 바 | 행동은 좋음 | 데이터 편집기(T-22)에 적합 · SQL 탭엔 과함 |
| **탭 배지 + 상태줄 세그먼트(팝업) + 툴바 배지** | 어느 탭·몇 개·언제부터가 한눈에 · 자동 모드에선 아무것도 안 보임 | 행동은 툴바·팝업 두 곳(같은 명령) · 잃는 순간만 모달 | **권장** |

---

## 4. 결정 · 작업

| # | 결정 |
|---|---|
| **D-51** | 수동 커밋의 단위 = 탭 세션(T-54 `per-editor`) · 모드 계층 = 전역 → 프로필 → 탭 · 표시 3층(탭 배지 `●n` · 상태줄 세그먼트+팝업 · 툴바 Commit 배지) · 잃는 순간만 모달 · 오래된 미커밋은 빨강 — 권장안 그대로 확정 요청 |
| D-52 | `tx.smart_commit` 기본 off(자동 커밋 기본 유지) vs on(DBeaver 기본) — 권장 off |

| ID | 항목 | 의존 |
|---|---|---|
| **T-77** | 트랜잭션 UX(§2) — 탭별 세션 위 `TxState{mode, pending, first_at}` · 탭 배지 · 상태줄 세그먼트+팝업 · 툴바 배지/활성 · 닫기/해제/전환/종료 모달 · 방언별 암묵 커밋 규칙 · 트랜잭션 로그(대기 문장 목록 · 로그 창 필터) · 설정 5키 · CLI 동등(`nsql shell` 프롬프트에 `*n` 표시) | T-54 T-55 T-61 |
