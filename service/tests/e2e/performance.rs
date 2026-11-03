//! Actual encrypted process latency and safe stage/row-volume profiles.
use super::*;
#[path = "performance/stages.rs"]
mod stages;

// Offline, real-process benchmark: JWT verification, HTTP/MCP, queueing, risk
// calculation and FULL-synchronous durable read auditing all stay enabled.
pub(super) async fn risk_preview_latency_budget() {
    use std::time::Instant;
    let mut profiles = Vec::new();
    for mcp in [false, true] {
        let gateway = Gateway::start().await;
        let mut f = Fixture::new(5000, 32);
        f.enable_encryption();
        let mut p = f.start(&gateway).await;
        let (account, _) = f.seed(&p, "long", "10", "performance").await;
        // Sign outside the timed region, as a real agent reuses its access token.
        let token = f.token("agent");
        let args = json!({"account_id":account,"entry":"100","stop":"95"});
        let request = if mcp {
            f.client.post(format!("{}/mcp", p.url)).json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"risk_preview","arguments":args}}))
        } else {
            f.client.post(format!("{}/v1/tools/risk_preview", p.url)).json(&args)
        }.header("host", "localhost:8000")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .bearer_auth(token);
        let measure = move |request: reqwest::RequestBuilder| async move {
            let start = Instant::now();
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), 200);
            let value: Value = response.json().await.unwrap();
            let reply = if mcp {
                &value["result"]["structuredContent"]
            } else {
                &value
            };
            assert!(reply["error"].is_null(), "{reply}");
            let quantity: rust_decimal::Decimal =
                serde_json::from_value(reply["data"]["final_quantity"].clone()).unwrap();
            assert_eq!(
                quantity,
                rust_decimal::Decimal::from(20),
                "unchanged $100 loss budget / $5 stop distance"
            );
            start.elapsed()
        };
        for _ in 0..6 {
            measure(request.try_clone().unwrap()).await;
        }
        for concurrency in [1, 4] {
            let before = stages::snapshot(&f, &p).await;
            let mut samples = Vec::new();
            let started = Instant::now();
            for _ in 0..(if concurrency == 1 { 32 } else { 16 }) {
                let mut pending = tokio::task::JoinSet::new();
                for _ in 0..concurrency {
                    pending.spawn(measure(request.try_clone().unwrap()));
                }
                while let Some(sample) = pending.join_next().await {
                    samples.push(sample.unwrap());
                }
            }
            let elapsed = started.elapsed();
            samples.sort_unstable();
            let percentile = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
            let latency = json!({"encrypted_storage":true,"transport":if mcp{"mcp"}else{"http"},"concurrency":concurrency,"samples":samples.len(),"elapsed_us":elapsed.as_micros(),"p50_us":percentile(50).as_micros(),"p95_us":percentile(95).as_micros(),"p99_us":percentile(99).as_micros(),"max_us":samples.last().unwrap().as_micros()});
            println!("service_perf {latency}");
            let after = stages::snapshot(&f, &p).await;
            let mut profile = stages::difference(&before, &after, mcp, concurrency, samples.len());
            profile["latency"] = latency;
            profiles.push(profile);
            let budget = if concurrency == 1 { 50 } else { 200 };
            assert!(
                percentile(95) < Duration::from_millis(budget),
                "p95 exceeded {budget}ms at concurrency {concurrency}: {samples:?}"
            );
        }
        let verified = f.ok(&p, "audit_verify", json!({})).await;
        assert!(verified["events"].as_u64().unwrap() >= 102);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
        let journal = f.journal();
        let audited: i64 = journal
            .query_row(
                "SELECT COUNT(*) FROM audit WHERE json_extract(event, '$.tool') = 'risk_preview'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            audited, 102,
            "every measured and warm-up read must be audited"
        );
        p.stop().await;
    }
    if let Ok(path) = std::env::var("TRUST_PROFILE_SERVICE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&profiles).unwrap()).unwrap();
    }
    for profile in profiles {
        println!("service_stages {profile}");
    }
}
