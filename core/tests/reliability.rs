//! Money-safety regressions with real SQLite and deterministic broker doubles.
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use diesel::{Connection, RunQueryDsl};
use model::{
    Account, Broker, BrokerKind, BrokerLog, Currency, DatabaseFactory, DraftTrade, Environment,
    Order, OrderIds, RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

struct BrokerStub;
impl Broker for BrokerStub {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: "entry".into(),
                stop: "stop".into(),
                target: "target".into(),
            },
        ))
    }
    fn sync_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        Err("no snapshot configured".into())
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn close_trade(&self, t: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        Ok((t.target.clone(), BrokerLog::default()))
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        Ok("stop".into())
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        Ok("target".into())
    }
}
struct Fixture {
    facade: TrustFacade,
    db: SqliteDatabase,
    connection: diesel::SqliteConnection,
    directory: tempfile::TempDir,
    account: Account,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("financial.db");
        let url = path.to_str().unwrap();
        let mut facade = TrustFacade::new(Box::new(SqliteDatabase::new(url)), Box::new(BrokerStub));
        let account = facade
            .create_account(
                "reliability",
                "isolated fixture",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .unwrap();
        facade
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(10000),
                &Currency::USD,
            )
            .unwrap();
        let db = SqliteDatabase::new(url);
        let connection = diesel::SqliteConnection::establish(url).unwrap();
        Self {
            facade,
            db,
            connection,
            directory,
            account,
        }
    }
    fn rule(&mut self, rule: RuleName) {
        self.facade
            .create_rule(&self.account, &rule, "reliability cap", &RuleLevel::Error)
            .unwrap();
    }
    fn trade(&mut self, side: TradeCategory, quantity: Decimal) -> Trade {
        let vehicle = self
            .facade
            .create_trading_vehicle(
                &uuid::Uuid::new_v4().simple().to_string(),
                None,
                &TradingVehicleCategory::Stock,
                "ibkr",
            )
            .unwrap();
        self.facade
            .create_trade(
                DraftTrade {
                    account: self.account.clone(),
                    trading_vehicle: vehicle,
                    quantity,
                    currency: Currency::USD,
                    category: side,
                    thesis: None,
                    sector: None,
                    asset_class: None,
                    context: None,
                },
                if side == TradeCategory::Long {
                    dec!(90)
                } else {
                    dec!(110)
                },
                dec!(100),
                if side == TradeCategory::Long {
                    dec!(120)
                } else {
                    dec!(80)
                },
            )
            .unwrap()
    }
    fn tables(&self) -> serde_json::Value {
        let path = self.directory.path().join("snapshot.json");
        self.db.export_backup_to_path(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        value["tables"].clone()
    }
    fn fail(&mut self, event: &str, table: &str, predicate: &str) {
        // Only compile-time test constants enter this trigger; no user SQL input.
        diesel::sql_query(format!("CREATE TRIGGER injected_failure BEFORE {event} ON {table} {predicate} BEGIN SELECT RAISE(ABORT, 'injected write failure'); END")).execute(&mut self.connection).unwrap();
    }
}

#[test]
fn fractional_risk_rules_round_trip_and_preview_is_fundable() {
    for risk in [0.1_f32, 0.3, 0.7, 1.1, 1.9, 2.01] {
        for side in [TradeCategory::Long, TradeCategory::Short] {
            let mut f = Fixture::new();
            f.rule(RuleName::RiskPerTrade(risk));
            assert_eq!(
                f.facade.search_rules(f.account.id).unwrap()[0].name.risk(),
                risk,
                "risk {risk} must survive SQLite"
            );
            let stop = if side == TradeCategory::Long {
                dec!(90)
            } else {
                dec!(110)
            };
            let quantity = f
                .facade
                .calculate_maximum_quantity(f.account.id, dec!(100), stop, &Currency::USD)
                .unwrap();
            assert!(
                quantity > Decimal::ZERO,
                "risk {risk} must allow a positive size"
            );
            let cap: Decimal = risk.to_string().parse().unwrap();
            assert!(quantity * dec!(10) <= dec!(10000) * cap / dec!(100));
            let trade = f.trade(side, quantity);
            f.facade
                .fund_trade(&trade)
                .unwrap_or_else(|e| panic!("risk {risk}, {side}, preview {quantity}: {e}"));
        }
    }
}

#[test]
fn invalid_rule_percentages_are_rejected_without_writes() {
    for risk in [-1.0, 100.01, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        for rule in [RuleName::RiskPerTrade(risk), RuleName::RiskPerMonth(risk)] {
            let mut f = Fixture::new();
            let before = f.tables();
            assert!(
                f.facade
                    .create_rule(&f.account, &rule, "invalid cap", &RuleLevel::Error)
                    .is_err(),
                "accepted {risk}"
            );
            assert_eq!(f.tables(), before);
        }
    }
}

#[test]
fn monthly_rule_alone_limits_risk_and_short_collateral_preview() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        let mut f = Fixture::new();
        f.rule(RuleName::RiskPerMonth(1.0));
        let too_large = f.trade(side, dec!(11));
        let before = f.tables();
        assert!(
            f.facade.fund_trade(&too_large).is_err(),
            "monthly rule alone must prevent 110 risk from 100 budget"
        );
        assert_eq!(f.tables(), before);
        let quantity = f
            .facade
            .calculate_maximum_quantity(
                f.account.id,
                dec!(100),
                if side == TradeCategory::Long {
                    dec!(90)
                } else {
                    dec!(110)
                },
                &Currency::USD,
            )
            .unwrap();
        assert_eq!(quantity, dec!(10));
        let allowed = f.trade(side, quantity);
        f.facade.fund_trade(&allowed).unwrap();
    }
    let mut f = Fixture::new();
    let quantity = f
        .facade
        .calculate_maximum_quantity(f.account.id, dec!(100), dec!(110), &Currency::USD)
        .unwrap();
    assert!(
        quantity * dec!(110) <= dec!(10000),
        "short preview must reserve stop collateral"
    );
    let trade = f.trade(TradeCategory::Short, quantity);
    f.facade.fund_trade(&trade).unwrap();
}

#[test]
fn monthly_risk_counts_long_and_short_through_active_states() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for status in [Status::Funded, Status::Submitted, Status::Filled] {
            let mut f = Fixture::new();
            // Set an unambiguous opening-capital baseline before this month.
            diesel::sql_query("UPDATE transactions SET created_at = '2000-01-01 00:00:00'")
                .execute(&mut f.connection)
                .unwrap();
            f.rule(RuleName::RiskPerMonth(2.0));
            f.rule(RuleName::RiskPerTrade(1.0));
            let t = f.trade(side, dec!(10));
            let (mut t, _, _, _) = f.facade.fund_trade(&t).unwrap();
            if status == Status::Submitted {
                t = f.facade.submit_trade(&t).unwrap().0;
            }
            if status == Status::Filled {
                t.entry.average_filled_price = Some(dec!(100));
                t = f.facade.fill_trade(&t, dec!(0)).unwrap().0;
            }
            assert_eq!(t.status, status);
            let available = f
                .facade
                .search_balance(f.account.id, &Currency::USD)
                .unwrap()
                .total_available;
            let quantity = f
                .facade
                .calculate_maximum_quantity(
                    f.account.id,
                    dec!(100),
                    if side == TradeCategory::Long {
                        dec!(90)
                    } else {
                        dec!(110)
                    },
                    &Currency::USD,
                )
                .unwrap();
            assert_eq!(
                quantity,
                available / dec!(1000),
                "{side} {status}: 100 monthly risk remains, enough for the next 1% position"
            );
            let rules = f.facade.search_rules(f.account.id).unwrap();
            let rule = rules
                .iter()
                .find(|r| matches!(r.name, RuleName::RiskPerTrade(_)))
                .unwrap();
            f.facade.deactivate_rule(rule).unwrap();
            f.rule(RuleName::RiskPerTrade(2.0));
            let quantity = f
                .facade
                .calculate_maximum_quantity(
                    f.account.id,
                    dec!(100),
                    if side == TradeCategory::Long {
                        dec!(90)
                    } else {
                        dec!(110)
                    },
                    &Currency::USD,
                )
                .unwrap();
            assert_eq!(
                quantity,
                Decimal::ZERO,
                "{side} {status}: 100 monthly risk remains, insufficient for a fresh 2% position"
            );
        }
    }
}

#[test]
fn funding_geometry_is_enforced_even_without_risk_rules() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for stop in [
            dec!(0),
            dec!(-1),
            dec!(100),
            if side == TradeCategory::Long {
                dec!(110)
            } else {
                dec!(90)
            },
        ] {
            let mut f = Fixture::new();
            let t = f.trade(side, dec!(1));
            // Simulate a legacy persisted draft with invalid geometry.
            // Bypass adapters intentionally: normal writes now reject negative prices.
            diesel::sql_query("UPDATE orders SET unit_price = ? WHERE id = ?")
                .bind::<diesel::sql_types::Text, _>(stop.to_string())
                .bind::<diesel::sql_types::Text, _>(t.safety_stop.id.to_string())
                .execute(&mut f.connection)
                .unwrap();
            let before = f.tables();
            assert!(
                f.facade.fund_trade(&t).is_err(),
                "accepted {side} stop {stop}"
            );
            assert_eq!(f.tables(), before);
        }
    }
}

#[test]
fn funding_and_cash_writes_roll_back_at_each_persistence_boundary() {
    for (event, table) in [
        ("UPDATE", "trades"),
        ("INSERT", "transactions"),
        ("UPDATE", "accounts_balances"),
        ("UPDATE", "trades_balances"),
    ] {
        for side in [TradeCategory::Long, TradeCategory::Short] {
            let mut f = Fixture::new();
            let t = f.trade(side, dec!(10));
            let before = f.tables();
            f.fail(event, table, "");
            assert!(
                f.facade.fund_trade(&t).is_err(),
                "trigger not reached: {event} {table}"
            );
            assert_eq!(
                f.tables(),
                before,
                "partial funding at {event} {table}, {side}"
            );
        }
    }
    for category in [
        TransactionCategory::Deposit,
        TransactionCategory::Withdrawal,
    ] {
        for (event, table) in [("INSERT", "transactions"), ("UPDATE", "accounts_balances")] {
            let mut f = Fixture::new();
            let before = f.tables();
            f.fail(event, table, "");
            assert!(f
                .facade
                .create_transaction(&f.account, &category, dec!(1.23), &Currency::USD)
                .is_err());
            assert_eq!(f.tables(), before, "partial {category} at {event} {table}");
        }
    }
}

#[test]
fn manual_execution_and_cancel_roll_back_at_each_persistence_boundary() {
    for action in ["fill", "target", "stop", "cancel"] {
        for (event, table) in [
            ("INSERT", "transactions"),
            ("UPDATE", "accounts_balances"),
            ("UPDATE", "trades_balances"),
            ("UPDATE", "trades"),
            ("UPDATE", "orders"),
        ] {
            if action == "cancel" && table == "orders" {
                continue;
            }
            for side in [TradeCategory::Long, TradeCategory::Short] {
                let mut f = Fixture::new();
                let t = f.trade(side, dec!(10));
                let (mut t, _, _, _) = f.facade.fund_trade(&t).unwrap();
                t.entry.average_filled_price = Some(dec!(100));
                if action == "target" || action == "stop" {
                    t = f.facade.fill_trade(&t, dec!(0)).unwrap().0;
                }
                t.target.average_filled_price = Some(t.target.unit_price);
                t.safety_stop.average_filled_price = Some(t.safety_stop.unit_price);
                let before = f.tables();
                f.fail(event, table, "");
                let result = match action {
                    "fill" => f.facade.fill_trade(&t, dec!(1.25)).map(|_| ()),
                    "target" => f.facade.target_acquired(&t, dec!(1.25)).map(|_| ()),
                    "stop" => f.facade.stop_trade(&t, dec!(1.25)).map(|_| ()),
                    "cancel" => f.facade.cancel_funded_trade(&t).map(|_| ()),
                    _ => unreachable!(),
                };
                assert!(
                    result.is_err(),
                    "trigger not reached: {action} {event} {table}, {side}"
                );
                assert_eq!(
                    f.tables(),
                    before,
                    "partial {action} at {event} {table}, {side}"
                );
            }
        }
    }
}

#[test]
fn generated_lifecycles_conserve_cash_with_fees_and_adverse_exits() {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed, TestRunner};
    let config = Config {
        cases: 128,
        rng_seed: RngSeed::Fixed(0x5452555354),
        ..Config::default()
    };
    let mut runner = TestRunner::new(config);
    runner
        .run(
            &(
                any::<bool>(),
                1_i64..=40,
                100_i64..=20000,
                0_i64..=500,
                0_i64..=500,
            ),
            |(short, quantity, exit_cents, opening_fee_cents, closing_fee_cents)| {
                let mut f = Fixture::new();
                let side = if short {
                    TradeCategory::Short
                } else {
                    TradeCategory::Long
                };
                let quantity = Decimal::from(quantity);
                let price = Decimal::new(exit_cents, 2);
                let open_fee = Decimal::new(opening_fee_cents, 2);
                let close_fee = Decimal::new(closing_fee_cents, 2);
                let t = f.trade(side, quantity);
                let (mut t, _, _, _) = f.facade.fund_trade(&t).unwrap();
                t.entry.average_filled_price = Some(dec!(100));
                let (mut t, _) = f.facade.fill_trade(&t, open_fee).unwrap();
                let profit = if short {
                    dec!(100) - price
                } else {
                    price - dec!(100)
                } * quantity;
                if profit >= Decimal::ZERO {
                    t.target.average_filled_price = Some(price);
                    f.facade.target_acquired(&t, close_fee).unwrap();
                } else {
                    t.safety_stop.average_filled_price = Some(price);
                    f.facade.stop_trade(&t, close_fee).unwrap();
                }
                let closed = f.facade.read_trade(t.id).unwrap();
                let account = f
                    .facade
                    .search_balance(f.account.id, &Currency::USD)
                    .unwrap();
                assert_eq!(
                    account.total_available,
                    dec!(10000) + profit - open_fee - close_fee
                );
                assert_eq!(closed.balance.capital_out_market, Decimal::ZERO);
                assert_eq!(closed.balance.capital_in_market, Decimal::ZERO);
                assert_eq!(account.total_in_trade, Decimal::ZERO);
                assert_eq!(closed.entry.filled_quantity, quantity);
                let before = f.tables();
                assert!(f.facade.fill_trade(&t, open_fee).is_err());
                assert_eq!(f.tables(), before);
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn open_risk_query_includes_only_active_states_and_correct_account_currency() {
    for state in Status::all() {
        let mut f = Fixture::new();
        let t = f.trade(TradeCategory::Long, dec!(1));
        f.db.trade_write().update_trade_status(state, &t).unwrap();
        let found =
            f.db.trade_read()
                .all_open_trades_for_currency(f.account.id, &Currency::USD)
                .unwrap();
        assert_eq!(
            found.len(),
            usize::from(matches!(
                state,
                Status::Funded | Status::Submitted | Status::PartiallyFilled | Status::Filled
            )),
            "{state}"
        );
        assert!(f
            .db
            .trade_read()
            .all_open_trades_for_currency(f.account.id, &Currency::EUR)
            .unwrap()
            .is_empty());
        assert!(f
            .db
            .trade_read()
            .all_open_trades_for_currency(uuid::Uuid::new_v4(), &Currency::USD)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn preview_never_exceeds_fundable_base_at_any_level() {
    for level in 0..=4 {
        let mut f = Fixture::new();
        f.rule(RuleName::RiskPerTrade(1.0));
        let mut persisted = f.db.level_read().level_for_account(f.account.id).unwrap();
        persisted.current_level = level;
        persisted.risk_multiplier = model::Level::multiplier_for_level(level).unwrap();
        f.db.level_write().update_level(&persisted).unwrap();
        let preview = f
            .facade
            .calculate_level_adjusted_quantity(f.account.id, dec!(100), dec!(90), &Currency::USD)
            .unwrap();
        assert!(preview.final_quantity <= preview.base_quantity);
        let trade = f.trade(TradeCategory::Long, preview.final_quantity);
        f.facade.fund_trade(&trade).unwrap();
    }
}

#[test]
fn fractional_risk_backup_round_trip_and_corruption_rejection() {
    use db_sqlite::ImportOptions;
    let mut f = Fixture::new();
    f.rule(RuleName::RiskPerTrade(0.3));
    let path = f.directory.path().join("risk-backup.json");
    f.db.export_backup_to_path(&path).unwrap();
    let mut restored = SqliteDatabase::new_in_memory();
    restored
        .import_backup_from_path(&path, ImportOptions::default())
        .unwrap();
    let rules = restored.rule_read().read_all_rules(f.account.id).unwrap();
    assert_eq!(rules[0].name, RuleName::RiskPerTrade(0.3));
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let bad_path = f.directory.path().join("invalid-backup.json");
    for invalid in [
        "-1",
        "100.01",
        "NaN",
        "1.123456789123456789",
        "0.00000000000000000000000000001",
    ] {
        let mut data = original.clone();
        data["tables"]["rules"][0]["risk_decimal"] = serde_json::json!(invalid);
        std::fs::write(&bad_path, data.to_string()).unwrap();
        let mut target = SqliteDatabase::new_in_memory();
        assert!(
            target
                .import_backup_from_path(&bad_path, ImportOptions::default())
                .is_err(),
            "accepted restored risk {invalid}"
        );
        assert!(target.account_read().id(f.account.id).is_err());
    }
    let mut data = original;
    data["tables"]["transactions"][0]["amount"] =
        serde_json::json!("1.12345678901234567890123456789");
    std::fs::write(&bad_path, data.to_string()).unwrap();
    let mut target = SqliteDatabase::new_in_memory();
    assert!(
        target
            .import_backup_from_path(&bad_path, ImportOptions::default())
            .is_err(),
        "restore must not round money"
    );
    assert!(target.account_read().id(f.account.id).is_err());
}

#[test]
fn read_failures_never_look_like_an_empty_portfolio() {
    let mut f = Fixture::new();
    diesel::sql_query("DROP TABLE trades")
        .execute(&mut f.connection)
        .unwrap();
    assert!(f
        .facade
        .calculate_open_positions(Some(f.account.id))
        .is_err());
    assert!(f.facade.calculate_open_positions(None).is_err());
    let mut f = Fixture::new();
    diesel::sql_query("DROP TABLE transactions")
        .execute(&mut f.connection)
        .unwrap();
    assert!(f.facade.get_all_transactions().is_err());
}

#[test]
fn mixed_currency_ledgers_remain_visible_and_cannot_be_aggregated() {
    let mut f = Fixture::new();
    f.facade
        .create_transaction(
            &f.account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::EUR,
        )
        .unwrap();
    let transactions = f.facade.get_account_transactions(f.account.id).unwrap();
    assert_eq!(transactions.len(), 2);
    assert!(transactions.iter().any(|t| t.currency == Currency::EUR));
    assert!(f
        .facade
        .service_report(f.account.id, "metrics", 30, None, None, "symbol", None)
        .is_err());
}

#[test]
fn partial_positions_remain_visible_after_an_exit_transaction() {
    let mut f = Fixture::new();
    let t = f.trade(TradeCategory::Long, dec!(10));
    f.facade.fund_trade(&t).unwrap();
    f.db.trade_write()
        .update_trade_status(Status::PartiallyFilled, &t)
        .unwrap();
    f.db.transaction_write()
        .create_transaction_by_account_id(
            f.account.id,
            dec!(120),
            &Currency::USD,
            TransactionCategory::CloseTarget(t.id),
        )
        .unwrap();
    let positions = f
        .facade
        .calculate_open_positions(Some(f.account.id))
        .unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].trade_id, t.id);
}

#[test]
fn monthly_budget_does_not_reset_after_current_month_deposit_or_carried_reservation() {
    for old_deposit in [false, true] {
        for old_reservation in [false, true] {
            for side in [TradeCategory::Long, TradeCategory::Short] {
                let mut f = Fixture::new();
                if old_deposit {
                    diesel::sql_query("UPDATE transactions SET created_at='2000-01-01 00:00:00'")
                        .execute(&mut f.connection)
                        .unwrap();
                }
                f.rule(RuleName::RiskPerMonth(1.0));
                let t = f.trade(side, dec!(10));
                let (mut t, _, _, _) = f.facade.fund_trade(&t).unwrap();
                if old_reservation {
                    diesel::sql_query("UPDATE transactions SET created_at='2000-01-01 00:00:00' WHERE category='fund_trade'").execute(&mut f.connection).unwrap();
                }
                let preview = f.facade.calculate_maximum_quantity(
                    f.account.id,
                    dec!(100),
                    if side == TradeCategory::Long {
                        dec!(90)
                    } else {
                        dec!(110)
                    },
                    &Currency::USD,
                );
                if old_reservation && !old_deposit {
                    assert!(
                        preview.is_err(),
                        "impossible negative opening cash must fail closed"
                    );
                    continue;
                }
                let preview = preview.unwrap();
                assert_eq!(preview,Decimal::ZERO,"all 100 monthly risk is reserved: old deposit {old_deposit}, old reservation {old_reservation}, {side}");
                t.entry.average_filled_price = Some(dec!(100));
                let (mut t, _) = f.facade.fill_trade(&t, dec!(0)).unwrap();
                t.safety_stop.average_filled_price = Some(t.safety_stop.unit_price);
                f.facade.stop_trade(&t, dec!(0)).unwrap();
                let preview = f
                    .facade
                    .calculate_maximum_quantity(
                        f.account.id,
                        dec!(100),
                        if side == TradeCategory::Long {
                            dec!(90)
                        } else {
                            dec!(110)
                        },
                        &Currency::USD,
                    )
                    .unwrap();
                assert_eq!(preview,Decimal::ZERO,"realized loss consumed monthly risk: old deposit {old_deposit}, old reservation {old_reservation}, {side}");
            }
        }
    }
}
