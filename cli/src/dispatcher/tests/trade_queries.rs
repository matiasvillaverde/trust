#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::TradeCommandBuilder;
use model::Currency;
use model::Environment;
use model::Status;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_trade_audit_exports_verified_agent_first_json() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (_account, trade) = seed_new_trade(&mut dispatcher);
    let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
    dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");

    let output = std::env::temp_dir().join(format!("trust-audit-{}.json", Uuid::new_v4()));
    let output_text = output.to_string_lossy().to_string();
    let trade_id = trade.id.to_string();
    let matches = TradeCommandBuilder::new()
        .audit()
        .build()
        .get_matches_from([
            "trade",
            "audit",
            "--trade-id",
            &trade_id,
            "--format",
            "json",
            "--output",
            &output_text,
        ]);
    crate::dispatcher::trade_queries::trade_audit(
        &mut dispatcher.trust,
        matches.subcommand_matches("audit").expect("audit matches"),
    )
    .expect("audit export should succeed");

    let bytes = std::fs::read(&output).expect("audit export should be readable");
    let envelope: serde_json::Value =
        serde_json::from_slice(&bytes).expect("audit export should be valid JSON");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["verification"]["event_count"], 2);
    assert_eq!(
        envelope["events"]
            .as_array()
            .expect("events should be an array")
            .len(),
        2
    );
    std::fs::remove_file(output).expect("audit export should be removable");
}

#[test]
fn watch_without_open_trades_reports_the_blocker() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "watch_without_open_trades_reports_the_blocker",
        || {
            let mut dispatcher = test_dispatcher();
            dispatcher
                .trust
                .create_account(
                    "watch-empty",
                    "watch",
                    Environment::Paper,
                    dec!(20),
                    dec!(10),
                )
                .expect("create watch account");

            crate::dialogs::scripted_reset();
            crate::dialogs::scripted_push_select(Ok(Some(0)));
            let search_watch = TradeCommandBuilder::new()
                .watch_trade()
                .build()
                .get_matches_from(["trade", "watch"]);
            crate::dispatcher::trade_queries::create_watch(
                &mut dispatcher.trust,
                search_watch.subcommand_matches("watch").unwrap(),
            );

            crate::dialogs::scripted_push_select(Ok(Some(0)));
            let latest_watch = TradeCommandBuilder::new()
                .watch_trade()
                .build()
                .get_matches_from(["trade", "watch", "--latest"]);
            crate::dispatcher::trade_queries::create_watch(
                &mut dispatcher.trust,
                latest_watch.subcommand_matches("watch").unwrap(),
            );

            crate::dialogs::scripted_reset();
        },
        |output| {
            assert!(output.contains("Trade watch error:"), "{output}");
            assert!(
                output.contains("No trade found with status Submitted / PartiallyFilled / Filled"),
                "{output}"
            );
            assert_eq!(output.matches("Trade watch error:").count(), 2, "{output}");
        },
    );
}

#[test]
fn trade_queries_report_scope_pagination_and_reconciliation_failures() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "trade_queries_report_scope_pagination_and_reconciliation_failures",
        || {
            let mut dispatcher =
                test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
            let (account, trade) = seed_filled_trade(&mut dispatcher);
            let account_id = account.id.to_string();
            let trade_id = trade.id.to_string();

            let search_matches = |args: &[&str]| {
                let mut argv = vec!["trade", "search"];
                argv.extend_from_slice(args);
                TradeCommandBuilder::new()
                    .search_trade()
                    .build()
                    .get_matches_from(argv)
            };
            let list_open_matches = |args: &[&str]| {
                let mut argv = vec!["trade", "list-open"];
                argv.extend_from_slice(args);
                TradeCommandBuilder::new()
                    .list_open()
                    .build()
                    .get_matches_from(argv)
            };
            let reconcile_matches = |args: &[&str]| {
                let mut argv = vec!["trade", "reconcile"];
                argv.extend_from_slice(args);
                TradeCommandBuilder::new()
                    .reconcile()
                    .build()
                    .get_matches_from(argv)
            };

            let filtered_text = search_matches(&[
                "--account",
                &account_id,
                "--status",
                "filled",
                "--symbol",
                "aapl",
                "--from",
                "2000-01-01",
                "--to",
                "2999-12-31",
            ]);
            crate::dispatcher::trade_queries::search_trade(
                &mut dispatcher.trust,
                filtered_text.subcommand_matches("search").unwrap(),
            )
            .expect("filtered text search should succeed");

            let aggregate_json = search_matches(&["--symbol", "AAPL", "--format", "json"]);

            println!("BEGIN:aggregate_json");
            crate::dispatcher::trade_queries::search_trade(
                &mut dispatcher.trust,
                aggregate_json.subcommand_matches("search").unwrap(),
            )
            .expect("aggregate json search should succeed");
            println!("END:aggregate_json");

            let status_only = search_matches(&["--status", "filled"]);
            crate::dispatcher::trade_queries::search_trade(
                &mut dispatcher.trust,
                status_only.subcommand_matches("search").unwrap(),
            )
            .expect("status-only search should scan all accounts");

            let list_text = list_open_matches(&["--account", &account_id]);
            crate::dispatcher::trade_queries::list_open_trades(
                &mut dispatcher.trust,
                list_text.subcommand_matches("list-open").unwrap(),
            )
            .expect("account-scoped list-open should succeed");

            let list_json = list_open_matches(&["--format", "json"]);

            println!("BEGIN:list_json");
            crate::dispatcher::trade_queries::list_open_trades(
                &mut dispatcher.trust,
                list_json.subcommand_matches("list-open").unwrap(),
            )
            .expect("aggregate list-open json should succeed");
            println!("END:list_json");

            let reconcile_all_json = reconcile_matches(&["--format", "json"]);

            println!("BEGIN:reconcile_all_json");
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut dispatcher.trust,
                reconcile_all_json.subcommand_matches("reconcile").unwrap(),
            )
            .expect("aggregate reconcile should scan open trades");
            println!("END:reconcile_all_json");

            let reconcile_text = reconcile_matches(&["--account", &account_id]);
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut dispatcher.trust,
                reconcile_text.subcommand_matches("reconcile").unwrap(),
            )
            .expect("account-scoped reconcile should collect broker failures");

            let reconcile_json = reconcile_matches(&["--trade-id", &trade_id, "--format", "json"]);

            println!("BEGIN:reconcile_json");
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut dispatcher.trust,
                reconcile_json.subcommand_matches("reconcile").unwrap(),
            )
            .expect("trade-id reconcile should collect broker failure as json");
            println!("END:reconcile_json");

            let mut many_dispatcher = test_dispatcher();
            let (many_account, many_vehicle) = seed_account_and_vehicle(&mut many_dispatcher);
            for index in 0..51 {
                many_dispatcher
                    .trust
                    .create_trade(
                        model::DraftTrade {
                            account: many_account.clone(),
                            trading_vehicle: many_vehicle.clone(),
                            quantity: 1.into(),
                            category: model::TradeCategory::Long,
                            currency: Currency::USD,
                            thesis: Some(format!("setup-{index}")),
                            sector: Some("technology".to_string()),
                            asset_class: Some("equity".to_string()),
                            context: None,
                        },
                        dec!(95),
                        dec!(100),
                        dec!(110),
                    )
                    .expect("bulk trade should be created");
            }
            let many_search = search_matches(&["--account", &many_account.id.to_string()]);
            crate::dispatcher::trade_queries::search_trade(
                &mut many_dispatcher.trust,
                many_search.subcommand_matches("search").unwrap(),
            )
            .expect("large text search should print truncation notice");

            let mut broker = StubBroker::quote_trade_fixture();
            broker.submit_succeeds = true;
            let mut failing_sync_dispatcher = test_dispatcher_with_broker(Box::new(broker));
            let (sync_account, sync_trade) = seed_new_trade(&mut failing_sync_dispatcher);
            let (funded, _, _, _) = failing_sync_dispatcher
                .trust
                .fund_trade(&sync_trade)
                .expect("fund trade");
            failing_sync_dispatcher
                .trust
                .submit_trade(&funded)
                .expect("submit trade");
            let failing_reconcile = reconcile_matches(&["--account", &sync_account.id.to_string()]);
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut failing_sync_dispatcher.trust,
                failing_reconcile.subcommand_matches("reconcile").unwrap(),
            )
            .expect("reconcile should print broker sync failures in text mode");
            let submitted_trade = failing_sync_dispatcher
                .trust
                .search_trades(sync_account.id, Status::Submitted)
                .expect("submitted trade should load")
                .into_iter()
                .find(|candidate| candidate.id == sync_trade.id)
                .expect("submitted trade should be persisted");
            let failing_trade_id_reconcile =
                reconcile_matches(&["--trade-id", &submitted_trade.id.to_string()]);
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut failing_sync_dispatcher.trust,
                failing_trade_id_reconcile
                    .subcommand_matches("reconcile")
                    .unwrap(),
            )
            .expect("trade-id reconcile should print broker sync failure");

            let missing_account_reconcile =
                reconcile_matches(&["--trade-id", &Uuid::nil().to_string(), "--format", "json"]);
            let mut missing_account_dispatcher = test_dispatcher_with_read_failures(
                crate::test_support::ReadFailureFactory::submitted_trade_missing_account(),
            );

            println!("BEGIN:missing_account_reconcile");
            crate::dispatcher::trade_queries::reconcile_trades(
                &mut missing_account_dispatcher.trust,
                missing_account_reconcile
                    .subcommand_matches("reconcile")
                    .unwrap(),
            )
            .expect("reconcile should collect account lookup failures");
            println!("END:missing_account_reconcile");
        },
        |output| {
            assert!(output.contains("Trades found: 1"), "{output}");
            assert!(output.contains("Open trades: 1"), "{output}");
            assert!(output.contains("Trades found: 51"), "{output}");
            assert!(output.contains("... (1 more trades omitted)"), "{output}");
            assert!(
                output.contains("Reconcile completed: success=0 failed=1"),
                "{output}"
            );
            assert!(output.contains("not implemented in test stub"), "{output}");
            use crate::test_support::capture::json_section;
            let found = json_section(output, "aggregate_json");
            assert_eq!(found["data"]["count"], 1);
            let trade = &found["data"]["trades"][0];
            assert_eq!(trade["symbol"], "AAPL");
            assert_eq!(trade["status"], "filled");
            assert_eq!(trade["entry_price"], "100");
            assert_eq!(trade["stop_price"], "95");
            assert_eq!(trade["target_price"], "110");
            let open = json_section(output, "list_json");
            assert_eq!(open["data"]["count"], 1);
            assert_eq!(open["data"]["trades"][0]["id"], trade["id"]);
            let reconciled = json_section(output, "reconcile_all_json");
            assert_eq!(reconciled["data"]["success_count"], 1);
            assert_eq!(reconciled["data"]["failure_count"], 0);
            let missing = json_section(output, "missing_account_reconcile");
            assert_eq!(missing["data"]["success_count"], 0);
            assert_eq!(missing["data"]["failure_count"], 1);
            assert!(!missing["data"]["failures"][0]["error"]
                .as_str()
                .unwrap()
                .is_empty());
        },
    );
}

#[test]
fn test_trade_search_validates_date_filters_before_querying() {
    let mut dispatcher = test_dispatcher();
    let search_matches = |args: &[&str]| {
        let mut argv = vec!["trade", "search"];
        argv.extend_from_slice(args);
        TradeCommandBuilder::new()
            .search_trade()
            .build()
            .get_matches_from(argv)
    };

    let invalid_from = search_matches(&["--symbol", "AAPL", "--from", "not-a-date"]);
    let error = crate::dispatcher::trade_queries::search_trade(
        &mut dispatcher.trust,
        invalid_from.subcommand_matches("search").unwrap(),
    )
    .expect_err("invalid --from should fail");
    assert!(error.to_string().contains("invalid_date"));

    let invalid_to = search_matches(&["--symbol", "AAPL", "--to", "not-a-date"]);
    let error = crate::dispatcher::trade_queries::search_trade(
        &mut dispatcher.trust,
        invalid_to.subcommand_matches("search").unwrap(),
    )
    .expect_err("invalid --to should fail");
    assert!(error.to_string().contains("invalid_date"));

    let reversed = search_matches(&[
        "--symbol",
        "AAPL",
        "--from",
        "2026-02-02",
        "--to",
        "2026-02-01",
    ]);
    let error = crate::dispatcher::trade_queries::search_trade(
        &mut dispatcher.trust,
        reversed.subcommand_matches("search").unwrap(),
    )
    .expect_err("reversed date window should fail");
    assert!(error.to_string().contains("invalid_date_range"));
}
