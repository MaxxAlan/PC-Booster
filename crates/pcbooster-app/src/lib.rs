//! Tauri application wiring: commands, state and window setup.

pub mod commands;

use pcbooster_core as core;

pub struct AppState {
    pub start_time: std::time::Instant,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            start_time: std::time::Instant::now(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::installed_apps,
            commands::ai_apps,
            commands::startup_entries,
            commands::optional_services,
            commands::temp_candidates,
            commands::clean_temp,
            commands::running_processes,
            commands::close_process,
            commands::drives,
            commands::largest_folders,
            commands::power_schemes,
            commands::switch_high_performance,
            commands::defender_status,
            commands::defender_detections,
            commands::defender_scan,
            commands::disable_service,
            commands::disable_startup,
            commands::restore_all,
            commands::journal_state,
            commands::remove_ai_app,
            commands::remove_ai_data,
            commands::elevation_state,
            commands::relaunch_elevated,
            commands::app_log,
            commands::bench,
            commands::uptime_ms,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PC-Booster");
}

/// Write a line to the application log.
pub fn log(message: &str) {
    core::util::write_log(message);
}
