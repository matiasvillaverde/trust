//! Aggregate safe financial spans into authenticated metrics; never format their payloads.
use crate::{
    error::{Result, ServiceError},
    metrics::{financial, Metrics},
};
use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Metadata, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

#[derive(Default)]
struct RowCount(u64);
impl Visit for RowCount {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "rows" {
            self.0 = value;
        }
    }
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {}
}
struct Observation {
    started: Instant,
    rows: RowCount,
}

/// Bounded measurements, attached once to the financial worker's metrics.
#[derive(Clone, Default)]
pub struct FinancialTimings(Arc<OnceLock<Arc<Metrics>>>);
impl FinancialTimings {
    /// Bind after worker startup, before accepting HTTP requests.
    pub fn bind(&self, metrics: Arc<Metrics>) -> Result<()> {
        self.0
            .set(metrics)
            .map_err(|_| ServiceError::invalid("Financial metrics already initialized"))
    }
    /// Use as a per-layer filter so operational logging keeps its existing policy.
    pub fn filter(metadata: &Metadata<'_>) -> bool {
        metadata.is_span()
            && metadata.target() == "trust::perf"
            && financial::known(metadata.name())
    }
}
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for FinancialTimings {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if !Self::filter(attributes.metadata()) {
            return;
        }
        if let Some(span) = ctx.span(id) {
            let mut rows = RowCount::default();
            attributes.record(&mut rows);
            span.extensions_mut().insert(Observation {
                started: Instant::now(),
                rows,
            });
        }
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(observation) = span.extensions_mut().get_mut::<Observation>() {
                values.record(&mut observation.rows);
            }
        }
    }
    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let (Some(metrics), Some(span)) = (self.0.get(), ctx.span(&id)) {
            if let Some(observation) = span.extensions().get::<Observation>() {
                metrics.financial.observe(
                    span.name(),
                    observation.started.elapsed(),
                    observation.rows.0,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn financial_metrics_record_durations_and_rows_without_leaking_arguments_or_new_labels() {
        let metrics = Arc::new(Metrics::default());
        let collector = FinancialTimings::default();
        assert!(collector.bind(metrics.clone()).is_ok());
        let subscriber = tracing_subscriber::registry().with(collector.with_filter(
            tracing_subscriber::filter::filter_fn(FinancialTimings::filter),
        ));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::trace_span!(target: "trust::perf", "db.cash_movements", rows = tracing::field::Empty, account = "SECRET-ACCOUNT", token = "SECRET-TOKEN");
            let guard = span.enter();
            span.record("rows", 7_u64);
            tracing::trace_span!(target: "trust::perf", "SECRET-LABEL").in_scope(|| {});
            tracing::info!(target: "trust::perf", payload = "SECRET-PAYLOAD");
            drop(guard);
        });
        let text = metrics.render(true, 8);
        assert!(text.contains(
            "trust_financial_stage_duration_seconds_count{stage=\"db.cash_movements\"} 1\n"
        ));
        assert!(text.contains("trust_financial_rows_read_total{stage=\"db.cash_movements\"} 7\n"));
        assert!(!text.contains("SECRET"));
    }
}
