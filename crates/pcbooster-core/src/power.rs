//! Power plan handling.
//!
//! `powercfg` output is localized, so the old code that matched the English
//! label "Power Scheme GUID:" silently failed on Vietnamese Windows.
//! Here only the GUID and the name in parentheses are matched, which works
//! in every locale.

use serde::{Deserialize, Serialize};

use crate::util;

pub const HIGH_PERFORMANCE_GUID: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const BALANCED_GUID: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerScheme {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

fn is_guid(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Extract every `GUID (name)` pair from a `powercfg /list` line.
///
/// The line is scanned for a 36 character GUID window instead of matching a
/// label, because the label is localized ("Power Scheme GUID:" in English,
/// "Sơ đồ nguồn GUID:" in Vietnamese).
pub fn parse_scheme_line(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    if bytes.len() < 36 {
        return None;
    }
    for index in 0..=(bytes.len() - 36) {
        if !line.is_char_boundary(index) || !line.is_char_boundary(index + 36) {
            continue;
        }
        let candidate = &line[index..index + 36];
        if !is_guid(candidate) {
            continue;
        }
        let rest = &line[index + 36..];
        let open = rest.find('(')?;
        let close = rest[open..].find(')')? + open;
        let name = rest[open + 1..close].trim().to_string();
        return Some((candidate.to_string(), name));
    }
    None
}

/// All power schemes known to Windows.
pub fn list_schemes() -> Vec<PowerScheme> {
    let output = std::process::Command::new("powercfg").arg("/list").output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut schemes = Vec::new();
    for line in text.lines() {
        if let Some((guid, name)) = parse_scheme_line(line) {
            let active = line.trim_end().ends_with('*');
            schemes.push(PowerScheme { guid, name, active });
        }
    }
    schemes
}

/// GUID of the power scheme that is currently active.
pub fn active_scheme_guid() -> Option<String> {
    let output = std::process::Command::new("powercfg")
        .arg("/getactivescheme")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some((guid, _)) = parse_scheme_line(line) {
            return Some(guid);
        }
        // The active scheme line may not repeat the name in parentheses.
        if let Some(index) = (0..=line.len().saturating_sub(36)).find(|i| {
            line.is_char_boundary(*i)
                && line.is_char_boundary(*i + 36)
                && is_guid(&line[*i..*i + 36])
        }) {
            return Some(line[index..index + 36].to_string());
        }
    }
    None
}

/// Find the High performance scheme, creating it when Windows hides it.
pub fn high_performance_scheme(create_if_missing: bool) -> Option<PowerScheme> {
    let schemes = list_schemes();
    if let Some(found) = schemes.iter().find(|s| {
        s.guid.eq_ignore_ascii_case(HIGH_PERFORMANCE_GUID)
            || s.name.to_lowercase().contains("high performance")
            || s.name.to_lowercase().contains("hiệu suất cao")
    }) {
        return Some(found.clone());
    }
    if !create_if_missing {
        return None;
    }
    // Windows hides the built-in High performance scheme by default.
    let output = std::process::Command::new("powercfg")
        .args(["/duplicatescheme", HIGH_PERFORMANCE_GUID])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let new_guid = text.split_whitespace().find(|token| is_guid(token))?;
    util::write_log(&format!("Created High performance power scheme {new_guid}"));
    list_schemes()
        .into_iter()
        .find(|s| s.guid.eq_ignore_ascii_case(new_guid))
}

fn set_active(guid: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("powercfg")
        .args(["/setactive", guid])
        .status()
        .map_err(|e| anyhow::anyhow!("Không chạy được powercfg: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Windows không áp dụng được gói nguồn {guid}"))
    }
}

/// Switch to the High performance power plan.
pub fn switch_to_high_performance() -> anyhow::Result<String> {
    let scheme = high_performance_scheme(true)
        .ok_or_else(|| anyhow::anyhow!("Không tạo được gói nguồn Hiệu suất cao"))?;
    set_active(&scheme.guid)?;
    util::write_log(&format!("Changed power plan to {} ({})", scheme.name, scheme.guid));
    Ok(scheme.name)
}

/// Restore a previously active power plan.
pub fn restore_plan(guid: &str) -> anyhow::Result<()> {
    set_active(guid)?;
    util::write_log(&format!("Restored power plan {guid}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_line() {
        let line = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *";
        let (guid, name) = parse_scheme_line(line).expect("should parse");
        assert_eq!(guid, "381b4222-f694-41f0-9685-ff5bb260df2e");
        assert_eq!(name, "Balanced");
    }

    #[test]
    fn parses_vietnamese_line() {
        let line = "Sơ đồ nguồn GUID: 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (Hiệu suất cao) *";
        let (guid, name) = parse_scheme_line(line).expect("should parse");
        assert_eq!(guid, "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
        assert_eq!(name, "Hiệu suất cao");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_scheme_line("Existing Power Schemes (* Active)").is_none());
        assert!(parse_scheme_line("no guid here").is_none());
    }
}
