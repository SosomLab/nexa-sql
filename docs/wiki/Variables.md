# 변수

두 종류가 있습니다.

| 종류 | 문법 | 어디서 계산 |
|---|---|---|
| **바인드 변수** | `VARIABLE v NUMBER` · `EXEC :v := 값` · `EXEC :v := 식` · 문장 안의 `:v` | 리터럴 대입은 클라이언트(왕복 0) · 식은 서버 · 값은 진짜 바인드로 전달 |
| **치환 변수** | `DEFINE v = 글` · `&v` · `&&v` · `${v[:형식]}` · `${env:이름}` | 클라이언트가 문장을 보내기 전에 글자로 바꿈 |

## 변수 안의 변수 — 언제 펴지나

```
DEFINE v1 = 2
DEFINE v2 = &v1 + 5
DEFINE v1 = 5
SELECT &v2 FROM dual;    -- 대입 시: 2 + 5 · 사용 시: 5 + 5
```

설정 **Session ▸ 변수 안의 변수 확장 시점**(`vars.expand_at`):

| 값 | 뜻 |
|---|---|
| `assign`(기본) | `DEFINE`할 때 바꿉니다(SQL*Plus · psql · sqlcmd와 같음). 나중에 `v1`을 바꿔도 `v2`는 그대로. |
| `use` | 원문을 보관하고 **쓸 때마다** 재귀로 펴 줍니다(깊이 16 · 순환 참조는 오류). `DEFINE` 목록과 변수 창은 `원문 → 현재 값`으로 보여 줍니다. 바인드 식(`EXEC :v2 := :v1 + 5`)도 `:v1`이 바뀐 뒤 `:v2`를 쓰는 문장 앞에서 **자동으로 다시 계산**합니다(서버 왕복 1회 · 바뀌지 않았으면 0). |

## 변수 창(View ▸ Variables)

바인드 변수(탭 층 · 연결 공유 층)와 치환 변수(`&이름` · 타입 `DEFINE`)를 한 표에서 봅니다. 값을 고치면 다음 실행에 반영됩니다. 비밀로 보이는 이름(`PASSWORD` 등)은 가려집니다.

## 자주 쓰는 것

- `${v:q}` = SQL 글자 상수로 감싸기 · `${v:id}` = 식별자 인용 · `${env:PATH}` = OS 환경 변수.
- `SET DEFINE OFF` = `&`를 글자로.
- 스크립트 인자 `&1 &2 …` = `nsql run file.sql arg1 arg2`.

## 내장 변수 — `${workspaceFolder}`처럼(설정 `vars.intrinsic` · 기본 켬)

앱이 아는 값을 VS Code와 같은 이름·문법으로 스크립트와 경로 설정(`log.file` · `oracle.client_dir` · `oracle.tns_admin`)에서 씁니다. 찾는 순서 = **`DEFINE` 변수 → 내장 변수 → 글자 그대로**, `${env:이름}`은 **내장 별칭(`NSQL_*`) → OS 환경 변수**.

| 이름 | 값 | 별칭(`${env:…}`로도) |
|---|---|---|
| `workspaceFolder` · `workspaceFolderBasename` · `workspaceFile` · `workspaceName` | 프로젝트 파일 폴더 · 그 이름 · 프로젝트 파일 · 프로젝트 이름(프로젝트가 없으면 없음) | `NSQL_PROJECT_DIR` · `NSQL_PROJECT_FILE` · `NSQL_PROJECT_NAME` |
| `workspaceFolder:이름` | 등록 폴더(마지막 폴더 이름으로) | — |
| `file` · `fileDirname` · `fileBasename` · `fileBasenameNoExtension` · `fileExtname` · `relativeFile` · `relativeFileDirname` · `fileWorkspaceFolder` | 활성 탭의 파일(미저장 스크립트면 없음) | `NSQL_FILE` · `NSQL_FILE_DIR` · `NSQL_FILE_NAME` |
| `lineNumber` · `columnNumber` | 캐럿(1 기준) | — |
| `userHome` · `nsqlHome` · `cwd` · `execPath` · `pathSeparator`(=`/`) · `os` | 홈 · 앱 설정 폴더 · 현재 폴더 · 실행 파일 · OS 구분자 · `windows`/`macos`/`linux` | `NSQL_USER_HOME` · `NSQL_HOME` · `NSQL_CWD` · `NSQL_EXEC` · `NSQL_OS` |
| `profile` · `dialect` | 활성 접속 프로필 이름 · 방언 | `NSQL_PROFILE` · `NSQL_DIALECT` |
| `config:키` | 설정 값(예 `${config:db.fetch_size}`) | — |

형식 접미도 됩니다: `${fileBasename:q}` = `'a.sql'`. 모르는 이름·없는 문맥은 글자 그대로 남습니다(묻지 않음). CLI(`nsql run`)에는 프로젝트가 없어 `${workspaceFolder}`가 없고 `${file}`은 스크립트 경로입니다.

```
SPOOL ${workspaceFolder}/out/${fileBasenameNoExtension}.log
SELECT '${profile}' AS who, '${env:NSQL_PROJECT_DIR}' AS here FROM dual;
```
