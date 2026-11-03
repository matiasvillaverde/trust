//! Opt-in benchmarks reuse the verified workloads without mixing synthetic records with sync counts.
use std::time::{Duration, Instant};

fn positive_env(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|v| *v > 0)
            .expect("performance setting must be a positive integer"),
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => panic!("invalid {name}: {error}"),
    }
}
fn median(work: impl Fn(), iterations: usize, warmups: usize) -> Duration {
    for _ in 0..warmups {
        work();
    }
    let mut samples: Vec<_> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            work();
            start.elapsed()
        })
        .collect();
    samples.sort_unstable();
    samples[iterations / 2]
}
#[test]
#[ignore = "Performance regression gate; run via make perf-gate"]
fn analytics_and_1200_persisted_lifecycles_meet_performance_budgets() {
    let iterations = positive_env("TRUST_PERF_GATE_ITERATIONS", 3);
    let warmups = positive_env("TRUST_PERF_GATE_WARMUP_ITERATIONS", 1);
    let analytics_limit = positive_env("TRUST_PERF_GATE_MAX_ANALYTICS_MS", 1000);
    let sync_limit = positive_env("TRUST_PERF_GATE_MAX_SYNC_MS", 6000);
    let analytics = median(super::volume::analytical_history, iterations, warmups);
    let sync = median(|| super::volume::sync_lifecycles(1200), iterations, warmups);
    eprintln!("median: 36,865 analytical records {analytics:?}; 1,200 persisted broker sync lifecycles {sync:?}");
    assert!(
        analytics.as_millis() <= analytics_limit as u128,
        "analytics {analytics:?} exceeds {analytics_limit}ms"
    );
    assert!(
        sync.as_millis() <= sync_limit as u128,
        "sync {sync:?} exceeds {sync_limit}ms"
    );
}

#[test]
#[ignore = "Large-ledger performance regression gate; run via make perf-gate"]
fn verified_admission_for_4800_persisted_lifecycles_meets_performance_budget() {
    let iterations = positive_env("TRUST_PERF_GATE_ITERATIONS", 3);
    let warmups = positive_env("TRUST_PERF_GATE_WARMUP_ITERATIONS", 1);
    let limit = positive_env("TRUST_PERF_GATE_MAX_LARGE_LEDGER_MS", 16000);
    let elapsed = median(|| super::volume::sync_lifecycles(4800), iterations, warmups);
    eprintln!("median: 4,800 persisted broker sync lifecycles {elapsed:?}");
    assert!(
        elapsed.as_millis() <= limit as u128,
        "large-ledger admission {elapsed:?} exceeds {limit}ms"
    );
}

#[path = "performance/profiler.rs"]
mod profiler;

#[test]
#[ignore = "Diagnostic timing and row-volume profile; run via make perf-profile"]
fn profile_risk_enabled_lifecycles() {
    let count = positive_env("TRUST_PROFILE_TRADES", 1200);
    let profile = profiler::capture(count);
    let output = std::env::var("TRUST_PROFILE_OUTPUT")
        .expect("set TRUST_PROFILE_OUTPUT to a JSON report path");
    std::fs::write(&output, serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
    eprintln!("Profile for {count} real lifecycles saved to {output}");
}
