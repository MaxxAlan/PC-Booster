//! Detection and removal of AI apps, AI coding agents and their data.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::registry::{self, InstalledApp, StorePackage};
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiApp {
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// app | appx | npm | pnpm | bun | native-cli
    pub kind: String,
    pub size_mb: Option<f64>,
    pub uninstall: String,
    pub package: String,
    pub package_full_name: String,
    pub package_family_name: String,
    pub residual_paths: Vec<String>,
}

fn app_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(ollama|lm studio|lmstudio|jan(\s+ai)?|anything\s*llm|open\s*webui|chatgpt|openai|claude(\s*code)?|anthropic|gemini|google\s+ai|cursor|windsurf|continue(\.dev)?|cline|roo\s*code|kilo\s*code|github\s+copilot|microsoft\s+copilot|tabnine|codeium|amazon\s+q|kiro|aider|codex|opencode|antigravity|trae|zed|openclaw|moltbot|deepseek|perplexity|msty|gpt4all|koboldcpp)\b",
        )
        .expect("valid AI pattern")
    })
}

fn package_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)(^|/|[._-])(codex|claude(-code)?|gemini-cli|opencode(-ai)?|cline|roo-code|kilo-code|copilot|aider|qwen-code|mistral-vibe|open-interpreter|openclaw|amp)(/|[._-]|$)",
        )
        .expect("valid package pattern")
    })
}

/// Every AI app / agent found on the machine.
pub fn ai_apps() -> Vec<AiApp> {
    let mut out: Vec<AiApp> = Vec::new();

    for app in registry::installed_apps() {
        if app_pattern().is_match(&app.name) {
            out.push(from_installed(app));
        }
    }

    for package in registry::store_packages() {
        let haystack = format!("{} {}", package.name, package.package_full_name);
        if app_pattern().is_match(&haystack) {
            out.push(from_store(package));
        }
    }

    // Claude Code native Windows installer puts the binary outside npm.
    let native = util::local_appdata()
        .parent()
        .map(|_| std::path::PathBuf::from(user_profile()))
        .map(|p| p.join(".local").join("bin").join("claude.exe"))
        .unwrap_or_default();
    if native.exists() {
        out.push(AiApp {
            name: "Claude Code (native CLI)".to_string(),
            version: String::new(),
            publisher: "Native user install".to_string(),
            kind: "native-cli".to_string(),
            size_mb: std::fs::metadata(&native)
                .ok()
                .map(|m| (m.len() as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0),
            uninstall: native.to_string_lossy().to_string(),
            package: String::new(),
            package_full_name: String::new(),
            package_family_name: String::new(),
            residual_paths: Vec::new(),
        });
    }

    out.extend(npm_like_globals("npm", "npm"));
    out.extend(npm_like_globals("pnpm", "pnpm"));
    out.extend(bun_globals());

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name && a.kind == b.kind);
    for app in out.iter_mut() {
        app.residual_paths = residual_paths(app);
    }
    out
}

fn user_profile() -> String {
    std::env::var("USERPROFILE").unwrap_or_else(|_| {
        util::local_appdata()
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    })
}

fn from_installed(app: InstalledApp) -> AiApp {
    AiApp {
        name: app.name.clone(),
        version: app.version.clone(),
        publisher: app.publisher.clone(),
        kind: "app".to_string(),
        size_mb: app.size_mb,
        uninstall: if app.uninstall_string.is_empty() {
            app.quiet_uninstall_string.clone()
        } else {
            app.uninstall_string.clone()
        },
        package: String::new(),
        package_full_name: String::new(),
        package_family_name: String::new(),
        residual_paths: Vec::new(),
    }
}

fn from_store(package: StorePackage) -> AiApp {
    AiApp {
        name: format!("Store app: {}", package.name),
        version: package.version.clone(),
        publisher: package.publisher.clone(),
        kind: "appx".to_string(),
        size_mb: None,
        uninstall: String::new(),
        package: String::new(),
        package_full_name: package.package_full_name.clone(),
        package_family_name: package.package_family_name.clone(),
        residual_paths: Vec::new(),
    }
}

fn run_json(command: &str, args: &[&str]) -> Option<serde_json::Value> {
    let output = std::process::Command::new(command)
        .args(args)
        .env("npm_config_update_notifier", "false")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return None;
    }
    serde_json::from_str(&text).ok()
}

fn npm_like_globals(kind: &str, command: &str) -> Vec<AiApp> {
    let Some(value) = run_json(command, &["list", "--global", "--depth=0", "--json"]) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(deps) = value.get("dependencies").and_then(|d| d.as_object()) {
        for (name, meta) in deps {
            if !package_pattern().is_match(name) {
                continue;
            }
            out.push(AiApp {
                name: format!("{kind} global: {name}"),
                version: meta
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                publisher: format!("{kind} global"),
                kind: kind.to_string(),
                size_mb: None,
                uninstall: String::new(),
                package: name.clone(),
                package_full_name: String::new(),
                package_family_name: String::new(),
                residual_paths: Vec::new(),
            });
        }
    }
    out
}

fn bun_globals() -> Vec<AiApp> {
    let Ok(output) = std::process::Command::new("bun")
        .args(["pm", "ls", "-g"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"(?<package>@[^/\s]+/[^@\s]+|[a-z0-9._-]+)@(?<version>\d[^\s]*)").unwrap();
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(caps) = re.captures(line) else {
            continue;
        };
        let package = caps.name("package").map(|m| m.as_str()).unwrap_or_default();
        if !package_pattern().is_match(package) {
            continue;
        }
        out.push(AiApp {
            name: format!("bun global: {package}"),
            version: caps.name("version").map(|m| m.as_str()).unwrap_or_default().to_string(),
            publisher: "bun global".to_string(),
            kind: "bun".to_string(),
            size_mb: None,
            uninstall: String::new(),
            package: package.to_string(),
            package_full_name: String::new(),
            package_family_name: String::new(),
            residual_paths: Vec::new(),
        });
    }
    out
}

/// Data folders that belong to an AI app (settings, history, models...).
pub fn residual_paths(app: &AiApp) -> Vec<String> {
    let identity = format!("{} {} {}", app.name, app.package, app.package_full_name);
    let home = user_profile();
    let local = util::local_appdata();
    let roaming = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| local.join("Roaming"));
    let mut paths: Vec<std::path::PathBuf> = Vec::new();

    if identity.to_lowercase().contains("claude") || identity.to_lowercase().contains("anthropic") {
        paths.push(std::path::PathBuf::from(&home).join(".claude"));
        paths.push(std::path::PathBuf::from(&home).join(".local").join("bin").join("claude.exe"));
        paths.push(roaming.join("Claude"));
        paths.push(local.join("Claude"));
        paths.push(local.join("Programs").join("Claude"));
    }
    if identity.to_lowercase().contains("chatgpt") {
        paths.push(roaming.join("ChatGPT"));
        paths.push(local.join("ChatGPT"));
    }
    if identity.to_lowercase().contains("opencode") {
        paths.push(roaming.join("opencode"));
        paths.push(local.join("opencode"));
        paths.push(std::path::PathBuf::from(&home).join(".config").join("opencode"));
        paths.push(std::path::PathBuf::from(&home).join(".local").join("share").join("opencode"));
        paths.push(std::path::PathBuf::from(&home).join(".opencode"));
    }
    if identity.to_lowercase().contains("gemini") {
        paths.push(std::path::PathBuf::from(&home).join(".gemini"));
    }
    if identity.to_lowercase().contains("codex") || identity.to_lowercase().contains("openai") {
        paths.push(std::path::PathBuf::from(&home).join(".codex"));
    }
    if identity.to_lowercase().contains("ollama") {
        paths.push(std::path::PathBuf::from(&home).join(".ollama"));
        paths.push(local.join("Ollama"));
    }
    if identity.to_lowercase().contains("lm studio") || identity.to_lowercase().contains("lmstudio") {
        paths.push(std::path::PathBuf::from(&home).join(".lmstudio"));
        paths.push(roaming.join("LM Studio"));
        paths.push(local.join("LM Studio"));
    }
    if identity.to_lowercase().contains("cursor") {
        paths.push(roaming.join("Cursor"));
        paths.push(std::path::PathBuf::from(&home).join(".cursor"));
    }
    if app.kind == "appx" && !app.package_family_name.is_empty() {
        paths.push(local.join("Packages").join(&app.package_family_name));
    }

    let mut seen = Vec::new();
    for path in paths {
        if path.exists() {
            let text = path.to_string_lossy().to_string();
            if !seen.contains(&text) {
                seen.push(text);
            }
        }
    }
    seen
}

/// Size of a path tree in bytes, used before deleting user data.
pub fn path_size(path: &str) -> u64 {
    let p = std::path::Path::new(path);
    if p.is_dir() {
        util::dir_size(p)
    } else {
        std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    }
}

/// Remove an AI app or agent.
pub fn remove(app: &AiApp) -> anyhow::Result<String> {
    match app.kind.as_str() {
        "appx" => {
            let output = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!(
                        "Remove-AppxPackage -Package '{}' -ErrorAction Stop",
                        app.package_full_name.replace('\'', "''")
                    ),
                ])
                .output()
                .map_err(|e| anyhow::anyhow!("Không chạy được PowerShell: {e}"))?;
            if output.status.success() {
                Ok(format!("Đã gỡ Store app {}", app.name))
            } else {
                Err(anyhow::anyhow!(
                    "Không gỡ được {}: {}",
                    app.name,
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        }
        "native-cli" => {
            let path = std::path::Path::new(&app.uninstall);
            std::fs::remove_file(path)
                .map_err(|e| anyhow::anyhow!("Không xoá được {}: {e}", app.uninstall))?;
            Ok(format!("Đã gỡ {}", app.name))
        }
        "npm" | "pnpm" | "bun" => {
            if !valid_package_name(&app.package) {
                return Err(anyhow::anyhow!("Tên package không hợp lệ: {}", app.package));
            }
            let (command, args): (&str, Vec<String>) = match app.kind.as_str() {
                "npm" => ("npm", vec!["uninstall".into(), "--global".into(), app.package.clone()]),
                "pnpm" => ("pnpm", vec!["uninstall".into(), "--global".into(), app.package.clone()]),
                _ => ("bun", vec!["remove".into(), "--global".into(), app.package.clone()]),
            };
            let status = std::process::Command::new(command)
                .args(&args)
                .status()
                .map_err(|e| anyhow::anyhow!("Không chạy được {command}: {e}"))?;
            if status.success() {
                Ok(format!("Đã gỡ {}", app.name))
            } else {
                Err(anyhow::anyhow!("{command} không gỡ được {}", app.name))
            }
        }
        _ => {
            let command = app.uninstall.trim();
            if command.is_empty() {
                return Err(anyhow::anyhow!("Không tìm thấy trình gỡ cài đặt"));
            }
            run_uninstall(command)
        }
    }
}

fn valid_package_name(name: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(@[a-z0-9._-]+/)?[a-z0-9._-]+$").unwrap())
        .is_match(name)
}

/// Run a registered uninstall command.
///
/// Only a real, existing executable is launched, and the registry text is
/// never passed through cmd.exe.
fn run_uninstall(command: &str) -> anyhow::Result<String> {
    if let Some(caps) = Regex::new(r"(?i)msiexec(?:\.exe)?\s+.*?/(?:i|x)\s*(\{[0-9a-f-]{36}\})")
        .unwrap()
        .captures(command)
    {
        let msi = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("System32")
            .join("msiexec.exe");
        let status = std::process::Command::new(msi)
            .arg(format!("/x {}", &caps[1]))
            .status()
            .map_err(|e| anyhow::anyhow!("Không chạy được msiexec: {e}"))?;
        return if status.success() {
            Ok("Đã chạy trình gỡ cài đặt MSI".to_string())
        } else {
            Err(anyhow::anyhow!("msiexec kết thúc với mã {}", status.code().unwrap_or(-1)))
        };
    }

    let (exe, args) = if let Some(caps) = Regex::new(r#"^"([^"]+\.exe)"\s*(.*)$"#).unwrap().captures(command) {
        (caps[1].to_string(), caps[2].to_string())
    } else if let Some(caps) = Regex::new(r"^(.+?\.exe)\s*(.*)$").unwrap().captures(command) {
        (caps[1].trim().to_string(), caps[2].to_string())
    } else {
        return Err(anyhow::anyhow!("Lệnh gỡ cài đặt không được hỗ trợ an toàn"));
    };
    let path = std::path::Path::new(&exe);
    if !path.is_absolute() || !path.exists() {
        return Err(anyhow::anyhow!("Không tìm thấy chương trình gỡ cài đặt trên máy"));
    }
    let mut command_builder = std::process::Command::new(path);
    if !args.trim().is_empty() {
        command_builder.arg(args.trim());
    }
    command_builder
        .status()
        .map_err(|e| anyhow::anyhow!("Không chạy được trình gỡ cài đặt: {e}"))?;
    Ok("Đã chạy trình gỡ cài đặt".to_string())
}

/// Remove leftover data folders (settings, history, models) of an app.
pub fn remove_residual_data(paths: &[String]) -> (Vec<String>, Vec<String>) {
    let home = user_profile();
    let profile_root = std::path::Path::new(&home);
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    for path in paths {
        let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
        if !resolved.starts_with(profile_root) {
            failed.push(format!("{path} (ngoài hồ sơ người dùng)"));
            continue;
        }
        let result = if resolved.is_dir() {
            std::fs::remove_dir_all(&resolved)
        } else {
            std::fs::remove_file(&resolved)
        };
        match result {
            Ok(()) => removed.push(path.clone()),
            Err(err) => failed.push(format!("{path} ({err})")),
        }
    }
    (removed, failed)
}
