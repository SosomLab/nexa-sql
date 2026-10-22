//! ★ T-29 설치 왕복(네트워크 없이): 로컬 평문 HTTP 서버가 색인·zip·sha256·sig를 내고, 시험 키로 서명한 패키지를
//! `install()`이 받아 검증·풀기·원자적 설치 → `list_extensions_in`에 보인다. 훼손된 서명·해시·ABI는 거부.
use ed25519_dalek::{Signer, SigningKey};
use nexa_license::keys::RootKey;
use nexa_license::types::Alg;
use nsql_drivers::ext::install::{self, Source};
use nsql_drivers::ext::sha256::{hex, sha256};
use nsql_drivers::ext::{library_file_name_for, zip};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SECRET: [u8; 32] = [7u8; 32];
static PUBLIC: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

fn keys() -> Vec<RootKey> {
    let pk = PUBLIC.get_or_init(|| SigningKey::from_bytes(&SECRET).verifying_key().to_bytes());
    vec![RootKey {
        id: "drv-test",
        alg: Alg::Ed25519,
        public: Box::leak(Box::new(*pk)),
    }]
}

fn sign(data: &[u8]) -> String {
    let sig = SigningKey::from_bytes(&SECRET).sign(data);
    format!("ed25519 drv-test {}\n", hex(&sig.to_bytes()))
}

fn package(id: &str, ver: &str, protocol: u32) -> Vec<u8> {
    let lib = library_file_name_for(id);
    let manifest =
        format!(r#"{{"id":"{id}","version":"{ver}","dialect":"{id}","protocol":{protocol}}}"#);
    zip::build(
        &[
            (
                &format!("{id}-{ver}/nexa-driver.json"),
                manifest.as_bytes(),
                None,
            ),
            (
                &format!("{id}-{ver}/{lib}"),
                b"not really a dll",
                Some(0o755),
            ),
            (&format!("{id}-{ver}/LICENSE"), b"PolyForm NC", None),
        ],
        true,
    )
}

type Routes = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

fn serve(routes: Routes) -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = l.local_addr().expect("addr");
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            let mut req = Vec::new();
            let mut b = [0u8; 2048];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut b).unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&b[..n]);
            }
            let line = String::from_utf8_lossy(&req).into_owned();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let body = routes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, b)| b.clone());
            let resp = match body {
                Some(b) => {
                    let mut r = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"t1\"\r\n\r\n",
                        b.len()
                    )
                    .into_bytes();
                    r.extend_from_slice(&b);
                    r
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            };
            let _ = s.write_all(&resp);
        }
    });
    format!("http://{addr}")
}

#[test]
fn install_round_trip_over_local_server() {
    let root = std::env::temp_dir().join(format!("nsql-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let id = "fakedb";
    let plat = install::platform();
    let good = package(id, "1.2.0", nsql_abi::ABI_VERSION);
    let bad_abi = package(id, "2.0.0", 99);
    let routes: Routes = Arc::new(Mutex::new(Vec::new()));
    let base = serve(routes.clone());
    {
        let mut r = routes.lock().expect("lock");
        r.push((
            "/SosomLab/t/main/drivers/index.json".into(),
            format!(
                r#"{{"drivers":[{{"id":"{id}","name":"Fake DB","dialect":"{id}","latest":"1.2.0","platforms":["{plat}"]}},
                     {{"id":"elsewhere","latest":"1.0.0","platforms":["nowhere-arch"]}}]}}"#
            )
            .into_bytes(),
        ));
        for (ver, pkg, sig_ok, sha_ok) in [
            ("1.2.0", &good, true, true),
            ("1.1.0", &good, false, true),
            ("1.0.0", &good, true, false),
            ("2.0.0", &bad_abi, true, true),
        ] {
            let stem = format!(
                "/SosomLab/t/releases/download/driver-{id}-v{ver}/nsql-driver-{id}-{ver}-{plat}"
            );
            r.push((format!("{stem}.zip"), pkg.clone()));
            let sha = if sha_ok {
                hex(&sha256(pkg))
            } else {
                hex(&sha256(b"x"))
            };
            r.push((
                format!("{stem}.sha256"),
                format!("{sha}  pkg.zip\n").into_bytes(),
            ));
            let sig = if sig_ok { sign(pkg) } else { sign(b"tampered") };
            r.push((format!("{stem}.sig"), sig.into_bytes()));
        }
    }
    let src = Source {
        repo: "SosomLab/t".into(),
        branch: "main".into(),
        token: None,
        timeout: std::time::Duration::from_secs(5),
        base: base.clone(),
        api: base.clone(),
        raw: base,
    };
    std::env::set_var("HTTPS_PROXY", "");
    let keys = keys();
    let mut stages = Vec::new();
    let rep = install::install(
        &src,
        &root,
        id,
        None,
        false,
        false,
        &keys,
        &mut |st, _, _| {
            if stages.last() != Some(&st.to_string()) {
                stages.push(st.to_string());
            }
        },
    )
    .expect("install latest");
    assert_eq!((rep.id.as_str(), rep.version.as_str()), (id, "1.2.0"));
    assert_eq!(rep.key_id, "drv-test");
    assert_eq!(rep.files, 3);
    assert_eq!(stages, ["index", "download", "verify", "install"]);
    assert!(rep.dir.join(library_file_name_for(id)).is_file());
    assert!(root.join(install::INDEX_FILE).is_file(), "index cached");
    assert!(
        !root.join(".tmp").exists()
            || std::fs::read_dir(root.join(".tmp"))
                .map(|d| d.count() == 0)
                .unwrap_or(true)
    );
    let listed = nsql_drivers::ext::list_extensions_in(&root);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].version, "1.2.0");
    assert!(listed[0].manifest);
    // 이미 설치 → 거부 · --force → 다시.
    let again = install::install(
        &src,
        &root,
        id,
        Some("1.2.0"),
        false,
        false,
        &keys,
        &mut |_, _, _| {},
    );
    assert!(again.expect_err("dup").contains("already installed"));
    install::install(
        &src,
        &root,
        id,
        Some("1.2.0"),
        false,
        true,
        &keys,
        &mut |_, _, _| {},
    )
    .expect("force");
    // 서명 훼손 · 해시 훼손 · ABI 불일치 · 색인에 없음 · 플랫폼 없음 · 키 없음.
    let e = install::install(
        &src,
        &root,
        id,
        Some("1.1.0"),
        false,
        false,
        &keys,
        &mut |_, _, _| {},
    )
    .expect_err("bad sig");
    assert!(e.contains("signature"), "{e}");
    let e = install::install(
        &src,
        &root,
        id,
        Some("1.0.0"),
        false,
        false,
        &keys,
        &mut |_, _, _| {},
    )
    .expect_err("bad sha");
    assert!(e.contains("sha256 mismatch"), "{e}");
    let e = install::install(
        &src,
        &root,
        id,
        Some("2.0.0"),
        false,
        false,
        &keys,
        &mut |_, _, _| {},
    )
    .expect_err("bad abi");
    assert!(e.contains("ABI"), "{e}");
    assert!(install::install(
        &src,
        &root,
        "nope",
        None,
        false,
        false,
        &keys,
        &mut |_, _, _| {}
    )
    .expect_err("not in index")
    .contains("not in the driver index"));
    assert!(install::install(
        &src,
        &root,
        "elsewhere",
        None,
        false,
        false,
        &keys,
        &mut |_, _, _| {}
    )
    .expect_err("platform")
    .contains("no build for"));
    assert!(install::install(
        &src,
        &root,
        id,
        Some("1.2.0"),
        false,
        true,
        &[],
        &mut |_, _, _| {}
    )
    .expect_err("no keys")
    .contains("no trusted"));
    assert!(!root.join(id).join("1.1.0").exists());
    assert!(!root.join(id).join("2.0.0").exists());
    // 갱신 비교: 색인 latest 1.2.0 = 설치본 → 없음 · 색인을 1.3.0으로 바꾸면 하나.
    let (idx, _) = install::fetch_index(&src, &root, false).expect("index");
    assert!(install::updates(&idx, &listed).is_empty());
    let mut newer = idx;
    newer.drivers[0].latest = Some("1.3.0".into());
    assert_eq!(install::updates(&newer, &listed).len(), 1);
    // 오프라인 --file: 자산 이름 + 옆의 .sha256/.sig.
    let off = root.join("offline");
    std::fs::create_dir_all(&off).expect("mkdir");
    let name = install::asset_name(id, "1.2.0");
    let zip_path: PathBuf = off.join(&name);
    std::fs::write(&zip_path, &good).expect("write");
    std::fs::write(off.join(name.replace(".zip", ".sig")), sign(&good)).expect("write");
    std::fs::write(
        off.join(name.replace(".zip", ".sha256")),
        format!("{}  {name}\n", hex(&sha256(&good))),
    )
    .expect("write");
    let rep = install::install_file(&root, &zip_path, true, &keys).expect("file install");
    assert_eq!(rep.version, "1.2.0");
    std::fs::write(
        off.join(name.replace(".zip", ".sig")),
        "ed25519 drv-test 00",
    )
    .expect("write");
    assert!(install::install_file(&root, &zip_path, true, &keys).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ D-350 오프라인 전체 설치: 번들 zip(자산 zip + .sha256 + .sig · 다른 OS 자산은 건너뜀 · 서명 틀린 자산은 그것만 실패).
#[test]
fn offline_bundle_installs_every_asset_for_this_platform() {
    let root = std::env::temp_dir().join(format!("nsql-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let plat = install::platform();
    let a = package("alpha", "1.0.0", nsql_abi::ABI_VERSION);
    let b = package("beta", "2.0.0", nsql_abi::ABI_VERSION);
    let c = package("gamma", "3.0.0", nsql_abi::ABI_VERSION);
    let name = |id: &str, ver: &str| format!("nsql-driver-{id}-{ver}-{plat}.zip");
    let (na, nb, nc) = (
        name("alpha", "1.0.0"),
        name("beta", "2.0.0"),
        name("gamma", "3.0.0"),
    );
    let sha = |p: &[u8], n: &str| format!("{}  {n}\n", hex(&sha256(p)));
    let sig_a = sign(&a);
    let sig_b = sign(b"tampered");
    let sig_c = sign(&c);
    let sha_a = sha(&a, &na);
    let sha_c = sha(&c, &nc);
    let other = "nsql-driver-delta-1.0.0-nowhere-arch.zip";
    let bundle = zip::build(
        &[
            ("bundle/index.json", b"{\"drivers\":[]}", None),
            ("bundle/README.txt", b"offline", None),
            (&format!("bundle/{na}"), &a, None),
            (
                &format!("bundle/{}", na.replace(".zip", ".sig")),
                sig_a.as_bytes(),
                None,
            ),
            (
                &format!("bundle/{}", na.replace(".zip", ".sha256")),
                sha_a.as_bytes(),
                None,
            ),
            (&format!("bundle/{nb}"), &b, None),
            (
                &format!("bundle/{}", nb.replace(".zip", ".sig")),
                sig_b.as_bytes(),
                None,
            ),
            (&format!("bundle/{nc}"), &c, None),
            (
                &format!("bundle/{}", nc.replace(".zip", ".sig")),
                sig_c.as_bytes(),
                None,
            ),
            (
                &format!("bundle/{}", nc.replace(".zip", ".sha256")),
                sha_c.as_bytes(),
                None,
            ),
            (&format!("bundle/{other}"), &a, None),
        ],
        true,
    );
    assert!(install::is_bundle(&bundle));
    assert!(!install::is_bundle(&a), "자산 하나는 번들이 아니다");
    let out = install::install_bundle(&root, &bundle, false, &keys()).expect("bundle");
    let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [na.as_str(), nb.as_str(), nc.as_str()],
        "이 OS 자산만 · 이름 순"
    );
    assert!(out[0].1.is_ok(), "{:?}", out[0].1);
    assert!(out[1]
        .1
        .as_ref()
        .expect_err("bad sig")
        .contains("signature"));
    assert_eq!(out[2].1.as_ref().expect("gamma").version, "3.0.0");
    let listed = nsql_drivers::ext::list_extensions_in(&root);
    let ids: Vec<&str> = listed.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"alpha") && ids.contains(&"gamma"));
    let _ = std::fs::remove_dir_all(&root);
}
