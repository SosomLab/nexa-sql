//! build.rs — CLI `nsql.exe`의 아이콘·버전 리소스(Windows · 09-28 신설 — 종전 버전 정보 공란). 본체 = `packaging/windows/winres.rs`.
include!("../../packaging/windows/winres.rs");

fn main() {
    embed_windows_resources("nsql.rc", "nsql");
}
