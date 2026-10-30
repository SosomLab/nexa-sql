# 단축키와 사용자 키맵 파일(Keyboard Shortcuts)

Nexa SQL의 단축키는 Sublime Text를 따릅니다. 바꾸는 방법은 두 가지입니다.

| 방법 | 어디서 | 특징 |
|---|---|---|
| 단축키 설정 창 | 메뉴 **보기 ▸ 단축키**(또는 설정 창의 [단축키…]) | 명령마다 [지정] → 키 조합 누르기 · [비우기] · [초기화] · 값은 설정 `key.<명령 id>`에 저장 |
| 사용자 키맵 파일 | `<설정 폴더>/Packages/User/Default.sublime-keymap` → `Default (Windows).sublime-keymap` / `Default (OSX).sublime-keymap` / `Default (Linux).sublime-keymap` | Sublime 형식 그대로 · **조건(context)** · 2단 코드(`ctrl+k, ctrl+u`) · 파일이 있을 때만 읽음 |

- 설정 폴더 = Windows `%APPDATA%\nexa-sql` · macOS `~/Library/Application Support/nexa-sql` · Linux XDG `~/.config/nexa-sql` · 환경 변수 `NSQL_HOME`이 있으면 그것이 우선.
- 파일은 **기동 때와 설정이 바뀔 때** 다시 읽습니다. 뒤 파일(OS별)이 앞 파일(`Default.sublime-keymap`)을 덮고, 같은 파일 안에서는 **뒤 바인딩이 우선**입니다(Sublime 규칙).
- 파일에 오류가 있으면 그 파일만 건너뛰고 앱은 그대로 뜹니다(로그에 `[keymap] <경로>: …` 한 줄).

- 기본 단축키가 없는 명령도 단축키 창에서 지정할 수 있습니다. 예 = `view.minimap`(미니맵 켜기/끄기 · 10-30) — 명령 팔레트 "View: Minimap"으로도 토글.

## 파일 형식

```json
[
  { "keys": ["ctrl+alt+9"], "command": "file.save_as" },
  { "keys": ["ctrl+alt+9"], "command": "edit.upper_case",
    "context": [ { "key": "num_selections", "operand": 2 } ] },
  { "keys": ["ctrl+k", "ctrl+9"], "command": "file.new",
    "context": [ { "key": "selection_empty" } ] }
]
```

- `keys` = 키 조합 1개(단일) 또는 2개(2단 코드). `//` 주석과 끝 쉼표를 허용합니다.
- `command` = 이 앱의 명령 id(명령 팔레트의 id) 또는 흔한 Sublime 명령 이름(`save` · `upper_case` · `duplicate_line` 등 → 자동 대응).
- 조건 없는 바인딩은 **기본 키에 덧붙습니다**(기본 키도 그대로 동작). 다른 명령이 쓰던 조합을 가져오면 그 명령에서는 빠집니다.
- 단축키 설정 창에서 파일 바인딩은 단축키 뒤에 **"(파일)"** 이 붙고, 조건 있는 바인딩은 목록 끝에 **"조건 …"** 행(읽기 전용)으로 보입니다.
- **파일이 창보다 우선합니다**(D-397). 창에서 [지정]한 조합을 파일이 다른 명령에 쓰고 있으면 그 행에 경고 `Keymap file: {조합} = {명령} (file wins)`가, 파일이 키를 지정한 명령을 창에서 [비우기]하면 `Keymap file binds {조합} (file wins; edit the file)`가 보입니다 — 바꾸려면 키맵 파일을 고치세요. 조건 있는 바인딩은 조건이 참일 때만 쓰이므로 이 안내 대상이 아닙니다.

## 조건(context) 키

`{ "key": …, "operator": …, "operand": …, "match_all": … }` · 연산자 = `equal`(기본) · `not_equal` · `regex_match` · `not_regex_match` · `regex_contains` · `not_regex_contains`. 선택(커서)이 여럿이면 값이 배열이고, `match_all: true`면 전부, 없으면 **첫 선택만** 봅니다(Sublime 규칙).

| 키 | 값 |
|---|---|
| `selection_empty` | 선택이 비었는가(true/false) |
| `has_selection` | 선택이 있는가 |
| `num_selections` | 커서·선택 개수 |
| `preceding_text` | 커서 앞(같은 줄)의 글 · 선택마다 |
| `following_text` | 커서 뒤(같은 줄)의 글 · 선택마다 |
| `text` | 선택한 글 · 선택마다 |
| `read_only` | 읽기 전용 탭인가 |
| `focus` · `panel` | 지금 포커스가 있는 곳 = `editor` · `grid`(결과 그리드) · `explorer` · `find` · `search` · `extensions` · `project` · `bookmarks` · `details` · `outline` |
| `panel_visible` · `panel_has_focus` | **찾기/바꾸기 패널**이 보이는가 · 그 패널에 포커스가 있는가 |
| `overlay_visible` | 명령 팔레트 창이 떠 있는가 |
| `auto_complete_visible` | 코드 완성 팝업이 떠 있는가 |
| `connected` · `dialect` | **현재 세션**이 접속돼 있는가 · 서버 유형 표시 이름(`Oracle` · `SQL Server` · `PostgreSQL` · `MySQL` · `SQLite` · `ODBC` · `DuckDB` · `ClickHouse` · `Db2` · `MS Access` — 접속 전에도 세션의 기본 유형이 나옴) |
| `setting.<키>` | 설정 값(예 `setting.ui.lang`) |
| `selector` | 커서 위치의 구문 스코프 — `source.sql` + 문자열 `string.quoted.sql` · 주석 `comment.sql` · 예약어 `keyword.sql` · 숫자 `constant.numeric.sql`(강조기 없는 탭 = `text.plain`) |

`selector`는 Sublime 선택자의 부분집합을 씁니다: `,`·`|` = 택일 · 공백 = 후손(순서대로) · `-` = 제외 · 점 경계 접두 일치(`string`은 `string.quoted.sql`에 맞음). `equal`/`not_equal` = 선택자 일치 여부 · 정규식 연산자는 스코프 글에 적용. 예 = 문자열 안에서만:

```json
{ "keys": ["ctrl+alt+q"], "command": "edit.upper_case",
  "context": [{ "key": "selector", "operator": "equal", "operand": "source.sql string" }] }
```

확인 = 기동 명령 `keymap.dump:`의 `key selector = …` 줄.
