//! **메모리 회수**(사용자 09-19 "불필요해진 메모리는 발생 즉시 1회 + 주기적으로 회수해 사용량이 유지되게"):
//! 러스트는 값이 버려지는 순간 힙에 돌려주지만, **힙이 OS에 돌려주는 것은 별개**다 — 큰 파일을 닫거나 10만 행 결과를
//! 버려도 작은 조각(셀 문자열 · 줄)이 많으면 Private Bytes는 그대로 남는다. 이 모듈은 그 "힙 → OS" 단계를 한다.
//!
//! - Windows: 프로세스 힙 `HeapSetInformation(HeapOptimizeResources)`(LFH 캐시 비움 · 8.1+) + `HeapCompact`(빈 구획 병합·디커밋).
//! - Linux(glibc): `malloc_trim(0)` · macOS: `malloc_zone_pressure_relief`.
//! - 워킹셋 트림(`SetProcessWorkingSetSize`)은 **하지 않는다** — 숫자만 작아지고 돌아올 때 페이지 폴트로 느려진다(docs/26 §7-2 ③).
//!
//! 언제 부를지는 호스트(`main.rs mem_*`)의 일: 큰 것을 놓은 직후 1회(디바운스) + 유휴 주기.

/// 프로세스의 (Private Bytes, Working Set) — 회수 전후 로그용(모르면 0).
pub(crate) fn usage() -> (u64, u64) {
    imp::usage()
}

/// 힙을 정리해 OS에 돌려준다. 돌려주는 값 = 걸린 시간(µs).
pub(crate) fn trim() -> u128 {
    let t = std::time::Instant::now();
    imp::trim();
    t.elapsed().as_micros()
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    struct HeapOptimizeResourcesInformation {
        version: u32,
        flags: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCountersEx {
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

    extern "system" {
        fn GetProcessHeap() -> *mut c_void;
        fn GetProcessHeaps(count: u32, heaps: *mut *mut c_void) -> u32;
        fn HeapCompact(heap: *mut c_void, flags: u32) -> usize;
        fn HeapSetInformation(
            heap: *mut c_void,
            class: u32,
            info: *const c_void,
            len: usize,
        ) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
    }

    /// `HeapOptimizeResources` 정보 클래스(Windows 8.1+ · 실패해도 무해).
    const HEAP_OPTIMIZE_RESOURCES: u32 = 3;

    pub(super) fn trim() {
        // SAFETY: 문서화된 Win32 호출 — 힙 핸들은 이 프로세스의 것이고, 정보 구조체는 호출 동안 살아 있다.
        //   실패(옛 Windows · 잠긴 힙)는 반환값으로만 알려지고 부작용이 없다.
        unsafe {
            let info = HeapOptimizeResourcesInformation {
                version: 1,
                flags: 0,
            };
            // 힙 핸들을 null로 주면 프로세스의 모든 힙에 적용된다.
            HeapSetInformation(
                std::ptr::null_mut(),
                HEAP_OPTIMIZE_RESOURCES,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of::<HeapOptimizeResourcesInformation>(),
            );
            let mut heaps: [*mut c_void; 64] = [std::ptr::null_mut(); 64];
            let n = GetProcessHeaps(64, heaps.as_mut_ptr()) as usize;
            if n == 0 || n > heaps.len() {
                HeapCompact(GetProcessHeap(), 0);
            } else {
                for h in &heaps[..n] {
                    HeapCompact(*h, 0);
                }
            }
        }
    }

    pub(super) fn usage() -> (u64, u64) {
        let mut c = ProcessMemoryCountersEx {
            cb: std::mem::size_of::<ProcessMemoryCountersEx>() as u32,
            ..ProcessMemoryCountersEx::default()
        };
        // SAFETY: 출력 구조체의 크기를 `cb`로 알리고 같은 크기를 넘긴다.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        if ok == 0 {
            return (0, 0);
        }
        (c.private_usage as u64, c.working_set_size as u64)
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod imp {
    extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }

    pub(super) fn trim() {
        // SAFETY: glibc의 인자 없는 정리 호출.
        unsafe {
            malloc_trim(0);
        }
    }

    pub(super) fn usage() -> (u64, u64) {
        // /proc/self/statm: size resident shared … (페이지 수) — Private ≈ resident − shared.
        let page = 4096u64;
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                let mut it = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
                let (_size, res, shared) = (it.next()?, it.next()?, it.next()?);
                Some((res.saturating_sub(shared) * page, res * page))
            })
            .unwrap_or((0, 0))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut c_void, goal: usize) -> usize;
        /// `mach_task_self()` 매크로의 실체.
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut u64, count: *mut u32) -> i32;
    }

    /// `TASK_VM_INFO` · rev1 길이(`phys_footprint`까지 = 8바이트 19칸 = natural_t 38개).
    const TASK_VM_INFO: u32 = 22;
    const REV1_WORDS: usize = 19;
    /// `task_vm_info`의 칸(모두 8바이트 경계 — `region_count`+`page_size`가 한 칸): 2 = resident_size · 18 = phys_footprint.
    const IX_RESIDENT: usize = 2;
    const IX_FOOTPRINT: usize = 18;

    pub(super) fn trim() {
        // SAFETY: zone = NULL(모든 영역) · goal = 0(가능한 만큼).
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }

    /// (메모리 풋프린트, 상주 크기) — 활성 상태 보기의 "메모리"가 `phys_footprint`다(Windows Private Bytes에 대응 ·
    /// 86차 mac 점검에서 `NSQL_TRACE_MEM`이 `0 -> 0`만 찍던 것 · T-148).
    pub(super) fn usage() -> (u64, u64) {
        let mut info = [0u64; REV1_WORDS];
        let mut count = (REV1_WORDS * 2) as u32;
        // SAFETY: 버퍼 길이를 `count`(natural_t 단위)로 알리고 커널은 그만큼만 채운다 · 자기 태스크 포트는 늘 유효.
        let kr = unsafe { task_info(mach_task_self_, TASK_VM_INFO, info.as_mut_ptr(), &mut count) };
        if kr != 0 || (count as usize) < REV1_WORDS * 2 {
            return (0, 0);
        }
        (info[IX_FOOTPRINT], info[IX_RESIDENT])
    }
}

#[cfg(not(any(
    windows,
    all(target_os = "linux", target_env = "gnu"),
    target_os = "macos"
)))]
mod imp {
    pub(super) fn trim() {}

    pub(super) fn usage() -> (u64, u64) {
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    /// 큰 조각 다수를 놓은 뒤 회수 — 실패 없이 돌고(어느 OS든) Windows·macOS에서는 사용량을 읽을 수 있다.
    #[test]
    fn trim_runs_and_usage_reads() {
        let junk: Vec<String> = (0..50_000).map(|i| format!("cell value {i:08}")).collect();
        drop(junk);
        let _us = super::trim();
        let (private, ws) = super::usage();
        if cfg!(any(windows, target_os = "macos")) {
            assert!(private > 0 && ws > 0);
            // 풋프린트가 터무니없는 값(칸을 잘못 읽음)이 아닌지 — 시험 프로세스는 1 MB ~ 8 GB 사이.
            assert!((1 << 20..8u64 << 30).contains(&private), "{private}");
        }
    }
}
