//! Windows service inspection and control.

use serde::{Deserialize, Serialize};
use windows::Win32::System::Services::{
    ChangeServiceConfigW, OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QueryServiceStatus,
    StartServiceW, ControlService, CloseServiceHandle, QUERY_SERVICE_CONFIGW, SC_HANDLE,
    SERVICE_CHANGE_CONFIG, SERVICE_ERROR, SERVICE_QUERY_CONFIG,
    SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_START_TYPE, SERVICE_STATUS, SERVICE_STOP,
    SERVICE_WIN32_OWN_PROCESS, SC_MANAGER_CONNECT,
};

use crate::util;

/// A service the app can offer to disable, with the real world impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCandidate {
    pub name: &'static str,
    pub impact: &'static str,
    /// Only services that are safe by default are recommended.
    pub recommended: bool,
}

/// Optional services, each with an honest description of what disabling does.
///
/// SysMain, WSearch and XblGameSave are *not* recommended: on modern
/// Windows they do not make the machine faster, and disabling them breaks
/// search, prefetch behaviour and game cloud saves.
pub const OPTIONAL_SERVICES: &[ServiceCandidate] = &[
    ServiceCandidate {
        name: "Fax",
        impact: "Dịch vụ fax, gần như không dùng trên máy tính cá nhân. Tắt an toàn.",
        recommended: true,
    },
    ServiceCandidate {
        name: "SysMain",
        impact: "Prefetch/SysMain. Trên SSD thường gây ghi đĩa thừa chứ không làm máy nhanh hơn.",
        recommended: false,
    },
    ServiceCandidate {
        name: "WSearch",
        impact: "Windows Search. Tắt sẽ làm hỏng tìm kiếm trong Start Menu, File Explorer và Outlook.",
        recommended: false,
    },
    ServiceCandidate {
        name: "XblGameSave",
        impact: "Đồng bộ save game Xbox. Tắt sẽ ngừng lưu save game lên cloud.",
        recommended: false,
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub start_type: String,
    pub running: bool,
}

fn start_type_name(value: SERVICE_START_TYPE) -> &'static str {
    match value.0 {
        0 => "Boot",
        1 => "System",
        2 => "Auto",
        3 => "Manual",
        4 => "Disabled",
        _ => "Unknown",
    }
}

fn status_name(value: u32) -> &'static str {
    match value {
        1 => "Stopped",
        2 => "StartPending",
        3 => "StopPending",
        4 => "Running",
        5 => "ContinuePending",
        6 => "PausePending",
        7 => "Paused",
        _ => "Unknown",
    }
}

fn manager() -> Option<SC_HANDLE> {
    unsafe { OpenSCManagerW(None, None, SC_MANAGER_CONNECT).ok() }
}

/// Read the configuration and status of one service.
pub fn service_info(name: &str) -> Option<ServiceInfo> {
    unsafe {
        let sc = manager()?;
        let name_w = util::Wide::new(name);
        let handle = OpenServiceW(
            sc,
            name_w.pcwstr(),
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        )
        .ok()?;
        let _ = CloseServiceHandle(sc);

        let mut needed = 0u32;
        let _ = QueryServiceConfigW(handle, None, 0, &mut needed);
        let mut buffer = vec![0u8; needed.max(1) as usize];
        let config = QueryServiceConfigW(handle, Some(buffer.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW), buffer.len() as u32, &mut needed);

        let mut display_name = String::new();
        let mut start_type = SERVICE_START_TYPE(2);
        if config.is_ok() {
            let cfg = &*(buffer.as_ptr() as *const QUERY_SERVICE_CONFIGW);
            start_type = cfg.dwStartType;
            if !cfg.lpDisplayName.is_null() {
                let offset = cfg.lpDisplayName.0 as usize - buffer.as_ptr() as usize;
                if offset + 2 <= buffer.len() {
                    let wide_slice =
                        std::slice::from_raw_parts(cfg.lpDisplayName.0 as *const u16, buffer.len() / 2 - offset / 2);
                    display_name = util::from_wide_slice(wide_slice);
                }
            }
        }
        // The buffer is owned by this function, Windows only writes into it.

        let mut status = SERVICE_STATUS::default();
        let status_ok = QueryServiceStatus(handle, &mut status).is_ok();

        let _ = CloseServiceHandle(handle);

        Some(ServiceInfo {
            name: name.to_string(),
            display_name,
            status: if status_ok {
                status_name(status.dwCurrentState.0).to_string()
            } else {
                "Unknown".to_string()
            },
            start_type: start_type_name(start_type).to_string(),
            running: status_ok && status.dwCurrentState.0 == 4,
        })
    }
}

/// Change the startup type of a service.
pub fn set_startup_type(name: &str, start_type: &str) -> anyhow::Result<()> {
    let value = match start_type.to_ascii_lowercase().as_str() {
        "automatic" | "auto" => SERVICE_START_TYPE(2),
        "manual" => SERVICE_START_TYPE(3),
        "disabled" => SERVICE_START_TYPE(4),
        other => return Err(anyhow::anyhow!("Loại khởi động không hợp lệ: {other}")),
    };
    unsafe {
        let sc = manager().ok_or_else(|| anyhow::anyhow!("Không mở được SCM"))?;
        let name_w = util::Wide::new(name);
        let handle = OpenServiceW(sc, name_w.pcwstr(), SERVICE_CHANGE_CONFIG)
            .map_err(|e| anyhow::anyhow!("Không mở được dịch vụ {name}: {e}"))?;
        let result = ChangeServiceConfigW(
            handle,
            SERVICE_WIN32_OWN_PROCESS,
            value,
            SERVICE_ERROR(1),
            None::<&windows::core::PCWSTR>,
            None::<&windows::core::PCWSTR>,
            None,
            None::<&windows::core::PCWSTR>,
            None::<&windows::core::PCWSTR>,
            None::<&windows::core::PCWSTR>,
            None::<&windows::core::PCWSTR>,
        );
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(sc);
        result.map_err(|e| anyhow::anyhow!("Không đổi được kiểu khởi động của {name}: {e}"))
    }
}

/// Stop a running service.
pub fn stop_service(name: &str) -> anyhow::Result<()> {
    unsafe {
        let sc = manager().ok_or_else(|| anyhow::anyhow!("Không mở được SCM"))?;
        let name_w = util::Wide::new(name);
        let handle = OpenServiceW(sc, name_w.pcwstr(), SERVICE_STOP)
            .map_err(|e| anyhow::anyhow!("Không mở được dịch vụ {name}: {e}"))?;
        let mut status = SERVICE_STATUS::default();
        let result = ControlService(handle, 1, &mut status); // SERVICE_CONTROL_STOP
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(sc);
        result.map_err(|e| anyhow::anyhow!("Không dừng được dịch vụ {name}: {e}"))
    }
}

/// Start a stopped service.
pub fn start_service(name: &str) -> anyhow::Result<()> {
    unsafe {
        let sc = manager().ok_or_else(|| anyhow::anyhow!("Không mở được SCM"))?;
        let name_w = util::Wide::new(name);
        let handle = OpenServiceW(sc, name_w.pcwstr(), SERVICE_START)
            .map_err(|e| anyhow::anyhow!("Không mở được dịch vụ {name}: {e}"))?;
        let result = StartServiceW(handle, None);
        let _ = CloseServiceHandle(handle);
        let _ = CloseServiceHandle(sc);
        result.map_err(|e| anyhow::anyhow!("Không khởi động được dịch vụ {name}: {e}"))
    }
}

/// Look up the service configuration buffer size helper used by tests.
#[allow(dead_code)]
pub fn service_config_size() -> usize {
    std::mem::size_of::<QUERY_SERVICE_CONFIGW>()
}
