#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::TradeCommandBuilder;

#[test]
fn interactive_trade_actions_with_no_eligible_trades_report_failure_without_writes() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "interactive_trade_actions_with_no_eligible_trades_report_failure_without_writes",
        || {
            let mut dispatcher = test_dispatcher();
            let (account, _) = seed_account_and_vehicle(&mut dispatcher);
            let balance = dispatcher
                .trust
                .search_balance(account.id, &model::Currency::USD)
                .unwrap();
            let receipts = dispatcher
                .trust
                .get_account_transactions(account.id)
                .unwrap();
            for name in [
                "manually-fill",
                "manually-stop",
                "manually-target",
                "manually-close",
                "watch",
                "search",
                "modify-stop",
                "modify-target",
            ] {
                crate::dialogs::scripted_reset();
                crate::dialogs::scripted_push_select(Ok(Some(0)));
                let matches = TradeCommandBuilder::new()
                    .manually_fill()
                    .manually_stop()
                    .manually_target()
                    .manually_close()
                    .watch_trade()
                    .search_trade()
                    .modify_stop()
                    .modify_target()
                    .build()
                    .get_matches_from(["trade", name]);
                println!("BEGIN:{name}");
                crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &matches)
                    .unwrap();
                println!("END:{name}");
                assert_eq!(
                    dispatcher
                        .trust
                        .search_balance(account.id, &model::Currency::USD)
                        .unwrap(),
                    balance
                );
                assert_eq!(
                    dispatcher
                        .trust
                        .get_account_transactions(account.id)
                        .unwrap(),
                    receipts
                );
                assert!(dispatcher
                    .trust
                    .search_trades(account.id, model::Status::New)
                    .unwrap()
                    .is_empty());
            }
            crate::dialogs::scripted_reset();
        },
        |output| {
            for name in [
                "manually-fill",
                "manually-stop",
                "manually-target",
                "manually-close",
                "watch",
                "search",
                "modify-stop",
                "modify-target",
            ] {
                let section = crate::test_support::capture::section(output, name);
                assert!(
                    section.to_lowercase().contains("error"),
                    "{name}: {section}"
                );
                assert!(
                    !section.contains("executed:") && !section.contains("Trade updated:"),
                    "{name}: {section}"
                );
            }
        },
    );
}
