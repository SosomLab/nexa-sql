//! 확장 드라이버 배포 **공개키**(검증 전용 · docs/22 §6). 비밀키는 어느 저장소에도 없다 — 릴리스 파이프라인 시크릿(nexa-sql 저장소 · 사용자 10-22 = 별도 저장소 없음)에만.
//! ★ 생성 = `nexa-license-tool keygen`(봉투 + `.pub`) → `nexa-license-tool keys-rs <a.pub> …` 꼴을 여기에 붙인다 — 손으로 고치지 말 것.
//! 회전 = 새 키를 다음 자리에 더해 두 키를 동시에 검증하다가, 옛 키로 서명한 자산이 사라진 뒤 뺀다.
//! **비어 있으면 네트워크·파일 설치가 모두 거부된다** — 서명 없이 설치되는 길은 없다(우회 옵션 없음).
pub use nexa_license::keys::RootKey;

/// 현재 유효한 드라이버 배포 공개키 목록(D-348 사용자 결정 전 = 비어 있음).
#[rustfmt::skip]
pub const DRIVER_KEYS: &[RootKey] = &[
];
