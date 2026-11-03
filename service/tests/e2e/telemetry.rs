pub(super) fn assert_metrics(metrics: &str, account: &str) {
    assert!(metrics.contains("trust_worker_ready 1\n"));
    assert!(metrics.contains("trust_replays_total 1\n"));
    assert!(metrics.contains("trust_execution_milliseconds_bucket"));
    assert!(metrics.contains("trust_tool_duration_seconds_count{tool=\"trade_submit\"} 3\n"));
    assert!(metrics.contains("trust_stage_duration_seconds_count{stage=\"journal_finish\"}"));
    assert!(metrics.contains("trust_http_responses_total{route=\"mcp\",outcome=\"success\"}"));
    assert!(metrics.contains("trust_log_dropped_lines_total 0\n"));
    assert!(!metrics.contains(account));
    for name in [
        "risk.available_cash",
        "risk.monthly_reference",
        "db.cash_movements",
        "db.cash_projection.append",
        "db.monthly_reference_inputs",
    ] {
        let count = sample(
            metrics,
            "trust_financial_stage_duration_seconds_count",
            name,
        );
        let duration = sample(metrics, "trust_financial_stage_duration_seconds_sum", name);
        assert!(count > 0.0, "actual broker lifecycle did not record {name}");
        assert!(duration > 0.0, "elapsed time disappeared for {name}");
    }
    assert!(
        sample(
            metrics,
            "trust_financial_rows_read_total",
            "db.cash_movements"
        ) > 0.0
    );
    assert!(
        sample(
            metrics,
            "trust_financial_rows_read_total",
            "db.monthly_reference_inputs"
        ) > 0.0
    );
}

fn sample(metrics: &str, metric: &str, stage: &str) -> f64 {
    let prefix = format!("{metric}{{stage=\"{stage}\"}} ");
    metrics
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .expect("metric is exposed")
        .parse()
        .expect("numeric telemetry sample")
}
