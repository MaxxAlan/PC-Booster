//! Command line front end for PC-Booster.
//!
//! Used by the end to end tests and for scripted maintenance.

use std::process::ExitCode;

use pcbooster_core as core;

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("JSON error: {err}"),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn run_action(action: &str) -> ExitCode {
    match action {
        "restore" => {
            let mut state = core::journal::RestoreState::load();
            let messages = core::restore_all(&mut state);
            if !state.is_empty() {
                println!("{}", messages.join("\n"));
                core::util::write_log("Restore finished with pending items");
                ExitCode::SUCCESS
            } else {
                state.clear();
                println!("{}", messages.join("\n"));
                ExitCode::SUCCESS
            }
        }
        "power-high-performance" => match core::power::switch_to_high_performance() {
            Ok(name) => {
                println!("Đã chuyển sang gói nguồn {name}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("Hành động không hợp lệ: {other}");
            ExitCode::FAILURE
        }
    }
}

fn selftest() -> ExitCode {
    let report = core::selftest::run();
    print_json(&serde_json::json!({
        "ok": report.ok,
        "checks": report.checks,
    }));
    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn usage() {
    println!(
        "PC-Booster CLI\n\
         \n\
         Lệnh:\n  \
         overview                 tổng quan hệ thống (RAM, ổ đĩa, startup, điểm sức khoẻ)\n  \
         apps                     ứng dụng đã cài đặt\n  \
         ai                       ứng dụng AI / coding agent\n  \
         startup                  mục tự khởi động\n  \
         services                 dịch vụ tùy chọn\n  \
         temp                     tệp tạm cũ hơn 7 ngày\n  \
         processes                tiến trình đang chạy\n  \
         disk                     dung lượng ổ đĩa\n  \
         power                    gói nguồn\n  \
         defender                 trạng thái Microsoft Defender\n  \
         selftest                 kiểm tra nhanh các chức năng\n  \
         bench                    đo tốc độ từng tác vụ\n\
         \n\
         Thêm --json để xuất JSON."
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(action) = core::elevation::requested_action() {
        return run_action(&action);
    }

    let command = args.first().map(|s| s.as_str()).unwrap_or("help");
    let json = has_flag(&args, "--json");

    match command {
        "overview" => {
            let report = core::overview();
            if json {
                print_json(&report);
            } else {
                println!("Điểm sức khoẻ: {}/100", report.score);
                println!("RAM: {:.0}/{:.0} MB", report.memory.used_mb, report.memory.total_mb);
                println!("Mục tự khởi động: {}", report.startup_count);
                println!("Ứng dụng AI tìm thấy: {}", report.ai_app_count);
                println!("Tệp tạm cũ: {}", core::util::human_bytes(report.temp_old_bytes));
                for note in &report.notes {
                    println!("- {note}");
                }
            }
            ExitCode::SUCCESS
        }
        "apps" => {
            let apps = core::registry::installed_apps();
            if json {
                print_json(&apps);
            } else {
                for app in &apps {
                    println!("{} | {}", app.name, app.version);
                }
                println!("{} ứng dụng", apps.len());
            }
            ExitCode::SUCCESS
        }
        "ai" => {
            let apps = core::ai::ai_apps();
            if json {
                print_json(&apps);
            } else {
                for app in &apps {
                    println!("[{}] {}", app.kind, app.name);
                }
                println!("{} ứng dụng AI", apps.len());
            }
            ExitCode::SUCCESS
        }
        "startup" => {
            let entries = core::registry::startup_entries();
            if json {
                print_json(&entries);
            } else {
                for entry in &entries {
                    println!("{} | {} | {}", entry.name, entry.location, entry.command);
                }
                println!("{} mục", entries.len());
            }
            ExitCode::SUCCESS
        }
        "services" => {
            let mut out = Vec::new();
            for candidate in core::services::OPTIONAL_SERVICES {
                if let Some(info) = core::services::service_info(candidate.name) {
                    out.push(serde_json::json!({
                        "name": info.name,
                        "display_name": info.display_name,
                        "status": info.status,
                        "start_type": info.start_type,
                        "running": info.running,
                        "recommended": candidate.recommended,
                        "impact": candidate.impact
                    }));
                }
            }
            if json {
                print_json(&out);
            } else {
                for item in &out {
                    println!(
                        "{} | {} | {} | khuyến nghị: {}",
                        item["name"].as_str().unwrap_or_default(),
                        item["status"].as_str().unwrap_or_default(),
                        item["start_type"].as_str().unwrap_or_default(),
                        item["recommended"].as_bool().unwrap_or(false)
                    );
                }
            }
            ExitCode::SUCCESS
        }
        "temp" => {
            let items = core::temp::scan_temp(7);
            if json {
                print_json(&items);
            } else {
                for item in &items {
                    println!(
                        "{} | {} | {} ngày",
                        item.name,
                        core::util::human_bytes(item.size_bytes),
                        item.age_days
                    );
                }
                let total: u64 = items.iter().map(|i| i.size_bytes).sum();
                println!("{} mục, {}", items.len(), core::util::human_bytes(total));
            }
            ExitCode::SUCCESS
        }
        "processes" => {
            let items = core::processes::processes(1.0);
            if json {
                print_json(&items);
            } else {
                for item in items.iter().take(20) {
                    println!(
                        "{:<24} pid={:<7} cpu={:<6}% ram={} MB",
                        item.name, item.pid, item.cpu_percent, item.memory_mb
                    );
                }
                println!("{} tiến trình", items.len());
            }
            ExitCode::SUCCESS
        }
        "disk" => {
            let drives = core::disk::drives();
            if json {
                print_json(&drives);
            } else {
                for drive in &drives {
                    println!(
                        "{} {} / {} ({}%)",
                        drive.path,
                        core::util::human_bytes(drive.used_bytes),
                        core::util::human_bytes(drive.total_bytes),
                        drive.used_percent
                    );
                }
            }
            ExitCode::SUCCESS
        }
        "power" => {
            let schemes = core::power::list_schemes();
            if json {
                print_json(&schemes);
            } else {
                for scheme in &schemes {
                    println!("{} | {}{}", scheme.guid, scheme.name, if scheme.active { " *" } else { "" });
                }
            }
            ExitCode::SUCCESS
        }
        "defender" => {
            let status = core::defender::status();
            if json {
                print_json(&status);
            } else {
                println!("Defender: {}", status.am_running_mode);
                println!(
                    "Chữ ký cập nhật: {} ({} ngày trước)",
                    status.signature_last_updated, status.signature_age_days
                );
            }
            ExitCode::SUCCESS
        }
        "selftest" => selftest(),
        "bench" => {
            let report = pcbooster_core::bench::run(5);
            print_json(&report);
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::SUCCESS
        }
    }
}
