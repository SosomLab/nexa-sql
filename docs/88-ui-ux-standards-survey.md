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

## 5. 미확인·주의

Material 3 공식 페이지 본문은 스크립트 렌더링이라 직접 인용 불가(M2·material-web으로 대체) · ISO 가격은 판매처가만 확인 · KDE 개편 HIG·IntelliJ 표 페이지에는 컨텍스트 메뉴/F2·Esc 세부가 없음 · Fluent 2 메뉴 페이지는 키보드·light dismiss 세부를 싣지 않음(WinUI Learn이 원천).
