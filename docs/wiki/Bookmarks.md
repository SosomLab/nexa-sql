# 북마크

줄에 표식을 남기고 돌아오는 기능입니다. 편집·외부 수정 뒤에도 **줄 본문과 앞뒤 문맥**으로 자리를 다시 찾습니다(못 찾으면 "무효"로 남기고, 30일 뒤 정리).

## 키

| 동작 | Windows/Linux | macOS |
|---|---|---|
| 토글 | Ctrl+F2 · **Ctrl+K, Ctrl+K** | ⌘F2 · **⌘K, ⌘K** |
| 다음 / 이전 | F2 / Shift+F2 · **Ctrl+K, Ctrl+N / Ctrl+K, Ctrl+P** | F2 / ⇧F2 · **⌘K, ⌘N / ⌘K, ⌘P** |
| 이 문서의 북마크 전부 지우기 | Ctrl+Shift+F2 · **Ctrl+K, Ctrl+X** | ⌘⇧F2 · **⌘K, ⌘X** |
| 북마크 줄 전부 멀티커서 선택 | Alt+F2 | ⌥F2 |
| **니모닉 지정** 0~9 | Ctrl+Shift+숫자 | ⌘⇧숫자 |
| 니모닉으로 이동 | Ctrl+숫자 | ⌘숫자 |
| 북마크 패널 | Ctrl+Shift+B | ⌘⇧B |

니모닉을 지정할 때 그 줄에 북마크가 없으면 만들어서 지정합니다. 굵은 키는 Visual Studio식 **두 단계 코드**(Ctrl+K를 누른 뒤 다음 키) — HHKB처럼 Ctrl+F1~F4를 키보드 펌웨어가 가로채는 키보드용 두 번째 바인딩입니다(둘 다 됩니다 · Preferences ▸ Keys에서 바꿀 수 있음).

편집기 왼쪽 배치는 **북마크 영역(색 띠 · 니모닉 상자) → 줄 번호 → 본문 → 미니맵** 순입니다. 줄 번호를 꺼도(`editor.line_numbers` off) 북마크 영역은 그대로 남고, 줄 변경 표시(초록/노랑 막대)는 줄 번호 오른쪽에 따로 있어 북마크와 겹치지 않습니다. 거터를 완전히 없애고 싶으면 대신 미니맵 점과 줄 끝 라벨(`bookmark.inline_label`)로 위치를 확인하세요.

## 표시

- 거터 색 띠(무효 = 흐림) · 니모닉 숫자 상자 · **미니맵 오른쪽 가장자리 점** · **줄 끝 라벨**(`◆ 라벨` · 흐린 글) — 설정 Bookmarks ▸ Display에서 각각 끌 수 있고, 라벨 길이 상한은 `bookmark.inline_label_chars`(40).
- 상태줄 `BM 3/12` = 이 문서 / 전체.

## 패널(활동 막대 ▸ 북마크)

그룹 → 문서 → 항목 트리 · 필터(Aa · ab · (.*) 토글) · **한 번 클릭 = 미리보기 탭**으로 열어 그 줄로(편집하면 정식 탭) · **더블클릭 · Enter · 메뉴 Open = 정식 탭**(프로젝트 탐색기와 같은 규칙 · 이미 미리보기로 열려 있으면 승격) · 우클릭: 열기 · 니모닉 · 그룹 이동 · 제거(5초 안에 토스트 클릭 = 되돌리기) · 문서/그룹 메뉴(전부 선택 · 이름 바꾸기 · 기본 그룹 · 켜기/끄기 · 삭제) · 무효 북마크 전부 지우기.

## 저장 위치

- 프로젝트가 열려 있으면 **프로젝트 파일** 안(`bookmarks`) — 프로젝트와 함께 이동·공유.
- 프로젝트가 없으면 사용자 설정 폴더의 `workspaces/default.nsql-workspace`.

## CLI

```
nsql bookmark list [--project <file>] [--doc <path>] [--md]
nsql bookmark add <file> <line> [--label <text>] [--project <file>]
nsql bookmark rm <id> [--project <file>]
nsql bookmark prune [--days N] [--project <file>]
```

`--project x.nsql-project`를 주면 그 프로젝트 파일 안의 북마크를 읽고 씁니다(다른 내용은 그대로).
