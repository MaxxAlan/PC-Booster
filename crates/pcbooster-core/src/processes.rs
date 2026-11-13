//! Process enumeration, window mapping, CPU/RAM sampling and safe closing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, GetWindowTextW, PostMessageW, WM_CLOSE,
};

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub window_title: String,
    pub exe_path: String,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub threads: u32,
}

fn snapshot() -> Vec<(u32, u32, String, u32)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(handle) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(handle, &mut entry).is_ok() {
            loop {
                let name = util::from_wide_slice(&entry.szExeFile);
                out.push((
                    entry.th32ProcessID,
                    entry.th32ParentProcessID,
                    name,
                    entry.cntThreads,
                ));
                if Process32NextW(handle, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(handle);
    }
    out
}

thread_local! {
    static WINDOW_MAP: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
    static TARGET_PID: RefCell<u32> = RefCell::new(0);
    static TARGET_WINDOW: RefCell<Option<HWND>> = RefCell::new(None);
}

unsafe extern "system" fn enum_window(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let mut buffer = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buffer);
    if len <= 0 || pid == 0 {
        return BOOL(1);
    }
    let title = util::from_wide_slice(&buffer[..len as usize]);
    if title.trim().is_empty() {
        return BOOL(1);
    }
    WINDOW_MAP.with(|map| {
        map.borrow_mut().insert(pid, title);
    });
    let target = TARGET_PID.with(|t| *t.borrow());
    if target != 0 && pid == target {
        TARGET_WINDOW.with(|w| *w.borrow_mut() = Some(hwnd));
    }
    BOOL(1)
}

fn window_titles() -> HashMap<u32, String> {
    WINDOW_MAP.with(|map| map.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(enum_window), LPARAM(0));
    }
    WINDOW_MAP.with(|map| map.borrow().clone())
}

fn find_window(pid: u32) -> Option<HWND> {
    TARGET_WINDOW.with(|w| *w.borrow_mut() = None);
    TARGET_PID.with(|t| *t.borrow_mut() = pid);
    unsafe {
        let _ = EnumWindows(Some(enum_window), LPARAM(0));
    }
    TARGET_PID.with(|t| *t.borrow_mut() = 0);
    TARGET_WINDOW.with(|w| (*w.borrow()).clone())
}

/// Full image path of a process, when the caller has enough rights.
pub fn process_image_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            Default::default(),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if ok {
            Some(util::from_wide_slice(&buffer[..size as usize]))
        } else {
            None
        }
    }
}

fn working_set_mb(pid: u32) -> f64 {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return 0.0;
        };
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let ok = GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if ok {
            counters.WorkingSetSize as f64 / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

/// CPU times (kernel + user) of every process, in seconds.
fn cpu_times() -> HashMap<u32, f64> {
    let mut map = HashMap::new();
    unsafe {
        for (pid, _, _, _) in snapshot() {
            if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut creation = windows::Win32::Foundation::FILETIME::default();
                let mut exit = windows::Win32::Foundation::FILETIME::default();
                let mut kernel = windows::Win32::Foundation::FILETIME::default();
                let mut user = windows::Win32::Foundation::FILETIME::default();
                if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user).is_ok() {
                    let ticks = (kernel.dwHighDateTime as u64) << 32 | kernel.dwLowDateTime as u64;
                    let uticks = (user.dwHighDateTime as u64) << 32 | user.dwLowDateTime as u64;
                    map.insert(pid, (ticks + uticks) as f64 / 10_000_000.0);
                }
                let _ = CloseHandle(handle);
            }
        }
    }
    map
}

/// List every running process with window title, CPU and memory usage.
///
/// `sample_seconds` controls the CPU measurement window.
pub fn processes(sample_seconds: f64) -> Vec<ProcessInfo> {
    let before = cpu_times();
    if sample_seconds > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(sample_seconds));
    }
    let after = cpu_times();
    let titles = window_titles();
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0)
        .max(1.0);

    let mut out = Vec::new();
    for (pid, parent_pid, name, threads) in snapshot() {
        let cpu_percent = match (before.get(&pid), after.get(&pid)) {
            (Some(a), Some(b)) if b >= a => {
                let delta = (b - a) / sample_seconds.max(0.001) / cpu_count * 100.0;
                (delta * 10.0).round() / 10.0
            }
            _ => 0.0,
        };
        out.push(ProcessInfo {
            pid,
            parent_pid,
            name,
            window_title: titles.get(&pid).cloned().unwrap_or_default(),
            exe_path: process_image_path(pid).unwrap_or_default(),
            cpu_percent,
            memory_mb: (working_set_mb(pid) * 10.0).round() / 10.0,
            threads,
        });
    }
    out.sort_by(|a, b| b.memory_mb.total_cmp(&a.memory_mb));
    out
}

/// True when a process with this id is still running.
pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let _ = CloseHandle(handle);
    }
    snapshot().iter().any(|(p, _, _, _)| *p == pid)
}

/// Ask the window to close first, then force the process if it does not exit.
///
/// Returns a short description of what happened.
pub fn close_process(pid: u32, graceful: bool) -> anyhow::Result<String> {
    if graceful {
        if let Some(hwnd) = find_window(pid) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            let deadline = Instant::now() + Duration::from_secs(4);
            while Instant::now() < deadline {
                if !process_alive(pid) {
                    return Ok(format!("Đã đóng cửa sổ của tiến trình {pid}"));
                }
                std::thread::sleep(Duration::from_millis(120));
            }
        }
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|e| anyhow::anyhow!("Không mở được tiến trình {pid}: {e}"))?;
        let result = TerminateProcess(handle, 0);
        let _ = CloseHandle(handle);
        result.map_err(|e| anyhow::anyhow!("Không thể đóng tiến trình {pid}: {e}"))?;
    }
    Ok(format!("Đã kết thúc tiến trình {pid}"))
}

/// Memory usage of the whole machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatus {
    pub total_mb: f64,
    pub available_mb: f64,
    pub used_mb: f64,
    pub used_percent: f64,
}

pub fn memory_status() -> MemoryStatus {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_err() {
            return MemoryStatus {
                total_mb: 0.0,
                available_mb: 0.0,
                used_mb: 0.0,
                used_percent: 0.0,
            };
        }
        let total = status.ullTotalPhys as f64 / (1024.0 * 1024.0);
        let available = status.ullAvailPhys as f64 / (1024.0 * 1024.0);
        let used = total - available;
        MemoryStatus {
            total_mb: (total * 10.0).round() / 10.0,
            available_mb: (available * 10.0).round() / 10.0,
            used_mb: (used * 10.0).round() / 10.0,
            used_percent: if total > 0.0 {
                (used / total * 1000.0).round() / 10.0
            } else {
                0.0
            },
        }
    }
}
