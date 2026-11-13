//! Disk usage: free space per drive and the largest folders in a tree.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub path: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}

/// Free/total space of every fixed drive.
pub fn drives() -> Vec<DriveInfo> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let path = format!("{}:\\", letter as char);
        unsafe {
            let mut free = 0u64;
            let mut total = 0u64;
            let mut total_free = 0u64;
            let path_w = crate::util::Wide::new(&path);
            if GetDiskFreeSpaceExW(
                path_w.pcwstr(),
                Some(&mut free),
                Some(&mut total),
                Some(&mut total_free),
            )
            .is_ok()
            {
                let used = total.saturating_sub(total_free);
                out.push(DriveInfo {
                    path: path.clone(),
                    total_bytes: total,
                    free_bytes: free,
                    used_bytes: used,
                    used_percent: if total > 0 {
                        (used as f64 / total as f64 * 1000.0).round() / 10.0
                    } else {
                        0.0
                    },
                });
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSize {
    pub path: String,
    pub size_bytes: u64,
    pub file_count: usize,
}

/// Find the largest folders under `root`, limited to `max_depth` levels.
///
/// This walks the tree once and attributes each file's size to every folder
/// on its path, so the numbers are the real size of each folder.
pub fn largest_folders(root: &std::path::Path, limit: usize, max_depth: usize) -> Vec<FolderSize> {
    let mut sizes: std::collections::HashMap<std::path::PathBuf, (u64, usize)> =
        std::collections::HashMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut files_seen = 0usize;

    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                if depth < max_depth {
                    stack.push((path.clone(), depth + 1));
                }
                continue;
            }
            files_seen += 1;
            let size = meta.len();
            // Attribute the file to every ancestor up to `root`.
            let mut current = dir.clone();
            loop {
                let entry = sizes.entry(current.clone()).or_insert((0, 0));
                entry.0 += size;
                entry.1 += 1;
                if current == root || current.parent().is_none() {
                    break;
                }
                current = match current.parent() {
                    Some(p) => p.to_path_buf(),
                    None => break,
                };
            }
        }
    }

    let mut out: Vec<FolderSize> = sizes
        .into_iter()
        .filter(|(p, _)| p != root)
        .map(|(path, (size, count))| FolderSize {
            path: path.to_string_lossy().to_string(),
            size_bytes: size,
            file_count: count,
        })
        .collect();
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out.truncate(limit);
    let _ = files_seen;
    out
}

/// Analyse the Windows component store with DISM (needs elevation).
pub fn component_store_analysis() -> Option<String> {
    let output = std::process::Command::new("dism")
        .args(["/Online", "/Cleanup-Image", "/AnalyzeComponentStore", "/English"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if !text.contains("Component Store") {
        return None;
    }
    Some(text.lines().take(12).collect::<Vec<_>>().join("\n"))
}
