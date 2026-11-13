//! Tauri commands: a thin bridge between the web UI and the core engine.

use serde::Serialize;
use tauri::State;

use pcbooster_core as core;

use crate::AppState;

#[derive(Serialize)]
pub struct ElevationState {
    pub is_admin: bool,
    pub executable: String,
}

#[tauri::command]
pub fn overview() -> core::Overview {
    core::overview()
}

#[tauri::command]
pub fn installed_apps() -> Vec<core::registry::InstalledApp> {
    core::registry::installed_apps()
}

#[tauri::command]
pub fn ai_apps() -> Vec<core::ai::AiApp> {
    core::ai::ai_apps()
}

#[tauri::command]
pub fn startup_entries() -> Vec<core::registry::StartupEntry> {
    core::registry::startup_entries()
}

#[derive(Serialize)]
pub struct ServiceRow {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub start_type: String,
    pub running: bool,
    pub impact: String,
    pub recommended: bool,
    pub exists: bool,
}

#[tauri::command]
pub fn optional_services() -> Vec<ServiceRow> {
    core::services::OPTIONAL_SERVICES
        .iter()
        .map(|candidate| {
            let info = core::services::service_info(candidate.name);
            ServiceRow {
                name: candidate.name.to_string(),
                display_name: info
                    .as_ref()
                    .map(|i| i.display_name.clone())
                    .unwrap_or_else(|| candidate.name.to_string()),
                status: info.as_ref().map(|i| i.status.clone()).unwrap_or_default(),
                start_type: info.as_ref().map(|i| i.start_type.clone()).unwrap_or_default(),
                running: info.as_ref().map(|i| i.running).unwrap_or(false),
                impact: candidate.impact.to_string(),
                recommended: candidate.recommended,
                exists: info.is_some(),
            }
        })
        .collect()
}

#[tauri::command]
pub fn temp_candidates(days: Option<u32>) -> Vec<core::temp::TempCandidate> {
    core::temp::scan_temp(days.unwrap_or(7))
}

#[tauri::command]
pub fn clean_temp(paths: Vec<String>) -> Result<(usize, Vec<String>), String> {
    let (removed, failures) = core::temp::send_to_recycle_bin(&paths);
    core::util::write_log(&format!("Moved {removed} temp items to the Recycle Bin"));
    Ok((removed, failures))
}

#[tauri::command]
pub fn running_processes() -> Vec<core::processes::ProcessInfo> {
    core::processes::processes(1.0)
}

#[tauri::command]
pub fn close_process(pid: u32, graceful: Option<bool>) -> Result<String, String> {
    core::processes::close_process(pid, graceful.unwrap_or(true))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn drives() -> Vec<core::disk::DriveInfo> {
    core::disk::drives()
}

#[tauri::command]
pub fn largest_folders(root: String, limit: Option<usize>) -> Vec<core::disk::FolderSize> {
    core::disk::largest_folders(std::path::Path::new(&root), limit.unwrap_or(20), 3)
}

#[tauri::command]
pub fn power_schemes() -> Vec<core::power::PowerScheme> {
    core::power::list_schemes()
}

#[tauri::command]
pub fn switch_high_performance() -> Result<String, String> {
    core::power::switch_to_high_performance().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn defender_status() -> core::defender::DefenderStatus {
    core::defender::status()
}

#[tauri::command]
pub fn defender_detections(limit: Option<usize>) -> Vec<core::defender::ThreatDetection> {
    core::defender::detections(limit.unwrap_or(10))
}

#[tauri::command]
pub fn defender_scan(kind: String, path: Option<String>) -> Result<String, String> {
    core::defender::scan(&kind, path.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn disable_service(name: String) -> Result<String, String> {
    if !core::util::is_admin() {
        return Err(
            "Cần quyền quản trị để thay đổi dịch vụ. Hãy chạy lại ứng dụng với quyền quản trị."
                .to_string(),
        );
    }
    let mut state = core::journal::RestoreState::load();
    core::apply_service_change(&name, true, &mut state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn disable_startup(location: String, name: String) -> Result<String, String> {
    core::registry::disable_startup_value(&location, &name).map_err(|e| e.to_string())?;
    let mut state = core::journal::RestoreState::load();
    state.startup.push(core::journal::StartupChange {
        name: name.clone(),
        location: location.clone(),
        command: String::new(),
    });
    state.save().map_err(|e| e.to_string())?;
    core::util::write_log(&format!("Disabled startup entry {name}"));
    Ok(format!("Đã tắt mục khởi động {name}"))
}

#[tauri::command]
pub fn restore_all() -> Result<Vec<String>, String> {
    let mut state = core::journal::RestoreState::load();
    let messages = core::restore_all(&mut state);
    if state.is_empty() {
        state.clear();
    }
    Ok(messages)
}

/// The change journal, so the UI can show what can be undone.
#[tauri::command]
pub fn journal_state() -> core::journal::RestoreState {
    core::journal::RestoreState::load()
}

#[tauri::command]
pub fn remove_ai_app(app: core::ai::AiApp) -> Result<String, String> {
    let result = core::ai::remove(&app).map_err(|e| e.to_string())?;
    let mut state = core::journal::RestoreState::load();
    state.removed_apps.push(core::journal::RemovedApp {
        name: app.name.clone(),
        source: app.kind.clone(),
        removed_at: chrono::Local::now().to_string(),
    });
    state.save().map_err(|e| e.to_string())?;
    core::util::write_log(&format!("Removed AI app {}", app.name));
    Ok(result)
}

#[tauri::command]
pub fn remove_ai_data(paths: Vec<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let (removed, failed) = core::ai::remove_residual_data(&paths);
    for path in &removed {
        core::util::write_log(&format!("Removed residual data {path}"));
    }
    Ok((removed, failed))
}

#[tauri::command]
pub fn elevation_state() -> ElevationState {
    ElevationState {
        is_admin: core::util::is_admin(),
        executable: core::elevation::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

#[tauri::command]
pub fn relaunch_elevated() -> Result<(), String> {
    core::elevation::relaunch_elevated().then_some(()).ok_or_else(|| {
        "Không mở được cửa sổ UAC. Hãy chạy ứng dụng bằng tài khoản quản trị.".to_string()
    })
}

#[tauri::command]
pub fn app_log(limit: Option<usize>) -> Vec<String> {
    let path = core::util::log_path();
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let take = limit.unwrap_or(200);
    lines
        .iter()
        .rev()
        .take(take)
        .rev()
        .map(|l| l.to_string())
        .collect()
}

#[tauri::command]
pub fn bench(iterations: Option<usize>) -> core::bench::BenchReport {
    core::bench::run(iterations.unwrap_or(3))
}

#[tauri::command]
pub fn uptime_ms(state: State<'_, AppState>) -> u128 {
    state.start_time.elapsed().as_millis()
}
