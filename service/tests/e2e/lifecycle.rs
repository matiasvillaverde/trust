use super::*;

pub(super) async fn verify_lifecycle_recovery() {
    for side in ["long", "short"] {
        let gateway = Gateway::start().await;
        let f = Fixture::new(45000, 8);
        let mut p = f.start(&gateway).await;
        assert_eq!(
            f.request(&p, "/metrics", None)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            f.request(&p, "/metrics", Some("reader"))
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let (account, trade) = f.seed(&p, side, "10", side).await;
        f.ok(
            &p,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
        )
        .await;
        let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":"submit-test"});
        let (http, mcp) = tokio::join!(
            f.call(&p, "trade_submit", args.clone(), false),
            f.call(&p, "trade_submit", args.clone(), true)
        );
        assert!(http["error"].is_null(), "{http}");
        assert_eq!(http["operation_id"], mcp["operation_id"]);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
        gateway.state.fail_activities.store(true, Ordering::SeqCst);
        for (entry, exit, expected) in [
            ("4", "0", "partially_filled"),
            ("10", "0", "filled"),
            ("10", "10", "closed_target"),
        ] {
            gateway.state.fills(entry, exit, false);
            let reply=f.call(&p,"trade_reconcile",json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("sync-{entry}-{exit}")}),true).await;
            assert_eq!(reply["error"]["code"], "reconciliation_incomplete");
            let data = f
                .ok(
                    &p,
                    "trade_get",
                    json!({"account_id":account,"trade_id":trade}),
                )
                .await;
            let ready = f
                .ok(&p, "account_readiness", json!({"account_id":account}))
                .await;
            assert_eq!(ready["halted"], true);
            // Public state spelling is defined by the domain enum serializer.
            let state = data["status"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .replace('_', "");
            assert_eq!(state, expected.replace('_', ""), "{data}");
        }
        gateway.state.fail_activities.store(false, Ordering::SeqCst);
        f.ok(
            &p,
            "trade_reconcile",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"sync-recovered"}),
        )
        .await;
        let balance = f
            .ok(&p, "balances_get", json!({"account_id":account}))
            .await;
        assert!(balance.to_string().contains("10100"), "{balance}");
        let recipient=f.ok(&p,"account_create",json!({"name":"recipient","account_type":"earnings","parent_account_id":account,"description":"E2E transfer","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":"recipient-account"})).await["id"].clone();
        f.ok(&p,"capital_transfer",json!({"account_id":account,"to_account_id":recipient,"amount":"1","reason":"E2E atomic transfer","idempotency_key":"transfer-once"})).await;
        let recipient_balance = f
            .ok(&p, "balances_get", json!({"account_id":recipient}))
            .await;
        assert!(
            recipient_balance
                .to_string()
                .contains("\"total_available\":\"1\""),
            "{recipient_balance}"
        );
        let invalid = f
            .call(&p, "capital_deposit", json!({"amount":SENTINEL}), false)
            .await;
        assert_eq!(error(&invalid), "invalid_argument");
        let conflict = f
            .call(
                &p,
                "trade_submit",
                json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
                true,
            )
            .await;
        assert_eq!(error(&conflict), "idempotency_conflict");
        let audit = f.ok(&p, "audit_list", json!({"limit":200})).await;
        let events = audit["events"].as_array().unwrap();
        assert!(events.iter().any(|e| e["event"]["outcome"] == "replayed"));
        assert!(events
            .iter()
            .any(|e| e["event"]["error_code"] == "idempotency_conflict"));
        assert!(events
            .iter()
            .any(|e| e["event"]["request_id"] == invalid["request_id"]));
        let submit = events
            .iter()
            .find(|e| {
                e["event"]["operation_id"] == http["operation_id"]
                    && e["event"]["outcome"] == "completed"
            })
            .unwrap();
        assert_eq!(submit["event"]["version"], 2);
        // Either concurrent transport can claim the intent. Only the winner
        // contacts the broker; the replay has its own request correlation ID.
        let submitting_request_id = submit["event"]["request_id"].clone();
        assert_eq!(submit["event"]["account_ids"], json!([account]));
        assert!(submit["event"]["duration_ms"].is_u64());
        assert_eq!(submit["event_hash"].as_str().unwrap().len(), 64);
        f.ok(&p, "audit_verify", json!({})).await;
        let metrics = f
            .request(&p, "/metrics", Some("owner"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        telemetry::assert_metrics(&metrics, account.as_str().unwrap());
        // Probes do not consume the ordinary global request budget.
        for _ in 0..610 {
            assert_eq!(
                f.request(&p, "/healthz", None)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
        }
        assert_eq!(
            f.request(&p, "/readyz", None)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        p.stop().await;
        let logs = p.log_text();
        assert!(!logs.contains(SENTINEL));
        assert!(!logs.contains(&f.token("owner")));
        assert!(logs.contains("activity_reconciliation_failed"));
        assert!(logs.contains("queue_wait_ms"));
        assert!(logs.contains("queue_wait_us"));
        let broker_logs: Vec<Value> = logs
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|event| event["fields"]["event"] == "broker_request")
            .collect();
        assert!(!broker_logs.is_empty());
        for event in &broker_logs {
            let fields = &event["fields"];
            assert!(fields["duration_us"].is_u64());
            assert!(!event.to_string().contains("DU_TEST"));
            assert!(fields["endpoint"]
                .as_str()
                .is_some_and(|endpoint| !endpoint.contains('/')));
        }
        assert!(
            broker_logs
                .iter()
                .any(|event| event["spans"].as_array().is_some_and(|spans| spans
                    .iter()
                    .any(|span| span["request_id"] == submitting_request_id))),
            "broker attempts must correlate to the originating request"
        );
        assert!(logs.contains(http["request_id"].as_str().unwrap()));
        let mut p = f.start(&gateway).await;
        let replay = f.call(&p, "trade_submit", args, true).await;
        assert_eq!(replay["operation_id"], http["operation_id"]);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
        p.stop().await;
    }
}
