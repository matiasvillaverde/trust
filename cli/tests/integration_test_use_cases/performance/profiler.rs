//! Scoped diagnostic subscriber: bounded labels, no SQL, identities, amounts or event bodies.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
    thread::ThreadId,
    time::Instant,
};
use tracing::{field::Visit, span, Id, Metadata, Subscriber};
use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, Layer};

#[derive(Default)]
struct Counts {
    calls: u64,
    total_ns: u128,
    self_ns: u128,
    rows: u64,
}
struct Frame {
    id: Id,
    name: &'static str,
    started: Instant,
    children_ns: u128,
}
#[derive(Default)]
struct Samples {
    stacks: HashMap<ThreadId, Vec<Frame>>,
    totals: BTreeMap<&'static str, Counts>,
    fields: BTreeSet<&'static str>,
}
#[derive(Default)]
struct Rows(u64);
impl Visit for Rows {
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "rows" {
            self.0 = value;
        }
    }
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}
#[derive(Clone, Default)]
struct Timings(Arc<Mutex<Samples>>);
impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Timings {
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.is_span() && metadata.target() == "trust::perf"
    }
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut rows = Rows::default();
        attrs.record(&mut rows);
        ctx.span(id).unwrap().extensions_mut().insert(rows);
        self.0
            .lock()
            .unwrap()
            .fields
            .extend(attrs.metadata().fields().iter().map(|f| f.name()));
    }
    fn on_record(&self, id: &Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        values.record(span.extensions_mut().get_mut::<Rows>().unwrap());
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let frame = Frame {
            id: id.clone(),
            name: ctx.span(id).unwrap().name(),
            started: Instant::now(),
            children_ns: 0,
        };
        self.0
            .lock()
            .unwrap()
            .stacks
            .entry(std::thread::current().id())
            .or_default()
            .push(frame);
    }
    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let ended = Instant::now();
        let rows = ctx.span(id).unwrap().extensions().get::<Rows>().unwrap().0;
        let mut samples = self.0.lock().unwrap();
        let stack = samples
            .stacks
            .get_mut(&std::thread::current().id())
            .unwrap();
        let frame = stack.pop().unwrap();
        assert_eq!(frame.id, *id, "profile spans must be properly nested");
        let elapsed = ended.duration_since(frame.started).as_nanos();
        if let Some(parent) = stack.last_mut() {
            parent.children_ns += elapsed;
        }
        let total = samples.totals.entry(frame.name).or_default();
        total.calls += 1;
        total.total_ns += elapsed;
        total.self_ns += elapsed.checked_sub(frame.children_ns).unwrap();
        total.rows += rows;
    }
}

pub(super) fn capture(count: usize) -> serde_json::Value {
    let layer = Timings::default();
    let subscriber = tracing_subscriber::registry().with(layer.clone());
    tracing::subscriber::with_default(subscriber, || super::super::volume::sync_lifecycles(count));
    let samples = layer.0.lock().unwrap();
    assert!(samples.stacks.values().all(Vec::is_empty));
    assert!(
        samples.fields.iter().all(|name| *name == "rows"),
        "performance spans must not record financial arguments"
    );
    let mut spans: Vec<_> = samples
        .totals
        .iter()
        .map(|(name, value)| {
            serde_json::json!({
                "span": name, "calls": value.calls, "inclusive_ns": value.total_ns,
                "exclusive_ns": value.self_ns, "rows": value.rows,
            })
        })
        .collect();
    spans.sort_by_key(|row| std::cmp::Reverse(row["exclusive_ns"].as_u64().unwrap()));
    serde_json::json!({"lifecycles": count, "spans": spans})
}

#[test]
fn instrumented_lifecycles_conserve_cash_and_emit_only_bounded_metrics() {
    let count = 64;
    let profile = capture(count);
    let spans = profile["spans"].as_array().unwrap();
    let workload = spans
        .iter()
        .find(|row| row["span"] == "lifecycle.workload")
        .unwrap();
    assert_eq!(workload["calls"], 1);
    let self_time: u64 = spans
        .iter()
        .map(|row| row["exclusive_ns"].as_u64().unwrap())
        .sum();
    assert_eq!(self_time, workload["inclusive_ns"].as_u64().unwrap());
    assert!(spans.iter().any(|row| row["rows"].as_u64().unwrap() > 0));
    let cash_rows: u64 = spans
        .iter()
        .filter(|row| row["span"] == "db.cash_movements")
        .map(|row| row["rows"].as_u64().unwrap())
        .sum();
    assert_eq!(cash_rows, 1,
        "the initial deposit is verified once; later append-only lifecycles must reuse the exact saved computation");
    let updates = spans
        .iter()
        .find(|row| row["span"] == "db.cash_projection.append")
        .unwrap();
    assert_eq!(
        updates["calls"],
        u64::try_from(count * 2).unwrap(),
        "both funding and exit receipts must update saved cash during their writes"
    );
}
