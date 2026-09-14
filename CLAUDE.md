# CLAUDE.md — Nexa SQL 프로젝트 컨텍스트 (이식용 메모리)

> **먼저 읽기:** [docs/00-foundation-report.md](docs/00-foundation-report.md)(보고서) → [docs/STATUS.md](docs/STATUS.md) → [docs/10-decision-record.md](docs/10-decision-record.md).

## 1. 이 프로젝트는

**Nexa SQL** = **크로스플랫폼 경량 SQL 클라이언트(IDE) + CLI `nsql`**(Windows · macOS · Linux). DBeaver의 커버리지 × TablePlus/Golden의 가벼움 × SQL*Plus/PL/SQL Developer의 DBA 워크플로.
**올 러스트 · 단일 바이너리 · `../nexa-ui`(자체 CPU 래스터) 위에 그린다** — Qt·WebView·Electron 없음.

- 조직: **SosomLab** · 개발자: Sangyong Bae · kiros33@gmail.com · 저장소 <https://github.com/SosomLab/nexa-sql> · 라이선스 **PolyForm NC 1.0.0**
- 현 단계: **M1·M2 병행 진행(2026-09-14 1차 · win)** — 드라이버 3종(Oracle 19c 사내 실서버 ✅ · MSSQL integration ✅) · `nsql run/shell/export/conn` · 최소 GUI 창 **실기 통과**(프로필 접속 → F5 → 그리드) · 연결 프로필(DR-22) · Instant Client 경로(`NSQL_ORACLE_CLIENT_DIR`) · DR-1~24 확정. **09-14: i18n(기본 영어 · `nsql-i18n`)·설정(`nsql-settings` · `settings.conf` · `nsql config`)·테마(System 기본 · ⇧T/⇧L 단축키) ✅ · 정품 인증 설계 [23](docs/23-license-activation.md) · VS Code 설정 분석 [24](docs/24-settings-and-vscode-analysis.md).** **09-14 6차: 접속 패널(GUI)·`conn add/test` 필드(CLI)·성능 계측 골격(`Stage/Timeline` · `--timing`) ✅ · 설계 [26 성능](docs/26-performance-architecture.md) · [27 CLI 규약](docs/27-cli-conventions.md).** **09-14 12차: 편집기 탭 · 그리드 이동/결합 정렬 · 구문 강조(`Packages/*.nexa-syntax` 플러그인) · 명령 팔레트(Ctrl+⇧P) · 서식 복사 · 상태줄 세그먼트 · `window.focus` ✅ · 설계 [29](docs/29-editor-syntax-palette-statusbar.md)(인텔리전스·다중 커서) · 프로필 3종(biscm·M4PLAN·matrixdb2 = vault).** **09-14 13차: Golden식 접속 창(`conn_win.rs` · T-31) ✅ · 앱 아이콘 Union(`packaging/branding/icon.svg` SSOT · 런타임 3창) ✅.** **다음 = T-31b(접속 창 보강) → nexa-grid(dir2 rows 이식 · [nexa-ui 21](../nexa-ui/docs/21-grid-family.md)) → T-48 페치 모델 → T-57 인텔리전스 → T-27 RPC** · T-32 `nsql-license`(D-23~25 답 뒤) · ⏳ 사용자: **D-23·24·40 라이선스 결정**(D-32~39 = DR-26 확정)([10 §3](docs/10-decision-record.md)) · 한글 IME 실기 · MSSQL/PG 접속 정보 · D-15~19 답.
- ★ 형제 저장소 clone은 `git@kiros33.github.com:SosomLab/<repo>.git`(SSH 별칭 · 사용자 지정 09-13).

### 참조 원천 (재발명 금지)

| 원천 | 경로 | 무엇 |
|---|---|---|
| **`nexa-ui`** | `../nexa-ui` | ★ 공용 UI — `nexa-gfx`·`nexa-ctl`(컨트롤 17종·토큰)·`nexa-conf`. path 의존 · **파일 관리·파일 대화상자 계층 설계 = nexa-ui docs/20**(3-OS 동일 · 네이티브 대화상자 0 · nexa-sql 배선 F-6) |
| `nexa-clip` | `../nexa-clip` | 앱 골격(plat·창·트레이·릴리스 파이프라인 3-OS·`check-3os.sh`·`21-manual-test` 실기표) |
| `nexa-dir2` | `../nexa-dir2` | `nexa-gui/widgets`(dock·tabbar·menubar·columns·rows) · **`wasmi` 플러그인** |
| `nexa-dir` | `../nexa-dir` | `docs/13`(라이선스 정책) · `docs/17`(Ed25519 활성화 설계) — ⚠️ 이 기기에 없음(09-14) |
| **`nexa-license`**(✅ 09-14 생성 · 공개 · T-45 🚧) | `../nexa-license` | ★ 라이선스 **공유 라이브러리**(형식·서명·검증·기기 ID·프로토콜) — 앱 `nsql-license`와 비공개 서버 `../nexa-license-server`(`nexa-licensed`·발급기)가 함께 씀 · [25 §9](docs/25-license-tiers-and-server.md) |

## 2. 확정 결정 (요약 — 전문 [docs/10](docs/10-decision-record.md))

| # | 결정 |
|---|---|
| DR-1 | **Rust 올 네이티브 · nexa-ui 확장**(프레임워크 도입 없음) |
| DR-2 | PolyForm NC 1.0.0 — 영리만 유료 |
| DR-3 | 외부 crate 0 지향의 명시 예외 = **드라이버·셰이핑**(어댑터 안에 격리 · 원장 기록) |
| DR-4 | 공용 컨트롤은 `nexa-ui`로 먼저 분리(완료) |
| DR-5 | **편집기 = Sublime Text 차용 · 확장 = Sublime 패키지 구조**(파일 형식 그대로 · WASM 플러그인) |
| DR-6 | **CLI `nsql` 포함**(접속·스크립트·export/import·bulk) — GUI와 같은 코어 |
| DR-7 | **4계층**: UI / Core / Network / 어댑터 — 허브 포트로만 연계(④→③←①, ①→②) |
| DR-8 | **세션 변수는 클라이언트에 산다** — 리터럴 대입 로컬 · 방언 재작성(MSSQL `sp_executesql` OUTPUT · `SELECT @X=`) |
| DR-10~18 | 단일 앱 · 지원 범위 3급 · 드라이버 선택 · 자체 결과셋 · rustybuzz · WASM 플러그인 · 예산 · **CLI+최소 GUI 병행** |
| DR-19 | **한글·고정폭 1급**(`nexa-font` · 편집기/그리드 고정폭 + 한글 폴백) |
| DR-20 | 코드 서명은 **별도 요청 시** |
| DR-22 | **연결 프로필 = 사용자 폴더 `nsql-vault`**(비밀번호만 봉투 · Windows DPAPI 기기 키 · CLI·GUI·다중 인스턴스 공유) — [21](docs/21-connection-profiles.md) |
| DR-23·24 | **접속 대화상자 = DBeaver식 + Golden 로그인 리스트** · **드라이버 확장 = GitHub Releases 최신 다운로드 · stdio JSON-RPC 프로세스 · SxS 다중 버전 · 관리자(목록·삭제)** — [22](docs/22-driver-extensions.md) · T-27~31 |
| DR-25 | **라이선스: 공유 라이브러리 `nexa-license`(형제 저장소) + 인증 서버는 비공개 저장소 `nexa-license-server`** · 티어 4단(Device·User 5대·Team·Org 서버) — [23](docs/23-license-activation.md) · [25](docs/25-license-tiers-and-server.md) · D-23~39 대기 |

### 설계 문서 지도(요구별)
연결 프로필 [21](docs/21-connection-profiles.md) · 드라이버 확장·접속 대화상자 [22](docs/22-driver-extensions.md) · 정품 인증·기능 게이트 [23](docs/23-license-activation.md) · 라이선스 종류·인증 서버·저장소 분리 [25](docs/25-license-tiers-and-server.md) · 설정 체계·i18n·테마 [24](docs/24-settings-and-vscode-analysis.md) · 편집기 점증 순서 [17](docs/17-editor-incremental-plan.md) · 세션/hot exit/프로젝트 [18](docs/18-session-and-projects.md) · 외부 변경(git식 병합) [15](docs/15-external-file-changes.md) · 비교/git/오브젝트 시점 캐시 [19](docs/19-compare-git-and-object-history.md) · 폰트·기능 모듈 [14](docs/14-fonts-and-feature-modules.md) · 사용자 Sublime 프로필 [12](docs/12-user-sublime-profile.md).

## 3. 작업 규약

- **문서·커밋/푸시 규약 SSOT = [docs/16](docs/16-doc-git-conventions.md)**. 한 작업 = 한 트랜잭션 갱신(journal → DEVLOG → STATUS → MILESTONES/TODO).
- **push는 사용자 명시 요청 시에만.** `git add <파일>`만(`-A`·`.` 금지). push 전 `scripts/check-3os.sh`.
- 크레이트 경계: `nsql-core`는 의존 0 · 드라이버 크레이트는 `nsql-driver-*` 안에만 · UI는 `Box<dyn Session>`만 안다.
- **실행 경로의 시간은 `nsql_core::Timeline`에 단계로 덧붙인다**(docs/26 — 각 층은 자기 단계만 · 문자열 포맷은 표시 시점에만). 결과 그리드는 속도·메모리 최우선.
- **사용자 문자열은 전부 `nsql-i18n::Msg`**(기본 영어 · 한국어 열) — 리터럴 금지. **설정 키는 `nsql-settings::REGISTRY`에만** 추가(라벨·설명 = `Msg`).
- 스크립트 엔진을 고치면 `examples/golden-session-vars.sql`로 `nsql plan` 양 방언을 다시 본다.
- `.claude/settings.json`은 덮어쓰기 금지, 병합만.
- ★ **포커스 규칙(사용자 09-14 · 반복 버그)**: 한 창에 포커스 링은 **정확히 하나 이하**. nexa-ctl 버튼·콤보·체크박스는 MouseDown에서 스스로 `focused=true`가 되고 **스스로 끄지 않는다** → 컨트롤을 여러 개 담는 쪽(패널·창)이 **MouseDown마다 하나만 남기고 끄고**(`connect.rs own_focus`) · 창/패널이 포커스를 잃으면 전부 끈다. **컨트롤을 추가·배선할 때마다 점검**: ① 클릭 뒤 링 1개 ② 다른 컨트롤 클릭 뒤 앞 링 꺼짐 ③ 텍스트박스 클릭 뒤 버튼 링 0개 ④ Esc/창 전환 뒤 0개 — 실기 캡처로 확인하고 journal에 남긴다.
- ★ **hover 효과 규칙(사용자 09-14)**: 마우스가 거쳐 가는 모든 대상이 이벤트를 만들어 속도를 떨어뜨리면 안 되고, **최종 위치가 아닌 대상의 효과는 즉시 취소**된다. 구현은 nexa-ctl `tokens::IntentFade` 하나로(행·콤보 항목·버튼 공통): 사건은 목표 **덮어쓰기만**(큐 0 · 비용 0) → `tick`이 70ms 머문 **마지막 목표만** 페이드에 넘김 → 목표가 바뀌면 켜지던 것은 바로 꺼짐. 속도 = 컨트롤 속성 `FadeSpeed { Fast, Slow }`(버튼·콤보 = Fast · 행 = Slow · `set_fade_speed`) → 실제 ms는 설정 `ui.fade_fast`(500)/`ui.fade_slow`(1000)에 연계 · 키보드 이동은 `jump`(즉시). 새 hover 효과를 넣을 때 자체 타이머·큐를 만들지 말고 이 부품을 쓴다.

## 4. 새 세션 오리엔테이션

1. 이 파일 + [docs/00](docs/00-foundation-report.md) → 2. [STATUS](docs/STATUS.md) → 3. [TODO](docs/TODO.md) T-1(DP 확정)부터.
