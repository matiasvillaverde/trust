use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statement_cash_includes_every_linked_profile_and_excludes_other_broker_accounts() {
    let (g, f, mut p, a, _) = setup().await;
    let mut linked = Value::Null;
    for (label, broker, amount) in [
        ("linked", "DU_TEST", "5000"),
        ("unrelated", "DU_OTHER", "2000"),
    ] {
        let id=f.ok(&p,"account_create",json!({"name":label,"description":"scope contract","environment":"paper","broker":"ibkr","broker_account_id":broker,"idempotency_key":format!("account-{label}")})).await["id"].clone();
        f.ok(
            &p,
            "capital_deposit",
            json!({"account_id":id,"amount":amount,"idempotency_key":format!("deposit-{label}")}),
        )
        .await;
        if label == "linked" {
            linked = id;
        }
    }
    past_capital(&f);
    let before = snapshot(&f);
    let matched = f
        .ok(
            &p,
            "broker_statement_import",
            import(
                &a,
                &report("15000", "", "")
                    .replace("startingCash=\"10000\"", "startingCash=\"15000\""),
                "whole-scope",
            ),
        )
        .await;
    assert_eq!(
        matched["reconciliation"]["result"]["status"], "matched",
        "{matched}"
    );
    assert_eq!(
        matched["reconciliation"]["result"]["cash"][0]["trust_closing"],
        "15000"
    );
    let source = f
        .ok(
            &p,
            "broker_statement_get",
            json!({"account_id":linked,"statement_id":matched["source"]["statement_id"]}),
        )
        .await;
    assert_eq!(source, matched);
    let unresolved = f
        .ok(
            &p,
            "broker_statement_import",
            import(
                &a,
                &report("15001", "", "")
                    .replace("startingCash=\"10000\"", "startingCash=\"15001\""),
                "whole-scope-mismatch",
            ),
        )
        .await;
    assert_eq!(
        unresolved["reconciliation"]["result"]["status"],
        "discrepancies"
    );
    let ready = f
        .ok(&p, "account_readiness", json!({"account_id":linked}))
        .await;
    assert_eq!(ready["unreconciled_statement_count"], 1);
    assert_eq!(ready["halted"], true);
    assert_eq!(snapshot(&f), before);
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statement_tools_require_owner_mfa_and_authorization_to_every_linked_profile_even_on_replay(
) {
    let (g, f, mut p, a, _) = setup().await;
    let args = import(&a, &report("10000", "", ""), "auth-import");
    let data = f.ok(&p, "broker_statement_import", args.clone()).await;
    let id = data["source"]["statement_id"].clone();
    let requests = [
        ("broker_statement_import", args.clone()),
        (
            "broker_statement_get",
            json!({"account_id":a,"statement_id":id,"include_source":true}),
        ),
        ("broker_statements_list", json!({"account_id":a})),
        (
            "broker_statement_reconcile",
            json!({"account_id":a,"statement_id":id,"idempotency_key":"auth-check"}),
        ),
    ];
    let before = snapshot(&f);
    for (name, args) in &requests {
        let denied = f
            .call_as(&p, "agent", name, args.clone(), true)
            .await
            .unwrap();
        assert_eq!(denied["error"]["code"], "forbidden", "{denied}");
        assert!(denied["data"].is_null());
        let response = f
            .client
            .post(format!("{}/v1/tools/{name}", p.url))
            .header("host", "localhost:8000")
            .bearer_auth(f.token_with_mfa("owner", false))
            .json(args)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(!response
            .text()
            .await
            .unwrap()
            .contains("statement-private-marker"));
    }
    assert_eq!(snapshot(&f), before);
    f.ok(&p,"account_create",json!({"name":"linked-profile","description":"same broker risk scope","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":"linked-profile"})).await;
    p.stop().await;
    let path = f.dir.path().join("config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["grants"]["owner"]["all_accounts"] = json!(false);
    config["grants"]["owner"]["accounts"] = json!([a]);
    std::fs::write(path, config.to_string()).unwrap();
    let mut p = f.start(&g).await;
    for (name, args) in &requests {
        let denied = f.call(&p, name, args.clone(), true).await;
        assert_eq!(denied["error"]["code"], "forbidden", "{name}: {denied}");
        assert!(denied["data"].is_null());
    }
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_xml_confirmation_and_cross_account_reads_do_not_mutate_books_or_expose_sources() {
    let (g, f, mut p, a, _) = setup().await;
    let mut args = import(&a, &report("10000", "", ""), "confirmation");
    args["complete_report"] = json!(false);
    let before = snapshot(&f);
    let denied = f.call(&p, "broker_statement_import", args, true).await;
    assert_eq!(denied["error"]["code"], "invalid_argument");
    let malicious = report("10000", "", "").replace("DU_TEST", "DU_OTHER");
    let denied = f
        .call(
            &p,
            "broker_statement_import",
            import(&a, &malicious, "bad-account"),
            true,
        )
        .await;
    assert_eq!(
        denied["error"]["code"],
        "statement_structure_or_account_mismatch"
    );
    let list = f
        .ok(&p, "broker_statements_list", json!({"account_id":a}))
        .await;
    assert_eq!(list["total"], 0);
    assert_eq!(snapshot(&f), before);
    let data = f
        .ok(
            &p,
            "broker_statement_import",
            import(&a, &report("10000", "", ""), "good-account"),
        )
        .await;
    let other=f.ok(&p,"account_create",json!({"name":"other-broker-account","description":"isolated","environment":"paper","broker":"ibkr","broker_account_id":"DU_OTHER","idempotency_key":"other-account"})).await["id"].clone();
    let denied=f.call(&p,"broker_statement_get",json!({"account_id":other,"statement_id":data["source"]["statement_id"],"include_source":true}),false).await;
    assert_eq!(denied["error"]["code"], "statement_not_found");
    assert!(!denied.to_string().contains("statement-private-marker"));
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}
