//! build.rs — GUI `nexa-sql.exe`의 아이콘·버전 리소스(Windows). 본체 = `packaging/windows/winres.rs`(CLI와 공용 · docs/93 §4).
include!("../../packaging/windows/winres.rs");

fn main() {
    embed_windows_resources("nexa-sql.rc", "nexa-sql");
}
