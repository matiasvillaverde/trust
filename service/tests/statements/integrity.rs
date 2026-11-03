use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_statement_storage_preserves_evidence_and_blocks_risk_instead_of_discarding_a_discrepancy(
) {
    let (g, f, mut p, a, _) = setup().await;
    let original = report("10000", "", "");
    let before = snapshot(&f);
    let mut last_id = Value::Null;
    for index in 0..1000 {
        // Exercise durable capacity across real restarts while keeping each
        // synthetic import batch within the service's normal request quota.
        if index > 0 && index % 50 == 0 {
            p.stop().await;
            p = f.start(&g).await;
        }
        let source = original.replace(
            "statement-private-marker",
            &format!("statement-private-marker-{index}"),
        );
        let data = f
            .ok(
                &p,
                "broker_statement_import",
                import(&a, &source, &format!("capacity-{index}")),
            )
            .await;
        assert_eq!(data["reconciliation"]["result"]["status"], "matched");
        last_id = data["source"]["statement_id"].clone();
    }
    let recheck = f
        .ok(
            &p,
            "broker_statement_reconcile",
            json!({"account_id":a,"statement_id":last_id,"idempotency_key":"capacity-recheck"}),
        )
        .await;
    assert_eq!(recheck["reconciliation"]["result"]["status"], "matched");
    let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(ready["halted"], false);
    assert_eq!(ready["unresolved_operation_count"], 0);
    let denied = f
        .call(
            &p,
            "broker_statement_import",
            import(&a, &report("9999", "", ""), "capacity-discrepancy"),
            true,
        )
        .await;
    assert_eq!(denied["error"]["code"], "audit_unavailable", "{denied}");
    let unavailable = f
        .call(&p, "account_readiness", json!({"account_id":a}), false)
        .await;
    assert_eq!(unavailable["error"]["code"], "unavailable");
    let db = f.journal();
    assert_eq!(
        db.query_row("SELECT count(*) FROM broker_statements", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1000
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM statement_reconciliations", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        1001
    );
    assert_eq!(
        db.query_row(
            "SELECT state FROM operations WHERE request_key='capacity-discrepancy'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "running"
    );
    assert_eq!(snapshot(&f), before);
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
    let mut restarted = f.start(&g).await;
    let ready = f
        .ok(&restarted, "account_readiness", json!({"account_id":a}))
        .await;
    assert_eq!(ready["ready_for_new_risk"], false);
    assert_eq!(ready["unresolved_operation_count"], 1);
    assert_eq!(snapshot(&f), before);
    restarted.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_execution_rows_cannot_hide_recorded_fills_behind_an_apparently_matching_empty_report(
) {
    let (g, f, mut p, a, t) = setup().await;
    close_with_fills(&f, &p, &g, &a, &t, false).await;
    let db = financial(&f);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM executions WHERE source='account_activities'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM fee_activities", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.execute(
            "DELETE FROM executions WHERE source='account_activities'",
            []
        )
        .unwrap(),
        2
    );
    let before = snapshot(&f);
    let data = f
        .ok(
            &p,
            "broker_statement_import",
            import(&a, &report("10000", "", ""), "missing-execution-evidence"),
        )
        .await;
    assert_eq!(
        data["reconciliation"]["result"]["status"], "inconclusive",
        "{data}"
    );
    assert_eq!(
        data["reconciliation"]["result"]["issues"][0]["code"],
        "trust_ledger_evidence_unavailable"
    );
    let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(ready["halted"], true);
    assert_eq!(ready["unreconciled_statement_count"], 1);
    assert_eq!(snapshot(&f), before);
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 1);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn audit_verify_checks_older_attempts_and_restart_refuses_modified_statement_bytes() {
    for old_attempt in [false, true] {
        let (g, f, mut p, a, _) = setup().await;
        let data = f
            .ok(
                &p,
                "broker_statement_import",
                import(&a, &report("10000", "", ""), "verify-all-attempts"),
            )
            .await;
        f.ok(&p,"broker_statement_reconcile",json!({"account_id":a,"statement_id":data["source"]["statement_id"],"idempotency_key":"verify-second-attempt"})).await;
        let db = f.journal();
        if old_attempt {
            db.execute_batch("DROP TRIGGER statement_attempts_no_update; UPDATE statement_reconciliations SET result=json_set(result,'$.result.checked_at','2026-03-18T00:00:00Z') WHERE sequence=(SELECT MIN(sequence) FROM statement_reconciliations);").unwrap();
            let denied = f.call(&p, "audit_verify", json!({}), true).await;
            assert_eq!(denied["error"]["code"], "audit_integrity_failed");
        } else {
            db.execute_batch("DROP TRIGGER broker_statements_no_update; UPDATE broker_statements SET source_xml=replace(source_xml,'10000','10001');").unwrap();
        }
        p.stop().await;
        let mut restarted = f.spawn(&g);
        assert!(
            !restarted.wait().await.success(),
            "corrupt evidence must prevent startup"
        );
        assert!(!restarted.log_text().contains("statement-private-marker"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statement_source_result_and_deletion_tampering_stop_the_worker() {
    for sql in [
        "UPDATE broker_statements SET source_xml=replace(source_xml,'10000','10001')",
        "UPDATE broker_statements SET metadata='{'",
        "UPDATE broker_statements SET metadata=json_remove(metadata,'$.scope')",
        "UPDATE statement_reconciliations SET result='{'",
        "UPDATE statement_reconciliations SET result=json_set(result,'$.result.checked_at','invalid-date')",
        "UPDATE statement_reconciliations SET result=json_set(result,'$.result.status','matched','$.result.issue_count',0)",
        "DELETE FROM statement_reconciliations WHERE sequence=(SELECT MAX(sequence) FROM statement_reconciliations)",
    ] {
        let (g,f,mut p,a,_)=setup().await;
        // An internally unbalanced statement is retained as inconclusive and halts.
        let data=f.ok(&p,"broker_statement_import",import(&a,&report("9999","",""),"tamper-import")).await;
        assert_eq!(data["reconciliation"]["result"]["status"],"inconclusive");
        let id=data["source"]["statement_id"].clone();
        f.ok(&p,"broker_statement_reconcile",json!({"account_id":a,"statement_id":id,"idempotency_key":"tamper-attempt"})).await;
        let before=snapshot(&f);
        let db=f.journal();
        assert!(db.execute_batch(sql).is_err(),"originals and attempts must be append-only");
        db.execute_batch("DROP TRIGGER broker_statements_no_update; DROP TRIGGER broker_statements_no_delete; DROP TRIGGER statement_attempts_no_update; DROP TRIGGER statement_attempts_no_delete;").unwrap();
        db.execute_batch(sql).unwrap();
        let denied=f.call(&p,"broker_statement_get",json!({"account_id":a,"statement_id":id}),true).await;
        assert_eq!(denied["error"]["code"],"audit_integrity_failed","{denied}");
        let unavailable=f.call(&p,"account_readiness",json!({"account_id":a}),false).await;
        assert_eq!(unavailable["error"]["code"],"unavailable");
        assert_eq!(snapshot(&f),before);assert_eq!(g.state.submissions.load(Ordering::SeqCst),0);
        p.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statement_failure_rolls_back_artifact_result_audit_and_halt_as_one_unit() {
    for table in [
        "broker_statements",
        "statement_reconciliations",
        "account_safety_history",
    ] {
        let (g, f, mut p, a, _) = setup().await;
        let db = f.journal();
        db.execute_batch(&format!("CREATE TRIGGER fail_statement_state BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;")).unwrap();
        let before = snapshot(&f);
        let denied = f
            .call(
                &p,
                "broker_statement_import",
                import(&a, &report("9999", "", ""), "storage-failure"),
                true,
            )
            .await;
        assert_eq!(denied["error"]["code"], "audit_unavailable", "{denied}");
        for table in ["broker_statements", "statement_reconciliations"] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(db.query_row("SELECT COUNT(*) FROM audit WHERE json_extract(event,'$.tool') IN ('broker_statement_source','broker_statement_reconciliation')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(
            db.query_row(
                "SELECT state FROM operations WHERE tool='broker_statement_import'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "running"
        );
        assert_eq!(snapshot(&f), before);
        assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
        p.stop().await;
        db.execute_batch("DROP TRIGGER fail_statement_state")
            .unwrap();
        drop(db);
        let mut p = f.start(&g).await;
        let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
        assert_eq!(ready["ready_for_new_risk"], false);
        assert_eq!(ready["unresolved_operation_count"], 1);
        assert_eq!(snapshot(&f), before);
        p.stop().await;
    }
}
