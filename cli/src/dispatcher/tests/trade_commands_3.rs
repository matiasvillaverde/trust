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
fn test_dispatch_trade_interactive_variants_are_reachable() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut dispatcher = test_dispatcher();
    let cases: Vec<Vec<&str>> = vec![
        vec!["trade", "manually-fill"],
        vec!["trade", "manually-stop"],
        vec!["trade", "manually-target"],
        vec!["trade", "manually-close"],
        vec!["trade", "manually-close", "--auto-distribute"],
        vec!["trade", "watch"],
        vec!["trade", "search"],
        vec!["trade", "modify-stop"],
        vec!["trade", "modify-target"],
    ];

    for argv in cases {
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
            .get_matches_from(argv);

        let result = catch_unwind(AssertUnwindSafe(|| {
            crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &matches)
        }));
        let _ = result;
    }
}
