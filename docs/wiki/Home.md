# Nexa SQL 위키

Nexa SQL = 크로스플랫폼 경량 SQL 클라이언트(IDE) + CLI `nsql`(Windows · macOS · Linux · 올 러스트 단일 바이너리).

## 페이지

| 페이지 | 내용 |
|---|---|
| [프로젝트와 작업 환경](Project-Workspace.md) | 프로젝트 파일에 무엇이 저장되고(탭 · 캐럿 · 북마크 · 미저장 스크립트) 어떻게 복원되는지 · 자동 저장 · 종료 흐름 · OPEN FILES · 탐색기 |
| [북마크](Bookmarks.md) | 키 · 니모닉 · 표시(거터 · 미니맵 · 줄 끝 라벨) · 패널 · 저장 위치 · CLI `nsql bookmark` |
| [변수](Variables.md) | 바인드/치환 변수 · 변수 안의 변수 확장 시점(`vars.expand_at`) · 변수 창 |
| [탐색기와 필터](Explorers-and-Filters.md) | 필터 토글(Aa · ab · 정규식 · 경로) · 타입어헤드(글자를 치면 그 이름으로 이동 · 한글) · 선택 규칙(빈 곳 클릭) · DBMS 아이콘 · 탭 색(미저장 탭 이름 색) · 객체 상세 패널 |
| [Goto Anything(Ctrl+P)](Goto-Anything.md) | 파일 이름 퍼지 검색 · `이름:줄` · 접두 `>` 명령 · `:` 줄 · `@` 심볼 · `%` 내용 · `?` 도움말 · 메뉴의 단축키 표시 |
| [코드 완성과 아웃라인](Code-Completion-and-Outline.md) | 완성 팝업(트리거 · 문맥별 후보 · 일치/순서 규칙 · 설정) · 아웃라인 패널 · Goto Symbol(Ctrl+R) |
| [결과 그리드에서 데이터 편집](Data-Editing.md) | 편집 가능 조건 · 행 식별 등급(키 · 숨은 키 열 · ROWID · 전 열) · 키 · 표시 · 적용의 안전 규칙(사전 검사 · 1행 · 롤백) · 적용 뒤 갱신 · 동시성 · 설정 |
| [라이선스](License.md) | 무료로 쓸 수 있는 것 · Pro가 여는 것과 무료 대체 경로 · 요청 코드 → 이메일 → 설치 3단계 · PC 추가/교체(재발급) · 백업·제거 · 상태가 이상할 때 |
| [파일로 넣기·내보내기(대량 적재)](Bulk-Import-Export.md) | CSV/TSV/JSON Lines를 표에 넣기(GUI Import 창 · CLI `nsql import`) · 빠른 내보내기(`--fast`) · 속도와 경로 · 실패했을 때 · 설정 `bulk.*` |
| [설정 창](Settings.md) | 카드 순서 · 키 이름 복사(Shift/Ctrl+클릭 = 보이는 설정 전부) · 고급(Advanced) · 키 이름 규칙과 바뀐 키 · 바뀐 기본값 |
| [트랜잭션 로그](Transaction-Log.md) | 문장·트랜잭션 결과 · 행 클릭 = 보낸 문장 / 바인드 변수 / 값 치환 문장 세 층 · 거르기 |
| [큰 파일과 기능 제한](Large-Files-and-Limits.md) | 파일 크기·줄 수에 따라 무엇이 꺼지고(L1/L2) 어떤 상한이 늘 걸리는지 · 바꾸는 설정 |
| [SQL 포맷](SQL-Formatting.md) | Shift+Alt+F 기본 포맷 · 내장 Basic 공통 옵션(콤마 위치 · 공백/탭 · `WHERE 1=1` · 별칭) · 확장 포맷터(SQL Formatter for kiros33)를 기본으로 지정 · 미리보기 탭 |
| [Output 탭](Output-Tab.md) | 서버 메시지(DBMS_OUTPUT · PRINT · RAISE NOTICE) · 컴파일 결과 · 오류 · 문장 완료 줄이 모이는 탭 · 언제 나타나는지(`output.show`/`output.activate`) · 객체 소스 탭의 F5 = 한 단위 실행 |
| [Ctrl 객체 링크와 설명 툴팁](Object-Links.md) | Ctrl(⌘)을 누르는 동안 테이블·컬럼·루틴이 링크 · 코멘트 툴팁(우상단 · 위치 설정) · 좌클릭 = 설명 복사 · 우클릭 메뉴 · 향상 모드·큰 파일 자동 끔 |

> 이 폴더(`docs/wiki/`)가 원본이고 `scripts/wiki-publish.sh`가 GitHub 위키 저장소로 복사합니다. 이미지는 `images/`(맥 캡처 = `scripts/mac-capture.sh`).
> 기술 원장·설계 문서는 저장소 `docs/`(예: 제한 원장 = docs/72).
