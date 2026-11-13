//! Microsoft Defender integration.
//!
//! Status and threat history come from the Defender PowerShell cmdlets (they
//! are the only supported read API), but scans are started through
//! `MpCmdRun.exe` so the app does not pay the PowerShell startup cost for a
//! long running scan.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefenderStatus {
    pub available: bool,
    pub am_running_mode: String,
    pub antivirus_enabled: bool,
    pub real_time_protection: bool,
    pub signature_last_updated: String,
    pub signature_age_days: f64,
    pub quick_scan_age_days: f64,
    pub full_scan_age_days: f64,
    pub error: String,
}

impl Default for DefenderStatus {
    fn default() -> Self {
        Self {
            available: false,
            am_running_mode: String::new(),
            antivirus_enabled: false,
            real_time_protection: false,
            signature_last_updated: String::new(),
            signature_age_days: 0.0,
            quick_scan_age_days: 0.0,
            full_scan_age_days: 0.0,
            error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatDetection {
    pub threat_name: String,
    pub detected_at: String,
    pub action_success: bool,
    pub resources: String,
    pub active: bool,
}

fn powershell_json(script: &str) -> Option<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Read the current Defender state.
pub fn status() -> DefenderStatus {
    let script = "$s = Get-MpComputerStatus -ErrorAction Stop; [pscustomobject]@{ \
        am = $s.AMRunningMode; av = $s.AntivirusEnabled; rtp = $s.RealTimeProtectionEnabled; \
        sig = $s.AntivirusSignatureLastUpdated; sigAge = $s.AntivirusSignatureAge; \
        quick = $s.QuickScanAge; full = $s.FullScanAge } | ConvertTo-Json -Compress";
    let Some(raw) = powershell_json(script) else {
        return DefenderStatus {
            error: "Không đọc được trạng thái Microsoft Defender trên máy này.".to_string(),
            ..Default::default()
        };
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => {
            return DefenderStatus {
                error: "Defender trả về dữ liệu không đọc được.".to_string(),
                ..Default::default()
            }
        }
    };
    let number = |key: &str| value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let text = |key: &str| {
        value
            .get(key)
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    DefenderStatus {
        available: true,
        am_running_mode: text("am"),
        antivirus_enabled: value.get("av").and_then(|v| v.as_bool()).unwrap_or(false),
        real_time_protection: value.get("rtp").and_then(|v| v.as_bool()).unwrap_or(false),
        signature_last_updated: text("sig"),
        signature_age_days: number("sigAge"),
        quick_scan_age_days: number("quick"),
        full_scan_age_days: number("full"),
        error: String::new(),
    }
}

fn mpcamdrun_path() -> Option<std::path::PathBuf> {
    let candidates = [
        std::path::PathBuf::from(r"C:\Program Files\Windows Defender\MpCmdRun.exe"),
        std::path::PathBuf::from(r"C:\ProgramData\Microsoft\Windows Defender\Platform\CurrentVersion\MpCmdRun.exe"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // Fall back to the newest platform folder.
    let platform = std::path::PathBuf::from(r"C:\ProgramData\Microsoft\Windows Defender\Platform");
    if let Ok(entries) = std::fs::read_dir(platform) {
        let mut folders: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("MpCmdRun.exe").exists())
            .collect();
        folders.sort();
        folders.pop();
    }
    None
}

/// Start a Defender scan. Returns the raw MpCmdRun output.
pub fn scan(kind: &str, path: Option<&str>) -> anyhow::Result<String> {
    let exe = mpcamdrun_path()
        .ok_or_else(|| anyhow::anyhow!("Không tìm thấy Microsoft Defender (MpCmdRun.exe)"))?;
    let mut command = std::process::Command::new(exe);
    command.arg("-Scan");
    match kind {
        "quick" => {
            command.args(["-ScanType", "1"]);
        }
        "full" => {
            command.args(["-ScanType", "2"]);
        }
        "custom" => {
            let target = path.ok_or_else(|| anyhow::anyhow!("Thiếu đường dẫn cần quét"))?;
            command.args(["-ScanType", "3", "-File", target]);
        }
        other => return Err(anyhow::anyhow!("Kiểu quét không hợp lệ: {other}")),
    }
    let output = command
        .output()
        .map_err(|e| anyhow::anyhow!("Không chạy được MpCmdRun: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    crate::util::write_log(&format!("Started Defender {kind} scan"));
    if output.status.success() {
        Ok(text)
    } else {
        Err(anyhow::anyhow!("Defender không khởi chạy được quét: {text}"))
    }
}

/// Recent Defender detections.
pub fn detections(limit: usize) -> Vec<ThreatDetection> {
    let script = format!(
        "$d = Get-MpThreatDetection -ErrorAction SilentlyContinue | Sort-Object InitialDetectionTime -Descending | Select-Object -First {limit}; \
         $d | ForEach-Object {{ [pscustomobject]@{{ name = $_.ThreatName; at = $_.InitialDetectionTime; \
         ok = $_.ActionSuccess; active = $_.IsActive; res = ($_.Resources -join '; ') }} }} | ConvertTo-Json -Compress"
    );
    let Some(raw) = powershell_json(&script) else {
        return Vec::new();
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    items
        .into_iter()
        .filter(|v| v.is_object())
        .map(|v| ThreatDetection {
            threat_name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            detected_at: v.get("at").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            action_success: v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
            resources: v.get("res").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            active: v.get("active").and_then(|x| x.as_bool()).unwrap_or(false),
        })
        .collect()
}
