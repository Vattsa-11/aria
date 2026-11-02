#[cfg(windows)]
use std::ffi::c_void;

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
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
}

#[cfg(windows)]
pub fn rss_bytes() -> Option<u64> {
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        ..ProcessMemoryCounters::default()
    };
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        None
    } else {
        Some(counters.working_set_size as u64)
    }
}

#[cfg(target_os = "macos")]
pub fn rss_bytes() -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct TaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [i32; 2],
        system_time: [i32; 2],
        policy: i32,
        suspend_count: i32,
    }
    extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut TaskBasicInfo, count: *mut u32) -> i32;
    }
    const MACH_TASK_BASIC_INFO: u32 = 20;
    let mut info = TaskBasicInfo::default();
    let mut count = (std::mem::size_of::<TaskBasicInfo>() / 4) as u32;
    let status = unsafe { task_info(mach_task_self_, MACH_TASK_BASIC_INFO, &mut info, &mut count) };
    (status == 0).then_some(info.resident_size)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()?;
            return Some(kib * 1024);
        }
    }
    None
}

pub fn rss_mb() -> Option<f64> {
    rss_bytes().map(|bytes| bytes as f64 / (1024.0 * 1024.0))
}

pub fn rss_label() -> String {
    match rss_mb() {
        Some(mb) => format!("{mb:.1} MB"),
        None => "unavailable".to_string(),
    }
}

pub fn report() -> String {
    format!("rss {}", rss_label())
}

pub fn spawn_reporter() {
    let _ = std::thread::Builder::new()
        .name("aria-stats".into())
        .spawn(|| loop {
            emit(&format!("[stats] {}", report()));
            std::thread::sleep(std::time::Duration::from_secs(2));
        });
}

fn emit(line: &str) {
    use std::io::Write as _;
    if let Some(mut handle) = crate::proc::console_handle() {
        let _ = writeln!(handle, "{line}");
        let _ = handle.flush();
        return;
    }
    println!("{line}");
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_plausible_working_set() {
        let bytes = rss_bytes().expect("working set");
        assert!(bytes > 1024 * 1024, "rss was {bytes}");
        assert!(bytes < 64 * 1024 * 1024 * 1024, "rss was {bytes}");
        assert!(rss_label().ends_with("MB"));
    }
}
