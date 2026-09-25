# STATUS — 지금 상태 한 장

> 시간 역순. 상세는 [journal](journal/), 여기는 요약.

## 2026-09-23~26 (100차 · mac) — **§211** 컬럼 설명 즉시(테이블 단위 코멘트 캐시 `comments_raw` · NULL 흐림 · 값 그대로) · **§210** 🔧 상세 본문 폰트 겹침(배율) · **§209** 객체 상세 = 탐색기 범위로 축소(테이블 PK·인덱스 · 컬럼 · 인자 · 의존) · 설명만 · ▼/▲ · `preview_box` 상자 · **§208** T-225 ①~④(코멘트 설명 · 스키마 개수 · 더블클릭 · 도형 화살표) · 검색 완료 플래시 · **§207** = ★ T-223 객체 상세 패널([86](86-object-details-panel.md) · 탐색기 아래 독립 영역 · 유형별 섹션 · 읽기 전용 본문 · ▾/▴ · 복사/Shift 복사 · CLI `cat detail`) · **§206** 🔧 검색 중 새 서버 접속 무반응(검색 재요청 · 기동 명령 `connect:`) · **§205** 🔧 변수 입력 창이 메인 뒤로(자식 창 미부착 → 실행 멈춤) · **§203~204** 🔧 검색 입력 지연(아이콘 선굽기 · refilter O(n)·라벨 캐시 · 첫 프레임 1,353 → 221 ms) · 북마크 니모닉 열쇠 이전 · **§202** 🔧 검색 미종료(유휴 타이머 · `background_pending`) · **§201** = T-222 ① L1 디스크 캐시(`metacache.rs` · `meta.disk_cache`) · ② 메모리 창 3층 · 🔧 CI clippy all-targets · **§200** = ★ T-221 메타 3층 L1/L2/L3([85](85-metadata-layers.md) · `Coverage::Names` · L2 컬럼 워머 · L3 TTL 회수 · 검색 판정 스레드 분리 = 프레임 끊김 해소 · D-209) · **§199** 검색 후속 셋 · **§198** = T-220 ① Oracle 인덱스 DBA/USER 빠른 길(3.6 → 0.27 s) · ★ 검색 진행 애니메이션(테두리 도는 선 · 완료 깜빡임 · [84 §8-1](84-explorer-search-index.md)) · **§197** = ★ T-219 탐색기 검색 인덱스([84](84-explorer-search-index.md) · 스키마 순차 인덱스 → 부분 폴더 `n/?` → 순차 완성 · 헤더 `인덱싱 d/T` · 유휴 선적재 · 설정 5키) · 🔧 이력 드롭다운 클릭/우클릭 · **§196** = 🔧 토글 시 탐색기 스크롤 밀림(헤더 높이) · DDL 기본/전체 범위 정정(T-218) · **§195** = CONNECT 동일성 진단 로그(T-217 · D-208) · **§194** = ★ T-211 글로벌 변수 층([63 §11](63-variable-management.md)) · ⑧ 원복(연결별 칸 기본 · `explorer.share_catalog`) · 헤더 정렬 · **§193** = ★ SQL Preview 편집기 동일 + 생성 옵션·DDL 샘플(T-210) · ★ 카탈로그 단위 메타 공유(T-212 · [54 §10](54-connection-model-and-disconnect.md)) · 시스템 스키마 숨김(T-213) · 필터 v2(T-214) · 📐 글로벌 변수([63 §11](63-variable-management.md) · T-211) · **§191~192** = ★ T-207 서버 묶음 트리(S1 → 연결 행 · 헤더 메뉴 모두/개별 · [54 §9](54-connection-model-and-disconnect.md)) · ★ T-208 탐색기 검색창([28 §7](28-object-explorer.md)) · 🔧 SQL Preview 자식 창 · DDL 구분자 · **§190** = ★ T-203·T-205 객체 탐색기 2차(DBMS별 트리 · 하위 폴더 18종 · 유효성 배지 · Generate SQL 7종 · SQL Preview 모달 · CLI `cat sub/gen` · 4-DBMS 실서버 + GUI 덤프 자동 점검 · [83](83-object-explorer-dbms-trees-and-generate-sql.md)) · **§188~189** = ★ T-202 REF CURSOR 결과 이어 받기(정석 · `Session::fetch_cursor_page` · Oracle `CursorSrc::Ref` · 커서 닫힘 = `FetchStop::CursorGone` · 유지 불가 = 전체 조회 강제 + ⚠ `RunEvent::Warning` · BISCM `\more`/`\all` 실측 · [43 §3-6](43-fetch-model-and-result-tabs.md)) · **§187** = ★ T-201 완성 × 문서 크기·향상 모드(72 표 · 창 방식 1 MB · L1 수동만 · 자기 감속 · BOOST +5) · **§185~186** = ★ T-200 문법 참조 기반 완성 1차([82](82-grammar-driven-completion.md) · `.sqlg` 5개 · `grammar.rs` · 절 감지 · 예약어 = 절의 `next` · 플러그인 `NSQL_HOME/grammar`) · 71 §8 팝업 반응 상시 점검 · T-192 미재현(Release 53 ms) · T-189/198 보류 · **후반(§171~184)** = 카드 머무름·불투명(§171) · 창 그룹 앞으로(맥 `orderFront` · §172) · 팝업 이동 지연 실측(§173·§178: Debug 94 ms → 의존 최적화 프로필 + **D-133 IOSurface 기본** = 14 ms · Release 9.8 ms) · `*`/`A.*` 모든 컬럼 조각(§174 · 한 줄/여러 줄·Space/Tab) · ★ T-196 페이지 로딩(§175) · 패키지 테이블 함수 ①~③(§176) · ★ T-187/188 메타 갱신·`intel.refresh`(§177) · `ㅁ.` 없음·사전 뷰 층·스키마 카드 수(§178~179) · 둘러보기 이름순(§180) · FROM 프로시저 제외(§181) · 메모리 창 스파크라인·힙 정리(§182) · T-190 원인 = Instant Client 적재 1.3 s(§183) · 4-DBMS 절별 검토표(76 §14 · `DUAL` · 방언 키워드) · 규칙 = 61 §2-4 ③ 자체 관리 대상·한 줄 고지 · `.claude/settings.json` 맥·Linux 허용 · `scripts/mac-restart-debug.sh` · **push 09-24 c68dfc9·777f850·8c5557d·9105c6f(전부 ci ✓ integration ✓ · nexa-ui 07a2e7f·0bda13e ✓)** · ★ **메모리 모니터**(§167 · [80](80-memory-monitor.md) · `memstat.rs` 원장·포트 · `mem_win.rs` 모델리스·최상위 · 상태줄 총량 · 닫혀 있으면 비용 0 · T-197) · 🔧 FROM 뷰 미표시 = 같은 층 무구분 + "N개 더"(§166) · 📐 완성 페이지 로딩(76 §13 · T-196) · ★ **FROM 자리 뷰·함수 레이어 + 종류 아이콘**(§163 · nsql-script `Cand.layer` · 등급 안 레이어 랭킹 · `from_kinds` · 테이블 함수 `nsql_catalog::table_function` · `intel.from_routines`/`intel.icons`) · 🔧 팝업 밖 휠 차단(§164 · nexa-ctl `wheel_inside` · `App.pointer`) · 🔧 `FROM 스키마.테이블.` 팝업 없음(§165) · ★ 완성 일치 강조(§162 · 전체 = 굵은 파랑 · 부분 = 일치 글자 · `match_positions`) · 🔧 팝업 튐(§133 앵커 = 접두 시작 · nexa-ui `point_at`) · 🔧 컬럼 로딩 지연(§134 메타 워커 우선순위 큐 · FROM 절 선적재 · Oracle 사전 분리) · ★ 팝업 폭 상한·가로 스크롤·가운데 …·상태줄 전체 이름(§135 · `intel.popup_max_width`) · ★ 키 관통 `intel.key_passthrough`(§136) · 아이콘 라벨 1px 규칙(§137) · i18n 간략화 + T-185(§138) · 📐 설정 재정리안 [78](78-settings-reorganization.md)(§139) · 🔧 백그라운드 메타 세션(§140) · ★ **완성 상세 카드**(§141 · `intel_card.rs` · 컬럼/테이블/함수 한 틀 · `#i/n`) · 팝업 클릭 선택·가로 키·트랙 클릭(§142) · 🔧 자동/수동 팝업 자리(§143 `lay_rev`) · 🔧 `ALL_TABLES` 컬럼 요청 누락 + 실측(컬럼 81 ms · 상세 1.36 s → FK 조인 제거 · bg 세션)(§144) · 룰러 45 %(§145) · ★ 정렬 기준·매칭 점수([76 §11](76-intellisense-and-outline.md) · §146) · 🔧 Oracle 사전 = `ALL_OBJECTS`+`V$FIXED_TABLE`(§148 · `ALL_VIEWS` 121 s) · 블록 주석 ⌘⇧/(§149) · 🔧 팝업 휠 delta 0(§150) · 카드 Description(§151) · 🔧 사전 버킷 조회 열쇠(§152 `lookup` · `ALL_TABLES` 컬럼) · 🔧 ⌘⇧/ = Shift+구두점 물리 키(§153) · 병렬 시험 SIGSEGV 간헐(T-191) · 팝업 10행 유지·`이름 : 타입`·⌘⇧? 정의(§154) · 카드 설명 상시(§155) · ★ 블록 주석 자동 점검 `scripts/func-block-comment.sh` ALL PASS + 🔧 종료 깃발 유휴 틱(§156) · ★ **맥 성능 전수**([26 §7-11](26-performance-architecture.md) · `scripts/mac-perf-all.sh` · 상주 개선(창 4개 −43 MB · 65 MB −16) · 누수 0 · ★ 결함 = 유휴 CPU 19 % ← 캐럿 깜빡임 × 맥 present 36 ms → 비활성 **앱** 깜빡임 정지(nexa-sys `app_active` · 80~200 → 0 ms/초 검증) · 활성 중 90 ms/초 = present → D-133 결정 대기(T-193)) · ⌘⇧? = macOS 도움말 검색이 선점 → 맥 기본 ⌘⌥/(§158) · T-195 Rainbow Alias 등재(§159) · ★ 접두 없는 컬럼 = 전 alias + `A.컬럼`/Alt(§160 · 기동 592 ms 내역 = NSWindow 생성 168(T-194) · T-190·192)(§157) · 📐 [79 메타 소유·공유·갱신](79-metadata-ownership-and-refresh.md)(T-187~189) · ★ **완성 메타 즉시 채움·미리 읽기·권한 반영 사전·즉시 회수**(§132 · [76 §9](76-intellisense-and-outline.md) — 현재 스키마 = 서버 값(SQL Server `dbo` 결함) · `intel.preload` · `스키마.` 시점 캐싱 · `nsql_catalog::dictionary` · 상한 없는 후보 · 미사용 버킷 즉시 `drop_bucket` · T-184 등재) · 🔧 **맥 ⌃Space = 공백 입력**(§131 · 맥 열 `ctrl+`=⌘ 결함 → `control+space` 등 다섯 · 수식키+Space는 글자 아님 · 시험 = 맥 표에 OS 키 0) · 최신화 둘(92~99차 win + 99차 후속 · 맥 테스트 nexa-ui 382 · nexa-sql 504 통과 · 회귀 없음) · Release 234 s/Debug 빌드 · 재기동(PID 18411 · 사용자가 닫음) · ★ **하네스 권한 = 맥·Linux 몫을 한 번에**(분류기가 `.claude/settings.json` 편집을 막아 제안 파일 → 사용자 복사 · allow +54 · ask +3 · Linux는 pull만 · [61 §3-1](61-core-design-and-working-rules.md)) · 눈에 띈 것 = CLAUDE.md §4 "지금 =" 줄 낡음. → [journal §130](journal/2026-09-22.md)

## 2026-09-23 (99차 · win) — 🔧 **북마크 문서 이름 = 탭 id로 지금 이름**(§118 · 열쇠는 이미 id(`DocKey::Scratch { tab }`)였고 굳은 것은 표시 이름(`short_name` = `Script_{id}`) → 패널이 **id → 지금 제목 표**(`set_tab_titles` · `doc_name` · 닫힌 탭 = `(closed tab #id)`)로 그릴 때 푼다 · 접힌 문서 = `(그룹, DocKey)` · 호스트는 `Editors::titles_rev` 세대가 바뀐 틱에만 표를 준다 · 자체 시험 `tab.rename_to:` · S69 · U-81) · 🔧 **스크롤 끝 마지막 줄 클릭 = 화면 유지**(§118 · nexa-ui 68차 — 캐럿 추종의 "보임" 판정을 줄 단위 `rows`에서 잔여 px를 포함한 **픽셀 기준 온전한 행 수 `vis`**로 · 시험 `click_on_last_line_at_scroll_end_keeps_pixel_scroll` · U-82) · `project.scan_max` 해석(§118 · 상한 = 트리 노드 수 · 새 폴더를 열 때만 검사 · 한 폴더는 통째로) · 🔧 **SQL `''` = 이스케이프 → 쌍 대상 제외**(§118 · nexa-ui 69차 `Highlighter::doubled_quote_escapes()` = 구문 속성(`SyntaxSpec` 문자열 구분자 있으면 true) · `PairTable`이 같은 인용부호로 열린 문자열 안의 `''`를 건너뜀 · `'O''Neil'` 쌍 하나 · 평문은 종전 · S70 · U-83) · ★ **검색 상자별 최근 검색어**(§119 · 부품 `search_history.rs` = `SearchHistory`(전역 한 파일 `search-history.json` · 상자 이름별 최근순 · 중복 없음 · 상한) + `Recall`(↑/↓ 되부르기 · 끝에서 치던 글 복귀 · 타이핑하면 초기화) · 상자 9(찾기/바꾸기 · 파일 검색 2 · 필터 4 · 설정 검색) · 기록 = Enter/실행/필터는 포커스 잃을 때 · 설정 `search.history_max` 20(0 = 끔) · 시험 3 · U-84) · ★ **검색 설계 변경 = 무제한 · 워커 · 제외 로그 · 병렬 · 로그 병합**(§120 · 사용자 지시 — (a) 프로젝트 필터 열거를 UI 동기 BFS+5,000 상한에서 **부품 `parwalk.rs`**(코디네이터 + 워커 `project.scan_threads` 4 · 폴더 단위 큐 · 채널 하나 = 도착 순 병합 · 부모 먼저)로 · 패널은 틱마다 6 ms 예산으로 합침(`scan_pump`/`fill_children`/`path_index`) · 글 바꾸면 취소 · `project.scan_max` 기본 **0 = 무제한** · 읽지 못한 폴더 = 안내 + 로그 창 · (b) 파일 검색 = `SkipReason`(크기·이진·읽기 실패·이름) + `Batch::Skipped` + `Progress.skipped` · 패널 "제외됨(N)" 접이식 목록 + 상태줄 꼬리 · CLI `nsql grep` 요약 · D-196 대체 · T-175 ✅ · 시험 parwalk 1 + 패널 1 + nsql-search 1 · 39 §3 · 72 · 위키 · U-85·U-86) · **push** = nexa-ui `540296b` ci ✓ · nexa-sql `3edaaf0` integration ✓ · ci ✗(clippy 1.98 `map_or_identity` 1건) → **후속 커밋**(린트 수정 · 위키 2절 · 30 §2 부품 등재 SearchHistory/ParWalk · 36 · 39 · 🔧 제외 행 이유 글 클립 · 🔧 parwalk 시험 flake · S71 펼침 캡처 ✓) = `4c48feb` **ci ✓ · integration ✓** · **검색 입력란 × 지우기**(§121 · nexa-ctl `with_clearable`는 있었으나 배선 0 → 찾기/바꾸기 · 파일 검색 2 · FilterBar 4 · 접속 창 필터 · 설정 검색 · 트랜잭션 로그 검색 · 팔레트 = 9 상자 · 글이 있을 때만 · U-87) · ★ **파일 검색 범위 필터**(§122 · 범위 상자 한 줄 = 경로 · `<open files>` · 포함 `*.sql`/`+pat`(OR) · 제외 `-pat`(AND NOT) · 조건은 콤마로 몇 개든 · 패턴만이면 기본 범위에 얹음 · nsql-search `SearchOpts.includes`(걷기에서 파일마다 글롭) · CLI `nsql grep -g *.sql --exclude *_test.sql` · 시험 2 · S72 · U-88 · `649efa5` ci ✓ integration ✓) · 🔧 **× 뒤 목록 미갱신**(§123 · 접속 창 마우스 경로가 `refilter`를 안 부름 → `take_changed`면 다시 거름 · 파일 검색 검색어를 비우면 결과·제외 로그·상태 비움 · 9 상자 전수 점검 · U-89 · `9bd2ba7` ci ✓ integration ✓) · **검색 결과 가로 스크롤 + Alt 전체 경로 빈칸 + 열린 탭 범위 필터 + Oracle 아이콘 라벨 맞춤**(§124 · `scroll_x`/`content_w` · 전체 보기 = 축약 없이 스크롤 · nsql-search `path_allowed` = 열린 탭도 같은 글롭 · 루트 아이콘 라벨 `select_font_sized` 축소 · S73 · U-90~U-93 · `679c851` ci ✓ integration ✓) · 🔧 **이진 파일 아웃라인 = 앱 종료**(§125 · nsql-script `words()` 바이트 슬라이스 패닉 → 글자 경계 · fuzz 시험 2 · 호스트 게이트 `intel_unsuitable` = 큰 파일·SQL 아님·이진(NUL/U+FFFD) → 아웃라인/완성/Goto Symbol 무시 + 안내 · S74 · U-94 · `307bb4a` ci ✓ integration ✓) · 🔧 **`"O'Neil"` 짝 오염 + fail-over**(§126 · nexa-ui 70차 — SQL 규격 raw 문자열의 `\x22`가 `"`가 아닌 `\`였던 것 → `string = ' "` · `Highlighter::strings_span_lines()` = 줄 끝 재동기화(VS Code/JetBrains/Sublime/Vim/Emacs 조사 → 공통 분모 셋 중 빠졌던 "끝나지 않은 문자열은 줄 끝에서 끊는다") · S75 · U-95) · ★ **검색어 이력 드롭다운(기본)/Flat**(§127 · nexa-ui 71차 `ContextMenu::set_max_rows` 스크롤 메뉴 · `Recall` = 클릭/↑↓로 상자 아래 최근 N개(`search.history_rows` 5) · 이동 키·휠·Enter·타이핑 거름·Esc · 마지막에서 ↓/Tab = 목록으로 포커스(`RecallEvent::LeaveDown` → FilterBar `LeaveDown` → 패널 4 · 파일 검색 · 설정 창) · `search.history_view` 기본 dropdown · 🔧 패널이 ↓를 가로채 Flat 되부르기가 죽어 있던 것 정정 · 시험 2 · S76 · U-96) · 📐 **[77 데이터 작업대 구조](77-data-workbench-architecture.md)**(§128 · 할 일 7 = 객체 척추 `ObjectRef`→`MetaStore`→`ObjectAction` + 데이터 척추 `View` 필터→`ChangeSet`→`SqlGen`→gate→`Export` · T-177~T-183 · D-197~200) · ★ **T-178 인텔리센스 1차**(§129 · nsql-script `builtins` 정적 표 = 방언별 내장 함수(시그니처)·Oracle DBMS 패키지 12·사전 뷰(`ALL_*`·`V$*`·`sys.`·`pg_catalog.`·`sqlite_master`) · `Context::paren_owner` · 시그니처 도움 상태줄 `ƒ …` · 확정 손질 = 함수 `NAME()`+캐럿 안·키워드 공백·FROM 뒤 alias · `INSERT INTO t (` 전체 컬럼 조각 · 팝업 스크롤(`set_max_rows`) · 컬럼 상세 PK/FK/UQ/NOT NULL · 🔧 카탈로그에 없는 이름 뒤 `.`이 빈 팝업이던 것 · 예산 로그 `intel.budget_ms` · 설정 7 · 위키 3분 사용법 · S77·S78 · U-97~U-99 · nexa-sql 199 · `d818cbc` ci ✓ integration ✓). → [journal §118~129](journal/2026-09-22.md)

## 2026-09-23 (97차 · win) — ★ **거터 메뉴 정정**(§108 · 맨 위 "Bookmarks"(패널 열기) · 토글 하나(있으면 ✓ · 제거 항목 없음) · 마지막 줄 아래 빈 영역 = 보기만(nexa-ui 66차 `TextBox::below_text`) · 폭 170 `open_status_popup_w`) · **북마크 패널 셋**(§109 · 🔧 `Char` 사건이 필터에 안 닿던 결함 → `filter_feed` · 이름 편집 상자 = 니모닉·줄번호 다음 이름 자리(`label_x` 실측) · 이름 있는 북마크 = 이름 강조색·굵게 + 본문 흐림) · ★ **한글 조합 중 자모로 즉시 검색 = 검색에 쓰이는 모든 입력**(§110 · IME 경로 `query_changed` 일반화(확장 → 프로젝트·북마크·찾기) · **nsql-core `hangul`**(의존 0 · 음절 → 타자 순 자모열 · 글자 경계 시작 · 마지막 글자 부분 일치 · "ㄱ"⊂가 "기"⊂긴 "달"⊂닭) · FilterBar 일반 모드 · 편집기 `find_in_chars` · nsql-search `Matcher::Jamo`(ASCII 접두 사전 필터) · 설정 창 검색) · **파일 검색 셋**(§111 · 범위 상자 비면 작업 모드별 = 파일 모드 열린 탭만 · 폴더 = + 그 폴더 · 프로젝트 = + 프로젝트 폴더 + 추가 폴더 `SearchCtx.default_roots` · 파일 행 셰브론 = `draw_chevron_90_in` · 결과 클릭 = 미리보기 탭(포커스 패널) · 더블클릭/Enter = 정식) · ★ **쌍(괄호·인용부호) = 기본 기능 + 설정 개편**(§112 · nexa-ui `PairTable` 문자열 안 스캔 = 열린 인용부호를 바닥으로 · 안의 짝 없음은 조용히 · `\"` · **`editor.pair_kinds`**(`() [] {} <> "" '' ``` 중 지정 · 기본 `< >` 제외) · `editor.pair_in_strings`(켬 = 문자열 안 모든 쌍 · 끔 = 그 문자열의 인용부호 쌍만) · `editor.pair_match`(옛 `rainbowpair.match`) · `rainbowpair.quotes`·`angle` 제거 · 확장 = 색만 · **규칙: "Rainbow Pairs"를 명시하지 않은 쌍 요구는 기본 기능**) · **줄 변경 표시**(§113 · 빈 줄 묶음 안 삽입/삭제 = 캐럿 줄 힌트로 창을 위로(`diff_lines_hint`) · §115 🔧 명령(줄 복제 등)으로 바꾼 본문은 다음 사건까지 표시·쌍 표가 안 바뀌던 것 → 본문 세대 `rev` 비교 `diff_rev`/`pairs_rev`) · **`project.scan_max` 검토**(§114 · 이 PC Downloads 10,485개 · 5,000 = 55 ms · 동기 열거라 5000 유지 권장 · 워커화 뒤 상향 = D-196/T-175) · 자동 점검 S61~S65 · 위키 Bookmarks·Explorers-and-Filters · 51 §13 · 36 · TODO U-72~U-76 · ★ **확장 SDK + WASM 동적 로딩 완성**(§116 · [75](75-extension-sdk-and-dynamic-loading.md) — 조사 결론 = WASM(`wasmi` · D-87 ① 확정) · `extensions/sdk` 작업 공간(`nexa-ext-sdk` ABI v1 = 버퍼 4바이트 LE 길이 + JSON · export 5 · import 2 · 의존 0 · 호스트 목 테스트) · 샘플 `hello-ext`(64 KB)·`rainbow-pairs`(76 KB · 공식 패키지 1.1.0 kind=wasm) · 런타임 `extensions/wasm.rs`(호출마다 새 인스턴스 · 연료 5천만/메모리 16 MB/200 ms/브레이커 3 · 편집기 op 큐) · `Registry::sync_wasm`(같은 id = WASM이 내장 대체 · 실패 = 내장 폴백 D-197) · 매니저 `kind=wasm` 설치 + `files[].url`(Releases 자산 D-199) · `scripts/ext-build.*` · 시험 3(실제 .wasm 로드 · 설치→로드→대체→삭제) · 앱 +1.1 MiB(12.08 MiB) · 잔여 75 §9) · ★ **IntelliSense + 문서 아웃라인 1차**(§117 · [76](76-intellisense-and-outline.md) — JetBrains/DBeaver/VS Code/SSMS/SQL Developer 조사 → 보편 = 트리거 3 · alias 문맥 · 객체/컬럼/키워드/문서 단어 · 조각 일치 · MRU · 아웃라인 창 + 심볼 팔레트 · nsql-script `outline`(문장·DEFINE/VARIABLE·CTE·CREATE 대상·PL/SQL 서브프로그램/커서/타입/라벨·선언 변수·T-SQL @v) · `intel`(문맥 5종 + alias 표 · 점수 6등급 + 한글 자모 · MRU 랭킹 · KEYWORDS) · 호스트 `intel.rs`(팝업 = ContextMenu D-201 · `.` 즉시 · 2자+250ms · Ctrl+Space · Enter/Tab/Esc · 탭별 캐시 D-203) · 설정 `intel.*` 13키(Editors ▸ Code completion) · Goto Symbol Ctrl+R(팔레트 `sym:<byte>`) · 아웃라인 패널(`view.outline`) · 탐색기 응답 → `MetaStore` feed + `Req::ColumnsMeta` 즉시 채움 · nexa-ctl `TextBox::rev/caret_point` · 시험 8 · S66~S68 · 위키 Code-Completion-and-Outline · 2차 = T-176 · 실기 U-77~U-80). → [journal §108~](journal/2026-09-22.md)

## 2026-09-22 (94차 · win) — ★ **성능 종합 점검 프로세스 규정**([71](71-performance-review-process.md) 신설 — 규정 대상 **12차원**(용량 · 정적/동적 라이브러리 · 구성 파일 · 기동 · 메모리 · 회수 · 누수 · 실행·입력 · I/O · 프로세스·스레드 · 메인 프로세스 영향 + 횡단 향상 모드·데이터 구조 재사용) · **순서 A~G**와 그 근거 · 회귀 판정선 · **기능 추가 때 등재 체크리스트 10** · 3-OS 도구 대응표 · "성능 평가해줘"가 오면 이 순서를 그대로 돈다 = CLAUDE.md §3 · [61 §4](61-core-design-and-working-rules.md)) · ★ 실행기 **`scripts/win-perf-all.ps1`**(Linux `linux-perf-all.sh` 대응 · `-Stages`/`-Only`) + **`scripts/win-inventory.ps1`**(PE 헤더 직접 파싱 — `dumpbin` 없이) · ★ **배포 인벤토리 첫 측정**([26 §7-8](26-performance-architecture.md)): GUI 10.07 MiB(`.text` 8.50) · CLI 6.54 · 정적 crate 165/143 · **동적 import 27/20 = 전부 OS 제공(배포 DLL 0) · 지연 import 0** · 구성 파일 **3개 294 B** · 설정 키 343 · ★ **향상 모드 2차 점검**([45 §4](45-perf-boost-benchmark.md) · 강제 키 26 → **36**): 기동 11.13 → 9.45 MB · 유휴 CPU 125 → **0** · **2 MB 스크립트 21.14 → 15.60 MB(−26 %) · 141 → 0 ms** · **확장 패널 유휴 CPU 688 → 94 ms** · 10만 행 결과는 39.4 MB 그대로(결과 불변 = 설계 확인) · 프로젝트 패널 차이 0 · ★ **[39 §3-7 기능 성능 프로파일](39-resource-governance.md)** 신설(기능 19 × 7열 + **스레드 원장 13종**(유휴 상주 11~14) + 자식 프로세스 6종 · 상주 자식 0) · **판정 = 93차 기능은 외부 crate 0 · 새 상주 스레드 0 · 새 구성 파일 0 B 잠금 하나 · 상주 증가 0**(2 MB 스크립트는 오히려 27.45 → 21.14) · ★ **A~F 전 단계 실행**: **기동 창까지 141 → 47 ms(−67 %) · 3초 CPU 500 → 156**(91차 Linux에서 고친 아이콘 `memo`+글꼴 걷기 캐시가 3-OS 공통이라 Windows에 그대로 — [65 §6](65-cross-os-performance-comparison.md) 3번의 답 · **한 OS의 병목 수정이 다른 OS의 이득이 된 첫 사례**) · **누수** 파일 0.008 · 10만 행 0.136(89차 0.26~0.30) · 로그 창·프로젝트 패널 0.000 MB/주기 · **벤치** 70만 줄 3.4/4.1 ms · 4만 줄 전 기능 8.8 · 되돌리기 0.08 · 문장 1.3 µs = 회귀 없음(70만 줄 **전 기능** 84.8 ms는 새 데이터 — 그 크기는 앱이 L2로 기능을 끈다) · 측정 도구 흠 둘 수정(PowerShell 변수 대소문자로 **전 단계가 조용히 건너뛰어짐** · 앞 프로세스 종료 미대기로 0 표본 · **0은 값이 아니라 측정 실패**) → T-168. → [journal §38](journal/2026-09-22.md)

## 2026-09-22 (92차 · win) — 최신화(mac 90차 + Linux 91차 · Windows nexa-ui 360 · nexa-sql 412 · 회귀 없음) · 🔧 **nexa-ui 53차 Windows CI `test` 실패의 진짜 원인 = nexa-font 시험끼리의 `set_text_gdi` 경주**(볼드 잉크 = 보통 잉크 · `gh` 로그로 확인 · 91차 "`file_type()` 경로" 결론 정정) → 시험 가드 `GdiOn` · 걷기 캐시 + `file_type()` **3-OS 복귀**(Windows 걷기 1회 12.6 → 1.2 ms · 기동마다 10여 회 → 한 번). CI = nexa-ui e5433cb · nexa-sql bcb4123 3-OS ✓(Windows 포함) · 규칙 = 개발 중 `target/` 아래 `nexa-sql.exe`는 누가 띄웠든 강제 종료하고 진행(61 §2-4) · Release 재기동 · ★ **토스트 남은 시간 표시**(실행 상태 카드·일반 토스트 — 왼쪽 상태 막대가 위에서부터 옅어지고(지나간 부분 30%) 카드가 진척 따라 투명해짐 · `ui.toast_progress` · 향상 모드 끔 · 415) · **비밀번호 창** 문구 정정(금고 = 종료까지 암호화 기억 · 두 줄 · `PasswordAsk`) · 폭 440 · 높이 = 내용만큼 · 🔧 ★ **프로필 이름 대/소문자만 바꿔 저장 → 프로필·파일 소실**(대/소문자 무시 FS) → **프로필 파일 = SHA-256 해시 이름**(`name=` 안에 · 종전 꼴 읽기 호환·이관 · **불변식 프로필 수 == 파일 수** 테스트 · nsql-vault 17) · 접속 창 파일명 줄 + 복사 버튼 · 우클릭 "설정 파일 이름/경로 복사"(단일 진입점 `copy_profile_file`) · 가린 입력란 **입력 언어 안내**(`imestate`/`imehint` · 가/あ/中/RUS · `ui.ime_hint_secs`) · 캡처 명령 `conn.menu`·`conn.ime_hint` · 420 · 📐 **[66 트랜잭션 테스트 시나리오](66-transaction-test-scenarios.md)**(자동/수동 × DBMS 4 · A~F · 기록표 · T-164 · ★ 재구성 = **DBMS별 복사 실행 대본** — 격리 홈·프로필·설정 명령부터 관찰 질의(PG·MSSQL `application_name` · Oracle `SYS_CONTEXT` SID · SQLite `BEGIN IMMEDIATE`)·정리까지 붙여넣기 블록 · CLI 대본은 실제 실행 검증 · 안 되는 명령 셋 수정) · 🔧 **편집기 가로 스크롤**(경로 정상 · 가로 막대가 드러날 길이 없던 것 → 가장자리 접근으로 그 축만 드러남 · nexa-ui 55차 · `ui.hwheel` 캡처 명령) · 📐 **67 프로젝트 관리**(조사 67a · 결정 D-144~149 · T-165) · 📐 **68 확장 개발자 API**(동적 로딩/해제 · D-87·D-150~155 · T-166) · 🔧 편집기 가로 범위 통일(휠 끝 = 줄 끝) · 🔧 클릭 시 잔여 px 유지 · **개발 규칙 점검**(확인 = 프로젝트 외부만 · 권한 제안 파일 적용 · 61 §2-4·§3-1 3-OS 권한) · ★ **동시 편집**(탭 Shift/Ctrl+클릭 ≤ `editor.split_max` 3 · 독립 칸 · 미니맵만 끔 · 단일 클릭 = 단일 · `split_plan` MC/DC · §27) · 📐 ★ **[69 북마크 관리](69-bookmarks.md)**(DB 클라이언트 18 · IDE 8 · 편집기 12 전수 조사 → 대상 = 파일 위치 + **객체 DDL 편집창**(객체 자체 북마크는 사용자 정정으로 제외) · 앵커 4단(줄 변경 기록 → diff 매핑 → 줄 원문 → 양방향 탐색 → 무효·부활) · 저장은 **67 프로젝트/워크스페이스에 얹음**(공용 = 프로필 이름만 · 개인 = 접속 좌표) · 무효는 회색 보관 후 30일 정리(**자동 만료를 가진 제품은 전무**) · 이름·메모·니모닉 0-9(문서별)·그룹·패널·거터·미니맵·인라인 라벨 · CLI · 결정 **D-156~177 확정** · T-167 · ★ 좌측 **북마크 관리자 패널**(그룹→문서→항목 · 좌클릭 미리보기 · 우클릭 메뉴 넷 · 어려운 경우 30 · 설정 독립 그룹 `Bookmarks` 분류 셋 38키)). · ★ **프로젝트 탐색기 + 미리보기 탭**(`project.rs` `.nsql-project` · `project_panel.rs` 지연 트리·깊이 무관 필터·빈 상태 링크 · 편집기 `open_preview`/`poll_preview` 승격 · Project 메뉴+팔레트 · 설정 `project.*` · T-165 P1·P2·P4 일부) · 동시 편집 탭 상단 줄(`TabBar::set_group`) · ★ **파일 열기 다중 선택 + 순차 적재**(nexa-dlg `marks` dir2 규약 · `ConfirmMany` · 확인 팝업 `file.open_max` 10 · 스레드 1개 `multi-open` · 자리 탭 전부 먼저) · ★ **다중 인스턴스 시작 모드**(`split_file_args` · `instance.lock` `File::try_lock` · `startup_project_plan` MC/DC · `project.restore_last` 기본 끔 · **MSRV 1.89**) · 📐 **70 자동 저장·저장 큐·프로젝트 복원**(조사 8종 · `SaveQueue` 대기 재설정+상한 · 백업 스레드 · 참조 파일 백업 7일 · D-178~183) · push = nexa-ui `f23243d` ✓ · nexa-sql `f452617`(**ci 3-OS ✓ · integration ✓** — CI rustc 1.98.1의 새 린트 2건·Linux dead_code 1건 수정 §33) · 🔧 IME 안내 = 입력란 아래·언어 바뀌면 즉시(`ui.ime_hint` · 150 ms 조회) · 미리보기 탭 더블클릭 = 승격(§34) · 🔧 파일 창 Ctrl 수식키 미전달(Ctrl+클릭·Ctrl+Space·Ctrl+A · §35) · 🔧 트리 키보드 이동 스크롤 추종 + PageUp/Down/Home/End(nexa-ui 56차 `reveal_row` · §36) · 🔧 다중 선택 강조 열쇠 = 노드 경로(펼침 뒤 어긋남 · §37) · ★ 긴 경로 가운데 축약 + Alt = 전체(nexa-ctl `ellipsize_middle`·`set_show_full` · `ui.menu_max_width` · §38) · 파일 창 Ctrl+드래그 스윕 선택(§39) · 빈 공간 드래그 러버밴드(§40) · 📐 71 §C-2 메모리 회수 시험 R1~R6 신설(실행 보류 · §41) · push = nexa-ui `7ec2462` ✓ · nexa-sql `c979106` ci 3-OS ✓·integration ✓(§42) · ★ **실행 Facade**(43 §11 · 새 SQL이면 카드 · 커서 이어 읽기는 상태줄/로그 · `Requery` 이벤트 · `Page.via_cursor` · D-184~186 · §43) · 그리드 선택 표시 5건(NULL 더 흐림 · `grid.row_focus[_color]` · 헤더 색 통일 · §44) · 다중 열기 확인 = 중앙 모달·Enter(§45) · 편집기 placeholder 제거(§46) · 열기 창 캐럿 테두리·기본 버튼(`ButtonTone::Accent`)·팝업 기본 항목(`ContextMenu::set_default`)·행번호 선택도 연한 행 배경(§47) · 실행 카드 `HH:MM:SS.mmm` + `run.toast_tick_ms`(향상 모드 1000 · §48) · 결과 헤더 위선 1px(§49) · ★ **실행 카드 누적**(`runs` · `stack_order` · 휠 스크롤 · 카드별 hit · 중지 버튼 빨강/회색/링 · §50) · ★ 카드 스택 앱 공유·최신 최상단·픽셀 스크롤·클리핑·`run.toast_follow`/`run.toast_max`(향상 모드 8 · §51) · 스택 아래 고정(§52) · 툴팁 = 카드 왼쪽(§53) · 🔧 편집 메뉴 팝업 층(`set_popup_deferred` · §54) · ★ **팝업 메뉴 배타 게이트**(`open_menus`/`route` 전후 비교 · §55) · 🔧 하위 메뉴 비활성 행 닫힘(§56) · 좀비 훅 정리 `scripts/win-kill-stale-hooks.ps1`(§57) · 🔧 **바깥 우클릭 통과**(nexa-ctl `ContextMenu::is_outside_click` · 컨테이너 넷 · §58) · ★ **Quick Skip Next `edit.skip_occurrence`**(Ctrl+K,Ctrl+D · 맥 ⌘K,⌘D · nexa-ctl `skip_next_occurrence` · §59) · ★ **Edit 메뉴 그룹 6**(nexa-ctl `MenuEntry::Sub` 풀다운 하위 메뉴 · §60) · push nexa-ui `22d7a7d` ✓ · nexa-sql `da0c15c` ci 3-OS ✓·integration ✓(§61) · ★ **기능 점검 자동화 `scripts/win-func-check.ps1`**(28 시나리오 · 전부 ✓ · 도구 결함 3 수정 · `multi.` 기동 명령 경로 · §62) · ★ **Windows 성능 전수 95차**(71 A~F 5회·8초 + **C-2 회수 시험 첫 실행** `scripts/win-mem-reclaim.ps1` · 회귀 0 · 용량 +0.34 % · 기동 36 ms · 누수 L2 0.078 · 회수 6항목 허용치 안 · [26 §7-9](26-performance-architecture.md) · §63) · push `af49db1` ci 3-OS ✓ · integration ✓(§64) · ★ **선택 되돌리기 soft undo/redo**(Ctrl+U/Ctrl+Shift+U · 맥 ⌘U/⌘⇧U · nexa-ctl `SoftStep` · Edit ▸ Undo Selection ▸ · §65) · ★ **크기·줄 수 제약 원장 [72](72-size-limits-and-large-file-constraints.md) + 위키 [Large-Files-and-Limits](wiki/Large-Files-and-Limits.md)**(L1/L2 단계표 · 자체 상한 전수 · `editor.max_occurrences` 배선 · `editor.highlight_max_kb` 폐기 · §66) · 🔧 다중 선택 ←/→ = 각 구간 앞/뒤로 접기(nexa-ctl · §67) · ★ **제약 후속 7건**(다중 선택 상한 = 모든 입구 `EditState::max_regions` · Rainbow 상한 = 단계 따름 · `editor.copy_confirm_mb` 복사 확인 · 스트리밍 없음 명시 · 39 규칙 · 위키 Home + `wiki-publish.sh` · §68) · 프로젝트 UX(새 프로젝트 저장 = 빈 프로젝트+기본 폴더 · 헤더 클릭 전환 · 열기 단일 선택) + 🔧 팔레트 한글(IME = 포커스 ∨ 팔레트 · §69) · ★ **북마크 1차**(`nsql-bookmarks` 코어 · `bookmarks.rs` · `bookmarks_panel.rs` · 명령 27 · 설정 그룹 Bookmarks 14키 · 워크스페이스 저장 · §70 · T-167 🚧) · ★ 북마크 2차(그룹 트리 · 우클릭 메뉴 넷 · 5초 되돌리기 토스트 · 거터 니모닉 상자 · §71) · 🔧 다중 캐럿 타이핑 화면 유지(nexa-ctl · §72) · push nexa-ui `18f7c5d` ci ✓ · nexa-sql `b598d65` ci 3-OS ✓ · integration ✓(§73) · ★ **북마크 3차**(B8 CLI `nsql bookmark list|add|rm|prune` = GUI와 같은 워크스페이스 파일 · U-2 미니맵 틱 `set_minimap_marks` · 줄 끝 라벨 `set_inline_labels` · 설정 `bookmark.minimap`/`inline_label`/`inline_label_chars` · 자동 점검 S36 · §74) · 🔧 **풀다운 Project 무반응**(`menu_action`에 `project.*` 분기 없음 → 한 길 · S37 · §75) · ★ **프로젝트 탐색기 후속**(셰브론 부품 · OS 아이콘 `project.icons`(향상 off) · 루트 우클릭 메뉴 · 대화상자 빈 곳 클릭 = 선택 해제(TreeGrid `clear_selection`) · 폴더 확정 = 지금 폴더 · 프로젝트 파일 `selected` 복원 · 탭 메뉴 Reveal in Project Explorer · `project.auto_reveal` · S38~S43 · §76~78) · ★ **필터 틀 부품 `FilterBar`**(Aa·ab·(.*)·Path · 숨김/점 파일 토글 = 설정 연동 · 4 패널 여백 8 · 빈 곳 클릭 해제 · S46~S50 · §80~81) · 가로 스크롤·행 클립·루트 메뉴 Remove만(§79) · 🔧 상한 안내 오판(§81) · ★ **변수 확장 시점**(63 §9 조사 · `vars.expand_at` assign/use · DEFINE 원문→값 · D-184 T-171 · §82) · 북마크 셰브론 부품(§83) · ★ **DBMS 아이콘 파일 12**(generic 틀 + 대표색 + 고정폭 라벨 2줄 · 로고 교체 가능 · `dbms_icons.rs` · §84→§86) · ★ **편집기 탭 유형 `TabKind`**(줄 색 분리 `editor.tab_line_*` · 메뉴 기준 · Keep Open · §85) · 🔧 Ctrl+Shift+숫자 = 물리 키(니모닉 · §87) · 빈 곳 클릭 = 캐럿 테두리(탐색기 셋 + 대화상자 · §88) · ★ **프로젝트 작업 환경**(탭·캐럿 앵커·활성·북마크 저장 · 자동 저장 30초 · 종료 흐름 · fuzzy 복원 · OPEN FILES · §89) · ★ **보완 일괄 7건**(북마크 단일 원천 · 저장 디바운스 · OPEN FILES × · 변수 창 DEFINE 행 · 미저장 스냅숏 `backups.rs` · T-171 최소판 `Action::Replan` · 북마크 미리보기 탭 · §90) · ★ **96차 성능 전수 재실행**([26 §7-10](26-performance-architecture.md) · [71 §10](71-performance-review-process.md) — 95차 대비 기동·10만 행·2 MB·벤치 회귀 0 · GUI +3.0 % · 외부 crate 0 · 회귀 1 = 프로젝트 패널 OS 아이콘(+2 MB · 핸들 +82) → **워커 유휴 5초 회수** `ICON_WORKER_IDLE_MS`(핸들 307 → 281) · 나머지는 OS 이미지 리스트 고정 비용으로 39 §3 등재 · L2 30주기 = 톱니 0.025 MB/주기 = 누수 아님 · T-168 ①②④⑥ 해소 · 기능 점검 전수 S01~S51 **51/51 ✓** · push `7c4499a` ci ✓ · integration ✓ · §91) · ★ **96차 후반(09-23)** = 🔧 묶인 탭 상단 줄 각자 색(파일 유형 `None` → 바 공통으로 떨어지던 것 · nexa-ui 63차 `tab_color`) · 북마크 패널 더블클릭/Enter = 정식 탭 · ★ **프로젝트 저장/복원 전면**(🔧 저장이 작업 환경을 안 담던 것 · 🔧 저장마다 복원이 돌아 탭이 늘던 것 → `project_set(p, restore)` · 열기 = **교체**(옛 탭 닫기) · 좌측 패널 상태 `expanded`·`panel`·`search`·`bm_collapsed`·`profiles`(표식만) · 탭 `preview`) · 자동 점검 S52~S55 + 전수 **55/55 ✓** · push nexa-ui `a6d8f50` ci ✓ · nexa-sql `5bb3ecf` ci ✓ · integration ✓ · §92 · 📐 **[73 프로젝트 경로 이식](73-project-path-portability.md)**(Sublime·VS Code·JetBrains·Eclipse·DBeaver 조사 → 앵커 `${project}`/`${folder:이름}`/`${home}`/`${var}` · 자리 탭 · v2 절 구조 · P1~P4 · D-186~190 대기 · T-172·T-173 · §93) · ★ **내장 변수 층 `${workspaceFolder}`**(nsql-script `intrinsic.rs` · VS Code 이름 + `NSQL_*` 별칭 · DEFINE → 내장 → `${env:}`(내장 → OS) · `${config:키}` · `${workspaceFolder:이름}` · GUI 실행마다 스냅숏 · CLI · 경로 설정 `expand` · `vars.intrinsic` · §94) · push `260e9a3` ci ✓ · integration ✓(§94) · **북마크 후속 셋(§95)** = 두 단계 코드 키 Ctrl+K,Ctrl+K / K,N / K,P / K,X(§94의 Ctrl+Alt 안은 교체) · ★ 거터 배치 **북마크 영역 → 줄 번호 → 편집 → 미니맵**(nexa-ui 64차 `bm_extra` · 줄 번호 꺼도 북마크 영역 유지) · 🔧 저장 길 `bind_project`가 북마크(니모닉)를 옛 워크스페이스로 되돌리던 결함(1/1,2 → 1/2,3) · S52 확장 · S56 · push nexa-ui `aca1533` ci ✓ · nexa-sql `dd45a5e` ci ✓ · integration ✓ · **실행 카드 복사 버튼**(§96 · `RunToastHit::Copy` · 카드 유지 · 카드 우측 끝 이미지 버튼) · ★ **복사 버튼 부품 `copybtn`**(§97 · 눌림 → 녹색 체크(12px) → `ui.copy_feedback_ms` 2초 뒤 복귀 · 실행 카드 + 접속 창 · 30 §2) · ★ **프로젝트 닫기 = 처음 실행 상태**(§98 · `reset_to_initial` · 빈 Script_1 · 탐색기만) + 🔧 북마크 로컬/프로젝트 분리(새 프로젝트 = 로컬 → 프로젝트 이관 · `mark_migrate_local`) · S57 · ★ **작업 모드 셋**([67 §6](67-project-workspace.md) · §99 · `project::WorkMode` = 파일(전역) · 폴더(`nexa-sql .` → `<폴더>/.nsql/` · 지금은 북마크 · `${workspaceFolder}`) · 프로젝트(파일 안) · `local_dir()` 한 자리 · S58) · OPEN FILES 점 표시 정정(§100) · ★ **파일 탭 미저장 본문 = 프로젝트 파일에 탭별 저장 → 자동 복구**(§101 · `hash`·`id` · 백업은 백업용 · 미저장 탭 북마크 `remap_scratch`) · ★ **프로젝트 파일 형식 v2**(§102 · [67 §7](67-project-workspace.md) · JSON 헤더 + `%%NSQL-BLOBS%%` 탭별 payload 블록 · nsql-settings `projfile` · GUI/CLI 공용) · push `c3921e4`(7 커밋) ci ✓ · integration ✓ · OPEN FILES = 프로젝트 없어도(§103 · S59) · 🔧 **저장 보증**(§104 · 북마크 디바운스·프로젝트 자동 저장이 스스로 깸 `next_save_at`/`project_autosave_next` · 창 X도 `request_exit` → `flush_on_exit` · S60) · 파일·폴더 모드 이름 없는 탭 북마크 = 메모리 전용(§105 · 로컬 파일엔 파일·객체 열쇠만 · 프로젝트 모드는 담고 복원) · 📐 **[74 변수 보존의 문서 식별](74-vars-persistence-identity.md)**(경로 열쇠의 한계 · fsid·지문·문서 id·프로젝트 탭 블록 층 쌓기 · P1~P3 · D-191~195 대기 · T-174 · §106) · ★ **거터 우클릭 북마크 메뉴**(§107 · nexa-ui 65차 `TextBox::in_gutter/line_at_point` · 없으면 추가·있으면 제거·니모닉 1~9/해제 · `bm_gutter_items` 순수 함수 · S61·S62). → [journal §14~91](journal/2026-09-22.md)

## 2026-09-22 (91차 · linux) — ★ **Linux 첫 세션**(Ubuntu 26.04 VM · 2코어): Oracle Instant Client 23.26 셸 환경(`sqlplus` · `libaio1t64` 링크 · `scripts/install-instantclient-linux.sh`) · 최신화(mac 90차) · 프로필 3개 등록·접속 OK · **성능 전수**(26 §7-7 · 도구 `scripts/linux-*.sh`): 상주·10만 행·65 MB 파일·릭·프레임 = Windows와 같은 형상 · ★ **Linux 기동 병목 둘 수정**(nexa-font 가족 탐색이 폰트 트리를 12번 걷고 `statx` 11,786회 → 걷기 1회 캐시 · 툴바/메뉴 아이콘 래스터가 메모 없이 호출마다 + 찾기 막대 11개 즉시 = 166 ms → `memo` + 첫 그리기 때) → 창까지 522 → **111 ms** · `[startup]` 구간 계측 · **전체 테스트 자동화**(`scripts/linux-all-tests.sh` · nexa-ui 357 · nexa-sql 414 · 실서버 통합 13/13 · Oracle 직접 지정 1/1 · CLI 5종) · OS별 차이표(61 §3 Linux 열 · journal §7) · Linux 앱 아이콘(`app_id` + `.desktop` · `scripts/install-desktop-linux.sh`) · ★ **Linux 모달 = 메인에 붙은 창**(창 백엔드 기본 X11 `gfx.linux_backend` · `WM_TRANSIENT_FOR`+`_NET_WM_STATE_MODAL` · x11rb — winit 0.30 Wayland는 부모·올리기 미지원) · **push 1f0d4d0·2f62a8d·cf4585e = CI 초록** · nexa-ui는 Windows에서 `file_type()`이 원인(종전 `is_dir()`로 되돌린 b3d8e8b = 3-OS 초록) · ⏳ 확인 대상: Linux 비밀번호 없는 접속의 비밀번호 창(시작 인자 경로는 뜸 · 편집기/로그인 창 경로 = T-163 ⑦). → [journal](journal/2026-09-22.md)

## 2026-09-21 (90차 · mac) — 최신화(89차 win 8 커밋 · 맥 nexa-sql 407 통과) · 프로필 `BISCM`(oracle)·`M4PLAN`(mssql) 등록·접속 OK · 🔧 **시작 직후 Details 팝업**(폼 저장본이 빈 값이라 첫 프레임부터 바뀜 판정 → 생성자 `mark_saved` · 테스트) · 🔧 **탐색기 키보드 서버 간 이동**(↑/↓ 경계 · PgUp/PgDn 뷰포트 페이지로 이웃 칸 안까지 · Home/End 전체 · `cross_pane` MC/DC) · 🔧 맥 편집기 탭·표식 메뉴 겹침(`Editors::set_bounds`에 `menu.set_scale`) · 🔧 Σ 건수 버튼 초기 활성(도구줄 초기 상태 = 판정 함수) · ★ **비밀번호 창 최상위 모달**(사건 가드 · 맥 자식 창 · Windows EnableWindow) · 🔧 **`EXEC` 블록 현재 문 실행 Msg 102**(정규화 `text` 재분할 → 원문 `span` · 왕복 불변식 테스트) · 🔧 **로그 표현**(`PRINT` = 값 · `VARIABLE` = 이름·타입 · 클라이언트 명령엔 전송 로그 없음 `Begin.server`) · 프로필 `Repository`(postgres) · 흠: `nsql conn add`가 `--password-stdin`·`NSQL_PASSWORD`를 무시(T-132 ③). 워크스페이스 414. 미커밋. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 24 · win) — 남은 작업 묶음: 성능 재점검 = **회귀 없음**(기동 11.1 MB · 10만 행 40.9 MB · 기동 141 ms · 유휴 6초 47 ms) + 바인드 열 이름 판정을 선형으로 · ✅ T-161(감시 시험 조건 대기) · ✅ T-132 ③(`--password-stdin`) · ✅ T-162 ②③ · ✅ T-158(포인터가 밖에 멈춰 있어도 드래그 선택이 이어진다) · "미사용 확장" · bash/gk 누수 = 우리 프로세스 아님(조치 없음). nexa-ui 360 · nexa-sql 405. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 마감 + 추가 21~23 · win) — ✅ **89차 push**(nexa-ui d116a91 · nexa-sql 317e6c7 · CI `ci`·`integration` 초록 — 중간에 CI clippy 1 · Windows 시험 1 · `POSTGRES_HOST` 1을 고침). 그 뒤(미커밋): SQL Server 결과 머리줄 = 변수 이름(바인드만 있는 SELECT 항목) · 읽기 전용 숫자 바인드 `3.0000000000` → `3` · ★ CLI도 비밀번호 자리가 없으면 서버에 가지 않는다 · **저장하지 않은 탭 닫기 = 저장 여부를 묻는다**(저장하고 닫기 → 이름 없으면 저장 창). nexa-sql 403. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 20 · win) — 🔧 `CONNECT M4PLAN`(프로필 이름)을 되풀이하면 **매번 다시 접속**하던 결함 수정 — 같은 서버 판정을 최종 접속 정보(저장소에서 완성한 스펙)로 한다. nexa-sql 402. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 19 · win) — 연결이 해제된 서버에서 탐색기 **새로 고침**: 아무 표시 없던 것 → 고른 자리의 목록을 "⚠ 접속 안 됨"으로 바꾸고 경고 토스트·상태줄로 알린다 · 접속 문자열로 붙은 서버도 루트에 `offline` 표시. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 18 · win) — 🔧 탐색기 우클릭 메뉴가 **항목으로 이동하는 사이 사라지던 것** 수정(탭 표식 메뉴가 열려 있을 때 · 이동 사건이 포커스를 편집기로 옮겨 메뉴가 닫혔다) → 메뉴는 하나만 · 이동으로는 포커스를 옮기지 않는다. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 17 · win) — 🔧 같은 서버 재접속이 거부된 **첫 실행에서 오류 카드가 뜨지 않던 것** 수정(전용 세션이 거둬지며 그 세션의 카드·상태줄이 함께 사라졌다 · 탭이 다른 공유 연결로 돌아갈 위험도 있었다) → 세션을 남기고 탭은 연결 없음. nexa-sql 400. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 16 · win) — 🔧 전용 세션에서 같은 서버에 **다른 비밀번호(빈 값 `user:@host` 포함)로 `CONNECT`** 하면 아무 동작이 없던 결함 수정: 기존 접속을 먼저 닫고 다시 접속 · 그 서버·계정의 **자격 자리**(하나)를 명시한 값으로 바꿔 들고 · 서버가 거부하면 자리를 비워 다음 `user@host`는 다시 묻는다. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 15 · win) — 임시 비밀번호(세션 자격 금고) 폐기 규칙: **서버가 비밀번호를 거부하면** 폐기하고 그 접속 안에서 바로 다시 묻는다(안내 포함) · 네트워크·잠김·만료로는 폐기하지 않는다 · **프로그램 종료 때** 봉투와 키를 0으로 덮어 버린다. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 14 · win) — ★ **세션 자격 금고**: 비밀번호 창으로 입력해 접속에 성공한 값은 **메모리 봉투**(실행마다 새 난수 키 · 디스크 0 · 앱 종료 = 소멸)로 들고 같은 서버·계정의 다음 접속(이 연결로 새 탭 · 재접속 · 탐색기)에 다시 쓴다 · 실패하면 바로 잊음 · 설정으로 끔. 🔧 "이 연결로 새 탭"이 전용 세션뿐인 서버에서 미연결 탭을 만들던 것 → 전용 세션을 하나 더 연다. nexa-sql 395. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 13 · win) — 🔧 탐색기 우클릭 새로 고침이 **한 번도 열지 않은 노드를 저절로 펼치던 것** 수정(접힌 노드는 펼치지 않는다 · 다음에 펼칠 때 새로 읽는다). → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 12 · win) — 🔧 비밀번호·변수 입력 창의 Enter/Esc가 메인 편집기로 전파되던 결함 수정(합성 누름 버림 + 키를 뗄 때까지 반복 누름 버림). ⏳ 키 실기 = U-12. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 11 · win) — 🔧 **객체 탐색기 우클릭**(새로 고침 · 연결 해제 · 이름 복사 · 새 탭)이 실제 입력으로는 동작하지 않던 결함 둘 수정(우클릭이 탐색기에 가지 않음 · 고른 동작이 다음 워커 응답까지 밀림) · 연결 해제 = 툴바와 같은 화면(픽셀 비교) · 메뉴는 창 위에 · 대상 행 아래/위(이름을 가리지 않음) · ★ **일회성 비밀번호**: `user@host` = 입력 창으로 한 번 묻고 접속에만 쓴 뒤 0으로 덮어써 버림(저장·로그·스펙 0 · 가린 칸 복사 불가) · `user:@host` = 빈 비밀번호 명시(창 없음). nexa-ui 359 · nexa-sql 393 · ⏳ 실제 비밀번호 접속 실기 = U-12. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 8·9 · win) — 설정 ▸ DBMS ▸ Oracle 정리: 중복 칸 둘 제거 · 설명 = "목적: …" · 파일은 **있음/없음**으로(자동·직접 지정 공통 · tnsnames.ora는 별칭 수) · 설정 창이 다시 활성화되면 **자동 갱신**(외부 편집 반영 · 버튼 없음). → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 7 · win) — 🔧 설정 창의 **잠긴 칸이 실제로는 입력되던 결함**(포커스·키·IME·붙여넣기 경로가 잠금을 지나지 않았다) → 잠긴 글자 칸 = 읽기 전용(복사는 됨) · 잠긴 콤보 = 포커스 0 · 저장 0 · 방식 전환 즉시 재계산 — 모든 잠긴 카드 공통. nexa-sql 389. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 6 · win) — ✅ **폴더 전용 대화상자**(nexa-dlg `PickerMode::Folder` — 폴더만 보인다 · 세 OS 공용) · 설정 ▸ DBMS ▸ Oracle의 폴더 칸에 **"찾아보기…"**(직접 입력도 그대로) · 자동 탐지 방식 = **탐지된 경로를 수정 불가로 표시**(없으면 공백). nexa-ui 357 · nexa-sql 388. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 4·5 · win) — 🔧 이름 바꾸기 상자: 바깥 **우클릭도 취소** · ✅ **탭 드래그 고스트**(편집기·결과 탭 공용 · 놓일 자리 = 옅은 강조 칸 · 본체 = 포인터를 따라가는 고스트 — 그리드 컬럼 이동과 같은 틀) · ✅ 결과 탭 우클릭 ▸ **실행 쿼리 복사**(`ResultTab.sql`에 보관). nexa-ui 356 · nexa-sql 387. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 3 · win) — 📐 [64 §4·§5](64-dbms-clients-and-driver-packaging.md): **경량 배포판 정의**(외부 네이티브 의존 0 · 지금 = Oracle 뺀 A판 · Oracle 공식 순수 Rust `oracledb`가 갖춰지면 C판 — T-159) · **동적 적재·해제 검토**(실측: 순수 Rust 드라이버는 접속해도 +0.2 MB·끊으면 회수 / Oracle만 +9.2 MB·모듈 36개가 남는다 → 돌려받는 길은 드라이버 호스트 **프로세스**뿐 · 선택 모드로) · 🔧 이름 바꾸기 상자 = **그 탭 바로 아래 + 강조 테두리**(제자리 편집은 T-160). → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 2 · win) — 🔧 탭 이름 바꾸기 입력 상자를 **간단한 형태**로(입력란 + 안내 한 줄 · 팔레트의 빈 목록 영역 제거 · 편집기 탭·결과 탭 공용). → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 추가 · win) — 🔧 **편집기 밖으로 나간 드래그 선택이 글자 단위로 느리게 가던 결함**(nexa-ctl · 원인 둘: 옆으로 나가면 y를 무시하고 사건마다 한 글자 이동 + 마우스 이동마다 본문 전체를 문자열로 만듦 — 2 MB 1.18 ms · 20 MB 12.7 ms/이동 → **0.001 ms 미만**) → 왼쪽 밖 = 그 줄의 처음 · 오른쪽 밖 = 그 줄의 끝 · 위·아래 밖은 멀수록 여러 줄 · 세 번 클릭의 본문 전체 복사 제거. nexa-ui 355 · nexa-sql 385. ⏳ 실기 U-9. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 마무리 묶음 · win) — ✅ 결과 탭 이름 = **`결과N`**(가장 큰 번호 + 1 · `grid.result_tab_title`) + 우클릭 **이름 바꾸기** · ✅ **우클릭 메뉴·툴팁·드롭다운 잘림 전수 조사**(nexa-ui `place_popup` + `DrawCtx::surface_size` 안전망 · 코딩 규칙 = CLAUDE.md §3 · 61 §2-2) · ✅ `${…:q}` 방언별 보완(SQL Server `N'…'` · MySQL 역슬래시) · ★ **설정 ▸ DBMS 그룹**([64](64-dbms-clients-and-driver-packaging.md)): Oracle Instant Client **자동 탐지 / 직접 지정** + 읽기 전용 파생 정보(`tnsnames.ora` 등) · 실서버 테스트가 "지정한 폴더가 비면 다른 클라이언트로 조용히 접속" 결함을 잡음 · 📐 드라이버 내장 ↔ 확장 검토 = **내장 유지 · 경량판은 Cargo feature로**. 테스트 385 · nexa-ui 354. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 끝 · win) — ✅ 결과 탭: 스위치 **`grid.result_tabbar_single`**(결과 1개에도 탭 영역 · 다중 탭이 켜져 있을 때만 · 옛 `grid.result_tabbar` 이관) · 다중 탭 끄기 = 향상 모드 표에 포함(메모리 억제 · 사용자 결정) · 설정 창의 잠긴 선택 상자 흐림 표시 수정. ✅ 큰 파일 단계별 제한: **확장 효과 L1부터**(`file.large_ext_level` — 괄호 색·쌍 강조·본문 전체 쌍 스캔 · 자동 닫기는 유지) · **구문 강조 L2부터**(`file.large_syntax_level`) · 단계가 내려가면 기능 복귀. ✅ **`${env:이름[:형식]}`** OS 환경 변수 치환(`vars.env_subst`). 테스트 380 · clippy 0. → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 후반 · win) — ✅ **T-151 완료(실서버 Oracle 19c · SQL Server 2019)**: Oracle DATE가 글자로 바인드돼 **시각이 잘리고 ORA-01722**이던 결함 → DATE·TIMESTAMP·BOOLEAN 진짜 바인드 · SQL Server 호출에 `OUTPUT`을 빠뜨리면 값이 **조용히 버려지던** 것 → 서명으로 보충 · 선언 없는 글자 변수가 `NVARCHAR(2)`로 **잘리던** 결함 · 세 실서버 통합 **13/13**. ✅ **T-153**: `COLUMN … NEW_VALUE` · 시스템 변수 9종 · `${이름:형식}` · 값 상한(hover만 보류). ✅ 성능 향상 영역: `perf.boost`에 `editor.undo_persist`·`meta.refresh_highlight_ms` 추가 · 새 부하원 둘에 스위치(`vars.signature_lookup` · `pg.refcursor_expand`) · 맥 `iosurface`는 D-133 뒤 후보. ⏳ **사용자 확인표 U-1~U-8**(TODO T-154). 테스트 377 · clippy 0. **commit/push는 지시 대기.** → [journal](journal/2026-09-21.md)

## 2026-09-21 (89차 · win) — ✅ **Windows 종합 점검**(맥 86~88차 뒤 · Release A/B): 상주 메모리 = 기준선 그대로(기동 11.7 · 접속 10.1 · 창 4개 16.2 · 10만 행 41.2 · 65 MB 파일 86.5/피크 106.5 MB) · 유휴 CPU 0~31 ms/초 · 기동 창 ≈155 ms · 릭 주기 4종 평탄(10만 행 재실행의 +0.3 MB/회는 기준 빌드도 같고 탭을 닫으면 회수) · 창 12종 그리기 · 수동 커밋 · 문장마다 결과 탭. 🔧 창이 화면 밖에 놓이던 결함(`wingeom::keep_on_screen`) · **T-155** 빈 `COMMIT` 제거. ★ 개발: **T-150 PG refcursor ✅**(실서버 PG 14.5) · **T-151 PG 서명 → OUT 받기 ✅** · **T-152 `Caps` 포트 ✅**(동작 불변 — 6방언 `plan` 동일) · **T-153 일부 ✅**(`ACCEPT` · CLI `-v` · 미정의 바인드 경고). 테스트 365 · clippy 0. **남음 = T-151 잔여(SQL Server·MySQL·Oracle 타입 — 서버 필요) · T-153 잔여 · T-154 키 입력 실기(사용자) · commit/push는 지시 대기.** → [journal](journal/2026-09-21.md)

## 2026-09-21 (88차 인계 · mac → win) — 📘 **윈도우에서 이어 가기**: [61 §1-5](61-core-design-and-working-rules.md)(09-20~21에 굳은 불변식 — 수동 커밋 `tx_open` · 변수 표 세 층 · 결과 탭 · `captures` · 서명 추론 · 입력 창 · `present.rs`) · [61 §3](61-core-design-and-working-rules.md) "윈도우에서 처음 할 일" · [61 §6](61-core-design-and-working-rules.md) 이어 받을 일 순서표 + 알아 둘 흠 7 + 결정 대기 · TODO **T-150~154**(PG refcursor · 서명/타입 · `Caps` 포트 · 변수 UX 잔여 · ⏳ Windows 실기) → [journal](journal/2026-09-21.md)

## 2026-09-21 (88차 후반 · mac) — ✅ **CI 전부 초록**(T-146 PG 실서버 확인) · 변수 관리 이어서: **변수 창**(보기·제자리 편집·공유 토글·새 변수) · `SELECT … INTO` 행 수 정책(D-139 · SQL Server `@@ROWCOUNT` 검사 · 실서버 9/9) · 기준 계측(문장 준비 3 µs · 덤프 훑기 0.37 ms) · 테스트 353 · 다음 = PG refcursor(Codespaces) · `Caps` 포트 · MacroStore · `ACCEPT` → [journal](journal/2026-09-21.md)

## 2026-09-21 (88차 · mac) — ★ **변수 관리 D-135~138 확정·구현**: 문장마다 결과 탭 · **계층형 변수 표**(탭 → 연결 공유 → 프로필 · `VAR x SHARE` · 탭이 주인) · `SHOW VARIABLES` 표 · **실행당 한 번 입력 창**(`:바인드` + `&치환` · 종전 GUI는 `&v`가 말없이 빈 글) · **파일별 변수 보존**(실행 가능한 스크립트 · 비밀 미저장) · 테스트 349 · 다음 = 변수 옆 패널 · V5 능력표·PG refcursor · V7 전수 점검 → [journal](journal/2026-09-21.md)

## 2026-09-21 (87차 후반 · mac) — 🔧 ★ **T-146 = 수동 커밋이 PG·SQLite·MSSQL·MySQL에서 실제로는 자동 커밋이던 데이터 안전 결함** 수정(CI 확인 대기) · ★ **변수 관리 [63](63-variable-management.md)** 설계(Usage 19군 · 타 도구 소스 수준 조사 [63a](63a-variable-research.md) · 능력표 포트 · 단계 V0~V7) + **V0·V1 구현**(REF CURSOR 자동 표시 · 서명 추론 = `VAR` 없이 `EXEC p(:PC_A, :PC_B)` · 암묵 결과 전부 · GUI 딸린 결과 탭 — Oracle 실기 ✅) · 테스트 340 · **⏳ 결정 D-135~138** → [journal](journal/2026-09-21.md)

## 2026-09-21 (87차 · mac) — 🔧 **86차 보완 전부**(T-148 ✅ `SHOW ERRORS` · `sqlite:////` · 오류 표기 방언 · 루트 이름 · `NSQL_TRACE_MEM` 맥) · ★ **T-147 맥 화면 내보내기 🚧**(IOSurface · `gfx.mac_present` · present 36.2 → **2.9 ms** · 프레임 51 → 16 ms · 기본은 softbuffer — **D-133 = 기본 전환 여부**) · 결정 번호 중복 정리 · 테스트 334 · **다음 = ★ 변수 관리 기능(조사 → 설계 → 결정 질문 → 구현)** → [journal](journal/2026-09-21.md)

## 2026-09-20 (86차 · mac) — ★ **T-139 맥 한글 입력 ✅**(앱 조합 · 전 입력란 `가나다1234 가나다!@#$` 자동 검증) · 🔧 **분할기 방언 결함**(SQLite 트리거/`BEGIN;` 뒤 문장이 오류 없이 미실행 → `split_script_in`) · 전 객체 유형 DDL(Oracle 19c · SQLite) 생성·수정·삭제 자동 시험(스냅숏 1,649개 일치) · 📐 맥 present 37 ms = 색 공간 변환(D-133 · T-147) · 메모리 mac 점검 → [62](62-macos-input-and-present.md) · 미커밋

## 2026-09-20 (85차 · win) — 📘 **[61 핵심 설계와 작업 규칙](61-core-design-and-working-rules.md)**(다른 PC·맥에서 그대로 이어 가기 — 편집기 불변식 ①~⑦ · 작업·검증·격리 규칙 · Windows ↔ macOS 차이 · 맥에서 처음 할 일) · CLAUDE.md §3 세션 공통 규칙(로컬 메모리 → 저장소) · 검증 스크립트 저장소 편입(`win-burst-capture` · `win-big-probe`) → [journal](journal/2026-09-20.md)

## 2026-09-20 (84차 · win) — ★ **편집 버퍼 교체 T-142** ✅([59 §6](59-large-file-handling.md) · nexa-ctl `TextBuf` = UTF-8 갭 버퍼 + 줄 표 + 변경 기록 · 그리기 = 보이는 줄만 — 65 MB: 상주 348 → **87 MB** · 피크 443 → 106 MB · 입력 190 → **3 ms** · 유휴 CPU ≈ 0) · **되돌리기 후속 넷** ✅([60 §7](60-undo-redo-redesign.md) · 쉬었다 치면 새 묶음 · 재로드 = 최소 줄 편집 · 거대 편집 2단 확인 · **재시작 뒤에도 남는 기록** `undofile.rs`) · 찾기 = 줄 단위 · 다음 = T-145(줄 변경 표시·괄호 표의 줄 단위 갱신) · Mac = T-139 → [journal](journal/2026-09-20.md)

## 2026-09-20 (83차 · win) — ★ **되돌리기 재설계**([60](60-undo-redo-redesign.md) · 연산 기록 · 저장 = 지운 글자만 · 저장 지점 O(1) · 바이트 예산 — 붙여넣기 100 KB 34초 → 1 ms · 모두 바꾸기 2,000건 15초 → 3.4 ms/1단계) · ★ **대용량 파일 구현**([59 §5](59-large-file-handling.md) · 큰 파일 모드 L1/L2 · 열기 선택 넷 · 디스크에서 실행 · 읽기 전용 · **탭 격리 적재 진행 막** `fileload.rs` — 65 MB: 여는 CPU 4.5 → 0.5초 · 피크 556 → 443 MB · UI 스레드 0 ms) · 로그인 표식 크기·정렬 · **확장 상세 = 확장 뷰 탭** · **탐색기 우클릭 ▸ 계층형 새로 고침** · ⏳ 결정 대기 D-129~132 · 다음 = **T-142 버퍼 교체** → [journal](journal/2026-09-19.md)

## 2026-09-19 (82차 · win) — ★ **큰 파일 성능**(4만 줄: 유휴 그리기 78 → 3.3 ms · 입력 209 → 16 ms · 유휴 CPU 812 → 188 ms) · **되돌리기 = 차이 저장**(묶음마다 본문 전체 복사 제거) · **메모리 회수**(`memtrim.rs` · 즉시 1회 + 유휴 주기 · 큰 탭 닫기 41.5 → 10.5 MB) · 덮어쓰기 = 타임아웃 버튼 · 📐 [59 대용량 파일 처리](59-large-file-handling.md)(결정 대기 D-125~128) → [journal](journal/2026-09-19.md)

## 2026-09-19 (81차 · win) — ★ **T-140 외부 파일 변경 처리** ✅([58 §5](58-external-change-policy.md) · 조용한 재로드 · 겹침 0 병합 · 비모달 띠 · 저장 2단 확인 · `file.external_*` 7키) · **접속 유형(개발/시험/운영)** ✅(목록 우클릭 · `?env=` · PROD 칩 · 운영 기준 · 실행 확인) · **메모리 재점검** [26 §7-3](26-performance-architecture.md)(기동 12.0 MB · 이전 대비 내역) · 🔧 세션 창 유휴 CPU 90% → [journal](journal/2026-09-19.md)

## 2026-09-19 (80차 · win) — ★ **T-138 객체 탐색기 갱신** ✅([57](57-explorer-refresh-after-ddl.md) · 실행한 DDL → 그 폴더만 디프 · 커밋 시점 · F5/Shift+F5 · "객체 없음" 신호 · 유휴 워터마크 · 설정 `meta.refresh_*` 7키) · T-137 "차단 중" 띠 ✅ · 안내 탭 = Plain Text + No connection ✅ · 📐 외부 파일 변경 처리 [58](58-external-change-policy.md)(결정 대기) → [journal](journal/2026-09-19.md)

## 2026-09-19 (79차 · win) — ★ **확장 패널**(활동 막대 "확장" · 검색 + 설치됨/설치 가능 · 행 버튼 · 상세 탭 · 관리자 켜져 있을 때만) ✅ · 관리자 명령 게이트(`ext.disable_mgr` · 끔 = 모든 확장 정지) ✅ · 빈 목록도 팔레트 안에서 안내 ✅ · 설치 다운로드 추적(파일별 URL·HTTP·크기·시간·속도·IP · 개발자 모드) ✅ · 🔧 일반 편집 모드 괄호 색 끔(확장 없을 때 색이 보이던 결함) · 이웃 깊이 대비 색 순서(nexa-ctl `contrast_order` · `rainbowpair.contrast_order`) ✅ → [journal](journal/2026-09-19.md)

## 2026-09-19 (78차 · win) — ★ **T-137 수동 커밋 잠금 방지 L1~L4** ✅([56 §9](56-manual-commit-lock-prevention.md) · 읽기 트랜잭션 자동 종료 · 유휴 경고/카운트다운 자동 롤백 카드 · 막힘 감지 · 서버 안전망 · 설정 8키) · 오류 2회 표시(카드+토스트 → 하나) · 명령 팔레트 마우스(hover 선택·휠·스크롤) → [journal](journal/2026-09-19.md)

## 2026-09-19 (77차 · win) — 📐 **수동 커밋 잠금 방지 설계 [56](56-manual-commit-lock-prevention.md)**(도구 조사 · 네 겹 L1~L4 · D-100~105 대기 · T-137) · 확장 테스트 안내 [50 §13](50-extension-system.md) · 자동 닫기 = 편집 코어 설정으로 정리(`rainbowpair.auto_close` 제거) → [journal](journal/2026-09-19.md)

## 2026-09-19 (76차 · win) — 괄호 자동 닫기 주 스위치 **`editor.auto_close_pairs`**(편집기 분류 · 확장 유무와 무관 · 종전엔 숨은 확장 분류에만 있어 끌 수 없었다) → [journal](journal/2026-09-19.md)

## 2026-09-19 (75차 · win) — 🔧 설정 창·트랜잭션 로그 창 **한글 입력 불가**(창 생성 시 `set_ime_allowed(true)` 누락) → [journal](journal/2026-09-19.md)

## 2026-09-19 (74차 · win) — 타입어헤드 HUD 위치 = **이미지 드롭다운**(nexa-ctl `PositionDropdown` · `SettingKind::Position` · 설정 창 `CardCtl::Pos`) → [journal](journal/2026-09-19.md)

## 2026-09-19 (73차 · win) — 탐색기 DBMS 오브젝트 종류 라벨 12종 = 모든 언어에서 영어(Tables · Views · Procedures …) → [journal](journal/2026-09-19.md)

## 2026-09-19 (72차 · win) — 🔧 설정 창 왼쪽 위 스위치 조각(화면 밖 카드의 빈 bounds에 nexa-ctl Switch가 (0,0)에 그림 → `is_empty` 가드 · Checkbox도) → [journal](journal/2026-09-19.md)

## 2026-09-19 (71차 · win) — ★ 탐색기 **타입어헤드**(nexa-beep 이식 · nexa-ctl `typeahead`/`hangul` 부품 · 한글 직접 조합 · Windows 한/영 키 · ↑/↓ 매치 순환 · HUD · 설정 5키 `explorer.typeahead*`) → [journal](journal/2026-09-19.md)

## 2026-09-19 (70차 · win) — 🔧 탐색기·파일 트리 **← 키**: 펼쳐진 폴더 = 접기 · 아니면 상위로(nexa-ctl TreeControl 공용 + 오브젝트 탐색기) · **DBeaver 내비게이터 키**(→ 첫 자식 · Home/End · PageUp/Down · Backspace · `+`/`-`/`*` · 앞글자 찾기 · [28 §1-1](28-object-explorer.md)) → [journal](journal/2026-09-19.md)

## 2026-09-19 (69차 · win) — 🔧 결과 도구줄 **연결 전/불필요 시 비활성**(nexa-ctl 글리프 비활성 흐림 결함 · `set_session_connected` · 새로고침 = 결과+출처 · Σ = 서버에 더 있을 때만 · 행 편집 버튼 T-94까지 흐림 · [43 §4-2](43-fetch-model-and-result-tabs.md)) → [journal](journal/2026-09-19.md)

## 2026-09-19 (68차 · win) — 🔧 툴바 **Disconnect 배지가 새 탭에 바로 안 늘던 결함**(표식 갱신이 탭 묶기보다 먼저 → 순서 교정 + `sess_ui_dirty`) · 실기 자동화 `scripts/win-badge-probe.ps1`(1→없음 · 2 · 3 · 닫기 2 즉시) → [journal](journal/2026-09-19.md)

## 2026-09-19 (67차 · win) — 🔧 **캐럿 깜빡임 위상 = 입력 기준**(↑로 1열 이동이 "오래 걸리던" 것 = 위상 꺼짐에 옮기면 최대 0.5초 안 보임 · Sublime/VS Code처럼 입력 직후 켜짐) · 실측 [55 §6](55-editor-input-latency.md): Release 입력→present **2~3 ms** · 이동 판정 0.01 ms · 계측 `tmark` 한 줄 + `scripts/win-latency-probe.ps1` · 헛짚기 2건(모달 열린 채 측정 · eprintln 파이프 비용) 기록 → [journal](journal/2026-09-19.md)

## 2026-09-19 (66차 · mac) — ✅ 접속 창 **접속 중 막**(반투명 덮개 · 입력 차단 · 대상/단계/경과 카드 · 성공 시 닫힘 · 실패 시 사유) · 65차 탐색기 루트 경주 수정

## 2026-09-19 (65차 · mac) — 🔧 시작 인자/`dev.start_demo` 접속에서 탐색기 루트가 안 붙던 경주 수정(`spec` 선설정 + `has_server` 보강) — 사용자 "12시간 뒤 SQLite 접속 안 됨" 점검 결과 세션은 살아 있었고 탐색기 표시 결함

## 2026-09-19 (64차 · mac) — ✅ 필수 항목 `*`·경고 띠(T-133) · 목표 열 커서 · I-빔 · **입력 지연 1차 개선 [55](55-editor-input-latency.md)**(2 MB 페인트 195→13 ms) · 임시 Demo 자동 접속(Debug 전용) — **결정 대기 D-115~118 · 다음 = T-136 부분 무효화/버퍼 보존** · 푸시 ✅(nexa-sql `febcf22` · nexa-ui `6933eec` · CI green)

## 2026-09-19 (63차 후반 · mac) — ✅ 툴바 드래그 고스트·다중 행·Esc · 그리드 컬럼 고스트·Esc · 탐색기 가로 스크롤/셰브론 · Σ 규정 · **Disconnect 연결 모델 [54](54-connection-model-and-disconnect.md)** · 해제 로그 경로 · 로그 창(선택 복사·체크박스·⌘C) · 창 위치 드리프트 — **결정 대기 D-115~118**

**⏳ 실기(63차 후반 전 항목) · 다음 = D-115~118 답 → T-133 · T-131 잔여 · T-132 · T-130 · T-121 잔여 · T-122.** 미푸시(63차 · nexa-ui 38차).

## 2026-09-19 (63차 · mac) — ✅ T-131 폼 바뀜 표시·Save 유도(잃는 순간 3택) · 비밀번호 없는 `CONNECT` = 자격 빌리기였음 → 제거·거부(T-132) · No connection 유지 · 📐 필수 항목 식별 설계 [22 §10](22-driver-extensions.md) — **결정 대기 D-115·116**

인라인 접속 문자열은 비밀번호 필수(`ErrPasswordRequired`) · 배지 "No connection"은 사용자 선택이라 공유로 되돌리지 않음 · 폼 = 칸 띠 + `•` + `Save •` + 목록 `*` + 불러오기/New/닫기 때 저장/버림/취소. **⏳ 실기(63차 ①②④) · 다음 = D-115·116 답 → T-133 구현 · T-131 잔여 · T-132 · T-130 · T-121 잔여 · T-122.** 미푸시(63차).

## 2026-09-19 (62차 · mac) — 신호등 갱신(창 열면 1회 · 상한 60s · 클릭 재확인) · SQLite 파일 판정 · 접속 문자열 열/복사 · 기본 스키마(`?schema=`)

## 2026-09-18 (61차 · mac) — ✅ 세션 창(별도 창 · 서버별 표 · 우클릭 조작) — Disconnect ▾ / View ▸ Session Manager

## 2026-09-18 (60차 · mac) — ✅ 접속 UX 전체 설계·구현([52 §7-2](52-session-modes.md)) — 표식 메뉴(공유 목록·미연결·전용) · 세션 관리자(서버별) · Disconnect 본체 = 모두 해제 / ▾ = 관리자 · 새 탭 규칙 · 같은 서버 CONNECT 옵션 · 표식 항상

케이스 표 = 탭 상태 3(공유·전용·미연결) × 조작 12 · 남은 틈 2건 명시. nexa-ui 33차: Toolbar ▾ 클릭 `id#drop`. **⏳ 실기 T-125(+60차 항목) · 다음 = T-130 · T-121 잔여 · T-122.** 미푸시(59·60차).

## 2026-09-18 (59차 · mac) — ✅ 끊긴 세션 관리 구현([53 §8](53-connection-liveness.md) · D-109~114 권장안) — 동작 직전 SYN 판정 · Broken 상태 표시 · `is_alive` · TCP keepalive · Broken 중지 = 워커 교체

VPN 끊김: 실행·페치·커밋·접속 전에 SYN 1로 판정해 **몇 초 안에** "서버에 닿지 않음"(Broken · 플러그 빨강 · 표식 끊김) · 복구 뒤 다음 실행 때 자동 재접속(세션 상태 소실 안내) · ■는 죽은 소켓에 OCIBreak 대신 워커 교체. 설정 `probe.stale_secs`(60) · `net.keepalive_secs`(60) · `session.call_timeout_secs`(0). 잔여 T-130. **⏳ 실기(VPN 끊긴 채) · 다음 = T-121 잔여 · T-122.**

## 2026-09-18 (58차 · mac) — 📐 접속 생존 관리 조사 [53](53-connection-liveness.md) — VPN 끊김 실측 · 타 도구 비교 · 처방(Broken 상태 · 동작 직전 판정 · TCP keepalive) · **결정 대기 D-109~114**

실측: 앱은 VPN 주소로 맺은 좀비 소켓을 "접속됨"으로 들고 있고(OS keepalive 끔 · 드라이버 미설정) 다음 F5는 수 분 막힌다. 다른 도구도 감지는 다음 동작 때이지만 드라이버 타임아웃/keepalive로 막힘을 짧게 한다. → 53 §3 상태 모델 · §4 고칠 곳 10 · T-127~130.

## 2026-09-18 (57차 · mac) — 탐색기 = 서버 루트를 접속 순으로 **한 트리처럼 이어 붙임**(공용 스크롤) · 로그인 목록 열 조절 = 결과 그리드와 같게 + 딱 맞는 자동 맞춤 · 기본 창 크기 · 미니맵 기본 켬 · REF CURSOR 예제

`ExplorerSet`: 56차의 "한 번에 한 서버 + 머리줄 선택기" 폐기 → 각 서버 트리를 내용 전체 높이로 연달아 놓고 스크롤은 전체에 하나(`Explorer::set_clip` 공용 뷰포트 · 선택은 전체에 하나 · 오프라인 루트 우클릭 제거 · SQLite 루트 = 파일 이름). 로그인 목록: 마지막 열을 끌면 경계가 커서를 따라 가로 스크롤 · 경계 더블클릭 = `pad + max(헤더, 값) + pad/2 + 1` 자동 맞춤(정렬 배지는 달려 있을 때만) · 기본 열 폭(대상 최소 408) · 로그인 창 748×526 · 메인 창 1375×945 · `editor.minimap` on · `examples/oracle-refcursor-pkg.sql`(`패키지.프로시저(:V, :rc)` → `PRINT rc`). **⏳ 실기 T-125(52 §13 + 위 항목) · 다음 = T-121 잔여 · T-122.**

## 2026-09-18 (56차 후반 · mac) — D-96~108 확정·개발 ✅ · ★ 서버별 탐색기(세션 ≥1이면 유지) · 세션 상태 있는 세션은 유휴 닫기 제외 · 트랜잭션 로그 세션 열 · 공유 상한 8

[52 §2-2](52-session-modes.md) `ExplorerSet` = 서버 키당 탐색기 하나(메타 = 인텔리센스·툴팁의 단일 원천 · 어떤 세션이든 붙으면 확보 · 0이면 접속만 닫고 트리 유지 · 메타 유휴 회수 · 활성 탭 서버로 전환 비용 0). [52 §6-4] 세션에 남는 것 조사(Oracle 패키지 상태/컨텍스트 · MSSQL `SESSION_CONTEXT`·`#temp` · PG 사용자 GUC · MySQL `@v` · **Oracle `DECLARE`는 블록 범위**) → `stateful` 세션은 닫지 않음 + 세션이 바뀌면 1회 안내. `CONNECT` 앞 문장 거부 · 탭 ✓/✗ · `TxLog` 전 세션 합본. **⏳ 실기 T-125(52 §13 ①~⑳) · 다음 = T-121 잔여(MetaStore 인텔리센스 서버 키) · T-122.** 미푸시.

## 2026-09-18 (56차 · mac) — ★ **DR-34 세션 컨텍스트 1차 ✅** · 공유 연결 N · 전용(CONNECT) · 개별 모드 · 통제 단일화 · MC/DC · 맥 로그인 창/텍스트박스

[52 세션 모드](52-session-modes.md) 신설. `sessions.rs`(`Sess` = 워커 + 실행 상태 전부 · 활성 탭 세션을 `self.sess`로 맞바꿈) · 문지기 `gate_open` 하나로 실행·Explain·페치·전체 조회·건수·키 조회·Commit/Rollback·접속 통제(무통제 5건 수정) · 편집기 `CONNECT <프로필|"접속 문자열">` = 탭 전용 세션 · `DISCONNECT`/탭 표식 메뉴 = 공유 복귀 · `session.mode=per-editor` · 접속 창 Connect = **기존 유지 + 추가**(같은 서버 중복 금지 · `session.max_shared`) · 툴바 Disconnect ▾ · 유휴 닫기 `session.idle_secs` · 설정 6키 · 분기 10개 순수 함수 + MC/DC 테스트. 맥: 로그인 창 Details가 목록 크기를 유지한 채 오른쪽으로만 확장 · 텍스트박스 앞부분 표시 · 가로 스크롤 표시 자동 숨김. **⏳ 실기 T-125(52 §13 ①~⑮) · 결정 대기 D-96~100·102~108 · 다음 = T-121 탐색기 다중 서버 노드 · T-122 문지기 우회 불가화.** 미푸시.

## 2026-09-17 (55차 후반 · mac 밤) — macOS UI 글꼴 = 시스템 SF(파인더와 폭 일치) · 창 배치 규칙 3줄 구현(논리 좌표 · 숨긴 채 생성 후 표시 = 배율 결함 해결) · macOS 로그인 모달(자식 창 + 이동 잠금) · 창 크기/위치 기억 10키 · push

## 2026-09-17 (55차 · mac 밤) — ★ **T-100 macOS CoreText 글리프 경로 ✅**(nexa-gfx `coretext.rs` · 파인더와 같은 픽셀 · 기본 켬) · 1x 보조 모니터 ASCII 캡처 검증 · `window.monitor` · Rainbow Pairs 다운로드형(WASM) = 다음

## 2026-09-17 (54차 후반) — Enable Extension Manager · 기본 저장소 `extensions.default_repository`(GitHub 주소 → raw) · builtin도 설치해야 켜짐 · 열 선택 마우스 OS별(`editor.column_select`) · **macOS 글꼴 깨짐 = Windows GDI 모방 텍스트 키 → `OS_DEFAULTS`로 macOS 끔** · push

## 2026-09-17 (54차 · mac · 저녁) — ★ 확장 매니저 1차 ✅(저장소 = `extensions/` · 메타 v1 · Sublime식 팔레트 8명령 · sha256 · SxS) · plugins → extensions 개명 ✅ · Rainbow Pairs 배선(T-119) ✅ · 설정 그룹 Extensions + 끄면 분류 숨김 ✅ · 성능 점검 기준 50 §11 · "SDK 없이?" 50 §12

**⏳ 실기 = journal 54차 ①~⑥**. 미푸시(요청 시). → [journal](journal/2026-09-17.md)

## 2026-09-17 (53차 · mac · 저녁) — 저장소 최신화 ✅ · 52차 변경 맥 검증 green ✅ · 플러그인 관리 축 조사 [50 §8-4](50-extension-system.md) ✅ · 다음 = T-119 배선(51 §11) → T-103 캡처/위키

두 저장소 fast-forward · 빌드/테스트/clippy 전부 통과 · 매니저 보강 5건(`plugins.installed` 선언형 목록 · `platforms` · `messages` · 고정/프리릴리스 · `requires`) → T-118 ①. → [journal](journal/2026-09-17.md)

## 2026-09-17 (52차 · **win** · 이관 후 첫 세션) — ★ DR-33 결과 데이터 1세트+뷰 투영 ✅ · ★ 툴바 그룹 도크/플로팅 ✅ · ■ 중지 버튼 결함 ✅(사용자 확인) · 텍스트 보기 자동 페치 결함 ✅ · **전체 조회 = 나머지 이어 받기(위치 유지)** ✅ · 컬럼 최대 폭 24자(자동/직접) ✅ · 트랜잭션 로그 설계 [44](44-transaction-log.md) 📐(T-107 · §4 DBMS 공용성) · **실행 속도 향상 `perf.boost`** ✅([39 §4-6](39-resource-governance.md) · 배선 누락 점검 · 벤치 [45](45-perf-boost-benchmark.md)) · 툴바 트랜잭션 버튼(배지·상태색 · [44 §5](44-transaction-log.md)) ✅ · **설정 창 콤보 저장 누락 결함**(모든 Choice 설정) ✅ · 키 자동 반복 = 편집·이동만(Ctrl+T 80개 결함) ✅  · **Demo 프로필·샘플 데이터**(최초 1회 팝업 · 도움말 메뉴 · [21 §5](21-connection-profiles.md)) ✅ · GUI 실행 인자 접속 ✅ · 키 자동 반복 필터 ✅ · 탭 우클릭 메뉴 ✅ · 선택 글자 세로 중앙 ✅ · **★ 트랜잭션 로그 창 T-107 ✅**(nsql-run `txlog` · `txlog_win.rs` · Tx 열 롤백/커밋/암묵/끊김) · **동일 출현 상자 스타일**(1px 선·여백 · 인접 행 선 공유 · 모양/선/배경 설정 4키 · 색 창을 임의 색 키에 · `#RRGGBBAA` 알파 우선) ✅ · 미니맵 선택/출현 색 구분 ✅ + 조사·추천 [46](46-minimap-features.md) 📐(T-110) · 미니맵 기본 폭 160(옛 기본값 표) ✅ · **★ 객체 인텔리센스 설계 [47](47-intellisense-metadata.md) 📐 확정(D-79~86 · 설정 48키 · 유니버설 메타 모델 §3-1 · T-57 재정의)** · **★ 실행 상태 카드**(`runtoast.rs` · 경과/단계/속도/■ · `run.toast*`) ✅ · 결과 영역 = 결과만(오류·메시지 → 로그 창 · 기본 형태 유지) ✅ · Ctrl+D 캐럿 방향 ✅ · 미니맵 뷰포트 회색/테두리 설정 ✅ · 실행 대기열 설계 [43 §8](43-fetch-model-and-result-tabs.md) 📐(T-112 최하위) · **★ 실행 로그 상세 계측+개발자 모드+저비용 게이트 [48](48-logging-architecture.md)** ✅ · Sublime 커서 규칙(단어/서브워드/스마트 Home/Ctrl+클릭) ✅ · 다중 커서 문장 실행 차단 ✅ · 열 선택 Sublime 규칙 ✅ · **★ Auto Indent [49](49-auto-indent.md)** ✅ · **★ 실행 중지 = 실제 취소(`CancelHandle` · SQLite/PG/Oracle · 툴바 ■) T-108 ①** ✅ · SQL Server 취소(Attention/소켓 종료 · `mssql.encrypt`/`mssql.cancel` · PRELOGIN 탐침) ✅ · **데이터 일관성 엄격 모드 [43 §9](43-fetch-model-and-result-tabs.md)** ✅ · 자율 배치 1(미니맵 1순위 · Ctrl+↑/↓ · 탭 ▶ · 트랜잭션 로그 우클릭) ✅ · 배치 2(Ctrl+M/Ctrl+Shift+M 괄호 · 주석 뒤 들여쓰기 제외 · 텍스트 변환 시간 로그) ✅ · 배치 3(**T-57 ① MetaStore** · `strict_all` · `devlog` feature) ✅ · 복사 끝 줄바꿈 제거 ✅ · **줄 변경 표시(기준선 디프 · 거터 띠 · 향상 모드 off)** ✅ · 플러그인 시스템 검토 [50](50-extension-system.md) 📐(D-87~90 · T-118) · **첫 플러그인 = 레인보우 괄호+괄호 이동 [51](51-rainbow-brackets.md)**(D-91~95 확정 · nexa-ctl `PairTable`/`BracketOpts`/자동 닫기/이동 ✅ · nexa-sql `extensions/` 호스트 API+`rainbow.*` 8키 ✅ · **배선 = T-119 🚧 Mac**) · VS Code/Sublime 확장 모델 비교 [50 §8](50-extension-system.md) · push a71d009 · 0981985 · 6ce73f7(CI green · nexa-ui CI 결함 a6fe849 수정) · **→ Mac 이관** · 실행 중지/배지 T-108 · 포터블 방향 D-78 · 다음 = 실기 21항목 → T-110 1순위 → T-103 잔여 → T-102

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
