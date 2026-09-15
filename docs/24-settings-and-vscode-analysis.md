# 24 · 앱 설정 체계 — VS Code 설정 방식(소스) · 설정 UI 구성(캡처) 분석 → Nexa SQL 적용

> **요청**(사용자 09-14): *"VS Code의 설정 방식을 오픈된 소스 수준에서 확인하고, 샘플 이미지를 기준으로 UI 구성도 분석"* + 같은 날 *"i18n(기본 영어) · 테마 System/Light/Dark(기본 System) 구현"*.
> **근거**: `microsoft/vscode` `main` 소스 4개 파일(§1 표) 직접 확인 + 사용자 캡처 8장(Settings 편집기 · Commonly Used / Text Editor › Files / Workbench › Appearance / Application › Network / 드롭다운 열림 / 테마 목록) — 2026-09-14.
> **적용 실체**: `nsql-settings`(레지스트리 + `settings.conf`) · `nsql-i18n` · `nsql config` · GUI 단축키(T-37/38 ✅ 09-14). 설정 **화면**은 §4의 설계로 T-39.

---

## 1. VS Code 설정 방식 — 소스에서 확인한 것

| 파일(`src/vs/…`) | 역할 | 확인한 사실 |
|---|---|---|
| `platform/configuration/common/configurationRegistry.ts` | **설정 레지스트리**(단일 원천) | `IConfigurationNode { id, order, title, properties, allOf }` · 항목 `IConfigurationPropertySchema { type, default, description/markdownDescription, enum/enumDescriptions, scope, tags, order, deprecationMessage, restricted, included, ignoreSync, policy … }` · `ConfigurationScope` = APPLICATION / MACHINE / APPLICATION_MACHINE / WINDOW / RESOURCE / LANGUAGE_OVERRIDABLE / MACHINE_OVERRIDABLE · `[language]` 오버라이드 키는 `OVERRIDE_PROPERTY_REGEX = ^(\[[^\]]+\])+$`로 직접 등록 금지(`configurationDefaults`로만) |
| `platform/configuration/common/configuration.ts` | **계층 해석** | `ConfigurationTarget` = APPLICATION · USER · USER_LOCAL · USER_REMOTE · WORKSPACE · WORKSPACE_FOLDER · DEFAULT · MEMORY · `IConfigurationValue { defaultValue, policyValue, applicationValue, userValue, userLocalValue, userRemoteValue, workspaceValue, workspaceFolderValue, memoryValue, value, overrideIdentifiers }` — 최종 `value`는 **default → user → workspace → folder → memory** 순으로 덮고 **policy가 최우선** |
| `workbench/contrib/preferences/browser/settingsLayout.ts` | **설정 편집기 목차(TOC)** | `tocData`: Editor(editor/cursor · find · font · format · diffEditor · multiDiffEditor · minimap · suggestions · files) · Workbench(appearance · breadcrumbs · editor · settings · zenmode · screencastmode · browser) · Window(newWindow) · Chat(agent · appearance · sessions · tools · mcp · context · inlineChat · misc) · Features(accessibility · explorer · search · debug · testing · scm · extensions · terminal · task · problems · output · comments · remote · timeline · notebook · mergeEditor · issueReporter) · Application(proxy · keyboard · update · telemetry · settingsSync · network · experimental · other) · Security(workspace). **각 노드는 `settings: ['editor.font*', …]` 글롭 패턴**으로 항목을 끌어온다 — 항목이 TOC를 모르고 TOC가 항목을 고른다. `commonlyUsedData` = `editor.fontSize · editor.formatOnSave · files.autoSave · editor.defaultFormatter · editor.fontFamily · editor.wordWrap · chat.agent.maxRequests · files.exclude · workbench.colorTheme · editor.tabSize · editor.mouseWheelZoom · editor.formatOnPaste`(캡처 1·2·7과 일치) |
| `workbench/contrib/preferences/browser/settingsTree.ts` | **항목 렌더러** | 스키마 `type` → 렌더러: `boolean`→`SettingBoolRenderer`(체크박스 + 설명 인라인) · `number/integer`→`SettingNumberRenderer` · `string`→`SettingTextRenderer`/`MultilineText` · `enum`→`SettingEnumRenderer`(드롭다운 · 항목별 설명 · `default` 배지) · `array`→`SettingArrayRenderer`(추가/삭제/편집) · `object`→`SettingObjectRenderer`/`BoolObject` · 복합→`SettingComplexRenderer`("Edit in settings.json" 링크) · exclude/include 전용 · 확장 토글. 공통 헤더 = **카테고리 + 이름** · 변경 표시(`.modified`) · 스코프/동기화 무시/정책 배지 · 설명은 마크다운(`#설정키#` = 다른 설정으로 가는 링크) · 항목 메뉴(⚙) = Reset Setting · Copy Setting ID · Copy Setting as JSON · Copy Setting as URL · (조건부) Apply to All Profiles · Sync This Setting |

### 1-1. 저장 형식과 위치(공개 문서 기준)

- 사용자: `settings.json`(JSONC · 주석 허용) — Windows `%APPDATA%\Code\User\settings.json` · macOS `~/Library/Application Support/Code/User/settings.json` · Linux `~/.config/Code/User/settings.json`.
- 작업 영역: `<repo>/.vscode/settings.json`(WINDOW·RESOURCE 스코프만 허용 · APPLICATION/MACHINE은 무시 — 저장소 복제로 기기 설정이 따라오지 않게).
- **파일에는 기본값과 다른 값만** 적힌다. 기본값은 코드(레지스트리)에 있고 `defaultSettings.json`은 읽기 전용 생성물.
- 확장은 `package.json` `contributes.configuration`으로 같은 레지스트리에 항목을 등록한다 → 설정 편집기·검색·IntelliSense가 자동으로 안다.
- 언어별 오버라이드 `"[sql]": { "editor.tabSize": 2 }` · 프로필(Profiles)별 사용자 설정 · Settings Sync(항목별 `ignoreSync`) · 조직 정책(`policy`)이 최상위.

### 1-2. 한 줄 원리

**레지스트리 하나(스키마 + 기본값 + 설명) → 계층별 "변경분" 파일 → 화면·검색·검증·JSON 편집이 전부 그 레지스트리에서 생성된다.** 항목을 추가하는 사람은 스키마 한 덩이만 쓴다.

---

## 2. 설정 UI 구성 — 캡처 8장 분석

```
┌ Settings 탭 ──────────────────────────────────────── [JSON 열기] [분할] [전체화면] [✕] ┐
│ [🔍 Search settings (⇅ for history)                             ] [지우기] [필터 ▾] [⧩] │
│ [User] [Workspace]                                            Last synced: 0 secs ago  │
├────────────────┬────────────────────────────────────────────────────────────────────────┤
│ Commonly Used  │ Commonly Used                          ← H1 = 현재 TOC 노드            │
│ › Text Editor  │ ───────────────                                                         │
│   Cursor       │ Editor: Font Size                      ← "카테고리: 이름"(이름만 굵게)   │
│   Find         │ Controls the font size in pixels.      ← 설명(다른 설정 링크·코드 스팬) │
│   Font  …      │ [14        ]                           ← 숫자 입력                      │
│ › Workbench    │                                                                         │
│   Appearance ← │ Editor: Format On Save                                                  │
│ › Window       │ ☐ Format a file on save. A formatter…  ← 불리언 = 체크 + 설명 인라인     │
│ › Chat         │                                                                         │
│ › Features     │ Files: Auto Save                                                        │
│ › Application  │ Controls auto save of editors…                                          │
│   Proxy …      │ [off                              ▾]   ← enum 드롭다운                  │
│ › Security     │                                                                         │
│ › Extensions   │ ▌Editor: Font Family                   ← ▌= 변경됨(왼쪽 세로 강조 바)   │
│                │  [D2Coding, Consolas, …          ]                                      │
│                │ Editor: Word Wrap (Modified elsewhere) ← 다른 스코프에서 바뀜 표시       │
│                │ ⚙ ┌───────────────────────────┐        ← 호버 = 카드 배경 + 왼쪽 ⚙ 메뉴  │
└────────────────┴────────────────────────────────────────────────────────────────────────┘
```

| 영역 | 구성 요소 | 관찰(캡처) |
|---|---|---|
| **헤더** | 탭 제목 "Settings" · 우측 아이콘 4개(설정 JSON 열기 · 편집기 분할 · 최대화 · 닫기) | 편집기 탭으로 열린다(모달 아님) — 여러 개 동시에 · 다른 탭과 나란히 |
| **검색줄** | 플레이스홀더 *"Search settings (⇅ for history)"* · 우측 지우기 · 필터 메뉴(`@modified` `@id:` `@ext:` `@feature:` `@tag:` `@lang:`) · 검색 결과 수 | 입력 즉시 TOC·본문이 결과로 필터 |
| **스코프 탭** | `User` / `Workspace`(폴더 열면 `Folder` 추가) · 우측 *"Last synced: 0 secs ago"* | 탭이 **저장 대상 파일**을 고른다 — 같은 항목이 스코프별로 다른 값 |
| **좌측 TOC** | 2단 트리(`›` 접힘) · 현재 섹션 **굵게 + 강조 상자** · 스크롤과 동기(본문을 내리면 TOC 강조가 따라옴 · 캡처 3·4) · Commonly Used가 맨 위 | 항목 수가 많을 때의 유일한 길찾기 · 폭 ≈ 230px 고정 |
| **본문 헤딩** | H1 = 최상위(Workbench) · H2 = 하위(Appearance) · 섹션 사이 얇은 구분선 | 캡처 4: "Workbench / Appearance" · 캡처 3: "Application / Network" |
| **항목 헤더** | `카테고리: 이름` — 카테고리는 보통, **이름만 굵게** · 하위 카테고리는 `Chat › Agent: Max Requests`(캡처 2) · 부가 표시 *(Modified elsewhere)* · *(Applies to all profiles)* 이탤릭 | 키 `chat.agent.maxRequests` → 표시 "Chat › Agent: Max Requests" — **키를 사람이 읽는 제목으로 자동 변환**(camelCase 분리 · 첫 글자 대문자) |
| **설명** | 한두 문장 · 다른 설정은 **파란 링크**(`Files: Auto Save` · `Window: Auto Detect Color Scheme`) · 값은 `코드 스팬`(`afterDelay` · `.gitignore`) | 링크 클릭 = 그 설정으로 스크롤+강조 |
| **컨트롤** | 숫자 = 좁은 입력(≈200px) · 불리언 = 체크박스 + 설명이 컨트롤 오른쪽에 인라인 · enum = 드롭다운(≈320px · 열면 항목 + 오른쪽 `default` 배지 + 아래 설명 패널 · 캡처 5) · 문자열 = 넓은 입력(≈420px) · 배열 = 항목 목록(호버 시 ✎ ✕ · **`Add Pattern` 파란 버튼** · 캡처 8은 편집 중 행이 입력 상자로 바뀜) · 복합 = *"Edit in settings.json"* 링크(캡처 4 Color Customizations) | 컨트롤 폭이 **종류별로 고정** — 값 길이에 따라 흔들리지 않는다 |
| **상태 표시** | 변경된 항목 = **왼쪽 세로 강조 바**(캡처 1 Font Family · 7) · 호버 = 카드 배경 + 왼쪽 밖 ⚙(항목 메뉴) · 포커스 = 파란 테두리 | 값이 기본과 다른지 한눈에 · 초기화는 ⚙ → Reset |
| **테마 드롭다운** | Preferred Dark Color Theme: 목록에 이름 + 회색 설명(테마 표시명) · 현재값 `default` 배지 · 항목 ≈ 25개 스크롤 | "Color Theme"과 "Preferred Dark/Light"가 **분리** — Auto Detect Color Scheme(시스템 추종) 켜면 후자가 쓰인다 |

### 2-1. 캡처에서 읽히는 설계 원칙

1. **하나의 긴 스크롤 페이지 + 좌측 목차** — 탭/대화상자로 쪼개지 않는다. 검색이 1급이고 목차는 보조.
2. **항목 = 제목 · 설명 · 컨트롤 3줄 카드** — 종류가 달라도 리듬이 같다(세로 간격 ≈ 100px).
3. **컨트롤 폭 고정 · 좌측 정렬** — 눈이 한 열만 훑는다.
4. **변경분이 보인다** — 세로 바 · "Modified elsewhere" · `default` 배지 · `@modified` 필터. 사용자가 "내가 뭘 바꿨지"를 항상 답할 수 있다.
5. **JSON 탈출구** — 복합 값·대량 편집은 텍스트 파일로. UI가 모든 형태를 그리려 하지 않는다.
6. **스코프는 탭** — User/Workspace를 화면 상단에서 고르고, 같은 화면이 다른 파일에 쓴다.

---

## 3. Nexa SQL 적용 — 무엇을 가져오고 무엇은 안 가져오나

| VS Code | Nexa SQL(결정) | 이유 |
|---|---|---|
| 레지스트리(`IConfigurationNode`) | ✅ `nsql-settings::REGISTRY` — `Entry { key, cat, label, desc, kind, default }` (✅ 09-14) | 단일 원천 → CLI·GUI·검색이 같은 표. nexa-clip `settings_registry.rs` 선례 |
| `settings.json`(JSONC) | ✅ **`settings.conf`(key=value · nexa-conf)** — 기본값과 다른 값만 · 모르는 키 보존 · 원자적 쓰기 | 워크스페이스에 JSON 파서 0(core 의존 0) · nexa 계열 공통 형식 · 손상에도 앱이 뜬다 |
| 계층 default → user → workspace | ✅ **default → user** 지금 · **프로젝트 `.nexa/settings.conf`**는 [18 세션·프로젝트](18-session-and-projects.md) 착수 시(T-39b) | 프로젝트(스크립트 폴더)별 방언·인코딩·포맷 규칙이 실제 필요 |
| `[language]` 오버라이드 | ❌ 지금은 없음 — 방언별(`[oracle]` `[mssql]`)로 재해석 가능성만 메모 | 편집기 설정이 생기면(E1~) 판단 |
| 프로필 · Settings Sync · 정책 | ❌ | 단일 사용자 도구 · 서버 없음(DR) |
| 설정 편집기 UI(§2) | 📐 **T-39** — 좌측 TOC(카테고리) · 검색줄 · 스크롤 본문 · 항목 카드(제목·설명·컨트롤) · 변경 바 · Reset · "파일 열기" 링크 | 캡처 원칙 6개 그대로. 컨트롤은 nexa-ctl(Combo·Checkbox·TextBox) · 목차는 nexa-dir2 `columns/rows` 이식 후 |
| `@modified` 검색 | ✅ 데이터는 있음(`Settings::is_modified`) · UI는 T-39 | |
| Commonly Used | T-39에서 `REGISTRY` 순서 상위 N개 = 자주 쓰는 것(레지스트리에 `common: bool` 추가) | |
| 키 → 제목 자동 변환 | ❌ **i18n 키를 명시**(`label: Msg`) | 한국어에는 자동 변환이 없다(DR-19 한글 1급) |
| 테마 = Color Theme + Preferred Dark/Light + Auto Detect | ✅ 축소판: `ui.theme = system|light|dark` 하나 · System = OS 추종 · 팔레트는 `nexa_ctl::Theme::{dark,light}` · **편집기 컬러스킴(`.sublime-color-scheme`)은 별도 키 `editor.color_scheme`(T-18)** | 지금 팔레트가 둘뿐. 커스텀 팔레트가 생기면 VS Code처럼 `theme.dark`/`theme.light` 선호 키를 추가(nexa-beep `theme.dark.*` 선례) |

### 3-1. 지금 구현된 것(09-14 · T-37/38)

| 층 | 실체 |
|---|---|
| 카탈로그 | `nsql-i18n` — `Lang {En(기본), Ko}` · `Msg`(58키) · `tr/t/tf` · 빈 한국어 칸 = 영어 폴백 · 자리표시자 `{0}` |
| 설정 | `nsql-settings` — `REGISTRY` 4항목(`ui.lang` `ui.theme` `ui.font_size` `editor.font_size`) · `Settings::{open_default,get,set,reset,save,is_modified,list}` · `ThemeMode::{System,Light,Dark}::is_dark(os)` |
| 파일 | `<설정 폴더>/nexa-sql/settings.conf`(`NSQL_HOME` 우선 · vault와 같은 폴더) |
| CLI | `nsql config list|get|set|reset|path` · 부팅 시 `ui.lang` 적용 |
| GUI | 부팅 시 언어·테마·글꼴 크기 적용 · `Ctrl/⌘+⇧T` 테마 순환 · `Ctrl/⌘+⇧L` 언어 전환(즉시 저장 · 즉시 반영) · OS 다크 전환(`ThemeChanged`) 추종 · 명시 모드는 창 제목줄도 동기(`Window::set_theme`) · nexa-ctl 우클릭 메뉴 라벨 주입 |
| OS 판정 | `nexa-sql/src/theme.rs` — Windows 레지스트리(advapi32 직접) · macOS `defaults read` · Linux `gsettings` · winit `Window::theme()` 우선 |

### 3-2. 남은 것

- **T-39** 설정 화면(§2 구성) — 메뉴바(`nexa-dir2` menubar 이식) 뒤. 그 전까지는 단축키 + `nsql config`.
- CLI의 나머지 한국어 문자열(`run/shell/export/conn` 사용법·오류)을 `Msg`로 — 지금은 `config`·GUI만 카탈로그.
- `ui.font_size`/`editor.font_size` GUI 변경 경로(설정 화면) — 값은 이미 읽는다.
- 방언별 오버라이드 · 프로젝트 스코프(T-39b).

## 4. 카테고리 트리 — DBeaver Preferences 차용 (사용자 09-15)

DBeaver의 Preferences 트리(General / User Interface / Editors / Connections / Data Editor)를 그대로 상위 그룹으로 쓰고, 우리 카테고리를 아래에 배치한다. 단일 원천 = `nsql-settings::CATEGORY_TREE` · **설정 화면(`prefs_win.rs` · 09-15 ✅ 1차)** 사이드바와 `nsql config list` 머리글이 같은 표를 읽는다.

| 그룹(DBeaver) | 카테고리 | 키(예) |
|---|---|---|
| **General** | Log(로그) · Session(세션) | `log.format` · `session.mode` |
| **User Interface** | Appearance(모양 = DBeaver "Colors and Fonts"+"Appearance") · Input(입력) · Keys(단축키 · 09-15 ✅) · Window(창) · Object explorer(= DBeaver Navigator) | `ui.lang/theme/font_size` · `ui.hover_color/pressed_color` · `ui.fade_*` · `tabs.*` · `input.scroll_natural` · `window.focus` · `explorer.*` · `key.<명령>`(Sublime 기본 · 캡처 창) |
| **Editors** | SQL Editor(편집기) · (들여쓰기 [31](31-indentation-settings.md) · T-69) | `editor.*` |
| **Connections** | Connection(접속: 프로필·신호등·시도 상한) | `probe.*` · `connect.*` · `conn.*` |
| **Data Editor** | Result Sets(결과 셋) | `grid.max_rows`(= DBeaver ResultSet fetch size 200) · `grid.row_numbers` · `grid.scroll` · `grid.font_size` |

DBeaver에 있으나 아직 없는 것(자리만): Drivers(드라이버 관리 · T-28) · Transactions(T-54) · Network profiles/SSH(계획 없음) · Confirmations(삭제 확인은 `conn.delete_confirm_ms`로 대체). 비노출 설정([`HIDDEN`])은 같은 카테고리에 속하되 목록·화면에서 기본 숨김.
