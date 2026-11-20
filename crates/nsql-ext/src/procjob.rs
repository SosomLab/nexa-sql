//! ★ 자식 프로세스 **나무째** 끝내기(T-479 · 11-20) — Windows 작업 개체(Job Object) 하나에 자식을 넣어 두고, 끝낼 때 그 안의
//! 프로세스(자식이 띄운 손자 포함)를 한꺼번에 끝낸다. Unix는 띄우는 쪽이 프로세스 그룹으로 같은 일을 한다(`process_group(0)` +
//! `kill(-pgid)`)라 여기서는 아무 일도 하지 않는다 — 호출하는 코드가 OS 분기 없이 같은 꼴로 쓰게 하는 껍데기.
//!
//! - [`KillJob::attach`] — 띄운 직후 부른다. 그 뒤 자식이 만드는 프로세스는 자동으로 같은 작업 개체에 든다.
//! - [`KillJob::kill`] — 지금 든 프로세스를 전부 끝낸다(`TerminateJobObject`).
//! - `Drop` — 핸들을 닫는다. `KILL_ON_JOB_CLOSE`라 **닫히는 순간에도** 남은 프로세스가 끝난다(일찍 돌아가는 오류 길 · 앱 비정상
//!   종료 = 커널이 핸들을 닫음).
//!
//! 한계: `spawn`과 `attach` 사이(수십 µs)에 자식이 이미 띄운 손자는 밖에 남는다(std로는 멈춘 채 띄울 수 없다). 넣기 실패(Windows 7의
//! 중첩 금지)는 조용히 지나간다 = 종전처럼 직계 자식만 끝난다. 외부 crate 0(`nsql-drivers` 터널의 선언과 같은 꼴).

/// 자식 하나(와 그 후손)를 묶는 작업 개체. Unix에서는 빈 값.
pub struct KillJob(imp::Inner);

impl std::fmt::Debug for KillJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KillJob")
    }
}

impl KillJob {
    /// `child`를 새 작업 개체에 넣는다. 만들거나 넣지 못하면 빈 값(그 뒤 호출은 아무 일도 하지 않는다).
    #[must_use]
    pub fn attach(child: &std::process::Child) -> Self {
        Self(imp::attach(child))
    }

    /// 지금 든 프로세스를 전부 끝낸다. 여러 번 불러도 된다.
    pub fn kill(&self) {
        imp::kill(&self.0);
    }

    /// 지금 살아 있는 프로세스 수(시험용 · 빈 값이면 `None`).
    #[cfg(test)]
    fn active(&self) -> Option<u32> {
        imp::active(&self.0)
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;

    type Handle = *mut core::ffi::c_void;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

    #[repr(C)]
    struct BasicLimit {
        per_process_user_time: i64,
        per_job_user_time: i64,
        limit_flags: u32,
        min_ws: usize,
        max_ws: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    struct ExtLimit {
        basic: BasicLimit,
        io: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attrs: *const core::ffi::c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: u32,
            info: *const core::ffi::c_void,
            len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(h: Handle) -> i32;
        #[cfg(test)]
        fn QueryInformationJobObject(
            job: Handle,
            class: u32,
            info: *mut core::ffi::c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
    }

    /// 핸들 하나(없으면 null). 핸들은 어느 스레드에서 닫아도 된다.
    pub(super) struct Inner(Handle);
    // SAFETY: 커널 핸들 값일 뿐 — 스레드에 묶인 상태가 없다(종료 대기 스레드로 옮겨 닫는다).
    unsafe impl Send for Inner {}

    impl Drop for Inner {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: 우리가 만든 핸들을 한 번만 닫는다.
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    pub(super) fn attach(child: &std::process::Child) -> Inner {
        // SAFETY: 인자는 null(이름 없는 개체) · 우리가 만든 구조체 · 살아 있는 자식 핸들뿐.
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return Inner(std::ptr::null_mut());
            }
            let mut info: ExtLimit = std::mem::zeroed();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&info as *const ExtLimit).cast(),
                std::mem::size_of::<ExtLimit>() as u32,
            );
            if ok == 0 || AssignProcessToJobObject(h, child.as_raw_handle() as Handle) == 0 {
                CloseHandle(h);
                return Inner(std::ptr::null_mut());
            }
            Inner(h)
        }
    }

    pub(super) fn kill(j: &Inner) {
        if !j.0.is_null() {
            // SAFETY: 우리가 만든 작업 개체 — 그 안의 프로세스만 끝난다.
            unsafe {
                TerminateJobObject(j.0, 1);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn active(j: &Inner) -> Option<u32> {
        /// `JOBOBJECT_BASIC_ACCOUNTING_INFORMATION`(class 1).
        #[repr(C)]
        struct Accounting {
            times: [i64; 4],
            page_faults: u32,
            total: u32,
            active: u32,
            terminated: u32,
        }
        if j.0.is_null() {
            return None;
        }
        // SAFETY: 크기를 넘긴 우리 구조체에 커널이 채운다.
        unsafe {
            let mut a: Accounting = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                j.0,
                1,
                (&mut a as *mut Accounting).cast(),
                std::mem::size_of::<Accounting>() as u32,
                std::ptr::null_mut(),
            );
            (ok != 0).then_some(a.active)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) struct Inner;
    pub(super) fn attach(_child: &std::process::Child) -> Inner {
        Inner
    }
    pub(super) fn kill(_j: &Inner) {}
    #[cfg(test)]
    pub(super) fn active(_j: &Inner) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// 빈 값(Unix · 넣기 실패)은 어떤 호출에도 아무 일도 하지 않는다 — 살아 있는 자식은 그대로 남는다(직접 끝낸다).
    #[cfg(not(windows))]
    #[test]
    fn empty_job_is_inert() {
        let mut child = crate::net::quiet_command("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let job = KillJob::attach(&child);
        assert_eq!(job.active(), None);
        job.kill();
        assert!(
            matches!(child.try_wait(), Ok(None)),
            "빈 작업 개체는 자식을 건드리지 않는다"
        );
        child.kill().unwrap();
        child.wait().unwrap();
    }

    /// ★ T-479: 자식(cmd)이 띄운 손자(ping)까지 한 번에 끝난다 — 종전(`child.kill()`만)에는 손자가 남았다.
    #[cfg(windows)]
    #[test]
    fn kill_takes_the_grandchild_too() {
        use std::time::{Duration, Instant};
        let wait_for = |job: &KillJob, ok: &dyn Fn(u32) -> bool| {
            let t0 = Instant::now();
            loop {
                let n = job.active().unwrap();
                if ok(n) || t0.elapsed() > Duration::from_secs(10) {
                    return n;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let mut child = crate::net::quiet_command("cmd.exe")
            .args(["/C", "ping -n 60 127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let job = KillJob::attach(&child);
        // cmd + ping(콘솔 호스트가 더 붙을 수 있다) — 손자가 든 것을 본 뒤에 끝낸다.
        assert!(
            wait_for(&job, &|n| n >= 2) >= 2,
            "손자가 작업 개체에 들지 않았다"
        );
        job.kill();
        assert_eq!(wait_for(&job, &|n| n == 0), 0, "손자가 남았다");
        child.wait().unwrap();
        // 닫는 길(Drop)도 같은 결과 — 일찍 돌아가는 오류 길.
        let mut child = crate::net::quiet_command("cmd.exe")
            .args(["/C", "ping -n 60 127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        drop(KillJob::attach(&child));
        let t0 = Instant::now();
        while matches!(child.try_wait(), Ok(None)) && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            matches!(child.try_wait(), Ok(Some(_))),
            "닫힌 뒤에도 자식이 살아 있다"
        );
    }
}
