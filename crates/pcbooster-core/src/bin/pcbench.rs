//! Benchmark runner for the Rust core.

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(5);
    let report = pcbooster_core::bench::run(iterations);
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
