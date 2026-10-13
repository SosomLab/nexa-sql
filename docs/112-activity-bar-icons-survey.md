# 112 · 활동 막대(Activity Bar) 아이콘 재조사 — 패널별 1·2·3안

> **요구 원문(사용자 10-13)**: "각 패널에 맞는 아이콘(이미지)을 다시 조사해 1·2·3안 추천 · SVG만이 아니라 색상 있는 이미지도 가능 · 색 이미지는 다크/라이트 반영 방식 · hover 이미지 · 선택 이미지를 각각 제시 · 1안 = SVG(깔끔·선명·직관) · 2안 = 이미지/SVG 중 추천 · 3안 = 이미지 · 충분히 조사해 예쁘고 이 프로그램 디자인에 맞게"
> **상태** = 조사 완료 → ✅ **D-285 사용자 결정·반영 10-13**(아래 §6 끝) · T-341 ✅
> **짝 문서** = [88 UI/UX 표준](88-ui-ux-standards-survey.md)(§4 머티리얼 · 단순·외곽선 사각형 없음) · [30 아키텍처 패턴](30-architecture-patterns.md)(부품 · 설정 선택) · [64](64-dbms-clients-and-driver-packaging.md) + `crates/nexa-sql/assets/dbms/NOTICE.md`(DBMS 아이콘 틀 = `assets/dbms/<light|dark>/<64|20>/*.png` → `build.rs` 표 → 지연 디코드)
> **조사 자료** = `target/icon-survey/`(받은 원본 · 라이선스 원문 · 대조 시트 · §5)

## 0. 결론

- **1안(단색 SVG)** = **Material Symbols Outlined**(Apache-2.0) 한 벌. 머티리얼 원칙(docs/88 §4)과 가장 잘 맞고 24 px에서 획이 굵고 또렷하다. 대안 = **Fluent System Icons Regular**(MIT · 획이 가늘고 Windows 11 느낌 · `nexa-gfx::svg`가 **지금 그대로** 읽는다).
- **2안(권장 색 아이콘)** = **Fluent System Icons Color**(MIT · 색 SVG) 중심 + 빈 자리 2개는 같은 Fluent 계열 **Fluent Emoji Flat**(MIT). 평면·은은한 그라데이션이라 머티리얼 화면에 튀지 않고 라이트/다크 둘 다 한 벌로 읽힌다.
- **3안(색 이미지)** = **Fluent Emoji 3D**(MIT · PNG 256) — 가장 화려·친근. 대안 = **Flat Color Icons**(icons8 · `MIT OR Good-Boy` → MIT 선택).
- **검색 · 확장 기능** = 사용자가 현행에 만족 → **현행 유지 권장 가능**(다른 패널을 색 안으로 바꿀 때만 같은 벌로 맞춘다).
- **상태 규칙** = 색 이미지도 **상태별 파일 불필요** — 테마별 1벌(라이트/다크 · 64·24 px)에서 비선택·hover·선택을 래스터 시점에 **파생**한다(§3). 이번 후보는 라이트/다크 공용 1벌로도 충분했다.
- **권장 결정(D-285)** = **1안을 기본으로 하고 2안을 설정 선택지로 추가**(`ui.activity_icons = mono | color` · 기본 mono). 활동 막대는 VS Code·JetBrains·Material 모두 단색이 표준이고, 색 아이콘을 원하는 사용자에게만 2안을 연다.

### 0-1. 패널별 추천 요약

| 패널 | 1안 · 단색 SVG(Material Symbols) | 2안 · 권장 색(Fluent Color) | 3안 · 색 이미지(Fluent Emoji 3D) |
|---|---|---|---|
| 객체 탐색기 | `database` | `ic_fluent_database_24_color` | `File cabinet`(서랍장 · DB 그림 없음 → 대용) |
| 검색 *(현행 유지 가능)* | `search` | Fluent Emoji Flat `Magnifying glass tilted left`(Fluent Color에 평범한 search 없음) | `Magnifying glass tilted left` |
| 프로젝트 탐색기 | `folder_open` | Fluent Emoji Flat `Open file folder`(대안 `ic_fluent_document_folder_24_color`) | `Open file folder` |
| 북마크 | `bookmarks`(리본 두 장) | `ic_fluent_bookmark_24_color` | `Bookmark`(🔖 꼬리표 모양 · 리본 아님 — 약점) |
| 아웃라인 | `toc` | `ic_fluent_text_bullet_list_square_24_color` | `Clipboard`(대안 `Card index dividers`) |
| 확장 기능 *(현행 유지 가능)* | `extension`(퍼즐) | `ic_fluent_puzzle_piece_24_color` | `Puzzle piece` |

## 1. 원천 · 라이선스

라이선스 원문은 공식 저장소에서 받아 `target/icon-survey/_licenses/`에 두었다(10-13 확인).

| 원천 | 라이선스(원문 확인) | THIRD-PARTY-NOTICES 고지 문구 | 판정 |
|---|---|---|---|
| Material Symbols(google/material-design-icons) | Apache-2.0 | `Material Symbols — Copyright Google LLC — Apache License 2.0` + 라이선스 원문 동봉 | ✅ 채택(1안) |
| Fluent UI System Icons(microsoft/fluentui-system-icons) · Regular + Color | MIT | `Fluent UI System Icons — Copyright (c) 2020 Microsoft Corporation — MIT License` + 허가 문구 | ✅ 채택(1안 대안 · 2안) |
| Fluent Emoji(microsoft/fluentui-emoji) · Flat/3D | MIT | `Fluent Emoji — Copyright (c) Microsoft Corporation — MIT License` + 허가 문구 | ✅ 채택(2안 일부 · 3안) |
| Flat Color Icons(icons8/flat-color-icons) | `MIT OR Good-Boy`(package.json · README) — LICENSE.md에는 Good Boy 문구만 | `Flat Color Icons — Copyright (c) Icons8 — MIT License(이중 라이선스 중 MIT 선택)` + MIT 원문을 우리가 붙여 동봉 | ⚠ 조건부(MIT 선택 명시 · 3안 대안) |
| VS Code Codicons(microsoft/vscode-codicons) | CC-BY-4.0(아이콘) | `Codicons by Microsoft — CC BY 4.0 — https://creativecommons.org/licenses/by/4.0/`(변경 시 "modified" 표시) | ✅ 가능(후보 아님 · VS Code와 똑같아 보임) |
| Lucide | ISC(Feather 부분 MIT) | `Lucide — Copyright (c) Lucide Contributors 2022 / Cole Bemis 2013-2022 — ISC` | ✅ 가능(후보 아님) |
| Tabler Icons | MIT | `Copyright (c) 2020-2024 Paweł Kuna — MIT` | ✅ 가능(후보 아님) |
| Phosphor Icons | MIT | `Copyright (c) 2023 Phosphor Icons — MIT` | ✅ 가능(후보 아님) |
| Iconoir | MIT | `Copyright (c) 2021 Luca Burgio — MIT` | ✅ 가능(후보 아님) |
| Remix Icon | **Remix Icon License v1.0(2026-01 · 자체 라이선스)** — 앱 안 사용은 허용하나 아이콘 단독 판매·경쟁 아이콘 세트·로고 사용 금지 | — | ❌ 제외(허용 목록 밖 자체 라이선스) |
| Icons8(일반 사이트 아이콘) | 무료판 = 링크 표기 조건 · 그 외 유료 | — | ❌ 제외 |
| JetBrains IntelliJ 아이콘 | 저장소는 Apache-2.0이나 경로 확인 실패(expui 경로 404) · DB 아이콘은 DataGrip(비공개) 쪽 | — | ❌ 제외(미검증) |
| DBeaver 아이콘 | 저장소는 Apache-2.0이나 아이콘에 외부 세트(FatCow CC-BY 3.0 · Eclipse EPL) 혼재 | — | ❌ 제외(출처 혼재) |

## 2. 패널 × 안

공통 상태 규칙: **단색** = 비선택 `text_dim` 틴트 · hover/선택 = `text` 틴트 · 선택 = 왼쪽 4 px 강조 막대(현행 그대로 · 09-18에 2→4 px). **색** = §3 파생 규칙 + 같은 막대.

### 2-1. 객체 탐색기
| 안 | 원천 · 이름 | 24 px 가독성 | 다크/라이트 | 현행 대비 |
|---|---|---|---|---|
| 1 | Material `database` · `symbols/web/database/materialsymbolsoutlined/database_24px.svg` | ◎ 원통 3단이 또렷 | 틴트라 무관 | 현행(`mi_files`)보다 "DB" 의미가 즉시 읽힘 |
| 1대안 | Fluent `ic_fluent_database_24_regular` | ○ 획이 가늘어 100 %에서 약간 흐림 | 무관 | 〃 |
| 2 | Fluent Color `ic_fluent_database_24_color` | ◎ 파란 그라데이션 원통 | 공용 1벌 OK | 색으로 첫눈에 구분 |
| 3 | Fluent Emoji 3D `File cabinet` | △ 보라 서랍장 · 좁고 어두움 · "DB" 아님 | 다크에서 대비 낮음 | 의미 약함 |
| 3대안 | Flat Color `database` | △ 연보라 · **라이트에서 거의 안 보임** | 다크 ○ | — |

### 2-2. 검색 *(현행 유지 권장 가능)*
| 안 | 원천 · 이름 | 평가 |
|---|---|---|
| 1 | Material `search` | ◎ 표준 돋보기 |
| 2 | Fluent Emoji Flat `Magnifying glass tilted left` | ○ 파란 렌즈 · Fluent Color에는 `search_sparkle`(AI 뜻)만 있어 제외 |
| 3 | Fluent Emoji 3D 같은 이름 | ○ |
| 3대안 | Flat Color `search` | ○ |

### 2-3. 프로젝트 탐색기
| 안 | 원천 · 이름 | 평가 |
|---|---|---|
| 1 | Material `folder_open` | ◎ 열린 폴더 = "작업 폴더" 의미 정확 |
| 1대안 | Fluent `ic_fluent_folder_open_24_regular` | ○ |
| 2 | Fluent Emoji Flat `Open file folder` | ◎ 노란 폴더 · 평면 · 깔끔(Fluent Color에는 평범한 folder 색 아이콘이 없음) |
| 2대안 | Fluent Color `ic_fluent_document_folder_24_color` | △ 문서+폴더 겹침 · 24 px에서 복잡 |
| 3 | Fluent Emoji 3D `Open file folder` | ○ |
| 3대안 | Flat Color `opened_folder` | ◎ |

### 2-4. 북마크
| 안 | 원천 · 이름 | 평가 |
|---|---|---|
| 1 | Material `bookmarks`(리본 두 장) | ◎ "여러 북마크 목록" 뜻 · 현행 리본(docs/69 §6-3)과 같은 은유 |
| 1대안 | Fluent `ic_fluent_bookmark_multiple_24_regular` | ○ |
| 2 | Fluent Color `ic_fluent_bookmark_24_color` | ◎ 분홍 리본 · 단순 |
| 3 | Fluent Emoji 3D `Bookmark`(🔖) | △ 꼬리표(태그) 모양 · 리본 은유와 다름 · 대안 `Bookmark tabs`(책+꼬리표 · 24 px에서 복잡) |
| 3대안 | Flat Color `bookmark` | ◎ 빨간 리본 |

### 2-5. 아웃라인
| 안 | 원천 · 이름 | 평가 |
|---|---|---|
| 1 | Material `toc`(목차) | ◎ 줄+점 = 심볼 목록 · 현행(들여쓴 줄 넷)과 비슷 |
| 1대안 | Fluent `ic_fluent_text_bullet_list_tree_24_regular` | ◎ 계층(트리) 뜻이 더 정확 |
| 2 | Fluent Color `ic_fluent_text_bullet_list_square_24_color` | ◎ 파란 사각 + 흰 목록 |
| 3 | Fluent Emoji 3D `Clipboard` | ○ 목록 클립보드 · 대안 `Card index dividers`(24 px에서 상자처럼 보여 약함) |
| 3대안 | Flat Color `list` | ○ |

### 2-6. 확장 기능 *(현행 유지 권장 가능)*
| 안 | 원천 · 이름 | 평가 |
|---|---|---|
| 1 | Material `extension`(퍼즐) | ◎ VS Code 등과 같은 관례 |
| 2 | Fluent Color `ic_fluent_puzzle_piece_24_color` | ◎ |
| 3 | Fluent Emoji 3D `Puzzle piece` | ○ 무지개 그라데이션 · 다소 화려 |
| 3대안 | Flat Color `puzzle` | ◎ 연두 |

## 3. 색 이미지 상태 규칙 — 파생 vs 상태별 파일

**결론 = 파생으로 충분**(상태별 파일 3벌 불필요). 시트에서 두 규칙을 견줬다.

| 규칙 | 비선택 | hover | 선택 | 결과 |
|---|---|---|---|---|
| 요구 초안(규칙 1) | 채도 −40 % · 알파 70 % | 밝기 +10 % | 원색 + 막대 | 라이트 = 약간 바램 · **다크 = 탁함**(알파를 낮추면 어두운 바탕이 비쳐 노랑→갈색 · 분홍→적갈색) |
| **권장(규칙 2)** | 채도 −30 % · 알파 88 % | 원색 · 다크만 밝기 +10 % · 라이트는 현행 hover 바탕(옅은 회색) | 원색 + 4 px 막대 | 두 테마 모두 맑음 · 선택은 막대가 맡음 |

- 라이트에서 밝기 +10 %는 흰 바탕과 대비를 낮춰 hover가 오히려 흐려 보인다 → 라이트 hover = 바탕 칠(현행 단색 막대와 같은 신호).
- **테마별 파일이 필요한 경우**만 다크 폴더에 넣는다: 어두운 윤곽(검정 테두리 · 짙은 보라)이 다크 바탕에 묻히는 그림. 이번 후보 중에는 `File cabinet` 3D(짙은 보라)와 Flat Color `database`(연보라 → 라이트에서 묻힘)가 해당 — 둘 다 추천에서 뺐다.
- 구현 = 아이콘 6 × 상태 3 = 18장을 **처음 그릴 때 한 번 만들어 보관**(테마·배율 바뀌면 다시) — 매 프레임 필터 연산 없음(docs/39 부하원 기준).

## 4. 적용 방법 개요

| 항목 | 단색(1안) | 색(2·3안) |
|---|---|---|
| 그리는 길 | 지금과 같은 **알파 마스크 + 테마색 틴트**(`IconImage::from_alpha_tinted`) | DBMS 아이콘 틀 재사용: `assets/activity/<light|dark>/<64|24>/<id>.png` → `build.rs` 표 → 지연 디코드 → `dc.image_scaled` |
| SVG 처리 | ⚠ `nexa-gfx::svg`는 경로 명령 **M/L/H/V/C/A/Z만** 지원 — Material은 `Q`/`T`(2차 곡선)를 써서 **그대로는 못 읽음** → ① nexa-ui `svg.rs`에 Q/T/S 추가(2차→3차 변환 · 수십 줄) 또는 ② 빌드 전 마스크 PNG로 굽기. Material의 음수 원점 viewBox(`0 -960 960 960`)도 확인 필요. **Fluent Regular(1대안)는 C/L/H/V/M/Z만 써서 바로 읽힌다** | Fluent Color는 그라데이션·opacity를 써서 `svg.rs`(그라데이션 없음 · 불투명 결과)로는 못 그림 → **빌드 전에 PNG로 굽는다**(헤드리스 Edge — `scripts/pack_icon.py`와 같은 방식). 3D는 원본이 PNG 256 → 64·24 축소 |
| 크기 | 6장 × 0.3~0.7 KB(SVG) | 3D 실측: 64 px 6장 22.7 KB + 24 px 6장 6.7 KB ≈ **29 KB/테마** · 두 테마 ≈ 58 KB(공용 1벌이면 29 KB) · Fluent Color는 더 작을 것(추정) |
| 크기 단계 | — | DBMS 틀은 64·20이지만 활동 막대는 24 논리 px → **24 + 64**(64는 HiDPI 150~250 % 축소용) 권장 |
| 설정 | — | `ui.activity_icons = mono|color`(기본 mono · 향상 모드·저사양 프로필 = mono 강제 검토) |
| 고지 | THIRD-PARTY-NOTICES에 §1 문구 | 같음 · `assets/activity/NOTICE.md`(DBMS 틀처럼) |

## 5. 대조 시트

- `target/icon-survey/sheet-light.html` · `sheet-light.png` — 라이트(바탕 #F3F4F7 · 강조 #3B82F6) · 규칙 1
- `target/icon-survey/sheet-dark.html` · `sheet-dark.png` — 다크(바탕 #1E1F22) · 규칙 1
- `target/icon-survey/sheet-light-rule2.png` · `sheet-dark-rule2.png` — 권장 규칙 2(비선택 채도 .7 · 알파 .88)
- 구성 = 위: 안마다 48 px 막대 모형(객체 탐색기 = 선택 · 검색 = hover · 나머지 = 비선택) / 아래: 패널 × 안 표(비선택·hover·선택 24 px + 2× 48 px). 현행 열 = "코드 마스크"(그리지 않음).
- 원본 = `target/icon-survey/<explorer|search|project|bookmarks|outline|extensions>/{1.svg, 1-alt-fluent.svg, 2.svg, 3.png, 3b.svg}` + 탈락 후보 `*-alt-*` · 라이선스 원문 `_licenses/` · 축소 PNG 실측 `_png_est/`.

## 6. 결정 후보 D-285

| 선택지 | 내용 | 비고 |
|---|---|---|
| A | 1안(Material Symbols 단색)으로 교체 | nexa-gfx Q/T 지원 또는 마스크 굽기 필요 |
| A' | 1대안(Fluent Regular 단색)으로 교체 | 코드 변경 최소(SVG 바로 파싱) · 획이 가늘다 |
| B | 2안(Fluent Color)으로 교체 | PNG 굽기 + 상태 파생 |
| C | 3안(Fluent Emoji 3D)으로 교체 | 화려 · 머티리얼과 거리 |
| **D(권장)** | **A 기본 + B를 설정 선택지**(`ui.activity_icons`) | 표준(단색) 유지 + 색 원하는 사용자 선택 · 검색·확장은 각 벌의 같은 계열로 맞추되 현행 유지도 가능 |

- **사용자 결정(10-13)** = 패널별 혼합 — 객체 탐색기 `database` · 프로젝트 `folder_open` · 북마크 `bookmarks` = **1안 Material Symbols** · 검색 = **1안-대안 Fluent Regular `search_24_regular`**(viewBox 24 · evenodd) · 아웃라인 = **자체 5선**(1·5줄 = 맨 위·아래 · 2~4줄 균등 · 가운데 들여쓰기) · 확장 = **현행 유지** · 색 아이콘(2안) 설정은 만들지 않음 · 구현 = `toolicons` `mi_act_explorer`/`mi_act_search`/`mi_folder_open`/`mi_act_bookmarks`/`mi_act_outline` + `activity.rs` 5자리 교체 · 자체 ASCII 미리보기 시험 `act_icons_ascii` · 고지 = `crates/nexa-sql/assets/icons/NOTICE.md`(Material Apache-2.0 · Fluent MIT).

## 7. 사용자에게 보일 요약(10줄)

1. 6개 패널 아이콘을 다시 조사해 안마다 한 벌(같은 계열)로 맞췄습니다.
2. 1안 = Google Material Symbols 단색 SVG(Apache-2.0) — DB 원통 · 돋보기 · 열린 폴더 · 리본 두 장 · 목차 · 퍼즐.
3. 1안 대안 = Microsoft Fluent Regular 단색(MIT) — 더 가늘고 Windows 11 느낌, 지금 SVG 엔진이 바로 읽습니다.
4. 2안(권장 색) = Microsoft Fluent Color(MIT) — 파란 DB · 노란 폴더 · 분홍 리본 · 파란 목록 · 퍼즐.
5. 3안 = Fluent Emoji 3D PNG(MIT) — 가장 화려하지만 DB(서랍장)·북마크(꼬리표)는 뜻이 약합니다.
6. 검색·확장 기능은 현행 유지로 두어도 됩니다.
7. 색 아이콘도 상태별 파일은 필요 없습니다 — 비선택 = 채도 −30 %·알파 88 %, hover = 다크만 밝기 +10 %(라이트는 바탕 칠), 선택 = 원색 + 파란 막대.
8. 처음 규칙(알파 70 %)은 다크에서 탁해져 고쳤습니다(시트로 비교).
9. 용량은 색 PNG 6장 × 2크기 ≈ 29 KB/테마로 작습니다.
10. 권장 = 1안을 기본으로, 2안을 설정(`ui.activity_icons`)으로 고를 수 있게(D-285) — 대조 시트 `target/icon-survey/sheet-*.png`.

## 8. 출처 URL

- Material Symbols: https://github.com/google/material-design-icons · 파일 `https://raw.githubusercontent.com/google/material-design-icons/master/symbols/web/<name>/materialsymbolsoutlined/<name>_24px.svg` · LICENSE https://raw.githubusercontent.com/google/material-design-icons/master/LICENSE
- Fluent UI System Icons: https://github.com/microsoft/fluentui-system-icons · 파일 `.../main/assets/<Name>/SVG/ic_fluent_<name>_24_<regular|color>.svg` · LICENSE `.../main/LICENSE`
- Fluent Emoji: https://github.com/microsoft/fluentui-emoji · 파일 `.../main/assets/<Name>/3D/<name>_3d.png` · `.../Flat/<name>_flat.svg` · LICENSE `.../main/LICENSE`
- Flat Color Icons: https://github.com/icons8/flat-color-icons · 파일 `.../master/svg/<name>.svg` · LICENSE.md · package.json(`"license": "MIT OR Good-Boy"`)
- Codicons: https://github.com/microsoft/vscode-codicons (LICENSE = CC-BY-4.0)
- Lucide: https://unpkg.com/lucide-static@0.460.0/LICENSE · Tabler: https://unpkg.com/@tabler/icons@3.19.0/LICENSE
- Phosphor: https://github.com/phosphor-icons/core (LICENSE MIT) · Iconoir: https://github.com/iconoir-icons/iconoir (LICENSE MIT)
- Remix Icon: https://github.com/Remix-Design/RemixIcon/blob/master/License (Remix Icon License v1.0 · 2026-01)
