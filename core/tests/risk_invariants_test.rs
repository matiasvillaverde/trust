use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerKind, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds,
    RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use proptest::prelude::Strategy;
use proptest::sample::select;
use proptest::test_runner::{Config as ProptestConfig, TestCaseError, TestCaseResult, TestRunner};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

struct NoopBroker;

impl Broker for NoopBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                stop: "stop".to_string(),
                entry: "entry".to_string(),
                target: "target".to_string(),
            },
        ))
    }

    fn sync_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        Ok((Status::Submitted, Vec::new(), BrokerLog::default()))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        Ok((trade.target.clone(), BrokerLog::default()))
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok("stop".to_string())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok("target".to_string())
    }
}

#[derive(Debug, Clone, Copy)]
struct RiskPct {
    decimal: Decimal,
    f32_value: f32,
}

#[derive(Debug, Clone)]
struct GeneratedRiskCase {
    category: TradingVehicleCategory,
    side: TradeCategory,
    capital: Decimal,
    risk: RiskPct,
    entry: Decimal,
    stop: Decimal,
}

fn trust() -> TrustFacade {
    TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(NoopBroker),
    )
}

fn account_with_rules(trust: &mut TrustFacade, capital: Decimal, risk: RiskPct) -> Account {
    let account = trust
        .create_account(
            "risk-proof",
            "risk invariant proof",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account creation");
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            capital,
            &Currency::USD,
        )
        .expect("deposit");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(100.0),
            "monthly risk cap",
            &RuleLevel::Error,
        )
        .expect("monthly risk rule");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(risk.f32_value),
            "trade risk cap",
            &RuleLevel::Error,
        )
        .expect("trade risk rule");
    account
}

fn trade(
    trust: &mut TrustFacade,
    account: &Account,
    category: TradingVehicleCategory,
    side: TradeCategory,
    quantity: i64,
    entry: Decimal,
    stop: Decimal,
) -> Trade {
    let symbol = format!("{}{}", category, side_label(side));
    let vehicle = trust
        .create_trading_vehicle(&symbol, None, &category, "ibkr")
        .expect("trading vehicle");
    let target = match side {
        TradeCategory::Long => entry.checked_add(dec!(20)).expect("target add"),
        TradeCategory::Short => entry.checked_sub(dec!(20)).expect("target subtract"),
    };
    trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity,
                currency: Currency::USD,
                category: side,
                thesis: None,
                sector: None,
                asset_class: Some(category.to_string()),
                context: None,
            },
            stop,
            entry,
            target,
        )
        .expect("trade creation")
}

fn side_label(side: TradeCategory) -> &'static str {
    match side {
        TradeCategory::Long => "long",
        TradeCategory::Short => "short",
    }
}

fn risk_per_unit(side: TradeCategory, entry: Decimal, stop: Decimal) -> Decimal {
    match side {
        TradeCategory::Long => entry.checked_sub(stop).expect("long risk per unit"),
        TradeCategory::Short => stop.checked_sub(entry).expect("short risk per unit"),
    }
}

fn max_risk_quantity(capital: Decimal, risk: RiskPct, risk_unit: Decimal) -> i64 {
    let max_risk = capital
        .checked_mul(risk.decimal)
        .and_then(|value| value.checked_div(dec!(100)))
        .expect("max risk");
    max_risk
        .checked_div(risk_unit)
        .and_then(|value| value.to_i64())
        .expect("risk quantity")
}

fn category_strategy() -> impl Strategy<Value = TradingVehicleCategory> {
    select(vec![
        TradingVehicleCategory::Stock,
        TradingVehicleCategory::Etf,
        TradingVehicleCategory::Bond,
    ])
}

fn side_strategy() -> impl Strategy<Value = TradeCategory> {
    select(vec![TradeCategory::Long, TradeCategory::Short])
}

fn risk_pct_strategy() -> impl Strategy<Value = RiskPct> {
    select(vec![
        RiskPct {
            decimal: dec!(1),
            f32_value: 1.0,
        },
        RiskPct {
            decimal: dec!(2),
            f32_value: 2.0,
        },
    ])
}

fn valid_risk_case_strategy() -> impl Strategy<Value = GeneratedRiskCase> {
    (
        category_strategy(),
        side_strategy(),
        50_000_i64..=250_000,
        risk_pct_strategy(),
        50_i64..=500,
        500_i64..=3_000,
    )
        .prop_map(
            |(category, side, capital_dollars, risk, entry_dollars, risk_bps)| {
                let entry = Decimal::from(entry_dollars);
                let risk_unit = entry
                    .checked_mul(Decimal::from(risk_bps))
                    .and_then(|value| value.checked_div(dec!(10000)))
                    .expect("risk unit");
                let stop = match side {
                    TradeCategory::Long => entry.checked_sub(risk_unit).expect("long stop"),
                    TradeCategory::Short => entry.checked_add(risk_unit).expect("short stop"),
                };
                GeneratedRiskCase {
                    category,
                    side,
                    capital: Decimal::from(capital_dollars),
                    risk,
                    entry,
                    stop,
                }
            },
        )
}

fn invalid_risk_case_strategy() -> impl Strategy<Value = GeneratedRiskCase> {
    (
        category_strategy(),
        side_strategy(),
        10_000_i64..=100_000,
        risk_pct_strategy(),
        50_i64..=500,
        0_i64..=49,
    )
        .prop_map(
            |(category, side, capital_dollars, risk, entry_dollars, invalid_offset)| {
                let entry = Decimal::from(entry_dollars);
                let offset = Decimal::from(invalid_offset);
                let stop = match side {
                    TradeCategory::Long => entry.checked_add(offset).expect("invalid long stop"),
                    TradeCategory::Short => entry.checked_sub(offset).expect("invalid short stop"),
                };
                GeneratedRiskCase {
                    category,
                    side,
                    capital: Decimal::from(capital_dollars),
                    risk,
                    entry,
                    stop,
                }
            },
        )
}

#[test]
fn funding_gate_respects_risk_formula_across_assets_sides_and_prices() {
    let categories = [
        TradingVehicleCategory::Stock,
        TradingVehicleCategory::Etf,
        TradingVehicleCategory::Bond,
    ];
    let sides = [TradeCategory::Long, TradeCategory::Short];
    let capitals = [dec!(10000), dec!(25000), dec!(50000)];
    let risk_pcts = [
        RiskPct {
            decimal: dec!(1),
            f32_value: 1.0,
        },
        RiskPct {
            decimal: dec!(2),
            f32_value: 2.0,
        },
    ];
    let price_pairs = [
        (dec!(100), dec!(95)),
        (dec!(100), dec!(90)),
        (dec!(250), dec!(240)),
    ];

    for category in categories {
        for side in sides {
            for capital in capitals {
                for risk in risk_pcts {
                    for (entry, long_stop) in price_pairs {
                        let stop = match side {
                            TradeCategory::Long => long_stop,
                            TradeCategory::Short => entry
                                .checked_add(risk_per_unit(TradeCategory::Long, entry, long_stop))
                                .expect("short stop"),
                        };
                        let risk_unit = risk_per_unit(side, entry, stop);
                        let boundary_qty = max_risk_quantity(capital, risk, risk_unit);
                        assert!(
                            boundary_qty > 0,
                            "test scenario must allow positive quantity"
                        );

                        let mut ok_trust = trust();
                        let account = account_with_rules(&mut ok_trust, capital, risk);
                        let ok_trade = trade(
                            &mut ok_trust,
                            &account,
                            category,
                            side,
                            boundary_qty,
                            entry,
                            stop,
                        );
                        ok_trust
                            .fund_trade(&ok_trade)
                            .expect("risk boundary quantity must fund");

                        let mut reject_trust = trust();
                        let account = account_with_rules(&mut reject_trust, capital, risk);
                        let rejecting_trade = trade(
                            &mut reject_trust,
                            &account,
                            category,
                            side,
                            boundary_qty
                                .checked_add(1)
                                .expect("rejecting quantity increment"),
                            entry,
                            stop,
                        );
                        let error = reject_trust
                            .fund_trade(&rejecting_trade)
                            .expect_err("one unit above risk boundary must be rejected");
                        assert!(
                            error.to_string().contains("Risk per trade exceeded"),
                            "expected risk rejection for {category} {side:?} capital={capital} entry={entry} stop={stop}, got {error}"
                        );

                        let sizing = reject_trust
                            .calculate_maximum_quantity(account.id, entry, stop, &Currency::USD)
                            .expect("maximum quantity");
                        assert_eq!(
                            sizing, boundary_qty,
                            "quantity calculator and funding gate must agree for {category} {side:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_stop_entry_geometry_is_rejected_across_assets_and_sides() {
    let categories = [
        TradingVehicleCategory::Stock,
        TradingVehicleCategory::Etf,
        TradingVehicleCategory::Bond,
    ];
    let invalid_setups = [
        (TradeCategory::Long, dec!(100), dec!(100)),
        (TradeCategory::Long, dec!(100), dec!(105)),
        (TradeCategory::Short, dec!(100), dec!(100)),
        (TradeCategory::Short, dec!(100), dec!(95)),
    ];
    let risk = RiskPct {
        decimal: dec!(2),
        f32_value: 2.0,
    };

    for category in categories {
        for (side, entry, stop) in invalid_setups {
            let mut trust = trust();
            let account = account_with_rules(&mut trust, dec!(10000), risk);
            let trade = trade(&mut trust, &account, category, side, 1, entry, stop);
            let error = trust
                .fund_trade(&trade)
                .expect_err("invalid stop/entry geometry must be rejected");
            assert!(
                error.to_string().contains("Invalid risk setup"),
                "expected invalid risk setup for {category} {side:?}, got {error}"
            );
        }
    }
}

#[test]
fn generated_funding_gate_matches_risk_formula() {
    let mut runner = TestRunner::new(ProptestConfig::with_cases(64));
    let strategy = valid_risk_case_strategy();

    runner
        .run(&strategy, |case| {
            let risk_unit = risk_per_unit(case.side, case.entry, case.stop);
            let boundary_qty = max_risk_quantity(case.capital, case.risk, risk_unit);
            if boundary_qty <= 0 {
                return Err(TestCaseError::reject("generated zero boundary quantity"));
            }

            let mut ok_trust = trust();
            let account = account_with_rules(&mut ok_trust, case.capital, case.risk);
            let ok_trade = trade(
                &mut ok_trust,
                &account,
                case.category,
                case.side,
                boundary_qty,
                case.entry,
                case.stop,
            );
            if let Err(error) = ok_trust.fund_trade(&ok_trade) {
                return Err(TestCaseError::fail(format!(
                    "risk boundary quantity must fund for {case:?}, got {error}"
                )));
            }

            let mut reject_trust = trust();
            let account = account_with_rules(&mut reject_trust, case.capital, case.risk);
            let sizing = reject_trust
                .calculate_maximum_quantity(account.id, case.entry, case.stop, &Currency::USD)
                .expect("maximum quantity");
            if sizing != boundary_qty {
                return Err(TestCaseError::fail(format!(
                    "quantity calculator and funding gate must agree for {case:?}: sizing={sizing}, boundary={boundary_qty}"
                )));
            }
            let rejecting_trade = trade(
                &mut reject_trust,
                &account,
                case.category,
                case.side,
                boundary_qty
                    .checked_add(1)
                    .expect("rejecting quantity increment"),
                case.entry,
                case.stop,
            );
            let error = reject_trust
                .fund_trade(&rejecting_trade)
                .expect_err("one unit above risk boundary must be rejected");
            if !error.to_string().contains("Risk per trade exceeded") {
                return Err(TestCaseError::fail(format!(
                    "expected risk rejection for {case:?}, got {error}"
                )));
            }
            Ok(())
        })
        .expect("generated risk boundary cases");
}

#[test]
fn generated_invalid_stop_entry_geometry_is_rejected() {
    let mut runner = TestRunner::new(ProptestConfig::with_cases(64));
    let strategy = invalid_risk_case_strategy();

    runner
        .run(&strategy, invalid_stop_entry_case)
        .expect("generated invalid stop/entry cases");
}

fn invalid_stop_entry_case(case: GeneratedRiskCase) -> TestCaseResult {
    let mut trust = trust();
    let account = account_with_rules(&mut trust, case.capital, case.risk);
    let trade = trade(
        &mut trust,
        &account,
        case.category,
        case.side,
        1,
        case.entry,
        case.stop,
    );
    let error = trust
        .fund_trade(&trade)
        .expect_err("invalid stop/entry geometry must be rejected");
    if !error.to_string().contains("Invalid risk setup") {
        return Err(TestCaseError::fail(format!(
            "expected invalid risk setup rejection for {case:?}, got {error}"
        )));
    }
    Ok(())
}
