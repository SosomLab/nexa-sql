# 19 · 비교(diff) · git 연동 · DB 오브젝트 시점별 로컬 캐시(원본 복원)

> 사용자(09-12): *"비교 기능과 머지 기능은 별도 구현 대상. 비교는 최소라도 범위에. SQL 편집이 git과 밀접하게 연동. 특히 object(package·procedure·view 등)를 수정할 때마다 로컬 캐시(시점별 백업)를 보조해 실수해도 원본 복원이 가능하게."*
> 관련: [15 외부 변경](15-external-file-changes.md)(3-way 병합) · [18 세션](18-session-and-projects.md).

## 1. 비교(Compare) — P0 최소 범위

| 기능 | 최소(M3) | 확장 |
|---|---|---|
| **두 버퍼/파일 비교**(Sublime `Compare Side-By-Side` 대응 — 사용자 설치 패키지) | 2-pane 나란히 · 줄 diff(Myers) · 변경 줄 배경 · 다음/이전 변경 이동 · 공백 무시 옵션 | 단어 단위 인라인 diff · 3-pane(base) |
| **오브젝트 vs 편집 중 소스** | 서버 현재 소스 ↔ 버퍼 diff("컴파일 전에 무엇이 바뀌나") | 서버 두 시점 비교 |
| **결과 집합 비교** | 같은 컬럼 두 결과의 행 diff(키 컬럼 지정) | DataGrip식 |
| **스크립트 시점 비교** | §3 캐시의 두 시점 diff | — |

크레이트: `similar`(Apache-2.0 · Myers/Patience · 인라인 diff · `TextMerge` 3-way) 하나로 비교+병합을 모두 덮는다 → 원장 등재(D-8). 병합은 별도 모듈(M3 후반)이며 비교와 코드를 공유하되 UI는 다르다(§15).

## 2. git 연동 — 스크립트 파일은 git 저장소의 파일이다

원칙: **git을 내장 구현하지 않는다**(`git` CLI 또는 `gitoxide` 호출). 편집기는 git 상태를 **보여주고**, 명령은 얇게.

| 기능 | 내용 | 단계 |
|---|---|---|
| Git gutter(GitGutter 대응) | 저장 파일의 HEAD 대비 추가/변경/삭제 줄 표시 · 변경 hunk 이동 · hunk diff 팝업 | M3 |
| 상태줄 | 브랜치 · dirty · ahead/behind | M3 |
| 명령 | `git_stage_file` · `git_commit`(메시지 입력) · `git_diff_file`(비교 뷰로) · `git_blame_line` · `git_log_file`(시점 목록 → 비교) | M4 |
| 프로젝트 | `.nexa-project`가 git 루트를 인지 · 무시 규칙(`.gitignore`) 반영 | M4 |
| ★ 오브젝트 소스 ↔ git | "DB에서 소스 가져오기 → 파일로 저장 → git" 흐름을 한 명령으로(`object_export_to_project`) — 스키마 소스를 저장소로 버전 관리하는 팀 관행 지원 | M4 |

## 3. ★ DB 오브젝트 시점별 로컬 캐시 — "실수해도 원본으로"

문제: PL/SQL 패키지·프로시저·뷰·트리거를 IDE에서 열어 고치고 컴파일하면 **서버의 이전 소스는 사라진다**(Oracle은 `DBA_SOURCE`에 현재본만 · 플래시백은 DBA 권한·기간 제한). Toad/PL/SQL Developer는 "컴파일 전 백업" 옵션이 있지만 기본 꺼짐이고 위치가 불투명하다.

설계(기본 켬 · 사용자가 의식하지 않아도 동작):

```text
~/…/nexa-sql/objcache/<연결 프로필 id>/<SCHEMA>/<TYPE>/<NAME>/
    2026-09-12T18-40-12.000Z_open.sql        ← 열 때(서버 원본 스냅샷 · 이것이 "원본")
    2026-09-12T18-52-03.412Z_compile.sql     ← 컴파일/저장 직전 버퍼
    2026-09-12T18-52-03.500Z_server.sql      ← 컴파일 성공 후 서버에서 다시 읽은 본
    index.json                               ← 시점 목록 · 해시 · 결과(성공/오류) · 연결 · 사용자
```

| 시점 | 무엇을 저장 | 왜 |
|---|---|---|
| **열기** | 서버 소스 그대로 | 원본 보존 — 이 파일이 있으면 어떤 실수도 되돌릴 수 있다 |
| **컴파일/저장 직전** | 버퍼 내용 | 서버가 거부해도 내가 쓴 것은 남는다 |
| **컴파일 성공 후** | 서버가 실제로 가진 소스 | 편집기 밖 변환(공백·대소문자)을 반영한 진짜 현재본 |
| 외부 변경 감지(다른 사람이 서버에서 바꿈) | 서버 소스 | 열려 있는 동안 `DBA_OBJECTS.LAST_DDL_TIME` 폴링(옵션 · 30초) → "서버에서 변경됨" 배지 + 비교 |

- **내용 해시가 같으면 새 시점을 만들지 않는다**(용량 · 소음 방지). 시점 수·용량 상한(기본 오브젝트당 50개 · 전체 200MB · 오래된 것부터 정리 · "고정" 표시는 보존).
- 복원 = 시점 선택 → 비교 뷰(현재 서버본 ↔ 시점) → **"이 시점으로 컴파일"** 한 번. 복원 자체도 시점으로 남는다(되돌리기의 되돌리기).
- 스크립트 파일(`.sql`)도 같은 기계로 **저장 시점 스냅샷**(git이 없는 폴더에서 로컬 히스토리 — DataGrip Local History 대응).
- 저장 형식은 평문 `.sql`(사용자가 탐색기에서 열어도 됨) + `index.json`. 민감 정보 없음(소스뿐). 위치는 설정으로 바꿀 수 있고 프로젝트 폴더 안(`.nexa/objcache`)으로 두면 팀 git에 올릴 수도 있다.
- 방언별 소스 취득: Oracle `DBMS_METADATA.GET_DDL` / `ALL_SOURCE` · SQL Server `OBJECT_DEFINITION()` / `sys.sql_modules` · PG `pg_get_functiondef` · MySQL `SHOW CREATE`.

## 4. 단계
- **M3**: 비교 뷰(2-pane) + 스크립트 저장 시점 스냅샷 + git gutter.
- **M4**: 오브젝트 캐시(열기/컴파일 전후) + 복원 명령 + git 명령 최소.
- **M5**: 서버 변경 폴링 · 결과 집합 비교 · 3-pane 병합([15](15-external-file-changes.md)).

## 5. 결정 후보
- D-12 `similar` 원장 등재(비교·병합 공용).
- D-13 오브젝트 캐시 기본 위치(앱 데이터 vs 프로젝트 `.nexa/`)와 상한값.

## 6. CREATE 문 ↔ 실제 객체 비교(10-06 · 사용자 요구 · T-283 · 설계 메모)

> ✅ **1단계 구현 10-07**(개발 세션 · `crates/nexa-sql/src/app/compare.rs`): 6-2의 1단계 그대로 — 캐럿 문장이 `CREATE … <종류> <이름>`이면 메타 스레드로 서버 DDL(Generate SQL ▸ DDL과 같은 원천)을 청해 **비교 탭**(읽기 전용 뷰 탭 `⇄ 이름` · 본문 = 편집기 문장 정규화 · 기준선 = 서버 DDL → 거터 띠 초록/노랑/빨간 쐐기 · 머리 줄 `-- 비교 S.T: 일치` 또는 `N군데 다름`) · 팔레트 `obj.compare`("실제 객체와 비교…") · `obj.compare_server`("마지막 비교의 서버 DDL 열기" = 읽기 전용 탭) · hover 카드 버튼 "실제 객체와 비교…"(CREATE 문의 이름 위 · 서버에 있을 때 활성) · 설정 `compare.ignore_ws`(편집기 · 기본 켬) · 기동 명령 `compare.dump:<파일>`(`same= hunks= name=` 또는 `none`) · 상태줄 = 대상 아님 / 접속 없음 / 비교 중 / 일치 / N군데 다름 / SQL 생성 실패. **2단계(nexa-ctl `DiffView` 부품) = 남음.** V1(10-07 · journal 10-07): 대상 아님 · 서버에 없음 · 서버 DDL 탭 · 변경 줄 노랑 띠 ✓ · ✗ 같은 문장이 "1군데 다름"(서버 DDL 머리 주석 `-- t2 definition` · 빈 줄 · 끝 `;`가 정규화 전) → 정규화에 머리 `--` 주석·빈 줄 제거 + 스키마 없으면 이름만 표기 = 고침(bin28 · 같은 문장 `same=true hunks=0` · 바꾼 컬럼 = 노랑 띠 1).

> 사용자 10-06: "CREATE 문에서도 문장과 실제 생성을 비교할 수 있도록 hover 메뉴". 편집기의 `CREATE TABLE emp (…)` 이름에 머무르면 뜨는 hover 카드(96 §7)에 **"실제 객체와 비교…"** 버튼 → 편집기 문장 vs 서버의 실제 DDL을 비교해 보여 준다. 작성 = 협업 세션 조사(코드 읽기만) · 구현 = 개발 세션.

### 6-1. 지금 있는 것(10-06 조사)

| 부품 | 자리 | 무엇 | 이번에 쓰는 법 |
|---|---|---|---|
| 줄 정합(Myers O(ND)) | nexa-ui `nexa-ctl/src/merge3.rs:26` `match_lines`(비공개) | a의 줄 i ↔ b의 줄 j 대응표 · 공통 앞뒤 잘라냄 · 편집 거리 `MAX_D` 1500 넘으면 `None` | **비교의 핵심** — 공개 래퍼로 덩어리(hunk) 출력을 만든다 |
| 최소 줄 편집 | 같은 파일 `:109` `pub fn line_edits(old, new)` | `(from, to, 넣을 글)` 글자 인덱스 목록 | 덩어리·맞은편 줄 번호는 안 줌 → 직접은 부족 |
| 3-way 병합 | `:162` `pub fn merge3` | 외부 변경 병합(nexa-sql `extfile.rs:114` · 58) | 이번엔 안 씀 |
| 거터 diff 표시 | nexa-ctl `controls/textbox.rs:221` `diff_lines`/`diff_lines_hint` · `DiffKind{Added, Modified, DeletedAbove}` · `TextBox::set_baseline`(:1756) · `diff_marks` | 기준선 대비 줄 변경을 거터 띠로(LCS · `LCS_CAP` 1500) | **1단계 그대로 재사용** — 편집기 탭의 기준선 = 생성 DDL |
| 줄 표식 | `set_line_marks` · `set_minimap_marks` · `set_gutter_labels` | 거터 색 막대 · 미니맵 칠 · 거터 라벨 | 변경 위치 미니맵 표시 |
| 나란히 칸 | nexa-sql `editors.rs:80` `split`(동시 편집 · `editor.split_max`) | 탭 여러 개를 독립 편집기로 나란히 | 2단계 2-pane의 틀(스크롤 동기화 없음) |
| 실제 DDL | nsql-catalog `gen.rs:337` `generate(s, &GenSpec{ what: GenWhat::Ddl, opts: GenOpts{ separate_fk, qualified, … } })` | 객체 탐색기 Generate SQL ▸ DDL과 같은 원천(83 · 100 §5 보수적 생성) | 비교의 오른쪽(서버) 글 |

- **없는 것**: 두 글을 색으로 나란히/통합 표시하는 **diff 뷰어 화면**(docs/19 §1 · TODO C-1 · T-135 = 계획만 ☐) · 줄 전체 배경 칠 API(거터·미니맵뿐) · 칸 스크롤 동기화 · 맞은편 빈 줄 채움 · 다음/이전 변경 이동 · 공백·대소문자 무시 · 단어 단위 인라인 diff. `similar` crate는 없음(§5의 D-12 후보는 실제로 자체 Myers로 대체됨).
- T-230 · docs/87 §12의 "편집기 비교"는 **그리드 데이터 편집기를 다른 도구와 비교한 조사**라 이번 주제와 무관.

### 6-2. 권장안 — 2단계

1. **1단계(작게 · 재사용만)** = **비교 탭**(읽기 전용 뷰 탭 · 결과 영역 없음 · 탭 종류 `Compare`): 본문 = 편집기 문장 그대로 · `TextBox::set_baseline(생성 DDL)` → 거터 띠(추가/변경/위에서 삭제) + 미니맵 표식 · 머리 줄 한 줄 = `일치` 또는 `N군데 다름 · 서버 기준 <시각>` + [서버 DDL 열기](생성 DDL을 새 탭 · 사용자가 동시 편집 칸으로 나란히) · 지연 로딩(61 §1-8) = 버튼을 누를 때만 `generate`(메타 스레드 · `gate_open` 통과 · 실패 = 상태줄 + Output) · 정규화(아래 6-3) 뒤 비교.
   - 장점 = 새 컨트롤 0 · 오늘 있는 부품만. 한계 = 서버 쪽에만 있고 편집기에 없는 줄의 **내용**은 안 보임("위에서 삭제" 표식뿐).
2. **2단계(diff 뷰어 부품 · nexa-ui)** = nexa-ctl `DiffView`(새 부품 · 30 §2 등재): 공개 `diff_hunks(a, b) -> Vec<Hunk{a: Range, b: Range, kind}>`(`match_lines` 래퍼) · 2-pane(왼쪽 편집기 문장 · 오른쪽 서버 DDL) · 줄 전체 배경(추가 초록 · 삭제 빨강 · 변경 노랑 = 테마 토큰) · 맞은편 빈 줄 채움 · 스크롤 동기화 · F7/Shift+F7 다음/이전 변경 · 옵션 공백 무시/대소문자 무시. 같은 부품을 docs/19 §1(두 버퍼 비교) · §3(시점 캐시 복원) · T-140 남은 "좌우 비교 뷰"가 함께 쓴다.

> ✅ **2단계 구현 10-09**(개발 세션 · nexa-ui `990ae1c` `diff::{align, hunks}` + TextBox `set_row_tints`/`vscroll_top` · nexa-sql `diff_view.rs` · f0fa563 → a145717 → 05ffd38 → 261839a · 협업 V1 bin110~113):
> - **화면** = 비교 탭(`compare:*` 뷰 탭)을 호스트가 2-pane으로 그림 — 왼쪽 **편집기** 문장 | 오른쪽 **서버** DDL · 머리 첫 줄 = "비교 X: N군데 다름(초록 = 편집기에만 · 빨강 = 서버에만 · 노랑 = 변경) · k/N · 팔레트: 비교 다음/이전 변경" · 둘째 줄 = 칸 라벨 "편집기 | 서버" · 행 정렬(맞은편 빈 행) · 스크롤 동기(주도권 쪽 첫 줄) · 읽기 전용 상자(우클릭 = 복사·전체 선택 · 잘라내기·붙여넣기 흐림).
> - **색 규칙**(범례와 매핑은 한 자리) = 편집기에만(a쪽 · `RowKind::Delete`) **초록** · 서버에만(b쪽 · `Insert`) **빨강** · 변경(Replace) **노랑** · `compare.dump` = `rows= equal= editor_only= server_only= replace= hunks= cur=` + 덩어리 범위.
> - **보이는 글 = 원문 줄**(들여쓰기·탭 유지) · 비교는 정규화 줄(`normalize_pairs` = 비교용·표시용 쌍).
> - **`compare.canon`**(기본 켬) = Oracle EDITIONABLE/NONEDITIONABLE 무시 + 대소문자 안 섞인 따옴표 식별자 벗김(`"BISCM"."SP_TEST1"` = `BISCM.SP_TEST1` · `"MyTab"` 유지 · 문자열 리터럴 보존) → 소스 열기 본문 그대로 비교 = `same=true hunks=0`(BISCM SP_TEST1 실측).
> - 이동 = 팔레트 "비교: 다음/이전 변경"(`compare.next/prev` · 순환) · F7/Shift+F7 키맵은 후속 · `compare.view` = diff(기본)/inline(1단계 거터 표식 탭) · 서버 DDL 탭 `obj.compare_server`는 비교 결과가 있을 때(비동기 왕복 뒤)만.
> - **회귀 2 수정**: 10-07 저녁 "상세 없는 뷰 탭 자동 닫기"가 모든 뷰 탭을 닫아 1·2단계 비교 탭이 사라짐(a145717 · `ext:` 키만) · 입력 라우팅의 뷰 탭 판정이 그리기와 달라 상세 없는 뷰 탭(inline·서버 DDL·정보 탭) 본문 우클릭 메뉴가 안 열림(261839a · `App::ext_view_active()` 한 판정).
> - 남음 = F7/Shift+F7 · §1 두 버퍼 비교에 같은 부품 재사용 · T-283 "정적 선언 차이"(인자·컬럼).

### 6-3. 비교 전 정규화(헛 차이 줄이기 · 순수 함수 + 시험)

- 줄 끝 공백 · 빈 줄 연속 · 끝 `;`/`/` 한 개 · 탭↔공백(설정 `compare.ignore_ws` 기본 on).
- 이름 인용·대소문자: Oracle `"EMP"` = `EMP` = `emp`(따옴표 없는 이름) · 스키마 한정 유무(`GenOpts.qualified`를 편집기 문장에 맞춤: 문장에 스키마가 없으면 끔).
- FK 위치: `GenOpts.separate_fk` = 편집기 문장에 `ALTER TABLE … ADD CONSTRAINT … FOREIGN KEY`가 따로 있으면 켬 · 인라인이면 끔.
- 서버가 덧붙이는 기본값(Oracle `SEGMENT CREATION` · `TABLESPACE` · PG `WITH (…)`) = `GenOpts.compact`(있으면)로 줄이고, 남는 것은 "서버 전용 절"로 흐리게(1단계 = 머리 줄에 "서버 기본 절 N줄 제외").
- 포맷 차이(줄 바꿈 위치) = 양쪽을 nsql-format Basic(95)으로 같은 옵션으로 정형화한 뒤 비교(옵션 `compare.format_both` 기본 on).

### 6-4. 진입·대상

- 진입 = CREATE 문 이름 hover 카드 버튼 "실제 객체와 비교…"(카드 버튼 줄 · MouseUp 확정 · 96 §7) + 팔레트 `obj.compare` + 편집기 우클릭(캐럿이 CREATE 문 안).
- 대상 판정 = 순수 함수: 캐럿 문장이 `CREATE [OR REPLACE] TABLE|VIEW|PROCEDURE|FUNCTION|PACKAGE|TRIGGER|INDEX <이름>` → `ObjectRef` · 서버에 없으면 버튼 비활성 + 툴팁 "서버에 없음(새 객체)" · 종류마다 `GenWhat::Ddl` 또는 소스(100 §5).
- 실행 통제 = `gate_open`(52) · 메타 세션(탐색기와 같은 서버) · 운영(PRD) 읽기 = 경고 없음(읽기만).

### 6-5. 시험

- 단위 = 대상 판정 MC/DC · 정규화 표(방언 4) · (2단계) `diff_hunks` 난수 대조(단순 LCS 모델 · 61 §2 자료 구조 규칙).
- E2E(격리 · SQLite + 실서버 읽기) = `CREATE TABLE` 그대로 → "일치" · 컬럼 하나 바꿈 → "1군데 다름" + 거터 Modified · 서버에 없는 이름 → 버튼 비활성.
- 자체 시험 기동 명령(제안) = `obj.compare:<이름>` · `compare.dump:<파일>`(`same=true|false hunks=N` + 덩어리 목록).
