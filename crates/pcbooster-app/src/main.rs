// PC-Booster desktop application.
//
// The release build is a GUI app (no console window). The `--selftest` flag
// runs the core self test and writes the result to a file, which is how the
// end to end test verifies the packaged binary.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--selftest") {
        let report = pcbooster_core::selftest::run();
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "ok": report.ok,
            "checks": report.checks,
        }))
        .unwrap_or_default();
        match args
            .iter()
            .position(|a| a == "--selftest-out")
            .and_then(|index| args.get(index + 1))
        {
            Some(path) => {
                if let Ok(mut file) = std::fs::File::create(path) {
                    let _ = file.write_all(json.as_bytes());
                }
            }
            None => {
                // GUI subsystem has no stdout, so mirror the result to a file.
                let path = std::env::temp_dir().join("pcbooster-selftest.json");
                if let Ok(mut file) = std::fs::File::create(&path) {
                    let _ = file.write_all(json.as_bytes());
                }
            }
        }
        std::process::exit(if report.ok { 0 } else { 1 });
    }

    pcbooster_app::run();
}
