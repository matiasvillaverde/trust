//! Manual executions and their public report/schema contracts.
use super::*;

#[tokio::test]
async fn manual_tools_record_actual_prices_and_conserve_long_short_cash() {
    for (side, stop, target, fill) in [("long", "90", "120", "99"), ("short", "110", "80", "101")] {
        let mut fixture = Fixture::new();
        let distribution_secret = fixture._directory.path().join("distribution-secret");
        {
            use std::io::Write;
            trust_security::private_file(&distribution_secret, true)
                .unwrap()
                .write_all(Uuid::new_v4().to_string().as_bytes())
                .unwrap();
        }
        Arc::make_mut(&mut fixture.config).distribution_password_file = Some(distribution_secret);
        let (service, worker) = fixture.start();
        let account = fixture.account(&service, side).await;
        let setup = [
            (
                "capital_deposit",
                json!({"account_id":account,"amount":"10000","idempotency_key":"setup-deposit"}),
            ),
            (
                "risk_rule_create",
                json!({"account_id":account,"kind":"per_trade","percentage":"0.1","priority":"error","description":"precise cap","idempotency_key":"setup-policy"}),
            ),
            (
                "risk_rule_create",
                json!({"account_id":account,"kind":"per_month","percentage":"5","priority":"error","description":"monthly cap","idempotency_key":"monthly-policy"}),
            ),
        ];
        for (tool, args) in setup {
            let reply = fixture.call(&service, tool, args).await;
            assert!(reply.error.is_none(), "{tool}: {reply:?}");
        }
        let instrument=fixture.call(&service,"instrument_create",json!({"symbol":"RELIABLE","category":"stock","broker":"ibkr","idempotency_key":"setup-instrument"})).await;
        assert!(instrument.error.is_none(), "{instrument:?}");
        let instrument = instrument.data.unwrap()["id"].clone();
        assert!(fixture.call(&service,"instrument_update",json!({"instrument_id":instrument,"broker_asset_id":"1234","idempotency_key":"native-test"})).await.error.is_none());
        let trade=fixture.call(&service,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":"1","entry":"100","stop":stop,"target":target,"idempotency_key":"prepare-trade"})).await;
        assert!(trade.error.is_none(), "{trade:?}");
        let id = trade.data.unwrap()["id"].clone();
        let preview = fixture
            .call(
                &service,
                "risk_preview",
                json!({"account_id":account,"entry":"100","stop":stop}),
            )
            .await;
        assert!(preview.error.is_none(), "{preview:?}");
        for (tool, args) in [
            (
                "trade_fund",
                json!({"account_id":account,"trade_id":id,"idempotency_key":"fund-trade-once"}),
            ),
            (
                "trade_record_fill",
                json!({"account_id":account,"trade_id":id,"price":fill,"fee":"0.5","idempotency_key":"record-entry-once"}),
            ),
            (
                "trade_record_target",
                json!({"account_id":account,"trade_id":id,"price":target,"fee":"0.25","idempotency_key":"record-exit-once"}),
            ),
        ] {
            let first = fixture.call(&service, tool, args.clone()).await;
            assert!(first.error.is_none(), "{tool}: {first:?}");
            let again = fixture.call(&service, tool, args).await;
            assert_eq!(first.operation_id, again.operation_id);
            assert_eq!(first.data, again.data);
        }
        let state = fixture
            .call(
                &service,
                "trade_get",
                json!({"account_id":account,"trade_id":id}),
            )
            .await
            .data
            .unwrap();
        assert_eq!(state["entry"]["average_filled_price"], fill);
        assert_eq!(state["entry"]["filled_quantity"], "1");
        let balances = fixture
            .call(&service, "balances_get", json!({"account_id":account}))
            .await
            .data
            .unwrap();
        assert_eq!(
            balances["items"][0]["total_available"]
                .as_str()
                .unwrap()
                .parse::<Decimal>()
                .unwrap(),
            "10020.25".parse::<Decimal>().unwrap()
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        // Exercise successful structured contracts beyond order/account mutations.
        for (tool, args) in [
            ("grade_preview", json!({"account_id":account,"trade_id":id})),
            (
                "grade_record",
                json!({"account_id":account,"trade_id":id,"idempotency_key":"schema-grade-record"}),
            ),
            ("grade_get", json!({"account_id":account,"trade_id":id})),
            ("grades_list", json!({"account_id":account})),
            ("level_policy_get", json!({"account_id":account})),
            ("level_progress", json!({"account_id":account})),
            ("level_history", json!({"account_id":account})),
            (
                "distribution_policy_set",
                json!({"account_id":account,"earnings":"0","tax":"0","reinvestment":"1","insurance":"0","minimum":"100","idempotency_key":"schema-distribution-policy"}),
            ),
            ("distribution_policy_get", json!({"account_id":account})),
            ("distribution_history", json!({"account_id":account})),
            ("advisor_status", json!({"account_id":account})),
            ("advisor_history", json!({"account_id":account})),
            (
                "advisor_check",
                json!({"account_id":account,"symbol":"RELIABLE","entry":"100","quantity":"1"}),
            ),
            (
                "advisor_configure",
                json!({"account_id":account,"sector_limit_pct":"40","asset_class_limit_pct":"80","single_position_limit_pct":"20","idempotency_key":"schema-advisor-config"}),
            ),
            (
                "metrics_get",
                json!({"account_id":account,"kind":"bond","face_value":"1000","price":"990","annual_coupon_rate_pct":"5","years_to_maturity":"10","quantity":1}),
            ),
        ] {
            let reply = fixture.call(&service, tool, args).await;
            assert!(reply.error.is_none(), "{tool}: {reply:?}");
        }
        // Valid local reports must succeed. Only the unavailable market data
        // dependency may reject a benchmark; an arbitrary error is not evidence.
        for kind in [
            "summary",
            "performance",
            "drawdown",
            "risk",
            "concentration",
            "metrics",
            "attribution",
            "benchmark",
            "timeline",
            "bias",
            "wash_sales",
            "tax",
        ] {
            let reply = fixture
                .call(
                    &service,
                    "report_get",
                    json!({"account_id":account,"kind":kind}),
                )
                .await;
            if kind == "benchmark" {
                assert_eq!(
                    reply.error.as_ref().map(|e| e.code.as_str()),
                    Some("domain_rejected"),
                    "{reply:?}"
                );
                assert!(reply.data.is_none());
            } else {
                assert!(reply.error.is_none(), "report {kind}: {reply:?}");
                assert!(reply.data.is_some(), "report {kind} has no payload");
            }
        }
        worker.shutdown().await.unwrap();
    }
}
