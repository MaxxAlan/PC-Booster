//! Micro benchmark used to compare the Rust core with the PowerShell script.

use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct BenchCase {
    pub name: String,
    pub median_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub iterations: usize,
    pub items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub iterations: usize,
    pub total_ms: f64,
    pub cases: Vec<BenchCase>,
}

fn measure<T>(iterations: usize, mut func: impl FnMut() -> T, items: impl Fn(&T) -> usize) -> BenchCase {
    // Warm up once so the first call does not pay for lazy initialization.
    let warm = func();
    let count = items(&warm);
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let value = func();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(value);
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    BenchCase {
        name: String::new(),
        median_ms: (samples[samples.len() / 2] * 1000.0).round() / 1000.0,
        min_ms: (samples[0] * 1000.0).round() / 1000.0,
        max_ms: (samples[samples.len() - 1] * 1000.0).round() / 1000.0,
        iterations,
        items: count,
    }
}

/// Run every benchmark case.
pub fn run(iterations: usize) -> BenchReport {
    let mut cases: Vec<BenchCase> = Vec::new();

    let mut case = measure(iterations, crate::registry::installed_apps, |v| v.len());
    case.name = "registry.installed_apps".to_string();
    cases.push(case);

    let mut case = measure(iterations, crate::registry::store_packages, |v| v.len());
    case.name = "registry.store_packages".to_string();
    cases.push(case);

    let mut case = measure(iterations, crate::registry::startup_entries, |v| v.len());
    case.name = "registry.startup_entries".to_string();
    cases.push(case);

    let mut case = measure(iterations, || crate::processes::processes(0.0), |v| v.len());
    case.name = "processes.list".to_string();
    cases.push(case);

    let mut case = measure(iterations, || crate::temp::scan_temp(7), |v| v.len());
    case.name = "temp.scan".to_string();
    cases.push(case);

    let mut case = measure(iterations, || {
        crate::services::OPTIONAL_SERVICES
            .iter()
            .filter_map(|c| crate::services::service_info(c.name))
            .collect::<Vec<_>>()
    }, |v| v.len());
    case.name = "services.query".to_string();
    cases.push(case);

    let mut case = measure(iterations, crate::power::list_schemes, |v| v.len());
    case.name = "power.list".to_string();
    cases.push(case);

    let mut case = measure(iterations, crate::ai::ai_apps, |v| v.len());
    case.name = "ai.scan".to_string();
    cases.push(case);

    let mut case = measure(iterations, crate::overview, |v| v.startup_count);
    case.name = "overview.full".to_string();
    cases.push(case);

    let total_ms: f64 = cases.iter().map(|c| c.median_ms).sum();
    BenchReport {
        iterations,
        total_ms: (total_ms * 1000.0).round() / 1000.0,
        cases,
    }
}
