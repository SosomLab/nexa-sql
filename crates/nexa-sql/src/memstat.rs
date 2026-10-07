//! **메모리 계측 원장·포트**(docs/80 · 사용자 09-24): 상태줄 총량과 메모리 맵 창이 같은 `Sample`을 읽는다.
//!
//! - 비용 원칙: 창이 닫혀 있으면 이 모듈은 **그릴 때 `sys_total()` 한 번**만 불린다(타이머·순회 없음).
//! - 원장 = [`Cat`](카테고리 · 라벨 · 색) · 포트 = [`MemSource`](부품이 자기 바이트를 보고) · 수집 = 호스트 `App::mem_sample()` 한 곳.
//! - 데이터 카테고리는 각 부품의 어림(`approx_bytes` 규칙)이고, OS 총량과의 차이는 [`Sample::other`]로 드러낸다.

use nsql_i18n::Msg;

/// 데이터 카테고리 원장 — 새 캐시를 만들면 여기 한 줄 + `MemSource` 구현 한 줄(39 §3 부하원 등재와 짝).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Cat {
    ResultData,
    ResultText,
    EditorText,
    EditorHistory,
    EditorCache,
    Meta,
    MetaCols,
    MetaDetail,
    Intel,
    Logs,
    Surfaces,
    Icons,
}

impl Cat {
    pub(crate) const ALL: [Cat; 12] = [
        Cat::ResultData,
        Cat::ResultText,
        Cat::EditorText,
        Cat::EditorHistory,
        Cat::EditorCache,
        Cat::Meta,
        Cat::MetaCols,
        Cat::MetaDetail,
        Cat::Intel,
        Cat::Logs,
        Cat::Surfaces,
        Cat::Icons,
    ];
    pub(crate) const N: usize = Self::ALL.len();

    pub(crate) fn idx(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// 묶음(표의 구획 · 소계 단위 · 사용자 10-07 "그룹별 합산").
    pub(crate) fn group(self) -> Group {
        match self {
            Cat::ResultData | Cat::ResultText => Group::Results,
            Cat::EditorText | Cat::EditorHistory | Cat::EditorCache => Group::Editor,
            Cat::Meta | Cat::MetaCols | Cat::MetaDetail | Cat::Intel => Group::Metadata,
            Cat::Logs | Cat::Surfaces | Cat::Icons => Group::Ui,
        }
    }

    pub(crate) fn label(self) -> Msg {
        match self {
            Cat::ResultData => Msg::MemCatResultData,
            Cat::ResultText => Msg::MemCatResultText,
            Cat::EditorText => Msg::MemCatEditorText,
            Cat::EditorHistory => Msg::MemCatEditorHistory,
            Cat::EditorCache => Msg::MemCatEditorCache,
            Cat::Meta => Msg::MemCatMeta,
            Cat::MetaCols => Msg::MemCatMetaCols,
            Cat::MetaDetail => Msg::MemCatMetaDetail,
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
            Cat::MetaCols => (0xE8, 0xA8, 0x55),
            Cat::MetaDetail => (0xF2, 0xC6, 0x8C),
            Cat::Intel => (0xF0, 0xBE, 0x6E),
            Cat::Logs => (0x9B, 0x6F, 0xC9),
            Cat::Surfaces => (0xD9, 0x53, 0x53),
            Cat::Icons => (0xE8, 0x9A, 0x9A),
        }
    }

    /// "기타"(런타임·라이브러리·미집계) 색 — 카테고리가 아니라 총량과의 차이라 원장 밖에 둔다.
    pub(crate) const OTHER_COLOR: (u8, u8, u8) = (0x9A, 0xA0, 0xA6);
}

/// 표의 구획 — 데이터 카테고리 묶음 넷 + 런타임(기타 = 총량 − 집계 합).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    Results,
    Editor,
    Metadata,
    Ui,
    Runtime,
}

impl Group {
    pub(crate) const ALL: [Group; 5] = [
        Group::Results,
        Group::Editor,
        Group::Metadata,
        Group::Ui,
        Group::Runtime,
    ];

    pub(crate) fn label(self) -> Msg {
        match self {
            Group::Results => Msg::MemGrpResults,
            Group::Editor => Msg::MemGrpEditor,
            Group::Metadata => Msg::MemGrpMeta,
            Group::Ui => Msg::MemGrpUi,
            Group::Runtime => Msg::MemGrpRuntime,
        }
    }
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
    /// ★ **전용 워킹 셋**(사용자 10-07 "작업 관리자와 다른 이유") = 지금 RAM에 있는 페이지 중 공유 아닌 것 — Windows 작업 관리자
    /// "메모리(개인 작업 집합)"의 정의(`QueryWorkingSet` Shared 비트 제외 합) · Linux `smaps_rollup` Private_* · mac = 0(활성 상태 보기는
    /// 풋프린트와 같다). 풋프린트(커밋)와 다른 축이라 두 숫자가 다른 것이 정상. **전체 표본에서만** 센다(상태줄 조회에는 없음 · 페이지 수 비례).
    pub private_ws: u64,
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
    /// 묶음 소계(런타임은 호출자가 `Sample::other`로).
    pub(crate) fn group_sum(&self, g: Group) -> u64 {
        Cat::ALL
            .iter()
            .filter(|c| c.group() == g)
            .fold(0u64, |a, c| a.saturating_add(self.get(*c)))
    }
}

/// 포트 — 부품은 자기 바이트를 보고만 한다(수집·표시는 호스트·창).
pub(crate) trait MemSource {
    fn mem_report(&self, acc: &mut Acc);
}

/// 한 번의 표본 — 상태줄·창이 같은 것을 읽는다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub sys: SysMem,
    pub data: Acc,
}

impl Sample {
    /// 총량(풋프린트) − 집계한 데이터 합(포화) = 런타임·라이브러리·미집계.
    pub(crate) fn other(&self) -> u64 {
        self.sys.footprint.saturating_sub(self.data.sum())
    }

    /// ★ **화면에 보이는 값의 서명**(T-310 · 10-08): 메모리 바이트는 매초 조금씩 흔들려 원값 비교로는 "안 바뀜"이 거의 없다 →
    /// 표에 실제로 찍히는 글(`fmt` 유효숫자 3 · 비율 소수 1 · 묶음 소계 · 시스템 행)로 해시를 만들어 같으면 다시 그리지 않는다.
    pub(crate) fn display_sig(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let foot = self.sys.footprint.max(1);
        let pct = |b: u64| format!("{:.1}", b as f64 * 100.0 / foot as f64);
        for c in Cat::ALL {
            let b = self.data.get(c);
            fmt(b).hash(&mut h);
            pct(b).hash(&mut h);
        }
        let other = self.other();
        fmt(other).hash(&mut h);
        pct(other).hash(&mut h);
        for g in Group::ALL {
            let sum = if g == Group::Runtime {
                other
            } else {
                self.data.group_sum(g)
            };
            fmt(sum).hash(&mut h);
        }
        for v in [
            self.sys.footprint,
            self.sys.private_ws,
            self.sys.resident,
            self.sys.anon,
            self.sys.file_backed,
            self.sys.compressed,
            self.sys.heap_used,
            self.sys.heap_held,
        ] {
            fmt(v).hash(&mut h);
        }
        h.finish()
    }
}

/// 전체 표본(창이 열려 있을 때 · `mem.refresh_ms`마다) — OS 조회 + 부품 보고.
/// `heap` = `Some((used, held))`이면 **힙 통계를 다시 재지 않고** 그 값을 쓴다(`os::sys_lite` · 가벼운 OS 요약만) ·
/// `None`이면 전부 잰다(`os::sys` = 힙 걷기 포함).
///
/// ★ T-310 뿌리(10-08 계측 · `NSQL_TRACE_MEMWIN`): 표본 22 ms 중 **OS 요약이 10~21 ms**였고 그 거의 전부가 힙 통계
/// (Windows `HeapSummary` = 프로세스 힙 걷기 · 결과 10만 행이면 블록 수 비례)였다 — 부품 보고 수십 µs · 전용 WS 2.5 ms.
/// 힙 통계는 호스트가 `mem.heap_refresh_ms`(5 s)마다만 새로 재고, 그 사이 표본은 지난 값을 쓴다.
pub(crate) fn sample(
    sources: &[&dyn MemSource],
    extra: impl FnOnce(&mut Acc),
    heap: Option<(u64, u64)>,
) -> Sample {
    let tr = trace_on();
    let t0 = std::time::Instant::now();
    let mut data = Acc::default();
    for s in sources {
        s.mem_report(&mut data);
    }
    let t1 = std::time::Instant::now();
    extra(&mut data);
    let t2 = std::time::Instant::now();
    let mut sys = match heap {
        Some((used, held)) => {
            let mut s = os::sys_lite();
            s.heap_used = used;
            s.heap_held = held;
            s
        }
        None => os::sys(),
    };
    let t3 = std::time::Instant::now();
    sys.private_ws = os::private_ws(&sys);
    if tr {
        // 진단: 표본 한 번의 비용을 네 단계로 가른다(부품 보고 · 그리드/표면/로그 · OS 요약(힙 포함 여부) · 전용 WS 페이지 걷기).
        let us = |a: std::time::Instant, b: std::time::Instant| (b - a).as_micros();
        eprintln!(
            "[memwin] parts: sources {} us · extra {} us · sys {} us ({}) · private_ws {} us (resident {} pages)",
            us(t0, t1),
            us(t1, t2),
            us(t2, t3),
            if heap.is_some() { "lite" } else { "full+heap" },
            t3.elapsed().as_micros(),
            sys.resident / 4096
        );
    }
    Sample { sys, data }
}

/// 진단 스위치 `NSQL_TRACE_MEMWIN=1`(한 번만 읽는다) — 표본 비용·다시 그리기 여부를 stderr로.
pub(crate) fn trace_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NSQL_TRACE_MEMWIN").is_some())
}

/// 변화 표시가 남는 표본 수(바뀐 뒤 이만큼의 표본 동안 ▲/▼를 보여 준다 · nexa-dir3와 같음).
pub(crate) const TREND_HOLD: u8 = 6;
/// "기타"·시스템 줄은 운영체제 값의 차이라 늘 조금씩 흔들린다 — 이보다 작은 변화는 표시하지 않는다.
const SYS_NOISE: u64 = 64 * 1024;

/// ★ 줄마다 **늘고 주는 과정**(사용자 10-07 · nexa-dir3 이식): 직전 표본과의 차이를 기억해 [`TREND_HOLD`] 표본 동안 보여 준다 —
/// 힙 정리 뒤 어느 줄이 얼마나 줄었는지가 바로 보인다. 칸 = 영역 [`Cat::N`] + 기타 1 + 시스템 [`Trend::SYS_N`](풋프린트·상주·힙 사용·힙 여유·전용 WS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Trend {
    last: Option<[u64; Trend::LEN]>,
    delta: [i64; Trend::LEN],
    ttl: [u8; Trend::LEN],
}

impl Trend {
    /// "기타" 칸.
    pub(crate) const OTHER: usize = Cat::N;
    /// 시스템 칸 시작(순서 = `SYS_ORDER`).
    pub(crate) const SYS: usize = Cat::N + 1;
    pub(crate) const SYS_N: usize = 5;
    const LEN: usize = Cat::N + 1 + Self::SYS_N;

    /// 시스템 칸 차례: 풋프린트 · 상주 · 힙 사용 · 힙 여유 · 전용 워킹 셋.
    fn sys_values(s: &SysMem) -> [u64; Self::SYS_N] {
        [
            s.footprint,
            s.resident,
            s.heap_used,
            s.heap_held,
            s.private_ws,
        ]
    }

    fn values(s: &Sample) -> [u64; Self::LEN] {
        let mut v = [0u64; Self::LEN];
        for c in Cat::ALL {
            v[c.idx()] = s.data.get(c);
        }
        v[Self::OTHER] = s.other();
        v[Self::SYS..].copy_from_slice(&Self::sys_values(&s.sys));
        v
    }

    /// 새 표본 — 바뀐 칸은 차이를 새로 적고, 안 바뀐 칸은 남은 표시 시간을 하나 줄인다.
    pub(crate) fn update(&mut self, s: &Sample) {
        let now = Self::values(s);
        if let Some(prev) = self.last {
            for i in 0..Self::LEN {
                let d = now[i] as i64 - prev[i] as i64;
                let noise = i >= Self::OTHER && d.unsigned_abs() < SYS_NOISE;
                if d != 0 && !noise {
                    self.delta[i] = d;
                    self.ttl[i] = TREND_HOLD;
                } else if self.ttl[i] > 0 {
                    self.ttl[i] -= 1;
                }
            }
        }
        self.last = Some(now);
    }

    /// 칸 `i`의 최근 변화(표시 시간이 남아 있을 때만 · 양수 = 늘었다).
    pub(crate) fn shown(&self, i: usize) -> Option<i64> {
        (self.ttl.get(i).copied().unwrap_or(0) > 0).then(|| self.delta[i])
    }

    /// 시스템 칸 `k`(0..SYS_N)의 최근 변화.
    pub(crate) fn shown_sys(&self, k: usize) -> Option<i64> {
        self.shown(Self::SYS + k)
    }

    /// 아직 보여 줄 변화가 남아 있는 칸이 있는가(있으면 표본마다 다시 그려야 ▲/▼가 사라진다 · T-310).
    pub(crate) fn any_shown(&self) -> bool {
        self.ttl.iter().any(|t| *t > 0)
    }
}

/// 변화 표기 — `▲ 1.20 MB` / `▼ 300 KB`.
pub(crate) fn fmt_delta(d: i64) -> String {
    let arrow = if d >= 0 { "▲" } else { "▼" };
    format!("{arrow} {}", fmt(d.unsigned_abs()))
}

/// 상태줄용 총량만(창이 닫혀 있을 때의 유일한 조회 · ≈ µs).
pub(crate) fn sys_total() -> u64 {
    // 가벼운 요약만(힙 걷기 없음 · µs) — 상태줄은 그릴 때 이 값 하나면 된다.
    os::sys_lite().footprint
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

    /// 가벼운 요약(`task_info` 한 번) — 힙 통계는 0.
    pub(super) fn sys_lite() -> SysMem {
        let mut info = [0u64; WORDS];
        let mut count = (WORDS * 2) as u32;
        // SAFETY: 버퍼 길이를 natural_t 단위로 알리고 커널은 그만큼만 채운다 · 자기 태스크 포트는 늘 유효.
        let kr = unsafe { task_info(mach_task_self_, TASK_VM_INFO, info.as_mut_ptr(), &mut count) };
        let ok = kr == 0 && (count as usize) >= WORDS * 2;
        SysMem {
            footprint: if ok { info[18] } else { 0 },
            resident: if ok { info[2] } else { 0 },
            anon: if ok { info[6] } else { 0 },
            file_backed: if ok { info[8] } else { 0 },
            compressed: if ok { info[15] } else { 0 },
            heap_used: 0,
            heap_held: 0,
            private_ws: 0,
        }
    }

    /// 전체(요약 + `mstats` = 존 걷기) — 호스트가 `mem.heap_refresh_ms`마다만.
    pub(super) fn sys() -> SysMem {
        let mut s = sys_lite();
        // SAFETY: `mstats`는 인자 없음.
        let ms = unsafe { mstats() };
        s.heap_used = ms.bytes_used as u64;
        s.heap_held = ms.bytes_free as u64;
        s
    }

    /// 활성 상태 보기의 "메모리" = 풋프린트 그대로 — 따로 세지 않는다(행 숨김).
    pub(super) fn private_ws(_s: &SysMem) -> u64 {
        0
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
        fn K32QueryWorkingSet(process: *mut c_void, pv: *mut c_void, cb: u32) -> i32;
    }

    /// 작업 관리자 "메모리(개인 작업 집합)" = 워킹 셋 페이지 중 **Shared 비트(8) 없는** 것 × 페이지 크기(`PSAPI_WORKING_SET_BLOCK`).
    /// 비용 = 페이지 수 비례(300 MB ≈ 77k 항목 · 수백 µs) — 창이 열려 있을 때의 전체 표본에서만 부른다.
    pub(super) fn private_ws(s: &SysMem) -> u64 {
        const PAGE: u64 = 4096;
        // 첫 칸 = 항목 수 · 이어서 항목들(ULONG_PTR) — 상주 페이지 수 + 여유로 잡고, 모자라면 첫 칸이 알려 주는 수로 한 번 더.
        let mut cap = (s.resident / PAGE) as usize + 4096;
        for _ in 0..2 {
            let mut buf = vec![0usize; cap + 1];
            let cb = (buf.len() * std::mem::size_of::<usize>()) as u32;
            // SAFETY: 버퍼 길이를 바이트로 알리고 커널은 그만큼만 채운다 · 자기 프로세스 핸들은 늘 유효.
            let ok =
                unsafe { K32QueryWorkingSet(GetCurrentProcess(), buf.as_mut_ptr().cast(), cb) }
                    != 0;
            let n = buf[0];
            if ok && n <= cap {
                let private = buf[1..=n].iter().filter(|e| (*e >> 8) & 1 == 0).count() as u64;
                return private * PAGE;
            }
            if n == 0 || n > (1usize << 26) {
                break;
            }
            cap = n + 1024;
        }
        0
    }

    /// 가벼운 요약(`GetProcessMemoryInfo` 한 번 · µs) — 힙 통계는 0.
    pub(super) fn sys_lite() -> SysMem {
        let mut pmc = Pmc {
            cb: std::mem::size_of::<Pmc>() as u32,
            ..Default::default()
        };
        // SAFETY: 구조체 크기를 `cb`로 알린다 · 자기 프로세스 핸들은 늘 유효.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) } != 0;
        let private = if ok { pmc.private_usage as u64 } else { 0 };
        let ws = if ok { pmc.working_set_size as u64 } else { 0 };
        SysMem {
            footprint: private,
            resident: ws,
            anon: private,
            file_backed: ws.saturating_sub(private),
            compressed: 0,
            heap_used: 0,
            heap_held: 0,
            private_ws: 0,
        }
    }

    /// 전체(요약 + 힙 통계). `HeapSummary`는 **프로세스 힙을 걷는다**(블록 수 비례 · 10만 행 결과 = 10~20 ms · T-310 계측) →
    /// 호스트가 `mem.heap_refresh_ms`마다만 부른다.
    pub(super) fn sys() -> SysMem {
        let mut s = sys_lite();
        let mut hs = HeapSummaryT {
            cb: std::mem::size_of::<HeapSummaryT>() as u32,
            ..Default::default()
        };
        // SAFETY: 구조체 크기를 `cb`로 알린다 · 프로세스 힙 핸들은 늘 유효.
        let hok = unsafe { HeapSummary(GetProcessHeap(), 0, &mut hs) } != 0;
        if hok {
            s.heap_used = hs.cb_allocated as u64;
            s.heap_held = (hs.cb_committed as u64).saturating_sub(hs.cb_allocated as u64);
        }
        s
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

    /// 가벼운 요약(`/proc/self/statm` 한 번) — 힙 통계는 0.
    pub(super) fn sys_lite() -> SysMem {
        let page = 4096u64;
        let (res, shared) = std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                let mut it = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
                let (_size, res, shared) = (it.next()?, it.next()?, it.next()?);
                Some((res * page, shared * page))
            })
            .unwrap_or((0, 0));
        SysMem {
            footprint: res.saturating_sub(shared),
            resident: res,
            anon: res.saturating_sub(shared),
            file_backed: shared,
            compressed: 0,
            heap_used: 0,
            heap_held: 0,
            private_ws: 0,
        }
    }

    /// 전체(요약 + `mallinfo2` · 아레나 잠금) — 호스트가 `mem.heap_refresh_ms`마다만.
    pub(super) fn sys() -> SysMem {
        let mut s = sys_lite();
        // SAFETY: 인자 없는 glibc 2.33+ 통계 호출.
        let mi = unsafe { mallinfo2() };
        s.heap_used = (mi.uordblks + mi.hblkhd) as u64;
        s.heap_held = mi.fordblks as u64;
        s
    }

    /// `/proc/self/smaps_rollup`의 Private_Clean + Private_Dirty(kB) — 없으면 0.
    pub(super) fn private_ws(_s: &SysMem) -> u64 {
        std::fs::read_to_string("/proc/self/smaps_rollup")
            .ok()
            .map(|s| {
                s.lines()
                    .filter(|l| l.starts_with("Private_Clean:") || l.starts_with("Private_Dirty:"))
                    .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                    .sum::<u64>()
                    * 1024
            })
            .unwrap_or(0)
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
    pub(super) fn sys_lite() -> SysMem {
        SysMem::default()
    }
    pub(super) fn private_ws(_s: &SysMem) -> u64 {
        0
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
        let s = sample(&[&a, &b], |acc| acc.add(Cat::Logs, 7), None);
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
        // 전용 워킹 셋(Windows·Linux glibc) = 상주 이하 · 0보다 큼.
        if cfg!(any(windows, all(target_os = "linux", target_env = "gnu"))) {
            assert!(
                s.sys.private_ws > 0 && s.sys.private_ws <= s.sys.resident.max(s.sys.private_ws)
            );
        }
    }

    /// 묶음 = 모든 카테고리를 정확히 한 번씩 덮고, 소계 합 = 전체 합.
    #[test]
    fn groups_partition_categories() {
        let mut acc = Acc::default();
        for (i, c) in Cat::ALL.into_iter().enumerate() {
            acc.add(c, (i as u64 + 1) * 10);
        }
        let by_group: u64 = Group::ALL
            .iter()
            .filter(|g| **g != Group::Runtime)
            .map(|g| acc.group_sum(*g))
            .sum();
        assert_eq!(by_group, acc.sum());
        assert_eq!(acc.group_sum(Group::Runtime), 0);
        assert!(Group::ALL
            .iter()
            .all(|g| *g == Group::Runtime || Cat::ALL.iter().any(|c| c.group() == *g)));
    }

    /// 표시 서명: 같은 글로 찍히는 작은 흔들림(수 바이트~수 KB)은 같은 서명 · 글이 바뀌는 변화(MB 단위)는 다른 서명(T-310).
    #[test]
    fn display_sig_ignores_sub_display_jitter() {
        let mb = 1024 * 1024;
        let a = sample_of(100 * mb, &[(Cat::ResultData, 10 * mb)], 20 * mb);
        let b = sample_of(
            100 * mb + 3_000,
            &[(Cat::ResultData, 10 * mb + 700)],
            20 * mb + 40_000,
        );
        assert_eq!(
            a.display_sig(),
            b.display_sig(),
            "글로 안 보이는 흔들림 = 같은 서명"
        );
        let c = sample_of(100 * mb, &[(Cat::ResultData, 15 * mb)], 20 * mb);
        assert_ne!(a.display_sig(), c.display_sig(), "5 MB 변화 = 다른 서명");
        let d = sample_of(100 * mb, &[(Cat::ResultData, 10 * mb)], 2 * mb);
        assert_ne!(
            a.display_sig(),
            d.display_sig(),
            "힙 여유 20 → 2 MB = 다른 서명"
        );
    }

    fn sample_of(footprint: u64, parts: &[(Cat, u64)], heap_held: u64) -> Sample {
        let mut data = Acc::default();
        for (c, n) in parts {
            data.add(*c, *n);
        }
        Sample {
            sys: SysMem {
                footprint,
                resident: footprint,
                heap_held,
                ..SysMem::default()
            },
            data,
        }
    }

    /// 변화량: 첫 표본 = 없음 · 바뀐 칸만 TREND_HOLD 표본 동안 · 기타·시스템 줄의 작은 흔들림은 생략 · 힙 여유가 줄면 ▼.
    #[test]
    fn trend_tracks_changes_with_hold_and_noise() {
        let mut t = Trend::default();
        let mb = 1024 * 1024;
        t.update(&sample_of(100 * mb, &[(Cat::ResultData, 10 * mb)], 20 * mb));
        assert_eq!(t.shown(Cat::ResultData.idx()), None);
        // 결과 데이터 +5 MB · 기타 −5 MB(= 같은 총량) · 힙 여유 20 → 2 MB(정리).
        t.update(&sample_of(100 * mb, &[(Cat::ResultData, 15 * mb)], 2 * mb));
        assert_eq!(t.shown(Cat::ResultData.idx()), Some(5 * mb as i64));
        assert_eq!(t.shown(Trend::OTHER), Some(-(5 * mb as i64)));
        assert_eq!(t.shown_sys(3), Some(-(18 * mb as i64)));
        assert_eq!(t.shown_sys(0), None, "풋프린트는 안 바뀜");
        assert_eq!(fmt_delta(5 * mb as i64), "▲ 5.00 MB");
        assert_eq!(fmt_delta(-(18 * mb as i64)), "▼ 18.0 MB");
        // 기타 줄의 32 KB 흔들림 = 잡음(표시 안 함 · 남은 시간만 줄어든다).
        t.update(&sample_of(
            100 * mb + 32 * 1024,
            &[(Cat::ResultData, 15 * mb)],
            2 * mb,
        ));
        assert_eq!(t.shown(Trend::OTHER), Some(-(5 * mb as i64)));
        // 안 바뀐 표본 HOLD번 뒤에는 사라진다.
        for _ in 0..TREND_HOLD {
            t.update(&sample_of(
                100 * mb + 32 * 1024,
                &[(Cat::ResultData, 15 * mb)],
                2 * mb,
            ));
        }
        assert_eq!(t.shown(Cat::ResultData.idx()), None);
        assert_eq!(t.shown(Trend::OTHER), None);
    }
}
