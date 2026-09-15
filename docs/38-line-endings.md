# 38 · 파일 줄끝(CRLF/LF) 정책 — 통합 로직 + OS 기본값 (사용자 요청 09-16)

> **요구**(사용자 09-16): 파일 열기·저장·다른 이름으로 저장에서 Windows·macOS·Linux의 CRLF/LF를 어떻게 다룰지 검토하고 통합 또는 OS별 분리 로직을 설계·구현. 설정이 필요하면 DBeaver 기준으로 분류.
> **결론**: **통합 로직**(편집기 내부 = `\n` 하나 · 변환은 파일 경계에서만) + **OS는 새 파일의 기본값에만 관여**. 구현 = `crates/nexa-sql/src/eol.rs` · 설정 2키 · 상태줄 `LF`/`CRLF` 세그먼트.

## 1. 경쟁 제품 검토

| 제품 | 새 파일 | 열린 파일 | 변환 | 설정 위치 |
|---|---|---|---|---|
| DBeaver(Eclipse 텍스트 편집기) | General ▸ Workspace **"New text file line delimiter"** = Default(OS) / Unix / Windows | 파일의 줄끝 **유지** | Edit ▸ Convert Line Delimiters To ▸ Windows/Unix(문서 단위) | General ▸ Workspace(전역) · 프로젝트별 재정의 |
| VS Code | `files.eol` = auto(OS)/`\n`/`\r\n` | 유지(혼합은 첫 줄 기준) | 상태줄 **LF/CRLF 클릭** → 문서 변환 | Files |
| Sublime Text | `default_line_ending` = system/unix/windows | 유지 | View ▸ Line Endings · 상태줄 | 설정 파일 |
| git | `core.autocrlf` — 저장소 경계 변환 | — | — | — |

공통점: **내부 표현은 한 종류**, 파일을 열면 그 파일의 줄끝을 기억해 저장 때 되돌리고, 새 파일만 OS/설정 기본을 따른다. OS별로 로직을 갈라 놓는 제품은 없다 — 같은 파일을 세 OS에서 번갈아 열어도 줄끝이 바뀌지 않아야 하기 때문이다.

## 2. 결정 — 통합 로직 · OS는 기본값만

1. **내부 = `\n`**: 편집기 버퍼·검색·복사·구문 강조·세션 저장은 줄끝을 모른다(3-OS 동일 코드 · nexa-ctl `EditState`도 `\n`).
2. **열기**: `eol::detect` — CRLF 수 ≥ 단독 LF + 단독 CR이면 CRLF(동률 = CRLF · Windows 파일에 LF 몇 줄이 섞인 흔한 경우) · 옛 Mac CR은 LF로 정규화. 탭이 줄끝(`crlf: Vec<bool>`)을 기억한다.
3. **새 탭**: `file.eol_new` = **auto**(OS: Windows CRLF · 그 외 LF · DBeaver "Default") | lf | crlf.
4. **저장/다른 이름으로**: `file.eol_save` = **keep**(탭 줄끝 · 기본 · DBeaver/VS Code와 같음) | lf | crlf | os. 원자적 쓰기(`.nsql-tmp` → rename)는 그대로.
5. **변환 UI**: 상태줄 `LF`/`CRLF` 세그먼트(공백/탭 세그먼트 왼쪽) 클릭 → 팝업(✓ 현재) → 탭 줄끝 변경 = **더러움**(`saved_crlf`와 비교) → 저장 때 반영. 메뉴 항목(Edit ▸ Convert Line Delimiters)은 T-61 상태줄 정리와 함께.
6. **혼합 파일**: 다수결로 한 종류로 저장(Eclipse는 혼합을 유지하지만 SQL 스크립트에서 혼합은 사고에 가깝다 · 사용자가 원하면 `keep`이라도 열 때 판정된 한 종류).

## 3. 설정(DBeaver 분류 대응)

| 키 | DBeaver 대응 | 값 | 기본 |
|---|---|---|---|
| `file.eol_new` | General ▸ Workspace ▸ New text file line delimiter | auto / lf / crlf | auto |
| `file.eol_save` | (Eclipse 없음 · VS Code `files.eol`은 새 파일만) | keep / lf / crlf / os | keep |

카테고리 = **Files**(DBeaver의 General ▸ Workspace는 워크스페이스 전역 항목이라 파일 항목 묶음에 두는 것이 우리 설정 창 구조(24)에 맞다). 인코딩(T-79)과 같은 묶음.

## 4. 구현 지도

- `eol.rs`: `detect` · `apply` · `default_crlf` · `save_crlf` + 테스트(다수결·동률·CR·정책).
- `editors.rs`: `crlf`/`saved_crlf`/`default_crlf` · `set_default_crlf` · `set_active_crlf` · `is_dirty`에 줄끝 비교.
- `main.rs`: 열기 `eol::detect` · 저장 `eol::save_crlf`+`apply` · 상태줄 세그먼트 `status_eol_rect` + `open_eol_menu` · 설정 즉시 반영.
- 잔여: Edit 메뉴 "Convert Line Delimiters" · 다른 이름으로 저장 대화상자 줄끝 콤보(T-79와 묶음).
