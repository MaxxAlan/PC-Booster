//! Restore journal: every change the app makes is recorded so it can be undone.

use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceChange {
    pub name: String,
    pub display_name: String,
    pub previous_start_type: String,
    pub was_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupChange {
    pub name: String,
    pub location: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedApp {
    pub name: String,
    pub source: String,
    pub removed_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestoreState {
    pub services: Vec<ServiceChange>,
    pub power_plan: Option<String>,
    pub startup: Vec<StartupChange>,
    pub removed_apps: Vec<RemovedApp>,
    pub cleaned_temp_paths: Vec<String>,
}

impl RestoreState {
    pub fn load() -> RestoreState {
        let path = util::state_path();
        if !path.exists() {
            return RestoreState::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(err) => {
                util::write_log(&format!("Restore state unreadable: {err}"));
                RestoreState::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        util::ensure_data_dir();
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(util::state_path(), text)?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
            && self.power_plan.is_none()
            && self.startup.is_empty()
            && self.removed_apps.is_empty()
            && self.cleaned_temp_paths.is_empty()
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(util::state_path());
    }
}
