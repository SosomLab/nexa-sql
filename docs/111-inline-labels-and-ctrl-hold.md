# 111 · 줄 끝 라벨 · Ctrl 홀드 표시 — 북마크 라벨 · 테이블/컬럼 설명 (사용자 10-13 · 설계만)

> **요구**(사용자 10-13 · 개발 세션 전달 원문 요지): ① `bookmark.inline_label`이 꺼져 있어도 **Ctrl을 누른 동안** 줄 끝 라벨이 보이는 설정 ② 테이블 Description·컬럼 Description을 줄 끝에 표시하는 기능 — **상시 설정 + Ctrl 홀드 설정을 같은 개념으로** ③ Ctrl 표시 적용 범위 = 1) 전체 문서 2) 보이는 영역만 3) 현재 Statement · **기본 = 현재 Statement** ④ 두 기능 모두 향상 모드(`perf.boost`)에서는 Ctrl 기능 꺼짐 ⑤ **설계만 · 추가 개발 대상 후보**("설계에만 반영 · 추가 개발 대상으로").
> **상태**: ✅ **개발됨 10-13**(사용자 "개발 진행" · D-281~D-284 권장안 채택 · `app/inline_labels.rs` · 설정 6키 · Debug 훅 `ui.key:ctrl_down|ctrl_up`·`editor.inline_dump:<경로>` · 협업 V1 ✓ = journal 10-13 §6) · 설계 = 개발 세션 결정 §1~§10.
> **짝 문서**: 북마크 줄 끝 라벨 [69](69-bookmarks.md)(`bookmark.inline_label` · nexa-ctl `set_inline_labels`) · Ctrl 객체 링크 [96](96-object-links.md)(`objlink::scan` · `Resolver` · Ctrl 홀드) · 메타 3층 [85](85-metadata-layers.md)(L2 코멘트 워머) · 객체 상세 [86](86-object-details-panel.md)(코멘트 = 값 그대로) · 지연 로딩 [61 §1-8](61-core-design-and-working-rules.md) · 향상 모드 [39 §4-6](39-resource-governance.md) · 키 규칙 [94](94-settings-key-naming-and-location.md) · 부품 원장 [30 §2](30-architecture-patterns.md).

## §0. 결론
- 새로 만드는 부품은 **호스트 쪽 합성기 하나**(`app/inline_labels.rs` `InlineLabels`)뿐이다. 그리기(nexa-ctl 줄 끝 라벨) · 객체 판정(`objlink::scan` + `Resolver`) · 설명 원천(MetaStore 코멘트) · Ctrl 상태(objlink 깃발)는 **이미 있는 것을 그대로** 쓴다.
- 북마크 라벨과 객체 설명은 **같은 개념**(상시 켬 / Ctrl 홀드 켬 / 범위)으로 다루되, **범위 키는 둘**이다 — 북마크 `bookmark.inline_label_ctrl_scope`(기본 `visible`) · 설명 `editor.inline_ctrl_scope`(기본 `statement`)(10-13 사용자 지시로 D-282 "키 하나 공유"를 뒤집음).
- 라벨은 **캐시에 있는 코멘트만** 쓴다(왕복 0) · Ctrl을 누른 순간 범위 안에 코멘트가 아직 없는 객체만 L2 워머에 1회 요청(D-284).
- 향상 모드 = Ctrl 기능 두 개를 꺼서 강제(FORCED) · 큰 파일 모드 = 꺼짐.

## §1. 부품 — 기존 셋 재사용(30 §2 규칙)
| 역할 | 부품 | 비고 |
|---|---|---|
| 줄 끝 라벨 그리기 | nexa-ctl `TextBox::set_inline_labels(Vec<(줄, 글)>)` | **줄당 라벨 하나** → 호스트가 합성(`◆ 북마크` 먼저 · 설명은 ` · ` 뒤 `» 설명`) |
| 객체 판정 | `app/objlink.rs::scan` + `Resolver` 포트(96 §1·§3) | 별칭·스키마·카탈로그 판정 그대로 · 세션 기준(96 §6) |
| 설명 원천 | MetaStore 코멘트(85 L2 · `meta.warm_comments` 워머) | hover 카드·객체 상세 패널과 **같은 값**(86 = 코멘트 그대로) |
| Ctrl 상태 | objlink의 Ctrl 홀드 깃발(`ctrl_down` · 96 §4) | 같은 깃발 · 두 번째 판정을 만들지 않는다 |

## §2. 새 부품 — `app/inline_labels.rs` `InlineLabels`(합성기)
- **입력**: 북마크 라벨(상시 켬 / Ctrl 켬) · 설명(상시 켬 / Ctrl 켬) · Ctrl 눌림 · 범위 설정 · 캐럿 문장 범위 · 보이는 줄 범위 · 큰 파일 모드 · 향상 모드.
- **출력**: 줄별 글 → `set_inline_labels`.
- **순수 함수 두 개**(52 §12 규칙 = 조건 2개 이상이면 순수 함수 + MC/DC 시험):
  - `wanted(cfg, state) -> Scope` — 무엇을(북마크/설명) 어느 범위(없음·문장·보이는 영역·문서)에 그릴지.
  - `compose(line, bookmark: Option<&str>, descs: &[&str], max_chars) -> Option<String>` — 한 줄 합성.

## §3. 범위 셋(북마크 `bookmark.inline_label_ctrl_scope` · 설명 `editor.inline_ctrl_scope` — 값은 같은 셋)
| 값 | 범위 | 비고 |
|---|---|---|
| `statement`(**기본**) | 캐럿이 있는 문장(`statement_at` 범위) | 보통 수십 줄 |
| `visible` | `TextBox::visible_range` | 스크롤 때 다시 계산 |
| `document` | 문서 전체 | 큰 파일 모드 L1 이상 · `objlink.max_kb` 초과면 Ctrl 표시 꺼짐 |

- **다시 계산하는 사건**: Ctrl 누름/뗌 · 캐럿 문장 바뀜(`statement`) · 스크롤(`visible`) · 메타 갱신(`intel.refresh`) · 북마크 변경. **매 프레임 0 · 사건마다 한 번.**
- 상시 모드(Ctrl 없이 켬)의 범위는 북마크 = 지금 규칙 그대로(문서의 북마크 줄 전부) · 설명 = 설명 범위 키를 따른다.

### §3-1. "현재 문장" = 위에 붙은 주석 줄 포함(10-13 사용자 지시)
- 문장 범위는 `statement_at` 범위에 **문장 바로 위에 붙은 연속 주석 줄**(`--` 줄 · `/* */` 블록 · 사이의 빈 줄은 건너뜀)을 더한다 → 주석 줄에 단 북마크가 Ctrl에 보인다(순수 함수 `widen_comment_lines` + 시험 3경로).
- **앞 문장 꼬리 주석은 가져오지 않는다**(앞 문장과 같은 줄·바로 뒤에 붙은 주석은 앞 문장 몫).
- 협업 V1(10-13): 줄1 `-- 주석`에 단 북마크 = 캐럿이 줄2 문장일 때 Ctrl에 보임 · 캐럿을 다른 문장으로 옮기면 빠짐.

### §3-2. 문서 용량 단계(10-13 사용자 추가 지시)
| 문서 크기 | Ctrl 라벨·설명 범위 | 키 |
|---|---|---|
| ≤ `editor.inline_ctrl_full_kb`(**256** KB · 0 = 무제한) | 고른 범위 그대로(`document` 포함) | — |
| full_kb < 크기 ≤ `editor.inline_ctrl_off_kb`(**2048** KB) | `document` → **보이는 영역으로 낮춤**(`visible`·`statement`는 그대로) | — |
| > off_kb(0 = 큰 파일 모드에서만 끔) · 큰 파일 모드 | **끔**(Ctrl 라벨·설명 둘 다) | — |
- 북마크·설명 공통 · 상시 북마크 라벨(`bookmark.inline_label`)은 영향 없음.
- 부분 분석(`objlink.max_kb` 초과)도 끔이 아니라 보이는 영역으로 낮춘다.
- 구현 = `inline_labels.rs` `Cfg{doc_kb, full_kb, off_kb}` · `wanted` 규칙 · MC/DC 시험 보강.
- 협업 V1(10-13): 300 KB · document = 보이는 줄만(`full_kb=0` = 끝 줄까지) · 3 MB = 빈칸(`off_kb=0` = 다시 됨).

## §4. 데이터 규칙(61 §1-8 지연 로딩)
- 라벨은 **캐시에 있는 코멘트만** 쓴다(왕복 0).
- Ctrl을 누른 순간: 범위 안 객체 중 코멘트가 아직 없는 것만 L2 워머에 **1회** 요청(상한 = 범위 안 객체 수) → 응답이 오면 다시 합성(D-284 · 대안 = 캐시만).
- 상시 모드도 같은 길(문서 열림·문장 이동 때).
- 설명 없음 = 라벨 없음(빈 글을 그리지 않는다).

## §5. 설정(키 = 94 규칙)
| 분류 | 키 | 종류 · 기본 | 뜻 |
|---|---|---|---|
| Bookmarks | `bookmark.inline_label`(기존) | Bool · on | 상시 줄 끝 라벨 |
| Bookmarks | **`bookmark.inline_label_ctrl`** | Bool · **on** | 상시가 꺼져 있어도 Ctrl을 누른 동안 라벨 표시 |
| Editor ▸ 줄 끝 설명(새 소분류) | **`editor.desc_inline`** | Bool · **off** | 테이블/컬럼 설명 상시 표시 |
| 〃 | **`editor.desc_inline_ctrl`** | Bool · **on** | Ctrl을 누른 동안 설명 표시 |
| 〃 | **`editor.desc_inline_what`** | Choice `table`\|`column`\|`both` · both | 무엇의 설명을 |
| 〃 | **`editor.desc_inline_chars`** | Int · 40(8~200) | 한 줄 라벨 총 길이 상한(넘으면 `…`) |
| Bookmarks | **`bookmark.inline_label_ctrl_scope`** | Choice `document`\|`visible`\|`statement` · **visible** | 북마크 Ctrl 라벨 범위(10-13 사용자 지시 · D-282 뒤집음) |
| Editor ▸ 줄 끝 설명 | **`editor.inline_ctrl_scope`** | Choice `document`\|`visible`\|`statement` · **statement** | 객체 설명 Ctrl 표시 범위 |
| Editor ▸ 줄 끝 설명 | **`editor.inline_ctrl_full_kb`** | Int · **256**(0 = 무제한) | 이 용량까지 `document` 범위 그대로 · 넘으면 보이는 영역으로(§3-2) |
| Editor ▸ 줄 끝 설명 | **`editor.inline_ctrl_off_kb`** | Int · **2048**(0 = 큰 파일 모드만) | 넘으면 Ctrl 라벨·설명·hover 카드 끔(§3-2 · 96 §2) |

- **향상 모드**: `perf` FORCED에 `bookmark.inline_label_ctrl=off` · `editor.desc_inline_ctrl=off`(강제값 > 개별 값 · 39 §4-6 · 설정 창은 FORCED 잠금 그대로). 기존 `bookmark.inline_label`의 강제 off(69 §설정 끝)와 같은 줄.
- 큰 파일 모드 = Ctrl 표시 둘 다 꺼짐(objlink와 같은 규칙).

## §6. 표시 모양(88 · 머티리얼)
- 북마크 라벨 = 지금 그대로(`◆ 라벨` · 흐린 색).
- 설명 = `» 설명`(흐린 색) · 북마크 라벨이 있으면 그 뒤 ` · `.
- 한 줄에 객체가 여럿 = 등장 순서 · 총 길이 `editor.desc_inline_chars`에서 `…`(D-283).
- 컬럼 설명은 `컬럼명: 설명`이 아니라 **설명만**(위치가 곧 컬럼).
- 캐럿 줄이어도 라벨을 캐럿 뒤로 밀지 않는다(지금 북마크 규칙 유지).
- 예: `SELECT e.ename, e.sal  ◆ 급여 점검 · » 사원 이름 · 급여(원)`

## §7. 성능(39 §3 부하원 등재)
| 부하원 | 상한 | 키 | full | balanced | low | 근거 |
|---|---|---|---|---|---|---|
| 줄 끝 라벨 Ctrl 홀드 합성(T-337·T-338) | 사건당 1회 · 범위 = 기본 문장(수십 줄) · `document`는 `objlink.max_kb` 상한 공유 · 코멘트 = 캐시만 + Ctrl 순간 워머 1회 | `bookmark.inline_label_ctrl` · `editor.desc_inline_ctrl` · `bookmark.inline_label_ctrl_scope` · `editor.inline_ctrl_scope` | 켬 | 켬 | 끔(향상 모드 FORCED) | `app/inline_labels.rs` |
- 큰 파일 모드 = 끔 · 매 프레임 비용 0.

## §8. 못 하는 것 · 보류
- 뷰 컬럼 설명: 뷰는 카탈로그 코멘트가 드물다 · 있으면 같은 길로 보인다.
- SQL Server 확장 속성(`MS_Description`) = 이미 코멘트로 읽는다(86).
- 별칭 없는 모호한 컬럼: objlink 판정이 객체를 못 정하면 라벨 없음.

## §9. 결정(10-13 권장안 채택)
- **D-281** 기본값 = `bookmark.inline_label_ctrl` on · `editor.desc_inline` off · `editor.desc_inline_ctrl` on — 권장안.
- **D-282** Ctrl 범위 키 = **기능별 두 키**(10-13 사용자 지시 · 권장안 "하나 공유"를 뒤집음) = 북마크 `bookmark.inline_label_ctrl_scope`(visible) · 설명 `editor.inline_ctrl_scope`(statement).
- **D-283** 설명 글머리 `»` · 총 길이 기본 40 — 권장안.
- **D-284** Ctrl을 누른 순간 범위 안 코멘트 없는 객체만 L2 워머에 1회 요청 허용 — 권장안(대안 = 캐시에 있는 것만 · 처음엔 비어 보일 수 있음).

## §10. 단계(TODO · 전부 P2 · "대기 · 사용자 개발 지시 뒤")
- **T-337 북마크 Ctrl 홀드**(소): `bookmark.inline_label_ctrl` · 합성기 `InlineLabels`(`wanted`·`compose`) · 범위 키 · FORCED.
- **T-338 객체 설명 줄 끝 표시**(중): objlink `scan` 연동 · 코멘트 캐시 · Ctrl 순간 워머 1회 · 설정 4키.
- **T-339 자체 시험·E2E**(소): 기동 명령 `ui.key:ctrl_down/up` 또는 훅 `editor.inline_dump` · 범위 3 MC/DC · 향상 모드 A/B · 기능 점검 시나리오 S98.
