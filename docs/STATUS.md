# STATUS — 지금 상태 한 장

> 시간 역순. 상세는 [journal](journal/), 여기는 요약.

## 2026-09-17 (54차 · mac · 저녁) — ★ 확장 매니저 1차 ✅(저장소 = `extensions/` · 메타 v1 · Sublime식 팔레트 8명령 · sha256 · SxS) · plugins → extensions 개명 ✅ · Rainbow Pairs 배선(T-119) ✅ · 설정 그룹 Extensions + 끄면 분류 숨김 ✅ · 성능 점검 기준 50 §11 · "SDK 없이?" 50 §12

**⏳ 실기 = journal 54차 ①~⑥**. 미푸시(요청 시). → [journal](journal/2026-09-17.md)

## 2026-09-17 (53차 · mac · 저녁) — 저장소 최신화 ✅ · 52차 변경 맥 검증 green ✅ · 플러그인 관리 축 조사 [50 §8-4](50-extension-system.md) ✅ · 다음 = T-119 배선(51 §11) → T-103 캡처/위키

두 저장소 fast-forward · 빌드/테스트/clippy 전부 통과 · 매니저 보강 5건(`plugins.installed` 선언형 목록 · `platforms` · `messages` · 고정/프리릴리스 · `requires`) → T-118 ①. → [journal](journal/2026-09-17.md)

## 2026-09-17 (52차 · **win** · 이관 후 첫 세션) — ★ DR-33 결과 데이터 1세트+뷰 투영 ✅ · ★ 툴바 그룹 도크/플로팅 ✅ · ■ 중지 버튼 결함 ✅(사용자 확인) · 텍스트 보기 자동 페치 결함 ✅ · **전체 조회 = 나머지 이어 받기(위치 유지)** ✅ · 컬럼 최대 폭 24자(자동/직접) ✅ · 트랜잭션 로그 설계 [44](44-transaction-log.md) 📐(T-107 · §4 DBMS 공용성) · **실행 속도 향상 `perf.boost`** ✅([39 §4-6](39-resource-governance.md) · 배선 누락 점검 · 벤치 [45](45-perf-boost-benchmark.md)) · 툴바 트랜잭션 버튼(배지·상태색 · [44 §5](44-transaction-log.md)) ✅ · **설정 창 콤보 저장 누락 결함**(모든 Choice 설정) ✅ · 키 자동 반복 = 편집·이동만(Ctrl+T 80개 결함) ✅  · **Demo 프로필·샘플 데이터**(최초 1회 팝업 · 도움말 메뉴 · [21 §5](21-connection-profiles.md)) ✅ · GUI 실행 인자 접속 ✅ · 키 자동 반복 필터 ✅ · 탭 우클릭 메뉴 ✅ · 선택 글자 세로 중앙 ✅ · **★ 트랜잭션 로그 창 T-107 ✅**(nsql-run `txlog` · `txlog_win.rs` · Tx 열 롤백/커밋/암묵/끊김) · **동일 출현 상자 스타일**(1px 선·여백 · 인접 행 선 공유 · 모양/선/배경 설정 4키 · 색 창을 임의 색 키에 · `#RRGGBBAA` 알파 우선) ✅ · 미니맵 선택/출현 색 구분 ✅ + 조사·추천 [46](46-minimap-features.md) 📐(T-110) · 미니맵 기본 폭 160(옛 기본값 표) ✅ · **★ 객체 인텔리센스 설계 [47](47-intellisense-metadata.md) 📐 확정(D-79~86 · 설정 48키 · 유니버설 메타 모델 §3-1 · T-57 재정의)** · **★ 실행 상태 카드**(`runtoast.rs` · 경과/단계/속도/■ · `run.toast*`) ✅ · 결과 영역 = 결과만(오류·메시지 → 로그 창 · 기본 형태 유지) ✅ · Ctrl+D 캐럿 방향 ✅ · 미니맵 뷰포트 회색/테두리 설정 ✅ · 실행 대기열 설계 [43 §8](43-fetch-model-and-result-tabs.md) 📐(T-112 최하위) · **★ 실행 로그 상세 계측+개발자 모드+저비용 게이트 [48](48-logging-architecture.md)** ✅ · Sublime 커서 규칙(단어/서브워드/스마트 Home/Ctrl+클릭) ✅ · 다중 커서 문장 실행 차단 ✅ · 열 선택 Sublime 규칙 ✅ · **★ Auto Indent [49](49-auto-indent.md)** ✅ · **★ 실행 중지 = 실제 취소(`CancelHandle` · SQLite/PG/Oracle · 툴바 ■) T-108 ①** ✅ · SQL Server 취소(Attention/소켓 종료 · `mssql.encrypt`/`mssql.cancel` · PRELOGIN 탐침) ✅ · **데이터 일관성 엄격 모드 [43 §9](43-fetch-model-and-result-tabs.md)** ✅ · 자율 배치 1(미니맵 1순위 · Ctrl+↑/↓ · 탭 ▶ · 트랜잭션 로그 우클릭) ✅ · 배치 2(Ctrl+M/Ctrl+Shift+M 괄호 · 주석 뒤 들여쓰기 제외 · 텍스트 변환 시간 로그) ✅ · 배치 3(**T-57 ① MetaStore** · `strict_all` · `devlog` feature) ✅ · 복사 끝 줄바꿈 제거 ✅ · **줄 변경 표시(기준선 디프 · 거터 띠 · 향상 모드 off)** ✅ · 플러그인 시스템 검토 [50](50-extension-system.md) 📐(D-87~90 · T-118) · **첫 플러그인 = 레인보우 괄호+괄호 이동 [51](51-rainbow-brackets.md)**(D-91~95 확정 · nexa-ctl `PairTable`/`BracketOpts`/자동 닫기/이동 ✅ · nexa-sql `extensions/` 호스트 API+`rainbow.*` 8키 ✅ · **배선 = T-119 🚧 Mac**) · VS Code/Sublime 확장 모델 비교 [50 §8](50-extension-system.md) · push a71d009 · 0981985(CI green) · **→ Mac 이관** · 실행 중지/배지 T-108 · 포터블 방향 D-78 · 다음 = 실기 21항목 → T-110 1순위 → T-103 잔여 → T-102

nsql-core `RowSource`/`ResultData`(Arc 세그먼트)/`View` · nsql-io 렌더러 제네릭 + `generate_src` · 복사 = 뷰 → 공용 렌더 · `grid.null_text`/`cli.null_text` · 텍스트 캐시 예산 포함. nexa-ctl `ToolDock`(그립 드래그 · 떼어 내기 · `DockLayout`) + `toolfloat.rs` · 설정 `toolbar.layout` · View ▸ 툴바 배치 초기화 · 자체 캡처 3장. **⏳ 실기 = journal 52차 ①~㊴**. → [journal](journal/2026-09-17.md)

## 2026-09-17 (51차 · mac · **세션 종료 → Windows 이관**) — T-48b 전체 조회 스트리밍 ✅ · 접속 창 단축키 ⌘⇧N · T-103 1차(데모 데이터 · 맥 캡처 스크립트 · 검수 결함 3건 기록) · push(CI green) · 다음 = T-103 잔여 → T-102 최적화

러너 `query_stream` + `fetch_all` 배치(5000) 진행률 · 워커 취소 깃발 · 도구줄 ■ · 푸터 "가져오는 중… n행 · MB". **⏳ 실기 = journal 51차 4항목**. → [journal](journal/2026-09-16.md)

## 2026-09-17 (50차 · mac) — T-81a 파일 검색 패널 ✅ · 텍스트 보기 선택/복사 ✅ · 텍스트 보기 스크롤 3건 ✅ · 전체 조회 일관성+예산 절단 ✅

⌘⇧F 패널(열린 탭 즉시 + 폴더 스트리밍 · Where · 결과 클릭 이동) · JSON/CSV 등 텍스트 보기 드래그/행번호 Ctrl 선택 복사 · 가로 최대 폭 실측(커지는 쪽만) · 추가 페치 뒤 위치 유지 · 자동 페치 중 전체 조회 수락 + 늦은 세그먼트 버림 · 전체 조회 예산 절단 안내 · 텍스트 보기 드래그 뒤 평 클릭 결함(히트 큐) + 거터 규약 = 그리드 · 탭 바 높이 점검(동일) · 전체 조회 34s→4.6s(`db.fetch_all_size`) · 예산 탭별 독립(기본 1024MB) · 찾기 버튼 활성 조건·우상단 10px · 파일 탭 강조 주황(`editor.tab_accent`) · 찾기 위젯(codicon 3종 · 셰브론 포커스 없음/크기 · 여백 10/10 · 우클릭 메뉴 라우팅/Esc/클립보드). clippy 0 · green. **⏳ 실기 = journal 50차 9항목**. → [journal](journal/2026-09-16.md)

## 2026-09-16 (49차 · mac · 일괄 배치 2 · 병렬 에이전트 5) — DR-30~32 ✅ · T-93 결과 다중 탭 ✅ · 찾기 위젯 VS Code 치수+Material ✅ · T-77 트랜잭션 UX 1차 ✅ · T-48a/d 서버 커서+CLI ✅ · T-90a/d 거버너+nexa-sys ✅ · T-97 미니맵 ✅ · T-81b 검색 엔진+nsql grep ✅ · T-72/T-62 배포 파이프라인 ✅ · 그리드 행 높이 ✅

사용자 질의 4개 답으로 범위 확정 → 병렬 5 + 직접. 결정 3건 DR 승격. GUI: 결과 탭(`results.rs` · Ctrl+\\ · 예산) · 찾기 위젯(419px · SVG 마스크 래스터 · 3상태 토글 · In selection · Preserve case · Alt+Enter) · 트랜잭션(배지 ●n · 팝업 · 툴바 · 잃는 순간 팝업 · 공유 세션 1차) · 미니맵/거버너 배선 · 행 높이 150%. 코어: 서버 커서(SQLite 세션 스레드 · Oracle · PG DECLARE · MSSQL 폴백 · Oracle e2e) · `perf.mode` 원장 24키 · `nexa-sys` · `nsql-search`(21k파일 0.3~0.6s) · 배포(맥 pkg/dmg 실기 · MSI/deb/rpm CI). 워크스페이스 테스트·clippy 전부 green · 커밋 nexa-ui 2 · nexa-sql 4(문서 포함 5) · **push 대기(사용자 요청 시)**. **⏳ 실기 대상 = journal 49차 6항목 + 48차 9항목**. **다음**: T-81a 검색 패널 · T-48b/c · T-90b/c · 서명 키 · PG 실서버 · T-54 탭별 세션. → [journal](journal/2026-09-16.md)

## 2026-09-16 (48차 · mac · 자율 배치) — 접속 창 결과 보존 ✅ · 트랙패드 스크롤 5단 수정 ✅(잔여 누적 · 픽셀 스크롤 · row 모드 표시 스냅 · 1:1+축 잠금 · CursorMoved 잔여 초기화 제거) · ⌘T 새 편집기 복구 ✅ · T-98 편집 명령 14종 ✅ · T-96 Goto Anything ✅ · T-89/T-73 잔여 ✅ · T-9/T-52 CLI ✅ · T-101 ✅

사용자 "진행 가능한 전체 작업 개발" + QA 5건. 편집기 = nexa-ui `EditCommand`(조각 편집 재매핑 · 되돌리기 1) + 키맵 2단 코드(`Ctrl+K, Ctrl+U`) + macOS `control+…` · `Ctrl+P` 탭/최근/`:줄` · Edit 메뉴 줄끝 3종 · 찾기 일치 전부 표시. 스크롤 = 입력 누적기(3px 양자화·축 잠금·픽셀 1:1) + 편집기 픽셀 스크롤 + `grid.scroll`/`editor.scroll` row = 표시 시점 스냅. CLI = SPOOL 포트 · `@@` 상대경로/인자 · `script.strict` · 셸 별칭(보조 에이전트). ⌘T는 36차 프리셋 정렬 때 유실된 것을 복구(회귀 테스트). 테스트 전부 green · clippy 0 · check-3os 뒤 push(두 저장소). **⏳ 사용자 실기 9항목 + ⌘T** = [journal 48차](journal/2026-09-16.md). **다음**: 실기 피드백 → T-93 결과 다중 탭(확인 대기) → T-48a → T-97 미니맵 → T-94. → [journal](journal/2026-09-16.md)

## 2026-09-16 · Windows 세션 요약(11~46차) — 다음 작업 진입점

- **끝난 것**: 로그 창 완성(스크롤·스위치·형식 어댑터·파일 싱크·필터·텍스트 선택·F10) · 토스트/오류 정규화(42) · 결과→SQL(41) · CLI 폭·형식·도움말 · **결과 도구줄+보기 모드+자동 페치(43 1차 · OFFSET 폴백)** · 지연 텍스트 변환 · 상태줄(선택·줄끝 3종·인코딩·git) · 편집기(거터·여백·선택어 외곽선·구분선·정규식 D-76·실행 뒤 캐럿) · 단축키 프리셋 · 글자 선명도 · 설정 창 종속 잠금 · 탭 메뉴 · 자원 거버넌스 설계(39).
- **글자 선명도(44~46차)**: Windows는 **GDI ClearType 글리프**(`ui.text_gdi` 켬 · 채널별 커버리지 · 진짜 볼드)로 Windows·Golden과 같은 래스터 — 검증 = nexa-font 단위 테스트 + `scripts/win-capture.ps1` 크롭. 결과 그리드 실데이터 실기 확인만 남음.
- **사용자 확인 대기**: 결과 다중 탭 구현 착수(D-73~75 · T-93) · 실기 점검(스크롤 끝 페치 600행↑ · 텍스트 보기 진척 카드 · 인코딩/줄끝/git 세그먼트 · alt+f3 · 글자 선명도).
- **후속 과제**: T-48a(서버 커서 유지) · T-94(데이터 편집기) · T-96(탭 검색 Ctrl+P) · T-97(미니맵) · T-98(Sublime 편집 명령) · T-90(거버너).
- **검증 도구(46차)**: `scripts/win-capture.ps1`(실기 캡처 크롭) · nexa-font `gdi_cleartype_stems_bold_and_advances`(CI windows) · `dump_gdi_glyphs`(글리프 ASCII 덤프).
- **문서(47차)**: [40 CLI 사용법](40-cli-usage.md) 신설 — 사용자·점검용 SSOT(설계는 11 · 인자 규약은 27). 옵션의 최종 근거는 `nsql <명령> --help`(`help.rs`).

## 2026-09-16 (47차 · win · 보조 세션) — CLI 사용 문서 40 신설 ✅ · 출력 형식 상세 ✅

- **[40 CLI 사용법](40-cli-usage.md)**(978줄) — 기존에 사용법 문서가 없었다(11 = 설계 · 27 = 조사). 모든 명령·출력을 릴리스 exe로 **실측**해 작성.
- **§2 순서대로 12단계**는 전부 SQLite로 되므로 **DB 서버 없이** 기능 점검이 끝난다.
- **§4 출력 형식** — 12종 별칭 · 실제 출력 비교 · SQL 5종 + 키 규칙 경고([41](41-sql-copy-key-rules.md)) · 표 4모드 · stdout/stderr 분리 · 종료 코드.
- **§1 접속** — 직접 지정을 프로필보다 앞에. 퍼센트 인코딩 · `NSQL_PASSWORD` · 필드 형식은 전 명령 공용 · **`@` 생략 불가**(실측).
- **미해결**: `--overflow` 도움말 기본값 오기(**T-101** · 실제 `none`).

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (46차 · win) — ★ GDI 경로 = ClearType + 진짜 볼드 ✅ · 닫기 깨짐 해결 ✅ · 캡처/단위 테스트 자동화 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (45차 · win) — nexa-ui CI 수정(GDI 이름 후보 = name 테이블) ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (44차 · win) — ★ 글자 선명도 = GDI 글리프 경로(`ui.text_gdi`) ✅ · 자체 캡처 비교 ✅ · D-77

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (43차 · win) — 글꼴 표시 원인 3가지(언어 · 소수 px · 상태줄 겹침) ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (42차 · win) — T-99 오토힌트 ✅ · 도구줄 글자 UI 글꼴 ✅ · i18n 전수 점검 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (41차 · win) — 글꼴 크기 pt/px ✅ · 설정 배치 ✅ · 강조선 제거 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (40차 · win) — 설정 창 입력란 붙여넣기·편집 메뉴·드래그 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (39차 · win) — 결과 글꼴 Calibri 13px ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (38차 · win) — push ✅(CI 3-OS green) · 툴팁 축약 · 아이콘 2종 · 텍스트 보기 행번호 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (37차 · win) — 결과 도구줄 22px ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (36차 · win) — 단축키 프리셋 3종 + Sublime 정렬 ✅ · 텍스트 보기 자동 페치 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (35차 · win) — 보기 모드 버튼 아이콘 + ▾ ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (34차 · win) — 정규식(D-76 · fancy-regex) 찾기/바꾸기 ✅ · 첫 글자 여백 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (33차 · win) — 줄끝 3종 ✅ · 인코딩 세그먼트 ✅ · git 세그먼트 ✅ · 툴바 버튼 표시 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (32차 · win) — 동일 출현 외곽선 ✅ · 구분선 설정 4종 ✅ · 정규식 엔진 D-76 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (31차 · win) — 글자 선명도(Segoe UI · 정수 스냅 · 대비 감마) ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (30차 · win) — 자동 페치 연속(limit+1) ✅ · 상태줄 선택 세그먼트 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (29차 · win) — 텍스트 보기 지연 변환(블록·스레드·진척 카드) ✅ · 툴팁 왼쪽 잘림 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (28차 · win) — 자동 페치 휠 경로 ✅ · 결과 글꼴 14 ✅ · 편집기 거터 띠 ✅ · 보기 메뉴 아이콘 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (27차 · win) — 결과 다중 탭 검토 ✅(D-73~75 · 구현 = T-93 대기)

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (26차 · win) — 실행 뒤 캐럿 옵션 + Alt+↓/↑ ✅ · 결과 글꼴 face/size ✅ · 헤더 더블클릭 자동 너비 ✅ · 탭 메뉴 ✅ · 탭 검색 추천안(T-96)

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (25차 · win) — 결과 도구줄 + 보기 모드 + 자동 추가 페치(T-48a/b 1차 · OFFSET 폴백) ✅ · 설정 창 종속 잠금 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (24차 · win) — 로그 창 글꼴/크기/푸터 ✅ · 텍스트 선택 + 자동 스크롤 ✅ · F10 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (23차 · win) — 페치 모델·결과 탭 설계 [43](43-fetch-model-and-result-tabs.md) ✅(D-68~72 · T-48a~d · T-93 · T-94)

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (22차 · win) — 로그 형식 어댑터 4종(형식·템플릿·파일 싱크·필터/컬럼) ✅ · 로그 창 우클릭 메뉴 ✅ · 푸터 Wrap/Sort/Scroll/Top + 툴팁 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (21차 · win) — 로그 창 스크롤/스위치 ✅ · 토스트 + 오류 정규화 42 ✅ · 덮어쓰기 카드 ✅ · 항상 위 ✅ · NULL 설정 ✅ · 그리드 스크롤/컬럼 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (20차 · win) — export sql:* 키 조회 ✅ · 실서버 3종 검증 ✅ · push

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (19차 · win) — 결과 → SQL 5종(CLI `-f sql:*` · GUI Copy SQL) ✅ · 키 규칙 + `sql.key_mode` ✅ · [41](41-sql-copy-key-rules.md)

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (18차 · win) — CLI 기본 none·markdown·copy ✅ · 그리드 3단 메뉴/우클릭 포커스 버그 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (17차 · win) — CLI 도움말 상세 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (16차 · win) — CLI 표 폭(설정·플래그·셸) ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (15차 · win) — CLI 비밀번호 프롬프트 ✅ · 편집기 드래그 지연 해소 ✅(프레임 6ms → 1.3ms) · `NSQL_TRACE_FRAMES`

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (14차 · win) — 탭 정지점 ✅ · `editor.tab_stops` ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (13차 · win) — 탭 ↔ 결과 그리드 쌍 ✅ · 플러그 제거 ✅ · 편집기 링 끔 ✅

→ [journal](journal/2026-09-16.md)

## 2026-09-16 (12차 · win) — 자원 거버넌스 설계 39 ✅(구현 T-90 · 결정 D-58~61 대기)

성능을 깎을 수 있는 기능 전부 = 부하원 원장 + 설정 키 + 수치 기준 + 자동 게이트. 구현은 T-90a(레지스트리 `Entry.perf` · `perf.mode` · 상태줄 ⚡)부터. → [journal](journal/2026-09-16.md)

## 2026-09-16 (11차 · win) — 최신화 ✅ · 즉시 접속 해제 ✅

Windows 빌드/테스트/CI 전부 green. Disconnect는 서버 상태와 무관하게 즉시(워커·탐색기 스레드 교체 · 갇힌 세션은 버림). T-88 실기 재검증은 사용자 캡처 대기. → [journal](journal/2026-09-16.md)

## 2026-09-16 (1~10차 · mac · 마지막 · Windows로 이관) — 맥 이관 ✅ · 맥↔Windows 차이 일괄 ✅ · 아이콘/스플리터/프로브 ✅ · 줄끝 정책 38 ✅ · push ✅

두 저장소 최신화 · Debug/Release 빌드 · GUI 기동. 접속 프로필 = `~/Library/Application Support/nexa-sql/`(Windows 프로필은 기기 키가 달라 재저장). 고친 것: HiDPI 논리/물리 px 혼용(로그인 버튼 · 툴바/탭 높이 · 메뉴 `set_scale` 누락) · Dock 아이콘 · 툴팁 층/다중 행 · 한글 IME 단축키 · 두부 폰트 · 가로 스크롤 클립/방향 · **글자 세로 정렬 = 잉크 기준**(nexa-ui `text_center_y`) · Material 아이콘 9종(사용자 SVG) · 스플리터 2개 · 접속 창 위치 규칙 · Test 문구 · 탭 문자 4 · 프로필 이름 변경 저장 · 목록 모드 결과 안내 · 신호등 대상 = 시도한 프로필 · macOS ping 단위. Linux는 CI 매트릭스로 확인(C 의존 크로스 clippy 불가). **미검증**: 세션 후반 캡처를 볼 수 없어 스플리터·아이콘·정렬은 빌드/테스트로만 확인(T-88) · **대기**: 우클릭 메뉴 아이콘 대상 메뉴(T-87). 줄끝 = 통합 로직([38](38-line-endings.md) · 상태줄 LF/CRLF · `file.eol_*`). 하위 메뉴 유실 방지 · SQL 복사 5종 테스트 · T-76 1차(settings.json 탭 편집). **Windows에서 이어서**: CI 확인 → T-88 재검증(잉크 정렬·스플리터·아이콘·메뉴 배율) → T-87(대상 메뉴 지정) → F-8 → T-81a → T-79 → D-53~57. 인수인계 메모 = [journal 9차](journal/2026-09-16.md). → [journal](journal/2026-09-16.md)

## 2026-09-15 (37차 · win · 마지막 · 맥으로 이관) — 자원 회수 점검 ✅(누수 아님)

대화상자 열고 닫기 5주기 = 핸들·GDI 평평 · 남는 것은 셸 일회성 초기화([37 §6-5](37-file-picker-performance.md)). 워커는 닫는 즉시 회수 · COM 짝 · 계측 도구 `scripts/memcycle.ps1`. 맥 이관 순서는 36차 메모 그대로. → [journal](journal/2026-09-15.md)

## 2026-09-15 (36차 · win · 마지막 · 맥으로 이관) — 메뉴 아이콘/토글 ✅ · 성능 37 P-1~P-4 ✅

오늘 36차까지 전부 push. 파일 대화상자는 탐색기급(자체 · 3-OS · 비동기) · 편집기 다중 선택 · 그리드 선택/복사 · 활동 막대 · 플로팅 찾기. **맥에서 이어서**: 빌드 확인 → F-8 NSWorkspace 아이콘 → T-81a 파일 검색 패널 → T-79 → F-3 가상화 → D-53~57. 인수인계 메모 = [journal 36차](journal/2026-09-15.md). → [journal](journal/2026-09-15.md)

## 2026-09-15 (34차 · win) — 파일 대화상자: 백그라운드 열거 ✅ · 빈 폴더 셰브론 억제 ✅ · 점 파일 토글 ✅ · 열 합 폭 선택 · 항목/빈 공간 메뉴 ✅

탐색기급 파일 대화상자 완성도 — UI 스레드는 디스크를 만지지 않는다(nexa-fs `lister` · 배치 · 프로브 · Drop 취소). **다음**: T-81a 파일 검색 패널(같은 `lister`/스레드 규칙) → F-3 가상화 → F-8 mac/Linux 아이콘 → T-79 EUC-KR → D-53~57. → [journal](journal/2026-09-15.md)

## 2026-09-15 (29차 · win) — 파일 대화상자: 경로 바(dir2) ✅ · 폴더 트리 ✅ · 결합 정렬/컬럼 이동·폭 ✅ · OS 아이콘 ✅

탐색기급 파일 대화상자 — ⌂←→↑ · 브레드크럼(클릭/편집) · `shell:`·`$env:`·`%V%` 별칭 · 장소 = 지연 로딩 폴더 트리 · OS 아이콘/종류 · Shift 결합 정렬 · 컬럼 이동/폭 · 인코딩. 잔여(F-3): PathBar/FileTree 독립 컨트롤 · 자동완성 · 가상화. **다음**: T-81a 파일 검색 패널 → F-8 mac/Linux 아이콘 → T-79 EUC-KR → D-53~57. → [journal](journal/2026-09-15.md)

## 2026-09-15 (24차 · win) — 파일 대화상자 OS 아이콘 ✅(비동기 · 캐시) · 설정 창 결함 2건 ✅ · 색 스와치 ✅

파일 열기/저장 = 탐색기와 같은 OS 아이콘·종류 이름(Windows · mac/Linux는 F-8 · 도착 전 자체 그림) · 콤보 위로 펼침. 설정 창: 컨트롤 하단 침범 · Pick 창 소유 · 스와치. **다음**: T-81a 파일 검색 패널 → F-8 macOS/Linux 아이콘 → T-79 EUC-KR → D-53~57. → [journal](journal/2026-09-15.md)

## 2026-09-15 (22차 · win) — 메뉴 아이콘/하위 메뉴 ✅ · 플로팅 찾기 ✅ · 활동 막대 ✅ · 인코딩 줄 ✅ · 파일 검색 설계 36 📐

좌측 = 활동 막대(탐색기 토글 · 접속 · 설정) + 패널. 우클릭 메뉴 = 아이콘·단축키·하위 메뉴(그리드 Advanced Copy ▸ SQL ▸). 찾기/바꾸기 = 편집기 위 플로팅(Aa · ab · n of m). 파일 열기/저장 하단 인코딩(자동/UTF-8/BOM/UTF-16). **다음**: T-81a 파일 검색 패널 → T-79 EUC-KR → D-53~57. → [journal](journal/2026-09-15.md)

## 2026-09-15 (21차 · win) — 글리프 캐시 ✅ · 그리드 선택 모델 ✅ · Advanced Copy ✅ · Run 줄 제거 ✅

텍스트 그리기 = 캐시 비트맵 블렌드(nexa-gfx) · 탐색기 아이콘 사전 스케일 · 행 캐시. 결과 그리드: 셀/범위/행 전체/Ctrl 개별/Shift 연속/키보드(dir2 규약) · 복사 = CSV·텍스트·Markdown·JSON·SQL(SELECT/INSERT/UPDATE/DELETE/MERGE). **다음**: 좌측 활동 막대(VS Code) → 플로팅 찾기/바꾸기 → 파일 대화상자 인코딩 줄 → D-53/54. → [journal](journal/2026-09-15.md)

## 2026-09-15 (20차 · win) — 파일 열기/저장/다른 이름으로 ✅(자체 대화상자) · 색 배치 조사 35 ✅

File ▸ Open…(Ctrl+O)/Save(Ctrl+S)/Save As…(Ctrl+⇧S)/최근 파일 · 툴바 아이콘 · 끌어놓기. 대화상자 = nexa-ui `nexa-dlg::FilePicker`(장소 · 목록 정렬 · 필터 · 숨김 · 새 폴더 · 덮어쓰기 2단) — Windows/macOS/Linux 동일. 탭 = 파일명 + `*` · 닫기 2단 · CRLF 보존. 조사 [35](35-editor-colors-reference.md). **다음**: 결과 그리드 선택 모델 → 좌측 활동 막대(VS Code) → 플로팅 찾기/바꾸기 → D-53/54. → [journal](journal/2026-09-15.md)

## 2026-09-15 (19차 · win) — Ctrl+D 다중 선택 ✅ · Alt+Shift 열 선택 ✅ · 탭별 들여쓰기 ✅ · 붙여넣기 탭→공백 ✅ · 드래그 결함 ✅

편집기: 다중 선택/캐럿 모델(nexa-ctl `EditState.extra`) · Ctrl+D 다음 출현 · Ctrl+⇧D 전부 · Alt+Shift 드래그 열 블록(행마다 캐럿) · 선택 행 줄번호 거터 강조 · Esc 접기 · 상태줄 `N selections`. 들여쓰기는 탭별(상태줄 팝업) · 설정은 기본값. 더블클릭 500ms 판정. **다음**: T-74 파일 열기/저장/다른 이름으로(자체 대화상자) → 35 색 배치 문서. → [journal](journal/2026-09-15.md)

## 2026-09-15 (18차 · win) — Tab Size 세그먼트(T-69 1차) ✅ · 로그 창 기본 꺼짐 ✅ · 드래그 선택 결함 2건 ✅ · 탐색기 아이콘 16종 ✅

상태줄 `Spaces: 4` 클릭 → 공백/탭 · 폭 1~8 · 변환 · `editor.tab_size/indent_spaces` · nexa-gfx 탭 폭 주입. 탐색기: 아이콘(`explorer.icons`) · 셰브론 90°·시각 중심 · 호버 페이드 · 로딩 애니메이션. 접속 창 모달(보조 창 포함). 설계 [34 트랜잭션 UX](34-transaction-ux.md). **다음**: T-69 잔여(문법/탭별) → T-77 트랜잭션 UX → T-74 파일 열기/저장 → T-72. → [journal](journal/2026-09-15.md)

## 2026-09-15 (15차 · win) — 설계 [34 트랜잭션 UX](34-transaction-ux.md) 📐

자동 커밋 기본 · 수동이면 탭 배지 `●n` + 상태줄 세그먼트(팝업) + 툴바 Commit 배지 · 잃는 순간만 모달 · T-77(T-54 탭별 세션 선행). ⏳ 사용자: D-51·52 승인.

## 2026-09-15 (10차 · win) — 설정 JSON 편집 ✅ · 설정 창 스플리터/세로 분할 ✅ · 탐색기 글꼴 ✅

Edit ▸ Edit settings as JSON… / 설정 창 [JSON 편집…] → `settings.json`(객체 계층) 외부 프로그램 · 저장 감시(1s) → 바뀐 키 즉시 반영. CLI `nsql config export-json/import-json`. `settings.json_editor` external(builtin = T-76). 설정 창: 왼쪽 열(검색+트리) | 스플리터(hover 페이드) | 카드. `explorer.font_size` 17. **다음**: T-74 파일 열기/저장 → T-76 내장 JSON 편집 → T-72. → [journal](journal/2026-09-15.md)

## 2026-09-15 (8차 · win) — 환경 설정 창 ✅(T-39 1차) · 접속 해제 툴바 ✅ · 메뉴 글꼴 설정 ✅ · 탐색기 셰브론 ✅

Edit ▸ Preferences…(Ctrl+,): 검색 · 트리(`CATEGORY_TREE`) · 카드(Switch/Combo/TextBox · 색 선택… · 단축키 캡처… · 초기화 · 고급) · 바꾸는 즉시 저장·반영. 툴바 Disconnect(접속 시만 활성). `ui.menu_font_size` 17. ⏳ 사용자 실기: 설정 창 배치·콤보·검색. **다음**: T-39 잔여(프로젝트 스코프 · 키맵 카테고리 그리드) → T-74 파일 열기/저장 → T-72. → [journal](journal/2026-09-15.md)

## 2026-09-15 (7차 · win) — 드라이버 분리 검토 → DR-29(정적 유지 · 확장은 in-process cdylib)

실측 기동 ≈20ms · 사설 8.5MB · 전역 초기화 0 → 분리 불필요. 확장 드라이버(다운로드·SxS)는 프로세스 대신 C ABI cdylib 지연 로드(T-27 개정). → [journal](journal/2026-09-15.md)

## 2026-09-15 (5차 · win · 야간 자율) — 찾기/바꾸기 바 ✅(T-73 1차) · 자동 재접속 ✅

Ctrl+F/Ctrl+H(mac ⌘F/⌥⌘F) · F3/Shift+F3 · Aa · Replace/All · 순환 · Esc. `connect.auto_reconnect`(기본 on) = 접속성 오류 뒤 다음 실행 전 재접속. **다음**: T-74 파일 열기/저장(nexa-ui 대화상자 선행) → T-72 배포 파이프라인 → T-59 정규식 → T-56/T-48 잔여. → [journal](journal/2026-09-15.md)

## 2026-09-15 (4차 · win · 야간 자율) — Oracle 라이브 로그 모니터 1차 ✅(T-71)

`oracle.live.source` session(기본)/table · 실행 중에만 `oracle.live.interval_ms`마다 메타 세션 폴링 · `[live]` 로그 줄 · 실행 끝 마지막 1회 · V$SESSION mechanism biscm 확인. 잔여 = CLI `--live` · 로그 창 Live 세그먼트 · DBMS_PIPE/LONGOPS 소스. → [journal](journal/2026-09-15.md)

## 2026-09-15 (3차 · win · 야간 자율) — 그리드 선택·복사 ✅ · 트랜잭션 ✅ · 실행 계획 ✅ · 서버 SET 통과 ✅

그리드: 셀/범위 선택 · Ctrl+C TSV · 우클릭 Copy/with headers/CSV/INSERT · Ctrl+A. 트랜잭션: `session.autocommit`(기본 on) · Run ▸ Commit/Rollback(Ctrl+Alt+C/R) · 상태줄 Auto-commit/Manual(●). 실행 계획: Run ▸ Explain(Ctrl+Shift+X) · `nsql explain -c t -q sql`(Oracle DBMS_XPLAN · MSSQL SHOWPLAN_TEXT · PG EXPLAIN · SQLite · MySQL). 결함 수정: `SET NOCOUNT ON` 등 서버 SET 문이 무시되던 것. 실서버 3종 확인. **다음**: T-71 Oracle 라이브 모니터 → 편집기 찾기/바꾸기 · 파일 열기/저장(nexa-ui 대화상자 필요) → T-72. → [journal](journal/2026-09-15.md)

## 2026-09-15 (2차 · win · 야간 자율) — 단축키 맵+캡처 창 ✅ · PostgreSQL ✅ · 카탈로그·`nsql cat`·DESC·컴파일 오류 ✅ · 탐색기 1차 ✅ · 페치 상한 ✅ · 실시간 서버 메시지 ✅ · 배포 설계 33 📐

드라이버 **4종**(Oracle·MSSQL·PostgreSQL·SQLite) · 키맵 = Sublime 기본(win/mac) + `key.*` 설정 + 캡처 창(View ▸ Keyboard Shortcuts… · Ctrl+Shift+E 탐색기) · `nsql cat`/탐색기 = 같은 `nsql-catalog`(스키마 → 종류 → 오브젝트 → 컬럼 · 소스 = `CREATE OR REPLACE`/`CREATE OR ALTER` 새 탭 → F5 재컴파일 → Oracle `ALL_ERRORS` 자동 보고) · `grid.max_rows` 200(더 있으면 상태줄) · PRINT/RAISERROR WITH NOWAIT·RAISE NOTICE **도착 즉시** 로그(Oracle DBMS_OUTPUT은 서버 제약 → T-71 폴링 모니터 설계 [32](32-server-messages-and-live-log.md)) · 배포 = 설치본만·목적별 exe·macOS `.app` Universal([33](33-distribution-and-packaging.md) · DR-27 · T-72). 실서버 자동 테스트 3서버 green · 워크스페이스 테스트·clippy green · Debug 실행 중 + Release 빌드(`target/release`). **다음**: T-71 Oracle 라이브 모니터 → T-72 배포 파이프라인(T-62 아이콘) → T-56 잔여(툴팁 카드 · 스레드 풀 · 자동 갱신 · nexa-grid 이식) → T-48 잔여("더 가져오기" 커서) → T-70 PG TLS → T-67 Command 레지스트리 통합 → T-69. ⏳ 사용자: 탐색기·단축키 창 실기 · docs/31 승인 · D-48~50. → [journal](journal/2026-09-15.md)

## 2026-09-15 (1차 · win) — 설계 [31 들여쓰기 설정 계층](31-indentation-settings.md) 📐 · 14차 push ✅ CI green

권장안 = 전역 → **문법**(`<Syntax>.nexa-settings`) → 프로젝트(예약) → 현재 탭(메모리·세션) · 상태바 `Tab Size: 4`/`Spaces: 4` + 팝업(공백/탭 · 폭 1~8 · 버퍼 감지 · 변환 · 문법/전역 기본으로 저장) · 선행 = nexa-gfx 탭 폭 4칸 고정 제거. **T-69** · ⏳ 사용자 승인 → 구현. **다음**: T-69 → T-65 WindowHost → T-67 Command 레지스트리. → [journal](journal/2026-09-15.md)

## 2026-09-14 (14차 · win) — 접속 창 완성도(신호등 정책 · 행 버튼 · 목록 그리드 · 팝업/포커스/hover UX · 색 설정 창 · 테스트 스레드) ✅ · push

**부하 검토(14차 끝)**: 트래픽 경로 5개 모두 상한 있음(주기 60s · 백오프 32분 상한 · 즉시 재확인 프로필당 1 · 테스트 클릭당 1 · 자동 재접속 없음) — 고칠 것 2건은 **T-63**으로 즉시 반영(대상 집합 밖 항목 재예약 금지 · 깨우기 1s) + Test/Connect **동시 상한 4 + FIFO 큐**(`connect.max_concurrent`) · 원칙은 [26 §8](26-performance-architecture.md) 표·체크리스트로 상시 관리(CLAUDE.md §3) · push 뒤 CI green · **구현 값 설정화**(비노출 13키 · `ConnTuning` · `nsql config list all`) · **[30 아키텍처 패턴 원장](30-architecture-patterns.md)**(확장점 = 포트+레지스트리+설정 · 부품 원장 · T-65~68) · 편집기 Tab 삽입. **다음**: T-65 WindowHost → T-67 Command 레지스트리 → T-31b 잔여(Import/Export·Pin·삭제 복구) → nexa-grid(U-3) → T-48 페치 모델 · T-57 인텔리전스 · T-39 설정 화면(T-64 색 선택기 연동). ⏳ 사용자: `biscm` 비밀번호 재입력 · 포커스 4항목·툴팁·페이드·색 설정 창 실기.

`ProbePolicy`(`probe.interval` 60 · `retry_delay` 60 · `max_retries` 5 · `timeout` 2) — 창 열려 있는 동안 주기 갱신 · 실패부터 횟수 누적 · **실패 간격 지수 증가**(60s→2→4→…×2⁵ 유지 · 빠른 재시도 없음) · Connect/Test 실패·실행 접속성 오류 시 그 서버 즉시 재프로브(창 닫혀 있어도) · 신호등이 초록이 아닌 서버에 쿼리 = TCP 빠른 판정 먼저(`ErrServerUnreachable`) · 워커 `catch_unwind`(세션 버림 · busy 해제) · 접속 창 행 아이콘 3열(신호등·Test 고리 결과색·Connect ▶) · 패널 상태는 작업 프로필 하나에 묶임(`panel_op`) · Connect 성공 = 활성 탭 적용(탭 0개면 새 탭) + 닫기 · 목록 헤더 폭 조절·정렬·결합 정렬(▲1/▼2)·오버레이 스크롤(nexa-ui `ScrollBars` **축별 표시** — 세로 휠엔 세로만) · 비밀번호 미저장 행 버튼 비활성 · 상단 버튼 = 라벨 실측 폭·간격 1/3 · Edit→상세 보기 · 컬럼 DnD(삽입선·고스트 · 끌면 정렬 안 함) · Details 토글 · 접속 창 열기는 끊지 않음 · 같은 서버 세션 유지(`connect.reconnect_same` off) · **포커스 링 ≤ 1 규칙**(`own_focus` · CLAUDE.md §3) · Shift+휠/←→ 가로 스크롤 · 툴바 아이콘 = 코드 마스크(글리프 0) · 행 접속 버튼 회색→파랑→초록(450ms 뒤 닫힘) · 아이콘 열/행 툴팁 · 호버 행 1s 페이드(`grid.hover_fade` · 결과 그리드·목록 공통) · 버튼 hover Fast·눌림(선택색+어둡게+1px)·마지막 눌린 버튼만 테두리(창 전체 단일 포커스 소유) · **hover = `IntentFade`**(의도 코얼레싱 · 행·콤보 항목 공통) · `FadeSpeed{Fast,Slow}` ↔ `ui.fade_fast` 500 / `ui.fade_slow` 1000 · 초록 화살표 = 현재 접속만 · 버튼 간격 pad/2 · Delete 2단 확인(빨간 타이머 5s · 재클릭/Enter 삭제 · Esc/만료 해제) · 우클릭 Duplicate(`_Copied`) · 프로브 = 필요한 대상만 각각 스레드(상한 16 · DB 워커 분리 · `probe.interval` 60s당 1회) · 접속/로그 창 = 메인의 소유 창(작업표시줄 1항목 · 항상 메인 위 · 다중 인스턴스 그대로) · **색 설정 창**(View ▸ 색 설정… · `ColorPanel` 투명도·프리셋·최근 · hover/눌림 색 → `ui.hover_color`/`ui.pressed_color` 즉시 반영) · 스크롤 반전 `input.scroll_natural` · 폼 Port 58/패널 292/라벨 +3 · 상단·폼 버튼 동일 폭 · 목록 유효 폭 + 빈 영역 메뉴(New) · 필터/폼 입력란 드래그 선택·클립보드·우클릭 편집 메뉴(모달 · 최상위) · 콤보 우클릭 복사 메뉴 · 텍스트박스 hover(회색 Slow) · 상태 메시지 워드랩+세로 스크롤 · 상단 버튼 폭 = 최장 라벨 ×1.32 고정 · 무장 Delete = "Delete" + 게이지만(잔여 초 숨김 옵션) · Details/Delete는 선택 있을 때만 활성 · 삭제 뒤 인접 항목 자동 선택(빈 목록 = New) · 빈 영역 클릭 = 선택 해제 · Save는 워커를 거치지 않고 즉시(접속 검증 없음) · Password 열(저장 진한 체크 / 세션 입력 연한 체크) · 세션 비밀번호 보관(프로그램 실행 동안) · 접속 테스트 = 요청당 스레드(병렬 · busy 무관 · 같은 프로필은 끝날 때까지 Test/Connect 잠금). ⏳ 사용자: `biscm` 비밀번호 재입력(삭제 복원). ⏳ 사용자 실기: 포커스 4항목 · 가로 스크롤 · 툴팁/페이드. **다음**: T-31b · T-54(탭별 세션 = 영향도 분리 나머지) · nexa-grid(U-3) · T-48. GUI 실행 중. → [journal](journal/2026-09-14.md)

## 2026-09-14 (13차 · win) — Golden식 접속 창(T-31) ✅ · 앱 아이콘(Union) ✅

별도 창(640×520 · 기본 목록만 · New/Edit 시 창이 오른쪽으로 커지며 폼 슬라이딩 · 접속 명칭 맨 위 · 더블클릭 접속 · 삭제) · **서버 신호등**(`probe.rs` · 한 번 이상 접속한 프로필만 · 지수 재시도 상한) · Ctrl+L/툴바/메뉴 · 부팅 시 열림 · 접속 시 닫힘 · 메인 창 본문 전폭 · 그리드 컬럼 폭 조절(커서 피드백) · 아이콘은 코드로 그림(정적 자원 0 · 풀블리드). 아이콘 = `packaging/branding/icon.svg` SSOT → PNG/ico/icns/rgba(`scripts/pack_icon.py`) → 3창 런타임 아이콘(작업표시줄 실기 ✅). **다음**: T-31b(Import/Export·Pin·정렬) → nexa-grid(U-3) → T-48 페치 모델 · T-57 인텔리전스. GUI 실행 중. → [journal](journal/2026-09-14.md)

## 2026-09-14 (12차 · win) — 탭 ✅ · 그리드 정렬/이동 ✅ · 구문 강조·팔레트·서식 복사·상태줄 ✅ · 창 포커스 ✅ · 설계 29(인텔리전스·다중 커서)

편집기 탭(TabBar · 툴팁 카드) · 그리드 컬럼 드래그/정렬(결합 · 인덱스 벡터) · `window.focus` · 연결 블록 선택 · **구문 강조**(nexa-ctl `highlight` · `.nexa-syntax` 플러그인 `Packages/` · 탭별 · 확장자 기본) · **명령 팔레트**(Ctrl+⇧P · `Set Syntax`) · 서식 있는 복사(CF_HTML) · 상태줄 세그먼트(접속·Ln/Col·rows·time·구문 클릭) · 세로 안내선(80)·공백 표시 설정. 설계 [29](29-editor-syntax-palette-statusbar.md): 상태줄 표 · 인텔리전스(끄면 비용 0 · alias→컬럼 · 방언 내장 함수) · 다중 커서/정규식. **다음**: T-31 Golden식 접속 창 · T-57. GUI 실행 중. → [journal](journal/2026-09-14.md)

## 2026-09-14 (10차 · win) — 스크롤바·픽셀 스크롤 ✅ · 메뉴바·툴바 ✅ · 줄번호/행번호 ✅ · 탭 이식 🚧

nexa-ctl `ScrollBars`를 그리드·로그 창에(필요 시만 · 호버 두껍게 · 자동 숨김) · 픽셀 스크롤 + `grid.scroll` 설정 · MenuBar/Toolbar 배선(`ToolIcon::Glyph`) · TextBox 줄번호 거터 · 그리드 행번호 열 · 설정 4개(`editor.line_numbers` `grid.row_numbers` `tabs.rows` `tabs.tooltip`). **진행 중**: dir2 TabBar → nexa-ctl 이식(에이전트) → 편집기 탭·툴팁. GUI 실행 중. → [journal](journal/2026-09-14.md)

## 2026-09-14 (9차 · win) — ★ 실행 로그 창·CLI·어댑터·독립 I/O 스레드 ✅ · session.mode 설정 · CONNECT 동사 검토

**구현**: `nsql-log`(LogEntry 단일 입력 · LogFormat 단일 출력 · Raw/Markdown/Grid · LogBuffer · **LogHub**(비차단·별도 스레드·싱크 패닉 격리) · StderrSink) · `nsql_run::log_entries` · GUI 로그 창(메인 창 옆 · Ctrl/⌘+⇧G · 스크롤·follow) · CLI `--log` · 설정 `log.format` / `session.mode`. 파일 싱크는 후속(같은 트레이트 · 배치 flush).
**검토**: CONNECT = 클라이언트 명령(SQL*Plus도) → 권장 ⓐ 우리 명령 하나 + 별칭 + 명시([27 §6](27-cli-conventions.md)) → **D-45**. **⏳ 사용자**: D-41~45 · nexa-ui D-4~D-10. → [journal](journal/2026-09-14.md)

## 2026-09-14 (8차 · win) — 편집기 기본 기능 ✅: 복사/잘라내기/붙여넣기 · 휠 스크롤 라우팅

OS 클립보드 3-OS(`clipboard.rs` · 외부 crate 0) · Ctrl/⌘+C/X/V · 우클릭 메뉴 · 휠은 커서 아래 영역으로 · 멀티라인 붙여넣기 탭 보존(nexa-ui f56db89). T-16의 "☐ 클립보드" 닫힘. GUI 실행 중. → [journal](journal/2026-09-14.md)

## 2026-09-14 (7차 · win) — Ctrl+Enter 한 문장 실행 ✅ · 셀 컨트롤 설계(nexa-ui 21 §3-2)

`statement_at`(nsql-script · `;` 종결 · GUI/CLI 공용) → Ctrl/⌘+Enter = 선택 → 캐럿 문장 → F5 전체. 그리드 셀의 TextEditor·Button·Checkbox·Image·ImageButton은 `CellKind` + 정적 페인터 + one live editor로 설계(G-2/G-2b · 엔진 이식 G-1 뒤). GUI 실행 중(PID 10004). → [journal](journal/2026-09-14.md)

## 2026-09-14 (6차 · win) — ★ 접속 설정→연결→상태 확인 구현 · 성능 계측 골격 · CLI 규약·성능·그리드 계열 설계

**요청**(사용자): 접속 흐름부터(DB 종류·정보 → 연결 → 상태) · 테스트 실행 · CLI 동일·핵심 모듈화 · CLI 인자 조사 · 성능 단계별 계측·경량 구조·호평 앱 조사 · 접속 목록 그리드(dir2 차용) · 그리드 계열 골격 공유.
**구현**: 공용 층(`ConnectSpec::from_parts` · `test_connection` · `Dialect::default_port`) · CLI `conn add/test` 필드 옵션 · GUI 접속 패널(프로필·DB 종류·호스트/포트·DB·사용자·비밀번호·저장·Test/Connect/Save·● 상태) · 계측 `Stage/Timeline`(Oracle 3단계 분리 · `--timing` · 그리드 푸터 load/render/~bytes). clippy 0 · green · **GUI 실행 중(테스트용)**.
**설계**: [26](26-performance-architecture.md) · [27](27-cli-conventions.md) · [nexa-ui 21](../../nexa-ui/docs/21-grid-family.md). **⏳ 사용자**: D-41(CLI 짧은 옵션 진영★) · D-42(결과 상한) · D-43(페치) · D-44(로그) · nexa-ui D-10(그리드 계열 크레이트). **다음**: T-50/U-3 nexa-grid(dir2 rows 이식 · 결과·접속 그리드) → T-31 로그인 리스트 · T-48 페치 모델. → [journal](journal/2026-09-14.md)

## 2026-09-14 (5차 · win) — UI 방향: 3-OS 동일 UI · 파일 관리·파일 대화상자 계층 설계(nexa-ui docs/20)

**요청**(사용자): OS별 차이 없는 동일 UI · 파일 관리·파일 Dialog 대폭 개선 · nexa-ui 위에 계층 구조로 확장. **설계는 nexa-ui에**([docs/20](../../nexa-ui/docs/20-file-management-and-dialogs.md) · 75634f6): 네이티브 대화상자 0 · `nexa-fs`(dir2 std 전용 코드 추출) → 파일 컨트롤 6종 + `Overlay` → `nexa-dlg` FilePicker → 앱. nexa-sql 몫 = F-6 배선(열기/저장/프로젝트 폴더/export/DroppedFile/최근). ⚠️ beep ADR-0014(네이티브 대화상자)와 충돌 → nexa-ui **D-9**. **⏳ 사용자**: nexa-ui D-4(모달)·D-5(별도 크레이트)·D-6(아이콘)·D-7(휴지통)·D-8(grid 공유)·D-9(beep 정정) + 라이선스 D-23·24·40. → [journal](journal/2026-09-14.md)

## 2026-09-14 (4차 · win) — DR-26(티어·서버 운영 확정) · `nexa-license` 공개 저장소 생성 · 골격

**요청**(사용자): D-32~39 추천대로 · 라이브러리는 공개가 적합하면 공개. → **DR-26** 기록 · `SosomLab/nexa-license` **PUBLIC** 생성 · `../nexa-license` clone · 골격 커밋·push(워크스페이스 · `format`/`base32`/`Product` · 12 테스트 · 3-OS CI · PolyForm NC · **검증 전용 — 서명·키 생성은 비공개 서버 저장소에만**(사용자 우려 반영)). 질문 3건 답 = [25 §11](25-license-tiers-and-server.md): 장치 vs 사용자 인증 차이표 · 백업/복원 가능 + 시작 시 자동 복원(T-46) · Device는 기기 ID로 다른 PC 거부. **⏳ 사용자**: D-23(게이트 목록) · D-24(기기 묶음 원천) · **D-40**(서명 포트 ed25519/p256 — dir2 포함 여부). **다음**: D-40 답 → `verify`(SigVerifier 포트)·`machine-id`·`fs`·`protocol` → nexa-sql `nsql-license` 얇은 층. → [journal](journal/2026-09-14.md)

## 2026-09-14 (3차 · win) — 라이선스 모듈 범용성 점검(beep·clip·dir2) → 수정안 7건 · D-40

**요청**(사용자): 라이선스 모듈이 beep·clip·dir 등에 범용인지 확인. **결론**: 골격(형식·체인·기기 ID·서버)은 범용, 초안에 nexa-sql 전용 가정 7곳 → [25 §10](25-license-tiers-and-server.md)에 수정안. 핵심 걸림돌 = **dir2 외부 crate 0(B3)** → 서명을 포트로(`alg=ed25519|p256` · dir2는 CNG 어댑터 · 루트 키 2개) = **D-40**. 그 외 `Product` 기술자 · 계열 공통 기기 코드 · 경로 주입 + 자체 파서(vendored nexa-conf 충돌 회피) · 문자열 0 · 다제품 서버. **⏳ 사용자**: D-40 + 기존 D-23·24·32·33·34·39. → [journal](journal/2026-09-14.md)

## 2026-09-14 (2차 · win) — 설계: 라이선스 종류 4단 · 사내 인증 서버 · 공유 라이브러리/비공개 서버 저장소 분리(DR-25)

**요청**(사용자): Device/User(5대)/소규모 조직(1~5)/조직(6+) 구분의 타당성을 경쟁 제품·최근 정책과 교차 조사 · 조직용 사내 인증 서버(데몬) 설계 포함 → 서버는 별도 비공개 저장소 · 공유 기능은 라이브러리로.
**결과** [25](25-license-tiers-and-server.md): 4단 타당(사용자 단위가 시장 표준 · 기기 단위는 TablePlus뿐 · 조직 서버 실물은 JetBrains License Vault). 조정 = Team은 서버 없이 파일 묶음 기본 · Org는 named 좌석 + concurrent 옵션. 서버 = 3단 Ed25519 체인(루트→조직 라이선스→리스) · 오프라인 검증 · TTL 7일 · HTTP+key=value · SQLite · 관리 페이지. **DR-25**: `SosomLab/nexa-license`(공유 · 형제 path 의존) ← `nsql-license`(앱) · `SosomLab/nexa-license-server`(비공개 · 서버+발급기).
**⏳ 사용자 결정**: D-32(Team 서버★) · D-33(좌석 모드★) · D-34(가격★) · D-35~38 · **D-39 라이브러리 공개 여부**(공개 권장 — CI 토큰 불요) + 1차의 D-23·24. **다음**: 답 오면 T-45 라이브러리 저장소 생성부터. → [journal](journal/2026-09-14.md)

## 2026-09-14 (1차 · win) — ★ 정품 인증 설계(23) · i18n·테마 구현(T-37/38 ✅) · VS Code 설정 분석(24)

**요청**(사용자 3건): 로컬 PC 인증 + 라이선스 파일 + 기능 게이트 설계 · i18n(기본 영어)·테마(System/Light/Dark · 기본 System) 구현 · VS Code 설정 방식(소스)·설정 UI(캡처 8장) 분석.
**설계** [23](23-license-activation.md): 요청 코드(OS 기기 ID 해시) → 오프라인 발급기(Ed25519 비밀키) → `nexa-sql.license`(key=value + 서명) → `nsql license install` → 실행 시 로컬 검증 · `nsql-license::check(Feature)` 1함수 · 게이트는 UI/CLI 진입점 1곳 · Core는 모른다. **⏳ 사용자 결정 D-23(게이트 목록★)·D-24(기기 묶음★)·D-25(모델★)·D-26~31**([10 §3](10-decision-record.md)). 코드는 T-32~36.
**구현**: `nsql-i18n`(`Lang/Msg/tr` · en 기본 · ko 폴백) · `nsql-settings`(레지스트리 단일 원천 · `settings.conf` 변경분만 · `ThemeMode`) · `nsql config list/get/set/reset/path` · GUI 전 문자열 카탈로그 · `theme.rs`(Windows 레지스트리·macOS·Linux 판정 + winit) · `Ctrl/⌘+⇧T` 테마 순환 · `Ctrl/⌘+⇧L` 언어 전환(즉시 저장·반영). 84 테스트 · clippy 0 · CLI 실기 ✓ · GUI System 모드 = OS 라이트 추종 캡처 ✓.
**분석** [24](24-settings-and-vscode-analysis.md): 레지스트리→변경분 파일→화면 생성 원리 · TOC 글롭 · type별 렌더러 · 캡처 6원칙 → T-39 설정 화면 설계.
**다음**: D-23~25 답 → T-32 · ☐ CLI 나머지 문자열 카탈로그화 · ☐ mac/Linux 테마 실기. → [journal](journal/2026-09-14.md)

## 2026-09-13 (9차 · win) — 정리 · 진행사항 최신화 · push

**요청**(사용자): *"내용 정리 후 진행사항 최신화 수행하고 commit 및 main 병합한 뒤 push"*. 브랜치는 main 하나(병합 대상 없음 — 원격 병합은 직전 차수). CLAUDE.md 현 단계 갱신. push 8커밋(5차~9차 + 병합 · ad7e0c9) → **CI green**: `ci` run 34760377408(windows·macos·ubuntu) · `integration` run 34760377376(Oracle 23ai · SQL Server 2022 실서버 5/5) 모두 success. **다음**: T-27 RPC 드라이버 프로토콜 착수. → [journal](journal/2026-09-13.md)

## 2026-09-13 (병합 · win) — origin/main(mac 3·4차) 병합 · win 기록 5~8차 재번호 · 병합본 검증 ✓

원격 mac 3커밋(`NSQL_ORACLE_CLIENT_DIR` · macOS Instant Client 설치) + 로컬 win 5커밋 병합(70709df). 병합본: 71 테스트 · clippy 0 · 19c 실접속 OK · `NSQL_ORACLE_CLIENT_DIR` Windows 동작 확인(잘못된 폴더 → DPI-1047). **미push 7커밋** — push는 사용자 요청 시. → [journal](journal/2026-09-13.md)

## 2026-09-13 (8차 · win) — ★ GUI 창 실기 통과(프로필 접속 → 쿼리 → 그리드) · 무한 재그리기 루프 수정

**요청**(사용자): *"프로그램 실행해줘"*. 창을 띄워 PowerShell로 직접 구동·캡처: 프로필 `biscm` 접속 ✓ · 편집기 입력 ✓ · F5 → 그리드 `BISCM · 22:13:33 · 442` ✓.
**결함 수정**: `about_to_wait`의 무조건 `request_redraw`가 **그리기 무한 루프**(유휴 CPU 100% · 키 입력 3초 지연 · 옛 결과 잔존)를 만들었다 → 타이머 만료 시에만 redraw. 유휴 CPU 13%(전체 창 래스터 65ms/프레임 · 더티 영역은 후속).
**⏳ 사용자 실기**: 한글 IME · 창 둘 프로필 공유. → [journal](journal/2026-09-13.md)

## 2026-09-13 (7차 · win) — ★ 사용자 Oracle 19c 실서버 실기 통과 · 결함 1건 수정 · thin 스파이크

**요청**(사용자): 19c 접속 정보 제공 · *"개발 가능한 부분은 먼저 정리해서 진행"* · Rust 클라이언트 질문 · 다른 DBMS 확인.
**결과**: 프로필 저장 → `conn test` OK 1.24s · `it-oracle.sql` 전부 통과. 결함 `EXEC :V := (SELECT …)`(PLS-00103) 수정(41acc2a). **thin `oracledb` 스파이크: Instant Client 없이 157ms 접속 ✓ — 그러나 OUT 바인드·REF CURSOR API 부재 → 주 드라이버 불가(D-22 교체 조건).** 다른 DBMS는 이 기기에 서버 없음 → MSSQL·PG 접속 정보 대기(PG는 드라이버 M4).
**다음**: T-27 RPC 프로토콜(로컬로 진행 가능) → T-28 → T-29(D-19) → T-31. → [journal](journal/2026-09-13.md)

## 2026-09-13 (6차 · win) — 설계: 드라이버 확장(GitHub 최신 다운로드 · SxS · 관리자) · DBeaver식 접속 대화상자 (DR-23·24)

**요청**(사용자): DBeaver 형태 접속 설정(Golden = 로그인 리스트) · 드라이버는 GitHub에서 최신 다운로드해 확장으로 · DBMS 버전별 하위 호환 확인 → SxS 다중 버전 · 목록·삭제.
**조사 결론**: Oracle은 실제로 필요(23ai ← 19c/21c/23ai · 구형 서버는 19c 클라이언트까지 · ODPI-C 프로세스당 클라이언트 1개) · ODBC 벤더 클라이언트도 필요 · MSSQL은 레거시 TLS 변종 1개 · PG/MySQL/SQLite 불요.
**설계**: 확장 = stdio JSON-RPC 프로세스(내장 순수 Rust 드라이버는 유지) · `drivers/<id>/<ver>/` SxS · 프로필 `driver=<id>@<ver>` · 최신 non-prerelease 기본 · sha256+Ed25519 필수 · 삭제는 참조 프로필/열린 세션 보호. → [22](22-driver-extensions.md) · DR-23·24 · T-27~T-31(순서 고정). **코드 변경 없음.** **다음**: T-27 프로토콜부터(D-15 답과 함께). → [journal](journal/2026-09-13.md)

## 2026-09-13 (5차 · win) — ★ 연결 프로필 저장소 `nsql-vault` · `nsql conn` · GUI Save (DR-22)

**요청**(사용자): 저장소 최신화·변경 분석 → *"사용자 폴더에 접속 정보를 암호화해서 저장 · 연결 시 재사용"* → *"몇 개의 Instance든 저장된 암호를 함께 사용"*. nexa-ui는 `git@kiros33.github.com:SosomLab/nexa-ui.git`로 clone(이 기기에 없었음 → 빌드 복구).
**산출**: `nsql-vault`(비밀번호만 ChaCha20-Poly1305 봉투 · 도메인 = 프로필 이름 · 기기 키 Windows DPAPI/그 외 0600 · 동시 첫 실행에도 단일 키) · `nsql conn list|add|show|rm|test|path` · `-c <이름>` · 스크립트 `CONNECT <이름>`(Runner Resolver) · GUI 이름 칸 + Save · 숨김 비밀번호 입력. **71 테스트 green**(+14) · clippy 0 · CLI 실기 ✓(저장 파일에 평문 없음).
**결정**: **DR-22**(파일 저장소 + 봉투 · D-2 닫힘) · D-18(mac Keychain·Linux Secret Service 후속). 원장 +3(clip 동일 판).
**⏳ 사용자 실기**: GUI Save → 이름으로 Connect · 창 둘 공유. **다음**: 사용자 후속 방향(DBeaver식 접속 대화상자 · GitHub 최신 드라이버 다운로드 · SxS 다중 버전·삭제) 설계 → [22](22-driver-extensions.md). → [journal](journal/2026-09-13.md)

## 2026-09-13 (4차 · mac) — Oracle Instant Client macOS 설치 실기 · 설치 스크립트 · 정리·push

**요청**(사용자): mac 설치 방법 → 전 패키지 한 폴더(CLI 포함) → 폴더 위치·이름 추천 → 정리·push.
**산출**: `scripts/install-instantclient-mac.sh`(아키텍처 감지 · Intel DMG/ARM64 ZIP · 6패키지 · 격리 해제 · `~/lib`·버전 링크 · `--rc`) · [20 §5](20-testing-codespaces.md) 전면 개편(폴더 추천 · URL 표 · PATH/TNS_ADMIN/NLS_LANG · 연결 3형식 · 한글) · 어댑터 `NSQL_ORACLE_CLIENT_DIR`·DPI-1047 안내.
**실측**: 이 Mac에 실제 설치 — 255MB · sqlplus 19.16 · `nsql run`이 ORA-12541까지 도달(클라이언트 로드 ✓). **폴더 규칙 확정**: `~/Oracle/instantclient_<major>_<minor>` + 버전 없는 링크 `~/Oracle/instantclient`.
**⏳ 남음**: 사내 Oracle 실접속(사용자 네트워크) · GUI 창 실기 · D-15~D-17 답.
→ [journal](journal/2026-09-13.md)

## 2026-09-13 (3차 · mac) — Instant Client macOS 안내(docs/20 §5) · `NSQL_ORACLE_CLIENT_DIR` · DPI-1047 힌트

**요청**(사용자): Instant Client mac 설치·설정 방법. Intel Mac → 19.16 · `~/lib` 심볼릭 링크 권장 · 어댑터에 lib dir 환경변수 + 로드 실패 안내 추가. → [journal](journal/2026-09-13.md)

## 2026-09-13 (2차 · mac) — 정리 · 진행사항 최신화 · push

**요청**(사용자): 정리 → commit → main 병합 → push. 브랜치는 main 하나. journal 09-13 파일 분리. 원격 `SosomLab/nexa-sql`·`nexa-ui` 모두 최신 · CI green. **다음**: D-15~D-17 답 대기 → 기본안 = nexa-edit E1(로프 버퍼) 착수([17](17-editor-incremental-plan.md)). → [journal](journal/2026-09-13.md)

## 2026-09-13 (1차 · mac) — Codespaces 최소 사양(2코어·8GB)

**요청**(사용자): *"codespaces 서버는 최소 사양으로 · 느려도 괜찮다"* → `hostRequirements` 2/8GB/32GB · DB 메모리 상한(oracle 2g · mssql 1.5g) · 빌드 병렬 1 · DB 대기 비차단. [20](20-testing-codespaces.md) 갱신. → [journal](journal/2026-09-13.md)

## 2026-09-12 (5차 · mac) — ★ Oracle · SQL Server 실서버 검증 통과 (integration 워크플로)

**결과**: `integration` run 34700220848 — 통합 테스트 **5/5 green**(Oracle: 세션 변수 왕복 · REF CURSOR PRINT · DBMS_OUTPUT · PL/SQL 블록 OUT · DML / MSSQL: SELECT INTO 재작성·OUT 회수 · `#temp`·`GO` 배치·한글 NVARCHAR / SQLite). `nsql run examples/it-oracle.sql`이 실서버에서 블록 EXEC→PRINT→REFCURSOR→DBMS_OUTPUT까지 그대로 동작.
**실서버가 잡아낸 결함 3건(수정)**: ① tiberius의 `query/execute`는 항상 `sp_executesql`이라 그 안의 `CREATE TABLE #t`가 소멸 → 파라미터 없는 DDL·세션 문장은 SQL 배치(`simple_query`)로 라우팅 ② `SELECT TOP 1 … INTO :V` 재작성이 `TOP`을 대입 뒤로 보냄 → 접두(TOP/DISTINCT) 보존 ③ CLI가 URL의 `%40`을 디코드하지 않아 sa 로그인 실패 → `scheme://` 형식만 퍼센트 디코드.
**닫힘**: T-4·T-5(실서버 검증) · D-14. **남은 미검증**: GUI 창 실기(사용자) · Instant Client 배포 방식(D-1).
→ [journal/2026-09-12](journal/2026-09-12.md) · [20](20-testing-codespaces.md)

## 2026-09-12 (4차 · mac) — ★ 실서버 테스트 구성: Codespaces devcontainer · DBMS별 Docker · Actions 통합 워크플로 (DR-21)

**요청**(사용자): *"codespaces를 사용한 테스트 구성"* · *"각 DBMS별 docker 방식으로"* → D-14 해소.
**산출**: `.devcontainer/`(compose: oracle · mssql · postgres · mysql 각 컨테이너 · Rust+Instant Client+한글 폰트 이미지 · nexa-ui 자동 clone) · `scripts/it.sh`·`wait-for-db.sh` · **통합 테스트 5건**(`nsql-drivers/tests/integration.rs` · 환경변수 게이트 · REFCURSOR·DBMS_OUTPUT·PL/SQL OUT·MSSQL SELECT INTO·GO 배치) · `examples/it-oracle.sql`·`it-mssql.sql` · `.github/workflows/integration.yml`(서비스 컨테이너 + Instant Client 설치 + `nsql run` 실기). `VarType::Auto`의 T-SQL 선언을 `NVARCHAR(4000)`로(SQL_VARIANT 회수 불안정).
**실측(로컬)**: 워크스페이스 56 테스트 green · 통합 테스트는 환경변수 없어 [skip]. **원격 검증**: push 후 `integration` 워크플로 결과를 journal에 기록(첫 실행은 이미지 pull·Oracle 기동으로 10분 내외 예상).
→ [journal/2026-09-12](journal/2026-09-12.md) · [20](20-testing-codespaces.md)

## 2026-09-12 (3차 · mac) — 정리 · 진행사항 최신화 · main push

**요청**(사용자): *"내용 정리 후 진행사항 최신화 수행하고 commit 및 main 병합한 뒤 push"*. 브랜치는 main 하나(병합 대상 없음).
**정리**: 열린 질문 4건을 D-14~D-17로 등재([10 §3](10-decision-record.md)) — 실서버 검증 방식 · 다음 우선순위 · 실기 OS · 캐시 위치. 사용자 답이 오면 DR로 승격하고 그 순서로 착수.
**push**: `SosomLab/nexa-sql`(첫 push) · `SosomLab/nexa-ui`(신규 생성) — CI 첫 실행 빨강 2건 수정 후 **양쪽 3-OS green**.
**다음 후보(답 전 기본안)**: D-15 권장 순서 = nexa-edit E1(로프 버퍼·Transaction·History) → 세션 hot exit → 비교 뷰. D-14 권장 = Docker 로컬 서버.
→ [journal/2026-09-12](journal/2026-09-12.md)

## 2026-09-12 (2차 · mac) — ★ M1+M2 병행 착수: 드라이버 3종 · CLI run/shell/export · 최소 GUI · 폰트 · 설계 문서 6건

**요청**(사용자): 서명은 별도 요청 시 · **나머지 DP 전부 확정 → 한꺼번에 개발** · GUI 최소 기능을 CLI와 병행 · 한글·고정폭 폰트 · 영역별 폰트 · 구문강조/포매터 등 패키지식 모듈 · Sublime 구성 학습 · 외부 파일 변경 처리 조사 · 편집기 점증 계획 · 미저장 버퍼 복원(hot exit)·프로젝트 · 성능 원칙(요청 병합·인덱스·미니맵) · 비교/git/오브젝트 시점 캐시.
**결정**: DP-1~9 → **DR-10~18 승격** · DR-19 한글·고정폭 1급 · DR-20 서명 보류([10](10-decision-record.md)).
**코드**(전부 fmt·clippy `-D warnings`·테스트 green — 워크스페이스 51 테스트):
- `nsql-io` export 작성기(grid/csv/tsv/json/jsonl/insert · CJK 폭 정렬) + CSV 파서.
- `nsql-driver-sqlite`(rusqlite bundled) · ★ `nsql-driver-oracle`(kubo · 이름 바인드 IN/InOut · REF CURSOR 핸들 · DBMS_OUTPUT 폴링 · 컴파일만 — 실서버 미검증) · ★ `nsql-driver-mssql`(tiberius · `DECLARE @v 타입 = @Pn` + 트레일러 SELECT로 OUT 회수 · 렌더 테스트 2 · 실서버 미검증).
- `nsql-run`(엔진 ↔ 세션 오케스트레이션 · OUT 흡수 규약 · 이벤트) · `nsql-drivers` 레지스트리(feature: sqlite/oracle/mssql) · `Session::set_option`(SERVEROUTPUT·fetch_size).
- `nsql` CLI: `plan` · **`run`** · **`shell`** · **`export`**. ★ 실기: `examples/sqlite-session-vars.sql`로 세션 변수 왕복(`EXEC :V_MAX := (SELECT MAX…)` → `PRINT` → 후속 조회 바인드) · export json/insert/csv 파일 ✓.
- ★ **GUI 최소 창**(`nexa-sql`): 접속 필드+Connect · 고정폭 SQL 편집기(TextBox 다중행 · IME) · 자체 가상화 그리드 · 상태줄 · 워커 스레드 · `⌘/Ctrl+Enter`/`F5`. `--smoke`로 폰트 체인·드라이버 로드 확인(UI = Apple SD Gothic Neo · 고정폭 = **D2Coding** → 한글 폴백). 창 실기는 사용자 몫(⏳).
- `../nexa-ui`: `nexa-font` 신설 · `Rect::intersection`(커밋 `1f75cc8`).
**분리기 결함 수정**: 한 줄 다중 문장 유실 · `;` 뒤 명령 · 블록 EXEC 오프셋 · `?` 방언 EXEC 래핑.
**문서**: [12 Sublime 프로필](12-user-sublime-profile.md) · [14 폰트·모듈](14-fonts-and-feature-modules.md) · [15 외부 변경](15-external-file-changes.md) · [17 편집기 점증 계획](17-editor-incremental-plan.md) · [18 세션·프로젝트](18-session-and-projects.md) · [19 비교·git·오브젝트 캐시](19-compare-git-and-object-history.md).
**⏳ 사용자 실기**: `cargo run -p nexa-sql` 창 열기 · 한글 입력 · `sqlite::memory:` 접속 후 실행 · Oracle/MSSQL 실서버 접속(`nsql run -c oracle://…`).
→ [journal/2026-09-12](journal/2026-09-12.md)

## 2026-09-12 (1차 · mac) — ★ 프로젝트 착수: 조사 5건 · nexa-ui 추출 · 세션 변수 엔진 · 기반 보고서

**요청**(사용자, 순차 7건): DBeaver급 크로스플랫폼 SQL 클라이언트 · Rust 평가 · 영리만 유료 라이선스 · 언어/플랫폼/아키텍처/범위 결정 · 경쟁 앱 조사(Golden·PLEdit·Orange·SSMS 등) · **sqlplus 변수/CONNECT를 SQL Server에서도** · 컨트롤 라이브러리 선분리 · 편집기 = Sublime 차용 + 패키지 확장 · 편집기 기능 목록(VS Code·ST·IntelliJ+α) · CLI(export/import/bulk) 포함 · 4계층 독립 아키텍처 · 전체 보고서.
**조사**: 에이전트 5건(크로스플랫폼 · Oracle · MSSQL · Rust 생태계 · 편집기) → [03](03-competitive-landscape.md)~[07](07-editor-research.md). 핵심 발견: Oracle 공식 순수 Rust thin 드라이버 베타(2026-08) · MS `mssql-tds` 0.1.0(09-10) · Orange 아직 판매 중 · ADS 은퇴 · T-SQL 변수는 배치에서 죽으므로 `sp_executesql` OUTPUT이 1차 우회.
**산출**: ① `../nexa-ui` 저장소(clip gfx/ctl/conf 이관 · **189 테스트** · 커밋 `d182eee`) ② `nsql-core` + `nsql-script`(분리·명령·바인드·방언 재작성·엔진·CONNECT · **37 테스트** · clippy 0) ③ `nsql plan` dry-run — 사용자 Golden 예시가 Oracle/MSSQL 양쪽으로 계획됨 ④ 문서 13건 + [00 보고서](00-foundation-report.md) ⑤ PolyForm NC 라이선스(양 저장소).
**결정**: DR-1~9 확정(사용자 발언 근거) · **DP-1~10 확인 대기**([10 §2](10-decision-record.md)) — 특히 DP-9(CLI 먼저 관통) · DP-6(셰이핑 크레이트) · DP-10(서명·법인).
**다음**: M1 — `nsql-net` · Oracle/MSSQL 드라이버 · `nsql run/shell` 실접속. → [journal/2026-09-12](journal/2026-09-12.md)
