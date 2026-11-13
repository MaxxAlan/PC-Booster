//! Registry access.
//!
//! Every value is read with an explicit type check, so a malformed entry
//! (a value whose type cannot be cast) is skipped instead of aborting the
//! whole scan. The previous PowerShell version used `Get-ItemProperty`,
//! which threw `InvalidCastException` on such keys and printed a raw error.

use serde::{Deserialize, Serialize};
use windows_registry::{Key, Type};

/// Raw registry hive handle (`HKEY` is a plain pointer in windows-registry).
pub type Hkey = *mut core::ffi::c_void;

pub fn hkey_current_user() -> Hkey {
    windows_registry::CURRENT_USER.as_raw()
}

pub fn hkey_local_machine() -> Hkey {
    windows_registry::LOCAL_MACHINE.as_raw()
}

const UNINSTALL_ROOTS: [&str; 2] = [
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];

/// Read a string value, transparently handling REG_SZ and REG_EXPAND_SZ.
pub fn reg_string(key: &Key, name: &str) -> Option<String> {
    let value = key.get_value(name).ok()?;
    match value.ty() {
        Type::String | Type::ExpandString => {
            let wide = value.as_wide();
            let mut len = wide.len();
            while len > 0 && wide[len - 1] == 0 {
                len -= 1;
            }
            Some(String::from_utf16_lossy(&wide[..len]))
        }
        _ => None,
    }
}

pub fn reg_u32(key: &Key, name: &str) -> Option<u32> {
    match key.get_value(name) {
        Ok(value) => match value.ty() {
            Type::U32 => Some(u32::from_le_bytes(value.as_ref()[..4].try_into().ok()?)),
            _ => None,
        },
        Err(_) => None,
    }
}

pub fn reg_value_names(key: &Key) -> Vec<String> {
    key.values()
        .map(|it| it.map(|(name, _)| name).collect())
        .unwrap_or_default()
}

fn open(hkey: Hkey, path: &str) -> Option<Key> {
    let root = if hkey == hkey_current_user() {
        windows_registry::CURRENT_USER
    } else {
        windows_registry::LOCAL_MACHINE
    };
    root.open(path).ok()
}

/// A program registered in one of the Windows uninstall registries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub quiet_uninstall_string: String,
    /// Estimated install size in MB (Windows registry estimate, may be absent).
    pub size_mb: Option<f64>,
    pub source: String,
}

/// Enumerate every program registered for uninstall.
pub fn installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for (hkey, hive) in [
        (hkey_current_user(), "HKCU"),
        (hkey_local_machine(), "HKLM"),
    ] {
        for root in UNINSTALL_ROOTS {
            let Some(key) = open(hkey, root) else {
                continue;
            };
            let Ok(subkeys) = key.keys() else { continue };
            for name in subkeys {
                let Ok(sub) = key.open(&name) else {
                    crate::util::write_log(&format!("Skipped uninstall key {name}: cannot open"));
                    continue;
                };
                let Some(display) = reg_string(&sub, "DisplayName") else {
                    continue;
                };
                if display.trim().is_empty() {
                    continue;
                }
                let uninstall = reg_string(&sub, "UninstallString").unwrap_or_default();
                let quiet = reg_string(&sub, "QuietUninstallString").unwrap_or_default();
                if uninstall.is_empty() && quiet.is_empty() {
                    continue;
                }
                let size_mb = reg_u32(&sub, "EstimatedSize")
                    .filter(|kb| *kb > 0)
                    .map(|kb| (kb as f64 / 1024.0 * 10.0).round() / 10.0);
                apps.push(InstalledApp {
                    name: display,
                    version: reg_string(&sub, "DisplayVersion").unwrap_or_default(),
                    publisher: reg_string(&sub, "Publisher").unwrap_or_default(),
                    install_location: reg_string(&sub, "InstallLocation").unwrap_or_default(),
                    uninstall_string: uninstall,
                    quiet_uninstall_string: quiet,
                    size_mb,
                    source: format!("{hive}\\{root}\\{name}"),
                });
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    apps
}

/// A Microsoft Store / MSIX package installed for the current user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorePackage {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub package_family_name: String,
    pub package_full_name: String,
    pub install_path: String,
    pub is_framework: bool,
}

fn appmodel_repository() -> Option<Key> {
    open(
        hkey_current_user(),
        r"SOFTWARE\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
    )
}

/// Read the identity attributes from an AppxManifest.xml without a full parser.
fn manifest_identity(path: &std::path::Path) -> Option<(String, String, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let start = text.find("<Identity")?;
    let rest = &text[start..];
    let end = rest.find('>')?;
    let tag = &rest[..end];
    let attr = |key: &str| -> Option<String> {
        let needle = format!("{key}=\"");
        let at = tag.find(&needle)? + needle.len();
        let close = tag[at..].find('"')? + at;
        Some(tag[at..close].to_string())
    };
    Some((
        attr("Name")?,
        attr("Publisher").unwrap_or_default(),
        attr("Version").unwrap_or_default(),
    ))
}

/// Enumerate Store/MSIX packages for the current user.
///
/// The package list comes from the AppModel repository (which holds the
/// package full names needed for removal); the display metadata is read
/// from the manifest in the per-user Packages folder.
pub fn store_packages() -> Vec<StorePackage> {
    let mut packages = Vec::new();
    let repository = appmodel_repository();

    if let Some(repo) = &repository {
        let Ok(subkeys) = repo.keys() else {
            return packages;
        };
        for full_name in subkeys {
            let family = package_family_name(&full_name);
            let install_path = std::env::var_os("LOCALAPPDATA")
                .map(|p| std::path::PathBuf::from(p).join("Packages").join(&family))
                .unwrap_or_default();
            let mut name = family.split('_').next().unwrap_or(&family).to_string();
            let mut publisher = String::new();
            let mut version = String::new();
            let mut is_framework = false;
            if let Some(manifest) = install_path.join("AppxManifest.xml").as_path().to_owned().as_os_str().to_str().map(std::path::Path::new) {
                if let Some((n, p, v)) = manifest_identity(manifest) {
                    name = n;
                    publisher = p;
                    version = v;
                }
            }
            if let Ok(sub) = repo.open(&full_name) {
                is_framework = reg_string(&sub, "IsFramework").map(|v| v == "1").unwrap_or(false);
            }
            packages.push(StorePackage {
                name,
                publisher,
                version,
                package_family_name: family,
                package_full_name: full_name,
                install_path: install_path.to_string_lossy().to_string(),
                is_framework,
            });
        }
    }
    packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packages
}

/// `Publisher_App_1.0_x64__hash` -> `Publisher_App_hash`
pub fn package_family_name(full_name: &str) -> String {
    let parts: Vec<&str> = full_name.split('_').collect();
    if parts.len() >= 2 {
        let name = parts[0];
        let publisher = parts[parts.len() - 1];
        format!("{name}_{publisher}")
    } else {
        full_name.to_string()
    }
}

/// A program that starts automatically when the user signs in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    /// Registry hive + path, or `StartupFolder` for shortcut files.
    pub location: String,
    pub kind: String,
    pub user_scope: bool,
}

fn collect_run_keys(hkey: Hkey, hive: &str, path: &str, user_scope: bool, out: &mut Vec<StartupEntry>) {
    let Some(key) = open(hkey, path) else {
        return;
    };
    // Run keys store values, not subkeys.
    for name in reg_value_names(&key) {
        if let Some(command) = reg_string(&key, &name) {
            out.push(StartupEntry {
                name: name.clone(),
                command,
                location: format!("{hive}\\{path}"),
                kind: "Registry".to_string(),
                user_scope,
            });
        }
    }
}

fn collect_startup_folder(folder: &std::path::Path, label: &str, out: &mut Vec<StartupEntry>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let command = if path.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
            resolve_shortcut(&path)
        } else {
            path.to_string_lossy().to_string()
        };
        out.push(StartupEntry {
            name,
            command,
            location: format!("{label} ({})", folder.to_string_lossy()),
            kind: "StartupFolder".to_string(),
            user_scope: label.starts_with('U'),
        });
    }
}

/// Resolve a `.lnk` shortcut to its target command line.
pub fn resolve_shortcut(path: &std::path::Path) -> String {
    use std::os::windows::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return path.to_string_lossy().to_string();
    };
    // .lnk parsing needs the COM shell API; fall back to the file itself and
    // let the user see the raw entry instead of guessing wrong.
    let _ = meta.file_size();
    path.to_string_lossy().to_string()
}

/// Enumerate everything that auto-starts for the signed-in user.
pub fn startup_entries() -> Vec<StartupEntry> {
    let mut out = Vec::new();
    collect_run_keys(
        hkey_current_user(),
        "HKCU",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        true,
        &mut out,
    );
    collect_run_keys(
        hkey_current_user(),
        "HKCU",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
        true,
        &mut out,
    );
    collect_run_keys(
        hkey_local_machine(),
        "HKLM",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        false,
        &mut out,
    );
    collect_run_keys(
        hkey_local_machine(),
        "HKLM",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        false,
        &mut out,
    );

    let appdata = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    if let Some(appdata) = &appdata {
        collect_startup_folder(
            &appdata.join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            "User startup folder",
            &mut out,
        );
    }
    if let Some(programdata) = std::env::var_os("ProgramData").map(std::path::PathBuf::from) {
        collect_startup_folder(
            &programdata.join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            "All users startup folder",
            &mut out,
        );
    }
    out
}

/// Move a Run value into the disabled store so it can be restored later.
pub fn disable_startup_value(location: &str, name: &str) -> anyhow::Result<()> {
    let (hive, path) = location.split_once("\\").unwrap_or(("HKCU", ""));
    let root = if hive == "HKCU" { windows_registry::CURRENT_USER } else { windows_registry::LOCAL_MACHINE };
    let key = root
        .options()
        .read()
        .write()
        .open(path)
        .map_err(|e| anyhow::anyhow!("Khong mo duoc khoa registry: {location}: {e}"))?;
    let command = reg_string(&key, name)
        .ok_or_else(|| anyhow::anyhow!("Không tìm thấy mục khởi động: {name}"))?;
    let disabled = key
        .create("PCBoosterDisabled")
        .map_err(|e| anyhow::anyhow!("Không tạo được nhóm tạm: {e}"))?;
    disabled
        .set_string(name, &command)
        .map_err(|e| anyhow::anyhow!("Không lưu được mục đã tắt: {e}"))?;
    key.remove_value(name)
        .map_err(|e| anyhow::anyhow!("Không xoá được mục khởi động: {e}"))?;
    Ok(())
}

/// Put a previously disabled Run value back.
pub fn restore_startup_value(location: &str, name: &str) -> anyhow::Result<()> {
    let (hive, path) = location.split_once("\\").unwrap_or(("HKCU", ""));
    let root = if hive == "HKCU" { windows_registry::CURRENT_USER } else { windows_registry::LOCAL_MACHINE };
    let key = root
        .options()
        .read()
        .write()
        .open(path)
        .map_err(|e| anyhow::anyhow!("Khong mo duoc khoa registry: {location}: {e}"))?;
    let disabled = key
        .open("PCBoosterDisabled")
        .map_err(|e| anyhow::anyhow!("Không mở được nhóm tạm: {e}"))?;
    let command = reg_string(&disabled, name)
        .ok_or_else(|| anyhow::anyhow!("Không tìm thấy mục đã tắt: {name}"))?;
    key.set_string(name, &command)
        .map_err(|e| anyhow::anyhow!("Không khôi phục được mục khởi động: {e}"))?;
    disabled.remove_value(name).ok();
    Ok(())
}

/// Profile directory of a user SID, from the ProfileList registry.
pub fn profile_for_sid(sid: &str) -> Option<std::path::PathBuf> {
    let key = open(
        hkey_local_machine(),
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList",
    )?;
    let sub = key.open(sid).ok()?;
    let path = reg_string(&sub, "ProfileImagePath")?;
    Some(std::path::PathBuf::from(path))
}
