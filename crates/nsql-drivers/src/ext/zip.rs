//! 최소 ZIP 읽기(설치 전용 · T-29): 중앙 디렉터리 → 항목 → 저장(0)/deflate(8) · CRC-32 검사 · 경로 안전(절대·`..`·드라이브 거부) ·
//! Unix 실행 비트 보존(외부 속성 상위 16비트) · **zip64·암호 = 거부**. deflate 해제 = `miniz_oxide`(순수 Rust · 이미 트리 안 · D-348).
//! 시험·패키징용 작성기 [`build`]도 둔다(저장/deflate · 한 파일 하나씩 · 중앙 디렉터리 + EOCD).
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;

/// 항목 하나(중앙 디렉터리 기준).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// `/` 구분 · 디렉터리는 `/`로 끝난다.
    pub name: String,
    pub method: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    /// 로컬 헤더 오프셋.
    pub offset: u64,
    /// Unix 모드(만든 OS가 Unix고 값이 있을 때).
    pub unix_mode: Option<u32>,
}

impl Entry {
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// 풀어 놓은 결과.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    pub files: usize,
    pub bytes: u64,
    /// 공통 최상위 폴더를 벗겼으면 그 이름.
    pub stripped: Option<String>,
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        *b.get(i + 3)?,
    ]))
}

/// 중앙 디렉터리를 읽어 항목 목록으로.
pub fn entries(data: &[u8]) -> Result<Vec<Entry>, String> {
    if data.len() < 22 {
        return Err("not a zip (too short)".into());
    }
    let min = data.len().saturating_sub(22 + 65_535);
    let mut eocd = None;
    let mut i = data.len() - 22;
    loop {
        if u32_at(data, i) == Some(SIG_EOCD) {
            eocd = Some(i);
            break;
        }
        if i == min {
            break;
        }
        i -= 1;
    }
    let e = eocd.ok_or("not a zip (no end-of-central-directory)")?;
    let disk = u16_at(data, e + 4).unwrap_or(0);
    let total = u16_at(data, e + 10).ok_or("eocd")? as usize;
    let cd_size = u32_at(data, e + 12).ok_or("eocd")? as usize;
    let cd_off = u32_at(data, e + 16).ok_or("eocd")? as usize;
    if disk != 0 || total == 0xFFFF || cd_off == 0xFFFF_FFFF || cd_size == 0xFFFF_FFFF {
        return Err("zip64 or multi-disk archives are not supported".into());
    }
    if cd_off + cd_size > data.len() {
        return Err("central directory out of range".into());
    }
    let mut out = Vec::with_capacity(total);
    let mut p = cd_off;
    for _ in 0..total {
        if u32_at(data, p) != Some(SIG_CENTRAL) {
            return Err("bad central directory entry".into());
        }
        let made_by = u16_at(data, p + 4).ok_or("cd")?;
        let flags = u16_at(data, p + 8).ok_or("cd")?;
        let method = u16_at(data, p + 10).ok_or("cd")?;
        let crc = u32_at(data, p + 16).ok_or("cd")?;
        let compressed = u32_at(data, p + 20).ok_or("cd")? as u64;
        let size = u32_at(data, p + 24).ok_or("cd")? as u64;
        let n = u16_at(data, p + 28).ok_or("cd")? as usize;
        let x = u16_at(data, p + 30).ok_or("cd")? as usize;
        let c = u16_at(data, p + 32).ok_or("cd")? as usize;
        let ext_attr = u32_at(data, p + 38).ok_or("cd")?;
        let offset = u32_at(data, p + 42).ok_or("cd")? as u64;
        let name = data
            .get(p + 46..p + 46 + n)
            .ok_or("cd name")
            .map(|b| String::from_utf8_lossy(b).replace('\\', "/"))?;
        if flags & 1 != 0 {
            return Err(format!("encrypted entry: {name}"));
        }
        if compressed == 0xFFFF_FFFF || size == 0xFFFF_FFFF || offset == 0xFFFF_FFFF {
            return Err(format!("zip64 entry: {name}"));
        }
        let unix_mode = ((made_by >> 8) == 3 && ext_attr >> 16 != 0).then_some(ext_attr >> 16);
        out.push(Entry {
            name,
            method,
            crc,
            compressed,
            size,
            offset,
            unix_mode,
        });
        p += 46 + n + x + c;
    }
    Ok(out)
}

/// 항목의 압축 데이터 조각(로컬 헤더를 지나서).
fn raw<'a>(data: &'a [u8], e: &Entry) -> Result<&'a [u8], String> {
    let o = e.offset as usize;
    if u32_at(data, o) != Some(SIG_LOCAL) {
        return Err(format!("bad local header: {}", e.name));
    }
    let n = u16_at(data, o + 26).ok_or("lh")? as usize;
    let x = u16_at(data, o + 28).ok_or("lh")? as usize;
    let start = o + 30 + n + x;
    data.get(start..start + e.compressed as usize)
        .ok_or_else(|| format!("entry data out of range: {}", e.name))
}

/// 항목 하나를 푼 바이트(CRC 검사 포함).
pub fn read_entry(data: &[u8], e: &Entry) -> Result<Vec<u8>, String> {
    let r = raw(data, e)?;
    let out = match e.method {
        0 => r.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(r, e.size as usize)
            .map_err(|err| format!("inflate {}: {err:?}", e.name))?,
        m => return Err(format!("unsupported compression method {m}: {}", e.name)),
    };
    if out.len() as u64 != e.size {
        return Err(format!("size mismatch: {}", e.name));
    }
    if crc32(&out) != e.crc {
        return Err(format!("CRC mismatch: {}", e.name));
    }
    Ok(out)
}

/// 안전한 상대 경로로(거부 = None): 절대 · 드라이브 · `..` · 빈 이름 · 제어 문자.
fn safe_relative(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.chars().any(|c| c.is_control()) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in name.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return None;
        }
        let p = Path::new(part);
        if p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        return None;
    }
    Some(out)
}

/// 모든 항목이 같은 최상위 폴더 아래면 그 이름.
#[must_use]
pub fn common_root(list: &[Entry]) -> Option<String> {
    let mut root: Option<&str> = None;
    for e in list {
        let first = e.name.split('/').next().unwrap_or("");
        let has_more = e.name[first.len()..].starts_with('/');
        if !has_more || first.is_empty() {
            return None;
        }
        match root {
            None => root = Some(first),
            Some(r) if r == first => {}
            Some(_) => return None,
        }
    }
    root.filter(|r| safe_relative(r).is_some())
        .map(str::to_string)
}

/// `dest` 아래에 전부 풀어 놓는다(`strip_root` = 공통 최상위 폴더 하나는 벗긴다).
pub fn extract(data: &[u8], dest: &Path, strip_root: bool) -> Result<Extracted, String> {
    let list = entries(data)?;
    let stripped = if strip_root { common_root(&list) } else { None };
    let mut out = Extracted {
        stripped: stripped.clone(),
        ..Extracted::default()
    };
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for e in &list {
        let name = match &stripped {
            Some(r) => &e.name[r.len() + 1..],
            None => e.name.as_str(),
        };
        if name.is_empty() {
            continue;
        }
        let rel = safe_relative(name).ok_or_else(|| format!("unsafe path in zip: {}", e.name))?;
        let target = dest.join(&rel);
        if e.is_dir() {
            std::fs::create_dir_all(&target).map_err(|er| format!("{}: {er}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|er| format!("{}: {er}", parent.display()))?;
        }
        let bytes = read_entry(data, e)?;
        let mut f =
            std::fs::File::create(&target).map_err(|er| format!("{}: {er}", target.display()))?;
        f.write_all(&bytes)
            .map_err(|er| format!("{}: {er}", target.display()))?;
        drop(f);
        #[cfg(unix)]
        if let Some(mode) = e.unix_mode {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode & 0o777));
        }
        out.files += 1;
        out.bytes += bytes.len() as u64;
    }
    Ok(out)
}

/// CRC-32(IEEE · zip).
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// 작성기(시험·패키징): `(이름, 바이트, unix_mode)` 목록 → zip 바이트. `deflate` = 8번 방식(아니면 저장).
#[must_use]
pub fn build(files: &[(&str, &[u8], Option<u32>)], deflate: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, mode) in files {
        let (method, body): (u16, Vec<u8>) = if deflate && !data.is_empty() {
            (8, miniz_oxide::deflate::compress_to_vec(data, 6))
        } else {
            (0, data.to_vec())
        };
        let crc = crc32(data);
        let offset = out.len() as u32;
        let n = name.as_bytes();
        // 로컬 헤더.
        out.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // time/date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(n);
        out.extend_from_slice(&body);
        // 중앙 디렉터리 항목.
        let made_by: u16 = if mode.is_some() { 3 << 8 | 20 } else { 20 };
        central.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
        central.extend_from_slice(&made_by.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attr
        central.extend_from_slice(&(mode.map_or(0, |m| m << 16)).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(n);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&SIG_EOCD.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn build_extract_round_trip_stored_and_deflate() {
        let big: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        for deflate in [false, true] {
            let z = build(
                &[
                    ("pkg/", b"", None),
                    ("pkg/nexa-driver.json", b"{\"id\":\"x\"}", None),
                    ("pkg/lib/big.bin", &big, Some(0o755)),
                ],
                deflate,
            );
            let list = entries(&z).expect("entries");
            assert_eq!(list.len(), 3);
            assert_eq!(common_root(&list).as_deref(), Some("pkg"));
            let dest =
                std::env::temp_dir().join(format!("nsql-zip-{}-{}", std::process::id(), deflate));
            let _ = std::fs::remove_dir_all(&dest);
            let r = extract(&z, &dest, true).expect("extract");
            assert_eq!((r.files, r.bytes), (2, 10 + big.len() as u64));
            assert_eq!(r.stripped.as_deref(), Some("pkg"));
            assert_eq!(
                std::fs::read(dest.join("lib").join("big.bin")).expect("read"),
                big
            );
            assert_eq!(
                std::fs::read_to_string(dest.join("nexa-driver.json")).expect("read"),
                "{\"id\":\"x\"}"
            );
            let _ = std::fs::remove_dir_all(&dest);
        }
    }

    #[test]
    fn rejects_unsafe_paths_and_bad_crc() {
        let z = build(&[("../evil.txt", b"x", None)], false);
        let dest = std::env::temp_dir().join(format!("nsql-zip-evil-{}", std::process::id()));
        let err = extract(&z, &dest, true).expect_err("must reject");
        assert!(err.contains("unsafe path"), "{err}");
        let z = build(&[("C:/abs.txt", b"x", None)], false);
        assert!(extract(&z, &dest, true).is_err());
        let _ = std::fs::remove_dir_all(&dest);
        // CRC 훼손.
        let mut z = build(&[("a.txt", b"hello", None)], false);
        let i = z.windows(5).position(|w| w == b"hello").expect("data");
        z[i] = b'j';
        let list = entries(&z).expect("entries");
        assert!(read_entry(&z, &list[0]).expect_err("crc").contains("CRC"));
        assert!(entries(b"not a zip at all, really not").is_err());
        assert_eq!(common_root(&[]), None);
    }
}
