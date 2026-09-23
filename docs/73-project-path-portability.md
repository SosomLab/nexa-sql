# 73 · 프로젝트 파일의 경로 관리 — 타 제품 조사 · 이식(압축·복사·다른 OS) 개선안

> **요구(사용자 09-23)**: Sublime · VS Code · DBeaver · IntelliJ · DataGrip 등 주요 편집기·IDE·DB 클라이언트가 **프로젝트 파일의 경로를 어떻게 관리하는지** 조사하고, 맥·리눅스·윈도우 사이에서 **폴더를 압축해 복사하면 같은 편집/개발 환경이 그대로 서는** 방식(절대 < **상대**)으로 Nexa SQL의 경로 관리를 개선할 안을 낸다.
>
> 선행: [67 프로젝트](67-project-workspace.md) · [70 자동 저장·복원](70-autosave-and-restore.md) §6-1(지금 구현) · [21 연결 프로필](21-connection-profiles.md)(비밀번호는 볼트 = 파일에 안 담김) · [61 §3 OS별 차이](61-core-design-and-working-rules.md).

## 0. 결론 여섯 줄

1. 조사한 제품 전부 **"프로젝트 파일 위치 기준 상대 경로 + 그 밖은 변수(앵커)"** 다 — 순수 절대 경로만 쓰는 것은 세션 파일(Notepad++ · Vim viminfo)뿐이고, 그것들은 공유·이식을 목적으로 하지 않는다.
2. 상대 경로의 **기준은 하나가 아니라 여럿**: 프로젝트 파일 폴더(Sublime `$project_path` · VS Code · JetBrains `$PROJECT_DIR$` · DBeaver `${project}` · Eclipse `PROJECT_LOC`) + 홈(`$USER_HOME$` · `${home}`) + 워크스페이스/앱(`WORKSPACE_LOC` · `${workspace}`) + **사용자 정의 변수**(JetBrains Path Variables · Eclipse path variables) — 못 푸는 변수는 **열 때 정의를 묻는다**(JetBrains).
3. **개인 상태는 분리**한다 — `.sublime-workspace` · JetBrains `workspace.xml`/`dataSources.local.xml` · Eclipse `.metadata` — 공유 파일에는 폴더·설정·연결 **정의**만, 탭·캐럿·비밀번호는 로컬. 우리는 사용자 요구(09-23 "환경 그대로 복제")로 한 파일에 담되 **절로 나눠** 같은 효과를 낸다(§4-5).
4. Nexa SQL 지금(§3)은 "프로젝트 파일 폴더 기준 상대 · 밖은 절대" 한 단계라, **형제 폴더(`../nexa-ui`)·홈 아래·다른 등록 폴더 안의 파일**이 절대로 남아 복사하면 깨진다. 또 열 때 경로를 절대화하지 않아 cwd 상대 인자가 파일에 그대로 새는 구멍이 있다.
5. **개선안(§4)** = 경로를 **앵커 접두 + 상대**로 쓴다: `${project}/…` → `${folder:<이름>}/…`(등록 폴더 기준 · 폴더가 어디로 가든 따라감) → `${home}/…` → `${var:NAME}/…`(사용자 정의) → 절대(마지막 수단 · 저장 때 경고). 해석은 그 역순 우선순위 · 못 찾으면 **자리 탭 + "경로 변수 정의" 팝업**(JetBrains 방식) · 저장은 새 위치 기준 재계산(VS Code Save As).
6. 이식 규칙: 구분자 `/` · 드라이브 문자 대문자 · 심볼릭 링크는 **그대로**(canonicalize 금지) · macOS는 NFC · 대소문자 비교는 OS 규칙 · `..`는 같은 루트(드라이브) 안에서만 · 비밀번호는 절대 안 담고 프로필 **이름**만(21). 단계 P1~P4 · 결정 D-186~D-190(§6).

---

## 1. 다른 제품은 어떻게 하나(공식 문서 기준)

| 제품 | 공유 파일 | 개인(로컬) 파일 | 경로 기준·변수 | 못 풀 때 | 이식 관례 |
|---|---|---|---|---|---|
| **Sublime Text** | `<이름>.sublime-project`(JSON · `folders[].path`) | `<이름>.sublime-workspace`(열린 파일·수정분·캐럿) | `path`는 "**프로젝트 폴더 기준 상대** 또는 절대"([docs/projects](https://www.sublimetext.com/docs/projects.html)) · 변수 `$project_path`(프로젝트 파일 폴더) · `$project` · `$project_base_name` · `$folder`(첫 폴더) · `$file` · `$packages` · `$platform`([build_systems](https://www.sublimetext.com/docs/build_systems.html)) | 폴더가 없으면 사이드바에 빈 폴더로 남김 | "`.sublime-project`는 VCS에 넣고 `.sublime-workspace`는 넣지 않는다" |
| **VS Code** | `<이름>.code-workspace`(`folders[]` · 설정 · 실행 구성) | `workspaceStorage/<해시>`(탭·뷰 상태 · 백업) | "폴더 배열은 **절대 또는 상대** — **공유하려면 상대가 낫다**" · **Save Workspace As…가 새 위치 기준으로 상대 경로를 다시 계산** · `${workspaceFolder}` · `${workspaceFolder:이름}`(다중 루트에서 폴더 이름으로 지정) · `${userHome}` · `${env:…}`([multi-root](https://code.visualstudio.com/docs/editing/workspaces/multi-root-workspaces)) | 없는 폴더는 트리에 "(없음)"으로 남김 | 상대 · `../형제` 허용 · 폴더에 `name`을 줄 수 있어 순서와 무관 |
| **JetBrains(IntelliJ · DataGrip)** | `.idea/*.xml`(모듈 · 데이터 소스 `dataSources.xml`) | `.idea/workspace.xml` · `tasks.xml` · `dataSources.local.xml`(비밀번호 · 스키마 표시) | **Path Variables** = "절대 경로의 자리표시자": `$PROJECT_DIR$` · `$USER_HOME$` · `$MODULE_IML_DIR$` + **사용자 정의 `$NAME$`**(IDE 전역 설정에 저장 · 프로젝트에는 이름만)([path variables](https://www.jetbrains.com/help/idea/absolute-path-variables.html)) | 프로젝트를 열 때 **정의되지 않은 변수를 찾아 값을 묻는다** | `workspace.xml`은 VCS 켜면 자동 `.gitignore`([VCS 안내](https://intellij-support.jetbrains.com/hc/en-us/articles/206544839-How-to-manage-projects-under-Version-Control-Systems)) · 데이터 소스는 "비밀번호 없이 복사"([DataGrip 블로그](https://blog.jetbrains.com/datagrip/2018/05/21/copy-and-share-data-sources/)) |
| **Eclipse** | `.project`(링크 리소스 정의) · `.settings/` | `.metadata/`(워크스페이스 = 탭·히스토리) | 링크 리소스 = "절대 또는 **path variable 기준 상대**": `PROJECT_LOC` · `PARENT_LOC` · `WORKSPACE_LOC` · `ECLIPSE_HOME` + 사용자 정의 · `${VAR}/sub`([path variables](https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.user/concepts/cpathvars.htm)) | 변수 미정의 = 리소스 "깨짐" 표시 · 변수 정의로 복구 | "프로젝트 수준 변수로 워크스페이스·컴퓨터를 건너 이식" |
| **DBeaver** | 프로젝트 폴더 `.dbeaver/data-sources.json`(+ `data-sources-*.json` 자동 병합) · `Scripts/` | `.dbeaver/credentials-config.json`(암호화) · 워크스페이스 `.metadata` | 연결 파일 경로(SQLite 등)에 **`${project}` · `${workspace}` · `${home}`** — "공유·다른 컴퓨터로 옮길 때 더 튼튼한 링크"([설정 파일](https://dbeaver.com/docs/dbeaver/Configuration-files-in-DBeaver/) · [연결](https://dbeaver.com/docs/dbeaver/Admin-Manage-Connections/)) | 파일 없으면 접속 실패 오류 | File ▸ Import/Export ▸ Projects(폴더 통째) |
| **Visual Studio** | `.sln`(프로젝트 경로 = **sln 기준 상대**) · `.vcxproj`(`$(ProjectDir)` · `$(SolutionDir)`) | `.vs/`(숨김 · 사용자 상태) · `*.user` | MSBuild 속성 = 변수 | 없는 프로젝트 = "(unavailable)" | `.vs`·`*.user`는 gitignore |
| **CMake / Cargo / npm** | `CMakePresets.json` · `Cargo.toml`(`path = "../nexa-ui"`) · `package.json` | `CMakeUserPresets.json` · `target/` | **파일 기준 상대** · `${sourceDir}` · `$env{}` | 오류 | 형제 저장소는 `../` 상대(우리 `nexa-ui` 의존이 이미 이 방식) |
| **Notepad++ · Vim · Emacs** | (없음) | `session.xml` · `viminfo` · `desktop` = **절대 경로** | 절대 | 없는 파일은 건너뜀 | 세션은 기기 전용 — 이식 목적이 아니다 |

**읽는 법**: ① 공유 파일의 경로는 **"기준(앵커) + 상대"** 가 표준이고 앵커는 최소 셋(프로젝트 · 홈 · 사용자 정의) ② 다중 루트는 **폴더 이름**으로 참조(VS Code `${workspaceFolder:이름}`)해 순서 변경에 안전 ③ 개인 상태·비밀은 별도 파일 ④ 못 푸는 경로는 **깨진 표시 + 정의 유도**이지 조용히 버리지 않는다 ⑤ "다른 이름으로"는 새 위치 기준 재계산.

---

## 2. OS를 건너 압축·복사할 때 깨지는 것들(경로 자체의 함정)

| 함정 | Windows | macOS | Linux | 규칙 |
|---|---|---|---|---|
| 구분자 | `\` (`/`도 허용) | `/` | `/` | 파일에는 **항상 `/`** · 읽을 때 OS 구분자로(지금 `relativize`가 하는 일) |
| 절대 경로 모양 | `C:\…` · `\\서버\공유` · `\\?\` | `/Users/…` | `/home/…` | 절대는 **OS를 건너면 무의미** → 저장 때 경고 · 앵커로 바꾸도록 유도 |
| 드라이브 | 문자 대소문자 무관(`c:` = `C:`) | — | — | 저장 때 대문자로 정규화(같은 파일이 다른 문자열로 두 번 저장되는 일 방지) |
| 대소문자 | 무시(보존) | 기본 무시(보존 · APFS 옵션) | **구분** | 비교는 OS 규칙(`nsql-bookmarks` `ci` 인자 · `undofile::name_for`와 같은 판정) · 저장은 사용자가 쓴 그대로 |
| 유니코드 정규화 | NFC 그대로 | 파일 시스템이 NFD로 돌려줄 수 있음(HFS+ · APFS는 보존) | 바이트 그대로 | 저장 때 **NFC**로 · 비교 때 양쪽 NFC |
| 홈 | `C:\Users\이름` | `/Users/이름` | `/home/이름` | `${home}` 앵커 하나로 |
| 심볼릭 링크 · 정션 | 정션·심링크 | 심링크 | 심링크 | `canonicalize()` **금지** — 사용자가 연 경로(링크)를 그대로 · 절대화만(`std::path::absolute`) |
| 형제 폴더 | `..\nexa-ui` | `../nexa-ui` | 같음 | 같은 루트(드라이브) 안이면 `..` 상대 허용(VS Code · Cargo 관례) · 다른 드라이브면 상대가 불가 → 앵커/절대 |
| 길이·금지 문자 | 260자(장경로 옵션) · `:*?"<>|` | `:` 금지(Finder) | 거의 없음 | 저장 때 검사하지 않고 열 때 오류로 |

---

## 3. Nexa SQL 지금(96차 후반 · [project.rs](../crates/nexa-sql/src/project.rs))

- 저장: `relativize(p, base)` — `base` = **프로젝트 파일 폴더** 아래면 상대(`/`) · 아니면 절대. 대상 = `folders` · `selected` · `tabs[].path` · `expanded`.
- 읽기: `resolve(rel, base)` — 상대면 `base.join` · 절대면 그대로.
- 못 찾으면: 탭은 **조용히 건너뜀**(상태줄 개수만) · 폴더는 목록에 남아 "없음".
- 구멍 셋: ① **열 때 절대화하지 않는다** → cwd 상대 인자(`nexa-sql a.sql`)가 `"a.sql"`로 새고 다시 열 때 프로젝트 폴더 기준으로 잘못 풀림 ② **프로젝트 폴더 밖은 전부 절대** → 형제 폴더·홈 아래·다른 등록 폴더의 파일은 복사하면 깨짐 ③ 깨진 탭이 **보이지 않는다**.
- 잘 된 것: 구분자 `/` · Save As 때 새 위치 기준 재계산(`with_path` 뒤 저장) · 비밀번호는 안 담김(프로필 **이름**만 `profiles`) · 미저장 본문·북마크가 파일 안에 있어 복사만으로 환경이 따라감.

---

## 4. 개선안

### 4-1. 경로 표기 = 앵커 접두 + 상대(한 문자열 · 사람이 읽고 고칠 수 있게)

```
${project}/sql/a.sql            ← 프로젝트 파일 폴더 아래(지금의 상대와 같음 · 접두는 생략 가능 = 호환)
${folder:nexa-ui}/README.md     ← 등록 폴더 "nexa-ui" 아래(폴더가 어디 있든 그 폴더 정의를 따라감)
${home}/Downloads/x.sql         ← 사용자 홈 아래
${var:DATA}/dump.sql            ← 사용자 정의 변수(프로젝트 파일 vars + 로컬 오버라이드)
D:/other/x.sql                  ← 마지막 수단(저장 때 로그 경고 "이식되지 않는 경로 1개")
```

- **쓰기 우선순위**(가장 좁은 앵커부터): project 아래 → 등록 폴더 아래(가장 깊게 맞는 폴더) → 사용자 변수 → 홈 → 절대. 접두 없는 상대(`sql/a.sql`)는 `${project}`와 같다(**옛 파일 호환** · 새로 쓸 때도 project 아래는 접두 없이).
- **읽기**: 접두를 풀어 절대로. `${folder:이름}`은 `folders[]`의 `name`(없으면 leaf 이름 · 중복이면 `name` 필수)으로 찾는다 — 순서(`${folder:0}`)가 아니라 이름(VS Code `${workspaceFolder:이름}`과 같은 이유 = 재정렬·삭제에 안전).
- 등록 폴더 자체도 같은 규칙: project 아래 → `../형제`(같은 루트) → `${home}` → `${var}` → 절대. 예: `[{ "path": "." }, { "path": "../nexa-ui", "name": "nexa-ui" }, { "path": "${home}/Downloads" }]`.

### 4-2. 사용자 정의 변수(JetBrains · Eclipse · DBeaver 방식의 합)

- 프로젝트 파일 `"vars": { "DATA": "${home}/data" }` = **공유 기본값**(팀이 같은 구조면 그대로 됨).
- 로컬 오버라이드 = `<설정 폴더>/projects/<프로젝트 해시>.local.json` `"vars": { "DATA": "E:/data" }` — 파일에 안 남고 기기마다 다르게. `undofile::name_for` 해시 규약 재사용(70 §2 워크스페이스 id와 같음).
- 설정 창 **Project ▸ Path Variables** 표(이름 · 값 · 출처 = 프로젝트/로컬) · CLI `nsql project vars [set NAME=값]`.

### 4-3. 못 풀 때 = 보이게 + 고칠 길

- 탭: 파일이 없으면 **자리 탭**(제목 + 띠 "원본을 찾을 수 없음: `${var:DATA}/dump.sql`" · [찾아보기] [변수 정의…] [닫기]) — 조용히 건너뛰지 않는다. 미저장 스냅숏(70 §5)이 있으면 그 본문을 읽기 전용으로 보여 준다.
- 등록 폴더: 트리에 "(없음)" + 우클릭 **Relocate…**(폴더 고르기 → `folders[i]` 갱신 · 그 폴더 앵커의 탭·펼침도 같이 살아남).
- 프로젝트를 열 때 **미정의 변수/못 찾은 앵커를 모아 한 번**에 묻는 팝업(JetBrains "Unresolved path variables") — 값을 넣으면 로컬 오버라이드에 저장.

### 4-4. 정규화 규칙(§2에서 도출 · `project::norm` 한 함수)

1. 열기 입구 전부(`open_file` · 인자 · 대화상자 · 최근 · 탐색기 · 북마크 · CLI) → `std::path::absolute()`(**canonicalize 금지**) → `paths[i]`는 늘 절대.
2. 저장 때: 구분자 `/` · Windows 드라이브 대문자 · macOS NFC · `..`는 같은 루트 안에서만(다른 드라이브 = 상대 불가 → 다음 앵커).
3. 비교(중복 탭 · 북마크 문서 키 · 펼침 목록)는 OS 규칙(Windows/macOS 대소문자 무시 · Linux 구분) — 이미 `nsql-bookmarks`가 쓰는 `ci` 인자를 프로젝트에도.
4. 절대 경로가 하나라도 남으면 저장 로그에 "이식되지 않는 경로 n개(절대)" 한 줄 + 설정 `project.warn_absolute`(on).

### 4-5. 한 파일 안의 절 나누기(공유 ↔ 개인 · 사용자 요구와의 절충)

사용자 요구(09-23)는 "폴더째 압축·복사 = 같은 환경" → 탭·캐럿·북마크가 **같이 따라가야** 하므로 타 제품처럼 파일을 둘로 쪼개지 않는다. 대신 **절**로 나눠 목적을 드러낸다:

```json
{ "version": 2,
  "folders": [...], "vars": {...},                 ← 공유(정의)
  "workspace": { "tabs": [...], "active": 1, "expanded": [...], "panel": "project",
                 "search": "...", "bm_collapsed": [...], "profiles": ["BISCM"] },   ← 환경(복제 대상)
  "bookmarks": {...} }
```

- 옛 파일(`version: 1` · 최상위 `tabs`)은 그대로 읽고, 저장은 v2로.
- `profiles`는 **이름만**(비밀번호·호스트는 볼트/프로필 파일 = 안 따라감 · 21 §7) — 다른 기기에서는 "직접 접속" 안내가 이미 있다(70 §6-1). 프로필 정의까지 같이 옮기려면 별도 `nsql conn export`(비밀번호 제외 · DataGrip "비밀번호 없이 복사"와 같은 선)로 — 이 문서 범위 밖(T-173).
- 로컬 전용(기기에 종속)인 것만 `.local.json`으로: 변수 오버라이드 · (나중) 창 위치.

### 4-6. 이식 시나리오 검증표(개선 뒤 기대)

| 시나리오 | 지금 | 개선 뒤 |
|---|---|---|
| 프로젝트 폴더째 zip → 다른 PC 같은 OS | 프로젝트 폴더 아래 ✓ · 밖은 ✗(절대) | 아래 ✓ · 형제 `../x` ✓(같이 압축했으면) · `${home}` ✓ · `${var}` = 묻기 · 절대 = 자리 탭 |
| Windows → macOS/Linux | 아래 ✓ · `C:/…` 전부 ✗ | 위와 같음 + 드라이브 절대만 자리 탭 · NFC/구분자 문제 없음 |
| 폴더 이름만 바꿈 | 아래 ✓ · `${folder}` 없음 → 등록 폴더 밖 탭 ✗ | `${folder:이름}` + Relocate 한 번이면 그 폴더의 탭·펼침 전부 복구 |
| cwd 상대 인자로 연 파일 | ✗(엉뚱한 파일) | 절대화로 ✓ |
| Git으로 공유(개인 상태 제외하고 싶다) | 한 파일이라 불가 | v2 절 구조 + `.gitattributes` filter 또는 `project.share_workspace=off`(저장 때 `workspace` 절 생략 · 개인 상태는 `.local.json`으로) — 선택 |

---

## 5. 구현 단계

| 단계 | 내용 | 크기 |
|---|---|---|
| **P1** | 열기 입구 절대화(`std::path::absolute`) · 저장 정규화(드라이브 대문자 · NFC · `..` 같은 루트) · 절대 경로 경고 로그 · **못 찾은 탭 = 자리 탭**(조용히 건너뛰지 않음) | 소 · 지금 결함 셋을 닫는다 |
| **P2** | `${project}`(생략) · `${folder:이름}` · `${home}` 앵커 쓰기/읽기 · `folders[].name` · 등록 폴더의 `../형제` 상대 · 왕복 시험(3-OS 경로 표본) | 중 |
| **P3** | `${var:NAME}` + 프로젝트 `vars` + 로컬 오버라이드 + 미정의 변수 팝업 + 설정 표 + CLI `nsql project vars` | 중 |
| **P4** | v2 절 구조(`workspace`) · 옛 파일 읽기 호환 · `project.share_workspace` · Relocate 메뉴 · 위키 페이지 | 중 |

부하원: 저장 때 문자열 처리뿐(디스크·스레드 0) · 열 때 `absolute` 1회/파일 — [39 §3](39-resource-governance.md) 등재 대상 아님(수치 0).

---

## 6. 결정(권장안 · 사용자 확인)

| # | 물음 | 권장 |
|---|---|---|
| **D-186** | 앵커 표기 = `${name}` 문자열 접두(사람이 편집 가능) vs `{ "base": "folder", "name": "x", "path": "…" }` 객체 | **`${name}/…` 문자열**(Sublime·VS Code·DBeaver·Eclipse 전부 이 꼴 · diff가 읽힌다) |
| **D-187** | 등록 폴더 참조 = 이름(`${folder:nexa-ui}`) vs 순서 | **이름**(재정렬·삭제 안전 · VS Code) · 중복 이름은 `name` 필수 |
| **D-188** | 절대 경로가 남을 때 = 저장 거부 vs 경고 | **경고 로그 + 저장**(막지 않는다 · 설정 `project.warn_absolute`) |
| **D-189** | 못 찾은 탭 = 건너뜀 vs 자리 탭 | **자리 탭**(찾아보기/변수 정의/닫기 · 스냅숏 있으면 읽기 전용 본문) |
| **D-190** | 파일 구조 = 한 파일 v2 절 나누기 vs 두 파일(`.nsql-project` + `.nsql-workspace`) | **한 파일 v2**(사용자 요구 = 복사만으로 환경 복제) + `project.share_workspace=off` 옵션으로 두 파일 효과 |

**출처**: [Sublime projects](https://www.sublimetext.com/docs/projects.html) · [Sublime build systems 변수](https://www.sublimetext.com/docs/build_systems.html) · [VS Code multi-root workspaces](https://code.visualstudio.com/docs/editing/workspaces/multi-root-workspaces) · [JetBrains path variables](https://www.jetbrains.com/help/idea/absolute-path-variables.html) · [JetBrains VCS 안내](https://intellij-support.jetbrains.com/hc/en-us/articles/206544839-How-to-manage-projects-under-Version-Control-Systems) · [DataGrip 데이터 소스 공유](https://blog.jetbrains.com/datagrip/2018/05/21/copy-and-share-data-sources/) · [Eclipse path variables](https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.user/concepts/cpathvars.htm) · [Eclipse linked resources](https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.user/concepts/concepts-13.htm) · [DBeaver 설정 파일](https://dbeaver.com/docs/dbeaver/Configuration-files-in-DBeaver/) · [DBeaver 연결 관리](https://dbeaver.com/docs/dbeaver/Admin-Manage-Connections/) · [DBeaver 프로젝트](https://dbeaver.com/docs/dbeaver/Projects/).
