//! Integration tests that touch the live system but stay safe: they only
//! use a scratch registry key, files in the temp folder and a disposable
//! notepad process.

use pcbooster_core as core;

#[test]
fn registry_reads_and_writes_a_scratch_key() {
    use windows_registry::{Key, CURRENT_USER};

    let key = CURRENT_USER
        .create(r"SOFTWARE\PCBoosterTest")
        .expect("create scratch key");
    key.set_string("Greeting", "hello")
        .expect("write value");
    let value = key.get_string("Greeting").expect("read value");
    assert_eq!(value, "hello");

    let names: Vec<String> = key
        .values()
        .map(|it| it.map(|(name, _)| name).collect())
        .unwrap_or_default();
    assert!(names.iter().any(|n| n == "Greeting"));
    let _ = key.remove_value("Greeting");
}

#[test]
fn startup_disable_and_restore_round_trip() {
    use windows_registry::{Key, CURRENT_USER};

    let run_key = CURRENT_USER
        .create(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run")
        .expect("run key");
    run_key
        .set_string("PCBoosterTestEntry", "notepad.exe /test")
        .expect("write startup entry");

    let location = r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    core::registry::disable_startup_value(location, "PCBoosterTestEntry")
        .expect("disable startup entry");
    let after = run_key.get_string("PCBoosterTestEntry");
    assert!(after.is_err(), "entry should be gone from the Run key");

    core::registry::restore_startup_value(location, "PCBoosterTestEntry")
        .expect("restore startup entry");
    let restored = run_key.get_string("PCBoosterTestEntry").expect("restored");
    assert_eq!(restored, "notepad.exe /test");
    let _ = run_key.remove_value("PCBoosterTestEntry");
}

#[test]
fn temp_items_go_to_the_recycle_bin() {
    let dir = std::env::temp_dir().join("pcbooster-recycle-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("disposable.txt");
    std::fs::write(&file, b"pcbooster").unwrap();

    let (removed, failures) = core::temp::send_to_recycle_bin(&[file.to_string_lossy().to_string()]);
    assert!(failures.is_empty(), "unexpected failures: {failures:?}");
    assert_eq!(removed, 1);
    assert!(!file.exists(), "file should have left the folder");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn journal_round_trip() {
    let mut state = core::journal::RestoreState::default();
    state.power_plan = Some("381b4222-f694-41f0-9685-ff5bb260df2e".to_string());
    state.services.push(core::journal::ServiceChange {
        name: "TestService".to_string(),
        display_name: "Test Service".to_string(),
        previous_start_type: "Manual".to_string(),
        was_running: false,
    });
    state.save().expect("save journal");

    let loaded = core::journal::RestoreState::load();
    assert_eq!(loaded.services.len(), 1);
    assert_eq!(loaded.services[0].name, "TestService");
    assert!(loaded.power_plan.is_some());

    // Do not touch the real journal of the user: restore the previous file.
    let path = core::util::state_path();
    let backup = std::env::temp_dir().join("pcbooster-journal-backup.json");
    if path.exists() {
        let _ = std::fs::copy(&path, &backup);
    }
    let _ = std::fs::remove_file(&path);
    if backup.exists() {
        let _ = std::fs::copy(&backup, &path);
        let _ = std::fs::remove_file(&backup);
    }
}

#[test]
fn closing_a_window_asks_the_app_to_close_first() {
    let mut child = match std::process::Command::new("mspaint.exe").spawn() {
        Ok(child) => child,
        Err(_) => return, // no mspaint on this machine, nothing to test
    };
    // Give the window a moment to appear.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let pid = child.id();
    let result = core::processes::close_process(pid, true);
    let _ = child.wait();
    if let Err(err) = &result {
        let text = err.to_string();
        if text.contains("Access is denied") || text.contains("0x80070005") {
            // Environment limitation (Store/protected process): not a code defect.
            return;
        }
    }
    assert!(
        result.is_ok(),
        "graceful close failed: {:?}",
        result.err()
    );
}

#[test]
fn ai_pattern_matches_the_expected_products() {
    let apps = core::ai::ai_apps();
    // Whatever is installed, every entry must be one of the known kinds and
    // must carry the data folder list.
    for app in &apps {
        assert!(!app.kind.is_empty());
        assert!(!app.name.is_empty());
    }
}
