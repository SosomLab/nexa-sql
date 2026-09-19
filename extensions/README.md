# extensions/ — Nexa SQL 확장 저장소 루트(공식) · 메타 형식 v1

이 폴더가 **기본 확장 저장소의 루트**다(사용자 09-17 "별도 저장소 없이 nexa-sql 아래 extensions/ 폴더를 루트로"). 앱은
소스 트리에서 실행하면 이 폴더를, 설치본에서는 `https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions`를
같은 구조로 읽는다. 사용자는 **같은 구조의 폴더/URL**을 "Extension Manager: Add Repository"로 더할 수 있다.
설계 전문 = [docs/50 §10](../docs/50-extension-system.md).

```
<root>/
  index.json                 ← 루트 메타: 패키지 폴더 식별(무엇이 있나)
  <package-dir>/
    extension.json           ← 패키지 메타: 설치에 필요한 것(파일 · sha256 · 배치 경로 · 종류)
    <files…>                 ← extension.json의 files[].path
```

## index.json (루트 메타)

| 키 | 필수 | 뜻 |
|---|---|---|
| `format` | ✓ | 메타 형식 버전. 지금은 `1`(다르면 저장소 전체 거부) |
| `name` | | 저장소 표시 이름 |
| `packages[]` | ✓ | 패키지 요약 목록 — 매니저의 "Install Extension" 텍스트 메뉴가 이 줄로 만들어진다 |
| `packages[].id` | ✓ | 확장 id(소문자·숫자·`-`) · 설치 폴더 이름 · 설정 `extensions.disabled` 항목 |
| `packages[].dir` | | 패키지 폴더 이름(비면 id) · `..`/경로 구분자 금지 |
| `packages[].name` | ✓ | 표시 이름 |
| `packages[].version` | ✓ | 버전(semver 문자열) |
| `packages[].kind` | ✓ | `builtin` · `data` · `wasm` · `process`(아래) |
| `packages[].summary` | | 한 줄 설명(메뉴에 그대로) |

## extension.json (패키지 메타)

| 키 | 필수 | 뜻 |
|---|---|---|
| `format` | ✓ | `1` |
| `id` · `name` · `version` · `kind` · `summary` | ✓ | index.json과 같아야 한다(`id`가 다르면 설치 거부) |
| `description` | | 긴 설명 |
| `author` · `license` · `homepage` | | 표시용 |
| `min_app` | | 필요한 최소 앱 버전 |
| `platforms[]` | | `windows` · `macos` · `linux` — 비면 전부. 이 OS가 없으면 설치 거부 |
| `requires[]` | | 먼저 있어야 하는 확장 id(지금은 표시만 · 자동 설치는 후속) |
| `settings_prefix` | | 이 확장이 읽는 설정 키 접두(예 `rainbowpair.`) — 설정 창 "Extensions ▸ <이름>" 분류와 짝 |
| `files[]` | data | 설치 파일 목록 |
| `files[].path` | ✓ | 패키지 폴더 안 상대 경로(`..` · 절대 경로 · 드라이브 금지) |
| `files[].sha256` | ✓ | 파일의 SHA-256(소문자 16진) — 불일치면 설치 거부 |
| `files[].dest` | | 설정 폴더 기준 배치 경로(예 `Packages/Foo/foo.sublime-syntax`) · 비면 보관만 |
| `messages.install` | | 설치 뒤 상태줄/로그에 보여 줄 안내문 |

## 종류(kind)

| kind | 뜻 | 설치가 하는 일 |
|---|---|---|
| `builtin` | 앱에 컴파일된 in-process 확장(예 Rainbow Pairs) | 파일 없음 · **설치 = 켜기 + 설정 분류 표시**(`installed.json` 기록) · 삭제 = 끄기 + 분류 숨김 · 끄기/켜기 = `extensions.disabled` |
| `data` | 문법 · 테마 · 스니펫 · 키맵 같은 **파일 패키지** | `files[]`를 sha256 검증 뒤 `<설정 폴더>/extensions/<id>/<version>/`에 보관하고 `dest`에 배치 · `installed.json`에 기록(삭제 때 되감기) |
| `wasm` · `process` | 코드 확장(docs/50 §2 · T-118) | **아직 설치 불가**(거부 메시지) |

## 설치본 위치(사용자 설정 폴더)

```
<설정 폴더>/extensions/<id>/installed.json       ← id · name · version · kind · placed[](배치한 dest 목록)
<설정 폴더>/extensions/<id>/<version>/…          ← 보관 사본(+ extension.json)
<설정 폴더>/<dest>                                ← 배치 파일(예 Packages/…)
```

## 저장소 주소 표기

| 입력 | 앱이 읽는 주소 |
|---|---|
| `https://github.com/SosomLab/nexa-sql/extensions` | `https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions`(브랜치 `main` 가정) |
| `https://github.com/SosomLab/nexa-sql/tree/main/extensions` | 같음 |
| `https://raw.githubusercontent.com/…/extensions` | 그대로 |
| `/path/to/folder` | 로컬 폴더 |

기본 저장소 = 설정 `extensions.default_repository`(기본값 위 raw 주소). **소스 트리에서 실행하면** 체크아웃의 `extensions/` 폴더를 대신 읽는다(네트워크 0 · push 전에도 같은 메타).

## 사용 순서(처음 한 번)

1. 명령 팔레트(⌘⇧P / Ctrl+Shift+P) → **`Extension Manager: Enable Extension Manager`** — 설정 `extensions.enabled=on`. 이때부터 관리자 명령이 저장소를 읽는다(명령을 실행할 때만 · 자동 조회 없음).
2. `Extension Manager: Install Extension` → 기본 저장소 index.json의 미설치 패키지 목록 → **Rainbow Pairs 1.0.0** 선택 → 설치(builtin이라 파일 없음 · `installed.json` 기록) → 상태줄 안내문.
3. 설치 즉시 켜진다: 괄호·인용부호 깊이 색 · 우클릭 "괄호 이동 ▸" · Ctrl+Alt+, . [ ] · 편집 메뉴 4항목.
4. 설정: Preferences → 그룹 **Extensions ▸ Rainbow Pairs** — `rainbowpair.enabled/quotes/angle/unmatched/match/colors/max_kb`. 바꾸면 즉시 전 탭 반영. (괄호·인용부호 **자동 닫기는 확장 기능이 아니라 편집 코어** — 설정 ▸ 편집기 `editor.auto_close_pairs` · 09-19)

> **단계별 테스트 안내**(설치 전/후 효과 · GitHub 원격 강제 · 실패 경로 · `data` 패키지 로컬 시험) = [docs/50 §13](../docs/50-extension-system.md).
5. 잠시 끄기 = `Extension Manager: Disable Extension`(설정 분류도 숨김) · 되돌리기 = `Enable Extension` · 없애기 = `Remove Extension`(기록 삭제 · 효과 off · 분류 숨김 · 다시 Install 가능).

## 매니저 명령(명령 팔레트 · Sublime "Package Control: …" 표기)

`Extension Manager: Enable Extension Manager`(최초 1회) · `Install Extension` · `Remove Extension` · `List Extensions` · `Enable Extension` · `Disable Extension` ·
`Add Repository`(루트 URL/폴더 입력 · index.json이 읽히면 등록) · `List Repositories` · `Remove Repository`.
설정 = `extensions.enabled` · `extensions.default_repository` · `extensions.repositories`(추가 저장소 · 쉼표) · `extensions.disabled`(끈 확장 id · 쉼표).

## 패키지 추가 절차(공식 저장소)

1. `extensions/<id>/` 폴더에 파일과 `extension.json`을 둔다(sha256은 `shasum -a 256 <file>` / `Get-FileHash`).
2. `index.json`의 `packages[]`에 요약 한 줄을 더한다.
3. PR — 앱은 `main`의 raw URL을 읽으므로 병합 즉시 배포된다(자동 갱신 없음 · 사용자가 Install/Upgrade를 누를 때 읽음).
