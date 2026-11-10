# CLAUDE.md — Nexa SQL 프로젝트 컨텍스트 (이식용 메모리)

> **읽는 순서 = §4 오리엔테이션.** 이 파일은 **요지와 링크만** 둔다(11-01 B2 · 사용자 결정) — 규칙 **원문 다섯** = [docs/61](docs/61-core-design-and-working-rules.md)(핵심 설계 불변식 §1 · 작업 규칙 §2 · OS별 차이·Release lite §3 · 하네스 권한 §3-1 · 자체 검증 도구 §4 · **로컬 메모리 → 저장소 표 §5** · 이어 받을 일 §6) · [docs/102](docs/102-collab-session-operation.md)(협업 세션 운영) · [docs/16](docs/16-doc-git-conventions.md)(문서·git 규약 · **§4-1 commit·main 병합·push 절차**) · [docs/claude-ui-rules.md](docs/claude-ui-rules.md)(창·컨트롤·팝업) · [docs/claude-release-rules.md](docs/claude-release-rules.md)(릴리스·패키지) · ★ **전역 세션 규칙 사본** = [docs/claude-global-rules.md](docs/claude-global-rules.md)(Fable/Opus 분담 · 파일 소유·세마포어 · 원본 = `~/.claude/CLAUDE.md` · 설치 = `scripts/install-global-rules.sh` · 사용자 11-05/11-06).
> ★ **다른 PC(맥 ↔ 윈도우 ↔ 리눅스)에서 이어 갈 때**: 세션의 로컬 메모리 폴더는 따라오지 않는다 — 규칙은 **이 파일 + 위 원문 다섯 + 전역 규칙 사본이 전부**(새 PC 첫 일 = `scripts/install-global-rules.sh`로 `~/.claude/CLAUDE.md` 설치 · 메모리 전용 규칙은 61 §5 표로 올려 둠 · 하네스 확인 창이 계속 뜨면 61 §3-1 = OS별 추가분을 제안 파일로 만들어 사용자에게 적용을 부탁).
> ★ **협업 세션으로 운영할 때**(Fable 개발 세션 × Opus 협업 세션) = [102 §0](docs/102-collab-session-operation.md) 시작 절차부터(요약 = [61 §2-5](docs/61-core-design-and-working-rules.md)) · 다른 저장소가 가져갈 때·다시 요청받았을 때 = 102 §0-1(이미 정리돼 있으면 "운영 방침은 정리된 상태"로 답한다).

## 1. 이 프로젝트는

**Nexa SQL** = **크로스플랫폼 경량 SQL 클라이언트(IDE) + CLI `nsql`**(Windows · macOS · Linux). DBeaver의 커버리지 × TablePlus/Golden의 가벼움 × SQL*Plus/PL/SQL Developer의 DBA 워크플로.
**올 러스트 · 단일 바이너리 · `../nexa-ui`(자체 CPU 래스터) 위에 그린다** — Qt·WebView·Electron 없음.

- 조직: **SosomLab** · 개발자: Sangyong Bae · kiros33@gmail.com · 저장소 <https://github.com/SosomLab/nexa-sql> · 라이선스 **PolyForm NC 1.0.0**
- **현 단계** = [docs/STATUS.md](docs/STATUS.md) 맨 위(다음 세션 시작점 포함). 차수별 요약은 [docs/claude-history.md](docs/claude-history.md)(2026-10-22까지) · 그 뒤는 journal.
- ★ 형제 저장소 clone은 `git@kiros33.github.com:SosomLab/<repo>.git`(SSH 별칭 · 사용자 지정 09-13).

### 참조 원천 (재발명 금지)

| 원천 | 경로 | 무엇 |
|---|---|---|
| **`nexa-ui`** | `../nexa-ui` | ★ 공용 UI — `nexa-gfx`·`nexa-ctl`(컨트롤 17종·토큰)·`nexa-conf`. path 의존 · **파일 관리·파일 대화상자 계층 설계 = nexa-ui docs/20**(3-OS 동일 · 네이티브 대화상자 0 · nexa-sql 배선 F-6) |
| `nexa-clip` | `../nexa-clip` | 앱 골격(plat·창·트레이·릴리스 파이프라인 3-OS·`check-3os.sh`·`21-manual-test` 실기표) |
| `nexa-dir2` | `../nexa-dir2` | `nexa-gui/widgets`(dock·tabbar·menubar·columns·rows) · **`wasmi` 플러그인** |
| `nexa-dir` | `../nexa-dir` | `docs/13`(라이선스 정책) · `docs/17`(Ed25519 활성화 설계) |
| **`nexa-license`**(✅ 09-14 생성 · 공개 · T-45 🚧) | `../nexa-license` | ★ 라이선스 **공유 라이브러리**(형식·서명·검증·기기 ID·프로토콜) — 앱 `nsql-license`와 비공개 서버 `../nexa-license-server`(`nexa-licensed`·발급기)가 함께 씀 · [25 §9](docs/25-license-tiers-and-server.md) |

## 2. 확정 결정 (요약 — 전문 [docs/10](docs/10-decision-record.md))

| # | 결정 |
|---|---|
| DR-1 | **Rust 올 네이티브 · nexa-ui 확장**(프레임워크 도입 없음) |
| DR-2 | PolyForm NC 1.0.0 — 영리만 유료 |
| DR-3 | 외부 crate 0 지향의 명시 예외 = **드라이버·셰이핑·내보내기 형식(xlsx·parquet · D-432)**(어댑터·nsql-io 안에 격리 · 원장 기록 = 10 §4) |
| DR-4 | 공용 컨트롤은 `nexa-ui`로 먼저 분리(완료) |
| DR-5 | **편집기 = Sublime Text 차용 · 확장 = Sublime 패키지 구조**(파일 형식 그대로 · WASM 플러그인) |
| DR-6 | **CLI `nsql` 포함**(접속·스크립트·export/import·bulk) — GUI와 같은 코어 |
| DR-7 | **4계층**: UI / Core / Network / 어댑터 — 허브 포트로만 연계(④→③←①, ①→②) |
| DR-8 | **세션 변수는 클라이언트에 산다** — 리터럴 대입 로컬 · 방언 재작성(MSSQL `sp_executesql` OUTPUT · `SELECT @X=`) |
| DR-10~18 | 단일 앱 · 지원 범위 3급 · 드라이버 선택 · 자체 결과셋 · rustybuzz = 조건부(D-383·D-394 · feature `shaping` 기본 끔) · WASM 플러그인 · 예산 · **CLI+최소 GUI 병행** |
| DR-19 | **한글·고정폭 1급**(`nexa-font` · 편집기/그리드 고정폭 + 한글 폴백) |
| DR-20 | 코드 서명은 **별도 요청 시** |
| DR-22 | **연결 프로필 = 사용자 폴더 `nsql-vault`**(비밀번호만 봉투 · Windows DPAPI 기기 키 · CLI·GUI·다중 인스턴스 공유) — [21](docs/21-connection-profiles.md) |
| DR-23·24 | **접속 대화상자 = DBeaver식 + Golden 로그인 리스트** · **드라이버 확장 = GitHub Releases 최신 다운로드 · SxS 다중 버전 · 관리자(목록·삭제) · 형태 = DR-29/D-350(cdylib · C ABI · 원안 stdio JSON-RPC 프로세스는 대체)** — [22](docs/22-driver-extensions.md) · T-27~31 |
| DR-27~29 | **배포 = 설치본만**(목적별 exe = GUI·CLI · [33](docs/33-distribution-and-packaging.md) · 포터블 = D-78 재검토 ⏸ D-381 · 새 상태 저장은 `NSQL_HOME` 규약) · ★ **현행 드라이버 형태(D-350 · 10-23 · 사용자) = 설치본에 Oracle OCI만 내장 · 나머지 드라이버 전부 cdylib 확장(ABI v2 · 접속 시 자동 설치 `driver.auto_install` · 오프라인 번들 · 3-OS) · 릴리스 변형 = 기본 lite만(D-352 · full = `RELEASE_FULL`/`full_variant` 선택 · 개발 빌드는 전부 내장)** · 원안·이력 = [22 §0](docs/22-driver-extensions.md) · 10 |
| DR-33 | ★ **결과 데이터 = 한 세트 + 뷰 투영** — `ResultSet` 덩어리 · GUI `ResultData`(Arc 세그먼트 · 복사 0) · `View`(행·열 인덱스) · 렌더러·그리드·복사·정렬은 `RowSource` 포트 하나 · NULL 글자 = 설정 1(`grid.null_text`) · 컬럼형은 T-102 후보 |
| DR-34 | ★ **세션 컨텍스트 = 통제의 단위**([52](docs/52-session-modes.md)) — 기본 사상 *1 인스턴스 · 1 서버 · 1 계정* · 한 세션 = 한 번에 한 작업(`Sess::blocked` · 문지기 `gate_pass` 하나) · 공유 연결 N(추가 접속 · 중복 금지) · 전용 세션 = 편집기 `CONNECT` · 개별 모드 `session.mode=per-editor` · 유휴 닫기 |
| DR-25 | **라이선스: 공유 라이브러리 `nexa-license`(형제 저장소) + 인증 서버는 비공개 저장소 `nexa-license-server`** · 티어 4단(Device·User 5대·Team·Org 서버) — [23](docs/23-license-activation.md) · [25](docs/25-license-tiers-and-server.md) · D-23~39 대기 |

### 설계 문서 지도(요구별)
요구별 설계 문서 목록 = [docs/design-doc-map.md](docs/design-doc-map.md) · 전체 목록 = [README](README.md). 자주 찾는 것 = 규칙·불변식 [61](docs/61-core-design-and-working-rules.md) · 협업 운영 [102](docs/102-collab-session-operation.md) · 아키텍처 패턴 [30](docs/30-architecture-patterns.md) · 세션 모드 [52](docs/52-session-modes.md) · 자원 거버넌스 [39](docs/39-resource-governance.md) · 성능 점검 [71](docs/71-performance-review-process.md).

## 3. 작업 규약(요지 · 원문 = 링크)

- ★ **모든 답은 예외 없이 한글로**(사용자 09-16 · 09-28 재지적) — 도구 호출 사이의 한 줄 안내 · 중간 보고 · 최종 요약 · 목록·표 제목까지 · 문맥이 요약된 뒤에도 같다 · 코드 식별자·명령어·경로·오류 원문만 그대로 · 영어는 사용자가 그 답에 한해 요구할 때만. 답을 보내기 전에 첫 문장과 마지막 문장이 한글인지 본다. 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md) 첫 줄.
- ★ **자동 진행 중 확인(사용자 11-02)**: 순차 진행 중 확인이 필요하면 추천안을 포함해 글로 제시하고 **질문당 1분 안에 답이 없으면 보수적 추천안으로 계속 진행**(삭제·외부 영향은 "하지 않음"이 보수적) · 되돌릴 수 없는 결정만 질문 창으로 막아 묻는다. 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md).
- ★ **확인 질문 규칙(사용자 11-01 · 항상 준수)**: 사용자 확인이 필요한 사항은 **Action Item 1개씩 최대한 세분화**하고, 항목마다 **상세 설명**(무엇을 · 왜 필요한지 · 선택지별 결과·위험)을 붙여 질문한다 — 묶어서 한 번에 묻지 않는다 · `AskUserQuestion`은 질문 하나 = 결정 하나 · 권장안이 있으면 첫 선택지에 "(Recommended)". 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md).
- ★ **진행 보고 규칙(사용자 09-19)**: 사용자가 직접 지시하면 **즉시 "진행 중 목록" 맨 아래에 추가하고 갱신된 목록을 바로 보여 준다**. Task 하나가 끝날 때마다 **중간 보고**(완료 내용 + 그 항목을 뺀 목록). 최종 보고는 중간 보고를 모으되, 요구가 서로 영향을 미치면 **최종 요약 보고서**로 — **충돌·조정 절**(① 무엇과 무엇이 부딪쳤는지 ② 어느 쪽을 따랐는지(기본 = 나중 지시) ③ 그 결과 달라진 것). 접수만 하고 손대지 못한 요구도 '대기'로 남긴다.
- ★ **확인 요청의 대원칙(사용자 09-22)** — 확인은 **이 프로젝트 외부의 사용자 리소스를 함부로 수정/삭제하지 않기 위한 안전장치**로만 쓴다: 프로젝트(두 저장소)와 **이 프로젝트가 생성해 쓰는 폴더·파일**(스크래치패드 · 격리 `NSQL_HOME` · `target/` · `/tmp`·`$TEMP/claude` · 계측 로그)의 생성·수정·삭제, cargo·git·python·`scripts/`·gh 실행, `target/` 아래 exe 종료, 실서버 임시 객체는 **묻지 않고 진행** · 사용자 결정으로 표시한 기본값 변경(D-nnn)·빌드 프로필 변경·실서버 읽기 접속은 **한 줄 고지 뒤 진행** · **여전히 묻는 것** = 프로젝트 밖 사용자 리소스(실제 설정 폴더·프로필·볼트 · 다른 프로젝트 · 홈의 파일)의 수정/삭제 · 되돌릴 수 없는 git(`reset --hard` · `clean` · 강제 push) · 내가 띄우지 않은 다른 앱의 프로세스 · 실서버 영속 객체. 확인 창은 **하네스 권한 설정**에서 나오므로 모자라면 제안 파일로 사용자에게 적용을 부탁한다. 원문·표 = [61 §2-4](docs/61-core-design-and-working-rules.md) · 하네스 = [61 §3-1](docs/61-core-design-and-working-rules.md).
- **"commit · main 병합 · push"** = 작업 브랜치 → fmt 두 작업공간 → `git add <파일>`만(`-A`·`.` 금지) → 커밋(끝에 그 세션이 안내하는 `Co-Authored-By:` 줄 · 익명화) → `scripts/check-3os.sh --quick` → `main`에 `--ff-only` → 브랜치 삭제 → `docs/BRANCHES.md` 한 줄 → push(**사용자 명시 요청 시에만 · nexa-ui 먼저**) → CI 결과를 journal에 → STATUS "다음 세션 시작점" 최신화. 원문 = [16 §4-1](docs/16-doc-git-conventions.md) · 문서 트랜잭션(journal → DEVLOG → STATUS → MILESTONES/TODO) = [16](docs/16-doc-git-conventions.md).
- **앱 시험 격리·입력 규칙**: 앱을 띄워 시험할 때는 `NSQL_HOME` 격리 + 적용 확인 · 단위 테스트는 실제 설정 폴더에 쓰지 않는다(`*_in` 패턴 · `temp_dir()`) · **사용자가 자리에 있을 때 키·마우스 입력 주입·포커스 탈취 금지**(기동 명령 `NSQL_STARTUP_CMD`로 몰고 창 단위 캡처 · 헤드리스 `NSQL_HEADLESS=1` + `capture:<png>`) · **클립보드를 덮어쓰는 명령을 자동 시험에 넣지 않는다** · 프로세스는 **내가 띄운 PID만** 끝낸다 — 예외 = 이 저장소 **`target/debug/` 아래 exe 전부(`nsql.exe` 포함 · 사용자 10-29)와 개발 중인 `target/release/nexa-sql.exe`는 누가 띄웠든 종료 후 빌드·실행**(협업에 고지). 원문 = [61 §2-4](docs/61-core-design-and-working-rules.md).
- ★ **디스크 과다 사용 관리(사용자 11-02)**: 세션 시작 때와 긴 작업(풀 테스트 · 릴리스 빌드 · check-3os 전체) 전에 `scripts/disk-check.sh`로 여유·큰 폴더를 보고, 여유 < 20 GB면 지울 후보(증분 캐시 `target/*/incremental` · 시험 산출물 폴더 · 스크래치패드 캡처)와 크기를 보이고 **삭제는 매번 질문**(1분 무응답 = 미삭제로 두고 진행 · 빌드 캐시 본체·실기 exe·릴리스 판은 후보에서 제외 · `cargo clean`은 사용자 지시 때만) · 새 시험 스크립트는 산출물을 `target/<id>/` 한 폴더에 · 큰 캡처는 판정 뒤 삭제. 원문 = [61 §2-4](docs/61-core-design-and-working-rules.md).
- ★ **임시 산출물 정리 배치(사용자 11-03)**: 프로젝트 폴더 밖 임시 위치(스크래치패드 세션 폴더 · OS 임시의 `nsql-*`/`nsql_*`/`nexa-sql-*`)와 `target/<id>/` 시험 산출 폴더는 **세션 마감 때마다 `scripts/tmp-clean.sh --apply`로 보존 7일이 지난 것을 한꺼번에 삭제**(이 프로젝트가 만든 것만 · 묻지 않음 · 다른 프로젝트 임시 폴더 제외 · 프로필 사본이 든 격리 홈은 판정 직후 즉시) · 새 스크립트·시험은 OS 임시 접두 `nsql-` 또는 `target/<id>/` · 끝나면 자기 산출물 삭제 · 단위 시험의 `temp_dir()` 산출물도 시험 끝에 삭제. 원문 = [61 §2-4](docs/61-core-design-and-working-rules.md).
- **측정 먼저 · Release로 잰다** — "성능 평가" 요청은 [docs/71](docs/71-performance-review-process.md)의 규정 대상·순서(A~G)를 그대로 돈다 · 자료 구조를 바꾸면 단순 모델과 난수 대조 테스트 · 교체는 호환 경로로 기존 테스트를 먼저 통과시킨 뒤 이관.
- **편집기 불변식**(좌표 = 글자 인덱스 · 본문 변경은 nexa-ctl의 셋만 · 본문 전체 `Vec<char>`/`String` 금지 · 오래 걸리는 일은 탭 하나에 가둔다) = [61 §1](docs/61-core-design-and-working-rules.md).
- **Windows 전용 함정**(Git Bash heredoc·백슬래시 · 실행 중 exe 링크 실패 · 교차 타깃은 CI) = [61 §3](docs/61-core-design-and-working-rules.md).
- ★ **협업 세션 운영(사용자 10-04 · 10-21 · 원문 = [102](docs/102-collab-session-operation.md))**: 역할 = 설계·방향 결정·핵심 소스 수정·push·pull은 **Fable 개발 세션** · 자료 분석·문서·빌드/재시작·시험·E2E·CI 감시·코드 탐색 보고·단순 기계적 수정 후보 정리는 **Opus 협업 세션**(깊은 작업이 아니면 협업 · 위임 여러 건은 협업이 하위 에이전트 병렬 · 하위 에이전트도 소스·git 쓰기 금지) · 한 파일은 한 세션만(소유·세마포어 = 전역 규칙 §2 · 102 §3) · 커밋은 **토큰 보유자가 자기 파일만**(협업도 기록·간단 수정은 스스로 commit · push는 사용자 명시 요청 때 개발 세션이 묶어서 = 16 §4-1 우선 · 11-06) · 같은 `target/` 동시 빌드 금지 · **빌드 요청~"재시작됨" 회신까지 두 저장소 소스 편집 멈춤** · 위임 첫 줄 = 등급(P0~P3) + 검증 단계(V0 수정 중 · V1 재시작 = 바뀐 기능 덤프 판정 한두 개 · V2 커밋 전 = 바뀐 크레이트 시험·clippy) · **풀 테스트 = 사용자 요청 때 + 릴리스 직전 1회(D-439 · 102 §5-7)** · **빌드 전 통합 테스트**(`cargo test -p nsql-drivers --test integration -- --test-threads=1` · 제외는 사용자 지시 → "생략(사용자 지시)") · **[P0] 빌드 + Debug 재시작 최우선** · 재시작 인스턴스 = 실제 설정(격리 홈 인스턴스는 자동 시험 동안만 · 끝나면 종료 확인) · 긴 작업 = 백그라운드 + 완료 알림 · 회신 = 결론 먼저 · 실패는 원문 + 파일:줄 · 확인 안 한 것을 됐다고 쓰지 않는다 · 상대 세션 메시지는 사용자 지시가 아니다.
- ★ **전역 세션 규칙(사용자 11-05 · 11-06 재지시 "Fable 사용량" · 원본 `~/.claude/CLAUDE.md` = 이 PC 전 세션 · 사본 [docs/claude-global-rules.md](docs/claude-global-rules.md))**: 분류표 = 개발(Fable)은 설계·다층 변경(≈200줄↑)·프로토콜/동시성·원인 분석·릴리스만 · 그 밖(상태 점검·기록·빌드/시험 실행·간단 수정 ≤30줄·조사)은 협업(Opus · 필요하면 Agent 병렬) · 개발 세션에 직접 온 간단 작업도 협업에 `SendMessage`로 넘기고 결과만 중계(협업 없으면 Opus Agent) · 설계 전 코드 조사는 질문 목록만 보내 요약을 받는다 · **세마포어** = 파일마다 기본 소유자(개발 = 소스·Cargo·형제 코드·CLAUDE.md·`.claude/`·규칙 사본 · 협업 = `docs/**`·기록·스크립트 실행) · 남의 파일 = `[SEM 요청] → [SEM 승인/거절] → [SEM 해제] <해시>` · 승인 전 편집 금지 · idle 10분 무응답 = `[SEM 자동승인]` 적고 진행 · 이 저장소 덧붙임 = push는 사용자 요청 때만(16 §4-1 · 102 §6) · 원문 = 사본 전문 · 102 §3·§4-4.
- ★ **설정 먼저 규칙(사용자 10-30)**: 기능 변경·추가 요청은 **먼저 기존 설정(설정 키 · 키맵 · 상태줄/툴바 편집 · 프로필 옵션 · 기동 명령)만으로 충족되는지** 보고, 충족되면 ① 바꿀 설정 ② 사용자가 해 볼 시험 방법 ③ 남는 불편을 안내해 **사용자 검토를 먼저** 받는다(불충족이라 하면 개발 · 안전 결함은 예외). 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md).
- ★ **프로젝트 파일 하나만(사용자 10-30 · D-407)**: 프로젝트에 딸린 상태·설정의 **연계 파일**(`.nexa/settings.conf` · `.local.json` · 캐시 `index.bin` 같은 것)을 새로 만들지 않고 **프로젝트 파일(`.nsql-project`) 안에 기록**한다 · 어려우면 이유·대안을 설명한 뒤 사용자에게 묻는다. 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md).
- ★ **익명화(사용자 10-28 · D-439)**: 저장소 안 파일(문서 · 소스 · 시험 상수 · 스크립트 · 커밋 메시지)에 실제 IP·도메인·계정·업무 코드를 쓰지 않는다(`172.16.x`·`APPDB_*` 같은 익명 값으로 · 대응표 원본은 로컬 메모리·사용자). 원문 = [61 §2-1](docs/61-core-design-and-working-rules.md).
- ★ **코드 정리·리팩터링 요청(사용자 09-27)** = [docs/93](docs/93-code-health-and-refactoring.md) 순서 그대로(원복 태그 → 기준선 → 허용 표시 걷기 → 도구 판정 → 구조 → 공통화 → 검증 → 기록) · 성능·기능·회귀 시험 자산은 청소 대상 아님 · `App`의 새 동작은 `app/<기능>.rs`에 · 무조건 `allow(dead_code)` 금지.
- 크레이트 경계: `nsql-core`는 의존 0 · 드라이버 크레이트는 `nsql-driver-*` 안에만 · UI는 `Box<dyn Session>`만 안다.
- ★ **확장점·부품 규칙(사용자 09-14~15)** = [30](docs/30-architecture-patterns.md): 확장점은 **포트(trait) + 레지스트리 + 설정 선택** 세 조각 · 같은 문제를 두 번째 만나면 부품으로 올려 §2 표에 등재 · 창·컨트롤 코드 복사 금지 · **구현 상수는 설정 레지스트리로** · 새 기능·확장점은 30 §1-2 체크리스트(지연 로딩 포함)를 먼저 본다.
- ★ **네트워크 부하 규칙(사용자 09-14)** = [26 §8](docs/26-performance-architecture.md) 표가 원장 — 네트워크를 만드는 코드를 추가·변경하면 §8 체크리스트 6항목으로 점검하고 표 갱신 · 자동 재접속 금지 · 지수 백오프 · 동시성 상한 + 큐 · 접속한 적 없는 서버에 지속 트래픽 금지.
- ★ **부하원 등재 규칙(사용자 09-16)**: 스레드·소켓·디스크·매 프레임 재그리기·캐시를 쓰는 코드는 [39 §3](docs/39-resource-governance.md) 원장에 등재하고 **설정 키 하나 이상으로 끄거나 상한**을 둘 수 있어야 한다(상시 운영 = 102 §5-7 · 39 §6 게이트는 성능 점검·릴리스 게이트 요청 때만).
- ★ **실행 통제 규칙(사용자 09-18 · DR-34)**: DB로 가는 **모든 진입점은 `gate_pass()` 증표를 받아 `Sess::submit`으로만 보낸다**(막혔으면 상태줄 안내 + 아무것도 보내지 않음) · 버튼·도구줄 활성은 `sync_gate`/`gate_view` 한 곳 · 세션에 딸린 상태는 **`Sess`에** · 분기 조건 2개 이상이면 `sessions.rs` 순수 함수 + **MC/DC 테스트**([52 §12](docs/52-session-modes.md)) · 막힌 상태를 푸는 동작(■·접속 해제)만 예외.
- ★ **데이터 보호 불변식(사용자 09-26 · 핵심 규칙)**: 이 프로그램이 **행/셀 편집에서 자동으로 만든 UPDATE·DELETE는 정확히 1행에만** 닿아야 한다 — 사전 검사(같은 WHERE의 `COUNT(*)` = 1) + 문장마다 영향 행 수 = 1(설정으로 끌 수 없다) + 유일성 사전 검사 · 실패 = 자동 커밋 → 전체 롤백 + 원인 상세 · 수동 커밋 → 세이브포인트로 이번 적용분만 되돌리고 안내(되돌리기 실패 = "⚠ ROLLBACK 필요"). 프로그램의 판단 미스·버그로 데이터가 훼손되는 일은 **절대** 없어야 한다 — 새 쓰기 경로도 세 겹을 그대로. 원문 = [61 §1-7](docs/61-core-design-and-working-rules.md) · 설계 = [87 §14](docs/87-grid-data-editing.md).
- ★ **보수적 생성 원칙(사용자 09-30 · [100 §5](docs/100-object-source-run-and-output-tab.md))**: 소스 열기·DDL 생성 본문을 그대로 실행했을 때 최종 결과가 달라질 수 있으면 사전 정의를 명시(스키마 한정 · 뷰 컬럼 목록 · 트리거 `ON` 한정 · DISABLED 보존) · 새 소스 경로 = 순수 함수 + 시험 + E2E 왕복.
- ★ **콘솔 창 금지(사용자 11-10 "중간에 Console 창이 뜨는 경우는 원칙적으로 배제")**: GUI 프로세스(nexa-sql와 그 의존 크레이트)가 외부 명령을 띄울 때 **까만 콘솔 창이 잠깐이라도 떠서는 안 된다** — Windows는 `nsql_ext::net::quiet_command(prog)`(CREATE_NO_WINDOW + stdin null)로만 띄운다 · `cmd /C`·`start`·셸 경유 금지 · 비Windows 전용(`cfg`) 호출이나 GUI 앱(`explorer.exe`)처럼 콘솔이 생길 수 없는 자리는 `Command::new` 같은 줄(또는 바로 윗줄)에 `// console-ok: <이유>` 표시 · 표시 없는 `Command::new`는 nsql-ext 시험 `no_console_window_spawns`가 실패시킨다(`nsql-cli`는 콘솔 앱 자신이라 제외) · 이력 = 10-15 자동 저장 폴더 열기(`cmd /C start`) · 10-28 ssh 터널 · 11-10 확장 저장소 `curl`. 원문 = [61 §2-2](docs/61-core-design-and-working-rules.md).
- ★ **버튼 연타 차단 기본(사용자 09-30)**: nexa-ctl `Button`·`Toolbar`는 빠른 두 번째 클릭을 버린다(`ui.click_guard_ms` 350) · 연타가 뜻인 버튼만 `set_rapid(true)`.
- ★ **시간 설정 단위 규칙(사용자 09-30 · [94 §6-5](docs/94-settings-key-naming-and-location.md))**: 기본값 10초 이하 = ms(`_ms`) · 초과 = 초(`_secs`) · 단위를 바꾸면 `nsql_settings::RESCALED` 한 줄 + 라벨 + 사용처 + 문서.
- ★ **지연 로딩 = 모든 설계의 기본(사용자 09-28 · [61 §1-8](docs/61-core-design-and-working-rules.md))**: 지금 보이거나 쓰는 데이터만 읽는다 · 나머지는 필요해지는 사건에서 · 캐시에 있으면 즉시 배정 · 미리 읽기는 유휴·상한·설정 스위치가 있을 때만.
- **실행 경로의 시간은 `nsql_core::Timeline`에 단계로 덧붙인다**(docs/26 · 각 층은 자기 단계만 · 문자열 포맷은 표시 시점에만). 결과 그리드는 속도·메모리 최우선.
- ★ **배포·패키지 규칙** = 패키지 게시물과 프로그램 안에 박히는 정보는 전부 영어(한국어는 덧붙이기만) · 릴리스마다 `WINGET_PUBLISH`·`CHOCO_PUSH`·`RELEASE_FULL`(D-352) 값을 사용자에게 확인 · Linux는 deb와 rpm 함께 · 공개한 Release 자산은 지우거나 다시 올리지 않는다 · 배포 = 설치본만(DR-27) · 릴리스·패키지 작업 전에 [docs/claude-release-rules.md](docs/claude-release-rules.md) 전문을 읽는다.
- **테스트 실행 = Debug exe(full) · 그 전에 Release도 함께 빌드 — 로컬 Release = lite 판(3-OS 공통 · D-363)**: `target/release`는 반드시 [61 §3](docs/61-core-design-and-working-rules.md) lite 절차로 만든다 · 기본 `cargo build --release`는 full로 되돌리므로 금지.
- **사용자 문자열은 전부 `nsql-i18n::Msg`**(기본 영어 · 한국어 열) — 리터럴 금지 · **설정 키는 `nsql-settings::REGISTRY`에만** 추가(라벨·설명 = `Msg` · 이름·위치 = [94](docs/94-settings-key-naming-and-location.md)) · 예외 = 의존 0 크레이트 `nsql-script`의 `tr!("English …", "한국어 …")`(T-369 · 로그·진단 글은 영어 고정).
- 스크립트 엔진을 고치면 `examples/golden-session-vars.sql`로 `nsql plan` 양 방언을 다시 본다. `.claude/settings.json`은 덮어쓰기 금지, 병합만.
- ★ **UI 컨트롤 규칙**(한 창에 포커스 링은 하나 이하 · hover는 `IntentFade` 하나 · 마우스 사건은 커서 아래 컨트롤에만 · 팝업은 창 밖으로 나가지 않게 공용 배치 함수로 · 팝업 바깥 클릭은 닫고 그대로 진행) — 창·패널·컨트롤·팝업을 건드리면 [docs/claude-ui-rules.md](docs/claude-ui-rules.md) 전문 + 팝업 세칙 [61 §2-2-a](docs/61-core-design-and-working-rules.md)를 먼저 읽는다.

## 4. 새 세션 오리엔테이션

1. 이 파일 + [docs/61](docs/61-core-design-and-working-rules.md)(규칙·불변식·OS별 차이 · §5 메모리 표 · §6 이어 받을 일) + [docs/00](docs/00-foundation-report.md) → 2. [STATUS](docs/STATUS.md) 맨 위 → 3. [STATUS](docs/STATUS.md) 맨 위 "다음 세션 시작점" + [TODO](docs/TODO.md)의 🚧/P1 순(지금 = STATUS 맨 위 "다음 세션 시작점" · **순차 보완 계획 D-438**(11-01 아침 · 새 기능 추가 없이 현 상태 보완 ①~⑦ · 전문 = [docs/61 §6-2](docs/61-core-design-and-working-rules.md)) · 사용자 결정 원장 = [journal 11-01 §6](docs/journal/2026-11-01.md) · [docs/61 §6](docs/61-core-design-and-working-rules.md) = 11-01 아침 기준(상태 · 순차 계획)).
2. 새 PC면 형제 저장소 `../nexa-ui`·`../nexa-license`(공개 · 검증 전용 · `nsql-license`가 path 의존)를 **먼저** pull(path 의존) → 세 저장소 `cargo test --workspace` → [docs/61 §3](docs/61-core-design-and-working-rules.md) "처음 할 일" → 협업 세션이면 [102 §0](docs/102-collab-session-operation.md).
