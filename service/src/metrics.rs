//! Bounded, process-local telemetry. No subjects, accounts, request arguments or keys as labels.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub fn microseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

pub fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub fn increment(value: &AtomicU64, amount: u64) {
    let _ = value.try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(amount))
    });
}

pub fn decrement(value: &AtomicU64) {
    let _ = value.try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_sub(1))
    });
}

// Microseconds internally: a fast request must not disappear into a zero-ms sum.
const BOUNDS: [u64; 16] = [
    50, 100, 250, 500, 1000, 5000, 10000, 25000, 50000, 100000, 250000, 500000, 1000000, 5000000,
    10000000, 45000000,
];

#[derive(Clone, Copy, Default)]
struct HistogramValues {
    buckets: [u64; 16],
    count: u64,
    sum: u64,
}
#[derive(Default)]
pub struct Histogram {
    values: Mutex<HistogramValues>,
}
impl Histogram {
    pub fn observe(&self, duration: Duration) {
        let us = microseconds(duration);
        let mut values = self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        values.count = values.count.saturating_add(1);
        values.sum = values.sum.saturating_add(us);
        for (bound, bucket) in BOUNDS.iter().zip(&mut values.buckets) {
            if us <= *bound {
                *bucket = bucket.saturating_add(1);
            }
        }
    }
    fn render(&self, name: &str, output: &mut String) {
        // A scrape must not observe half of an observation: cumulative buckets,
        // +Inf/count and sum are one snapshot. Release the lock before formatting.
        let values = *self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        output.push_str(&format!("# TYPE {name} histogram\n"));
        for (bound, bucket) in BOUNDS.iter().zip(&values.buckets) {
            if *bound >= 1000 {
                output.push_str(&format!(
                    "{name}_bucket{{le=\"{}\"}} {bucket}\n",
                    bound / 1000
                ));
            }
        }
        let count = values.count;
        output.push_str(&format!(
            "{name}_bucket{{le=\"+Inf\"}} {count}\n{name}_count {count}\n{name}_sum {}\n",
            values.sum / 1000
        ));
    }

    fn seconds(&self, name: &str, labels: &str, output: &mut String) {
        let values = *self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prefix = if labels.is_empty() {
            String::new()
        } else {
            format!("{labels},")
        };
        for (bound, count) in BOUNDS.iter().zip(&values.buckets) {
            output.push_str(&format!(
                "{name}_bucket{{{prefix}le=\"{}\"}} {count}\n",
                seconds(*bound)
            ));
        }
        output.push_str(&format!("{name}_bucket{{{prefix}le=\"+Inf\"}} {}\n{name}_count{{{labels}}} {}\n{name}_sum{{{labels}}} {}\n", values.count, values.count, seconds(values.sum)));
    }
}

fn seconds(us: u64) -> String {
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Stage {
    Validation,
    Authorization,
    JournalClaim,
    Domain,
    JournalFinish,
}
impl Stage {
    const ALL: [Self; 5] = [
        Self::Validation,
        Self::Authorization,
        Self::JournalClaim,
        Self::Domain,
        Self::JournalFinish,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Authorization => "authorization",
            Self::JournalClaim => "journal_claim",
            Self::Domain => "domain",
            Self::JournalFinish => "journal_finish",
        }
    }
}

#[derive(Default)]
struct OperationMetrics {
    latency: Histogram,
    errors: AtomicU64,
}

/// Fixed route names only; paths, query strings and caller tool names are never labels.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Route {
    Mcp,
    Tool,
    Catalogue,
    Metrics,
    Probe,
    Metadata,
    Unknown,
}
impl Route {
    const ALL: [Self; 7] = [
        Self::Mcp,
        Self::Tool,
        Self::Catalogue,
        Self::Metrics,
        Self::Probe,
        Self::Metadata,
        Self::Unknown,
    ];
    pub(crate) fn classify(path: &str) -> Self {
        match path {
            "/mcp" => Self::Mcp,
            "/v1/tools" => Self::Catalogue,
            "/metrics" => Self::Metrics,
            "/healthz" | "/readyz" => Self::Probe,
            "/.well-known/oauth-protected-resource"
            | "/.well-known/oauth-protected-resource/mcp" => Self::Metadata,
            _ if path.starts_with("/v1/tools/") => Self::Tool,
            _ => Self::Unknown,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Tool => "tool",
            Self::Catalogue => "catalogue",
            Self::Metrics => "metrics",
            Self::Probe => "probe",
            Self::Metadata => "metadata",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Default)]
struct HttpMetrics {
    latency: Histogram,
    success: AtomicU64,
    client_error: AtomicU64,
    server_error: AtomicU64,
    other: AtomicU64,
}

pub struct Metrics {
    pub queued: AtomicU64,
    pub active: AtomicU64,
    pub completed: AtomicU64,
    pub failed: AtomicU64,
    pub replayed: AtomicU64,
    pub busy: AtomicU64,
    pub expired: AtomicU64,
    pub unknown: AtomicU64,
    pub audit_failures: AtomicU64,
    pub http_requests: AtomicU64,
    pub http_rejected: AtomicU64,
    pub queue_wait: Histogram,
    pub execution: Histogram,
    pub http_duration: Histogram,
    pub authentication: Histogram,
    pub log_drops: OnceLock<tracing_appender::non_blocking::ErrorCounter>,
    operations: BTreeMap<&'static str, OperationMetrics>,
    stages: BTreeMap<Stage, Histogram>,
    routes: BTreeMap<Route, HttpMetrics>,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            queued: AtomicU64::new(0),
            active: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            replayed: AtomicU64::new(0),
            busy: AtomicU64::new(0),
            expired: AtomicU64::new(0),
            unknown: AtomicU64::new(0),
            audit_failures: AtomicU64::new(0),
            http_requests: AtomicU64::new(0),
            http_rejected: AtomicU64::new(0),
            queue_wait: Histogram::default(),
            execution: Histogram::default(),
            http_duration: Histogram::default(),
            authentication: Histogram::default(),
            log_drops: OnceLock::new(),
            operations: crate::catalog::tools()
                .iter()
                .map(|tool| tool.name)
                .chain(["unknown"])
                .map(|name| (name, OperationMetrics::default()))
                .collect(),
            stages: Stage::ALL
                .into_iter()
                .map(|stage| (stage, Histogram::default()))
                .collect(),
            routes: Route::ALL
                .into_iter()
                .map(|route| (route, HttpMetrics::default()))
                .collect(),
        }
    }
}
impl Metrics {
    pub(crate) fn operation(&self, name: &str, duration: Duration, failed: bool) {
        if let Some(metric) = self
            .operations
            .get(name)
            .or_else(|| self.operations.get("unknown"))
        {
            metric.latency.observe(duration);
            if failed {
                increment(&metric.errors, 1);
            }
        }
    }
    pub(crate) fn http(&self, route: Route, status: u16, duration: Duration) {
        if let Some(metric) = self.routes.get(&route) {
            metric.latency.observe(duration);
            increment(
                match status {
                    200..=299 => &metric.success,
                    400..=499 => &metric.client_error,
                    500..=599 => &metric.server_error,
                    _ => &metric.other,
                },
                1,
            );
        }
    }
    pub(crate) fn measure<T>(&self, stage: Stage, action: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = action();
        if let Some(metric) = self.stages.get(&stage) {
            metric.observe(started.elapsed());
        }
        result
    }
    pub fn render(&self, ready: bool, queue_capacity: usize) -> String {
        let mut text = format!("# TYPE trust_worker_ready gauge\ntrust_worker_ready {}\n# TYPE trust_queue_capacity gauge\ntrust_queue_capacity {queue_capacity}\n", u8::from(ready));
        for (name, kind, counter) in [
            ("queue_depth", "gauge", &self.queued),
            ("worker_active", "gauge", &self.active),
            ("operations_total", "counter", &self.completed),
            ("operation_errors_total", "counter", &self.failed),
            ("replays_total", "counter", &self.replayed),
            ("queue_rejections_total", "counter", &self.busy),
            ("queued_cancellations_total", "counter", &self.expired),
            ("unknown_responses_total", "counter", &self.unknown),
            ("audit_failures_total", "counter", &self.audit_failures),
            ("http_requests_total", "counter", &self.http_requests),
            ("http_rejections_total", "counter", &self.http_rejected),
        ] {
            text.push_str(&format!(
                "# TYPE trust_{name} {kind}\ntrust_{name} {}\n",
                counter.load(Ordering::Relaxed)
            ));
        }
        self.queue_wait
            .render("trust_queue_wait_milliseconds", &mut text);
        self.execution
            .render("trust_execution_milliseconds", &mut text);
        self.http_duration
            .render("trust_http_duration_milliseconds", &mut text);
        for (name, metric) in [
            ("trust_queue_wait_seconds", &self.queue_wait),
            ("trust_execution_seconds", &self.execution),
            ("trust_http_duration_seconds", &self.http_duration),
            ("trust_authentication_seconds", &self.authentication),
        ] {
            text.push_str(&format!("# TYPE {name} histogram\n"));
            metric.seconds(name, "", &mut text);
        }
        text.push_str("# TYPE trust_tool_duration_seconds histogram\n# TYPE trust_tool_errors_total counter\n");
        for (name, metric) in &self.operations {
            let labels = format!("tool=\"{name}\"");
            metric
                .latency
                .seconds("trust_tool_duration_seconds", &labels, &mut text);
            text.push_str(&format!(
                "trust_tool_errors_total{{{labels}}} {}\n",
                metric.errors.load(Ordering::Relaxed)
            ));
        }
        text.push_str("# TYPE trust_stage_duration_seconds histogram\n");
        for (stage, metric) in &self.stages {
            metric.seconds(
                "trust_stage_duration_seconds",
                &format!("stage=\"{}\"", stage.name()),
                &mut text,
            );
        }
        text.push_str("# TYPE trust_http_route_duration_seconds histogram\n# TYPE trust_http_responses_total counter\n");
        for (route, metric) in &self.routes {
            let labels = format!("route=\"{}\"", route.name());
            metric
                .latency
                .seconds("trust_http_route_duration_seconds", &labels, &mut text);
            for (outcome, counter) in [
                ("success", &metric.success),
                ("client_error", &metric.client_error),
                ("server_error", &metric.server_error),
                ("other", &metric.other),
            ] {
                text.push_str(&format!(
                    "trust_http_responses_total{{{labels},outcome=\"{outcome}\"}} {}\n",
                    counter.load(Ordering::Relaxed)
                ));
            }
        }
        text.push_str(&format!(
            "# TYPE trust_log_dropped_lines_total counter\ntrust_log_dropped_lines_total {}\n",
            self.log_drops
                .get()
                .map_or(0, |counter| counter.dropped_lines())
        ));
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submillisecond_latencies_are_preserved_and_labels_stay_bounded() {
        let metrics = Metrics::default();
        let before = metrics.render(true, 8).lines().count();
        metrics.operation("risk_preview", Duration::from_micros(123), false);
        for n in 0..1000 {
            metrics.operation(
                &format!("untrusted-secret-{n}\""),
                Duration::from_micros(20),
                true,
            );
            metrics.http(
                Route::classify(&format!("/untrusted-secret-{n}")),
                401,
                Duration::from_micros(20),
            );
        }
        let text = metrics.render(true, 8);
        assert_eq!(text.lines().count(), before);
        assert!(!text.contains("untrusted-secret"));
        assert!(text.contains("trust_tool_duration_seconds_sum{tool=\"risk_preview\"} 0.000123\n"));
        assert!(text.contains(
            "trust_tool_duration_seconds_bucket{tool=\"risk_preview\",le=\"0.000100\"} 0\n"
        ));
        assert!(text.contains(
            "trust_tool_duration_seconds_bucket{tool=\"risk_preview\",le=\"0.000250\"} 1\n"
        ));
        assert!(text.contains("trust_tool_duration_seconds_count{tool=\"unknown\"} 1000\n"));
        assert!(text.contains("trust_tool_errors_total{tool=\"unknown\"} 1000\n"));
        assert!(text.contains(
            "trust_http_responses_total{route=\"unknown\",outcome=\"client_error\"} 1000\n"
        ));
    }

    #[test]
    fn concurrent_scrapes_have_consistent_histogram_buckets_count_and_sum(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let histogram = std::sync::Arc::new(Histogram::default());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(5));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let histogram = histogram.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..1000 {
                        histogram.observe(Duration::from_millis(1));
                    }
                })
            })
            .collect();
        barrier.wait();
        for _ in 0..1000 {
            let mut text = String::new();
            histogram.render("latency", &mut text);
            let values: Vec<u64> = text
                .lines()
                .filter(|line| !line.starts_with('#'))
                .map(|line| {
                    line.split_whitespace()
                        .last()
                        .ok_or("missing metric value")?
                        .parse::<u64>()
                        .map_err(|_| "invalid metric value")
                })
                .collect::<Result<_, _>>()?;
            assert!(
                values.windows(2).all(|pair| pair.first() == pair.last()),
                "inconsistent scrape: {text}"
            );
        }
        for handle in handles {
            handle.join().map_err(|_| "observer failed")?;
        }
        let mut text = String::new();
        histogram.render("latency", &mut text);
        assert!(text.contains("latency_count 4000\n"));
        assert!(text.contains("latency_sum 4000\n"));
        Ok(())
    }
}
