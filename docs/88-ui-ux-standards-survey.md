# 88 · UI/UX·디자인 국제 표준 조사 — 데스크톱 데이터 도구가 지킬 규칙(원천 · 라이선스 · 점검표 · T-229)

> **요청**(사용자 09-26): *"개발자가 참고할 UI 개발 표준·UX 개발 표준·디자인 표준 — 국제 가이드가 있는지, 구글·애플·마이크로소프트 등이 준수·제공하는 문서/규칙을 조사"* — 편집·팝업 기본 규칙이 어긋나는 일이 되풀이돼([61 §2-2-b](61-core-design-and-working-rules.md) · §218) "당연한 것"의 **원천**을 원장으로 삼는다.
> **조사**: 2026-09-26 · 공식 페이지 직접 확인(Apple HIG는 DocC JSON · Material 3는 M2·material-web 문서로 보강). 요약이므로 인용은 짧게 · 세부는 각 URL.
> **적용**: §3 점검표가 새 UI·컨트롤·팝업을 만들 때의 체크리스트(CLAUDE.md §3 팝업 규칙 · 61 §2-2-b의 상위 원천).

---

## 0. 한 줄 결론

**무료·재배포 가능한 W3C(WCAG 2.2 · ARIA APG) · Microsoft Learn(WinUI · Windows UX Guide) · GNOME/KDE HIG · IBM Carbon을 1차 원장으로, Apple HIG·Material 3·NN/g는 열람 원천으로, ISO 9241-110의 7원칙을 상위 범주로 쓴다.** 우리가 겪은 다섯 결함(글자 밀림 · 긴 글 잘림 · 메뉴 클릭 누출 · 메뉴가 선택 가림 · 테두리가 메뉴 위)은 각각 IntelliJ/WinUI(인플레이스 편집 불변) · IntelliJ/Win UX(시작 표시) · **WinUI light-dismiss**(닫는 클릭 흡수) · **Win UX 툴팁·WCAG 2.4.11**(대상을 가리지 않음) · WinUI/Material(팝업 최상위)에 원천이 있다.

---

## 1. 원천 원장

| # | 원천 | URL | 라이선스·이용 | 데이터 도구에 직결되는 규칙(발췌) |
|---|---|---|---|---|
| 1 | **Apple Human Interface Guidelines**(macOS) | https://developer.apple.com/design/human-interface-guidelines/ (`context-menus` · `menus` · `text-fields` · `keyboards` · `lists-and-tables` · `undo-and-redo` · `focus-and-selection`) | 무료 열람 · 저작권 Apple(CC 아님 · 인용 짧게) | 컨텍스트 메뉴는 사용 불가 항목을 **숨긴다**(일반 메뉴는 흐림) · 그룹 ≤ 3 · 서브메뉴 1단 · "포인터가 메뉴를 드러낸 지점에서 가장 가까운 항목부터 읽는다" · 라벨 = 동사 · `…` = 추가 입력 · 텍스트 필드는 경계 밖 클립(앞/중/끝 말줄임 선택) · 표 = 헤더 클릭 정렬(재클릭 역순)·열 크기 조절·교대 행 색 · 표준 단축키(⌘Z/⇧⌘Z/⌘X/C/V/A/F) 재정의 금지 · **Esc = 현재 동작 취소** · 되돌리기 = 다중·무제한·결과 강조 · 포커스 = 텍스트 필드 링 / 목록은 행 하이라이트 |
| 2 | **Google Material Design 3** | https://m3.material.io/ (menus · text-fields · states) · https://github.com/material-components/material-web/blob/main/docs/components/menu.md | 문서 CC BY 4.0(Google 개발자 문서 정책) · 코드 Apache 2.0 · **M3에 Data Table 없음**(M2만) | 메뉴는 앵커에 붙고 경계에 잘리면 **반대편으로**(flip) · `popover` = 최상위 층 · 열리면 첫 항목 포커스 · Esc 닫기 · 타입어헤드 · 항목 클릭 = 닫기 · 닫힌 뒤 포커스 복원 · 상태 층 hover 8 % / focus·pressed 12 % · 텍스트 필드 오류 = 색 + 글 · 잘림보다 확장, 말줄임이면 hover로 전체 |
| 3 | **Microsoft Fluent 2 · Windows 앱 디자인(WinUI)** | https://fluent2.microsoft.design/ · https://learn.microsoft.com/windows/apps/design/ (`controls/menus` · `menus-and-context-menus` · `dialogs-and-flyouts/flyouts` · `text-box` · `input/keyboard-interactions` · `guidelines-for-visualfeedback`) | Learn 문서 CC BY 4.0 · Fluent UI 코드 MIT · 글꼴/아이콘 자산 별도 | ★ **Flyout·메뉴 = light dismiss: 닫는 탭은 흡수되어 아래 UI로 가지 않는다**(통과는 옵트인 · 파괴적 버튼은 통과 금지) · 열린 메뉴는 포커스를 안에 가둠 · 팝업 안에서만 ↑/↓ 순환 · **Esc = 일시 UI·진행 중 변경 취소(원값 복원) · 앱 UI는 닫지 않음** · Enter = 실행/확정 · **단일 포커스 비주얼**(키보드일 때만 · 2+1 px 이중 테두리) · TextBox 메뉴 항목은 상태 의존(Copy/Cut = 선택 · Paste = 클립보드 · Undo = 변경) · "타이핑 중 높이가 자라지 않게" · 대비 4.5:1 / 3:1 |
| 4 | **W3C WAI — WCAG 2.2 · ARIA APG** | https://www.w3.org/WAI/WCAG22/quickref/ · https://www.w3.org/WAI/ARIA/apg/patterns/ (grid · menubar · dialog-modal · combobox) | W3C Document License(복제·배포 무료 · 고지 유지) · WCAG 2.2 = 2023-10 권고(2024-12 편집) | 1.4.3 텍스트 4.5:1 · 1.4.11 UI 3:1 · 2.5.8 타깃 24×24 · 2.1.1/2.1.2 키보드 전 기능·트랩 금지 · 2.4.7 포커스 가시 · ★ **2.4.11 포커스 요소가 다른 콘텐츠에 가려지지 않음** · 2.4.13 포커스 2 px·3:1 · 3.2.1 포커스만으로 컨텍스트 변경 금지 · 3.3.1 오류는 글로 · 3.3.4 되돌림/확인 · **APG grid**: 화살표 셀 · Home/End 행 · Ctrl+Home/End 표 · Shift+Space 행 · Ctrl+Space 열 · Ctrl+A · **Enter/F2 = 편집 진입 · Esc = 편집 취소 + 탐색 복귀** · APG menu: Esc = 닫고 포커스 복귀 · Enter = 실행 후 닫기 |
| 5 | **ISO 9241**(-110 상호작용 원칙 · -171 접근성 · -210 인간 중심 설계) | https://www.iso.org/standard/75258.html · …/39080.html · …/77520.html | **전부 유료**(미리보기만) | -110:2020 7원칙 = 과업 적합성 · 자기 설명성 · 사용자 기대 일치 · 학습 용이성 · 통제 가능성 · **사용 오류 강건성** · 사용자 참여 — 상위 범주로만 차용 |
| 6 | **Nielsen Norman Group 10 휴리스틱** | https://www.nngroup.com/articles/ten-usability-heuristics/ | 무료 열람 · 포스터 무료 · CC 아님 | #1 상태 가시성 · #3 통제와 자유(Undo/Redo · 비상 출구) · #4 일관성·표준 · #5 오류 예방 · #6 인식 > 기억 · #7 전문가 단축키 · #9 오류 메시지 = 평이·정확·건설적 |
| 7 | **GNOME HIG · KDE HIG** | https://developer.gnome.org/hig/ · https://develop.kde.org/hig/ | CC BY-SA 4.0 | GNOME: 메뉴 3~12 · 서브메뉴 3~6 · 팝오버 ≤ 부모 1/3 · **Esc = 일시 컨테이너 닫기** · Shift+F10/Menu = 컨텍스트 메뉴 · Ctrl+Z/Shift+Ctrl+Z · 색만으로 구분 금지 · 깜빡임 금지 / KDE: 무관 항목 비활성(숨기지 않음) · **활성 포커스 항목 ≠ 비활성 뷰의 선택색** · 툴팁에 중요 글 금지 · 기능키 단독 단축키 금지 |
| 8 | JetBrains IntelliJ Platform UI Guidelines | https://plugins.jetbrains.com/docs/intellij/table.html · `input-field.html` · `validation-errors.html` · `tooltip.html` | 문서 Apache 2.0 | 표 셀 편집 = 클릭 · **편집 셀 = 입력 필드 테두리만 추가** · 숫자 우측 정렬 · **포커스가 떠난 표는 비활성 선택색** · 입력 필드 = **비활성 시 값의 시작을 보인다** · 재입력 잦으면 전체 선택 · 검증 = 금지 글자/길이 즉시 · 값은 이탈/제출 · 오류 = 붉은 테두리 + 툴팁(위 컨트롤 안 가리게) · 툴팁 500 ms |
| 9 | IBM Carbon · Atlassian | https://carbondesignsystem.com/components/data-table/usage/ · https://atlassian.design/components/dropdown-menu/usage | Apache 2.0 / 열람 | 다중 선택 헤더 3상태 · 일괄 작업 바 · 정렬 아이콘은 정렬 열만 · 지브라 옵션 / 메뉴 = 트리거 아래 · 포커스 락 · 라벨 잘림 회피 |
| 10 | Windows UX Guidelines(Win7 · 레거시 · "원칙은 유효") | https://learn.microsoft.com/windows/win32/uxguide/guidelines (`cmd-menus` · `ctrl-text-boxes` · `inter-keyboard` · `ctrl-tooltips-and-infotips` · `mess-error`) | CC BY 4.0 | 컨텍스트 메뉴 ≤ 15 · 그룹 ≤ 7 · 순서 = 주 명령 → 보조 → Cut/Copy/Paste → 설정 → Delete/Rename → Properties · Cut/Copy/Paste/Delete/Rename은 늘 두고 비활성 · 컨텍스트 메뉴에 단축키 표시 안 함 · 메뉴 전용 명령 금지 · 숫자·금액 우측 정렬 · 재입력형 = 포커스 시 전체 선택 · 잘못된 입력은 지우지 않음 · 입력 포커스 컨트롤을 비활성화하지 않음 · Esc = 취소 · Enter = 기본 버튼 · **툴팁은 "보거나 조작하려는 대상을 가리지 말 것 · 다음에 볼 항목(오른쪽·아래)도"** · 오류 = 문제·원인·해법 · 타이핑 중 모달 금지 |
| 11 | Shneiderman 8 골든 룰 | (교과서 · 요약 https://ixdf.org/literature/article/shneiderman-s-eight-golden-rules-will-help-you-design-better-interfaces) | 열람 | 일관성 · 단축키 · 피드백 · 닫힘 있는 대화 · 오류 예방 · **쉬운 되돌리기** · 사용자 주도 · 단기 기억 부담 축소 |

---

## 2. 우리가 겪은 결함 ↔ 원천(§218)

| 결함(09-26) | 원천 규칙 | 우리 규칙 |
|---|---|---|
| 편집 진입 시 글자가 밀림 | IntelliJ "편집 셀 = 테두리만 추가" · WinUI "타이핑 중 크기 불변" | 61 §2-2-b ① `TextBox::set_cell_pad` |
| 긴 글이 뒤로 스크롤돼 잘림 | IntelliJ "비활성 시 시작 표시" · Apple 클립 기본 · Win UX 편집형 = 캐럿 끝(재입력형 = 전체 선택) | ② 전체 선택 + 캐럿 앞 · 셀과 같은 폭 |
| 메뉴 항목 클릭이 아래 셀로 샘 | **WinUI light dismiss = 닫는 클릭 흡수 · 통과는 옵트인** | ⑤ 팝업 사각형 안 사건은 팝업에만 · 바깥 클릭만 통과(CLAUDE.md §3) |
| 메뉴가 선택 내용을 가림 | **Win UX 툴팁 "대상을 가리지 말 것" · WCAG 2.4.11** · Apple 포인터 근접 · Material 앵커 인접/flip | ③ 셀 편집 메뉴 = 셀 아래 · `geom::place_popup` |
| 편집 테두리가 메뉴 위 | WinUI 트랩 · Material top-layer · GNOME 오버레이(팝업 = 최상위 전제) | ④ 팝업 층(`paint_overlays`/`paint_popup`) |
| (관례) Esc·Enter | WinUI · APG grid · GNOME · Apple 전부 일치 | ⑥ Esc 취소 · Enter 확정(+아래) · Tab(+오른쪽) |

---

## 3. 점검표(데스크톱 데이터 도구 · 30항 · 새 UI마다)

키보드·포커스
1. 모든 기능은 키보드만으로 · 트랩 금지 [WCAG 2.1.1/2.1.2 · GNOME]
2. 창 안 키보드 포커스 표시는 **정확히 하나** · 포커스 잃은 표는 비활성 선택색 [WinUI · Win UX · IntelliJ · KDE · CLAUDE.md 포커스 규칙]
3. 포커스 요소가 팝업·고정 헤더에 가려지지 않음 [WCAG 2.4.11]
4. 포커스 표시 = 3:1 대비 · 2 px 둘레 이상 [WCAG 2.4.13 · WinUI 2+1 px]
5. 포커스 이동만으로 값 변경·오류·컨텍스트 전환 금지 [WCAG 3.2.1 · Win UX]

색·크기
6. 텍스트 4.5:1 · UI 부품/아이콘 3:1 · 색만으로 정보 전달 금지 [WCAG 1.4.3/1.4.11 · GNOME · Fluent]
7. 클릭 타깃 ≥ 24×24 px [WCAG 2.5.8]

팝업·메뉴
8. 팝업은 창 밖으로 나가지 않고 경계에서 반대편/밀어 넣기 [Material · WinUI · CLAUDE.md 팝업 배치 규칙]
9. 컨텍스트 메뉴는 포인터 인접 · **선택/앵커·다음 대상 가리지 않기** [Apple · Win UX 툴팁 · WCAG 2.4.11 준용]
10. 팝업은 열린 동안 **최상위** · 키보드 포커스 안에 가둠 · 닫히면 포커스 복귀 [WinUI · APG menu]
11. **바깥 클릭은 기본 흡수(닫기)** · 통과시키면 파괴적 대상 제외 · **항목 클릭은 아래로 새지 않음** [WinUI] — 우리는 "바깥 클릭 = 닫고 진행"(CLAUDE.md §3)을 택하되 파괴적 대상(닫기·삭제)에는 통과 금지
12. Esc = 일시 UI 닫기 + 진행 중 편집/값 변경 취소(원값 복원) · 앱 UI는 닫지 않음 [WinUI · APG · GNOME · Apple]
13. Enter = 확정(셀 편집 진입/확정 · 기본 버튼) · Space = 토글/선택 [WinUI · APG grid · Win UX]
14. 팝업 안에서만 ↑/↓ 순환 · 목록/그리드는 순환 금지 [WinUI]
15. 컨텍스트 메뉴 ≤ 15항목 · 그룹 ≤ 7 · 서브메뉴 1단 · 메뉴 전용 명령 금지 [Win UX · Apple · GNOME]
16. 무관 항목 = 제거(단 Cut/Copy/Paste/Delete/Rename은 늘 두고 비활성) [Win UX · Apple · KDE]
17. 순서 = 주 명령 → 보조 → Cut/Copy/Paste → 설정 → Delete/Rename → Properties [Win UX]

키·되돌리기
18. 표준 단축키 재정의 금지(⌘/Ctrl Z·X·C·V·A·F · Shift+F10 · F2 이름 바꾸기) [Apple · WinUI · GNOME · Win UX]
19. 그리드 키: 화살표 셀 · Home/End 행 · Ctrl+Home/End 표 · Shift+Space 행 · Ctrl+Space 열 · Ctrl+A · F2/Enter 편집 [APG grid]
20. 되돌리기 = 다중·무제한 · Edit 메뉴+단축키 · 결과 강조·스크롤 [Apple · NN/g #3 · Shneiderman]

편집·검증
21. 인플레이스 편집: 셀 자리 그대로 · 테두리만 추가 · 편집 중 높이/폭 불변 [IntelliJ · WinUI]
22. 편집 필드 포커스: 편집형 = 캐럿 끝 · 재입력형 = 전체 선택 · 비활성 시 시작 표시 · 긴 글 = 클립/말줄임 + hover 전체 [IntelliJ · Win UX · Apple · Material]
23. 숫자·금액 우측 정렬 · 헤더 클릭 정렬(재클릭 역순) · 열 크기 조절 · 넓은 표 교대 행 [IntelliJ · Win UX · Apple · Carbon]
24. 선택 피드백 즉시 · 다중 선택 헤더 3상태 [Apple · Carbon]
25. 검증 = 금지 글자/길이 즉시 · 값은 이탈/제출 · 타이핑 중 모달 금지 · 잘못된 입력은 지우지 않음 · 첫 오류로 포커스+전체 선택 [IntelliJ · Win UX · WCAG 3.3.1]
26. 오류 메시지 = 문제·원인·해법 · 구체적 이름/값 · 비난 금지 [Win UX · NN/g #9]
27. 파괴적 작업 = 확인 또는 되돌리기 · 초기 포커스 = 안전한 버튼 [WCAG 3.3.4 · APG dialog · WinUI]

툴팁·다이얼로그·상태
28. 툴팁 ≈ 500 ms 뒤 · 대상 옆(가리지 않음) · 중요 정보는 툴팁에만 두지 않음 [Win UX · IntelliJ · KDE]
29. 다이얼로그: Tab 순환 트랩 · Esc 닫기 · 닫힌 뒤 호출 요소로 포커스 [APG dialog · WinUI]
30. 상태 가시성: 실행 중·완료·건수 즉시 · 고대비/큰 글자에서 잘림·대비 검사 [NN/g #1 · GNOME · KDE · ISO 110]

---

## 4. 적용 순서(T-229)

1. 이 표를 [61 §2-2-b](61-core-design-and-working-rules.md)의 상위 원천으로 연결(완료) · CLAUDE.md §3 팝업 규칙 줄에 링크(완료).
2. 현 UI 대조 — 컨트롤별로 §3 항목을 훑어 위반을 TODO에(후보: 컨텍스트 메뉴 항목 수·순서(17) · 포커스 잃은 그리드의 선택색(2) · 포커스 링 2 px(4) · 타깃 24 px(7) · 메뉴 안 ↑/↓ 순환(14) · 툴팁 위치(28)).
3. 새 기능 체크리스트([30 §1-2](30-architecture-patterns.md) · [71 §5](71-performance-review-process.md))에 "88 §3 해당 항목 번호"를 한 줄 추가.

### 4-1. 현 UI 대조 1차(09-26 · 코드 사실 기준 · 결과 그리드·팝업·툴바)

| § | 항목 | 지금 | 판정 | 조치 |
|---|---|---|---|---|
| 2 | 포커스 잃은 표 = 비활성 선택색 | `Grid`에 포커스 상태가 없다 — 편집기로 포커스가 가도 셀 선택색·행 포커스 띠가 활성 그대로 | **위반 후보** | **T-232** 그리드 `focused` 주입(호스트 `Focus::Grid`) → 비활성이면 선택색 알파 절반·캐럿 테두리 생략 |
| 4 | 포커스 링 2 px · 3:1 | nexa-ctl `draw_focus_ring` = 2 px · **알파 0.5**(사용자 확정 헤일로) | 두께 ✓ · 대비는 테마별 실측 필요 | 대비 실측(라이트/다크 `focus_ring`) → 3:1 미달이면 알파 상향 · 61 §2-2-b |
| 7 | 타깃 ≥ 24 px | 툴바 아이콘 32 px(`DEFAULT_ICON`) · 그리드 행 높이 ≈ 20~22 px(글꼴 종속) | 툴바 ✓ · 행은 표 예외(인라인) | — |
| 8·9·10 | 팝업 창 안 · 선택 안 가림 · 최상위 | `geom::place_popup` · 셀 편집 메뉴 = 셀 아래 · `paint_overlays` 지연 그리기(§218) | ✓ | — |
| 11 | 항목 클릭 비전파 · 바깥 클릭 = 닫고 진행(파괴적 제외) | 셀 편집기·편집기·DDL 창 §218 수정 ✓ · 바깥 클릭 통과는 CLAUDE.md §3 규칙 | ✓ · **파괴적 대상 통과 금지**는 명문만(탭 × · 행 삭제 버튼 위 바깥 클릭) | 점검 항목으로 남김(실기 U) |
| 12·13 | Esc 취소(원값) · Enter 확정 | 셀 편집기 Esc = 원값 · Enter = 확정 + 아래 · Tab/Shift+Tab(§229) | ✓ | — |
| 14 | 팝업 안 ↑/↓ 순환 · 표는 순환 금지 | `ContextMenu::move_hover` = 순환(비활성·구분선 건너뜀) · 그리드 `move_sel`은 끝에서 멈춤 | ✓ | — |
| 15 | 컨텍스트 메뉴 ≤ 15 · 그룹 ≤ 7 · **서브메뉴 1단** | 그리드 편집 가능 상태 = 상위 16항목(Copy · Copy with headers · Select All · Advanced ▸ · View value · Set NULL · Duplicate · Insert · Delete · Undo · Redo · Changes · Preview SQL · Apply · Revert …) · **Advanced ▸ Copy SQL ▸ = 2단** | **위반 후보 둘** | **T-233** 메뉴 재편: Copy SQL을 Advanced와 같은 층으로(1단) · Undo/Redo·Changes/Preview는 툴바·키로 이미 있으니 메뉴에서 묶기(≤ 15) |
| 16 | 무관 항목 = 비활성 유지(Cut/Copy/Paste/Delete) | `CtxItem::maybe` = 선택 없으면 Copy 비활성 ✓ · 읽기 전용 결과에는 편집 항목을 **제거**(Apple식) | ✓(두 관례 중 하나) | — |
| 17 | 순서 = 주 명령 → 보조 → Cut/Copy/Paste → 설정 → Delete → Properties | 그리드는 **Copy가 맨 위**(데이터 도구 관례 = DBeaver·DataGrip도 Copy 우선) | 도구 관례 우선 · 예외 기록 | T-233에서 편집 항목 순서만 정돈(주 명령 = View value/Set NULL → 행 → Undo/Redo → Apply/Revert) |
| 18·19 | 표준 키 재정의 금지 · APG grid 키 | ⌘Z/X/C/V/A/F 표준 · 화살표·Home/End·⌘A ✓ · **Shift+Space 행 · ⌃Space 열 선택 없음** · F2 = 북마크(편집은 Enter) | 부분 | 후속(포커스별 키맵 F2 · 행/열 선택 키) |
| 21·22 | 인플레이스 편집 자리 그대로 · 긴 글 첫머리 | `set_cell_pad`(§218) ✓ | ✓ | — |
| 25·26 | 검증 즉시(길이·종류) · 오류 = 문제·원인·해법 | `CellSpec::validate` · 87 §14 상세 문구(단계·문장·키·행 수·되돌림) | ✓ | — |
| 27 | 파괴적 = 확인 또는 되돌리기 | 행 삭제 = 표시만(✓ 전 되돌리기) · 적용 = 사전 검사·1행·롤백 | ✓ | — |
| 28 | 툴팁 ≈ 500 ms · 대상 옆 | `draw_tooltip_in` = 아래 6 px · 없으면 위 · `ui.tooltip_delay_ms` | ✓ | — |
| 30 | 상태 가시성 | 실행 카드 · 푸터 건수 · 편집 `수정 n · 추가 m · 삭제 k` · 적용 로그 | ✓ | 고대비/큰 글자 잘림 검사는 미실측 |

남은 대조 = 접속 창·설정 창·탐색기·편집기 메뉴(각 컨트롤별로 같은 표 한 번씩).

## 5. 원천별 상세(조사 원문 정리 · 09-26)

### 5-1. Apple Human Interface Guidelines(macOS)
- 컨텍스트 메뉴: 사용 불가 항목은 **숨긴다**(일반 메뉴는 흐림 유지) · 그룹 약 3개 이하 · 서브메뉴 1단 · 단축키는 메인 메뉴에만 표시.
- "사람들은 포인터가 메뉴를 드러낸 지점에서 가장 가까운 부분부터 읽는다 — 위/아래로 열리는 위치에 따라 항목 순서를 뒤집는 것도 고려."
- 메뉴 라벨 = 동사 · 제목 대문자 · 관사 제거 · 추가 입력이 필요하면 `…` · 모든 항목이 비활성이어도 메뉴 자체는 열 수 있게.
- 텍스트 필드: 기본은 경계 밖 텍스트를 클립 · 앞/중간/끝 말줄임 선택 가능 · 검증 시점은 필드마다(이메일 = 포커스 이탈 시).
- 표: 열 헤더 클릭 정렬(재클릭 = 역순) · 열 크기 조절 허용 · 넓은 표는 교대 행 색 · 계층은 아웃라인 뷰.
- 키보드: 표준 단축키(⌘Z/⇧⌘Z/⌘X/C/V/A/F) 재정의 금지 · Esc = 현재 동작이나 프로세스 취소 · Control 수식키는 시스템용이라 피함 · Full Keyboard Access 지원.
- 되돌리기: 여러 번 허용 · 불필요한 횟수 제한 금지 · 결과를 강조하고 화면 밖이면 스크롤해 보여 줌 · 메뉴 라벨에 결과 명시("Undo Typing").
- 포커스: 텍스트/검색 필드는 포커스 링, 목록/컬렉션은 행 하이라이트 · 표준 포인터(I-beam 등)를 상태 신호로.

### 5-2. Google Material Design 3
- 메뉴는 앵커에 붙어 열리며 화면/브라우저 경계에 잘릴 위치면 요소의 좌/우/위로 대신 나타난다(M2) · material-web = 뷰포트 밖이면 자동 flip · `popover` 모드는 최상위 층.
- 열리면 첫 항목에 포커스 · Esc 닫기 · 화살표 순환 · 타입어헤드 200 ms · 항목 클릭 = 닫기(`keep-open` 예외) · 닫힌 뒤 이전 포커스 복원.
- 상태 층: hover 8 % · focus/pressed 12 % · 포커스 뒤 hover가 겹치면 hover가 끝나면 focus 층으로 복귀 · 키보드 포커스 = 링 표시.
- 텍스트 필드: 지원 텍스트 1줄 · 글자 수 카운터 · 오류는 색 + 텍스트 · 잘림 대신 줄바꿈/확장 우선, 말줄임이면 hover/링크로 전체 제공.
- M3에는 Data Table 컴포넌트가 없다(M2에만 · material-web 이슈 #3867·#4052).

### 5-3. Microsoft Fluent 2 · Windows 앱 디자인(WinUI)
- Flyout/메뉴 = light dismiss: "탭으로 닫을 때 그 제스처는 흡수되어 아래 UI로 전달되지 않는다 — 뒤의 버튼은 두 번째 탭이 필요" · 통과시키려면 `OverlayInputPassThroughElement`를 지정하되 Close/Delete 같은 파괴적 버튼은 절대 통과 대상에 넣지 말 것.
- 열린 메뉴는 키보드 포커스를 안에 가두고(트랩) 팝업은 화면 공간에 따라 위/아래로 열림 · 팝업 안에서만 ↑/↓ 순환 허용(비팝업 UI에서는 순환 금지).
- Esc: 일시적 UI와 진행 중 동작을 취소 — ComboBox를 열어 값을 바꾸다 Esc = 원래 값으로 복원 · 앱 UI를 닫거나 뒤로 가지 않음. Enter = 명령 실행/피커 확정·닫기.
- 포커스: WinUI는 단일 포커스 비주얼 · 키보드/게임패드일 때만 표시 · 기본 = 바깥 2 px + 안쪽 1 px 이중 테두리 · 초기 포커스는 가장 논리적 요소(파괴적 버튼 금지).
- TextBox: 컨텍스트 메뉴 항목은 상태 의존(Copy/Cut = 선택 있을 때 · Paste = 클립보드에 텍스트 · Undo = 변경됐을 때) · 타이핑 중 높이가 자라게 하지 말 것 · 기본은 교체 아닌 편집(캐럿 놓고 선택 없음) · 교체가 주 용도면 포커스 시 전체 선택.
- 메뉴 아이콘은 표준·잦은 명령에만 · Fluent 2 메뉴 폭 300 px 상한 · 보조 슬롯은 단축키만 · 임시 UI를 닫은 뒤 포커스를 잃지 말 것.
- 대비 4.5:1 / 큰 글자·UI 3:1(WCAG AA) · 200 % 텍스트 확대.

### 5-4. W3C WAI — WCAG 2.2 · ARIA APG
- 1.4.3 텍스트 대비 4.5:1(AA) · 1.4.11 UI 부품/그래픽 3:1(AA) · 2.5.8 타깃 24×24 CSS px(AA · 간격/동등/인라인 예외).
- 2.1.1 모든 기능 키보드 조작 · 2.1.2 키보드 트랩 금지 · 2.4.7 포커스 가시(AA) · **2.4.11 포커스 받은 컴포넌트가 작성자 콘텐츠에 전부 가려지면 안 됨(AA)** · 2.4.13 포커스 표시 = 2 px 둘레 면적 + 3:1 변화(AAA).
- 3.2.1 포커스만으로 컨텍스트 변경 금지 · 3.3.1 오류는 텍스트로 식별·설명 · 3.3.4 되돌릴 수 있거나 검사·확인.
- APG grid: 화살표 셀 이동 · Home/End 행 끝 · Ctrl+Home/End 표 끝 · Shift+Space 행 선택 · Ctrl+Space 열 선택 · Ctrl+A 전체 · Enter/F2 = 편집 진입 · 다음 Enter는 이웃 셀 이동 가능 · **Escape = 그리드 탐색 복귀, 편집 중이면 편집 취소**.
- APG menu: Escape = 포커스가 있는 메뉴를 닫고 열었던 요소로 포커스 복귀 · 출력 가능 문자 = 타입어헤드 · Enter = 실행 후 닫기 · Tab = 메뉴 밖으로.
- APG dialog: Tab 순환 트랩 · Esc 닫기 · 닫힌 뒤 호출 요소로 포커스 · 파괴적 다이얼로그는 가장 안전한 버튼에 초기 포커스.
- APG combobox: ↓ 열기 · Esc = 팝업 닫기(이미 닫혔으면 입력 지우기 선택) · Enter = 제안 수락.

### 5-5. ISO 9241 시리즈(유료)
- 9241-110:2020 "Interaction principles" — 7 원칙: 과업 적합성 · 자기 설명성 · 사용자 기대 일치 · 학습 용이성 · 통제 가능성(개인화 통합) · 사용 오류 강건성(회피·허용·복구) · 사용자 참여(신설). 20 범주 · 65 권고 · 약 15쪽.
- 9241-171:2008 "Guidance on software accessibility" — 입력/출력/문서 전반 접근성 지침(키보드 포커스 가시성 · 포커스 커서 등) · 개정 FDIS 진행 중.
- 9241-210:2019 "Human-centred design for interactive systems" — 프로세스 표준(원칙 6 + 활동 4: 맥락 이해 → 요구 → 설계안 → 평가 · 반복).
- 소규모 팀: 원문 구매 대신 110의 7 원칙을 체크리스트 상위 항목으로 쓰고 세부 규칙은 무료 원천에 위임.

### 5-6. Nielsen Norman Group 10 휴리스틱
- #1 상태 가시성(진행·결과 즉시 피드백) · #3 사용자 통제와 자유("비상 출구" · Undo/Redo) · #4 일관성·표준(플랫폼 관례) · #5 오류 예방(확인·제약·기본값) · #6 인식 > 기억 · #7 유연성·효율(전문가 단축키) · #9 오류 메시지 = 평이한 언어·정확한 문제·건설적 해법. 10항목을 리뷰 체크리스트로 그대로 쓴다.

### 5-7. GNOME HIG · KDE HIG
- GNOME: 메뉴 3~12 항목 · 서브메뉴 3~6 · 중첩 금지 · 모든 항목 액세스 키 · 동사 라벨 · 팝오버는 부모 창의 1/3을 넘지 않게 · Esc로 닫힘 보장 · Return = 활성화 · Space = 토글 · F10 메뉴 · Shift+F10/Menu = 컨텍스트 메뉴 · Esc = 일시적 컨테이너(메뉴·팝오버·다이얼로그) 닫기 · Ctrl+Z / Shift+Ctrl+Z · Super 예약 · 모든 동작은 키보드로도 · 색만으로 정보 구분 금지 · 고대비/큰 글자 모드 테스트 · 깜빡임 금지.
- KDE(2024 개편 · 원칙 중심): 메뉴바/햄버거 메뉴 = 정적 내용 · 무관 항목은 숨기지 말고 비활성(Apple 컨텍스트 메뉴 규칙과 대비) · 햄버거 ≤ 15항목 · 활성 포커스 항목은 비활성 뷰의 선택 항목과 눈에 띄게 달라야 · 기본 포커스 위치 논리적 · 마우스 없이 전 UI 조작 테스트 · 표 = 항목당 데이터 3개 이상·비교가 의미 있을 때 · 선택 항목 볼드 · hover 툴팁에 중요한 텍스트를 넣지 말 것 · 시스템 글꼴 14로 잘림 검사 · 단축키 = 수정키+영숫자, 기능키 단독 금지.

### 5-8. 선택 원천
- **JetBrains IntelliJ Platform UI Guidelines**(Apache 2.0): 표 = 텍스트 데이터는 마우스 클릭으로 편집 활성 · 편집 셀 테두리 = 입력 필드 테두리 색 · 숫자는 길이 비교가 유용할 때 우측 정렬, 그 외 좌측 · 포커스가 다른 곳으로 가면 표에 활성 선택색을 남기지 말 것(비활성 선택색) · 빈 표는 이유+행동 링크. 입력 필드 = 비활성이 되면 값의 시작 부분을 보인다 · 포커스 시 캐럿 끝(재입력이 잦으면 전체 선택) · 검증 = 허용 안 되는 글자/길이는 즉시, 값 검증은 포커스 이탈·제출 중 빠른 쪽 · 오류 = 빨간 테두리 + 툴팁(필드 위 · 40 px 우측 이동해 위 컨트롤 가리지 않게) · 툴팁 500 ms · 도움 툴팁 250 px 폭.
- **IBM Carbon**(Apache 2.0): 다중선택 = 체크박스 + 헤더 3상태 · 일괄 작업 바 · 정렬 아이콘은 정렬된 열만(그 외 hover) · 지브라 옵션 · 행 오버플로 메뉴 · 확장 행. 인라인 편집 지침 없음.
- **Atlassian**: 메뉴는 트리거 아래 · 포커스 락 · 키보드로 열면 첫 항목 포커스 · 라벨 잘림 회피.
- **Shneiderman 8 골든 룰**: 일관성 · 단축키 · 정보 피드백 · 닫힘 있는 대화 · 오류 예방 · 쉬운 되돌리기 · 사용자 주도 · 단기 기억 부담 축소.
- **Windows UX Guidelines**(Win7 레거시 · "원칙은 여전히 유효"): 컨텍스트 메뉴 ≤ 15항목·그룹 ≤ 7·서브메뉴 회피 · 순서 = 주 명령 → 보조 → Cut/Copy/Paste → 설정 → Delete/Rename → Properties · 기본 명령 볼드 · 무관 항목 제거(단 Cut/Copy/Paste/Delete/Rename은 항상 두고 비활성) · 컨텍스트 메뉴엔 단축키 표시 안 함 · 컨텍스트 메뉴만으로 접근 가능한 명령 금지. 텍스트 박스: 숫자·금액 우측 정렬 · 재입력형 = 포커스 시 전체 선택, 편집형 = 캐럿 끝 · 잘못된 입력은 지우지 말 것 · 단일 줄 읽기 전용은 테두리 없음·비활성 금지. 키보드: 입력 포커스가 있는 컨트롤을 비활성화하지 말 것(먼저 포커스 이동) · 포커스 표시는 항상 · Esc = 취소/닫기 · Enter = 기본 버튼 · 탭 순서 = 읽기 순서 · F2 = 선택 항목 이름 바꾸기 · 탐색만으로 값이 바뀌거나 오류가 나면 안 됨. 툴팁: 사용자가 보거나 조작하려는 대상을 가리지 말 것 — 포인터와 떨어지더라도 대상 옆에 · 항목 모음에서는 다음에 볼 항목(가로 = 오른쪽 · 세로 = 아래) 가리지 않기 · 예외 = 목록/트리의 전체 이름 툴팁만 대상 위. 오류 메시지 = 문제·원인·해법 · 사용자 비난 금지 · 구체적 이름·값 · 입력 문제는 첫 오류 컨트롤에 포커스+전체 선택 · 타이핑 중 모달 금지(풍선/인플레이스).

### 5-9. 우리 여덟 질문 (a)~(h) — 원천별 판정

| 질문 | Apple | Material | WinUI/Fluent | W3C | GNOME/KDE | IntelliJ/Win UX |
|---|---|---|---|---|---|---|
| (a) 컨텍스트 메뉴가 선택/앵커를 가리지 않고 포인터 인접 | 포인터 근접 읽기 순서 ✅ · 가림 금지 명문 없음 | 앵커 인접 ✅ · 경계 시 반대편 ✅ | 대상 기준 위/아래 · 가림 금지 명문 없음 | **2.4.11 포커스 요소 가림 금지** ✅ | — | **Win UX 툴팁 "대상을 가리지 말 것"** ✅ |
| (b) 팝업 항상 최상위 | — | top-layer ✅ | 포커스 트랩 = 사실상 최상위 ✅ | — | 팝오버 오버레이 ✅ | — |
| (c) 항목 클릭이 아래로 새지 않음 | — | — | **닫는 클릭 흡수(기본) · 통과는 옵트인** ✅ | — | — | — |
| (d) 인플레이스 편집의 텍스트 위치 불변 | — | — | 타이핑 중 크기 불변 ✅ | — | — | 편집 셀 = 테두리만 추가 ✅ |
| (e) 긴 텍스트는 시작 표시 | 클립 기본 + 말줄임 위치 선택 | 말줄임이면 hover 전체 | — | — | — | **비활성 시 시작 표시**(IntelliJ) · 편집형 캐럿 끝(Win UX) |
| (f) 포커스 링 정확히 하나 | 링/하이라이트 구분 | 링 ✅ | **단일 포커스 비주얼** ✅ | 2.4.7 · roving focus ✅ | KDE 활성/비활성 구분 ✅ | Win UX "포커스 컨트롤 하나" ✅ |
| (g) Esc 취소 / Enter 확정 | Esc = 취소 ✅ | Esc ✅ | **가장 구체적**(원값 복원) ✅ | **APG grid 가장 명확** ✅ | GNOME Esc ✅ | Win UX ✅ |
| (h) 되돌리기 어디서나 | **다중·무제한·결과 표시** ✅ | — | TextBox Undo 항목 | 3.3.4 | 표준 단축키 ✅ | — |

## 5. 미확인·주의

Material 3 공식 페이지 본문은 스크립트 렌더링이라 직접 인용 불가(M2·material-web으로 대체) · ISO 가격은 판매처가만 확인 · KDE 개편 HIG·IntelliJ 표 페이지에는 컨텍스트 메뉴/F2·Esc 세부가 없음 · Fluent 2 메뉴 페이지는 키보드·light dismiss 세부를 싣지 않음(WinUI Learn이 원천).
