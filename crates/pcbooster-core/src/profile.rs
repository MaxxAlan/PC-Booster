//! Profile resolution.
//!
//! When the app is elevated with a *different* administrator account,
//! `%USERPROFILE%`, `%TEMP%` and `%LOCALAPPDATA%` point at that admin, not
//! at the signed-in user. The interactive user's profile is resolved from the
//! running `explorer.exe`, which always lives in the console session.

use std::path::PathBuf;

use crate::processes;

pub fn current_profile() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::util::local_appdata())
}

/// Profile of the user who is currently signed in at the console.
pub fn interactive_profile() -> PathBuf {
    if let Some(path) = explorer_profile() {
        return path;
    }
    current_profile()
}

fn explorer_profile() -> Option<PathBuf> {
    let list = processes::processes(0.0);
    list.iter()
        .find(|p| p.name.eq_ignore_ascii_case("explorer.exe") && !p.exe_path.is_empty())
        .and_then(|p| PathBuf::from(&p.exe_path).parent().map(|p| p.to_path_buf()))
}
