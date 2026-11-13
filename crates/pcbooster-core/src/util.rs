//! Shared helpers: paths, logging, string conversion, admin detection.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `%LOCALAPPDATA%\PCBooster` - state and log live here, never in %TEMP%.
/// The temp cleanup in this app deletes %TEMP% files, so keeping the log
/// there used to make the app delete its own log.
pub fn data_dir() -> PathBuf {
    local_appdata().join("PCBooster")
}

pub fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\config\systemprofile\AppData\Local"))
}

pub fn temp_dir() -> PathBuf {
    std::env::var_os("TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| local_appdata().join("Temp"))
}

pub fn log_path() -> PathBuf {
    data_dir().join("PCBooster_Log.txt")
}

pub fn state_path() -> PathBuf {
    data_dir().join("restore-state.json")
}

pub fn ensure_data_dir() {
    let _ = fs::create_dir_all(data_dir());
}

pub fn write_log(message: &str) {
    ensure_data_dir();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(file, "{stamp} - {message}");
    }
}

/// Encode a Rust string as a NUL terminated UTF-16 buffer for Win32 calls.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Owned UTF-16 buffer that can be passed to Win32 calls as `PCWSTR`.
///
/// The windows crate expects `PCWSTR`, not `&Vec<u16>`, and the buffer has
/// to outlive the call, so it is kept in a named binding.
pub struct Wide(Vec<u16>);

impl Wide {
    pub fn new(text: &str) -> Wide {
        Wide(text.encode_utf16().chain(std::iter::once(0)).collect())
    }

    pub fn pcwstr(&self) -> windows::core::PCWSTR {
        windows::core::PCWSTR(self.0.as_ptr())
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

impl std::ops::Deref for Wide {
    type Target = [u16];

    fn deref(&self) -> &[u16] {
        &self.0
    }
}

/// Decode a UTF-16 buffer (no NUL terminator) into a Rust string.
pub fn from_wide_slice(buf: &[u16]) -> String {
    let mut len = buf.len();
    while len > 0 && buf[len - 1] == 0 {
        len -= 1;
    }
    String::from_utf16_lossy(&buf[..len])
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// True when the current process holds an elevated (administrator) token.
pub fn is_admin() -> bool {
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = u32::MAX;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut std::ffi::c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// Recursively sum the size of a directory tree, skipping unreadable entries.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024 * 3), "3.0 MB");
    }

    #[test]
    fn wide_roundtrip() {
        let encoded = wide("PC-Booster");
        assert_eq!(from_wide_slice(&encoded[..encoded.len() - 1]), "PC-Booster");
    }
}
