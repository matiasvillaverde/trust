//! Reports over recorded trades, not fabricated status/P&L fields.
#![cfg(feature = "legacy-cli")]
mod support;

use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Currency, DraftTrade, Environment, TradeCategory, TradingVehicleCategory,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::Value;
use std::str::FromStr;
use support::TestDatabase;

struct Reports {
    database: TestDatabase,
    account: Account,
    other: Account,
}

impl Reports {
    fn new() -> Self {
        let database = TestDatabase::new();
        let mut trust =
            TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new(&database.url())), vec![]);
        let account = trust
            .create_account(
                "reports",
                "recorded outcomes",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .unwrap();
        let other = trust
            .create_account(
                "isolated",
                "no trading",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .unwrap();
        for (account, amount) in [(&account, dec!(10000)), (&other, dec!(777.77))] {
            trust
                .create_transaction(
                    account,
                    &TransactionCategory::Deposit,
                    amount,
                    &Currency::USD,
                )
                .unwrap();
        }
        let instrument = trust
            .create_trading_vehicle("PRODUCT", None, &TradingVehicleCategory::Stock, "alpaca")
            .unwrap();
        for (name, sector, entry, exit, fee, stopped) in [
            (
                "winner",
                "Technology",
                dec!(99),
                Some(dec!(110)),
                dec!(0.5),
                false,
            ),
            (
                "loser",
                "Healthcare",
                dec!(100),
                Some(dec!(95)),
                dec!(0.5),
                true,
            ),
            (
                "flat",
                "Technology",
                dec!(100),
                Some(dec!(100)),
                dec!(0),
                false,
            ),
            ("open", "Technology", dec!(100), None, dec!(0), false),
        ] {
            let trade = trust
                .create_trade(
                    DraftTrade {
                        account: account.clone(),
                        trading_vehicle: instrument.clone(),
                        quantity: dec!(1),
                        category: TradeCategory::Long,
                        currency: Currency::USD,
                        thesis: Some(name.into()),
                        sector: Some(sector.into()),
                        asset_class: Some("Stocks".into()),
                        context: None,
                    },
                    dec!(95),
                    dec!(100),
                    dec!(110),
                )
                .unwrap();
            let (mut funded, ..) = trust.fund_trade(&trade).unwrap();
            funded.entry.average_filled_price = Some(entry);
            let (mut filled, _) = trust.fill_trade(&funded, fee).unwrap();
            if let Some(exit) = exit {
                if stopped {
                    filled.safety_stop.average_filled_price = Some(exit);
                    trust.stop_trade(&filled, fee).unwrap();
                } else {
                    filled.target.average_filled_price = Some(exit);
                    trust.target_acquired(&filled, fee).unwrap();
                }
            }
        }
        Self {
            database,
            account,
            other,
        }
    }

    fn json(&self, kind: &str, account: &Account, extra: &[&str]) -> Value {
        let output = support::legacy_command(&self.database.url(), "product-contract")
            .args([
                "report",
                kind,
                "--account",
                &account.id.to_string(),
                "--format",
                "json",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("one machine-readable result")
    }

    fn text(&self, kind: &str, account: &Account, zen: bool, extra: &[&str]) -> String {
        let mut command = support::legacy_command(&self.database.url(), "product-contract");
        if zen {
            command.arg("--zen");
        }
        let output = command
            .args(["report", kind, "--account", &account.id.to_string()])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{kind}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

fn money(value: &Value) -> Decimal {
    Decimal::from_str(value.as_str().expect("money string")).unwrap()
}

#[test]
fn performance_and_summary_use_persisted_wins_losses_fees_and_breakeven_trades() {
    let f = Reports::new();
    let report = f.json("performance", &f.account, &[]);
    let stats = &report["data"];
    assert_eq!(stats["total_trades"], 3);
    assert_eq!(stats["winning_trades"], 1);
    assert_eq!(stats["losing_trades"], 1);
    assert_eq!(money(&stats["average_win"]), dec!(10));
    assert_eq!(money(&stats["average_loss"]), dec!(-6));
    assert_eq!(report["consistency"]["trade_count_balanced"], true);
    let summary = f.json("summary", &f.account, &[]);
    // This legacy field reports ledger cash: 10,000 + 4 realized - 100 deployed.
    // Broker marked-to-market equity is a separate service risk input.
    assert_eq!(money(&summary["data"]["equity"]), dec!(9904));
    assert_eq!(summary["data"]["performance"]["total_trades"], 3);
    let other = f.json("summary", &f.other, &[]);
    assert_eq!(money(&other["data"]["equity"]), dec!(777.77));
    assert_eq!(other["data"]["performance"]["total_trades"], 0);
    // A separate invocation reopens the persisted ledger and must agree.
    assert_eq!(f.json("performance", &f.account, &[])["data"], *stats);
}

#[test]
fn concentration_reports_separate_realized_pnl_from_open_exposure_and_respect_scope() {
    let f = Reports::new();
    let all = f.json("concentration", &f.account, &[]);
    let groups = all["data"]["sector"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    for (name, count, pnl, risk) in [
        ("Technology", 3, dec!(10), dec!(100)),
        ("Healthcare", 1, dec!(-6), dec!(0)),
    ] {
        let group = groups.iter().find(|g| g["name"] == name).unwrap();
        assert_eq!(group["trade_count"], count);
        assert_eq!(money(&group["realized_pnl"]), pnl);
        assert_eq!(money(&group["current_open_risk"]), risk);
    }
    let open = f.json("concentration", &f.account, &["--open-only"]);
    let groups = open["data"]["sector"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["name"], "Technology");
    assert_eq!(groups[0]["trade_count"], 1);
    assert_eq!(money(&groups[0]["realized_pnl"]), dec!(0));
    assert_eq!(money(&open["data"]["sector"]["total_risk"]), dec!(100));
    let text = f.text("concentration", &f.account, false, &["--open-only"]);
    assert!(
        text.contains("Showing open positions only") && text.contains("Technology"),
        "{text}"
    );
    assert!(!text.contains("Healthcare"), "{text}");
    let unrelated = f.json("concentration", &f.other, &[]);
    assert!(unrelated["data"]["sector"]["groups"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn text_reports_render_actual_results_and_zen_hides_absolute_equity() {
    let f = Reports::new();
    for kind in [
        "summary",
        "performance",
        "concentration",
        "risk",
        "drawdown",
    ] {
        let plain = f.text(kind, &f.account, false, &[]);
        let zen = f.text(kind, &f.account, true, &[]);
        assert!(
            !zen.contains('$'),
            "{kind} exposed a monetary amount: {zen}"
        );
        assert!(!plain.contains("777.77") && !zen.contains("777.77"));
        if kind == "summary" {
            assert!(
                plain.replace(',', "").contains("Equity: $9904.00"),
                "{plain}"
            );
            assert!(!zen.replace(',', "").contains("$9904.00"), "{zen}");
        }
        if kind == "concentration" {
            for text in [&plain, &zen] {
                assert!(
                    text.contains("Technology") && text.contains("Healthcare"),
                    "{text}"
                );
            }
            for expected in [
                "+$10.00",
                "-$6.00",
                "Current open risk: $100.00",
                "100.0% of open risk in Technology",
            ] {
                assert!(plain.contains(expected), "missing {expected}: {plain}");
            }
        }
        if kind == "performance" {
            for text in [&plain, &zen] {
                for expected in [
                    "Total Trades: 3",
                    "Winning Trades: 1 (33.3%)",
                    "Losing Trades: 1 (33.3%)",
                ] {
                    assert!(text.contains(expected), "missing {expected}: {text}");
                }
            }
            for expected in [
                "Average Win: $10.00",
                "Average Loss: $-6.00",
                "Best Trade: $10.00",
                "Worst Trade: $-6.00",
            ] {
                assert!(plain.contains(expected), "missing {expected}: {plain}");
            }
        }
        if kind == "risk" {
            for expected in [
                "Capital at Risk: $100.00",
                "PRODUCT: $100.00",
                "Target/Stop active",
                "HEALTHY",
            ] {
                assert!(plain.contains(expected), "missing {expected}: {plain}");
            }
        }
        if kind == "drawdown" {
            assert!(
                plain.contains("Current Account Equity: $9904.00"),
                "{plain}"
            );
            assert!(
                plain.contains("does not include open position losses"),
                "{plain}"
            );
        }
    }
}

#[test]
fn empty_reports_and_cash_drawdown_recovery_remain_explicit() {
    let f = Reports::new();
    let stats = f.json("performance", &f.other, &[]);
    assert_eq!(stats["data"]["total_trades"], 0);
    assert_eq!(money(&stats["data"]["win_rate_percentage"]), dec!(0));
    assert!(f
        .text("performance", &f.other, false, &[])
        .contains("No closed trades found"));
    let risk = f.text("risk", &f.other, false, &[]);
    assert!(
        risk.contains("No open positions") && risk.contains("Capital at Risk: $0.00"),
        "{risk}"
    );
    let concentration = f.text("concentration", &f.other, false, &["--open-only"]);
    assert!(concentration.contains("No trades found"), "{concentration}");
    assert!(
        !concentration.contains("Technology")
            && !concentration.contains("Risk Concentration Alert")
    );
    assert!(f
        .text("drawdown", &f.other, false, &[])
        .contains("No drawdowns recorded"));

    // This legacy report follows ledger cash, including cash flows. Its output
    // must distinguish no recovery, partial recovery, and a recovered peak.
    let mut trust =
        TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new(&f.database.url())), vec![]);
    for (category, amount, drawdown, recovery, message) in [
        (
            TransactionCategory::Withdrawal,
            dec!(100),
            dec!(100),
            dec!(0),
            None,
        ),
        (
            TransactionCategory::Deposit,
            dec!(40),
            dec!(60),
            dec!(40),
            Some("partially recovered"),
        ),
        (
            TransactionCategory::Deposit,
            dec!(60),
            dec!(0),
            dec!(100),
            Some("fully recovered"),
        ),
    ] {
        trust
            .create_transaction(&f.other, &category, amount, &Currency::USD)
            .unwrap();
        let report = f.json("drawdown", &f.other, &[]);
        assert_eq!(money(&report["data"]["current_drawdown"]), drawdown);
        assert_eq!(money(&report["data"]["max_drawdown"]), dec!(100));
        assert_eq!(money(&report["data"]["recovery_from_max"]), recovery);
        let plain = f.text("drawdown", &f.other, false, &[]);
        if let Some(message) = message {
            assert!(plain.contains(message), "{plain}");
        } else {
            assert!(!plain.contains("recovered"), "{plain}");
        }
        let zen = f.text("drawdown", &f.other, true, &[]);
        assert!(!zen.contains('$'), "{zen}");
    }
}
