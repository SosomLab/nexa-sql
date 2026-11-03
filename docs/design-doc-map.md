# 설계 문서 지도(요구별)

> 2026-10-22에 CLAUDE.md §2에서 글자 그대로 옮겼다(이력). 새 설계 문서를 만들면 맞는 요구 표에 한 줄을 더한다.
> ★ **11-01 B8 재구성**: 한 줄 나열 → 요구별 표(요구 → 원문 문서 · 보조 문서) · 기존 링크 전부 유지 · `docs/` 접두 깨진 링크 정정 · 빠진 번호 문서 추가(11-01 정합).
> ★ **규칙·운영 원문** = [CLAUDE.md §4](../CLAUDE.md) · [61](61-core-design-and-working-rules.md) · [102](102-collab-session-operation.md) · [16 §4-1](16-doc-git-conventions.md) · [claude-ui-rules](claude-ui-rules.md) · [claude-release-rules](claude-release-rules.md) · 문서 홈 = [README](README.md) · 현황 = [STATUS](STATUS.md) · [MILESTONES](MILESTONES.md) · [TODO](TODO.md).

## 1. 기반 · 결정 · 규칙 · 운영

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 기반 보고서 · 아키텍처 · 로드맵 | [00 기반 보고서(언어·플랫폼·아키텍처·지원 범위·조사 종합)](00-foundation-report.md)(11-01 정합) · [01 아키텍처(계층 독립·포트 연계)](01-architecture.md)(11-01 정합) | [02 로드맵(09-12 원안 · 현황 = MILESTONES)](02-roadmap.md)(11-01 정합) |
| 결정 원장 | [10 결정 기록(DR·DP·D)](10-decision-record.md)(11-01 정합) | — |
| 열린 항목 | 📐 ★ **열린 항목 브리프(P0 8 + P1 지시 대기 5묶음 · 항목별 현황·개발 방향·테스트·결정 필요 · 10-29 · ✅ 사용자 결정 반영 D-393) [126](126-open-items-brief.md)** | — |
| 규칙·불변식 · 협업 운영 | ★ **핵심 설계의 불변식 · 작업 규칙 · OS별 차이(다른 PC에서 이어 가기) [61](61-core-design-and-working-rules.md)** · 📐 ★ **협업 세션 운영 기준(Fable 개발 × Opus 협업 · 시작 절차 · 역할 표 · 파일 소유 · 메시지 본보기 · P0 우선순위·끼어들기 · git · 동시 빌드 금지 · 상시 갱신 · 사례 원장) [102](102-collab-session-operation.md)** | [16 문서·커밋/푸시 규약](16-doc-git-conventions.md)(11-01 정합) · 📐 ★ **코드 건강 점검·리팩터링 절차(원복 태그 · 순서 P0~P8 · `scripts/code-health.py` · 판정 규칙 · `app/` 구조 지도 · 차용 체크리스트) [93](93-code-health-and-refactoring.md)** |
| 아키텍처 패턴 · 부품 | **아키텍처 패턴·확장점·부품 원장 [30](30-architecture-patterns.md)** | 📐 ★ **데이터 작업대 구조 — 객체 척추(`ObjectRef` → `MetaStore` → `ObjectAction` = 정보·hover·Ctrl+클릭·바로가기) + 데이터 척추(`ResultData` → `View` 필터 → `ChangeSet` 편집 → `SqlGen` → gate 실행 · `Export` 싱크 = 파일/XLSX/Excel 인스턴스) · T-177~T-183 · D-197~200 [77](77-data-workbench-architecture.md)** · [99 "마지막 사건만" — 최신 의도 코얼레싱](99-latest-intent-coalescing.md)(11-01 정합) |
| 경쟁·생태계 조사 | [03 크로스플랫폼 SQL 클라이언트 조사](03-competitive-landscape.md) · [04 Oracle 도구·SQL*Plus 의미론](04-oracle-tools.md) · [05 MSSQL 도구·세션 변수 우회](05-mssql-tools.md) · [06 Rust 생태계(드라이버·GUI·배포)](06-rust-ecosystem.md)(11-01 정합) | 📐 ★ **UI/UX·디자인 국제 표준(원천 11 · 라이선스 · 점검표 30 · §218 결함 대조) [88](88-ui-ux-standards-survey.md)** |

## 2. 편집기

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 편집기·패키지 구조 | [09 편집기·패키지 확장 설계(`nexa-edit`)](09-editor-and-packages.md)(11-01 정합) · 편집기 점증 순서 [17](17-editor-incremental-plan.md) | [07 편집기 조사(기능 인벤토리·Sublime 패키지)](07-editor-research.md)(11-01 정합) · 사용자 Sublime 프로필 [12](12-user-sublime-profile.md) · 폰트·기능 모듈 [14](14-fonts-and-feature-modules.md) |
| 구문·팔레트·상태줄·색 | [29 구문 강조·명령 팔레트·서식 복사·상태줄·안내선](29-editor-syntax-palette-statusbar.md)(11-01 정합) | 편집기 색·선택 참조(DBeaver·Golden 실측) [35](35-editor-colors-reference.md) · [118 편집기 줄 간격(타 도구 조사·설정 체계)](118-editor-line-spacing.md)(11-01 정합) |
| 들여쓰기 · 포맷터 | 들여쓰기 설정 계층 [31](31-indentation-settings.md) · Auto Indent(Sublime 변수 4·규칙 세트) [49](49-auto-indent.md) | 📐 ★ **SQL 포맷터(내장 Basic · 공통 옵션 · ABI v1.1 `nx_ext_format` · 확장 포맷터 · D-147~D-155) [95](95-sql-formatter.md)** |
| 완성 · 아웃라인 | ★ **IntelliSense · 문서 아웃라인(조사 · 문맥/랭킹 · 팝업 · Goto Symbol · 메타 feed) [76](76-intellisense-and-outline.md)** · 📐 ★ **아웃라인 방향 = 선언된 이름 지도(심볼 기본 보임 · 문장 토글 4 · CREATE 1행 · 트리+접기 · 커서 추종 · D-357 · T-422/T-423) [76 §20](76-intellisense-and-outline.md) · 조사 원문 [76a](76a-outline-research.md)** | [82 문법 참조 기반 완성(절별 예약어·객체 · `.sqlg` 플러그인)](82-grammar-driven-completion.md)(11-01 정합) · **객체 인텔리센스(메타 저장소 한 벌 · D-79~86 · 설정 §8) [47](47-intellisense-metadata.md)** |
| 객체 링크 · 줄 끝 라벨 · 오류 원인 | 📐 ★ **Ctrl 객체 하이퍼링크 + 설명 툴팁(관계 자리·별칭·카탈로그 판정 · `Resolver` 포트 · 툴팁 4위치 · 향상 모드·큰 파일 끔 · T-256/T-257) [96](96-object-links.md)** | [111 줄 끝 라벨·Ctrl 홀드 표시(북마크 라벨·테이블/컬럼 설명)](111-inline-labels-and-ctrl-hold.md)(11-01 정합) · [117 구문 오류 원인 분석(키워드 오타 후보·첫 후보 캐럿)](117-syntax-error-cause-analysis.md)(11-01 정합) |
| 다중 캐럿 일괄 생성 | 📐 ★ **Insert Sequence(다중 캐럿 일괄 생성 · 표현식 한 줄 문법 `시작[연산 증가][#반복][~형식] [x 목록] ["틀"]` · 팔레트 프롬프트 + 미리보기 + `?` 도움말 · Ctrl+Alt+N · D-366) [124](124-insert-sequence.md)** | — |
| 미니맵 · 괄호 | 미니맵 기능 조사·우선순위 [46](46-minimap-features.md) | [51 첫 플러그인 — 레인보우 괄호 + 괄호 이동 메뉴](51-rainbow-brackets.md)(11-01 정합) |
| 입력 지연 · 되돌리기 | [55 편집기 입력 지연 조사·실측·개선](55-editor-input-latency.md)(11-01 정합) · **되돌리기 재설계(연산 기록 · 11종 조사 · 남은 제안) [60](60-undo-redo-redesign.md)** | [62 macOS 점검(한글 입력·화면 내보내기·메모리·객체 DDL)](62-macos-input-and-present.md)(11-01 정합) |
| 대용량 파일 · 크기 제약 | **대용량 파일(큰 파일 모드 · 탭 격리 적재 · 실측) [59](59-large-file-handling.md)** · **대용량 파일(병목 수정 · 메모리 회수 · 편집기 비교 · 3단계 제안 D-125~128) [59](59-large-file-handling.md)** | ★ **크기·줄 수 제약 원장(L1/L2 단계 · 자체 상한 전수 · 새 상한 = 키+표+39+위키) [72](72-size-limits-and-large-file-constraints.md) · 사용자 위키 [docs/wiki](wiki/Large-Files-and-Limits.md)** |
| 북마크 | [69 북마크 관리(전수 조사·앵커링·문서 동일성·수명·저장)](69-bookmarks.md)(11-01 정합) | — |

## 3. 스크립트 · 변수 · 실행

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 세션 변수 · 스크립트 엔진 | [08 세션 변수·스크립트 엔진(`nsql-script`)](08-session-variables.md)(11-01 정합) · **변수 관리(Usage 19군 · VarStore/MacroStore/Caps · 방언별 기법 · 단계 · ★ 내장 변수 층 `${workspaceFolder}`·`${config:키}`·`NSQL_*` = nsql-script `intrinsic.rs` · §10) [63](63-variable-management.md)** | [63a 변수 관리 조사 원문](63a-variable-research.md)(11-01 정합) · 📐 **탭별 변수 보존의 문서 식별(경로·fsid·지문·문서 id·프로젝트 탭 id 층 쌓기 · D-191~195) [74](74-vars-persistence-identity.md)** |
| 바인드 변수 | 📐 ★ **바인드 변수 실행 전략(`:VAR` = 바인드 기본 · 리터럴은 바인드 불가 자리만 · 방언별 현황 · 기술 평가 · D-356 · T-416~T-420) [123](123-bind-variable-execution-strategy.md)** | — |
| 객체 소스 실행 · Output | 📐 ★ **객체 소스 탭 F5 = 한 단위 · Output 탭 · 소스 열기 vs DDL 생성 분석 · Oracle DDL E2E [100](100-object-source-run-and-output-tab.md)** · ★ **객체 편집기 구현 범위 현황(11-03 · 소스 기준 · 기능×방언 · 사용 방법 · 미구현 15) = [100 §6](100-object-source-run-and-output-tab.md)** | — |
| 실행 계획 | 📐 ★ **실행 계획 검토(방언별 EXPLAIN 관용 · 공통 트리 파싱 · 자동 검토 규칙 7 · 추정/실제 · GUI 결과 탭 2 · CLI `--analyze`/`--raw` · 사용법) [122](122-execution-plan-review.md)** | — |
| 서버 메시지 · 오류 | 서버 메시지·실행 중 로그 [32](32-server-messages-and-live-log.md) · DBMS 오류 정규화·토스트 [42](42-db-error-normalization.md) | **실행 로그 상세 계측·개발자 모드·저비용 게이트 [48](48-logging-architecture.md)** |

## 4. 접속 · 세션 · 트랜잭션

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 연결 프로필 · 접속 대화상자 | 연결 프로필 [21](21-connection-profiles.md) · 드라이버 확장·접속 대화상자 [22](22-driver-extensions.md) | **DBMS별 종속 설정(Oracle Instant Client 자동 탐지/직접 지정) · 드라이버 내장 ↔ 확장 검토 [64](64-dbms-clients-and-driver-packaging.md)** |
| 세션 모드 · 실행 통제 | **세션 모드(공유 N·전용 CONNECT·개별 · 실행 통제 · 유휴 · MC/DC) [52](52-session-modes.md)** | [54 연결 모델과 접속 해제 UX(DBeaver 항해자 대비)](54-connection-model-and-disconnect.md)(11-01 정합) |
| 접속 생존 · 끊김 전파 | 📐 ★ **서버 연결 끊김 다계층 감지·기능별 반응·전파 `ServerHealth`(VPN 끊김 사건 · 메타 세션 생존 관리 부재 · 감지 L0~L4 · 기능별 반응 표 · 안내 채널 · T-313 단계 ①~⑥ · D-265~269) [107](107-server-health-propagation.md)** | [53 접속 생존 관리(장기 방치·네트워크 단절)](53-connection-liveness.md)(11-01 정합) |
| SSH 터널 | 📐 ★ **SSH 터널 원리·수명(대상당 ssh 1 공유 · 명령창 숨김 · Job · PID 원장 · D-391) [125 §7](125-ssh-tunnel-real-server-test.md)** | 📐 ★ **실서버 SSH 터널 시험 ✅ 통과(10-28 밤 · 로컬 ssh 설정 · `jumpuser@172.16.0.108` 경유 Oracle 172.16.0.110/APPDBDEV · 키 = 프로필 `ssh_key` · `integration::ssh_tunnel_real_server` · `scripts/win-ssh-tunnel-test.ps1` · T-3 3차 ✅) [125](125-ssh-tunnel-real-server-test.md)** |
| 트랜잭션 | 트랜잭션 UX [34](34-transaction-ux.md) · **트랜잭션 로그(DBeaver QM + Tx 열) [44](44-transaction-log.md)** · **트랜잭션 로그(DBeaver QM 검토 · Tx 열 · T-107) [44](44-transaction-log.md)** | **수동 커밋 잠금 방지(도구 조사 · L1~L4 · D-100~105) [56](56-manual-commit-lock-prevention.md)** · [66 트랜잭션 테스트 시나리오(DBMS별 대본)](66-transaction-test-scenarios.md)(11-01 정합) |
| 운영기(PRD) 보호 | [114 운영기 보류 커밋·방어 장치 전수·Activity 설계](114-prod-held-commit-and-safeguards.md)(11-01 정합) | [115 운영기 점검 시나리오 — `nsql` 사용법](115-prod-mode-scenario-nsql-guide.md)(11-01 정합) |

## 5. DBMS 지원 · 드라이버

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 지원 DBMS 우선순위 | [121 지원 데이터베이스 분석·우선순위](121-database-support-priority.md)(11-01 정합) | 📐 ★ **Db2 LUW 조사 원문(CI 컨테이너 · clidriver 배포 · SYSCAT 질의 검증 · SQLCODE 분류 검증 · EXPLAIN · DBeaver 대조 · D-359) [121a](121a-db2-research.md)** · 📐 ★ **Db2 네이티브 DRDA 조사 원문(DSS·코드포인트·FD:OCA 1차 최소 집합 · SECMEC 3/9 · TLS · Db2 LUW 고유 · 순수 구현체 justybase-db2/go-db2/pydrda · 범위 · D-361 후보) [121b](121b-db2-drda-research.md)** |
| 드라이버 어댑터 | [110 Oracle Thin 드라이버 어댑터(설정 2중 종속·기능 변화)](110-oracle-thin-driver-adapter.md)(11-01 정합) | [06 Rust 생태계](06-rust-ecosystem.md)(§1 표와 같음) |
| 문법·객체 전수 | [108 DBMS별 문법·객체 생성 전수 조사(재사용 템플릿)](108-dbms-syntax-and-objects-survey.md)(11-01 정합) | [109 ANSI/ISO SQL 문법 전수(SQL:2016·2023)](109-ansi-sql-syntax-survey.md)(11-01 정합) |
| 실서버 시험 환경 | [20 실서버 테스트(Codespaces·Docker·Actions)](20-testing-codespaces.md)(11-01 정합) | — |

## 6. 결과 · 데이터 작업

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 페치 · 결과 탭 | **페치 모델·결과 탭(상한·커서·Ctrl+\\) [43](43-fetch-model-and-result-tabs.md)** | [103 결과 필터의 두 상한(값 목록·필터 중 자동 페치)](103-filter-values-and-fetch-under-filter.md)(11-01 정합) |
| 그리드 편집 · 데이터 보호 | 📐 ★ **그리드 데이터 편집(엑셀식 · nexa-ctl `gridedit` 재사용 부품 + nexa-sql 어댑터 · 타입/길이/NULL/기본값 · 날짜 입력 · LOB 보기 · 복제/붙여넣기 자동 확장 · SqlGen·적용 · E-1~E-6 · D-210~213) [87](87-grid-data-editing.md)** | 결과 → SQL 키 규칙 [41](41-sql-copy-key-rules.md) |
| 필터 질의 | [98 필터 질의 언어(용량·종류 술어 + 논리 조합)](98-filter-query-language.md)(11-01 정합) | — |
| 대량 I/O | 📐 ★ **대량 I/O 벌크 적재·추출 기술 검토(DBMS별 지원 방식·crate 지원·기대 배수 · 포트 `BulkSink`·`Caps.bulk_load` · CLI `nsql import` · D-220~224 · T-236) [89](89-bulk-io-review.md)** | — |
| 테이블 구조 편집기 | [119 테이블(객체) 구조 편집기 설계](119-table-editor-design.md)(11-01 정합) | [119a 조사·자산 대조](119a-table-editor-survey.md)(11-01 정합) |

## 7. 객체 탐색기 · 메타데이터

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 탐색기 골격 · DBMS별 트리 | 오브젝트 탐색기 [28](28-object-explorer.md) · [83 객체 탐색기 2차(DBMS별 트리·Generate SQL·SQL Preview)](83-object-explorer-dbms-trees-and-generate-sql.md)(11-01 정합) | [101 SQL Server 탐색기 = SSMS 골격(Databases 층)](101-mssql-ssms-explorer-databases-layer.md)(11-01 정합) |
| 갱신 · 소유 | **객체 탐색기 갱신(DDL 뒤 그 폴더만 · 유휴 워터마크 · 못 찾음 신호 · D-106~110) [57](57-explorer-refresh-after-ddl.md)** · [79 메타 소유·공유·갱신](79-metadata-ownership-and-refresh.md)(11-01 정합) | [85 서버별 객체 정보 3층(L1·L2·L3)](85-metadata-layers.md)(11-01 정합) |
| 검색 · 객체 찾기 | [84 탐색기 검색 인덱스](84-explorer-search-index.md)(11-01 정합) | [113 객체 찾기 창(Find Objects)](113-find-objects-window.md)(11-01 정합) |
| 객체 상세 | [86 객체 상세 패널(유형별 섹션·축소/확장)](86-object-details-panel.md)(11-01 정합) | — |
| 시점 캐시 · 비교 | 비교/git/오브젝트 시점 캐시 [19](19-compare-git-and-object-history.md) | — |

## 8. 파일 · 프로젝트 · 복원

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 외부 변경 | 외부 변경(git식 병합) [15](15-external-file-changes.md) · **외부 변경 정책(병합·확인 빈도·감지 시점·감시 구조 · D-119~124) [58](58-external-change-policy.md)** | [38 파일 줄끝(CRLF/LF) 정책](38-line-endings.md)(11-01 정합) |
| 세션 복원 · 자동 저장 | 세션/hot exit/프로젝트 [18](18-session-and-projects.md) · [70 자동 저장·저장 큐·프로젝트 복원](70-autosave-and-restore.md)(11-01 정합) | — |
| 프로젝트(워크스페이스) | [67 프로젝트(워크스페이스) 관리](67-project-workspace.md)(11-01 정합) · 📐 **프로젝트 파일 경로 이식(타 제품 조사 · `${project}`/`${folder:이름}`/`${home}`/`${var}` 앵커 · 자리 탭 · v2 절 구조 · D-186~190) [73](73-project-path-portability.md)** | [67a 조사 원문](67a-project-research.md)(11-01 정합) |
| 파일 검색 · 선택기 | 파일 검색 탭·프로젝트 [36](36-find-in-files-and-project.md) · 파일 선택기·트리 성능 점검 [37](37-file-picker-performance.md) | [105 OS별 빠른 파일 열거 조사](105-fast-file-enumeration-survey.md)(11-01 정합) |

## 9. 확장 · 플러그인

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 확장 시스템 · SDK | 플러그인 시스템(GitHub 매니저 · WASM/프로세스 · Python 검토) [50](50-extension-system.md) · ★ **확장 SDK · WASM 동적 로딩(ABI v1 · `extensions/sdk` · 매니저 kind=wasm · 내장 대체/폴백) [75](75-extension-sdk-and-dynamic-loading.md)** | [68 확장 개발자 API(동적 로딩/해제·샘플·배포)](68-extension-developer-api.md)(11-01 정합) |
| 확장 개발 기록 | [97 확장 "Project Explorer Menus" 개발 기록](97-project-explorer-menus-extension.md)(11-01 정합) | [51 레인보우 괄호](51-rainbow-brackets.md)(§2 표와 같음) |

## 10. CLI

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| CLI `nsql` | [11 CLI 도구 `nsql`(접속·스크립트·export/import·bulk)](11-cli.md)(11-01 정합) · [27 CLI 인자 규약](27-cli-conventions.md)(11-01 정합) | [40 CLI 사용법(순서대로 따라 하는 점검)](40-cli-usage.md)(11-01 정합) · [115 운영기 점검 시나리오](115-prod-mode-scenario-nsql-guide.md)(§4 표와 같음) |

## 11. 설정 · UI

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 설정 체계 | 설정 체계·i18n·테마 [24](24-settings-and-vscode-analysis.md) · [78 설정 카테고리 재정리](78-settings-reorganization.md)(11-01 정합) | [94 설정 키 이름 체계(위치 연계·길이·동의어)](94-settings-key-naming-and-location.md)(11-01 정합) |
| 아이콘 · 컨트롤 편입 | [106 메뉴 아이콘 점검표](106-menu-icon-audit.md)(11-01 정합) · [112 활동 막대 아이콘 재조사](112-activity-bar-icons-survey.md)(11-01 정합) | [116 nexa-ui 미사용 컨트롤 점검·편입 판단](116-nexa-ui-control-adoption-review.md)(11-01 정합) · UI 규칙 = [claude-ui-rules](claude-ui-rules.md) |
| 메모리 모니터 | [80 메모리 모니터(상태줄 총량·메모리 맵 창)](80-memory-monitor.md)(11-01 정합) | — |

## 12. 성능 · 자원

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 성능 원칙 · 점검 절차 | [26 성능 아키텍처(단계별 계측·경량 구조)](26-performance-architecture.md)(11-01 정합) · ★ **성능 종합 점검 프로세스(규정 대상 12차원 · 순서 A~G · 판정선 · 기능 추가 체크리스트) [71](71-performance-review-process.md)** | 성능 향상 벤치 [45](45-perf-boost-benchmark.md) · ★ **OS별 성능 비교(속도·용량·메모리 · Windows·mac·Linux · 최적화 전/후) [65](65-cross-os-performance-comparison.md)** |
| 자원 거버넌스 | **자원 거버넌스(기준·부하원 원장·`perf.mode`·게이트) [39](39-resource-governance.md)** | — |
| 실행 파일 크기 | [120 Release 실행 파일 크기 분석·축소 방안](120-binary-size-analysis.md)(11-01 정합) | — |
| 100차 기능 기록 | [81 100차 기능 목록과 테스트 방법(이력)](81-session-100-features-and-tests.md)(11-01 정합) | — |

## 13. 배포 · 라이선스 · 보호

| 요구 | 원문 문서 | 보조 문서 |
|---|---|---|
| 배포판 | **배포판(설치본만·목적별 exe·macOS) [33](33-distribution-and-packaging.md)** | 배포 규칙 = [claude-release-rules](claude-release-rules.md) |
| 라이선스 | 정품 인증·기능 게이트 [23](23-license-activation.md) · 라이선스 종류·인증 서버·저장소 분리 [25](25-license-tiers-and-server.md) | [13 라이선스(PolyForm NC 1.0.0)](13-licensing.md)(11-01 정합) · [91 라이선스 루트 키 운영 지침](91-license-root-key-operations.md)(11-01 정합) |
| 자산 보호 | 📐 ★ **디지털 자산 보호(자산·공격자·보호 층 · 분석 지연 기법 효과 검토 = 하지 않음 · R1~R4 · D-227~D-230) [92](92-digital-asset-protection.md)** | — |
