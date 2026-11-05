//! ★ 확장 패키지 코어(T-166 E8 · 11-05): GUI 확장 매니저(`nexa-sql/src/extensions/manager.rs`)와 CLI `nsql ext`가 **같은 코드**로
//! 저장소 메타(`index.json` · `extension.json`)를 읽고, 파일 체크섬을 검증·갱신하고, 색인 서명(D-373)을 검증한다.
//!
//! - [`meta`] — 메타 형식 v1 파싱(종전 manager.rs에서 이관 · 형식 = `extensions/README.md`)
//! - [`sha256`] — SHA-256(외부 crate 0 · DR-3)
//! - [`check`] — 패키지 폴더 검사(메타 · 파일 존재 · sha256 · wasm 매직 · id/버전/플랫폼/`min_app` 꼴 · 색인 일치)
//! - [`pack`] — 패키지 폴더 다듬기(`files[].sha256` 재계산 · 색인 행 갱신/추가 · 원문 서식 보존)
//! - [`sig`] — 색인 서명 사이드카(`index.sig` = `ed25519 <key-id> <hex128>` · nexa-license-tool `sign-file`과 같은 꼴) **검증**
//! - [`net`] — 원격 저장소 읽기(curl · 매니저와 같은 길)
//!
//! 의존 = nsql-json · nexa-license(`ed25519` 검증만). 서명 **생성**은 여기 없다(비밀키 = 발급 PC · nexa-license 규칙).

pub mod check;
pub mod meta;
pub mod net;
pub mod pack;
pub mod sha256;
pub mod sig;

pub use meta::{parse_index, parse_meta, FileSpec, Index, Kind, Meta, Summary, FORMAT};
