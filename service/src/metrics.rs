//! Bounded, process-local telemetry. No subjects, accounts, request arguments or keys as labels.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;

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

const BOUNDS: [u64; 12] = [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000, 10000, 45000];

#[derive(Clone, Copy, Default)]
struct HistogramValues {
    buckets: [u64; 12],
    count: u64,
    sum: u64,
}
#[derive(Default)]
pub struct Histogram {
    values: Mutex<HistogramValues>,
}
impl Histogram {
    pub fn observe(&self, duration: Duration) {
        let ms = milliseconds(duration);
        let mut values = self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        values.count = values.count.saturating_add(1);
        values.sum = values.sum.saturating_add(ms);
        for (bound, bucket) in BOUNDS.iter().zip(&mut values.buckets) {
            if ms <= *bound {
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
            output.push_str(&format!("{name}_bucket{{le=\"{bound}\"}} {bucket}\n"));
        }
        let count = values.count;
        output.push_str(&format!(
            "{name}_bucket{{le=\"+Inf\"}} {count}\n{name}_count {count}\n{name}_sum {}\n",
            values.sum
        ));
    }
}

#[derive(Default)]
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
}
impl Metrics {
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
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
