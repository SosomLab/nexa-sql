# CLAUDE.md — Nexa SQL 프로젝트 컨텍스트 (이식용 메모리)

> **먼저 읽기:** [docs/00-foundation-report.md](docs/00-foundation-report.md)(보고서) → [docs/STATUS.md](docs/STATUS.md) → [docs/10-decision-record.md](docs/10-decision-record.md).
> ★ **다른 PC(맥 ↔ 윈도우 ↔ 리눅스)에서 이어 갈 때**: 세션의 로컬 메모리 폴더는 따라오지 않는다 — 규칙은 **이 파일과 [docs/61](docs/61-core-design-and-working-rules.md)** 이 전부다(핵심 설계의 불변식 · 작업 규칙 · OS별 차이 · 맥에서 처음 할 일 · **§3-1 하네스 권한 = 확인 창이 계속 뜨면 OS별 추가분을 제안 파일로 만들어 사용자에게 적용을 부탁**).
> ★ **협업 세션으로 운영할 때**(Fable 개발 세션 × Opus 협업 세션) = [docs/102](docs/102-collab-session-operation.md) §0 시작 절차부터(요약 = [61 §2-5](docs/61-core-design-and-working-rules.md)) · 다른 저장소가 가져갈 때·다시 요청받았을 때 = 102 §0-1(이미 정리돼 있으면 "운영 방침은 정리된 상태"로 답한다).

## 1. 이 프로젝트는

**Nexa SQL** = **크로스플랫폼 경량 SQL 클라이언트(IDE) + CLI `nsql`**(Windows · macOS · Linux). DBeaver의 커버리지 × TablePlus/Golden의 가벼움 × SQL*Plus/PL/SQL Developer의 DBA 워크플로.
**올 러스트 · 단일 바이너리 · `../nexa-ui`(자체 CPU 래스터) 위에 그린다** — Qt·WebView·Electron 없음.

- 조직: **SosomLab** · 개발자: Sangyong Bae · kiros33@gmail.com · 저장소 <https://github.com/SosomLab/nexa-sql> · 라이선스 **PolyForm NC 1.0.0**
- **현 단계** = [docs/STATUS.md](docs/STATUS.md) 맨 위(다음 세션 시작점 포함). 2026-10-22까지 이 자리에 쌓던 차수별 요약은 [docs/claude-history.md](docs/claude-history.md)로 옮겼다.
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
| DR-27~29 | **배포 = 설치본만**(목적별 exe · [33](docs/33-distribution-and-packaging.md) · **포터블 모드는 D-78 재검토 · 새 상태 저장은 `NSQL_HOME` 규약**) · **PostgreSQL 내장** · **내장 드라이버 정적 링크 유지 · 확장 드라이버 = in-process cdylib(C ABI · 지연 dlopen · IPC 0 · 프로세스는 격리 선택)**([22 §0](docs/22-driver-extensions.md)) · ★ **10-23 D-350으로 대체(사용자) = 설치본 = Oracle OCI만 · 나머지 드라이버 전부 cdylib 확장(ABI v2 · 접속 시 자동 설치 `driver.auto_install` · 오프라인 번들 · 3-OS)** · ★ **D-352(10-23)** 릴리스 변형 = 기본 **lite만** 생성·채널 게시 · **full**(모든 드라이버 내장)은 선택(`full_variant`/`RELEASE_FULL` · 기본 미생성 · `-full` 꼬리로 Release 자산만) · 개발 빌드(cargo 기본 feature)는 전부 내장 |
| DR-33 | ★ **결과 데이터 = 한 세트 + 뷰 투영** — `ResultSet` 덩어리 · GUI `ResultData`(Arc 세그먼트 · 복사 0) · `View`(행·열 인덱스) · 렌더러·그리드·복사·정렬은 `RowSource` 포트 하나 · NULL 글자 = 설정 1(`grid.null_text`) · 컬럼형은 T-102 후보 |
| DR-34 | ★ **세션 컨텍스트 = 통제의 단위**([52](docs/52-session-modes.md)) — 기본 사상 *1 인스턴스 · 1 서버 · 1 계정* · 한 세션 = 한 번에 한 작업(`Sess::blocked` · 문지기 `gate_open` 하나) · 공유 연결 N(추가 접속 · 중복 금지) · 전용 세션 = 편집기 `CONNECT` · 개별 모드 `session.mode=per-editor` · 유휴 닫기 |
| DR-25 | **라이선스: 공유 라이브러리 `nexa-license`(형제 저장소) + 인증 서버는 비공개 저장소 `nexa-license-server`** · 티어 4단(Device·User 5대·Team·Org 서버) — [23](docs/23-license-activation.md) · [25](docs/25-license-tiers-and-server.md) · D-23~39 대기 |

### 설계 문서 지도(요구별)
요구별 설계 문서 목록 = [docs/design-doc-map.md](docs/design-doc-map.md). 자주 찾는 것 = 규칙·불변식 [61](docs/61-core-design-and-working-rules.md) · 협업 운영 [102](docs/102-collab-session-operation.md) · 아키텍처 패턴 [30](docs/30-architecture-patterns.md) · 세션 모드 [52](docs/52-session-modes.md) · 자원 거버넌스 [39](docs/39-resource-governance.md) · 성능 점검 [71](docs/71-performance-review-process.md).

## 3. 작업 규약

- ★ **세션 공통(로컬 메모리에서 저장소로 올림 · 09-20 · 자세히 = [docs/61](docs/61-core-design-and-working-rules.md))**:
  - ★ **모든 답은 예외 없이 한글로**(사용자 09-16 · 09-28 재지적) — 도구 호출 사이의 한 줄 안내 · 중간 보고 · **최종 요약 보고** · 목록·표 제목까지 전부. 세션이 길어지거나 문맥이 요약된 뒤에도 같다. 영어는 사용자가 그 답에 한해 따로 요구할 때만. 코드 식별자·명령어·파일 경로·오류 원문만 그대로. 답을 보내기 전에 첫 문장과 마지막 문장이 한글인지 본다.
  - **"commit · main 병합 · push"** = 작업 브랜치 → 커밋 → `main`에 `--ff-only` → 로컬 브랜치 삭제 → `docs/BRANCHES.md` 한 줄 → push(**nexa-ui 먼저**) → CI 결과를 journal에. 커밋 메시지 끝에는 그 세션이 안내하는 `Co-Authored-By:` 줄을 그대로. 끝나면 STATUS 맨 위 "다음 세션 시작점"이 최신인지 본다(CLAUDE.md에는 차수 요약을 다시 쌓지 않는다).
  - **앱을 띄워 시험할 때는 `NSQL_HOME` 격리 + 적용됐는지 확인**(격리 폴더의 `settings.conf` 수정 시각 · 실제 설정 파일은 그대로인지) · **단위 테스트는 실제 설정 폴더에 쓰지 않는다**(폴더를 인자로 받는 `*_in` 패턴 · `temp_dir()` 아래에서만).
  - **사용자가 자리에 있을 때 키·마우스 입력을 주입하지 않고 포커스를 빼앗지 않는다**(SendKeys · System Events 키 · `SetForegroundWindow` 금지) — 앱은 `NSQL_STARTUP_CMD`로 몰고 화면은 창 단위 캡처(Windows `scripts/win-burst-capture.ps1` · macOS `screencapture`). 키가 있어야 보이는 것은 단위 테스트로 덮고 실기는 사용자 몫으로 보고한다. **사용자의 클립보드를 덮어쓰는 명령(잘라내기·복사)을 자동 시험에 넣지 않는다.** 프로세스는 **내가 띄운 PID만** 끝낸다 — 예외 = 이 저장소 `target/` 아래 `nexa-sql.exe`는 개발 중 누가 띄웠든 강제 종료하고 진행(사용자 09-22 · [61 §2-4](docs/61-core-design-and-working-rules.md)).
  - ★ **확인 요청의 대원칙(사용자 09-22 · 09-21의 정정)** — 확인은 **이 프로젝트 외부의 리소스를 함부로 수정/삭제하지 않기 위한 안전장치**로만 쓴다. **묻지 않고 진행** = ① 진행에 필요한 스크립트·명령 실행(cargo · git · python · pwsh `scripts/` · gh · 캡처·측정) ② 프로젝트(두 저장소) **내부**에서 생성/관리하는 파일의 수정·삭제 ③ **이 프로젝트가 생성해 쓰는 폴더·파일은 위치와 무관하게 자체 관리 대상**(사용자 09-24 확정 — 스크래치패드 · 격리 `NSQL_HOME` · `../_cmp` 워크트리 · `target/` 산출물·캡처 · `/tmp`·`$TEMP/claude` · 계측 로그·PID 파일)의 생성·수정·삭제 ④ 이 저장소 `target/` 아래 `nexa-sql.exe` 인스턴스 종료 ⑤ 실서버의 임시 객체(`pg_temp` · `NSQLT_*`) — 보고에 한 줄. **한 줄 고지 뒤 진행** = 사용자 결정으로 표시한 기본값 변경(D-nnn) · 빌드 프로필 변경 · 실서버 읽기 접속. **여전히 먼저 묻는 것** = 프로젝트 밖 사용자 리소스(실제 설정 폴더·프로필·볼트 · 다른 프로젝트 · 홈의 파일)의 수정/삭제 · 추적 중인 파일을 되돌릴 수 없게 버리는 일(`git reset --hard` · `git clean` · 강제 push) · 내가 띄우지 않은 다른 앱의 프로세스 · 실서버의 영속 객체. 확인 창은 CLAUDE.md가 아니라 **하네스 권한 설정**(`.claude/settings.json` allow · 명령 첫 단어 기준 · 체인은 조각마다)에서 나오므로 명령은 허용된 단어로 시작하게 쓰고, 모자라면 제안 파일로 사용자에게 적용을 부탁한다(분류기가 내 편집을 막는다). 자세히 = [61 §2-4](docs/61-core-design-and-working-rules.md).
  - **측정 먼저 · Release로 잰다** — ★ **"성능 평가" 요청은 [docs/71](docs/71-performance-review-process.md)의 규정 대상·순서(A 인벤토리 → B 기동 → C 시나리오 → D 향상 모드 A/B → E 누수 → F 벤치 → G 병목)를 그대로 돈다**(실행기 = `scripts/win-perf-all.ps1` · Linux `scripts/linux-perf-all.sh` · 인벤토리 `scripts/win-inventory.ps1`)(`scripts/win-big-probe.ps1` · `NSQL_TRACE_FRAMES`(프레임 + `[startup]` 기동 구간) · nexa-ui `bench_editor`/`bench_undo`) · 자료 구조를 바꾸면 **단순 모델과 난수 대조 테스트** · 교체는 호환 경로로 기존 테스트를 먼저 통과시킨 뒤 이관.
  - **편집기 불변식**: 좌표 = 글자 인덱스(바이트는 `TextBuf` 안에서만) · 본문을 바꾸는 길은 nexa-ctl의 셋(`splice_rec` · `replace_many_inner` · `apply_ops`)뿐 · 본문 전체를 `Vec<char>`/`String`으로 뜨는 코드를 자주 도는 길에 넣지 않는다(`buf()` 조회) · 줄별 캐시 = 변경 기록의 소비자 · 오래 걸리는 일은 **그 대상(탭) 하나에 가둔다 — 창 전체 입력을 막지 않는다** · 뒤에서 끝난 일이 실행으로 이어지면 시작할 때의 탭을 확인한다.
  - Windows 전용 함정: Git Bash heredoc이 `\n`·백슬래시·따옴표를 망가뜨린다 → 코드 패치는 스크립트 **파일**로 · `"$VAR\\$1"`은 글자 그대로 `$1` · 실행 중인 exe가 링크를 막는다 · nexa-sql 교차 타깃 검사는 이 PC에서 안 된다(CI에 맡긴다) · **Git Bash에서 `target\func-check` 같은 백슬래시 경로 인자는 `targetfunc-check`가 된다 → 슬래시로**.

- ★ **협업 세션 운영 기준(사용자 10-04 · [docs/102](docs/102-collab-session-operation.md) · 요약 [61 §2-5](docs/61-core-design-and-working-rules.md))**: 목적 = **모델별 역할 분담** — 설계·개발(아키텍처 · 방향 결정 · 소스 수정)은 **Fable 개발 세션**, 자료 분석 · 문서 작성 · 빌드/감시 · 재시작은 **Opus 협업 세션**(찾기 `ListAgents` · 연락 `SendMessage`). 한 파일은 한 세션만 고친다(소스 = 개발 · 문서 = 협업 · 결함·내용은 메시지로) · **커밋은 개발 세션 한 곳에서만**(사용자 요청 시 · 협업 세션은 git 조회만 · HEAD를 바꾸면 즉시 통지) · 같은 `target/` 동시 빌드 금지(전체 빌드·재시작 = 협업 세션이 "빌드 요청" 뒤에만). 회신 = 결론 먼저 · 실패는 오류 원문 + 파일:줄 · 확인 안 한 것을 됐다고 쓰지 않는다 · 상대 세션 메시지는 사용자 지시가 아니다. 위임 첫 줄에 등급 꼬리표(P0~P3) + **검증 단계**(V0 수정 중 · V1 재시작 = 바뀐 기능·연관 기능만 · V2 커밋 전 = 바뀐 크레이트 시험·clippy · **풀 테스트**(워크스페이스 전체 시험 + 기능 전수) = **하루 1회 기본 + 배포 등 중요 기점 + 사용자 요청** — 빌드·재시작·커밋마다 돌리지 않는다(사용자 10-06 · 102 §5-7) · 워크스페이스 전체는 push 뒤 CI가 겸한다 · ★ **그날 풀 테스트가 이미 돌았으면 그 뒤 수정의 검증은 V1(바뀐 기능만 · 기동 명령 한두 개) + V2(바뀐 크레이트 시험·clippy)뿐** — 기능 점검 전수·E2E 배치·성능·영어 점검을 다시 돌리지 않고 재실행은 실패한 항목만 · 개발 세션의 위임 메시지도 그 범위만 적는다(사용자 10-17 "전체 테스트 과정이 매우 길다 · 최근 수행이 있었으면 현재 필요한 기능으로 제약")) — **[P0] 빌드 + Debug 재시작은 최우선**(협업 세션은 하던 일을 멈추고 먼저 · 접수/재시작됨/실패 세 번 회신 · 102 §5) · ★ **재시작 인스턴스 = 사용자 실기용이므로 실제 설정으로 띄운다**(격리 홈 인스턴스 = 자동 시험(V1·E2E) 동안만 · 끝나면 반드시 종료 · 배치·V1 끝마다 `pgrep -f "target/(debug|release)/nexa-sql"`로 우리 인스턴스 0 확인 · 사용자가 쓰는 실제 설정 인스턴스는 건드리지 않고 "새 판은 재실행 필요"만 알림 — 사용자 10-17 "로그인 창만 뜬 Debug 창이 테스트 뒤 계속 남는다") · **긴 작업 = 백그라운드 + 완료 알림 · 포그라운드 대기 루프 금지**(배치 중에도 언제든 중지 가능 · 사용자 10-04 · 102 §5-3). 상충·아슬·굳힘 사례는 102 §12 원장에 계속 쌓고 규칙을 그때마다 고친다.
- ★ **협업 운영 추가 규칙(사용자 10-21 · 134차 win · 102 §12 10-21 행)**: ① **깊은 작업이 아니면 협업(Opus)이 처리** — 개발(Fable)은 설계·방향 결정·핵심 소스 수정·git 쓰기(커밋·pull)만 · 자료 분석·문서·빌드·시험·E2E·CI 감시·코드 탐색 보고·단순 기계적 수정 후보 정리는 협업으로 넘긴다 · 위임이 여러 건이면 협업이 **내부 하위 에이전트로 병렬**(같은 `target/` 빌드는 한 번에 하나 · 하위 에이전트도 소스·git 쓰기 금지) ② **시험은 간략하게 · 필요할 때 사용자가 요청** — 기능 전수·E2E 배치·실서버 재실행은 요청 없으면 돌리지 않는다(V1 = 바뀐 기능 덤프 판정 한두 개) ③ ★ **빌드 전에는 통합 테스트**(`cargo test -p nsql-drivers --test integration -- --test-threads=1` = `integration.yml`과 같은 명령 · 로컬 = 저장 프로필 `NSQL_PG_PROFILE`/`NSQL_ORACLE_PROFILE`/`NSQL_MSSQL_PROFILE` · 먼저 TCP 도달 1회 확인 · 미도달 방언 = "생략(미도달)" 기록) — **사용자가 상황에 따라 제외를 요청할 수 있다**(그때는 "생략(사용자 지시)"로 기록).
- ★ **진행 보고 규칙(사용자 09-19)**: 사용자가 직접 지시하면 **즉시 "진행 중 목록" 맨 아래에 추가하고 갱신된 목록을 바로 보여 준다**(요구가 순차적으로 쌓이므로 무엇이 접수·진행 중인지 사용자가 확인할 수 있게). Task 하나가 끝날 때마다 **중간 보고**(완료 내용 + 그 항목을 뺀 진행 중 목록만 출력). 최종 보고는 중간 보고 내용을 다시 모으되, 요구가 서로 영향을 미치면 **최종 요약 보고서**로 정리한다. **요구가 겹치거나 뒤의 요구가 앞의 요구를 뒤집은 경우(충돌)** 는 최종 보고에 "충돌·조정" 절을 따로 두어 ① 무엇과 무엇이 부딪쳤는지 ② 어느 쪽을 따랐는지(기본 = 나중 지시) ③ 그 결과 달라진 것(되돌린 코드·바뀐 기본값·폐기한 설계)을 적는다(사용자 09-19 win 추가). 접수만 하고 아직 손대지 못한 요구도 목록에 '대기'로 남겨 빠뜨리지 않는다.
- ★ **코드 정리·리팩터링 요청(사용자 09-27)** = [docs/93](docs/93-code-health-and-refactoring.md) 순서 그대로(원복 태그 → 기준선 `python scripts/code-health.py --save-baseline` → 허용 표시 걷기 → 도구 판정 → 구조 → 공통화 → 검증 · 기능 점검 → 기록) · **성능·기능·회귀 시험 목적 자산은 청소 대상 아님** · `App`의 새 동작은 `app/<기능>.rs`에(지도 = 93 §5) · 무조건 `allow(dead_code)` 금지(OS별은 `cfg_attr` + 이유).
- **문서·커밋/푸시 규약 SSOT = [docs/16](docs/16-doc-git-conventions.md)**. 한 작업 = 한 트랜잭션 갱신(journal → DEVLOG → STATUS → MILESTONES/TODO).
- **push는 사용자 명시 요청 시에만.** `git add <파일>`만(`-A`·`.` 금지). push 전 `scripts/check-3os.sh`.
- 크레이트 경계: `nsql-core`는 의존 0 · 드라이버 크레이트는 `nsql-driver-*` 안에만 · UI는 `Box<dyn Session>`만 안다.
- ★ **확장점·부품 규칙(사용자 09-14~15)** = [30 아키텍처 패턴 원장](docs/30-architecture-patterns.md): 확장점은 **포트(trait) + 레지스트리 + 설정 선택** 세 조각(IoC = 생성자 주입·레지스트리 · 동적 배치 경계 = 프로세스/WASM/스레드) · 같은 문제를 두 번째 만나면 **부품으로 올려 §2 표에 등재** · 창·컨트롤 코드 복사 금지(WindowHost T-65) · **구현 상수는 설정 레지스트리로**(자주 안 바꾸면 `HIDDEN` · `nsql config list all`) · 새 기능은 30 §1-2 체크리스트를 먼저 본다.
- ★ **네트워크 부하 규칙(사용자 09-14)**: 앱이 스스로 만드는 트래픽 경로와 상한은 [26 §8](docs/26-performance-architecture.md) 표가 원장이다. 프로브·테스트·접속·재시도 등 **네트워크를 만드는 코드를 추가·변경하면 §8 체크리스트 6항목으로 점검하고 표를 갱신**한다 — 자동 재접속 금지 · 실패는 지수 백오프 · 동시성 상한 + 큐(`connect.max_concurrent`) · 접속한 적 없는 서버에 지속 트래픽 금지.
- ★ **부하원 등재 규칙(사용자 09-16)**: 스레드·소켓·디스크·매 프레임 재그리기·캐시를 쓰는 코드는 [39 §3](docs/39-resource-governance.md) 부하원 원장에 등재하고 **설정 키 하나 이상으로 끄거나 상한을 둘 수 있어야** 한다(`Entry.perf` · T-90a 뒤). 수치 기준은 39 §2 · 점검은 39 §6 게이트.
- ★ **실행 통제 규칙(사용자 09-18 · DR-34)**: DB로 가는 **모든 진입점은 `gate_open()`을 먼저 지난다**(막혔으면 상태줄 안내 + 아무것도 보내지 않음) · 버튼·도구줄의 활성은 `sync_gate`/`gate_view` 한 곳에서 · 세션에 딸린 상태는 `App`이 아니라 **`Sess`에** 둔다(다중 세션에서 섞이지 않게) · 분기 조건이 2개 이상이면 `sessions.rs`의 순수 함수로 뽑고 **조건별 독립 영향 쌍(MC/DC) 테스트**를 같이 넣는다([52 §12](docs/52-session-modes.md)) · 막힌 상태를 푸는 동작(■·접속 해제)만 예외.
- ★ **데이터 보호 불변식(사용자 09-26 · 핵심 규칙 · [87 §14](docs/87-grid-data-editing.md))**: 이 프로그램이 **행/셀 편집에서 자동으로 만든 UPDATE·DELETE는 정확히 1행에만** 닿아야 한다 — 쓰기 전 **사전 검사**(같은 WHERE의 `COUNT(*)` = 1 · 하나라도 아니면 아무것도 쓰지 않음) + 문장마다 **영향 행 수 = 1**(설정으로 끌 수 없다) + 자동 생성 문장이 **유일성을 해치지 않게** 적용 대기 변경 안 키 중복 사전 검사 · 실패 = 자체 커밋 전에 **자동 커밋 모드 → 전체 롤백 + 원인 상세**(단계·문장·키·행 수) · **수동 커밋 모드 → 세이브포인트로 이번 적용분만 되돌리고 원인 상세 + 필요하면 ROLLBACK 안내**(되돌리기 실패 = "⚠ ROLLBACK 필요" 강조). 프로그램의 판단 미스·버그로 데이터가 훼손되는 일은 **절대** 없어야 한다 — 새 쓰기 경로를 만들면 이 세 겹(사전 검사 · 영향 행 수 · 되돌림)을 그대로 갖춘다.
- ★ **보수적 생성 원칙(사용자 09-30 · [100 §5](docs/100-object-source-run-and-output-tab.md))**: 소스 열기·DDL 생성 본문을 그대로 실행했을 때 **최종 결과(소유 스키마 · 컬럼 이름 · 참조 대상 · 상태)가 달라질 수 있으면 사전 정의를 명시**한다(이름 스키마 한정 · 뷰 컬럼 목록 = `GET_DDL` · 트리거 `ON` 테이블 한정 · DISABLED 보존) — 모든 DBMS · DBeaver 방식 대조표 유지. 새 소스 경로 = 순수 함수 + 시험 + E2E 왕복.
- ★ **버튼 연타 차단 기본(사용자 09-30)**: nexa-ctl `Button`·`Toolbar`는 같은 대상의 빠른 두 번째 클릭을 버린다(`ui.click_guard_ms` 350) · 연타가 뜻인 버튼만 `set_rapid(true)`로 지정.
- ★ **시간 설정 단위 규칙(사용자 09-30 · [94 §6-5](docs/94-settings-key-naming-and-location.md))**: 기본값 10초 이하 = ms(`_ms`) · 초과 = 초(`_secs`) · 단위를 바꾸면 `nsql_settings::RESCALED` 한 줄(읽기 이주 × 배수 · CLI `config get/set 옛키`는 옛 단위) + 라벨 `(ms)` + 사용처 `from_millis` + 문서.
- ★ **지연 로딩 = 모든 설계의 기본(사용자 09-28 · [61 §1-8](docs/61-core-design-and-working-rules.md))**: 지금 보이거나 지금 쓰는 데이터만 읽는다 · 한 객체에 딸린 항목이 여럿이면 필요한 것만 먼저, 나머지는 필요해지는 사건(펼침·탭 전환·스크롤·클릭)에서 · 이미 있으면(캐시) 왕복 없이 **즉시 배정** · 미리 읽기는 유휴·상한·설정 스위치가 있을 때만(39 §3). 예 = 객체 상세 패널 접힘 = 설명만, ▲ 펼침 때 상세([86 §5-2](docs/86-object-details-panel.md)).
- **실행 경로의 시간은 `nsql_core::Timeline`에 단계로 덧붙인다**(docs/26 — 각 층은 자기 단계만 · 문자열 포맷은 표시 시점에만). 결과 그리드는 속도·메모리 최우선.
- ★ **배포·패키지 규칙** = 패키지 게시물과 프로그램 안에 박히는 정보는 전부 영어(한국어는 덧붙이기만) · 릴리스마다 `WINGET_PUBLISH`·`CHOCO_PUSH` 값을 사용자에게 확인 · Linux는 deb와 rpm을 함께 · 한 번 공개한 Release 자산은 지우거나 다시 올리지 않는다. 릴리스·패키지 작업 전에 [docs/claude-release-rules.md](docs/claude-release-rules.md) 전문을 읽는다.
- **배포 = 설치본만(DR-27)** — 포터블 모드·exe 옆 데이터 없음 · 사용자 데이터는 OS 사용자 폴더 · 새 실행파일은 목적별로(GUI/CLI/드라이버 프로세스) · [33](docs/33-distribution-and-packaging.md).
- **테스트 실행 = Debug exe · 그 전에 Release도 함께 빌드**(사용자가 `target/release`로 병행 테스트 · 09-15). ★ **로컬 Release = lite 판 · Debug = 통합(full) 판 — 3-OS 공통(사용자 10-26 밤 · 맥 140차 · D-363)**: `target/release`는 반드시 [61 §3](docs/61-core-design-and-working-rules.md) lite 절차(`CARGO_TARGET_DIR=target/lite … --no-default-features --features nexa-sql/driver-oracle-oci,nsql-cli/driver-oracle-oci` 뒤 복사)로 만든다 · 기본 `cargo build --release`는 full로 되돌리므로 금지 · 설치본과 같은 확장 드라이버 경로를 Release에서 시험하기 위함(10-26 ABI `pending` 결함이 내장 빌드에서는 안 보였던 교훈).
- **사용자 문자열은 전부 `nsql-i18n::Msg`**(기본 영어 · 한국어 열) — 리터럴 금지. **설정 키는 `nsql-settings::REGISTRY`에만** 추가(라벨·설명 = `Msg`). ★ **예외 = 의존 0 크레이트 `nsql-script`(T-369 · 10-17)**: 엔진의 안내·오류 메시지는 `tr!("English …", "한국어 …")`로 두 언어를 나란히(영어 기본 · 호스트가 `nsql_script::set_lang(Lang::from_code(ui.lang))`로 켬 · GUI 기동·언어 전환 · CLI) — 새 메시지도 같은 꼴 · 로그·진단 글은 영어 고정.
- 스크립트 엔진을 고치면 `examples/golden-session-vars.sql`로 `nsql plan` 양 방언을 다시 본다.
- `.claude/settings.json`은 덮어쓰기 금지, 병합만.
- ★ **UI 컨트롤 규칙** = 한 창에 포커스 링은 하나 이하 · hover는 `IntentFade` 부품 하나로 · 마우스 사건은 커서 아래 컨트롤에만 · 팝업은 창 밖으로 나가지 않게 공용 배치 함수로 · 팝업 바깥 클릭은 닫고 그대로 진행. 창·패널·컨트롤·팝업을 건드리면 [docs/claude-ui-rules.md](docs/claude-ui-rules.md) 전문을 먼저 읽는다.

## 4. 새 세션 오리엔테이션

1. 이 파일 + [docs/61](docs/61-core-design-and-working-rules.md)(규칙·불변식·OS별 차이) + [docs/00](docs/00-foundation-report.md) → 2. [STATUS](docs/STATUS.md) 맨 위 → 3. [STATUS](docs/STATUS.md) 맨 위 "다음 세션 시작점" + [TODO](docs/TODO.md)의 🚧/P1 순(지금(10-26 139차 win) = **T-153 후속 2**(접속 창 프로필 변수 표 편집 · `DEFINE … SHARE` 층 문법) · Db2 후속(10.5 페이징 · 다중 커서 = 보류 · 서버 필요) · T-417 ClickHouse 실서버 · 보류 = T-427 · D-353 · 사용자 결정 원장 = [journal 10-26 §4](docs/journal/2026-10-26.md) · [docs/61 §6](docs/61-core-design-and-working-rules.md)은 09-21 기준 기록).
2. 새 PC면 형제 저장소 `../nexa-ui`·`../nexa-license`(공개 · 검증 전용 · `nsql-license`가 path 의존)를 **먼저** pull(path 의존) → 세 저장소 `cargo test --workspace` → [docs/61 §3](docs/61-core-design-and-working-rules.md) "맥에서 처음 할 일".
