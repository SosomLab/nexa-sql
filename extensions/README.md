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
| `builtin` | 앱에 컴파일된 in-process 확장(예 Rainbow Pairs) | 파일 없음 · "설치/삭제" 대신 **켜기/끄기**(`extensions.disabled`) |
| `data` | 문법 · 테마 · 스니펫 · 키맵 같은 **파일 패키지** | `files[]`를 sha256 검증 뒤 `<설정 폴더>/extensions/<id>/<version>/`에 보관하고 `dest`에 배치 · `installed.json`에 기록(삭제 때 되감기) |
| `wasm` · `process` | 코드 확장(docs/50 §2 · T-118) | **아직 설치 불가**(거부 메시지) |

## 설치본 위치(사용자 설정 폴더)

```
<설정 폴더>/extensions/<id>/installed.json       ← id · name · version · kind · placed[](배치한 dest 목록)
<설정 폴더>/extensions/<id>/<version>/…          ← 보관 사본(+ extension.json)
<설정 폴더>/<dest>                                ← 배치 파일(예 Packages/…)
```

## 매니저 명령(명령 팔레트 · Sublime "Package Control: …" 표기)

`Extension Manager: Install Extension` · `Remove Extension` · `List Extensions` · `Enable Extension` · `Disable Extension` ·
`Add Repository`(루트 URL/폴더 입력 · index.json이 읽히면 등록) · `List Repositories` · `Remove Repository`.
설정 = `extensions.repositories`(추가 저장소 · 쉼표) · `extensions.disabled`(끈 확장 id · 쉼표).

## 패키지 추가 절차(공식 저장소)

1. `extensions/<id>/` 폴더에 파일과 `extension.json`을 둔다(sha256은 `shasum -a 256 <file>` / `Get-FileHash`).
2. `index.json`의 `packages[]`에 요약 한 줄을 더한다.
3. PR — 앱은 `main`의 raw URL을 읽으므로 병합 즉시 배포된다(자동 갱신 없음 · 사용자가 Install/Upgrade를 누를 때 읽음).
