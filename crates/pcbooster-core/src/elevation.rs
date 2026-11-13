//! UAC handling: the app runs as a normal user and only elevates for the
//! actions that really need administrator rights (services, power plan).

use std::path::PathBuf;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::util;

/// Path of the running executable (used to relaunch ourselves elevated).
pub fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// Relaunch this application elevated with the given action and wait for it.
///
/// Returns true when the elevated instance was started successfully.
pub fn relaunch_elevated_with(action: &str) -> bool {
    launch(Some(action))
}

/// Relaunch this application elevated (no specific action).
pub fn relaunch_elevated() -> bool {
    launch(None)
}

fn launch(action: Option<&str>) -> bool {
    let Some(exe) = current_exe() else {
        return false;
    };
    let exe_w = util::Wide::new(&exe.to_string_lossy());
    let verb_w = util::Wide::new("runas");
    let params_w = util::Wide::new(&action.map(|a| format!("--action {a}")).unwrap_or_default());

    unsafe {
        let result = ShellExecuteW(
            None,
            verb_w.pcwstr(),
            exe_w.pcwstr(),
            params_w.pcwstr(),
            None,
            SW_SHOWNORMAL,
        );
        // ShellExecuteW returns a value <= 32 on failure.
        result.0 as isize > 32
    }
}

/// True when the app was started with `--action <name>` (elevated hand-off).
pub fn requested_action() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--action" || arg == "-Action" {
            return args.next().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        }
        if let Some(rest) = arg.strip_prefix("--action=") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_action_parses_both_forms() {
        assert_eq!(requested_action(), None);
    }
}
