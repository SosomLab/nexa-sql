//! 기기 키(32B 무작위) — 프로필 비밀번호 봉투를 여는 유일한 비밀. `<dir>/device.key`.
//!
//! | OS | 보관 형식 | 보호 수준 |
//! |---|---|---|
//! | Windows | `"NSDK"` ‖ ver(1) ‖ DPAPI(CryptProtectData · 사용자 범위 · 앱 엔트로피) 블롭 | **같은 Windows 계정**만 푼다 — 폴더째 복사·다른 계정·다른 PC에서는 열리지 않는다 |
//! | macOS · Linux | 평문 32B · 파일 모드 0600 | 폴더째 복사에는 못 버틴다(nexa-clip v1과 같은 정직한 한계 — Keychain·Secret Service 결합은 후속) |
//!
//! 키 계층은 nexa-clip `nclip-store/src/keys.rs`를 따른다(마스터를 직접 파생하지 않고 파일로
//! 두어 보호 수단을 바꿀 때 프로필 재암호화가 필요 없게).

use std::io;
use std::path::Path;

const MAGIC: [u8; 4] = *b"NSDK";
const VER: u8 = 1;

pub(crate) fn random32() -> io::Result<[u8; 32]> {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).map_err(|_| io::Error::other("OS 난수 실패"))?;
    Ok(k)
}

/// 디스크 표현 → 32B. 평문 32B(이식 · unix 형식)와 DPAPI 봉투(`NSDK`) 둘 다 받는다.
fn decode(bytes: &[u8], path: &Path) -> io::Result<[u8; 32]> {
    if bytes.len() == 32 {
        let mut k = [0u8; 32];
        k.copy_from_slice(bytes);
        return Ok(k);
    }
    if bytes.len() > 5 && bytes[..4] == MAGIC && bytes[4] == VER {
        #[cfg(windows)]
        {
            let plain = dpapi::unprotect(&bytes[5..]).map_err(|e| {
                io::Error::other(format!(
                    "{}: 기기 키를 풀 수 없습니다(다른 Windows 계정·PC에서 만든 키) — {e}",
                    path.display()
                ))
            })?;
            if plain.len() == 32 {
                let mut k = [0u8; 32];
                k.copy_from_slice(&plain);
                return Ok(k);
            }
        }
        #[cfg(not(windows))]
        {
            return Err(io::Error::other(format!(
                "{}: Windows DPAPI로 보호된 기기 키는 이 OS에서 열 수 없습니다",
                path.display()
            )));
        }
    }
    // 길이·형식이 틀린 파일은 손상 — 덮지 않고 실패한다(fail-closed · 보관 정책은 호출측).
    Err(io::Error::other(format!(
        "{} 손상(길이 {})",
        path.display(),
        bytes.len()
    )))
}

/// 32B → 디스크 표현. Windows는 DPAPI 봉투, 그 외는 평문.
fn encode(k: &[u8; 32]) -> io::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        let blob = dpapi::protect(k)?;
        let mut out = Vec::with_capacity(5 + blob.len());
        out.extend_from_slice(&MAGIC);
        out.push(VER);
        out.extend_from_slice(&blob);
        Ok(out)
    }
    #[cfg(not(windows))]
    {
        Ok(k.to_vec())
    }
}

/// 파일이 있으면 읽고, 없으면 32B 무작위를 만들어 쓴다.
///
/// ★ **여러 인스턴스 동시 첫 실행**(GUI 창 둘 · CLI 병렬)에도 기기 키는 **하나**여야 한다 —
/// 둘이 각자 키를 만들면 한쪽이 봉인한 비밀번호를 다른 쪽이 못 연다. 그래서 생성은
/// `create_new`(원자적 · 한 프로세스만 성공)로 하고, 진 쪽은 이긴 쪽이 쓴 파일을 읽는다.
/// 파일은 있는데 아직 비어 있으면(이긴 쪽이 쓰는 찰나) 잠깐 기다렸다 다시 읽는다.
pub(crate) fn load_or_create(path: &Path) -> io::Result<[u8; 32]> {
    for _ in 0..200 {
        match std::fs::read(path) {
            Ok(b) if b.is_empty() => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Ok(b) => return decode(&b, path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let k = random32()?;
        let bytes = encode(&k)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // ★ 생성 시점부터 0600(unix) — 쓰고 나서 chmod 하면 그 사이 umask 모드로 잠깐 열린다.
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        match opts.open(path) {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(&bytes)?;
                f.sync_all()?;
                return Ok(k);
            }
            // 다른 인스턴스가 먼저 만들었다 — 그쪽 키를 읽는다.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!(
        "{}: 기기 키 생성 대기 초과(다른 인스턴스가 쓰는 중)",
        path.display()
    )))
}

/// Windows DPAPI — crypt32 직접 바인딩(외부 crate 0 · 계열 `nclip-plat` FFI 관례).
#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;
    use std::io;

    #[repr(C)]
    struct DataBlob {
        cb: u32,
        pb: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            descr: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            descr: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h: *mut c_void) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    /// 자격 증명 UI 금지(무인 CLI·워커 스레드에서도 프롬프트가 뜨지 않게).
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
    /// 앱 엔트로피 — 같은 계정의 다른 프로그램이 이 블롭을 그냥 풀지 못하게 한 겹 더.
    const ENTROPY: &[u8] = b"nexa-sql/vault-v1";

    fn blob(b: &[u8]) -> DataBlob {
        DataBlob {
            cb: b.len() as u32,
            pb: b.as_ptr().cast_mut(),
        }
    }

    /// 출력 블롭을 Vec으로 복사하고 LocalFree.
    unsafe fn take(out: DataBlob) -> Vec<u8> {
        let v = std::slice::from_raw_parts(out.pb, out.cb as usize).to_vec();
        LocalFree(out.pb.cast());
        v
    }

    pub(super) fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            cb: 0,
            pb: std::ptr::null_mut(),
        };
        // SAFETY: 모든 포인터는 이 스코프 안의 유효한 버퍼를 가리키고, 출력은 성공 시에만 읽는다.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(io::Error::other(format!("CryptProtectData 실패({code})")));
        }
        Ok(unsafe { take(out) })
    }

    pub(super) fn unprotect(sealed: &[u8]) -> io::Result<Vec<u8>> {
        let input = blob(sealed);
        let entropy = blob(ENTROPY);
        let mut out = DataBlob {
            cb: 0,
            pb: std::ptr::null_mut(),
        };
        // SAFETY: protect와 동일.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return Err(io::Error::other(format!("CryptUnprotectData 실패({code})")));
        }
        Ok(unsafe { take(out) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("nsql-vault-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn create_then_reload_same_key() {
        let d = tmp("devkey");
        let p = d.join("device.key");
        let a = load_or_create(&p).unwrap();
        let b = load_or_create(&p).unwrap();
        assert_eq!(a, b, "두 번째 호출은 같은 키를 읽는다");
        let on_disk = std::fs::read(&p).unwrap();
        if cfg!(windows) {
            assert_eq!(&on_disk[..4], b"NSDK", "Windows는 DPAPI 봉투");
            assert!(on_disk.len() > 32);
            assert!(
                !on_disk.windows(32).any(|w| w == a),
                "평문 키가 디스크에 없다"
            );
        } else {
            assert_eq!(on_disk.len(), 32);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_file_fails_closed() {
        let d = tmp("devkey-corrupt");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("device.key");
        std::fs::write(&p, b"short").unwrap();
        assert!(
            load_or_create(&p).is_err(),
            "손상 파일은 덮어쓰지 않고 실패"
        );
        assert_eq!(std::fs::read(&p).unwrap(), b"short");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 여러 인스턴스가 동시에 첫 실행해도 기기 키는 하나(사용자 요청 09-13 — 어느 인스턴스든 같은 비밀번호).
    #[test]
    fn concurrent_first_run_yields_single_key() {
        let d = tmp("devkey-race");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("device.key");
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let p = p.clone();
                std::thread::spawn(move || load_or_create(&p).unwrap())
            })
            .collect();
        let keys: Vec<[u8; 32]> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(keys.iter().all(|k| *k == keys[0]), "8 스레드 전부 같은 키");
        assert_eq!(load_or_create(&p).unwrap(), keys[0]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 평문 32B(unix 형식 · 다른 OS에서 복사)는 어느 OS에서나 읽힌다.
    #[test]
    fn plain32_is_accepted_everywhere() {
        let d = tmp("devkey-plain");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("device.key");
        std::fs::write(&p, [9u8; 32]).unwrap();
        assert_eq!(load_or_create(&p).unwrap(), [9u8; 32]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
