# 46 · 미니맵 기능 조사와 우선순위(Sublime Text 기준 · VS Code 대조)

> **상태**: 📐 조사·추천(09-17 · 사용자 "미니맵에 선택된 텍스트·자동 선택되는 동일값 미리보기(다중 선택과 선택 대상은 다르게) · Sublime 미니맵 활용 방식을 조사해 1·2·3순위로 추천") · 구현 = **T-110**.
> 현재 구현(T-97 ✅ 09-16): nexa-ctl `TextBox::set_minimap` — 줄당 2px 블록 · 토큰 색 · 뷰포트 상자(반투명+테두리 · hover 진하게) · 창 한정 캐시 비트맵 · 클릭/드래그 · `editor.minimap`(off) · `editor.minimap_width`(80). **09-17 추가**: 선택 = 강조색(accent · 0.6) · 같은 값의 다른 출현 = 출현 상자 선 색(기본 `warn` 호박 · 0.45)으로 **색을 달리 표시**.

## 0. 결론

1. Sublime의 미니맵은 "**축소 렌더 + 뷰포트 상자 + 클릭/드래그**"가 전부이고, 나머지는 **편집기 영역 표시(regions)가 미니맵에도 비치는 것**이다 — 선택 · 찾기 결과 · 플러그인 영역(add_regions). 설정은 3개뿐: `draw_minimap_border` · `always_show_minimap_viewport` · (ST4) `minimap_scroll_to_clicked_text`.
2. 우리는 그 골격을 이미 갖췄다. **1순위 = "미니맵이 비춰야 할 영역 표시 4종"**(선택 · 동일 출현 · 찾기 결과 · 오류 줄)과 **뷰포트 옵션 2개**. 2순위 = 토글/자동 숨김/문장 범위/git 줄. 3순위 = 취향 옵션(자동 숨김 페이드 · 배율 · 왼쪽 배치 · 섹션 헤더).
3. 표시 원칙: 미니맵은 **줄 단위 색 띠**만 그린다(글자 렌더 ✗ · 비용과 가독성 모두 불리 · VS Code `renderCharacters`도 대부분 끄고 쓴다). 새 표시는 전부 "행 범위 + 색 + 알파" 하나의 부품(`dot`)으로 그린다 — 종류마다 그리기 코드를 늘리지 않는다.

## 1. Sublime Text 미니맵 — 실제 동작 정리

| 항목 | Sublime Text(3/4) | 비고 |
|---|---|---|
| 렌더 | 파일 전체를 축소한 색 블록(글자 아님) · 고정 배율 · 오른쪽 | 폭 고정(약 90px) · 스케일 옵션 없음 |
| 뷰포트 | 반투명 상자 · **hover 때만** 표시(기본) · `always_show_minimap_viewport: true`면 항상 · `draw_minimap_border: true`면 테두리 | 우리 = 항상 + 테두리 + hover 진하게(둘 다 켠 상태) |
| 이동 | 클릭 = 뷰포트를 그 위치로(중앙) · 드래그 = 따라감 · (ST4) `minimap_scroll_to_clicked_text: true` = 클릭한 글로 스크롤 | 우리 = 클릭/드래그 ✅ · "클릭한 글로" 옵션 ✗ |
| 비치는 표시 | **선택 영역** · **찾기 강조(Highlight matches · Find All)** · `add_regions`로 플러그인이 넣은 영역(린터 오류 · 북마크 · 괄호 등 — 채움/외곽선 무관하게 미니맵엔 색 띠) | 우리 = 선택·동일 출현 ✅ · 찾기 ✗ · 오류 ✗ |
| 토글 | View ▸ Hide/Show Minimap(`toggle_minimap`) · 창별 기억 | 우리 = 설정 `editor.minimap`만 · 메뉴/단축키 ✗ |
| 숨김 | 좁은 창에서도 숨기지 않음(사용자가 끔) | VS Code는 `minimap.autohide`(hover 때만) |
| 없음 | 배율 · 문자 렌더 · 왼쪽 배치 · 섹션 헤더 · git 띠 | VS Code에 있음(아래) |

VS Code 대조(사용자 평이 좋은 것): `showSlider`(always/mouseover) · `autohide` · `scale`(1~3) · `side`(left/right) · `maxColumn`(120) · `renderCharacters`(대개 끔) · **찾기 결과·선택·오류(빨강)·경고·git 변경(초록/파랑/빨강 띠)·접힌 영역·섹션 헤더(`//#region`·`MARK:`)** 표시.

## 2. 추천 — 우선순위

### 1순위 · 꼭 구현(미니맵이 "지도"로 쓸모 있으려면)

| # | 기능 | 설명 · 우리 구현 방식 | 설정 키 |
|---|---|---|---|
| 1-1 | **선택 vs 동일 출현 구분** ✅ 09-17 | 선택 = accent 0.6 · 다른 출현 = 출현 상자 선 색(기본 warn) 0.45 — 편집기 본문의 채움/외곽선과 같은 색 논리 | (출현 선 색 `editor.occurrence_line_color`를 따름) |
| 1-2 | **찾기 결과 띠** | 찾기 바가 열려 있고 매치가 있으면 매치 행에 `find` 색(테마 `warn` 계열 · 선택과 구분) — 찾기 바가 이미 매치 목록을 가지므로 `TextBox::set_minimap_marks(kind, rows)`로 넘긴다 | `editor.minimap_find`(on) |
| 1-3 | **오류 줄 마크** | 실행 오류(`RunEvent::Error.line`)·구문 오류 줄에 `danger` 띠 + 오른쪽 가장자리 2px 점(줄이 화면 밖이어도 보이게) · 편집하면 지움 | `editor.minimap_errors`(on) |
| 1-4 | **뷰포트 표시 방식** | `always`(현재) / `hover`(Sublime 기본) · 테두리 on/off · ✅ 09-17 색/테두리: 회색 `#808080` 18% 기본 · 테두리 없음 · `editor.minimap_box_color`/`editor.minimap_border` | `editor.minimap_viewport`(always/hover · 남음) |
| 1-5 | **클릭 동작** | `center`(현재 · 뷰포트 중앙) / `text`(클릭한 글로 스크롤 · ST4) | `editor.minimap_click`(center/text) |

### 2순위 · 자주 쓰는 것

| # | 기능 | 설명 | 설정/명령 |
|---|---|---|---|
| 2-1 | **토글 메뉴·단축키** | View ▸ 미니맵(체크) + 키맵 `view.minimap` · 값은 설정에 저장(창별 아님 — 설정 하나로 단순하게) | `view.minimap` |
| 2-2 | **현재 문장 범위** (SQL 특화) | 캐럿이 든 문장(`;` 블록 = 실행 대상)의 행 범위를 아주 옅은 accent로 — "지금 실행하면 어디까지인가"가 지도에 보임 | `editor.minimap_statement`(on) |
| 2-3 | **git 변경 줄 띠** | 저장된 파일과 HEAD 차이(추가 초록 · 수정 파랑 · 삭제 빨강 점) — 상태줄 git 부품이 이미 있으므로 diff 행 계산만 추가 · 파일 탭에서만 | `editor.minimap_git`(on) |
| 2-4 | **저장 뒤 수정 줄** | 마지막 저장 이후 바뀐 행(옅은 노랑) — 편집기 undo 스택으로 행 추적 | `editor.minimap_dirty`(off) |
| 2-5 | **좁은 창 자동 숨김** | 편집기 폭 < N열이면 숨김(설정 값은 유지) | `editor.minimap_min_cols`(60) |

### 3순위 · 자주 쓰진 않지만 평이 좋은 것

| # | 기능 | 설명 | 설정 |
|---|---|---|---|
| 3-1 | **자동 숨김 페이드**(VS Code autohide) | 편집기 hover/스크롤 때만 서서히 나타남 — `IntentFade` 부품 재사용 | `editor.minimap_autohide`(off) |
| 3-2 | **배율** | 줄당 1/2/3px(긴 파일은 1 · 짧은 파일은 3) · `auto` = 파일 길이로 | `editor.minimap_scale`(2/auto) |
| 3-3 | **왼쪽 배치** | 거터 옆 | `editor.minimap_side`(right) |
| 3-4 | **섹션 헤더** | 주석 구분선(`-- ====` · `-- #region` · `/* == */`)의 첫 단어를 미니맵에 작은 글자로 — 긴 스크립트 탐색에 좋다(VS Code 1.85+) | `editor.minimap_sections`(off) |
| 3-5 | **접힌 영역 표시** | 접기(T-계획) 뒤 접힌 블록을 한 줄 띠로 | — |
| 3-6 | **뷰포트 부드러운 이동** | 클릭 이동 때 `ui.animations`에 맞춰 짧은 슬라이드 | (애니메이션 마스터 따름) |

## 3. 구현 메모(T-110)

- nexa-ctl `TextBox`: 미니맵 표시 원천을 **한 벌의 마크 목록**으로 — `MinimapMark { rows: Range<usize>, kind: Selection | Occurrence | Find | Error | Statement | GitAdd | GitMod | GitDel | Dirty }` · 색·알파는 종류 → 테마/설정 표에서 · 그리기는 `dot` 하나. 선택·출현은 편집기가 스스로(지금처럼), 나머지는 호스트가 `set_minimap_marks(kind, ranges)`로 넣고 종류별로 갈아 끼운다(캐시 비트맵은 텍스트 변경에만 무효 · 마크는 위에 덧그려 싸다).
- 설정은 전부 `nsql-settings` REGISTRY(라벨 = Msg) · 자주 안 바꾸는 것은 `HIDDEN`. 부하: 마크 그리기는 보이는 띠 폭 × 행 수 O(n)이나 미니맵 자체가 창 높이 한정이라 상한 있음 — [39 §3](39-resource-governance.md)에 `editor.minimap` 한 줄로 이미 등재.
- 순서: 1-2 → 1-3 → 1-4/1-5 → 2-1 → 2-2 → 2-3 → 나머지는 요청 시.
