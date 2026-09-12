# 14 · 영역별 폰트 · 기능 모듈(패키지식) 설계

> 사용자(09-12): *"쿼리 파트와 결과 파트(옵션)를 제외하곤 일반 폰트로 UI를 구성. 다른 앱들처럼 영역별 폰트 구성이 필요. Syntax Highlight · SQL format 등 기능을 위한 설계를 구성해두고 하나씩 구현. Sublime 패키지처럼 기능을 별도로 구축."*

## 1. 영역별 폰트 (`nexa-ctl::FontSlot` 매핑 · `nexa-font`)

| 영역 | 슬롯 | 기본 얼굴 | 설정 키 |
|---|---|---|---|
| UI 전반(메뉴·툴바·연결 필드·설정·오브젝트 트리) | `Base` | 한글 UI 본(Apple SD Gothic Neo / 맑은 고딕 / Noto CJK KR) | `ui.font_family` · `ui.font_size`(16) |
| ★ **SQL 편집기** | `Mono` | 한글 고정폭(D2Coding 우선) → OS 고정폭 + 한글 UI 폴백 | `editor.font_family` · `editor.font_size`(14 · Sublime 프로필) |
| ★ **결과 그리드** | `Mono`(옵션) / `Base` | 기본 = 고정폭(숫자 정렬) · 옵션으로 UI 본 | `grid.font_mono`(true) · `grid.font_family` · `grid.font_size` |
| 상태줄·메시지 로그 | `Status` | UI 본 | `status.font_size`(13) |
| 스크립트 출력(DBMS_OUTPUT · PRINT) | `Mono` | 편집기와 동일 | — |

구현: 한 프레임을 두 `RasterCtx`로 그린다 — UI 크롬은 `FontSet{base: ui}`, 편집기·그리드는 `FontSet{base: mono, …}`. `nexa-font::mono_font`가 한글 UI 본을 폴백으로 붙이므로 고정폭 영역에서도 한글이 깨지지 않는다. 폰트 크기는 영역별 `SlotFont`.

## 2. 기능 모듈 — Sublime 패키지식으로 하나씩

원칙: **기능 = 크레이트(코드) + `Packages/<이름>/`(데이터)**. 데이터(문법·키맵·설정·스니펫)는 Sublime 형식 그대로, 코드는 `Command`/`EventListener` 계약([09 §4](09-editor-and-packages.md)). 핵심 기능도 같은 계약으로 만들어 **내장 패키지**로 배포한다(ST의 `Default`처럼).

| 모듈 | 형태 | 입력 → 출력 | 순서 |
|---|---|---|---|
| **SQL 구문 강조** | `syntect` + `Packages/SQL/*.sublime-syntax`(Oracle PL/SQL · T-SQL · PG 변형) + `.sublime-color-scheme` | 버퍼 줄 → scope 범위 → 색 | M3 ① |
| **SQL 포매터** | 크레이트 `nsql-format`(토크나이저 + 규칙) · 명령 `format_sql` · 설정 `sql_format.*` | 텍스트 → 텍스트. 스타일 옵션: 키워드 대문자 · **선행 콤마(사용자 스크립트 스타일 `,\tA.COL`)** · 탭/공백 · `=` 정렬(Alignment 대응) · 절 줄바꿈 | M3 ② |
| **자동완성** | `CompletionProvider` — 키워드(`.sublime-completions`) + 스키마 인덱스(`Catalog`) | 접두·컨텍스트(FROM 뒤=테이블 · 별칭.=컬럼) → 후보 | M3 ③ |
| **스니펫** | `.sublime-snippet`(SQL 관용구: `sel`·`ins`·`exec`·`var`) | tab trigger → 필드 | M3 ③ |
| **정렬·연번·대소문자 변환** | 명령 패키지(`Alignment`·`Insert Nums` 대응) | 선택 → 변환 | M3 ④ |
| **인코딩 변환** | `ConvertToUTF8` 대응 — EUC-KR/CP949 감지·저장 | 파일 바이트 ↔ 버퍼 | M2(P0) |
| **실행계획 뷰어** | 방언 패키지(`Packages/Oracle`·`Packages/MSSQL`)의 `explain` 명령 + 트리 뷰 | 계획 텍스트/XML → 트리 | M5 |
| **데이터 편집기** | 그리드 변경분 → SQL 미리보기 → 커밋 | M4 |
| **결과 비교 · 스크립트 diff** | `Compare Side-By-Side` 대응 | P2 |
| **JSON/16진 셀 뷰어** | `Pretty JSON`·`HexViewer` 대응 | 셀 → 패널 | M4 |
| **테마** | `.sublime-color-scheme`(편집기) + `nexa-ctl::Theme`(UI) 한 쌍 | — | M3 |

각 모듈은 (a) 순수 로직 크레이트 + 테스트 → (b) 명령 등록 → (c) 키맵·메뉴·팔레트 항목(데이터) → (d) 설정 키 순서로 만든다. 모듈 간 결합은 **scope selector와 Command 이름**으로만.
