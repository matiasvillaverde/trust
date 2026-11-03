//! Read only named duration/count samples; never retain a full metrics response.
use super::*;
use std::collections::BTreeMap;

const FINANCIAL: [&str; 15] = [
    "risk.available_cash",
    "risk.monthly_reference",
    "risk.month_opening",
    "risk.carried_trades",
    "risk.quantity",
    "risk.monthly_percentage",
    "risk.protected_capital",
    "db.cash_movements",
    "db.all_transactions",
    "db.account_transactions",
    "db.opening_transactions",
    "db.monthly_reference_inputs",
    "db.ledger.load",
    "db.ledger.decode_transactions",
    "db.ledger.decode_cash",
];

pub(super) async fn snapshot(f: &Fixture, p: &support::Process) -> BTreeMap<String, [f64; 3]> {
    let response = f
        .request(p, "/metrics", Some("owner"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let text = response.text().await.unwrap();
    let mut result = BTreeMap::new();
    for (name, family, label) in [
        ("queue", "trust_queue_wait_seconds", ""),
        (
            "authentication_all_routes",
            "trust_authentication_seconds",
            "",
        ),
        (
            "risk_preview",
            "trust_tool_duration_seconds",
            "tool=\"risk_preview\"",
        ),
        (
            "http_tool",
            "trust_http_route_duration_seconds",
            "route=\"tool\"",
        ),
        (
            "http_mcp",
            "trust_http_route_duration_seconds",
            "route=\"mcp\"",
        ),
    ] {
        result.insert(name.to_owned(), timing(&text, family, label));
    }
    for name in [
        "validation",
        "authorization",
        "journal_claim",
        "domain",
        "journal_finish",
    ] {
        result.insert(
            name.to_owned(),
            timing(
                &text,
                "trust_stage_duration_seconds",
                &format!("stage=\"{name}\""),
            ),
        );
    }
    for name in FINANCIAL {
        let label = format!("stage=\"{name}\"");
        let mut values = timing(&text, "trust_financial_stage_duration_seconds", &label);
        values[2] = sample(
            &text,
            &format!("trust_financial_rows_read_total{{{label}}}"),
        );
        result.insert(name.to_owned(), values);
    }
    result
}

fn timing(text: &str, family: &str, label: &str) -> [f64; 3] {
    [
        sample(text, &format!("{family}_count{{{label}}}")),
        sample(text, &format!("{family}_sum{{{label}}}")),
        0.0,
    ]
}

fn sample(text: &str, key: &str) -> f64 {
    let prefix = format!("{key} ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("Missing bounded metric {key}"))
        .parse()
        .unwrap()
}

#[allow(clippy::float_arithmetic)] // Prometheus timing samples, never financial values.
pub(super) fn difference(
    before: &BTreeMap<String, [f64; 3]>,
    after: &BTreeMap<String, [f64; 3]>,
    mcp: bool,
    concurrency: usize,
    requests: usize,
) -> Value {
    let mut stages = Vec::new();
    for (name, end) in after {
        let start = before.get(name).unwrap();
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        assert!(delta.iter().all(|value| *value >= 0.0));
        if delta[0] > 0.0 {
            stages.push(json!({"stage":name,"calls":delta[0],"total_seconds":delta[1],"mean_microseconds":delta[1]*1_000_000.0/delta[0],"rows":delta[2]}));
        }
    }
    let observed = stages
        .iter()
        .find(|stage| stage["stage"] == "risk_preview")
        .unwrap();
    assert_eq!(observed["calls"].as_f64().unwrap(), requests as f64);
    json!({"transport":if mcp {"mcp"} else {"http"},"concurrency":concurrency,"requests":requests,"encrypted_storage":true,"authentication_includes_closing_metrics_scrape":true,"timing":"inclusive; parent and child totals overlap","stages":stages})
}
