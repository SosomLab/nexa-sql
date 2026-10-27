# 124. Insert Sequence — 다중 캐럿 일괄 생성(숫자 · 문자 · 시간 · 목록 · 곱집합)

> 사용자 10-27(142차 win 뒤): *"Sublime Insert Nums · VS Code Insert Numbers를 참고해 컬럼 모드에서 숫자·문자 등 일괄 생성 — VS Code식 상단 표현식
> 입력창 + 미리보기 · 도움말 · Placeholder = 기본값 표현식 · 빈 입력 = Placeholder 결과 · 오류 = 공백 · 기능 범위를 정의하고 표현식을 통일되고 직관적으로."*
> 이름 = **Insert Sequence**(사용자 지정). 내장 기능(확장 SDK의 편집기 API는 괄호 이동뿐 · 다중 캐럿 실시간 편집·상단 입력은 호스트만 가능 · [75 §3](75-extension-sdk-and-dynamic-loading.md)).

## 0. 결론 한 장

- **진입** = `Edit ▸ Insert Sequence…` · 명령 팔레트 · 키 **Ctrl+Alt+N**(Sublime Insert Nums · VS Code Insert Numbers와 같은 키 · mac `Cmd+Alt+N`) · 명령 id `edit.insert_sequence`.
- **UI** = 명령 팔레트의 **프롬프트 모드**(상단 입력 상자 · `tab.rename`와 같은 부품) + **미리보기 행**(캐럿마다 한 줄 `L줄: 값` · 많으면 "…외 n개") + **본문 실시간 미리보기**(VS Code Insert Numbers처럼 캐럿 자리에 실제로 넣어 보여 준다 · Enter = 확정 · Esc/바깥 클릭 = 원문 복원) · 되돌리기 **한 단계**(`begin_group`/`end_group`).
- **Placeholder** = 기본 표현식 `1+1`(설정 `editor.seq_default`) — 빈 입력이면 그 결과를 미리보고 Enter로 넣는다 · 표현식 오류 = 미리보기 행에 오류 글(빨강) + 본문은 **공백**.
- **도움말** = 입력 상자에 `?`(만) 치면 미리보기 자리에 치트 시트(표현식별 한 줄 · 아래 §2 표 그대로) · 안내 줄 "Enter 넣기 · Esc 취소 · ? 도움말".
- **표현식 한 줄 문법**(§2) = `시작[연산 증가][#반복][~형식] [x 목록 …] ["틀"]` — 모든 종류(정수 · 소수 · 진수 · 문자 · 시각 · 날짜 · 목록)가 같은 자리에 같은 꼴.
- 구현 = 순수 엔진 `crates/nexa-sql/src/insseq.rs`(파서 · 생성기 · 도움말 표 · 시험) + 호스트 `app/insseq.rs`(세션 = 원 구간·원문 · `replace_many`로 매번 교체 · 확정/취소) + 팔레트 프롬프트 행 · 키맵 · 메뉴 · i18n · 기동 명령(자체 시험).

## 1. 조사 — 베낄 것과 피할 것

| 도구 | 문법 | 채택 |
|---|---|---|
| **Sublime Insert Nums**(James Brooks) | `시작:증가~형식` · 알파 `a:1` · `!` 역순 · `@` 중단식 · `\|` 수식 | 시작·증가·형식의 **3부 구성** · 알파벳 연번(a…z, aa) · 형식 접미 `~` |
| **VS Code Insert Numbers**(Asuka) | `시작:증가:반복` + `%` 형식(`%03d` `%x`) · 입력 상자에서 바로 미리보기 | **본문 실시간 미리보기** · 반복 · printf식 폭/0채움/진수 |
| Sublime Text Pastry · Notepad++ Column Editor | 목록 붙여넣기 · 날짜 · UUID · 시작·증가·0채움 체크박스 | 목록 순환 `[a,b,c]` · 날짜·시각 증분 |
| Excel 자동 채우기 | 1,2,3… · 월·요일 · 날짜 · 시각 | 날짜·시각 단위 증분 · 알파 열 이름(Z 다음 AA) |

피할 것 = `:`를 증가 구분자로 쓰면 시각(`09:30`)과 부딪힌다 → 증가는 **연산자**(`+ - * ^`)로 적는다(읽으면 "1에서 1씩 더해" · 음수 증가도 `10-1`로 자연스럽다).

## 2. 표현식 문법(한 벌)

```
식   := 항 { 'x' 항 } [ 공백 틀 ]
항   := 시작 [ 연산 증가 ] [ '#' 반복 ] [ '~' 형식 ]
시작 := 정수 | 0x… 0b… 0o… | 소수 | 알파벳 | 글자 | 시각 | 날짜 | 목록
연산 := '+' | '-' | '*' | '^'           (더하기 · 빼기 · 곱하기(등비) · 제곱)
목록 := '[' 원소 {',' 원소} ']'          (원소 = 값 | 범위 `a..e` `1..10` `1..10+3`)
틀   := '"…{}…"'                        ({} = 값 · {1}{2} = 곱집합 각 항 · {n} = 1부터 번호 · {i} = 0부터)
```

| 하고 싶은 것 | 표현식 | 결과(캐럿 5개) |
|---|---|---|
| 기본(1부터 1씩) | *(비움)* = `1+1` | 1 2 3 4 5 |
| 시작·증가 | `10+5` | 10 15 20 25 30 |
| 내려가기 | `10-2` | 10 8 6 4 2 |
| 음수 시작 | `-3+1` | -3 -2 -1 0 1 |
| 등비 | `1*2` | 1 2 4 8 16 |
| 제곱(시작부터 n²) | `1^2` | 1 4 9 16 25 |
| 세제곱 | `2^3` | 8 27 64 125 216 |
| 반복(값마다 k번) | `1+1#2` | 1 1 2 2 3 |
| 0 채움 자리수 | `1+1~03` | 001 002 003 004 005 |
| 자리수(공백 채움 · 오른쪽 정렬) | `1+1~3` | `  1` `  2` … |
| 부호 표시 | `-1+1~+` | -1 +0 +1 +2 +3 |
| 16진수 | `250+1~x` · `~X` | fa fb fc … · FA FB … |
| 2진수 · 8진수 | `1+1~b` · `~o` | 1 10 11 100 101 · 1 2 3 4 5 |
| 진수 + 0채움 | `1+1~04b` | 0001 0010 0011 … |
| 진수 시작값 | `0x1f+1` | 1f 20 21 …(시작이 진수면 출력도 그 진수 · `~d`로 10진) |
| 접두 붙이기 | `1+1~x "0x{}"` | 0x1 0x2 0x3 … |
| 소수 증분(정밀) | `0.2+0.02` | 0.2 0.22 0.24 0.26 0.28 **0.3**(오차 없음 · 끝 0 정리) |
| 소수 자리 고정 | `0.2+0.02~.2` | 0.20 0.22 … 0.30 |
| 알파벳 | `a+1` · `A+1` · `aa+1` | a b c d e · A B … · aa ab ac …(z 다음 aa = 엑셀 열) |
| 유니코드 글자 | `가+1` · `①+1` · `Ⅰ+1` | 가 각 갂 … · ① ② ③ … · Ⅰ Ⅱ Ⅲ …(코드 포인트 순) |
| 시각(시:분) | `09:00+15` | 09:00 09:15 09:30 09:45 10:00(증가 단위 = 마지막 자리 = 분) |
| 시각(시:분:초) | `09:00:00+90s` · `+1h` · `+15m` | 09:00:00 09:01:30 09:03:00 … |
| 분:초로 보기 | `00:00+30s~mm:ss` | 00:00 00:30 01:00 … |
| 날짜 | `2026-10-27+1` · `+1w` · `+1M` · `+1y` | 2026-10-27 2026-10-28 …(일 · 주 · 달 · 해) |
| 날짜 형식 | `2026-10-27+1~yyyy/MM/dd` · `~MM-dd` · `~yyyyMMdd` | 2026/10/27 … |
| 목록 순환 | `[월,화,수]` | 월 화 수 월 화 |
| 목록 범위 | `[1..3]` · `[a..c]` · `[0..20+5]` | 1 2 3 1 2 · a b c a b · 0 5 10 15 20 |
| 곱집합(Product) | `[A,B]x[1,2]` · `[A,B]*[1,2]`(149차 별칭) | A1 A2 B1 B2 A1(순환) |
| 곱집합 + 틀 | `[A,B]x[1..3] "{1}-{2}"` | A-1 A-2 A-3 B-1 B-2 |
| 틀(앞뒤 글) | `1+1 "col_{}"` | col_1 col_2 … |
| 번호 틀 | `[x,y] "{n}. {}"` | 1. x  2. y  3. x … |

규칙:
- 공백은 무시한다(`1 + 1 ~ 03`도 같다) · 대소문자는 알파벳 시작값·진수 형식 글자(`x`/`X`)에만 뜻이 있다.
- **형식 `~`** = `[+][0][폭][.소수][b|o|d|x|X]` 또는 시각·날짜 패턴(`HH` `H` `mm` `ss` `yyyy` `yy` `MM` `M` `dd` `d`). 숫자 형식은 알파벳·글자에는 폭만 적용(공백 채움).
- **증가 단위**(시각·날짜) = 단위 생략 시 **시작값의 마지막 자리**(`HH:mm` → 분 · `HH:mm:ss` → 초 · 날짜 → 일) · 명시 = `s m h` · `d w M y`. 시각은 24시간을 넘으면 다음 날로 돌아간다(날짜 없음).
- **소수** = 10진 정밀 연산(이진 부동소수 오차 없음) · 자리수 = 시작·증가의 소수 자리 중 큰 쪽 · 기본은 끝 0 정리(`0.30` → `0.3`) · `~.n`이면 고정.
- ★ **곱집합 구분자 = `x` · `×` · `*`**(149차 · 사용자 10-28 · D-370) — `*`는 **`]` 바로 뒤에서만** 곱집합(`[A,B]*[1,2]`) · 숫자 항 뒤 `*`는 그대로 등비(`1*2` = 1 2 4 8) · 치트 시트 = `[A,B]*[1,2]`.
- **곱집합** `x`의 피연산자는 **목록만**(무한 수열은 곱할 수 없다 — 범위 `[1..3]`로 유한하게) · 순서 = 왼쪽이 바깥 · 결과가 캐럿보다 적으면 순환 · 틀 없으면 그대로 이어 붙인다.
- 반복 `#k`는 항마다(곱집합의 각 목록에도).
- 캐럿이 하나뿐이면 첫 값만 넣는다(일반 입력과 같다) · 선택 구간이 있으면 그 구간을 값으로 **대체**.
- 오류(파싱 실패 · 빈 목록 · `x` 피연산자가 목록 아님 · 폭 > 64 · 반복 0)는 미리보기 행 첫 줄에 원인 · 본문엔 공백.

## 3. UI 흐름

1. `Ctrl+Alt+N` → 세션 시작 = 지금 구간 목록(`regions()` · 오름차순 · 빈 캐럿 포함)과 원문 저장 · `begin_group`.
2. 팔레트 프롬프트(id `seq`) 열림 · Placeholder = `editor.seq_default` · 안내 줄 = "Enter 넣기 · Esc 취소 · ? 도움말" · 즉시 기본 결과를 본문에 넣고 미리보기 행 표시.
3. 키를 칠 때마다(`query()` 변화) 파싱 → 값 n개 생성 → **`replace_many`로 직전 삽입분을 새 값으로 교체**(좌표 = 원 구간 + 앞선 길이 변화 누적) → 미리보기 행 갱신 · 오류면 공백으로 교체 + 오류 행.
4. `?`만 치면 미리보기 자리 = 도움말 행(§2 표) · 본문은 직전 결과 유지.
5. Enter = 확정 · 캐럿을 각 값의 **끝**에 두고(다중 캐럿 유지 · Sublime과 같다) `end_group` · 상태줄 "n곳에 넣음".
6. Esc · 바깥 클릭 · 탭 전환 = 원문으로 복원(같은 `replace_many`) · 구간·캐럿 복원 · `end_group` · 상태줄 "취소".
7. 자체 시험 = 기동 명령 `seq.open` · `seq.set:<식>`(입력 상자 글 = 식 · 미리보기 갱신) · `seq.commit` · `seq.cancel` · `seq.dump:<파일>`(식 · 오류 · 값 n개 · 본문) — 키 주입 없이 V1.

## 4. 구현 지도

| 조각 | 파일 | 내용 |
|---|---|---|
| 엔진 | `crates/nexa-sql/src/insseq.rs` | `parse(&str) -> Result<Seq, String>` · `Seq::values(n) -> Vec<String>` · `DEFAULT_EXPR` · `help_rows()` · 10진 정밀 소수 · 시각·날짜 산술 · 시험(표의 예 전부 + 오류) |
| 호스트 | `crates/nexa-sql/src/app/insseq.rs` | `SeqSession` · `insseq_open/update/commit/cancel/dump` |
| 팔레트 | `palette.rs` | 프롬프트 행(`set_prompt_rows` · 선택 불가 · Enter = Prompt) · `prompt_id()` · 빈 입력 Enter 허용(`open_prompt_live`) · 행 색(오류 = 경고색) |
| 명령 | `keymap.rs` · `app/menus.rs` | `edit.insert_sequence` · Ctrl+Alt+N / Cmd+Alt+N · Edit 메뉴(대소문자 변환 아래) · 팔레트 명령 등록 |
| 설정 | `nsql-settings` | `editor.seq_default`(기본 `1+1` · Editor 카테고리) |
| 문자열 | `nsql-i18n` | 메뉴 · 안내 · 오류 · 도움말 행(en/ko) |

부하원 = 없음(키 입력 때 n개 문자열 생성 + 한 번의 `replace_many` · 캐럿 수 상한 = 편집기 `regions_full`).

## 5. 결정

- **D-366(개발 기본값 · 사용자 요구에서 도출)**: 증가 구분자 = 연산자(`+ - * ^`) · 형식 접미 `~` · 반복 `#` · 곱집합 `x` · 틀 = 따옴표 + `{}` — 시각·날짜와 충돌 없는 한 벌.
- 역순 `!`(Sublime)은 두지 않는다(`10-1`로 충분) · 수식 `|x*2`(Sublime)도 1차 제외(필요 시 후속 · 틀 `{}`로 접두·접미는 된다).
- 난수 · UUID · 요일/월 이름 목록 상수(`[mon..sun]`)는 후속 후보(T-440 후보 원장에).

## 6. 구현 기록(10-27 · 143차 win)

- 엔진 `crates/nexa-sql/src/insseq.rs`(의존 0 · 순수 · 시험 8 = §2 표의 예 전부 + 오류·상한 + 날짜 왕복) · 호스트 `app/insseq.rs`(`SeqSession` · `insseq_open/update/apply/rows/commit/cancel/dump`).
- 팔레트 = **라이브 프롬프트**(`Palette::open_prompt_live(id, placeholder, hint, initial)` · 빈 입력 Enter 허용 · `set_prompt_rows((글, 경고색))` = 안내 줄 아래 미리보기 행 · 상자 폭 = 팔레트 전체 · `prompt_id()` · `query()` 공개) · 입력 라우팅 = `PaletteAction::None`마다 질의 비교 → `insseq_update` · `Close` → `insseq_cancel` · `Prompt{seq}` → `insseq_commit`.
- 편집기 = nexa-ui **239차** `TextBox::regions()`(시작 구간 읽기) + 기존 `set_regions_pub`(확정 = 각 값 끝 캐럿 · 취소 = 원 구간) · `begin_undo_group`/`end_undo_group`(되돌리기 한 단계) · `replace_many(edits, inv)`(한 번에 교체).
- 명령 `edit.insert_sequence`(Ctrl+Alt+N · mac Cmd+Alt+N · Edit 메뉴 대소문자 변환 아래 · 팔레트 "Edit: Insert Sequence…") · 설정 `editor.seq_default`(Text · 기본 `1+1`) · i18n 19(`MnInsertSequence` · `SeqHint` · `SeqMore` · `StSeqInserted/Cancelled` · `SeqErr*` 10 · 설정 라벨 2).
- 기동 명령(자체 시험 · 키 주입 0) = `editor.regions:<a[-b];…>`(구분 `;`) · `seq.open` · `seq.set:<식>` · `seq.commit` · `seq.cancel` · `seq.dump:<파일>`(`query=` `error=` `n=` `v:…` `text:`).
- 미리보기 행 = 캐럿 11개까지 `L줄: 값` + "…외 n개" · 오류 = 첫 행 경고색 · `?` = `HELP` 20행(언어 = `current_lang`).

## 7. 검토 — Insert Sequence를 **확장(wasm)** 으로 개발할 수 있게 SDK를 보완할 수 있는가(사용자 10-27 지시 · 결론 = **가능 · 권장 설계 = "프롬프트 변환" API 한 벌")

> ★ **결정(사용자 10-27 밤)** = 권장안 B **착수** · 내장 확장 기본 켜짐([D-368](10-decision-record.md)) · 설정 접근 = 런타임 overlay만([D-369](10-decision-record.md)) · 구현 = §8 · T-446.

### 7-1. 지금 못 하는 이유(사실)
| 필요한 것 | 지금 SDK(ABI v1.1) | 내장 구현이 쓰는 것 |
|---|---|---|
| 다중 캐럿/구간 읽기 · 값으로 교체 · 확정/취소 복원 · 되돌리기 한 단계 | 없음(편집기 API = 괄호 이동 6개 · `doc_path` 조회 · 파일 op 3) | nexa-ui 239차 `TextBox::regions()` · `replace_many` · `set_regions_pub` · `begin/end_undo_group` |
| 상단 입력창(라이브 프롬프트) · 키 입력마다 콜백 · 미리보기 행 · Enter/Esc | 없음(게스트는 명령 1회 호출만) | 팔레트 `open_prompt_live` · `set_prompt_rows` · `PaletteAction::None/Close/Prompt` 훅 |
| 호출 사이의 상태(원문 · 지금 들어간 글) | **호출마다 새 인스턴스**(`call_inner` = `instantiate_and_start`) → 게스트는 상태를 못 든다 | `App.seq: SeqSession` |
| 도움말 치트 시트 · 오류 문구 | 메타엔 없음 | `insseq::HELP` · `SeqErr` → i18n |

### 7-2. 세 가지 길
| 길 | 내용 | 평가 |
|---|---|---|
| A. 편집기 쓰기 API를 그대로 노출 | `nx_editor_regions` · `nx_editor_replace_many` · `nx_prompt_open/rows/close` + 게스트 이벤트 export(`nx_ext_event`) · 게스트가 세션 상태를 들어야 하므로 **인스턴스 유지 모드**(현재 "호출마다 새 인스턴스" 원칙 = docs/68 §3 · 능력 승인 D-90 전)가 필요 | 가장 범용이나 **능력 승인(D-90) 없이 본문 쓰기 권한을 주는 것** · 실행기 수명 모델 변경 · 위험·규모 큼 |
| **B. "프롬프트 변환" API(권장)** | 호스트가 세션(구간·원문·교체·복원·되돌리기·프롬프트 UI)을 전부 들고, 게스트는 **순수 함수**만 낸다: `nx_ext_transform(ctx) -> out` · ctx = `{"id","text":표현식,"n":구간 수,"regions":[{"line","text"}…(상한)],"doc_path"}` · out = `{"values":[…n개] 또는 "error":"…","rows":[미리보기 행],"help":[…]}` · 메타 선언 `prompts:[{id,label,placeholder,hint,key}]` → 호스트가 명령·팔레트·키맵에 등록하고 고르면 **지금 Insert Sequence와 같은 세션**을 연다 | 게스트에 쓰기 권한 0(값만 제안 · 적용은 호스트 = 87 §14 데이터 보호 정신과 같다) · 호출마다 새 인스턴스 그대로 · 호스트 코드 = 지금 `app/insseq.rs`를 "값 공급자가 게스트"로 일반화 · **1일 규모** |
| C. 내장 유지 + 패널 등재만 | `kind=builtin` 확장으로 등재(Rainbow Pairs 내장판과 같은 꼴) | 확장 패널엔 보이지만 "확장으로 개발"은 아님 |

### 7-3. 권장안 B 설계(ABI v1.2 · 요약)
- 메타: `"prompts":[{"id":"ext.insert_sequence.open","label":{…},"placeholder":"1+1","hint":{…},"key":"ctrl+alt+n","help":[{"expr","en","ko"}…]}]` — 호스트는 prompt id를 명령으로 등록(팔레트·키맵·메뉴 `target` 가능).
- 흐름: 명령 → 호스트 `prompt_open(ext, prompt)`: 구간·원문 저장 · `begin_undo_group` · 라이브 프롬프트(자리표시자·안내 = 메타) → 키마다 `nx_ext_transform(ctx)` → `values` 있으면 `replace_many`(지금 `insseq_apply`) · `error`면 공백 + 경고 행 · `rows` 있으면 그대로 미리보기(없으면 호스트가 `L줄: 값` 생성) · `?` = 메타 `help` → Enter = 확정(`set_regions_pub` 끝 캐럿 · `end_undo_group`) · Esc = 복원.
- 게스트 import 추가 없음(ctx로 다 준다) · export 하나 추가(`nx_ext_transform`) · 상한 = 구간 수 ≤ 편집기 다중 선택 상한 · ctx 크기 ≤ 1 MB(`OUT_CAP`) · 연료·시간 상한 그대로.
- 내장 ↔ 확장 공존 = Rainbow Pairs 방식: 앱에 `insert-sequence` **내장(builtin) 확장**을 두고(엔진 `insseq.rs` 그대로 · 확장 패널에 보임 · 기본 켜짐 여부는 사용자 결정) 같은 id의 wasm 샘플 `extensions/sdk/samples/insert-sequence`(엔진 크레이트를 path 의존 = nsql-format처럼 `nsql-seq`로 분리)를 설치하면 wasm이 앞선다.

### 7-4. 비용·순서(착수 결정 시)
1. 엔진 분리 `crates/nsql-seq`(의존 0 · insseq.rs 이동 · 시험 8 그대로) ½일.
2. SDK v1.2 = `Prompt` 메타 + `Transform{Request,Response}` + `export_extension!`에 `nx_ext_transform`(기본 = 없음) ½일.
3. 호스트 = `app/insseq.rs` → `app/prompt.rs` 일반화(값 공급자 = 내장 엔진 | 게스트) + 메타 `prompts` 파싱·명령 등록 + 내장 확장 등재 ½일.
4. 샘플 `insert-sequence` wasm + 패키지(index.json) + V1 ½일.
- 보류 = 길 A(쓰기 API)는 D-90 능력 승인·인스턴스 유지 모델 결정 뒤.

## 8. 구현 기록(10-27~28 · 148차 win · T-446)

- **엔진** = `crates/nsql-seq`(의존 0 · 앱 내장과 wasm 샘플이 같은 엔진 · 시험 8) — 문법(§2)은 그대로.
- **SDK ABI v1.2(SDK 0.3)** = 메타 `prompts[{id, label, placeholder, placeholder_key, hint, key, help}]` + export `nx_ext_transform` — 요청 `{id, text, n, regions[(line, text)], selection, phase(live·commit·cancel), lang, doc_path, settings}` → 응답 `{values | text | none, error, rows}`(`apply` = regions · text · none) · 한도 = 연료 2e8 · 500 ms · 요청 ≤ 1 MB · 구간 ≤ 256. 자세히 = [75 §3-2](75-extension-sdk-and-dynamic-loading.md).
- **호스트** = `app/insseq.rs`가 프롬프트 세션 일반화(`prompt_open(id)` · 값 공급자 = `Registry::transform` = 내장 엔진 또는 wasm) · 구간 저장 · 교체 · Enter 확정(commit 때 한 번 더 묻고 적용) · Esc 원문 복원 + cancel 알림 · 되돌리기 한 단계 — **게스트는 본문에 쓰지 않는다**(값만 낸다).
- **내장 확장** = `extensions/insert_sequence.rs`(id `insert-sequence` · 프롬프트 `edit.insert_sequence` · 기본 표현식 키 `editor.seq_default`) · D-368 = 설치 없이 켜짐 · 관리자 꺼짐이어도 동작 · 삭제 = 끄기 · 꺼진 채 Ctrl+Alt+N = 상태줄 안내.
- **wasm 패키지** = `extensions/insert-sequence` 0.1.0(설정 `ext.insert_sequence.default` · 팔레트 "Insert Sequence: Open (WASM)" · Ctrl+Alt+N) — 설치하면 내장을 대체 · 삭제하면 내장 복귀 · 패널·업데이트 목록은 "builtin → 0.1.0 (wasm)"로 교체를 안내.
- **함께 고친 결함** = 재기동 시 동적 확장 설정 값 유실(`Settings::adopt_unknown`) · `ext.<id>.use` 거울 미저장 · 상세 탭 종류 칸 · 팔레트 목록에 내장 누락 — [journal 10-27 §9](journal/2026-10-27.md).
- **V1** = 격리 홈 1·2·3차(내장 · 관리자 꺼짐 · 패널/상세 · wasm 교체·폴백 · 기본 표현식 · 회귀 3종) — journal §9.
- **보류** = 길 A(본문 쓰기 API · D-90 능력 승인 뒤) · 행 고르기 pick·팔레트 UI(T-447) · 상세 탭 "읽는/바꾸는 설정" 표시(T-448).

### 8-1. 149차(10-28) — 단축키·설정값을 확장 영역으로 · 곱집합 `*`

- 정적 키맵·레지스트리·팔레트에서 Insert Sequence를 빼고 **내장 확장이 선언**(명령 `edit.insert_sequence` · Ctrl+Alt+N / Cmd+Alt+N · 설정 `ext.insert_sequence.default`) · 옛 키 `editor.seq_default`는 `RENAMED`로 새 키로 이주.
- 단축키 = 설정 `key.edit.insert_sequence`(환경 설정 ▸ 확장 ▸ Insert Sequence · 비우면 확장 기본 · `none` = 없음) — 모든 확장 명령 공통 규칙([94 §6-4](94-settings-key-naming-and-location.md)).
- 곱집합 `*` 별칭(§2) · wasm 패키지 0.1.1 · 편집 메뉴 단축키 표시 결함 수정 — [journal 10-27 §10](journal/2026-10-27.md).
- 153차(10-28 · T-447) = 프롬프트 응답 행을 고를 수 있게(`Row.id` · ↑↓ · Enter의 `pick`) — Insert Sequence 내장은 행 선택을 쓰지 않는다(dump `pick=` 빈 값 · 동작 변화 없음) · 예 = hello-ext 0.3.0 "Hello: Pick a greeting".
