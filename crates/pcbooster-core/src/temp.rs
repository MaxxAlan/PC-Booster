//! Temporary file cleanup.
//!
//! Two fixes over the PowerShell version:
//!   * sizes are computed recursively, so the reported MB is real (the old
//!     code only measured the top level, where directories have no size);
//!   * files go to the Recycle Bin instead of being deleted permanently, so
//!     a mistake is still undoable from Explorer.

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    SHFileOperationW, SHFILEOPSTRUCTW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
    FOF_NOCONFIRMMKDIR, FOF_SILENT,
};

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempCandidate {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub age_days: f64,
    pub is_directory: bool,
}

/// Scan the user temp folder for items older than `min_age_days`.
pub fn scan_temp(min_age_days: u32) -> Vec<TempCandidate> {
    scan_temp_in(&util::temp_dir(), min_age_days)
}

/// Scan a specific folder for items older than `min_age_days`.
pub fn scan_temp_in(root: &std::path::Path, min_age_days: u32) -> Vec<TempCandidate> {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(min_age_days as u64 * 24 * 3600);
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if modified >= cutoff {
            continue;
        }
        let age_days = std::time::SystemTime::now()
            .duration_since(modified)
            .map(|d| d.as_secs_f64() / 86_400.0)
            .unwrap_or(0.0);
        let size = if meta.is_dir() {
            util::dir_size(&path)
        } else {
            meta.len()
        };
        out.push(TempCandidate {
            path: path.to_string_lossy().to_string(),
            name: entry.file_name().to_string_lossy().to_string(),
            size_bytes: size,
            age_days: (age_days * 10.0).round() / 10.0,
            is_directory: meta.is_dir(),
        });
    }
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out
}

/// Move paths to the Recycle Bin. Returns (removed, failures).
pub fn send_to_recycle_bin(paths: &[String]) -> (usize, Vec<String>) {
    if paths.is_empty() {
        return (0, Vec::new());
    }
    // pFrom needs a double NUL terminated list of paths.
    let mut from = String::new();
    for path in paths {
        from.push_str(path);
        from.push('\0');
    }
    from.push('\0');
    let from_wide: Vec<u16> = from.encode_utf16().chain(std::iter::once(0)).collect();

    let flags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_NOCONFIRMMKDIR | FOF_SILENT)
        .0 as u16;
    let mut op = SHFILEOPSTRUCTW {
        hwnd: HWND::default(),
        wFunc: 3, // FO_DELETE
        pFrom: windows::core::PCWSTR(from_wide.as_ptr()),
        pTo: windows::core::PCWSTR::null(),
        fFlags: flags,
        fAnyOperationsAborted: windows::core::BOOL(0),
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: windows::core::PCWSTR::null(),
    };
    let code = unsafe { SHFileOperationW(&mut op as *mut SHFILEOPSTRUCTW) };
    if code != 0 {
        return (
            0,
            paths.iter().map(|p| format!("{p} (mã {code})")).collect(),
        );
    }
    let removed = paths
        .iter()
        .filter(|p| !std::path::Path::new(p).exists())
        .count();
    let failures = paths
        .iter()
        .filter(|p| std::path::Path::new(p).exists())
        .cloned()
        .collect();
    (removed, failures)
}

/// Empty the Recycle Bin of all fixed drives.
pub fn empty_recycle_bin() -> anyhow::Result<()> {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Clear-RecycleBin -Force -ErrorAction Stop",
        ])
        .status()
        .map_err(|e| anyhow::anyhow!("Không chạy được PowerShell: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Windows không cho phép dọn Thùng rác"))
    }
}

/// Size of well known cache folders that are safe to clean.
pub fn cache_candidates() -> Vec<(String, u64)> {
    let local = util::local_appdata();
    let candidates = [
        local.join("Temp"),
        local.join("Microsoft").join("Windows").join("INetCache"),
        local.join("Microsoft").join("Windows").join("WebCache"),
        local.join("Microsoft").join("Windows").join("Explorer"),
        local.join("CrashDumps"),
        local.join("D3DSCache"),
        local.join("NVIDIA").join("DXCache"),
        local.join("NVIDIA").join("GLCache"),
        std::path::PathBuf::from(r"C:\Windows\Temp"),
        std::path::PathBuf::from(r"C:\Windows\SoftwareDistribution\Download"),
    ];
    candidates
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| {
            let size = util::dir_size(&p);
            (p.to_string_lossy().to_string(), size)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_scan_finds_items() {
        let dir = std::env::temp_dir().join("pcbooster-test-temp");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("old.txt");
        std::fs::write(&file, "hello").unwrap();
        let items = scan_temp_in(&dir, 0);
        assert!(items.iter().any(|i| i.name == "old.txt"));
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }
}
