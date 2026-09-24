//! **메모리 계측 원장·포트**(docs/80 · 사용자 09-24): 상태줄 총량과 메모리 맵 창이 같은 `Sample`을 읽는다.
//!
//! - 비용 원칙: 창이 닫혀 있으면 이 모듈은 **그릴 때 `sys_total()` 한 번**만 불린다(타이머·순회 없음).
//! - 원장 = [`Cat`](카테고리 · 라벨 · 색) · 포트 = [`MemSource`](부품이 자기 바이트를 보고) · 수집 = 호스트 `App::mem_sample()` 한 곳.
//! - 데이터 카테고리는 각 부품의 어림(`approx_bytes` 규칙)이고, OS 총량과의 차이는 [`Sample::other`]로 드러낸다.

use nsql_i18n::Msg;
use std::time::Instant;

/// 데이터 카테고리 원장 — 새 캐시를 만들면 여기 한 줄 + `MemSource` 구현 한 줄(39 §3 부하원 등재와 짝).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Cat {
    ResultData,
    ResultText,
    EditorText,
    EditorHistory,
    EditorCache,
    Meta,
    Intel,
    Logs,
    Surfaces,
    Icons,
}

impl Cat {
    pub(crate) const ALL: [Cat; 10] = [
        Cat::ResultData,
        Cat::ResultText,
        Cat::EditorText,
        Cat::EditorHistory,
        Cat::EditorCache,
        Cat::Meta,
        Cat::Intel,
        Cat::Logs,
        Cat::Surfaces,
        Cat::Icons,
    ];
    pub(crate) const N: usize = Self::ALL.len();

    fn idx(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub(crate) fn label(self) -> Msg {
        match self {
            Cat::ResultData => Msg::MemCatResultData,
            Cat::ResultText => Msg::MemCatResultText,
            Cat::EditorText => Msg::MemCatEditorText,
            Cat::EditorHistory => Msg::MemCatEditorHistory,
            Cat::EditorCache => Msg::MemCatEditorCache,
            Cat::Meta => Msg::MemCatMeta,
            Cat::Intel => Msg::MemCatIntel,
            Cat::Logs => Msg::MemCatLogs,
            Cat::Surfaces => Msg::MemCatSurfaces,
            Cat::Icons => Msg::MemCatIcons,
        }
    }

    /// 고정 팔레트(테마 무관 · 밝은/어두운 배경 모두 읽히는 중간 채도) — 같은 뿌리(결과·편집기)는 같은 색조의 명도 차.
    pub(crate) fn color(self) -> (u8, u8, u8) {
        match self {
            Cat::ResultData => (0x3A, 0x7B, 0xD5),
            Cat::ResultText => (0x7F, 0xB0, 0xE6),
            Cat::EditorText => (0x2E, 0xA0, 0x6E),
            Cat::EditorHistory => (0x7C, 0xC4, 0x9A),
            Cat::EditorCache => (0xB4, 0xDC, 0xC2),
            Cat::Meta => (0xE0, 0x8E, 0x2B),
            Cat::Intel => (0xF0, 0xBE, 0x6E),
            Cat::Logs => (0x9B, 0x6F, 0xC9),
            Cat::Surfaces => (0xD9, 0x53, 0x53),
            Cat::Icons => (0xE8, 0x9A, 0x9A),
        }
    }

    /// "기타"(런타임·라이브러리·미집계) 색 — 카테고리가 아니라 총량과의 차이라 원장 밖에 둔다.
    pub(crate) const OTHER_COLOR: (u8, u8, u8) = (0x9A, 0xA0, 0xA6);
}

/// OS가 말하는 프로세스 총량(모르는 칸 = 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SysMem {
    /// 활성 상태 보기·작업 관리자의 "메모리"(mac phys_footprint · Win Private Bytes · Linux resident−shared · D-208).
    pub footprint: u64,
    pub resident: u64,
    /// 익명 페이지(힙·스택 — mac internal · Win private · Linux resident−shared).
    pub anon: u64,
    /// 파일 매핑(라이브러리·글꼴 — mac external · Win ws−private · Linux shared).
    pub file_backed: u64,
    /// 압축돼 있는 몫(mac).
    pub compressed: u64,
    /// 할당자가 **쓰고 있는** 바이트.
    pub heap_used: u64,
    /// 할당자가 **들고 있지만 안 쓰는** 바이트 — mac `mstats().bytes_free`·glibc `fordblks`·Win 커밋−할당. mac은 **가상 예약**이라
    /// 상주보다 클 수 있다(회수 `memtrim`이 돌려줄 수 있는 몫의 상한으로 읽는다).
    pub heap_held: u64,
}

/// 카테고리 누적기.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Acc {
    bytes: [u64; Cat::N],
}

impl Acc {
    pub(crate) fn add(&mut self, cat: Cat, n: u64) {
        self.bytes[cat.idx()] = self.bytes[cat.idx()].saturating_add(n);
    }
    pub(crate) fn get(&self, cat: Cat) -> u64 {
        self.bytes[cat.idx()]
    }
    pub(crate) fn sum(&self) -> u64 {
        self.bytes.iter().fold(0u64, |a, b| a.saturating_add(*b))
    }
}

/// 포트 — 부품은 자기 바이트를 보고만 한다(수집·표시는 호스트·창).
pub(crate) trait MemSource {
    fn mem_report(&self, acc: &mut Acc);
}

/// 한 번의 표본 — 상태줄·창이 같은 것을 읽는다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub at: Instant,
    pub sys: SysMem,
    pub data: Acc,
}

impl Sample {
    /// 총량(풋프린트) − 집계한 데이터 합(포화) = 런타임·라이브러리·미집계.
    pub(crate) fn other(&self) -> u64 {
        self.sys.footprint.saturating_sub(self.data.sum())
    }
}

/// 전체 표본(창이 열려 있을 때 · `mem.refresh_ms`마다) — OS 조회 + 부품 보고.
pub(crate) fn sample(sources: &[&dyn MemSource], extra: impl FnOnce(&mut Acc)) -> Sample {
    let mut data = Acc::default();
    for s in sources {
        s.mem_report(&mut data);
    }
    extra(&mut data);
    Sample {
        at: Instant::now(),
        sys: os::sys(),
        data,
    }
}

/// 상태줄용 총량만(창이 닫혀 있을 때의 유일한 조회 · ≈ µs).
pub(crate) fn sys_total() -> u64 {
    os::sys().footprint
}

/// 바이트 표기 — 1024 단위 · 유효숫자 3(`312 MB` · `1.24 GB` · `640 KB`).
pub(crate) fn fmt(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", U[i])
    } else if v >= 10.0 {
        format!("{v:.1} {}", U[i])
    } else {
        format!("{v:.2} {}", U[i])
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::SysMem;

    #[repr(C)]
    #[derive(Default)]
    struct MStats {
        bytes_total: usize,
        chunks_used: usize,
        bytes_used: usize,
        chunks_free: usize,
        bytes_free: usize,
    }
    extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut u64, count: *mut u32) -> i32;
        fn mstats() -> MStats;
    }
    const TASK_VM_INFO: u32 = 22;
    /// rev1 = `phys_footprint`까지(8바이트 19칸). 칸: 2 resident · 6 internal · 8 external · 15 compressed · 18 phys_footprint.
    const WORDS: usize = 19;

    pub(super) fn sys() -> SysMem {
        let mut info = [0u64; WORDS];
        let mut count = (WORDS * 2) as u32;
        // SAFETY: 버퍼 길이를 natural_t 단위로 알리고 커널은 그만큼만 채운다 · 자기 태스크 포트는 늘 유효 · `mstats`는 인자 없음.
        let kr = unsafe { task_info(mach_task_self_, TASK_VM_INFO, info.as_mut_ptr(), &mut count) };
        let ms = unsafe { mstats() };
        let ok = kr == 0 && (count as usize) >= WORDS * 2;
        SysMem {
            footprint: if ok { info[18] } else { 0 },
            resident: if ok { info[2] } else { 0 },
            anon: if ok { info[6] } else { 0 },
            file_backed: if ok { info[8] } else { 0 },
            compressed: if ok { info[15] } else { 0 },
            heap_used: ms.bytes_used as u64,
            heap_held: ms.bytes_free as u64,
        }
    }
}

#[cfg(windows)]
mod os {
    use super::SysMem;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct Pmc {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }
    #[repr(C)]
    #[derive(Default)]
    struct HeapSummaryT {
        cb: u32,
        cb_allocated: usize,
        cb_committed: usize,
        cb_reserved: usize,
        cb_max_reserve: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessHeap() -> *mut c_void;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Pmc, cb: u32) -> i32;
        fn HeapSummary(heap: *mut c_void, flags: u32, summary: *mut HeapSummaryT) -> i32;
    }

    pub(super) fn sys() -> SysMem {
        let mut pmc = Pmc {
            cb: std::mem::size_of::<Pmc>() as u32,
            ..Default::default()
        };
        let mut hs = HeapSummaryT {
            cb: std::mem::size_of::<HeapSummaryT>() as u32,
            ..Default::default()
        };
        // SAFETY: 구조체 크기를 `cb`로 알린다 · 프로세스 힙 핸들은 늘 유효.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) } != 0;
        let hok = unsafe { HeapSummary(GetProcessHeap(), 0, &mut hs) } != 0;
        let private = if ok { pmc.private_usage as u64 } else { 0 };
        let ws = if ok { pmc.working_set_size as u64 } else { 0 };
        SysMem {
            footprint: private,
            resident: ws,
            anon: private,
            file_backed: ws.saturating_sub(private),
            compressed: 0,
            heap_used: if hok { hs.cb_allocated as u64 } else { 0 },
            heap_held: if hok {
                (hs.cb_committed as u64).saturating_sub(hs.cb_allocated as u64)
            } else {
                0
            },
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod os {
    use super::SysMem;

    #[repr(C)]
    #[derive(Default)]
    struct MallInfo2 {
        arena: usize,
        ordblks: usize,
        smblks: usize,
        hblks: usize,
        hblkhd: usize,
        usmblks: usize,
        fsmblks: usize,
        uordblks: usize,
        fordblks: usize,
        keepcost: usize,
    }
    extern "C" {
        fn mallinfo2() -> MallInfo2;
    }

    pub(super) fn sys() -> SysMem {
        let page = 4096u64;
        let (res, shared) = std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                let mut it = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
                let (_size, res, shared) = (it.next()?, it.next()?, it.next()?);
                Some((res * page, shared * page))
            })
            .unwrap_or((0, 0));
        // SAFETY: 인자 없는 glibc 2.33+ 통계 호출.
        let mi = unsafe { mallinfo2() };
        SysMem {
            footprint: res.saturating_sub(shared),
            resident: res,
            anon: res.saturating_sub(shared),
            file_backed: shared,
            compressed: 0,
            heap_used: (mi.uordblks + mi.hblkhd) as u64,
            heap_held: mi.fordblks as u64,
        }
    }
}

#[cfg(not(any(
    windows,
    all(target_os = "linux", target_env = "gnu"),
    target_os = "macos"
)))]
mod os {
    use super::SysMem;
    pub(super) fn sys() -> SysMem {
        SysMem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_units_and_precision() {
        assert_eq!(fmt(0), "0 B");
        assert_eq!(fmt(999), "999 B");
        assert_eq!(fmt(1536), "1.50 KB");
        assert_eq!(fmt(12 * 1024 * 1024 + 300 * 1024), "12.3 MB");
        assert_eq!(fmt(312 * 1024 * 1024), "312 MB");
        assert_eq!(fmt(3 * 1024 * 1024 * 1024 / 2), "1.50 GB");
    }

    struct Fake(u64);
    impl MemSource for Fake {
        fn mem_report(&self, acc: &mut Acc) {
            acc.add(Cat::EditorText, self.0);
        }
    }

    #[test]
    fn sample_sums_sources_and_other_never_underflows() {
        let (a, b) = (Fake(100), Fake(50));
        let s = sample(&[&a, &b], |acc| acc.add(Cat::Logs, 7));
        assert_eq!(s.data.get(Cat::EditorText), 150);
        assert_eq!(s.data.get(Cat::Logs), 7);
        assert_eq!(s.data.sum(), 157);
        // OS를 읽을 수 있는 OS면 총량 > 0 · 기타 = 총량 − 합(포화라 음수 없음).
        if cfg!(any(windows, target_os = "macos")) {
            assert!(s.sys.footprint > 0 && s.sys.resident > 0);
            assert!(s.sys.heap_used > 0);
        }
        assert!(s.other() <= s.sys.footprint);
        assert!(Cat::ALL.iter().all(|c| c.color() != Cat::OTHER_COLOR));
    }
}
