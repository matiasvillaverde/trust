use super::{json, Fixture, Gateway};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn history_pages_match_over_mcp_http_and_restart_and_account_units_are_explicit() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(10000, 32);
    let mut process = f.start(&gateway).await;
    for (index, tax, earnings) in [(0, "-1", "0"), (1, "80", "80")] {
        let reply = f.call(&process, "account_create", json!({
            "name":format!("invalid-{index}"),"description":"invalid reserves","environment":"paper",
            "broker":"ibkr","broker_account_id":"DU_TEST","taxes_percentage":tax,
            "earnings_percentage":earnings,"idempotency_key":format!("invalid-account-{index}")
        }), true).await;
        assert_eq!(reply["error"]["code"], "invalid_argument");
    }
    assert_eq!(f.ok(&process, "accounts_list", json!({})).await["total"], 0);
    let (account, trade) = f.seed(&process, "long", "1", "history-pages").await;
    let other = f
        .ok(
            &process,
            "account_create",
            json!({
                "name":"one-percent","description":"percentage points","environment":"paper",
                "broker":"ibkr","broker_account_id":"DU_TEST","taxes_percentage":"1",
                "earnings_percentage":"0","idempotency_key":"one-percent-account"
            }),
        )
        .await;
    assert_eq!(other["taxes_percentage"], "1");
    f.ok(
        &process,
        "capital_deposit",
        json!({"account_id":other["id"],"amount":"999","idempotency_key":"other-history-deposit"}),
    )
    .await;
    let original = f
        .ok(
            &process,
            "trade_get",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await;
    for index in 0..3 {
        f.ok(&process,"capital_deposit",json!({"account_id":account,"amount":"12.345","idempotency_key":format!("history-deposit-{index}")})).await;
        f.ok(
            &process,
            "trade_prepare",
            json!({
                "account_id":account,"instrument_id":original["trading_vehicle"]["id"],
                "side":"long","quantity":"1","entry":"100","stop":"95","target":"110",
                "idempotency_key":format!("history-trade-{index}")
            }),
        )
        .await;
    }
    for tool in ["trades_list", "transactions_list"] {
        let args = json!({"account_id":account,"limit":2,"offset":2});
        let mcp = f.call(&process, tool, args.clone(), true).await;
        let http = f.call(&process, tool, args.clone(), false).await;
        assert!(mcp["error"].is_null(), "{mcp}");
        assert_eq!(mcp["data"], http["data"]);
        assert_eq!(mcp["data"]["total"], 4);
        assert_eq!(mcp["data"]["items"].as_array().unwrap().len(), 2);
        assert!(mcp["data"]["next_offset"].is_null());
        for item in mcp["data"]["items"].as_array().unwrap() {
            assert_eq!(item["account_id"], account);
        }
        let first = f
            .ok(
                &process,
                tool,
                json!({"account_id":account,"limit":2,"offset":0}),
            )
            .await;
        assert_eq!(first["next_offset"], 2);
        let first_ids: Vec<_> = first["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].clone())
            .collect();
        assert!(mcp["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| !first_ids.contains(&item["id"])));
        process.stop().await;
        process = f.start(&gateway).await;
        assert_eq!(f.ok(&process, tool, args).await, mcp["data"]);
    }
    assert_eq!(
        f.ok(
            &process,
            "trades_list",
            json!({"account_id":account,"symbol":"test","status":"new"})
        )
        .await["total"],
        4
    );
    assert_eq!(
        f.ok(
            &process,
            "trades_list",
            json!({"account_id":account,"open_only":true})
        )
        .await["total"],
        0
    );
    process.stop().await;
}
