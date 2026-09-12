# 13 · 라이선스 — PolyForm Noncommercial 1.0.0 (계열 동일)

> 사용자 요청(09-12): *"nexa-clip 등과 동일하게 영리 목적인 경우에만 라이선스 구매가 필요한 라이선스."* 정책 배경·대안 비교의 원본은 `../nexa-dir/docs/13-licensing.md`(DR-5 · 2026-06-30), 집행(정품 인증) 설계는 `../nexa-dir/docs/17-licensing-activation.md`.

## 1. 적용

- [LICENSE.md](../LICENSE.md)(영문 정본) · [LICENSE.ko.md](../LICENSE.ko.md)(비공식 한글, 영문 우선) — `Required Notice: Copyright (c) 2026 SosomLab — Nexa SQL`.
- `Cargo.toml` `license = "LicenseRef-PolyForm-Noncommercial-1.0.0"` (전 크레이트 상속).
- 개인·비상업·교육·공공 = 무료. **상업적 사용 = 별도 유료 라이선스**(문의 kiros33@sosomlab.com).
- `nexa-ui`(공용 라이브러리)도 동일. ⚠️ 향후 **플러그인 SDK(패키지 API 타입·예제)는 MIT로 분리** 권장(nexa-dir DR-6 선례 — 서드파티 패키지 생태계 위축 방지).

## 2. 의존성 정책 (유료 판매 보호)

- 허용: MIT · Apache-2.0 · BSD · ISC · MPL-2.0(파일 단위) · UPL. **GPL/AGPL 금지**(본체 링크). LGPL은 아웃오브프로세스만.
- DB 클라이언트 라이브러리: Oracle Instant Client(OTN 재배포 조건 — [06 Part C](06-rust-ecosystem.md)) · MS ODBC 18(재배포 조건) → **순수 Rust 드라이버 우선**으로 회피. 동봉 시 EULA 동봉·D-1.
- 참고한 오픈소스 에디터: Helix(MPL) · Zed(GPL/AGPL)는 **설계만** 참고, 코드 차용은 MIT/Apache(Lapce·cosmic-text·syntect)만([09 §1](09-editor-and-packages.md)).
- CI 게이트: `cargo-deny`(라이선스 화이트리스트) — M1에 추가. `THIRD-PARTY-NOTICES` 생성.

## 3. 집행 (D-6)

nexa-dir 17 설계 재사용: **Ed25519 서명 토큰(PASETO v4.public) · 공개키만 내장 · 오프라인 1차 · Cloudflare Workers+D1 온라인 2차**. `nsql-license` 크레이트(계열 공용 후보). 비밀키는 저장소·앱에 절대 미포함. 무료 기능 제한은 두지 않고(라이선스에 내재) 상업 사용자에게 "라이선스 등록" 화면만 제공하는 방향 권장 — TablePlus/DbVisualizer식 영구 라이선스 + 업데이트 기간이 개인 개발자 수용도가 높다([03 §7](03-competitive-landscape.md)).
