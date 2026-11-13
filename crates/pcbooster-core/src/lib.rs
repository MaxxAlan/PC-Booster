//! PC-Booster core engine.
//!
//! Everything the app can do lives here, independent of the GUI, so the same
//! code backs the Tauri front end, the CLI and the end to end tests.

pub mod ai;
pub mod bench;
pub mod defender;
pub mod disk;
pub mod elevation;
pub mod journal;
pub mod power;
pub mod processes;
pub mod profile;
pub mod registry;
pub mod selftest;
pub mod services;
pub mod temp;
pub mod util;

use serde::{Deserialize, Serialize};

use crate::journal::RestoreState;

/// A quick health summary of the machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub memory: processes::MemoryStatus,
    pub drives: Vec<disk::DriveInfo>,
    pub startup_count: usize,
    pub startup: Vec<registry::StartupEntry>,
    pub installed_app_count: usize,
    pub ai_app_count: usize,
    pub optional_services: Vec<services::ServiceInfo>,
    pub temp_old_bytes: u64,
    pub temp_old_count: usize,
    pub active_power_scheme: String,
    pub score: u32,
    pub notes: Vec<String>,
}

/// Collect the overview (this is what the app shows on start).
pub fn overview() -> Overview {
    let memory = processes::memory_status();
    let drives = disk::drives();
    let startup = registry::startup_entries();
    let installed = registry::installed_apps();
    let ai_apps = ai::ai_apps();
    let temp_items = temp::scan_temp(7);
    let temp_old_bytes: u64 = temp_items.iter().map(|i| i.size_bytes).sum();

    let mut optional_services = Vec::new();
    for candidate in services::OPTIONAL_SERVICES {
        if let Some(info) = services::service_info(candidate.name) {
            optional_services.push(info);
        }
    }

    let active_power_scheme = power::active_scheme_guid().unwrap_or_default();
    let mut notes = Vec::new();
    let mut score: i32 = 100;

    if startup.len() > 12 {
        score -= ((startup.len() - 12) as i32 * 2).min(24);
        notes.push(format!(
            "{} mục tự khởi động cùng Windows",
            startup.len()
        ));
    }
    let c_drive = drives.iter().find(|d| d.path.eq_ignore_ascii_case("C:\\"));
    if let Some(drive) = c_drive {
        let free_percent = drive.free_bytes as f64 / drive.total_bytes.max(1) as f64 * 100.0;
        if free_percent < 10.0 {
            score -= 25;
            notes.push(format!(
                "Ổ C: chỉ còn {} GB trống ({:.1}%)",
                drive.free_bytes / (1024 * 1024 * 1024),
                free_percent
            ));
        } else if free_percent < 20.0 {
            score -= 10;
            notes.push(format!(
                "Ổ C: còn {} GB trống",
                drive.free_bytes / (1024 * 1024 * 1024)
            ));
        }
    }
    if temp_old_bytes > 2 * 1024 * 1024 * 1024 {
        score -= 10;
        notes.push(format!(
            "Tệp tạm cũ chiếm {}",
            util::human_bytes(temp_old_bytes)
        ));
    }
    if memory.used_percent > 90.0 {
        score -= 10;
        notes.push("RAM đang dùng trên 90%".to_string());
    }
    if power::high_performance_scheme(false).is_none() {
        notes.push("Gói nguồn Hiệu suất cao chưa được tạo trên máy này".to_string());
    }

    Overview {
        memory,
        drives,
        startup_count: startup.len(),
        startup,
        installed_app_count: installed.len(),
        ai_app_count: ai_apps.len(),
        optional_services,
        temp_old_count: temp_items.len(),
        temp_old_bytes,
        active_power_scheme,
        score: score.clamp(0, 100) as u32,
        notes,
    }
}

/// Apply the changes the user confirmed, recording them in the journal.
pub fn apply_service_change(
    name: &str,
    disable: bool,
    state: &mut RestoreState,
) -> anyhow::Result<String> {
    let info = services::service_info(name)
        .ok_or_else(|| anyhow::anyhow!("Không tìm thấy dịch vụ {name}"))?;
    if disable {
        if !state.services.iter().any(|s| s.name == name) {
            state.services.push(journal::ServiceChange {
                name: name.to_string(),
                display_name: info.display_name.clone(),
                previous_start_type: info.start_type.clone(),
                was_running: info.running,
            });
        }
        state.save()?;
        if info.running {
            let _ = services::stop_service(name);
        }
        services::set_startup_type(name, "Disabled")?;
        util::write_log(&format!("Disabled service {name}"));
        Ok(format!("Đã tắt dịch vụ {}", info.display_name))
    } else {
        services::set_startup_type(name, "Manual")?;
        util::write_log(&format!("Restored service {name} to Manual"));
        Ok(format!("Đã bật lại dịch vụ {}", info.display_name))
    }
}

/// Undo everything the journal remembers.
pub fn restore_all(state: &mut RestoreState) -> Vec<String> {
    let mut messages = Vec::new();
    for change in state.services.clone() {
        let target = match change.previous_start_type.as_str() {
            "Auto" => "Automatic",
            "Manual" => "Manual",
            "Disabled" => "Disabled",
            other => other,
        };
        match services::set_startup_type(&change.name, target) {
            Ok(()) => {
                if change.was_running {
                    let _ = services::start_service(&change.name);
                }
                messages.push(format!("Đã khôi phục dịch vụ {}", change.name));
            }
            Err(err) => messages.push(format!("Không khôi phục được {}: {err}", change.name)),
        }
    }
    for item in state.startup.clone() {
        match registry::restore_startup_value(&item.location, &item.name) {
            Ok(()) => messages.push(format!("Đã khôi phục mục khởi động {}", item.name)),
            Err(err) => messages.push(format!("Không khôi phục được {}: {err}", item.name)),
        }
    }
    if let Some(plan) = state.power_plan.clone() {
        match power::restore_plan(&plan) {
            Ok(()) => messages.push("Đã khôi phục gói nguồn trước đó".to_string()),
            Err(err) => messages.push(format!("Không khôi phục được gói nguồn: {err}")),
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_runs() {
        let report = overview();
        assert!(report.score <= 100);
        assert!(report.installed_app_count >= 0);
    }
}
