//! Built in self test: exercises every subsystem and reports what worked.
//!
//! Used by the CLI (`pcbooster selftest`), by the Tauri app (`--selftest`)
//! and by the end to end test script.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

pub struct SelfTest {
    pub ok: bool,
    pub checks: Vec<Check>,
}

/// Run every check against the live machine.
pub fn run() -> SelfTest {
    let mut checks: Vec<Check> = Vec::new();
    let mut push = |name: &str, passed: bool, detail: String| {
        checks.push(Check {
            name: name.to_string(),
            passed,
            detail,
        });
    };

    let apps = crate::registry::installed_apps();
    push(
        "registry.installed_apps",
        !apps.is_empty(),
        format!("{} ứng dụng đã đăng ký", apps.len()),
    );
    let empty_names = apps.iter().filter(|a| a.name.trim().is_empty()).count();
    push(
        "registry.no_invalid_entries",
        empty_names == 0,
        format!("{empty_names} mục lỗi (giá trị registry không đọc được)"),
    );

    let packages = crate::registry::store_packages();
    push(
        "registry.store_packages",
        !packages.is_empty(),
        format!("{} gói Store", packages.len()),
    );

    let startup = crate::registry::startup_entries();
    push(
        "registry.startup_entries",
        !startup.is_empty(),
        format!("{} mục tự khởi động", startup.len()),
    );

    let schemes = crate::power::list_schemes();
    push(
        "power.list_schemes",
        !schemes.is_empty(),
        format!("{} gói nguồn", schemes.len()),
    );
    let active = crate::power::active_scheme_guid();
    push(
        "power.active_scheme",
        active.is_some(),
        active.clone().unwrap_or_else(|| "không đọc được".to_string()),
    );

    let processes = crate::processes::processes(0.0);
    push(
        "processes.list",
        !processes.is_empty(),
        format!("{} tiến trình", processes.len()),
    );
    let with_window = processes.iter().filter(|p| !p.window_title.is_empty()).count();
    push(
        "processes.window_titles",
        with_window > 0,
        format!("{with_window} cửa sổ"),
    );

    let memory = crate::processes::memory_status();
    push(
        "memory.status",
        memory.total_mb > 0.0,
        format!("{:.0} MB tổng", memory.total_mb),
    );

    let temp = crate::temp::scan_temp(7);
    push(
        "temp.scan",
        true,
        format!("{} mục cũ hơn 7 ngày", temp.len()),
    );

    let drives = crate::disk::drives();
    push(
        "disk.drives",
        !drives.is_empty(),
        format!("{} ổ đĩa", drives.len()),
    );

    let ai_apps = crate::ai::ai_apps();
    push(
        "ai.scan",
        true,
        format!("{} ứng dụng AI", ai_apps.len()),
    );

    let state = crate::journal::RestoreState::load();
    push(
        "journal.load",
        true,
        format!(
            "{} dịch vụ, {} mục khởi động đã ghi nhớ",
            state.services.len(),
            state.startup.len()
        ),
    );

    push(
        "elevation.detect",
        true,
        format!(
            "admin={}",
            if crate::util::is_admin() { "có" } else { "không" }
        ),
    );

    let ok = checks.iter().all(|c| c.passed);
    SelfTest { ok, checks }
}
