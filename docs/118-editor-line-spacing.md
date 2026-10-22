# 118 · 편집기 줄 간격 — 타 도구 조사와 설정 체계 (134차 win · 2026-10-21)

> 요구(사용자 10-21 · 134차 win): ① 글꼴을 줄여도 줄 간격이 그대로(바닥 20 px) → 글꼴을 따르게 ② 16 px 글꼴에서 지금(24 px)의 **80 %**로 ③ 16 px를 기준으로 글꼴 크기별 줄 간격을 **비율**로 ④ 설정 방식 = Word·PowerPoint와 Sublime·VS Code·IntelliJ의 줄 간격 지정 방식을 조사해 구성.
> 결정 = **D-315**(아래 §4) · 구현 = nexa-ui 220차 `LineSpacing`/`line_h_for` + nexa-sql `editor.line_height_mode`/`editor.line_height`.

## 1. 종전 동작과 문제

| 항목 | 종전(≤ 219차) | 문제 |
|---|---|---|
| 줄 높이 식 | `max(20×배율, 글꼴 상자 + 4)` | 글꼴 상자가 16 px(≈ 12 pt) 아래면 20으로 고정 → 작은 글꼴에서 줄 간격이 그대로 |
| 여백 | 고정 4 px | 8 px 글꼴에선 여백이 글자의 절반(너무 넓음) · 24 px에선 1/6(너무 좁음) — 비율이 글꼴에 따라 달라짐 |
| 설정 | 없음(글꼴 크기만) | 줄 간격을 바꿀 길이 없음 |

글꼴 상자(`text_height`) = 어센트 + 디센트(내부 여백 포함) ≈ 글꼴 px × 1.2(Windows GDI 실측 · 16 px → 약 20).

## 2. 타 도구 조사

| 도구 | 설정 | 단위·기본 | 비고 |
|---|---|---|---|
| **Word** | 단락 ▸ 줄 간격 = 1줄 · 1.5줄 · 2줄 · **배수** · **최소** · **고정** (+ 단락 앞/뒤 간격) | 배수 = 글꼴 자연 줄 높이의 배(기본 1.15 · 1줄 = 1.0) · 최소/고정 = pt | "자연 줄 높이" = 글꼴 상자(어센트+디센트+내부 여백) |
| **PowerPoint** | 단락 ▸ 줄 간격 = **배수** · **정확히** (+ 앞/뒤) | 배수 기본 1.0 · 정확히 = pt | Word의 최소가 없음 |
| **Sublime Text** | `line_padding_top` · `line_padding_bottom` | px 고정(기본 0 · 0) | 여백을 **px로** 더함 → 글꼴을 바꾸면 비율이 변함(= 이번 결함과 같은 구조) |
| **VS Code** | `editor.lineHeight` | 0 = 자동(글꼴 × 1.35 Win/Linux · 1.5 macOS) · **8 미만 = 글꼴 크기의 배수** · 8 이상 = px | 한 키에 배수/px를 값 크기로 가름 |
| **IntelliJ/JetBrains** | Editor ▸ Font ▸ **Line height** | 배수(기본 **1.2**) | 줄 높이 = 글꼴 크기 × 값 |
| Zed | `buffer_line_height` | `comfortable`(1.618) · `standard`(1.3) · 숫자 | 배수 |
| Notepad++ · Golden · DBeaver | 없음 | 글꼴 자연 높이 | — |

정리 = 코드 편집기는 **글꼴 크기의 배수** 한 가지(IntelliJ 1.2 · VS Code 1.35 · Zed 1.3)가 주류이고, 문서 도구는 **배수 · 고정 · 최소** 셋을 준다. Sublime의 px 여백 방식은 글꼴에 비례하지 않아 채택하지 않는다.

## 3. 요구 ②③의 수치

- 16 px 글꼴 · 종전 24 px → 80 % = **19.2 ≈ 19 px**.
- 19 ÷ 16 = **1.19 ≈ 1.2** = IntelliJ 기본값과 같다 → 기본 배수 **1.2**(글꼴 px 기준).
- 비율 처리 = 줄 높이 = 글꼴 px × 1.2 → 8 px = 10 · 12 px = 14 · 16 px = 19 · 20 px = 24 · 24 px = 29 · 32 px = 38.
- 바닥 = **8 px**(VS Code 최솟값과 같음 · 안전망). 처음 12로 두었더니 8 px 글꼴에서 12가 걸려 1.5배가 됐고(사용자 10-21 2차 캡처 "8 px 줄 간격이
  글자 높이만큼") → 8로 내려 보통 글꼴 범위(8~40 px)에선 비율이 그대로다.

## 4. 결정 D-315 — 설정 체계(✅ 확정 10-22 · 사용자 · 권장안 그대로 · 구현 = nexa-ui 220차 + nexa-sql 134차)

| 키 | 종류 | 기본 | 뜻 |
|---|---|---|---|
| `editor.line_height_mode` | Choice `multiple` · `exact` · `at_least` | `multiple` | Word/PPT의 셋 = **배수**(IntelliJ·VS Code 방식) · **고정**(px) · **최소**(px이되 글꼴 상자가 더 크면 상자) |
| `editor.line_height` | Text | `1.2` | 방식에 따라 해석 — 배수 0.8~3.0 · 고정/최소 px 6~80(`19` · `19px`) · 잘못된 값·범위 밖 = 기본 배수 1.2 |

- 적용 범위 = **편집기 탭 전부**(새 탭 포함 · `Editors::set_line_spacing`) · 글꼴 크기(`editor.font_size` · Ctrl+휠)가 바뀌면 배수를 다시 계산해 같이 바뀐다.
- 조건 바·설정 창 등 다른 멀티라인 상자는 `LineSpacing::Auto`(종전 식 · 바닥 8 = `MIN_LINE_H` 공용) 그대로 — 영향 없음.
- 바닥 = 8 px × 배율(어느 방식이든 그 아래로는 안 내려간다 · 안전망 · §3).
- 줄 번호·캐럿·선택·히트 테스트·북마크 니모닉 상자·미니맵 창 비율은 전부 같은 `lh`를 쓰므로 따로 보정할 것이 없다(220차 1차 확인 · 협업 V1 캡처).
- 그리드 행 높이는 이 결정의 범위 밖(그리드는 `grid.font_size`에 따른 자체 행 높이).
- 채택하지 않은 것 = Sublime식 위/아래 px 여백(비율 깨짐) · VS Code식 "한 키에 배수/px를 값 크기로 가름"(뜻이 숨어 있음 → 방식 키를 따로 둠).

## 5. 구현 자리

| 층 | 자리 |
|---|---|
| nexa-ui 220차 | `nexa-ctl` `controls/textbox.rs` — `LineSpacing`(Auto·Multiple·Exact·AtLeast) · `line_h_for(spacing, text_h, font_px, scale)` 순수 함수 + 시험 `line_height_modes` · `TextBox::set_line_spacing(spacing, font_px)` · `MIN_LINE_H` 8 · `line_h()` 바닥 |
| nexa-sql | `nsql-settings` 2키 · `nsql-i18n` 라벨 7 · `editors.rs` `line_spacing` 전파(전 탭 + 새 탭) · `app/settings.rs` `apply_line_height` + 순수 `line_spacing_from(mode, value)`(시험 `line_height_tests`) · `main.rs` 기동 적용 · `editor.font_size` 변경 때 재적용 |

## 6. 점검(V1 · 격리 홈)

1. 기본값 = 16 px 글꼴에서 줄 피치 19 px(종전 24) — 캡처.
2. `editor.font_size` 8 → 10 · 24 → 29(비율) — 캡처 2장.
3. `editor.line_height_mode=exact` · `editor.line_height=30` → 30 · `at_least` 14 → 글꼴 상자(≈ 20).
4. Ctrl+휠로 글꼴을 바꾸면 줄 간격이 즉시 따라감.
5. 조건 바 높이는 종전 그대로.
