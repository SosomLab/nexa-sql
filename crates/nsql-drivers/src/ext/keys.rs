//! 확장 드라이버 배포 **공개키**(검증 전용 · docs/22 §6). 비밀키는 어느 저장소에도 없다 — 릴리스 파이프라인 시크릿(nexa-sql 저장소 · 사용자 10-22 = 별도 저장소 없음)에만.
//! ★ 생성 = `nexa-license-tool keygen`(봉투 + `.pub`) → `nexa-license-tool keys-rs <a.pub> …` 꼴을 여기에 붙인다 — 손으로 고치지 말 것.
//! 회전 = 새 키를 다음 자리에 더해 두 키를 동시에 검증하다가, 옛 키로 서명한 자산이 사라진 뒤 뺀다.
//! **비어 있으면 네트워크·파일 설치가 모두 거부된다** — 서명 없이 설치되는 길은 없다(우회 옵션 없음).
//! D-348(사용자 10-23): `drv-v1` = `C:\Users\<사용자>\nexa-issuer\drv-v1.key.pub`(base32 앞 8자 `8CFVCNTJ`).
pub use nexa_license::keys::RootKey;
use nexa_license::types::Alg;

/// 현재 유효한 드라이버 배포 공개키 목록.
#[rustfmt::skip]
pub const DRIVER_KEYS: &[RootKey] = &[
    RootKey {
        id: "drv-v1",
        alg: Alg::Ed25519,
        public: &[
            0x43, 0x1f, 0xb6, 0x57, 0x52, 0x2b, 0xc5, 0xe5,
            0x28, 0x2b, 0x5c, 0x9e, 0xe2, 0x14, 0x24, 0x5a,
            0x06, 0x56, 0x15, 0x07, 0x4e, 0x67, 0x7e, 0x4e,
            0xe3, 0xfc, 0x53, 0x18, 0x04, 0xee, 0x54, 0x16,
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 박힌 공개키가 `.pub`(base32)와 같은 바이트인지 — 손으로 고친 흔적을 잡는다.
    #[test]
    fn embedded_key_matches_pub_file_text() {
        let b32 =
            nexa_license::base32::decode("8CFVCNTJ5F2YAA1BBJFE4514B835C5879SKQWKQ3ZH9HG17EAGB0")
                .expect("base32");
        assert_eq!(DRIVER_KEYS.len(), 1);
        assert_eq!(DRIVER_KEYS[0].id, "drv-v1");
        assert_eq!(DRIVER_KEYS[0].public, b32.as_slice());
    }
}
