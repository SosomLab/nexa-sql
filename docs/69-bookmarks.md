# 69 · 북마크 관리 — 전수 조사 · 설계 · 결정 (사용자 요청 09-22)

> **요구**(사용자 09-22): *"데이터베이스 클라이언트, 일반 텍스트 편집기, 개발용 IDE 주요 제품을 전수 조사해서 북마크 관리 방법을 어떻게 구현하고 사용하고 있는지 조사해줘. ① 파일의 경우 기록한 북마크가 어떤 파일에 종속되었는지 어떻게 판단할 수 있는가 ② 기록한 북마크 위치가 줄번호 기준일텐데, 파일의 내용이 수정되면 무조건 줄번호로 지정할 것인지, 근처를 탐색해서 동일위치를 판단하고 연결할 것인지 ③ DBMS의 객체에 대한 북마크는 동일 객체명 기준으로 동일하게 볼 것인지, DBMS 별로 판단하는게 좋을지 ④ 유사하게 외부에서 내용이 변경된 경우 북마크 위치를 어떻게 동기화 할 것인지 ⑤ 불필요해진(삭제나 이름이 변경된 파일, 이동된 파일, Drop이나 Rename되어 연결이 끊어진) 북마크를 어떻게 관리해야 하는지, 자동 제거를 해야하는지, 일정기간동안 관리하다가 기간이후 점증적으로 제거 할 것인지 ⑥ 별도 파일로 관리해서 다른 기능과 완전 분리할 것인지, 프로젝트별(워크스페이스)로 프로젝트 파일에 기록 관리할 것인지."*
> **상태**: 📐 설계 확정(사용자 확인 09-22 · 결정 **D-156~D-177** §8) · 작업 **T-167**(§11) · 다른 세션에서 이 문서를 기준으로 개발.
> **선행·연계**: [67 프로젝트(워크스페이스)](67-project-workspace.md)(저장 위치의 주인 · T-165) · [58 외부 파일 변경](58-external-change-policy.md)(감시·병합 재사용) · [59 대용량 파일](59-large-file-handling.md)(상한) · [60 되돌리기 재설계](60-undo-redo-redesign.md)(`TextBuf` 줄 변경 기록) · [57 탐색기 갱신](57-explorer-refresh-after-ddl.md)(`ddl_target` 부활 신호) · [52 세션 컨텍스트](52-session-modes.md)(접속·게이트) · [47 메타 저장소](47-intellisense-metadata.md) · [39 자원 거버넌스](39-resource-governance.md)(부하원 등재) · [30 아키텍처 패턴](30-architecture-patterns.md)(포트+레지스트리).

---

## 0. 결론 열 줄

1. **"북마크"는 한 단어로 네 가지를 가리킨다** — ① 접속 즐겨찾기 ② DB 객체 즐겨찾기 ③ 쿼리 텍스트 저장 ④ 문서 안의 자리. 조사한 38종이 이 넷을 섞어 쓰고 있어 비교가 어려웠다. **우리는 ④만 "북마크"라 부른다**(§2-1).
2. **대상 = 편집창 문서 안의 자리**. 문서는 셋뿐이다 — **파일 탭**, **객체 DDL 탭**(탐색기에서 연 소스), **이름 없는 탭**. 사용자 정정(09-22): *"객체 자체에 대한 북마크는 필요없고, 객체의 내용 DDL 에 대한 편집창에서의 북마크만 고려하고 있음 … 결론적으로는 객체와 파일은 다른 대상임은 분명해"* → 탐색기 트리의 객체 핀/즐겨찾기는 **만들지 않는다**(§13 충돌·조정).
3. **위치 앵커는 4단**이다(JetBrains가 유일하게 실전 검증한 조합 + micro 플러그인의 유사도): ① 열린 동안은 `TextBuf`의 **줄 변경 기록**으로 정확히 따라간다 ② 다시 열거나 외부에서 바뀌면 **줄 정합(diff) → 줄 원문 검증 → 원래 줄 기준 양방향 탐색(정확 일치 → 유사도 0.6) → 무효** 순서 ③ 되돌리기는 편집의 역연산이므로 **원위치 복귀가 공짜**(VS Code 확장이 못 하는 것) ④ 큰 파일은 재탐색을 건너뛰고 줄 번호 + 표식.
4. **문서 동일성**: 파일 = 정규화 경로(프로젝트 안이면 **프로젝트 상대**) + 내용 해시(이동 탐지용). 객체 DDL = **접속 좌표(방언·호스트·포트·DB) + 스키마 + 종류 + 이름 + 부분**(계정·역할은 키에서 뺀다 — 같은 객체다). 이름 없는 탭 = **워크스페이스 탭 id**(67 §2-1에 이미 있다).
5. **저장은 프로젝트/워크스페이스에 얹는다**(별도 파일을 만들지 않는다) — **공용 = 프로젝트 파일**(VCS · 프로필 이름만 적는다) · **개인 = 워크스페이스 파일**(접속 좌표까지) · 프로젝트가 없으면 기본 워크스페이스. 저장소는 **포트 하나**(`BookmarkStore`)라 67의 T-165가 끝나면 그 backend만 갈아 끼운다.
6. **끊어진 북마크는 지우지 않고 무효로 강등**한다(회색 · 사유 표시). 파일이 돌아오거나 객체가 다시 생기면 **되살린다**. 무효가 된 지 `bookmark.stale_days`(기본 30일)가 지난 것만 조용히 정리한다. 조사한 모든 제품이 자동 정리를 갖고 있지 않았고(유일한 문서화 사례 AQT = "수동으로 지우세요"), 반대로 즉시 삭제(Eclipse·Qt Creator)는 VPN 끊김·브랜치 전환 같은 **일시적 부재에서 사용자 기록을 날린다**.
7. **UX는 한 번에 다 넣는다**(사용자 선택): 이름·메모 + 니모닉 0–9(문서별 유일) + 그룹(리스트) + 패널(타이핑 필터 + ↑↓) + 거터 아이콘 + 미니맵 틱 + 줄 끝 인라인 라벨 + 상황줄/배지. CLI는 `nsql bookmark list|add|rm|prune`.
8. **DDL 탭을 파일로 저장하면 북마크를 파일 쪽으로 복사하고 양쪽 다 유지**한다(사용자 요구). 객체와 파일은 다른 대상이므로 이동이 아니라 **복사**이고, 복사본은 출처(`origin`)를 기억한다.
9. **관리자는 좌측 활동 막대의 패널**이다(§6-3~6-9): 그룹 → 문서 → 항목으로 묶고, 한 번 클릭은 **미리보기**(포커스는 목록에 남아 ↑↓로 계속 훑는다), 더블클릭은 이동. 우클릭 메뉴는 **자리마다 다르다**(항목·무효·문서·그룹·빈 곳). 어려운 경우 30가지를 §6-8에 표로 박아 두었다 — 그중 셋은 규칙이 걸린 것이다: **미리보기는 절대 접속하지 않고**(C-4), **제거는 `Ctrl+Z`가 아니라 5초 실행 취소**이며(C-28), **공용으로 올릴 수 없는 것**(프로젝트 밖 파일 · 프로필 아닌 접속)은 막고 이유를 말한다(C-14·15).
10. **설정은 독립 영역**(§9 · 사용자 요청): 설정 창 목차에 그룹 `Bookmarks` 하나와 분류 셋(동작·저장 / 표시 / 위치 추적·정리), 키 38개. 정리 기준(며칠)·상한·유사도 문턱·미리보기·묶음·확인 여부까지 전부 값으로 뺐다.

---

## 1. 전수 조사

조사 방법: 공식 문서·매뉴얼, 제품 소스 코드 직접 열람(intellij-community · eclipse-platform · apache/netbeans · qt-creator · dbeaver · beekeeper-studio · squirrel-sql · alefragnani/vscode-bookmarks), 이슈 트래커(GitHub · YouTrack · SourceForge · Developer Community), 벤더 블로그. **조사 미확인** = 근거를 찾지 못한 것(추측하지 않는다).

### 1-1. 먼저 용어 — 한 단어가 네 가지

| 뜻 | 대표 제품의 UI 이름 | 우리 |
|---|---|---|
| ① 접속 즐겨찾기 | Sequel Ace **Favorites**, SSMS **Registered Servers**, Aqua **Shortcuts Toolbar**, Navicat **Add Star** | 이미 있다(연결 프로필 목록 · [21](21-connection-profiles.md)) — 북마크 아님 |
| ② DB 객체 즐겨찾기 | DBeaver **Bookmarks**(`.bm`), DataGrip **F11**, Toad **SB Favorites**, HeidiSQL **노란 별**, Beekeeper **Pin**, Navicat **Favorites**, AQT **Favorites** | **만들지 않는다**(사용자 09-22 · §13) |
| ③ 쿼리 텍스트 저장 | DbVisualizer **Bookmark**(= 스크립트), SQuirreL **SQL Bookmark**, TablePlus **Favorite**, Workbench **Snippets**, RazorSQL **SQL Favorites** | 범위 밖(스니펫·팔레트와 겹친다 · §14) |
| ④ **문서 안의 자리** | SSMS·SQL Developer·Toad·Aqua·DataGrip·VS·Eclipse·Sublime·Notepad++·Vim·Emacs | ★ **이 문서의 "북마크"** |

②와 ④를 **둘 다 1급으로 갖춘 제품은 DataGrip 하나뿐**이고, 나머지는 한쪽만 있으며 다른 쪽은 수년째 요청 상태다(DBeaver #7877·#8560 편집기 줄 북마크, pgAdmin #1714 객체 즐겨찾기 = not planned, Azure Data Studio #13645 = Backlog).

### 1-2. DB 클라이언트

| 제품 | 편집기 줄 북마크 | 객체 즐겨찾기 | 저장 위치 | 범위 | 객체 식별 키 |
|---|---|---|---|---|---|
| **DBeaver** | ✗ 정식 기능 아님(Eclipse `BookmarkView` · 24.3.3 회귀 #36924) | ✅ `.bm` XML 1파일/1북마크 + base64 아이콘 | 프로젝트 `Bookmarks/` | **프로젝트**(Git·Team 공유 ✅) | datasource **id**(안정) + 노드 **이름** 경로(rename에 깨짐) |
| **DataGrip** | ✅ 익명 + 니모닉(0-9·A-Z) · 리스트 · 설명 | ✅ Explorer에서 F11 | `<IDE config>/workspace/<id>.xml` | IDE 전역(`.idea` 불가 = 공유 불가) | 조사 미확인 |
| **SSMS / VS 셸** | ✅ Bookmarks 창 · **가상 폴더** · **체크박스로 비활성** | ✗(필터만 · 접속은 Registered Servers) | `.suo`(바이너리 · 내보내기 없음) | 솔루션·사용자 | — |
| **Oracle SQL Developer** | ✅ 번호 0-9 + 무번호 · ★ **앵커·영속화를 설정으로 노출**(JDeveloper 계승 — persistence · 순회 · "없어진 줄" 처리) | ✗(Reports · Recent Objects로 우회) | `%APPDATA%\SQL Developer`(파일명 조사 미확인) | 조사 미확인 | — |
| **Toad for Oracle** | ✅ 번호 0-9(거터에 번호) · **워크스페이스에 저장·복원** | ✅ SB Favorites(폴더) — ★ 범위가 **"instance 기준(접속·스키마 아님)"** | `User Files\Databases\<db>\SBFavorites.txt` | **DB(인스턴스)별** | 이름 |
| **HeidiSQL** | ✅ marker 0-9(`Ctrl+Shift+N` 지정 / `Ctrl+N` 이동) | ✅ 노란 별 + `Show only favorites` | 레지스트리 `…\Servers\<session>\FavoriteObjects` | **세션(서버)별** | 이름 목록(그룹 불가 — 개발자 거절) |
| **Beekeeper Studio** | ✗ | ✅ Pin | SQLite `app.db` 테이블 `pins` | 저장된 접속별 | ★ **`(connectionId, databaseName, schemaName, entityName, entityType)`** = 가장 잘 구조화된 이름 키 |
| **DbVisualizer** | ✗ | ✅ Favorites(Pro · ★ 클릭하면 **자동 접속**) | `~/.dbvis/Bookmarks/<name>` + `<name>.met` | 사용자 전역 | 접속 포함 |
| **AQT** | 조사 미확인 | ✅ 폴더 · 한 객체가 여러 폴더 소속 | `fav<dbname>.txt` | DB별 | ★ **내부 object id**(MSSQL·Sybase) · ★ **스테일을 문서화한 유일한 제품**("목록에 남고 상세가 빈다 — 수동으로 지우세요") |
| **Aqua Data Studio** | ✅ 마커 바 tick + Lens 미리보기 | △ Query Builder 전용 | `~/.datastudio` · `.QBW` | 전역 ↔ 파일 선택 | 이름(table·schema·database) |
| **Navicat** | ✗ | ✅ 객체 **경로 링크** · `Shift+Ctrl+#` 지정 / `Ctrl+#` 이동(**9칸 고정 · 폴더 없음**) | 조사 미확인 | 접속별 | 조사 미확인 |
| **SQL Workbench/J** | ✅ ★ **`-- @WbTag` 주석**(저장하지 않고 본문에서 도출) | ✗ | **저장 안 함** | 열린 탭 | — |
| **pgAdmin 4** | ✗ | ✗ — #1714 **not planned** · #1437 "Macros로 충족" · ★ pgAdmin **III에는 있었다**(퇴행) | — | — | — |
| **MySQL Workbench** | ✗(EER 다이어그램 마커만) | ✗ | Snippets 평문 파일 | 전역 | — |
| **SQuirreL SQL** | 조사 미확인 | ✗ | `~/.squirrel-sql/plugins/sqlbookmark/bookmarks.xml` | 전역 | 북마크 = SQL 템플릿(`_name`,`_description`,`_sql`뿐 → **스테일 개념 자체가 없다**) |
| **TablePlus · Sequel Ace · Azure Data Studio · Golden** | ✗ | △ Pin 정렬 / ✗ / ✗ / 조사 미확인 | — | — | — |

### 1-3. 개발 IDE

| 제품 | 종류 | 그룹 | 설명·노트 | 저장 위치 | VCS 공유 | 끊어졌을 때 |
|---|---|---|---|---|---|---|
| **JetBrains**(IntelliJ·DataGrip·AS) | 익명 · 니모닉 36(0-9·A-Z · **프로젝트 전역 유일** · 중복 시 "가져올까요 + 다시 묻지 않기") · 라인/파일/폴더/**모듈**/스크래치 루트 | ✅ **Bookmark Lists**(기본 리스트 지정 · 드래그 · 한 북마크 다중 소속) | ✅ `Edit Description`(기본값 = 그 줄 원문) | `<IDE config>/workspace/<projectWorkspaceId>.xml`(`.idea` **아님**) | ✗ (벤더 답변: 프로젝트로 옮길 방법 없음) | **`InvalidBookmark`로 강등 · 자동 삭제 없음 · 파일이 돌아오면 부활 시도** · 탈색 아이콘 + 회색 텍스트 |
| **Visual Studio** | 익명 + **체크박스 비활성**(Disable All/Enable All) | ✅ 가상 폴더(폴더 내 next/prev 단축키까지) | 이름(Rename) | `.vs/<Sln>/v17/.suo` 바이너리 | ✗ · 내보내기 ✗ | 조사 미확인 · 대신 **"북마크가 사라진다"** 불만이 반복(권한·`.suo` 손상) |
| **Eclipse** | `IMarker`(`bookmark` 타입 · 리소스 어디에나) | ✗ | ✅ `IMarker.MESSAGE` | `<ws>/.metadata/…/.markers`(+`.snap`) | ✗ | **리소스 삭제 = 마커도 삭제** · 저장 시 범위가 지워졌으면 **제거**(`BasicMarkerUpdater`) · `IResource.move()`엔 따라감 |
| **VS Code** | **내장 없음**(#270923 `*as-designed`) · 확장 Bookmarks(alefragnani)가 표준 | ✗ | ✅ 라벨 + **줄 끝 인라인 표시** | `workspaceState` 또는 ★ **`.vscode/bookmarks.json`**(`saveBookmarksInProject`) | ✅ **유일하게 공유를 설계** | 줄 삭제 시 제거(옵션으로 다음 줄 · **Undo 미지원**) · **외부 변경·git pull·브랜치 전환은 추적 못 함**(메인테이너 명시) |
| **Xcode 15+** | 라인 · 파일 · ★ **검색 질의**(수동 새로 고침) · ★ **완료 체크박스** | ✅ 그룹 | ✅ | `…/xcuserdata/<user>.xcuserdatad/Bookmarks/bookmarks.plist` | ✗ | 조사 미확인 |
| **NetBeans** | 이름 + 단일 문자 키 | ✗ | ✅ name | 프로젝트 **private** 설정(ns `editor-bookmarks/2`) | ✗ | **유지** + `<Non-existent File>` · `Bookmarked line (number N) does not exist.` |
| **Qt Creator** | 노트 + ★ **드래그로 다른 줄에 옮기기** · Edit 대화상자의 줄 번호 스핀박스 | ✗ | ✅(툴팁 + 줄 주석) | **세션** `.qws`(`":"+경로+":"+줄+"\t"+노트`) | ✗ | 그 줄이 사라지면 **북마크 자체 삭제** · 파일 경로 변경은 추적(`updateFilePath`) |

### 1-4. 텍스트 편집기

| 제품 | 앵커 | 지속성 | 이름·메모 / 그룹 | 특징 |
|---|---|---|---|---|
| **Sublime Text** | 리전(`view.get_regions("bookmarks")`) — 편집을 따라감 · 토글 시점의 **선택 영역 그대로** | ★ **남지 않는다** — API에 `sublime.PERSISTENT`가 있는데 내장 북마크는 쓰지 않는다. 파일을 닫으면 소멸(미해결 #6589) | ✗ / ✗ | `Ctrl+F2` 토글 · `F2`/`Shift+F2` 이동 · `Ctrl+Shift+F2` 전체 지우기 · ★ `Alt+F2` **Select All Bookmarks = 모든 북마크에 멀티커서** — **키맵은 여기서 가져오되 지속성은 여기서 배울 것이 없다**(DR-5) |
| **Notepad++** | Scintilla 마커 20번 = **줄 단위** · 줄을 지우면 **다음 줄로 승계**(마커 OR 병합) | 세션 한정 — `session.xml`의 `<Mark line="N"/>`(**줄 번호만** · 종료 때 한 번 씀 · 파일 하나만 닫으면 그 북마크는 소멸) | ✗ / ✗ | ★ **디스크에서 다시 읽으면 전멸**(`SCI_CLEARALL` · #10318 2021년부터 Open) — 우리 요구 ④가 실제로 아픈 지점 · `Copy/Cut/Remove Bookmarked Lines` · `Remove Non-Bookmarked Lines` |
| **Vim / Neovim** | `'a`–`'z`·`'A`–`'Z` — 편집마다 `mark_adjust`로 수동 보정 · **그 줄을 지우면 소문자는 무효, 대문자는 삭제 시작 줄로 접힘** · 되돌리기로 복원 | `viminfo`/`shada`(**줄·열 번호만 · 문맥 없음** · `'100` = 최근 파일 100개) | ✗ / ✗ | ★ **디스크 재적재(`buf_reload`)는 마크를 조정하지 않는다** → 줄 번호가 내용과 어긋난 채 남는다 · ★ shada는 종료 시 **타임스탬프로 병합**(§14 공용 파일 병합의 본보기) · extmark는 marktree 앵커로 완전 스티키지만 **세션을 넘기지 못한다** |
| **Emacs `bookmark.el`** | ★ **문자 오프셋 + 앞뒤 문맥 문자열 각 16자**(`front-context-string`/`rear-context-string` · `bookmark-search-size`) | `~/.emacs.d/bookmarks`(사람이 읽는 평문 · 31.1부터 `.eld`) | ✅ `annotation` / ✗ | ★ **내용 앵커의 원조**: ① 저장 오프셋으로 점프 ② 앞문맥을 **버퍼 끝까지** 전진 탐색 ③ 뒷문맥을 **버퍼 앞까지** 후진 탐색 — **탐색 범위 제한이 없다** · 실패해도 조용히 그 자리 · 파일이 없으면 **대화식 relocate 프롬프트** · 소스 주석: 사이에 글이 끼면 **그 앞에 선다**("읽을 수 있게") |
| **micro**(공식 채널 플러그인) | ★ **줄 텍스트 + 앞뒤 이웃 줄** → 재열기 때 **Sørensen–Dice 바이그램 유사도 ≥ 0.6**으로 재배치 · 스캔 ±2,000줄 · 실패 시 줄 번호로 폴백("저장된 줄 번호는 힌트일 뿐") | 플러그인 저장소 | ✅ 라벨 + A–Z 니모닉 / ✅ 이름 있는 리스트 | ★ **우리 §3-2와 거의 같은 설계가 이미 있다** — 정확 일치가 아니라 **유사도**를 쓰는 점이 우리보다 낫다(D-171) |
| **Kate / KWrite** | 줄 · ★ 재적재 전에 `{줄 번호, 그 줄 텍스트}`를 저장하고 **줄 내용이 같을 때만** 복원 · 영속(`katemetainfos`)도 문서 **체크섬이 다르면 통째로 버린다** | 메타인포(수정되지 않은 문서만) | ✗ / ✗ | 우리 `anchor.doc_hash` 판정과 같은 생각 — 다만 **다르면 버리는** 쪽(우리는 재탐색한다) |
| **Zed** | ★ 메모리에선 `text::Anchor`(Lamport 타임스탬프 + offset + bias)로 완전 스티키 · **디스크에는 `row` 정수로 환원** | 워크스페이스 SQLite `bookmarks(workspace_id, path, row, label)` | ✅ 라벨 / ✗ | 우리 L0(줄 변경 기록)과 같은 층 · 디스크 앵커가 없어 외부 변경엔 약하다 |
| **CudaText** | 줄 번호(`LineNum`) | `settings/bookmarks<SUFFIX>.json` | ✅ `Hint` 툴팁 / ✅ **`Tag`로 묶어 일괄 삭제** + `Kind` 1–63(번호 아이콘) | 그룹 일괄 삭제는 우리 그룹 운용의 참고 |
| **UltraEdit** | 줄+열 번호 | 제품 설정 | ✅ 이름 **20자** + hover 툴팁 / 부분(검색으로 단 북마크만 따로 지우기) | 이름을 **달 때 물어보는** UI(우리는 나중에 달 수 있게) |
| **Geany · EmEditor · Textadept** | 줄(Scintilla 마커) | Geany 코어는 **세션에 저장 안 함**(#1269 Open · 번호 북마크는 플러그인) · Textadept는 줄 번호만 | ✗ / ✗ | — |
| **Helix** | — | — | — | ★ **북마크 기능이 없다**(#703 2021~ Open) — jumplist를 편집마다 `selection.map(changes)`로 재맵하는 것이 전부 |

> 편집기에서 가져오는 것 셋: **키맵(Sublime)** · **문맥 앵커(Emacs)** · **유사도 재배치와 스캔 상한(micro)**. 피할 것 둘: **재적재에서 전멸**(Notepad++) · **닫으면 소멸**(Sublime 내장).

### 1-5. 위치 앵커링 비교 — 이 설계의 핵심

| 방식 | 제품 | 편집 중 | 다시 열 때 | 외부 변경·브랜치 전환 |
|---|---|---|---|---|
| **줄 번호만** | 다수(SSMS `.suo`, Navicat, HeidiSQL 추정) | 에디터가 따라감 | 저장된 숫자 그대로 | ✗ 어긋남 |
| **산술 보정** | VS Code 확장 | 삽입/삭제 줄 수만큼 ± · 특수 케이스 핸들러(자동 들여쓰기·Move Line) | 상대 경로 + 줄 | ✗ **추적 안 함**(메인테이너 명시) · **Undo 미지원** |
| **문서 위치 객체** | Eclipse `Position` · NetBeans `Line` · Qt Creator `TextMark` | 정확 | 저장 시점에 확정 | ✗(닫힌 파일은 갱신 계기 없음) |
| ★ **다단 방어** | **JetBrains** | `RangeMarker`/거터 하이라이터 | `expectedText`(줄 원문) 검증 | **`translateLineViaDiffStrict` diff 매핑 → 텍스트 검증 → 원래 줄 기준 양방향 탐색 → `InvalidBookmark`** · VFS 이벤트마다 100ms 디바운스 재검증(부활 포함) |
| ★ **문맥 문자열** | **Emacs** | — | 앞뒤 각 16자 문맥으로 재배치 | 같은 방식(탐색 범위 제한 없음 · 실패해도 조용히) |
| ★ **유사도 재배치** | **micro 플러그인** | — | 줄 텍스트 + 이웃 줄 · **Sørensen–Dice ≥ 0.6** · ±2,000줄 | 같은 방식 — 정확 일치보다 강하다 |
| **내용 검증 후 버리기** | **Kate** | 정확 | 줄 텍스트가 같을 때만 복원 | 체크섬이 다르면 **통째로 폐기**(안전하지만 아깝다) |
| **메모리 앵커 + 디스크는 줄 번호** | **Zed** | 완전 스티키(`text::Anchor`) | `row` 정수 | ✗ 외부 변경엔 줄 번호뿐 |
| ★ **앵커를 아예 두지 않음** | **SQL Workbench/J `@WbTag`** | 본문이 곧 앵커 | 완벽 | **완벽**(git 공유도 공짜) · 대가 = 읽기 전용·남의 파일엔 못 씀 |

**벤더가 인정한 문제**: Microsoft는 Bookmark Studio를 내며 *"drifting to the wrong line"* 을 고치겠다고 했고, JetBrains에는 재오픈 시 5~6줄 밀림 티켓(IJPL-52732)이 있으며, VS Code 확장의 sticky 엔진은 여전히 실험 옵션이다. **즉 이 영역은 모든 제품의 약점이고, 제대로 하면 차별점이 된다.**

### 1-6. 끊어진 북마크 처리 비교

| 처리 | 제품 | 평가 |
|---|---|---|
| 강등 후 보관 + 부활 시도 | JetBrains | ★ 채택 |
| 남겨 두고 "수동으로 지우세요" | AQT(문서화한 유일한 제품) · NetBeans | 목록이 지저분해진다 |
| 클릭할 때 오류 카드 | DBeaver(`Can't find datasource '<id>'`) | 사유는 친절, 표시는 없음 |
| 회색 + 빨간 막대 | Aqua(미접속 Favorites) | 표시 방식만 차용 |
| 즉시 삭제 | Eclipse · Qt Creator | ✗ 일시적 부재에 사용자 기록 소실 |
| **자동 만료·정리** | **없음(전무)** | ★ 우리가 처음 — 무효가 된 시각을 갖는 제품조차 없다 |

### 1-7. 저장 위치의 선택이 사고를 만든다 (반면교사 8)

① 캐시 폴더 저장 → 청소 도구가 지운다(TablePlus #1944) ② IDE 전역 설정에만 → 팀 공유 영구 불가(DataGrip) ③ `.suo` 바이너리 → 내보내기·복구 불가 + 소실 불만(VS·SSMS) ④ 전역/문서 **이중 범위** → "사라졌다"는 체감(Sequel Pro #1645 · Ace #1885) ⑤ 알파벳 정렬 고정(DbVisualizer) ⑥ 정렬 없는 핀 누적(Beekeeper #1498) ⑦ 접속별 그룹 없음·펼칠 수 없음(DBeaver #22090) ⑧ 이름 없는 번호 슬롯만(SQL Developer·Toad·HeidiSQL).
**git 공유를 자연스럽게 얻은 것은 둘뿐** — DBeaver `.bm`(프로젝트 파일)과 `@WbTag`(본문 내장). VS Code 확장의 `saveBookmarksInProject`가 세 번째다.

---

## 2. 우리 모델

### 2-1. 무엇을 북마크라 부르는가

> **북마크 = 편집창 문서 안의 한 자리**(줄) + 이름·메모·니모닉·그룹.

접속 즐겨찾기(연결 프로필), 객체 즐겨찾기(만들지 않음), 쿼리 저장(스니펫·팔레트)은 북마크가 아니다. 이 경계를 UI 문자열과 설정 키에서도 지킨다(`bookmark.*`).

### 2-2. 문서 세 가지 — `DocKey`

| 문서 | 언제 | 열쇠 | 지속 |
|---|---|---|---|
| **파일 탭** | 디스크의 `.sql` 등 | 정규화 경로(프로젝트 안 = 프로젝트 상대 · 밖 = 절대) | 영구 |
| **객체 DDL 탭** | 탐색기에서 소스 열기(`ExplorerAction::OpenSql`) | 접속 좌표 + 스키마 + 종류 + 이름 + 부분 | 영구(문서는 열 때마다 다시 조회) |
| **이름 없는 탭** | 새 스크립트 | 워크스페이스 **탭 id**(67 §2-1 `tabs[].id`) | 그 워크스페이스가 사는 동안 |

★ **객체와 파일은 다른 대상이다**(사용자 09-22). 같은 프로시저의 DDL 탭과 그것을 저장한 `.sql` 파일은 **서로 다른 문서**이고, 북마크도 각각 존재한다(§2-4 복사 규칙).

### 2-3. 자료구조(`nsql-bookmarks` · 의존 0)

```rust
pub struct BookmarkId(pub u64);               // 저장소 안에서 단조 증가 · 재사용 없음
pub struct GroupId(pub u32);

pub enum DocKey {
    File   { path: String },                                   // 정규화 · Windows는 비교만 대소문자 무시
    Object { server: ServerKey, schema: String, kind: ObjKind, name: String, part: SourcePart },
    Scratch{ tab: u64 },                                       // 67 워크스페이스 탭 id
}

/// 접속 좌표 — 비밀번호·계정·역할은 **넣지 않는다**(같은 객체를 다른 계정으로 봐도 같은 북마크).
/// `worker::same_server`와 축이 같되 `user`/`role`을 뺀 것 — 그 차이를 코드 주석에 남긴다.
pub struct ServerKey {
    pub dialect: String,                  // "oracle" | "mssql" | "postgres" | "sqlite"
    pub host: Option<String>,             // 소문자 · 공용 저장에는 **쓰지 않는다**(D-165)
    pub port: Option<u16>,
    pub database: Option<String>,         // Oracle 서비스명 · MSSQL/PG DB명 · SQLite 파일 경로
    pub profile: Option<String>,          // 표시용 · 공용 저장의 유일한 좌표
}

pub enum SourcePart { Source, Spec, Body }   // Oracle PACKAGE/BODY 등. 기본 Source.

pub struct Anchor {
    pub line: u32,                 // 0-base — 마지막으로 확인된 자리
    pub col: u32,                  // 캐럿 복원용(판정에는 쓰지 않는다)
    pub text: String,              // 그 줄 원문(상한 `bookmark.anchor_chars`)
    pub before: String,            // 바로 위 줄 원문(문맥) — Emacs front-context
    pub after: String,             // 바로 아래 줄 원문(문맥) — Emacs rear-context
    pub doc_hash: u64,             // 그때 문서 전체 해시(FNV-1a · 같으면 재탐색 생략)
    pub doc_lines: u32,            // 그때 줄 수(탐색 범위 보정)
}

pub struct Bookmark {
    pub id: BookmarkId,
    pub doc: DocKey,
    pub anchor: Anchor,
    pub label: Option<String>,     // 이름(없으면 목록에 줄 원문을 보여 준다 — JetBrains 관례)
    pub note: Option<String>,      // 메모(여러 줄 · 상한 `bookmark.note_chars`)
    pub mnemonic: Option<u8>,      // 0–9 · **문서별 유일**(D-162)
    pub group: GroupId,
    pub shared: bool,              // true = 프로젝트 파일(공용) · false = 워크스페이스(개인)
    pub state: State,
    pub origin: Option<DocKey>,    // 복사본의 출처(DDL → 파일 저장 시 · D-167)
    pub created: u64,              // epoch s
    pub visited: u64,              // 마지막으로 이동한 시각(정리·정렬 기준)
}

pub enum State {
    Live,
    Invalid { since: u64, reason: Reason },   // ★ 조사한 어느 제품도 이 시각을 갖지 않는다
}
pub enum Reason { FileMissing, ObjectMissing, TextGone, ServerUnknown, TooBig }

pub struct Group { pub id: GroupId, pub name: String, pub default: bool, pub enabled: bool }
```

`enabled`는 SSMS의 체크박스(지우지 않고 순회에서만 제외)를 그룹 단위로 가져온 것이다 — 조사한 제품 중 **그룹 토글을 가진 곳은 없었다**.

### 2-4. DDL 탭 → 파일 저장(사용자 요구 "복사본을 전달")

DDL 탭에서 **Save As**로 파일을 만들면:

1. 그 문서의 북마크를 **파일 문서 키로 복사**한다(`origin = Object{…}` · 새 `id`).
2. 원본(객체 DDL) 북마크는 **그대로 남는다** — 사용자 선택 "동시에 둘 다 유지"(D-167).
3. 복사본 목록 행에는 출처 칩 `⟵ ORCL:HR.PKG_X(BODY)`를 옅게 보여 준다(클릭 = 원본으로).
4. 반대 방향(파일 → 객체)은 하지 않는다. 파일을 실행해 객체를 만든다고 해서 자리가 같다는 보장이 없다.

---

## 3. 위치 앵커 — 4단 방어

### 3-1. L0 · 열린 동안 (정확)

편집 버퍼는 nexa-ctl `TextBuf`이고 **줄 변경 기록**(`LineChange{first, removed, inserted}` · `changes_since(seq)`)을 이미 갖고 있다(60 · 84차). 북마크는 **줄별 그리기 캐시와 같은 소비자**가 된다.

```text
for c in buf.changes_since(seq):            # 없으면(기록이 밀렸거나 epoch 변화) → L1
    if bm.line < c.first:              변화 없음
    elif bm.line >= c.first + c.removed:  bm.line += c.inserted - c.removed      # 뒤로 밀림
    else:                                   # 지워진 구간 안이었다
        if c.inserted > 0: bm.line = c.first + min(bm.line - c.first, c.inserted - 1)   # 대치 편집 = 제자리
        else:              bm.line = c.first; bm.shifted = true                   # 줄 삭제 = 경계로
```

- **되돌리기는 공짜다**: 되돌리기는 같은 통로(`splice_rec`/`apply_ops`)로 역연산을 내므로 줄 변경 기록도 역으로 나온다 → 위 식이 그대로 원위치로 되돌린다. VS Code 확장이 *"does not support Undo operations"* 라고 적은 지점이 우리에겐 문제가 아니다.
- 앵커 텍스트는 그 줄이 편집에 닿았을 때만 갱신한다(`bookmark.anchor_update` 기본 on · 디바운스 2초 — 타이핑마다 쓰지 않는다).
- `buf.epoch()`가 바뀌면(통째 교체 = 외부 변경 채택 · 큰 편집) 기록이 끊긴다 → **L1**.

### 3-2. L1 · 다시 열기 · 재로드 · 외부 변경 (재탐색)

입력: 옛 앵커, 새 본문(줄 배열), 있으면 옛 본문.

```text
1. anchor.doc_hash == hash(새 본문)          → 그대로 확정(비용 0 · 가장 흔한 경로)
2. 옛 본문이 있으면: line_map = merge3::line_map(옛, 새)   # Myers 줄 정합(신설 · §11 U-1)
   cand = line_map[anchor.line]              # 없으면(삭제된 줄) cand = 근처 정합 줄
   ※ 크레이트 경계: `nsql-bookmarks`는 **의존 0**이라 nexa-ctl을 부르지 않는다 —
     재탐색 함수는 `map: Option<&[Option<usize>]>`를 **인자로** 받고, 그 값을 만드는 쪽(UI)이
     `merge3::line_map`을 쓴다(CLI·테스트는 `None`으로 부른다).
3. cand(없으면 anchor.line)의 줄 원문 == anchor.text       → 확정
4. 양방향 탐색: d = 1,2,3,… bookmark.search_lines(기본 400)
      위/아래 번갈아 보며 ① 줄 원문 정확 일치 → 즉시 확정
                         ② 아니면 **유사도 점수**를 기억:
                            sim(text) × 2 + sim(before) + sim(after)      # sim = Sørensen–Dice 바이그램(0.0~1.0)
      점수가 bookmark.similarity(기본 0.6 · 0 = 정확 일치만) × 4 이상인 것 중 최대
   여러 후보가 같은 점수면 **원래 줄에 가까운 쪽**
5. 그래도 없으면: anchor.text가 문서 전체에서 **유일**하면 그 줄(`bookmark.relocate_unique` 기본 on)
6. 실패 → State::Invalid{ reason: TextGone }
```

- 빈 줄·공백만 있는 앵커는 3~5를 건너뛴다(어디에나 있어 잘못 붙는다) — 줄 번호 + 무효 아님, `shifted` 표식만.
- 비교는 `bookmark.anchor_trim`(기본 on)이면 좌우 공백을 접고 비교한다(포매터·들여쓰기 변경에 강해진다).
- 확정될 때마다 `anchor`(text·before·after·doc_hash·doc_lines)를 갱신한다.
- **상한**: 문서가 큰 파일 모드 L2이거나 줄 수 > `bookmark.relocate_max_lines`(기본 200,000)면 2~5를 건너뛰고 줄 번호 그대로 + `TooBig` 표식(59 §4 — 오래 걸리는 일은 그 탭 하나에 가둔다).
- 실행 시점: **그 문서를 열 때 · 외부 변경을 채택할 때(`extfile.rs` Reload/Merge) · DDL을 다시 조회할 때**. 앱 시작 때 전부 돌지 않는다(문서를 열지 않고는 본문이 없다).

### 3-3. L2 · 문서 자체가 없을 때

| 상황 | 판정 | 신호 |
|---|---|---|
| 파일 없음 | `Invalid{FileMissing}` | 탭 열기 실패 · `extfile.rs` 감시(58 §5 `FileSig`) |
| 파일이 옮겨짐·이름 바뀜 | §4-1 | 앱 안 = 즉시 갱신 · 밖 = 제안 |
| 객체 없음(Drop) | `Invalid{ObjectMissing}` | DDL 조회 실패 · 우리가 실행한 `DROP`을 `ddl_target`이 잡음(57) |
| 객체 rename | `Invalid{ObjectMissing}` + 제안 | `ddl_target`의 `Rename`(옛 이름 → 새 이름) — 같은 서버·스키마·종류면 **"이어 붙일까요"** 한 줄 |
| 서버 프로필이 사라짐 | `Invalid{ServerUnknown}` | 저장소를 열 때 |

### 3-4. L3 · 부활 (조사에서 JetBrains만 하는 것)

무효 북마크는 **다음 신호에서 다시 검증**한다 — 새 타이머를 만들지 않고 이미 있는 것에 얹는다(39 §3):

- 그 문서를 열 때(가장 흔함) · `extfile.rs` 감시 틱이 그 경로의 파일이 **다시 나타났다**고 알릴 때 · `ddl_target`이 같은 이름의 `CREATE`를 볼 때 · 탐색기 갱신(57 T2 워터마크)이 그 객체를 다시 찾았을 때 · 저장소를 열 때.
- 되살아나면 `State::Live`로 돌리고 L1 재탐색을 한 번 돌린다.

### 3-5. 큰 파일·읽기 전용

- 큰 파일 모드 L1 = 정상 동작(재탐색 상한만 적용) · L2 = 줄 번호만(재탐색 없음) · 읽기 전용 탭 = 북마크 가능(읽기만 하는 로그에 표시를 남기는 것이 Notepad++ 사용자의 대표 사용법).

---

## 4. 문서 동일성 — "어떤 파일에 종속되었는가"(요구 ①)

### 4-1. 파일

- **키 = 경로**. 프로젝트 안이면 **프로젝트 파일 기준 상대 경로**(67 §2-1 규약 · 저장소를 다른 PC로 옮겨도 산다), 밖이면 절대 경로. 저장은 `/` 구분자로 정규화(Windows `\` → `/`).
- 비교는 `std::fs::canonicalize` 후 Windows에서 **대소문자 무시**(`varsfile.rs`/`undofile.rs`의 `name_for`와 같은 규칙 · 09-22 프로필 사고의 교훈).
- **앱 안에서의 이동·이름 변경은 즉시 따라간다**(파일 대화상자 · 탐색기 · Save As 경로가 저장소의 경로를 그 자리에서 고친다 — Qt Creator `updateFilePath` · VS Code `onDidRenameFiles`에 대응).
- **밖에서의 이동은 제안한다**(사용자 D-163): 무효(`FileMissing`)가 된 북마크의 `anchor.doc_hash`와 **같은 해시의 파일을 열었을 때** 탭 상단 띠 한 줄 —
  `이 내용에 붙어 있던 북마크 3개가 있습니다(옛 경로 …/old.sql).  [이어 붙이기]  [아니오]`
  해시는 파일을 열 때 이미 계산된다(`nexa-fs::content_hash`) → **추가 비용 0**. 자동으로 갈아끼우지 않는다(라이브러리 사본·백업 파일을 잘못 짚을 수 있다).

### 4-2. 객체 DDL 문서 (요구 ③)

> **DBMS별로 판단한다**(사용자 D-159). 같은 이름이라도 **서버가 다르면 다른 북마크**다.

- 키 = `ServerKey{dialect, host, port, database} + schema + kind + name + part`.
- **계정(user)·역할은 키에서 뺀다** — 같은 테이블을 다른 계정으로 봐도 같은 객체다(`same_server`와 다른 점 · 코드 주석에 명시).
- 이름 접기: Oracle은 따옴표 없는 식별자를 **대문자**로, PostgreSQL은 **소문자**로, SQL Server·SQLite는 원문 보존 + 비교만 대소문자 무시(`ddl_target`의 규칙과 같은 표를 쓴다).
- PG 오버로드 함수는 지금 탐색기가 쓰는 **서명 포함 이름**을 그대로 `name`에 넣는다(`open_source`).
- **개발/시험/운영이 같은 이름을 쓴다**는 것이 이 선택의 이유다(`ConnectSpec.env` · PROD 칩). dev에서 만든 북마크가 prod 창에서 열리면 오접속이다.
- 배선: `ExplorerAction::OpenSql`에 `origin: Option<ObjOrigin>`를 더하고, 호스트가 새 탭을 만들 때 `Editors.doc_keys: HashMap<u64, DocKey>`에 넣는다(`view_tabs`와 같은 자리 · 탭 id 열쇠).
- **재개**(패널에서 무효 아닌 객체 북마크 클릭): 그 서버에 세션이 있으면 바로 조회해 열고, 없으면 **한 지점 확인** — `BISCM에 연결하고 HR.PKG_X(BODY)를 열까요?` → 예일 때만 접속(자동 재접속 금지 · 26 §8 · PROD면 운영 칩 + `tx.prod_*` 규칙과 같은 문구). 조사에서 DbVisualizer Favorites는 **묻지 않고 접속**하는데, 우리 규칙과 충돌하므로 따르지 않는다.

### 4-3. 이름 없는 탭

워크스페이스의 `tabs[].id`(67 §2-1)를 그대로 쓴다. 그 탭이 파일로 저장되면 북마크의 `doc`을 `File`로 **바꾼다**(복사가 아니라 이동 — 같은 문서가 이름을 얻은 것이다). 워크스페이스에서 탭이 사라지면 그 북마크도 사라진다(본문이 없으므로 무효 보관이 무의미).

---

## 5. 수명 · 정리 (요구 ⑤)

| 단계 | 언제 | 보이는 모습 |
|---|---|---|
| **유효** | 자리를 찾음 | 거터 아이콘 · 목록 정상 |
| **무효(회색)** | 파일·객체·줄이 없음 | 탈색 아이콘 + 회색 글자 + 사유 칩(`파일 없음` `객체 없음` `줄 사라짐` `서버 모름`) · 클릭하면 사유와 마지막 본 자리(줄 원문)를 보여 준다 |
| **정리** | 무효가 된 지 `bookmark.stale_days`(기본 30일) | 조용히 사라짐 · 로그(48)에 `북마크 n개 정리` 한 줄 |

- 정리 시점: **저장소를 열 때 1회** + **하루 1회 유휴**(`varsfile::prune_in`과 같은 패턴 · 앱이 유휴일 때만).
- `bookmark.stale_days = 0`이면 **영원히 보관**(조사한 제품 전부의 동작을 원하는 사용자를 위한 값).
- **유효한 북마크는 시간으로 지우지 않는다**(오래 안 갔다고 없애면 안 된다).
- 수동: 팔레트·패널 우클릭 — `Bookmarks: Remove Invalid`(전체) · `Remove All in This Document` · `Remove All`(3초 안에 한 번 더 = 이 저장소의 파괴적 동작 관례).
- **상한**(39 §3 등재 규칙): 문서당 `bookmark.max_per_doc`(500) · 저장소 전체 `bookmark.max_total`(5,000). 넘으면 ① 가장 오래된 **무효**부터 버리고 ② 그래도 넘치면 새 북마크를 거부하고 토스트 한 줄. 조사한 어느 제품도 상한이 없다.

---

## 6. UI

### 6-1. 명령·키 (Sublime 관례 · `keymap.rs` `Command` 표에 추가 · 충돌 없음을 확인)

| id | Windows·Linux | macOS | 동작 |
|---|---|---|---|
| `bookmark.toggle` | `ctrl+f2` | `cmd+f2` | 캐럿 줄 토글(여러 커서면 각각) |
| `bookmark.next` | `f2` | `f2` | 다음(문서 → 끝나면 그룹 안 다음 문서 · 순환) |
| `bookmark.prev` | `shift+f2` | `shift+f2` | 이전 |
| `bookmark.clear_doc` | `ctrl+shift+f2` | `cmd+shift+f2` | 이 문서의 북마크 전부(되돌리기 불가 → 3초 확인) |
| `bookmark.label` | — | — | 이름·메모 편집(패널·팔레트·거터 우클릭) — UltraEdit는 **달 때** 묻지만 우리는 나중에도 달 수 있게 |
| `bookmark.select_all` | `alt+f2` | `alt+f2` | ★ 이 문서의 북마크 줄 전부에 **멀티커서**(Sublime `Select All Bookmarks` · 우리 편집기는 이미 다중 커서를 갖고 있다) |
| `bookmark.copy_lines` | — | — | 북마크된 줄만 클립보드로(Notepad++ `Copy Bookmarked Lines` · 로그·검증 결과를 추려 보낼 때) — 사용자의 클립보드를 쓰므로 **자동 시험에는 넣지 않는다** |
| `bookmark.set_0`…`set_9` | `ctrl+shift+0`…`9` | `cmd+shift+0`…`9` | 니모닉 지정(문서 안에서 이미 쓰는 번호면 **가져올까요** 확인 + "다시 묻지 않기") |
| `bookmark.goto_0`…`goto_9` | `ctrl+0`…`9` | `cmd+0`…`9` | 그 번호로 이동 |

⚠️ 지금 키맵에 `f2`·`ctrl+숫자`·`ctrl+shift+b`를 쓰는 명령은 **없다**(09-22 확인). 다만 Sublime은 `ctrl+0`을 **글꼴 크기 초기화**로 쓰므로, 나중에 확대/축소 명령을 넣을 때 충돌한다 — 그때는 북마크가 `ctrl+0..9`를 지키고 초기화는 `ctrl+shift+0`으로 보낸다(니모닉 쪽이 쓰임새가 잦다). 사용자는 어차피 `key.preset`·설정에서 바꿀 수 있다.
| `view.bookmarks` | `ctrl+shift+b` | `cmd+shift+b` | 북마크 패널 열기/포커스 — 활동 막대 식구와 같은 꼴(`view.explorer` `ctrl+shift+e` · `view.search` `ctrl+shift+f` · `view.extensions` `ctrl+shift+x` · `view.project`) |
| `bookmark.list` | — | — | 팔레트에 이 문서/전체 목록(`Bookmarks: …`) |

### 6-2. 화면 표시 (사용자 = 넷 전부)

| 표시 | 자리 | 비고 |
|---|---|---|
| **거터 아이콘** | 줄번호 옆 | 익명 = 리본 글리프 · 니모닉 = 숫자 상자(DataGrip·Toad) · 무효 = 탈색. 지금 `TextBox::set_line_marks`는 **색 띠만** 그린다 → nexa-ui에 글리프 마크 API 필요(§11 U-2) |
| **미니맵 틱** | 미니맵(T-97) | 짧은 색 띠(Sublime `add_regions` 관례) |
| **줄 끝 인라인 라벨** | 본문 오른쪽 | 이름·메모 첫 줄을 옅게(`bookmark.inline_label` · 상한 `..._chars` 40 · Qt Creator·VS Code 확장) — **주석을 남기지 않고 메모**할 수 있는 것이 이 기능의 값 |
| **상황줄 · 배지** | 상태줄 세그먼트 + 활동 막대 배지 | `북마크 3/12`(이 문서/전체) · 클릭 = 패널 |

### 6-3. 북마크 관리자 패널 — 자리와 뼈대 (사용자 요청 09-22 "좌측 탭에 추가")

**자리**: 활동 막대(`activity.rs` `ActivityBar::items`)의 **세 번째 패널 항목** — `view.explorer`(파일 아이콘) · `view.search`(돋보기) · **`view.bookmarks`(리본)** · `view.project`(67 T-165) · `view.extensions`(숨김 가능) 순. `ActItem{ id: "view.bookmarks", icon: toolicons::mi_bookmark(), panel: true, bottom: false, hidden: false }` 한 줄이면 등록된다(아이콘 `mi_bookmark`는 `toolicons.rs`에 신설). 배지 = 무효가 아닌 북마크 수(`bookmark.badge`).

**뼈대**: 새 파일 `bookmarks_panel.rs` — **`search_panel.rs`의 골격을 그대로 따른다**(같은 문제를 두 번 풀지 않는다 · 30 §2): `visible/bounds/scale` · 필터 `TextBox`(`set_focus_ring(false)`) · 도구 버튼 `FindBtn` 줄 · 자체 행 목록 + `ScrollBars` · `hover`/`sel` · `CtxMenu` · 툴팁. 행을 직접 그리는 이유는 한 행에 **아이콘 + 니모닉 상자 + 줄 번호 + 줄 원문 + 이름 칩 + 사유 칩**이 함께 들어가고 인라인 편집·드래그가 필요해서다(nexa-ctl `TreeView`/`TreeGrid`는 라벨+열 구조라 여기엔 좁다 — 설정 창처럼 단순한 트리에는 그대로 쓴다).

```text
[🔖] BOOKMARKS                                ⟳  ⌫  ＋  ≡  ⊟        ← 다시 검사 · 무효 지우기 · 새 그룹 · 묶음/정렬 · 전체 접기
     [필터                                            ] [Aa] [⌫]
     ▾ ■ 기본                                            9          ← ■ = 그룹 토글(켬) · 우클릭 = 그룹 메뉴
       ▾ 이슈관리/2026-09-01.sql                          3
         ⑴  12 │ AND A.PROJECT_CD = 'SEBANG'      재고 기준         ← 니모닉 · 줄 · 줄 원문 · 이름 칩
             40 │ -- 여기부터 검증                                   메모 있음 ▤
       ▾ BISCM · HR.PKG_M4P (BODY)                        2          ← 객체 DDL 문서(프로필 · 스키마.이름 · 부분)
             88 │ PROCEDURE calc_plan(
       ▾ Script_7 (저장 안 함)                            1          ← 이름 없는 탭
       ▾ old/scratch.sql                                  1  파일 없음 ← 무효 = 회색 + 사유 칩
     ▾ □ 검증 2026-09                                      3          ← □ = 꺼진 그룹(순회·거터에서 빠짐 · 항목은 남음)
     ─────────────────────────────────────────────────────
     12개 · 무효 1 · 공용 4                                           ← 바닥 요약(클릭 = 그 묶음만 필터)
```

- 행 높이·색·hover 페이드는 탐색기와 같은 부품(`IntentFade` Slow · `ROW_H 22`).
- **가상 스크롤**: 보이는 행만 그린다(상한 5,000 · 접힌 그룹은 자식 계산도 안 한다).
- 팝업·툴팁은 `geom::place_popup`/`ContextMenu`/`draw::draw_tooltip`만 쓴다(팝업 배치 규칙) · 새 팝업이므로 **창 네 모서리 캡처 1장**을 남긴다.
- 패널이 좁으면 줄 원문부터 말줄임하고 이름 칩·사유 칩은 지킨다. 전체 글은 툴팁으로.

### 6-4. 묶음 · 정렬 · 필터 (관리 기준)

| 기준 | 값 | 설정 |
|---|---|---|
| **묶음**(트리 1단) | ★ **그룹 → 문서 → 항목**(기본 · D-172) · 문서 → 항목(그룹은 위쪽 칩) · 평면 | `bookmark.panel_group_by` = `group` \| `doc` \| `flat` |
| **정렬**(문서 안) | ★ 줄 번호(기본) · 만든 순 · 최근 방문 순 · 이름 | `bookmark.panel_sort` |
| **문서 정렬** | 이름 순(기본) · 최근 방문 순 · 열려 있는 문서 먼저 | 같은 키의 둘째 값 |
| **무효 표시** | ★ 회색으로 보임(기본) · 맨 아래로 모음 · 숨김 | `bookmark.panel_show_invalid` |
| **필터** | 이름·메모·**줄 원문**·문서 이름·프로필을 함께 본다 · `Aa` 대소문자 · 공백은 AND | — |
| **필터 접두** | `@이름` 이름만 · `#그룹` 그룹만 · `!무효` 무효만 · `:12` 줄 번호 | — |
| **바닥 요약** | `12개 · 무효 1 · 공용 4` — 클릭하면 그 조건으로 필터(한 번의 입력으로 이어진다) | — |

필터 칸에 **커서를 둔 채 ↑↓로 선택을 옮기고 Enter로 이동**한다(SQL Workbench/J가 유일하게 제대로 한 것 · DbVisualizer Quick Load와 같은 감각). Esc는 ① 필터가 있으면 지우고 ② 비어 있으면 편집기로 포커스를 돌린다.

### 6-5. 좌클릭 · 더블클릭 · 키보드 (D-173)

| 입력 | 동작 |
|---|---|
| **한 번 클릭** | ★ **미리보기** — 그 자리를 편집기에 보여 주되 **포커스는 목록에 남긴다**. ↑↓로 계속 훑어볼 수 있다(`bookmark.panel_preview` 기본 on) |
| 더블클릭 · `Enter` | 편집기로 **완전히 이동**(포커스 이동 · 패널은 그대로) |
| `Alt+Enter` | 그 문서의 북마크 전부를 멀티커서로 열기(`bookmark.select_all`과 같은 결과) |
| 가운데 클릭 | 없음(탭을 닫는 관례와 헷갈린다) |
| `Space` | 그룹 행이면 토글 · 항목 행이면 미리보기 다시 |
| `F2` | 이름 편집(제자리 입력 상자) — JetBrains는 `Alt+클릭`, 우리는 탭 이름 편집(T-160)과 같은 관례로 |
| `Delete` | 제거 — **확인 없이 지우고 5초짜리 [실행 취소] 토스트**(C-29) |
| `↑ ↓ ← → Home End PgUp PgDn` | 탐색기와 같은 규칙(←는 닫기/부모로 · 타입어헤드는 필터가 대신한다) |
| `Ctrl+C` | 그 항목을 한 줄로 복사(`파일:줄  줄 원문  이름`) |
| 드래그 | 항목 → 다른 그룹으로 이동(고스트 + Esc 취소 · 툴바 그룹·그리드 컬럼과 같은 부품) |

**두 방향 동기화**(JetBrains·VS Code의 autoscroll 두 토글을 그대로):

| 토글 | 기본 | 뜻 |
|---|---|---|
| `bookmark.panel_autoscroll_to` | on | 목록에서 고르면 편집기가 따라간다(= 위의 미리보기) |
| `bookmark.panel_autoscroll_from` | off | 편집기에서 캐럿이 북마크 줄에 닿으면 목록이 그 항목을 선택한다(켜면 편집 중 목록이 계속 움직여 거슬린다는 보고가 많아 기본 끔) |

**타 제품 기준**: JetBrains = 단일 클릭 선택 + autoscroll 옵션 · 더블클릭 열기 · `Alt+클릭` 이름 변경 · 미리보기 창 · DBeaver = 더블클릭 `Open Bookmark`(단일 클릭은 아무것도 안 한다) · Visual Studio = 더블클릭 이동 · 우클릭 Rename/Delete · Qt Creator = 더블클릭 이동 + **드래그로 줄 옮기기** · Xcode = 클릭 이동 + hover 체크박스 · VS Code 확장 = 클릭 이동 + 인라인 ✗ 버튼. **단일 클릭을 "이동"으로 두면 되돌릴 수 없는 탭 전환이 자꾸 일어나고, "선택만"으로 두면 한 번 더 눌러야 한다 — 미리보기가 그 사이다.**

### 6-6. 우클릭 메뉴 (자리마다 다르게)

| 자리 | 항목 |
|---|---|
| **북마크 행** | 열기 · 새 탭으로 열기 · ─ · 이름 편집(`F2`) · 메모 편집… · 니모닉 지정 ▸(0–9 · 지우기) · 그룹으로 이동 ▸(그룹 목록 + 새 그룹…) · ─ · **공용으로 올리기 / 개인으로 내리기** · ─ · 줄로 복사 · 경로 복사 · 탐색기에서 보기 · ─ · 제거(`Delete`) |
| **무효 행**(추가) | 왜 끊어졌는지 보기… · **다른 파일에 다시 잇기…**(파일 선택) · 되살리기 다시 시도 |
| **문서 행** | 이 문서 열기 · ─ · 이 문서의 북마크 전부 멀티커서로 · 전부 다른 그룹으로 ▸ · ─ · 이 문서의 북마크 전부 제거 · 경로 복사 |
| **그룹 행** | 이름 바꾸기 · 기본 그룹으로 지정 · 켜기/끄기 · ─ · 새 그룹… · 이 그룹만 보기 · ─ · 그룹 삭제(항목 처리 2택 · C-12) |
| **빈 곳** | 새 그룹… · 무효 전부 지우기 · 묶음 ▸ · 정렬 ▸ · 모두 접기/펼치기 · ─ · 북마크 설정 열기(설정 창 Bookmarks 그룹으로 바로) |

메뉴는 **열려 있는 동안 모달이되 바깥 클릭은 메뉴를 닫고 그 클릭을 그대로 진행**한다(팝업/메뉴 UX 규칙).

### 6-7. 그룹(카테고리) 관리

- **한 북마크는 그룹 하나에만 속한다**(D-174). 옮기면 이전 그룹에서 빠진다 — 폴더와 같은 감각이고 "여기서 지우면 저기도 지워지나"라는 혼란이 없다. JetBrains는 다중 소속을 허용하면서도 **줄 북마크만은 예외로 막아 두었다**(`findGroupsToAdd`에서 `is LineBookmark -> null`) — 우리 대상이 바로 그 줄 북마크다.
- **기본 그룹**: 새 북마크가 들어가는 곳. 이름은 프로젝트 이름(프로젝트가 없으면 `기본`). 지울 수 없고, 다른 그룹을 기본으로 지정하면 그때 넘어간다(JetBrains `Use as default list`).
- **켜기/끄기**(`enabled`): 끄면 **순회(F2)·거터·미니맵·인라인에서 빠지고** 패널에는 흐리게 남는다. 지우지 않고 잠시 치우는 길 — Visual Studio의 `Disable All Bookmarks`를 **그룹 단위로** 올린 것이고, 조사한 제품 중 그룹 토글을 가진 곳은 없었다.
- 만들기 = 패널 `＋`·우클릭·팔레트 `Bookmarks: New Group…` · 이름 중복은 뒤에 `(2)`.
- 순서 = 사용자가 드래그(기본 그룹이 맨 위) · 그룹 수 상한 `bookmark.max_groups`(50).
- 공용/개인은 **그룹의 속성이 아니라 항목의 속성**이다(같은 그룹 안에 둘이 섞일 수 있고, 행 끝 작은 칩으로 구분). 그룹 이름 자체는 공용 파일에도 쓰인다(공용 항목이 하나라도 있으면).

### 6-8. 예상되는 어려운 경우 (설계 답)

| # | 경우 | 어떻게 |
|---|---|---|
| **C-1** | 같은 줄에 북마크가 둘 생김(여러 커서 토글 · 재탐색이 두 개를 같은 줄로) | 한 줄에 하나만 남기고 병합 — 이름·메모가 둘이면 **먼저 것**을 지키고 상태줄 안내(JetBrains도 같은 줄 중복을 제거한다) |
| **C-2** | 같은 파일이 두 탭에 열려 있음 | 문서는 **하나**(경로가 같으면 같은 문서) · 거터 표시는 두 탭 모두 · 미리보기는 활성 탭 우선 |
| **C-3** | 닫혀 있는 문서의 항목을 미리보기 | 탭을 열되 **미리보기 표식**(제목 기울임)을 주고, 다른 항목을 미리보기 하면 **그 탭을 재사용**한다. 편집하거나 더블클릭하면 고정(`bookmark.panel_preview_reuse` · VS Code 프리뷰 탭 관례 · 1차에 어려우면 재사용 없이 열기만) |
| **C-4** | **미접속 서버의 객체 DDL 항목을 미리보기** | ★ 미리보기는 **접속하지 않는다**(네트워크 규칙 · 클릭 하나로 접속이 생기면 안 된다) — 행에 `연결 없음` 칩 + 오른쪽 [연결하고 열기] 버튼, 더블클릭도 그 확인을 거친다 |
| **C-5** | 운영(PROD) 서버 객체 | 행에 **PROD 칩**(붉은 계열) · 열기 확인 문구에 서버·환경 명시(`tx.prod_*`와 같은 결) |
| **C-6** | 무효 항목을 눌렀다 | 이동하지 않고 **사유 카드**: 왜(파일 없음·객체 없음·줄 사라짐·서버 모름) · 마지막으로 본 줄 원문 · [다시 잇기…] [되살리기 시도] [지우기] |
| **C-7** | 큰 파일 모드 L2 문서 | 재탐색 없이 줄 번호로 이동 + `대략 위치` 칩(59 §4) |
| **C-8** | 그 문서가 **적재 중**(탭 격리 적재) | 자리 탭에 "적재 중"으로 두고, 끝나면 **그때의 탭을 확인한 뒤** 이동(뒤에서 끝난 일이 실행으로 이어질 때의 규칙) |
| **C-9** | 코드 접기 안의 줄 | 펼치고 이동(접기 상태를 복원해 두지 않는다) |
| **C-10** | 니모닉 중복 지정 | "`3`은 이미 *…*에 있습니다. 가져올까요?" + **다시 묻지 않기**(JetBrains `rewriteBookmarkType`) · 문서별 유일(D-162) |
| **C-11** | 그룹 삭제 | 2택 — **항목을 기본 그룹으로 옮기기**(기본) 또는 **항목까지 지우기**(빨강 · 3초 확인). 빈 그룹은 바로 삭제 |
| **C-12** | 기본 그룹을 지우려 함 | 막고 안내 — 먼저 다른 그룹을 기본으로 지정 |
| **C-13** | 그룹을 껐는데 `F2` 순회가 비었다 | 상태줄 "켜진 그룹에 북마크가 없습니다 — 꺼진 그룹 2개" + 패널로 가는 링크 |
| **C-14** | 공용으로 올리는데 문서가 **프로젝트 밖**(절대 경로) | 막고 안내(남의 로컬 경로를 저장소에 올리지 않는다 · §7-1) — 폴더를 프로젝트에 추가하면 된다고 알려 준다 |
| **C-15** | 공용으로 올리는데 그 접속이 **프로필이 아니라 접속 문자열** | 막고 안내(공용은 프로필 이름만 적는다 · D-165) — 프로필로 저장하면 된다고 알려 준다 |
| **C-16** | 공용으로 올렸더니 **같은 문서·같은 줄에 이미 공용 항목**이 있다 | 옮기지 않고 안내(중복). 이름·메모가 다르면 [이름 이어 붙이기] 한 번으로 병합 |
| **C-17** | 프로젝트를 전환했다 | 패널을 통째로 갈아 끼운다(저장소가 바뀐다) · 기본 워크스페이스의 항목은 프로젝트 없는 상태로 돌아갔을 때 다시 보인다 · 전환 중에는 빈 상태 대신 "불러오는 중" |
| **C-18** | 같은 프로젝트를 **두 인스턴스**에서 열었다 | 67의 파일 규칙을 그대로 따른다 — 마지막에 쓴 쪽이 이기고, 파일 서명이 바뀌면 다시 읽어 패널을 갱신한다(장차 타임스탬프 병합 §14) |
| **C-19** | git이 공용 파일을 바꿨다(pull·브랜치 전환) | 58의 감시가 프로젝트 파일 변경을 잡아 **공용 항목만** 다시 읽는다(개인 항목은 그대로) · 패널에 "공용 북마크가 갱신되었습니다" 한 줄 |
| **C-20** | 북마크가 5,000개 | 가상 스크롤 + 필터 인덱스(소문자 캐시) · 그룹 기본 접힘 · 상한에 닿으면 새 북마크 거부 토스트(§5) |
| **C-21** | 이름 없는 탭을 저장했다 | 문서 키가 `Scratch{tab}` → `File{path}`로 **이동**(복사 아님) · 패널의 문서 행이 이름을 얻는다 |
| **C-22** | DDL 탭을 파일로 저장했다 | **복사본**이 생겨 두 문서 행에 각각 보인다(§2-4) · 복사본 행에 출처 칩 |
| **C-23** | 같은 객체 DDL을 두 번 열었다 | 한 탭 재사용(문서 키가 같다) |
| **C-24** | 이름도 없고 줄 원문도 빈 줄 | 표시는 `(빈 줄) 파일이름:12` — 이름이 없으면 줄 원문을 쓰는 JetBrains 관례의 예외 처리 |
| **C-25** | 필터 중 선택했다가 필터를 지웠다 | 선택과 스크롤 위치를 지킨다(고른 것을 잃지 않는다) |
| **C-26** | 패널이 아주 좁다(폭 180px) | 줄 원문 → 이름 칩 → 줄 번호 순으로 줄여 나가고, 아이콘·니모닉은 끝까지 남긴다 |
| **C-27** | 드래그 중 Esc | 고스트를 걷고 아무것도 바꾸지 않는다(툴바 그룹·그리드 컬럼과 같은 규칙) |
| **C-28** | 북마크를 지웠다가 되돌리고 싶다 | `Ctrl+Z`는 **본문 편집만** 되돌린다(북마크는 본문이 아니다) → 제거는 **5초 [실행 취소] 토스트**로 되돌린다. 문서 전체·전부 지우기는 3초 두 번 확인 |
| **C-29** | 저장이 실패했다(권한·디스크) | 메모리에는 그대로 두고 경고 토스트 1회 + 다음 저장 시점에 재시도(67 §2-3) |
| **C-30** | 설정에서 기능을 껐다(`bookmark.enabled = off`) | 표시·순회·저장을 멈추되 **파일에 있는 것은 지우지 않는다** · 패널 아이콘은 숨긴다(확장 아이콘과 같은 규칙) |

### 6-9. 빈 상태 · 처음 쓰는 사람

- 북마크가 하나도 없을 때: `Ctrl+F2`로 지금 줄을 북마크하세요 + 키 3개 안내 + [설정 열기]. 팔레트로 가는 길도 한 줄(빈 확장 목록에서 쓴 방식과 같다).
- 필터에 걸린 게 없을 때: `"…"에 맞는 북마크가 없습니다 — 꺼진 그룹 1개 · 무효 숨김` 처럼 **왜 안 보이는지**를 같이 말한다.

---

## 7. 저장 (요구 ⑥)

> **별도 파일을 만들지 않는다.** 북마크는 67의 프로젝트/워크스페이스 파일에 **키 하나**로 얹는다. 이유: ① 파일이 하나 더 늘면 이동·동기화에서 짝이 어긋난다 ② 저장 시점·원자성·백업·손상 대응을 67이 이미 푼다 ③ 사용자의 공용/개인 요구가 67의 두 파일 구조(VCS용/개인용)와 정확히 겹친다.

### 7-1. 어디에

| 무엇 | 어디 | 근거 |
|---|---|---|
| **공용**(`shared: true`) | `<이름>.nsql-project`의 `"bookmarks"` | VCS로 팀과 공유(DBeaver `.bm`·VS Code `saveBookmarksInProject`가 검증) |
| **개인**(기본) | `<이름>.nsql-workspace`의 `"bookmarks"` | 개인용 · `.gitignore` 권장(67) |
| 프로젝트 없음 | `NSQL_HOME/workspaces/default.nsql-workspace` | 67 D-148 |
| 프로젝트 밖 절대 경로 파일 | **개인 쪽만** | 공용에 남의 로컬 경로를 올리지 않는다 |

### 7-2. 형식

```jsonc
// my.nsql-project  (공용 · git)
{ "version": 1, "folders": [ /* … 67 … */ ],
  "bookmarks": {
    "groups": [ { "id": 1, "name": "검증 2026-09", "default": false } ],
    "items": [
      { "id": 7, "doc": { "file": "이슈관리/2026-09-01.sql" },          // 프로젝트 상대 경로만
        "line": 39, "col": 4, "text": "AND A.PROJECT_CD = 'SEBANG'",
        "before": "FROM M4S_I002040 A", "after": "AND A.PLAN_VER = :V",
        "hash": "9f2a…", "lines": 412,
        "label": "재고 기준", "note": "PLAN_VER 규칙 확인", "mn": 1, "group": 1,
        "created": 1758499200, "visited": 1758502800 },
      { "id": 9, "doc": { "obj": { "profile": "BISCM", "dialect": "oracle",  // ★ 공용엔 프로필 이름만
                                   "schema": "HR", "kind": "package_body", "name": "PKG_M4P" } },
        "line": 87, "text": "PROCEDURE calc_plan(", "…": "…" }
    ] } }

// my.nsql-workspace  (개인)
{ "version": 1, "project": "my.nsql-project", "tabs": [ /* … 67 … */ ],
  "bookmarks": { "groups": [ /* … */ ],
    "items": [
      { "id": 3, "doc": { "obj": { "profile": "BISCM", "dialect": "oracle",
                                   "host": "db1.example.lan", "port": 1521, "database": "ORCL",
                                   "schema": "HR", "kind": "package_body", "name": "PKG_M4P" } },
        "…": "…" },
      { "id": 4, "doc": { "tab": 18 }, "line": 2, "text": "SELECT 1", "…": "…" },   // 이름 없는 탭
      { "id": 5, "doc": { "file": "D:/tmp/probe.sql" }, "state": { "invalid": { "since": 1758412800, "reason": "file_missing" } } }
    ] } }
```

- **공용에는 접속 좌표를 적지 않는다**(D-165) — 내부 호스트·포트가 저장소에 새지 않게. 여는 쪽은 자기 프로필 목록에서 그 이름을 찾는다. 이름이 없으면 `ServerUnknown`(회색) + "어느 접속에 연결할까요" 한 번.
- 개인 쪽은 좌표를 다 적는다(프로필 이름이 바뀌어도 산다).
- 모르는 키는 **읽고 그대로 되쓴다**(다음 판과 남의 버전에 안전). 북마크 키가 깨졌으면 **그 키만 무시**하고 프로젝트 파일 전체를 버리지 않는다.

### 7-3. 시점·원자성

67 §2-3과 **같은 경로**로만 쓴다 — 입력 디바운스 1초 + 최대 5초 · 임시 파일 → `rename` · 닫기/전환/종료에 즉시. 북마크가 바뀌었다고 따로 쓰지 않는다.

### 7-4. 포트(30 §2 부품 등재)

```rust
pub trait BookmarkStore {
    fn load(&self) -> Result<Vec<Bookmark>, String>;
    fn save(&mut self, all: &[Bookmark], groups: &[Group]) -> Result<(), String>;
    fn scope(&self) -> Scope;            // Shared | Private
}
```

`ProjectStore`(공용) · `WorkspaceStore`(개인) 둘을 `Bookmarks`가 들고, 항목의 `shared`로 어느 쪽에 쓸지 고른다. 67의 T-165가 끝나기 전에도 개인 backend만으로 굴러가고(기본 워크스페이스), 끝나면 공용이 붙는다.

---

## 8. 결정 (사용자 확인 완료 · 09-22)

| # | 질문 | 결정 |
|---|---|---|
| **D-156** | 대상 범위 | **파일 위치 + DB 객체** → **정정(09-22 3차)**: 객체 자체는 빼고 **객체 DDL 편집창의 위치**(§13) |
| **D-157** | 저장 위치 | **하이브리드** — 프로젝트(워크스페이스)가 있으면 그 파일, 없으면 기본 워크스페이스. 저장소는 포트 |
| **D-158** | 파일 위치 앵커 | **줄번호 + 내용 앵커로 재탐색**(§3) |
| **D-159** | 객체 문서 식별 | **접속 좌표 + 스키마·종류·이름**(계정 제외 · 서버가 다르면 다른 북마크) |
| **D-160** | 끊어진 북마크 | **회색 보관 + `bookmark.stale_days`(30) 뒤 자동 제거** · 부활 시도 |
| **D-161** | 1차 UX 범위 | **전부** — 이름·메모 + 니모닉 + 그룹 + 패널 |
| **D-162** | 니모닉 | **0–9 · 문서별 유일** |
| **D-163** | 이동·이름 변경 | **앱 안 = 자동 추적 · 앱 밖 = 내용 해시로 "이어 붙일까요" 제안** |
| **D-164** | 팀 공유 | **공용·개인 2단**(공용 = 프로젝트 파일 · 기본은 개인) |
| **D-165** | 공용 파일의 서버 표기 | **프로필 이름만**(호스트·포트·DB·계정 제외) |
| **D-166** | DDL 북마크의 복원 | 목록에 남기고 **클릭할 때 다시 조회**해 연다(기동 시 서버에 질의하지 않는다) · 유일성은 서버+객체 경로+유형 |
| **D-167** | DDL을 파일로 저장 | **양쪽 다 유지**(복사 · `origin` 기억) |
| **D-168** | 이름 없는 탭 | **워크스페이스 탭 id로 동일하게 지원** |
| **D-169** | 화면 표시 | **거터 아이콘 · 미니맵 틱 · 인라인 라벨 · 상황줄/배지 넷 다** |
| **D-170** | CLI | `nsql bookmark list \| add \| rm \| prune` |
| **D-171** | 재배치 판정(설계 판단 · 편집기 조사 뒤 추가 · 사용자 확인 대상 아님) | 정확 일치가 실패하면 **유사도**로 잇는다(Sørensen–Dice ≥ `bookmark.similarity` 0.6 · micro 플러그인이 쓰는 값) — 들여쓰기·별칭만 바뀐 줄을 잃지 않는다 |
| **D-172** | 패널 묶음 | **그룹 → 문서 → 항목**(도구줄에서 문서·평면으로 바꿀 수 있다) |
| **D-173** | 목록 좌클릭 | **미리보기 + 포커스는 목록에**(더블클릭·Enter = 편집기로 이동) |
| **D-174** | 그룹 소속 | **한 북마크는 그룹 하나** |
| **D-175** | 설정 자리 | **새 그룹 `Bookmarks`** + 분류 셋(동작·저장 / 표시 / 위치 추적·정리) |
| **D-176** | 미리보기와 접속(설계 판단 · C-4) | 미리보기는 **절대 접속하지 않는다** — 미접속 객체 행은 칩 + [연결하고 열기]로만 |
| **D-177** | 제거 되돌리기(설계 판단 · C-28) | `Ctrl+Z`가 아니라 **5초 [실행 취소] 토스트**(`bookmark.undo_secs`) · 전체 지우기는 두 번 확인 |

---

## 9. 설정 — **독립 영역 `Bookmarks`**(사용자 요청 09-22 · D-175)

설정 창의 좌측 목차에 **새 그룹 하나**를 만들고 그 아래 분류 셋을 둔다(`nsql-settings::CATEGORY_TREE`에 한 줄 · [24](24-settings-and-vscode-analysis.md) 체계 그대로). DBMS 그룹(09-21)과 확장 그룹(09-17)을 그렇게 만든 전례가 있다.

```rust
// nsql-settings/src/lib.rs — CATEGORY_TREE
(Msg::GrpBookmarks, &[Msg::CatBookmarkGeneral, Msg::CatBookmarkDisplay, Msg::CatBookmarkAnchor]),
```

```text
설정                                   [검색                    ]
├ General                              Bookmarks › 동작·저장
├ User Interface                       ┌──────────────────────────────────────────┐
├ Editors                              │ 북마크 사용            [●] 켬            │
├ Connections                          │ 저장                   [●] 켬            │
├ Data Editor                          │ 새 북마크의 공유 범위  (개인 ▾)          │
├ DBMS                                 │ 니모닉 유일 범위       (문서 ▾)          │
├ ★ Bookmarks                          │ 무효 보관 기간(일)     [ 30 ]            │
│   ├ 동작·저장                        │ 문서당 최대            [ 500 ]           │
│   ├ 표시                             │ 전체 최대              [ 5000 ]          │
│   └ 위치 추적·정리                   │ …                                        │
└ Extensions                           └──────────────────────────────────────────┘
```

### 9-1. `Bookmarks ▸ 동작·저장`(`CatBookmarkGeneral`)

| 키 | 종류·기본 | 뜻 | 종속 |
|---|---|---|---|
| `bookmark.enabled` | Bool · **on** | 끄면 표시·순회·저장을 멈춘다(파일의 기록은 지우지 않는다 · 활동 막대 아이콘도 숨김) · `Entry.perf` | — |
| `bookmark.persist` | Bool · on | 끄면 세션 안에서만 산다(프로젝트·워크스페이스에 쓰지 않는다) | `enabled` |
| `bookmark.share_default` | Enum · **private** \| shared | 새 북마크가 개인인가 공용인가 | `enabled` |
| `bookmark.mnemonic_scope` | Enum · **doc** \| global | 니모닉 0–9의 유일 범위(D-162) | `enabled` |
| `bookmark.mnemonic_ask_rewrite` | Bool · on | 중복 지정 때 "가져올까요" 묻기(끄면 조용히 가져온다 · C-10의 "다시 묻지 않기"가 이 값을 끈다) | `enabled` |
| `bookmark.stale_days` | Int · **30**(0 = 영원히) | 무효가 된 지 N일 뒤 정리 | `enabled` |
| `bookmark.max_per_doc` · `bookmark.max_total` | Int · **500** · **5000** | 상한(넘으면 무효부터 버리고, 그래도 넘치면 거부) | `enabled` |
| `bookmark.max_groups` | Int · 50 · HIDDEN | 그룹 수 상한 | `enabled` |
| `bookmark.default_group_name` | Text · (비면 프로젝트 이름) | 기본 그룹의 이름 | `enabled` |
| `bookmark.save_copy_on_save` | Bool · on | DDL 탭을 파일로 저장할 때 복사본을 만든다(D-167) | `enabled` |
| `bookmark.reconnect_ask` | Bool · on | 객체 문서를 열 때 미접속이면 물어본다(끄면 연결된 서버만 열린다) | `enabled` |
| `bookmark.undo_secs` | Int · 5 | 제거 뒤 [실행 취소] 토스트가 머무는 초(0 = 없음 · C-28) | `enabled` |
| `bookmark.confirm_clear` | Bool · on | 문서 전체·전부 지우기에 두 번 확인 | `enabled` |

### 9-2. `Bookmarks ▸ 표시`(`CatBookmarkDisplay`)

| 키 | 종류·기본 | 뜻 | 종속 |
|---|---|---|---|
| `bookmark.gutter` | Bool · on | 거터 아이콘(니모닉은 숫자 상자) | `enabled` |
| `bookmark.gutter_color` | Color · 테마 기본 | 아이콘 색(무효는 자동 탈색) | `gutter` |
| `bookmark.minimap` | Bool · on | 미니맵 틱 | `enabled` |
| `bookmark.inline_label` | Bool · on | 줄 끝 인라인 라벨(이름·메모 첫 줄) | `enabled` |
| `bookmark.inline_label_chars` | Int · 40 | 인라인 라벨 길이 상한 | `inline_label` |
| `bookmark.statusbar` | Bool · on | 상태줄 `북마크 3/12` 세그먼트 | `enabled` |
| `bookmark.badge` | Enum · **count** \| dot \| off | 활동 막대 배지 | `enabled` |
| `bookmark.panel_group_by` | Enum · **group** \| doc \| flat | 패널 묶음(D-172) | `enabled` |
| `bookmark.panel_sort` | Enum · **line** \| created \| visited \| name | 문서 안 정렬 | `enabled` |
| `bookmark.panel_show_invalid` | Enum · **inline** \| bottom \| hide | 무효 항목을 어떻게 보일까 | `enabled` |
| `bookmark.panel_preview` | Bool · on | 한 번 클릭 = 미리보기(끄면 선택만 · D-173) | `enabled` |
| `bookmark.panel_preview_reuse` | Bool · on | 미리보기가 **탭 하나를 재사용**(C-3 · 2차 구현이면 자동 off) | `panel_preview` |
| `bookmark.panel_autoscroll_from` | Bool · **off** | 편집기 캐럿을 따라 목록 선택이 움직인다 | `enabled` |
| `bookmark.panel_row_text` | Enum · **line** \| label \| both | 행에 줄 원문을 보일까, 이름을 보일까 | `enabled` |

### 9-3. `Bookmarks ▸ 위치 추적·정리`(`CatBookmarkAnchor`)

| 키 | 종류·기본 | 뜻 | 종속 |
|---|---|---|---|
| `bookmark.anchor_context` | Bool · on | 앞뒤 줄까지 앵커로 저장(끄면 그 줄만) | `enabled` |
| `bookmark.anchor_chars` | Int · 200 | 앵커 한 줄의 저장 길이 | `enabled` |
| `bookmark.anchor_update` | Bool · on | 그 줄을 고치면 앵커를 새 내용으로 갱신 | `enabled` |
| `bookmark.anchor_trim` | Bool · on | 좌우 공백을 접고 비교(포매터 대응) | `enabled` |
| `bookmark.search_lines` | Int · **400** | 재배치 양방향 탐색 범위(줄) — micro 플러그인 2,000 · Emacs 무제한 | `enabled` |
| `bookmark.similarity` | Float · **0.6**(0 = 정확 일치만) | 유사도 문턱(Sørensen–Dice · D-171) | `enabled` |
| `bookmark.relocate_unique` | Bool · on | 마지막 수단 = 문서에서 그 줄이 유일하면 거기로 | `enabled` |
| `bookmark.relocate_max_lines` | Int · 200000 | 이보다 큰 문서는 재탐색을 건너뛴다(줄 번호만) | `enabled` |
| `bookmark.revive` | Bool · on | 파일·객체가 돌아오면 무효를 되살린다(L3) | `enabled` |
| `bookmark.move_suggest` | Bool · on | 내용 해시가 같은 파일을 열면 "이어 붙일까요" 띠(D-163) | `enabled` |

### 9-4. 규약

- 라벨·설명은 전부 `nsql-i18n::Msg`(리터럴 금지) · 키 이름은 **안정 계약**(바꾸면 마이그레이션 표).
- 종속(`Dep::On`)이 걸린 항목은 부모가 꺼지면 **설정 화면에서 잠긴다**(09-16 규칙).
- 패널 우클릭 ▸ "북마크 설정 열기"와 설정 검색 `bookmark`가 같은 자리로 온다.
- `perf.boost`(향상 모드)는 `inline_label`·`minimap`·`panel_autoscroll_from`을 끈 값으로 강제한다(39 §4-6 · 강제값 > 개별 값).
- CLI `nsql config list bookmark`로 전부 보이고 `nsql config set bookmark.stale_days 0`로 바꾼다.

## 10. 부하원 등재([39 §3](39-resource-governance.md))

| 부하원 | 상한 | 키 | 비고 |
|---|---|---|---|
| 디스크 쓰기 | 67의 저장 경로에 얹힘(추가 파일 0) | `bookmark.persist` | 북마크만 따로 쓰지 않는다 |
| CPU(재탐색) | 문서를 열 때 1회 · 줄 수·탐색 범위 상한 2 | `bookmark.search_lines` · `..._max_lines` | 큰 파일 L2는 생략 |
| 매 프레임 | **0** — 거터·미니맵은 보이는 줄만, 위치 보정은 줄 변경 기록 소비 | `bookmark.gutter` 등 | 자체 타이머 없음 |
| 네트워크 | 객체 문서 재개 = **사용자 클릭 1회** · 자동 조회 없음 | `bookmark.reconnect_ask` | 26 §8 규칙 준수 |
| 메모리 | 북마크 1건 ≈ 앵커 3줄(≤ 600자) · 상한 5,000 | `bookmark.max_total` | ≈ 3 MB 최악 |
| 패널 그리기 | **보이는 행만**(가상 스크롤) · 필터는 타이핑마다 한 번 훑기(5,000행 ≈ 0.3 ms · 소문자 캐시) | `bookmark.enabled`(패널째 숨김) | 열려 있을 때만 · 접힌 그룹은 자식 계산 없음 |
| 패널 ↔ 편집기 동기화 | 미리보기 = 클릭할 때만 · `panel_autoscroll_from`은 **기본 끔**(켜면 캐럿 이동마다 목록 탐색) | `bookmark.panel_autoscroll_from` | 켜도 O(log n)(문서별 줄 정렬 이진 탐색) |

## 11. 구현 순서(T-167)

| 단계 | 내용 | 의존 |
|---|---|---|
| **U-1**(nexa-ui 선행) | `merge3`에 **`pub fn line_map(old, new) -> Option<Vec<Option<usize>>>`**(지금 private `match_lines` 공개) | — |
| **U-2**(nexa-ui 선행) | `TextBox` 거터 **글리프 마크**(`set_gutter_glyphs` · 아이콘/숫자 상자 · 지금은 색 띠뿐) · **줄 끝 주석**(`set_line_annotations`) · 미니맵 마크 | — |
| **B1** | `nsql-bookmarks` 크레이트(의존 0) — 모델 · JSON 직렬화 · **재탐색 순수 함수** · 줄 보정 · 정리 · 상한 · 테스트(난수 대조) | — |
| **B2** | 편집기 배선 — `Editors.doc_keys` · `TextBuf` 줄 변경 기록 소비 · 토글/이동/니모닉 명령 · 거터·미니맵·인라인 표시 | U-1 U-2 B1 |
| **B3** | 저장소 — `BookmarkStore` 포트 + 워크스페이스 backend(기본 워크스페이스부터) · 저장 시점 배선 | B1 · (67 T-165) |
| **B4a** | **패널 뼈대** — 활동 막대 항목 `view.bookmarks` + 아이콘 `mi_bookmark` · `bookmarks_panel.rs`(search_panel 골격) · 묶음/정렬/필터 · 가상 스크롤 · 빈 상태 · 배지 · 상태줄 세그먼트 | B2 B3 |
| **B4b** | **패널 조작** — 미리보기(좌클릭)·이동(더블클릭·Enter)·키보드 전부 · 우클릭 메뉴 넷(항목·무효·문서·그룹·빈 곳) · 제자리 이름 편집 · 드래그 이동(고스트·Esc) · 제거 + 5초 실행 취소 | B4a |
| **B4c** | **그룹 관리** — 만들기·이름·기본 지정·켜기/끄기·삭제 2택·순서·상한 · 순회/거터가 `enabled`를 존중 | B4b |
| **B4d** | **설정 영역** — `GrpBookmarks` + 분류 셋 · 키 38개 등록(라벨·설명 `Msg` · 종속 · HIDDEN · `perf.boost` 강제값) · 패널 ▸ "북마크 설정 열기" | B4a |
| **B5** | 객체 DDL 문서 — `OpenSql`에 `origin` · 재개(확인 후 접속) · 저장 시 복사본 · rename 제안 | B2 · 57 |
| **B6** | 무효·부활·정리 — 판정 · `stale_days` · 상한 · 파일 이동 제안 띠 | B3 · 58 |
| **B7** | 공용/개인 2단 — 프로젝트 파일 backend · "공용으로 올리기" · 프로필 이름만 적기 | 67 T-165 |
| **B8** | CLI `nsql bookmark list\|add\|rm\|prune` · 문서(이 파일 · 24 설정 · 39 §3) · 캡처(네 모서리) | B1 B3 |

★ **nexa-ui를 먼저 push**한다(CLAUDE.md §3 규약).

## 12. 검증

- **단위**(B1): 줄 보정(무작위 편집열 vs 전체 재계산 단순 모델 대조) · 재탐색 7종(위 삽입 · 위 삭제 · 그 줄 수정 · 그 줄 삭제 · 블록 이동 · 전체 재포맷 · 중복 줄) · 되돌리기 왕복 · 경로 정규화(Windows 대소문자) · 니모닉 유일성 · 상한 · 정리(N일) · JSON 왕복(모르는 키 보존).
- **MC/DC**([52 §12](52-session-modes.md)): 재탐색 판정 함수의 조건 5개(해시 같음 · 정합 있음 · 텍스트 일치 · 문맥 점수 · 큰 파일)와 정리 판정(무효 · 경과일 · 상한 · `stale_days=0`).
- **자동 GUI**(입력 주입 없이): `NSQL_HOME` 격리 + `NSQL_STARTUP_CMD=open:<파일>,bookmark.toggle,view.bookmarks` → 창 단위 캡처. 외부 변경은 앱이 떠 있는 동안 **스크립트가 파일을 고쳐** 확인(58에서 쓴 방식).
- **패널 단위 테스트**(순수 부분만 함수로 뽑는다): 묶음·정렬 결과(그룹/문서/평면 × 정렬 4) · 필터 매칭(접두 `@ # ! :` · 대소문자 · 공백 AND) · 가시 행 계산(접힘·무효 숨김·꺼진 그룹) · 그룹 삭제 2택 · 니모닉 중복 판정 · 좌클릭/더블클릭 분기(미리보기 vs 이동 vs 확인 필요)는 **MC/DC**(조건 = 미리보기 켬 · 문서 열림 · 무효 · 미접속 객체).
- **실기(사용자 몫)**: ① 니모닉 ctrl+숫자 ② git 브랜치 전환 뒤 자리 ③ 포매터로 전체 재들여쓰기 뒤 자리 ④ DDL 탭 재개(미접속 확인 · PROD 칩) ⑤ 파일을 탐색기 밖에서 옮긴 뒤 제안 띠 ⑥ 공용으로 올린 뒤 다른 PC에서 열기 ⑦ **패널 우클릭 메뉴 넷을 창 네 모서리에서**(팝업 배치 규칙 · 캡처) ⑧ 미리보기로 ↑↓ 훑기(포커스가 목록에 남는가) ⑨ 그룹 끄기 뒤 `F2` 순회 ⑩ 좁은 패널 폭에서 말줄임.

## 13. 충돌·조정

| 무엇과 무엇 | 어느 쪽을 따랐나 | 달라진 것 |
|---|---|---|
| 1차 답 *"대상 = 파일 위치 + DB 객체"* · 1차 답 *"객체 식별 = 접속 좌표"* ↔ 3차 답 *"객체 자체에 대한 북마크는 필요없고, 객체의 내용 DDL 에 대한 편집창에서의 북마크만"* | **나중 지시**(3차) | ① 탐색기 트리의 객체 핀/즐겨찾기 UI를 **설계에서 뺐다**(DBeaver `.bm`·Beekeeper `pins` 형 기능 없음) ② 접속 좌표 식별은 버리지 않고 **DDL 문서의 열쇠**로 재사용(D-159 유효) ③ "객체 북마크를 눌렀는데 미접속이면?"이라는 질문은 **DDL 문서 재개 규칙**(§4-2)으로 대체 ④ 대신 사용자가 지목한 편의 기능 둘을 넣었다 — 서버+객체 경로+유형으로 **파일처럼 유일성 부여**(D-166) · 파일로 저장하면 **복사본 전달**(D-167) |
| 2차 답 *"1차 UX = 전부(이름·메모+니모닉+그룹)"* ↔ 3차 답 *"니모닉은 파일별 유일"* | 충돌 아님(범위와 규칙) | 니모닉 유일 범위만 문서별로 확정(D-162) — JetBrains는 프로젝트 전역이라 다르다 |

## 14. 후속(이번 범위 밖)

- 결과 그리드의 행·셀 북마크(재실행하면 달라져 안정 열쇠가 없다 · [43](43-fetch-model-and-result-tabs.md) 뒤).
- 쿼리 텍스트 즐겨찾기·스니펫(팔레트·확장과 겹침 · DbVisualizer/SQuirreL형).
- 브랜치별 북마크 세트(DataGrip의 "브랜치마다 컨텍스트" · git 연동 뒤).
- 옛 내용을 묘비로 남기기(JetBrains master의 memorial — 목록이 두 배가 되는 값을 확인한 뒤).
- GUI 내보내기(Markdown·CSV) — 1차는 `nsql bookmark list --format md`로 대신한다.
- 공용 북마크의 git 병합 충돌 최소화(항목을 `id` 정렬로 써서 줄 단위 충돌을 줄인다 — 1차부터 정렬은 지킨다). 더 나아가면 **Neovim `shada`의 병합 규칙**이 본보기다: 종료할 때 디스크의 파일을 다시 읽어 **같은 항목은 타임스탬프가 큰 쪽을, 내 세션이 건드리지 않은 항목은 그대로** 둔다 — 두 사람이 같은 프로젝트 파일을 고쳐도 서로의 북마크를 지우지 않는다(1차는 마지막에 쓴 쪽이 이긴다).

## 출처

**DB 클라이언트** — DBeaver: [Bookmarks](https://dbeaver.com/docs/dbeaver/Bookmarks/) · `BookmarkStorage.java`·`BookmarksHandlerImpl.java`(devel) · [#22090](https://github.com/dbeaver/dbeaver/issues/22090) · [#7877](https://github.com/dbeaver/dbeaver/issues/7877) · [#8560](https://github.com/dbeaver/dbeaver/issues/8560) · [#20567](https://github.com/dbeaver/dbeaver/issues/20567) · [#36924](https://github.com/dbeaver/dbeaver/issues/36924) · [Project team work](https://dbeaver.com/docs/dbeaver/Project-team-work/). DataGrip: [Bookmarks](https://www.jetbrains.com/help/datagrip/bookmarks.html) · [DBE-15043](https://youtrack.jetbrains.com/issue/DBE-15043) · [DBE-21416](https://youtrack.jetbrains.com/issue/DBE-21416) · [IDEA-267257](https://youtrack.jetbrains.com/issue/IDEA-267257). SSMS/VS: [Manage bookmarks](https://learn.microsoft.com/en-us/sql/ssms/scripting/manage-bookmarks) · [Bookmark Studio](https://devblogs.microsoft.com/visualstudio/bookmark-studio-evolving-bookmarks-in-visual-studio/) · [.suo 답변](https://learn.microsoft.com/en-gb/answers/questions/1664697/). SQL Developer: [thatjeffsmith 2019](https://www.thatjeffsmith.com/archive/2019/01/bookmarks-in-your-sql-worksheet/) · [JDeveloper Bookmarks 설정](https://docs.oracle.com/middleware/12212/jdev/user-guide/GUID-B7923916-9E65-46DF-8DAD-97BF22B38A8A.htm). Toad: [SB Favorites](https://forums.toadworld.com/t/schema-browser-sb-favorites/34257) · [KB 105214](https://support.quest.com/zh-cn/toad-for-oracle/kb/105214/) · [bookmarks and workspaces](https://forums.toadworld.com/t/bookmarks-and-workspaces/60162). HeidiSQL: [help](https://www.heidisql.com/help.php) · [forum 17478](https://www.heidisql.com/forum.php?t=17478). Beekeeper: [`PinnedEntity.ts`](https://github.com/beekeeper-studio/beekeeper-studio/blob/master/apps/studio/src/common/appdb/models/PinnedEntity.ts) · [#1498](https://github.com/beekeeper-studio/beekeeper-studio/issues/1498). DbVisualizer: [Using Files](https://www.dbvis.com/docs/ug/accessing-frequently-used-objects/using-files/) · [Using Favorites](https://confluence.dbvis.com/display/UG100/Using+Favorites) · [저장 위치 KB](https://support.dbvis.com/support/solutions/articles/1000196574-). AQT: [Favorites](https://querytool.com/help/favorites). Aqua: [Using Favorites](http://www.aquaclusters.com/app/home/project/public/aquadatastudio/wikibook/Documentation21.0/page/Using-Favorites/Using-Favorites). Navicat: [manual 17](https://www.navicat.com/manual/pdf_manual/en/navicat_17/win_manual/navicat_en.pdf). SQL Workbench/J: [editor bookmarks](https://www.sql-workbench.eu/manual/editor-bookmarks.html). pgAdmin: [#1714](https://github.com/pgadmin-org/pgadmin4/issues/1714) · [#1437](https://github.com/pgadmin-org/pgadmin4/issues/1437). Workbench: [snippets](https://dev.mysql.com/doc/workbench/en/wb-sql-editor-snippets.html). SQuirreL: [`BookmarkManager.java`](https://github.com/squirrel-sql-client/squirrel-sql-code/blob/master/sql12/plugins/sqlbookmark/src/net/sourceforge/squirrel_sql/plugins/sqlbookmark/BookmarkManager.java).

**IDE** — JetBrains 소스(intellij-community master): `platform/bookmarks/src/com/intellij/ide/bookmark/BookmarksManagerImpl.kt` · `state.kt` · `providers/LineBookmarkProvider.kt` · `providers/InvalidBookmark.kt` · `providers/InvalidNode.kt` · `platform/core-api/src/com/intellij/openapi/editor/RangeMarker.java` · [Bookmarks 문서](https://www.jetbrains.com/help/idea/bookmarks.html) · [2021.3 블로그](https://blog.jetbrains.com/idea/2021/10/intellij-idea-2021-3-eap-2/). Eclipse: [IMarker](https://help.eclipse.org/latest/rtopic/org.eclipse.platform.doc.isv/reference/api/org/eclipse/core/resources/IMarker.html) · [markerUpdaters](https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.isv/reference/extension-points/org_eclipse_ui_editors_markerUpdaters.html) · `BasicMarkerUpdater.java` · `LocalMetaArea.java`. VS Code: [#270923](https://github.com/microsoft/vscode/issues/270923) · [vscode-bookmarks](https://github.com/alefragnani/vscode-bookmarks) `src/sticky/sticky.ts`·`src/storage/workspaceState.ts` · [#652 외부 변경 미추적](https://github.com/alefragnani/vscode-bookmarks/issues/652). Xcode: [Bookmarks Navigator](https://www.avanderlee.com/xcode/bookmarks-navigator/) · [저장 위치](https://www.jessesquires.com/blog/2023/07/11/where-are-xcode-bookmarks-stored/). NetBeans: `ide/editor.bookmarks/src/org/netbeans/modules/editor/bookmarks/BookmarksPersistence.java`. Qt Creator: [bookmarks](https://doc.qt.io/qtcreator/creator-how-to-use-bookmarks.html) · `src/plugins/texteditor/bookmarkmanager.cpp`.

**편집기** — Emacs: [`lisp/bookmark.el`](https://git.savannah.gnu.org/cgit/emacs.git/tree/lisp/bookmark.el)(`bookmark-search-size` 16 · `bookmark-default-handler` · `bookmark-relocate`) · [매뉴얼 Bookmarks](https://www.gnu.org/software/emacs/manual/html_node/emacs/Bookmarks.html). Vim/Neovim: [`src/mark.c`](https://github.com/vim/vim/blob/master/src/mark.c)(`mark_adjust_internal` · `one_adjust` vs `one_adjust_nodel`) · [motion.txt](https://vimhelp.org/motion.txt.html#mark-motions) · [`src/fileio.c` `buf_reload`](https://github.com/vim/vim/blob/master/src/fileio.c) · [starting.txt `shada-merging`](https://github.com/neovim/neovim/blob/master/runtime/doc/starting.txt) · [api.txt extended marks](https://github.com/neovim/neovim/blob/master/runtime/doc/api.txt). Sublime: [#6589 지속 북마크 요청](https://github.com/sublimehq/sublime_text/issues/6589) · [forum 59099](https://forum.sublimetext.com/t/persistent-bookmarks-for-st4/59099) · [Alt+F2의 뜻](https://forum.sublimetext.com/t/what-alt-f2-select-all-bookmarks-is-for/59408). Notepad++: [`ScintillaEditView.h`(MARK_BOOKMARK 20)](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/src/ScintillaComponent/ScintillaEditView.h) · [Scintilla Markers 문서](https://scintilla.sourceforge.io/ScintillaDoc.html#Markers) · [#10318 재적재 시 소실(Open)](https://github.com/notepad-plus-plus/notepad-plus-plus/issues/10318). micro: [`haqk/micro-bookmark` `bookmark.lua`](https://github.com/haqk/micro-bookmark/blob/master/bookmark.lua)(Sørensen–Dice 0.6 · ±2,000줄) · [micro #1967](https://github.com/zyedidia/micro/issues/1967). Kate: `katedocument.cpp` `documentReload()` · `katedocmanager.cpp` `loadMetaInfos`([invent.kde.org/utilities/kate](https://invent.kde.org/utilities/kate)) · [bug 351757](https://bugs.kde.org/show_bug.cgi?id=351757). Zed: [`crates/project/src/bookmark_store.rs`](https://github.com/zed-industries/zed/blob/main/crates/project/src/bookmark_store.rs) · `workspace/src/persistence.rs`. CudaText: [`atsynedit_bookmarks.pas`](https://github.com/Alexey-T/ATSynEdit/blob/master/atsynedit/atsynedit_bookmarks.pas). UltraEdit: [wiki/Bookmarks](https://wiki.ultraedit.com/Bookmarks). Geany: [#1269](https://github.com/geany/geany/issues/1269) · [geanynumberedbookmarks](https://plugins.geany.org/geanynumberedbookmarks.html). Helix: [#703](https://github.com/helix-editor/helix/issues/703).
