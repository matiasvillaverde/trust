#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
extern crate core as trust_core;
use chrono::{DateTime, Duration, Utc};
use model::{account_safety::*, portfolio_risk::*, MarketBar, Status, Trade, TradeCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::BTreeMap;
use trust_core::{account_safety, portfolio_risk::*};
use uuid::Uuid;
fn now() -> DateTime<Utc> {
    "2026-10-28T14:00:00Z".parse().unwrap()
}
fn controls() -> PortfolioRiskPolicy {
    PortfolioRiskPolicy {
        max_portfolio_heat_pct: dec!(2),
        max_trade_heat_pct: dec!(1),
        max_stress_loss_pct: dec!(5),
        min_margin_cushion_pct: dec!(10),
        maintenance_margin_stress_pct: dec!(50),
        max_instrument_pct: dec!(30),
        max_sector_pct: dec!(50),
        max_cluster_pct: dec!(60),
        max_quote_age_seconds: 10,
        max_history_age_seconds: 604800,
        volatility_lookback_bars: 2,
        atr_multiplier: dec!(1),
        max_spread_bps: dec!(20),
        max_entry_deviation_bps: dec!(100),
        max_volume_participation_pct: dec!(10),
        max_book_participation_pct: dec!(10),
        slippage_bps: dec!(0),
        fee_per_unit: dec!(0),
        minimum_fee: dec!(0),
        drawdown_steps: vec![
            DrawdownStep {
                drawdown_pct: dec!(0),
                risk_multiplier: dec!(1),
            },
            DrawdownStep {
                drawdown_pct: dec!(5),
                risk_multiplier: dec!(0.5),
            },
            DrawdownStep {
                drawdown_pct: dec!(10),
                risk_multiplier: dec!(0),
            },
        ],
        recovery_delay_seconds: 3600,
        recovery_step: dec!(0.1),
        instruments: ["1234", "2345", "3456"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    InstrumentRiskPolicy {
                        sector: "tech".into(),
                        correlation_group: "growth".into(),
                        gap_stress_pct: dec!(5),
                        minimum_risk_pct: dec!(0.1),
                        entry_windows: vec![RiskWindow {
                            starts_at: now() - Duration::days(1),
                            ends_at: now() + Duration::days(30),
                        }],
                        blackouts: vec![],
                        valid_until: now() + Duration::days(31),
                    },
                )
            })
            .collect(),
    }
}
fn state() -> AccountSafetyState {
    let scope = BrokerAccountScope {
        broker: "ibkr".into(),
        environment: "paper".into(),
        account_id: "DU_TEST".into(),
    };
    let mut s = AccountSafetyState {
        policy: Some(AccountSafetyPolicy {
            currency: "USD".into(),
            max_gross_notional: dec!(100000),
            max_order_notional: dec!(10000),
            max_daily_loss: dec!(1000),
            max_drawdown: dec!(2000),
            max_snapshot_age_seconds: 120,
            controls: Some(controls()),
        }),
        ..Default::default()
    };
    account_safety::observe(
        &mut s,
        BrokerAccountSnapshot {
            scope: scope.clone(),
            currency: "USD".into(),
            observed_at: now(),
            received_at: now(),
            complete: true,
            consistent: true,
            equity: dec!(10000),
            cash: dec!(10000),
            available_funds: dec!(10000),
            realized_pnl: dec!(0),
            unrealized_pnl: dec!(0),
            positions: vec![],
            working_orders: vec![],
            margin: Some(BrokerMargin {
                observed_at: now(),
                maintenance_margin: dec!(1000),
                excess_liquidity: dec!(9000),
            }),
        },
        &scope,
        now(),
    )
    .unwrap();
    observe_throttle(&mut s, now()).unwrap();
    for id in ["1234", "2345", "3456"] {
        s.portfolio.market.insert(
            id.into(),
            RiskMarketData {
                scope: scope.clone(),
                instrument_id: id.into(),
                observed_at: now(),
                received_at: now(),
                real_time: true,
                bid: dec!(99.99),
                ask: dec!(100.01),
                bid_size: dec!(10000),
                ask_size: dec!(10000),
                daily_bars: (1..=3)
                    .rev()
                    .map(|days| MarketBar {
                        time: now() - Duration::days(days),
                        open: dec!(100),
                        high: dec!(101),
                        low: dec!(99),
                        close: dec!(100),
                        volume: 100000,
                    })
                    .collect(),
            },
        );
    }
    s
}
fn trade(q: Decimal) -> Trade {
    let mut t = Trade {
        category: TradeCategory::Long,
        ..Default::default()
    };
    t.trading_vehicle.broker_asset_id = Some("1234".into());
    t.entry.quantity = q;
    t.entry.unit_price = dec!(100);
    t.safety_stop.quantity = q;
    t.safety_stop.unit_price = dec!(95);
    t.target.quantity = q;
    t.target.unit_price = dec!(110);
    t
}
fn p(s: &mut AccountSafetyState) -> &mut PortfolioRiskPolicy {
    s.policy.as_mut().unwrap().controls.as_mut().unwrap()
}
fn check(
    s: &AccountSafetyState,
    t: &Trade,
    others: &[Trade],
) -> Result<AdmissionRisk, account_safety::SafetyError> {
    admit(s, t, others, None, now())
}
fn position(q: Decimal) -> BrokerPosition {
    BrokerPosition {
        instrument_id: "1234".into(),
        symbol: "TEST".into(),
        currency: "USD".into(),
        asset_class: "STK".into(),
        quantity: q,
        market_value: q * dec!(100),
    }
}
fn order(
    t: &Trade,
    leg: &model::Order,
    qty: Decimal,
    kind: &str,
    side: &str,
) -> BrokerWorkingOrder {
    BrokerWorkingOrder {
        order_id: leg.id.to_string(),
        client_order_id: Some(leg.id.to_string()),
        instrument_id: t.trading_vehicle.broker_asset_id.clone().unwrap(),
        symbol: "TEST".into(),
        currency: "USD".into(),
        asset_class: "STK".into(),
        side: side.into(),
        order_type: kind.into(),
        stop_price: Some(leg.unit_price),
        active: true,
        remaining_quantity: qty,
        notional: Some(qty * leg.unit_price),
    }
}
fn agent(s: &mut AccountSafetyState, t: &Trade) {
    s.mandates.insert(
        "agent".into(),
        AgentMandate {
            instrument_ids: [t.trading_vehicle.id].into(),
            max_order_notional: dec!(10000),
            max_total_notional: dec!(100000),
            max_open_trades: 100,
            allow_short: true,
            expires_at: now() + Duration::days(1),
            risk_budget: Some(AgentRiskBudget {
                max_daily_loss: dec!(50),
                max_daily_turnover: dec!(4000),
                max_daily_entries: 2,
                reentry_cooldown_seconds: 0,
            }),
        },
    );
    s.trade_owners.insert(t.id, "agent".into());
    observe_agent_pnl(s, &BTreeMap::from([("agent".into(), dec!(0))]), now()).unwrap();
}
#[test]
fn shared_heat_reserves_across_profiles_and_rejects_one_decimal_over_boundary() {
    let mut s = state();
    p(&mut s).max_instrument_pct = dec!(50);
    let mut all = vec![];
    for _ in 0..2 {
        let mut t = trade(dec!(20));
        t.account_id = Uuid::new_v4();
        let r = check(&s, &t, &all).unwrap();
        reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
        t.status = Status::Funded;
        all.push(t);
    }
    let r = portfolio_report(&s, &all, now()).unwrap();
    assert_eq!(r.planned_loss, dec!(200));
    assert_eq!(r.heat_remaining, dec!(0));
    assert_eq!(
        check(&s, &trade(dec!(0.00000001)), &all).unwrap_err().code,
        "portfolio_heat_exceeded"
    );
    let restored: AccountSafetyState =
        serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(
        check(&restored, &trade(dec!(1)), &all).unwrap_err().code,
        "portfolio_heat_exceeded"
    );
}
#[test]
fn funding_and_submission_do_not_double_charge_or_release_previous_reservation() {
    let mut s = state();
    let mut t = trade(dec!(10));
    agent(&mut s, &t);
    let first = admit(&s, &t, &[], Some("agent"), now()).unwrap();
    let fund = Uuid::new_v4();
    reserve(&mut s, &t, "agent", fund, &first, now()).unwrap();
    t.status = Status::Funded;
    let second = admit(&s, &t, std::slice::from_ref(&t), Some("agent"), now()).unwrap();
    assert_eq!(second.portfolio.planned_loss, dec!(50));
    let submit = Uuid::new_v4();
    reserve(&mut s, &t, "agent", submit, &second, now()).unwrap();
    assert_eq!(s.portfolio.entry_charges.len(), 1);
    release_definitive_rejection(&mut s, submit);
    assert!(s.portfolio.reservations.contains_key(&t.id));
    release_definitive_rejection(&mut s, fund);
    assert!(s.portfolio.reservations.is_empty());
    assert!(s.portfolio.entry_charges.is_empty());
}
#[test]
fn unresolved_outcome_retains_terminal_reservations_until_verified() {
    let mut s = state();
    let mut t = trade(dec!(10));
    let r = check(&s, &t, &[]).unwrap();
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
    t.status = Status::Canceled;
    s.unresolved_operations.insert(Uuid::new_v4());
    reconcile_reservations(&mut s, &[t.clone()], now());
    assert_eq!(s.portfolio.reservations.len(), 1);
    s.unresolved_operations.clear();
    reconcile_reservations(&mut s, &[t], now());
    assert!(s.portfolio.reservations.is_empty());
    assert_eq!(s.portfolio.entry_charges.len(), 1);
}
#[test]
fn external_long_without_stop_consumes_full_notional_and_short_is_blocked() {
    let mut s = state();
    s.snapshot
        .as_mut()
        .unwrap()
        .positions
        .push(position(dec!(1)));
    assert_eq!(
        portfolio_report(&s, &[], now()).unwrap().planned_loss,
        dec!(100)
    );
    s.snapshot.as_mut().unwrap().positions[0] = position(dec!(-1));
    assert_eq!(
        portfolio_report(&s, &[], now()).unwrap_err().code,
        "unprotected_short_exposure"
    );
}
#[test]
fn external_full_stop_credit_requires_unique_active_market_stop() {
    for short in [false, true] {
        let mut s = state();
        let mut t = trade(dec!(10));
        if short {
            t.category = TradeCategory::Short;
            t.safety_stop.unit_price = dec!(105);
        }
        s.snapshot
            .as_mut()
            .unwrap()
            .positions
            .push(position(if short { dec!(-10) } else { dec!(10) }));
        let mut stop = order(
            &t,
            &t.safety_stop,
            dec!(10),
            "stop",
            if short { "buy" } else { "sell" },
        );
        stop.notional = None;
        s.snapshot
            .as_mut()
            .unwrap()
            .working_orders
            .push(stop.clone());
        assert_eq!(
            portfolio_report(&s, &[], now()).unwrap().planned_loss,
            dec!(50)
        );
        stop.order_id = "duplicate".into();
        s.snapshot.as_mut().unwrap().working_orders.push(stop);
        assert!(portfolio_report(&s, &[], now()).is_err());
    }
}
#[test]
fn partial_fill_counts_position_and_remaining_entry_once() {
    for short in [false, true] {
        let mut s = state();
        let mut t = trade(dec!(10));
        t.status = Status::PartiallyFilled;
        t.entry.filled_quantity = dec!(4);
        if short {
            t.category = TradeCategory::Short;
            t.safety_stop.unit_price = dec!(105);
        }
        s.snapshot
            .as_mut()
            .unwrap()
            .positions
            .push(position(if short { dec!(-4) } else { dec!(4) }));
        s.snapshot.as_mut().unwrap().working_orders = vec![
            order(
                &t,
                &t.entry,
                dec!(6),
                "limit",
                if short { "sell" } else { "buy" },
            ),
            order(
                &t,
                &t.safety_stop,
                dec!(4),
                "stop",
                if short { "buy" } else { "sell" },
            ),
        ];
        let r = portfolio_report(&s, &[t], now()).unwrap();
        assert_eq!(r.planned_loss, dec!(50));
        assert_eq!(r.instrument_notional["1234"], dec!(1000));
    }
}
#[test]
fn manual_working_orders_consume_heat_and_liquidity() {
    let mut s = state();
    let mut o = order(
        &trade(dec!(1)),
        &trade(dec!(1)).entry,
        dec!(1),
        "limit",
        "buy",
    );
    o.order_id = "external".into();
    o.client_order_id = None;
    s.snapshot.as_mut().unwrap().working_orders.push(o);
    assert_eq!(
        portfolio_report(&s, &[], now()).unwrap().planned_loss,
        dec!(100)
    );
    p(&mut s).max_book_participation_pct = dec!(0.01);
    assert_eq!(
        check(&s, &trade(dec!(1)), &[]).unwrap_err().code,
        "liquidity_participation_exceeded"
    );
}
#[test]
fn gap_stress_and_fees_can_bind_with_safe_planned_stop() {
    let mut s = state();
    p(&mut s)
        .instruments
        .get_mut("1234")
        .unwrap()
        .gap_stress_pct = dec!(60);
    assert_eq!(
        check(&s, &trade(dec!(10)), &[]).unwrap_err().code,
        "portfolio_stress_exceeded"
    );
    p(&mut s)
        .instruments
        .get_mut("1234")
        .unwrap()
        .gap_stress_pct = dec!(5);
    p(&mut s).slippage_bps = dec!(10);
    p(&mut s).fee_per_unit = dec!(0.05);
    p(&mut s).minimum_fee = dec!(1);
    let r = check(&s, &trade(dec!(10)), &[]).unwrap();
    assert_eq!(r.proposed_planned_loss, dec!(52));
    assert_eq!(r.proposed_stress_loss, dec!(52));
}
#[test]
fn margin_buffer_reserves_cash_and_stresses_maintenance() {
    let mut s = state();
    s.snapshot
        .as_mut()
        .unwrap()
        .margin
        .as_mut()
        .unwrap()
        .excess_liquidity = dec!(2550);
    let r = check(&s, &trade(dec!(10)), &[]).unwrap();
    assert_eq!(r.portfolio.margin_cushion_after_stress, dec!(1000));
    s.snapshot
        .as_mut()
        .unwrap()
        .margin
        .as_mut()
        .unwrap()
        .excess_liquidity = dec!(2549.99999999);
    assert_eq!(
        check(&s, &trade(dec!(10)), &[]).unwrap_err().code,
        "margin_cushion_exhausted"
    );
    s.snapshot.as_mut().unwrap().margin = None;
    assert_eq!(
        check(&s, &trade(dec!(10)), &[]).unwrap_err().code,
        "margin_evidence_missing"
    );
}
#[test]
fn concentration_groups_aggregate_without_offset_credit() {
    let mut s = state();
    p(&mut s).max_instrument_pct = dec!(5);
    p(&mut s).max_sector_pct = dec!(10);
    p(&mut s).max_cluster_pct = dec!(10);
    assert_eq!(
        check(&s, &trade(dec!(5.00000001)), &[]).unwrap_err().code,
        "instrument_concentration_exceeded"
    );
    let mut a = trade(dec!(5));
    a.status = Status::Funded;
    a.trading_vehicle.broker_asset_id = Some("2345".into());
    let mut b = a.clone();
    b.id = Uuid::new_v4();
    b.trading_vehicle.broker_asset_id = Some("3456".into());
    assert_eq!(
        check(&s, &trade(dec!(1)), &[a.clone(), b.clone()])
            .unwrap_err()
            .code,
        "sector_concentration_exceeded"
    );
    p(&mut s).instruments.get_mut("3456").unwrap().sector = "other".into();
    assert_eq!(
        check(&s, &trade(dec!(1)), &[a, b]).unwrap_err().code,
        "cluster_concentration_exceeded"
    );
}
#[test]
fn tight_stops_cannot_evade_atr_floor_and_submission_preserves_floor() {
    let mut s = state();
    let mut t = trade(dec!(20));
    t.safety_stop.unit_price = dec!(99.99);
    let r = check(&s, &t, &[]).unwrap();
    assert_eq!(r.proposed_planned_loss, dec!(40));
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
    t.status = Status::Funded;
    for b in &mut s.portfolio.market.get_mut("1234").unwrap().daily_bars {
        b.high = dec!(100.01);
        b.low = dec!(99.99);
    }
    let r = check(&s, &t, &[t.clone()]).unwrap();
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
    assert_eq!(
        portfolio_report(&s, &[t], now()).unwrap().planned_loss,
        dec!(40)
    );
}
#[test]
fn quote_evidence_and_history_corruption_fail_closed() {
    let original = state();
    for case in 0..12 {
        let mut s = original.clone();
        let d = s.portfolio.market.get_mut("1234").unwrap();
        match case {
            0 => d.real_time = false,
            1 => d.scope.account_id = "other".into(),
            2 => d.instrument_id = "wrong".into(),
            3 => d.observed_at = now() - Duration::seconds(11),
            4 => d.received_at = now() + Duration::seconds(1),
            5 => d.ask = dec!(99),
            6 => d.bid_size = dec!(0),
            7 => d.daily_bars.clear(),
            8 => d.daily_bars[2].time = d.daily_bars[1].time,
            9 => d.daily_bars[2].time = now(),
            10 => d.daily_bars[0].low = dec!(101),
            _ => d.daily_bars[0].volume = 0,
        }
        if case == 11 {
            d.daily_bars[2].volume = 0;
        }
        assert!(check(&s, &trade(dec!(1)), &[]).is_err(), "case {case}");
    }
}
#[test]
fn collar_spread_time_windows_expiry_and_blackouts_are_mandatory() {
    let mut s = state();
    let mut t = trade(dec!(1));
    t.entry.unit_price = dec!(102);
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "entry_price_collar_exceeded"
    );
    t.entry.unit_price = dec!(100);
    s.portfolio.market.get_mut("1234").unwrap().ask = dec!(101);
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "spread_limit_exceeded"
    );
    s = state();
    p(&mut s)
        .instruments
        .get_mut("1234")
        .unwrap()
        .blackouts
        .push(RiskWindow {
            starts_at: now(),
            ends_at: now() + Duration::hours(1),
        });
    assert_eq!(check(&s, &t, &[]).unwrap_err().code, "entry_window_blocked");
    s = state();
    p(&mut s).instruments.get_mut("1234").unwrap().entry_windows[0].ends_at = now();
    assert_eq!(check(&s, &t, &[]).unwrap_err().code, "entry_window_blocked");
    s = state();
    p(&mut s).instruments.get_mut("1234").unwrap().valid_until = now();
    assert!(check(&s, &t, &[]).is_err());
}
#[test]
fn throttle_drops_immediately_and_recovers_only_one_timed_step() {
    let mut s = state();
    s.snapshot.as_mut().unwrap().equity = dec!(9500);
    observe_throttle(&mut s, now()).unwrap();
    assert_eq!(s.portfolio.throttle.as_ref().unwrap().multiplier, dec!(0.5));
    assert_eq!(
        check(&s, &trade(dec!(10)), &[]).unwrap_err().code,
        "trade_heat_exceeded"
    );
    s.snapshot.as_mut().unwrap().equity = dec!(10000);
    observe_throttle(&mut s, now() + Duration::seconds(3599)).unwrap();
    assert_eq!(s.portfolio.throttle.as_ref().unwrap().multiplier, dec!(0.5));
    observe_throttle(&mut s, now() + Duration::seconds(3600)).unwrap();
    assert_eq!(s.portfolio.throttle.as_ref().unwrap().multiplier, dec!(0.6));
    observe_throttle(&mut s, now() + Duration::days(10)).unwrap();
    assert_eq!(s.portfolio.throttle.as_ref().unwrap().multiplier, dec!(0.7));
    s.snapshot.as_mut().unwrap().equity = dec!(9000);
    observe_throttle(&mut s, now() + Duration::days(10)).unwrap();
    assert_eq!(s.portfolio.throttle.as_ref().unwrap().multiplier, dec!(0));
}
#[test]
fn daily_and_drawdown_remaining_loss_reduce_prospective_heat() {
    let mut s = state();
    s.snapshot.as_mut().unwrap().equity = dec!(9050);
    s.portfolio.throttle.as_mut().unwrap().multiplier = dec!(1);
    assert_eq!(
        check(&s, &trade(dec!(10)), &[])
            .unwrap()
            .portfolio
            .heat_limit,
        dec!(50)
    );
    assert_eq!(
        check(&s, &trade(dec!(10.00000001)), &[]).unwrap_err().code,
        "portfolio_heat_exceeded"
    );
}
#[test]
fn agent_loss_entry_turnover_and_cooldown_survive_policy_replacement() {
    let mut s = state();
    let t = trade(dec!(10));
    agent(&mut s, &t);
    let r = admit(&s, &t, &[], Some("agent"), now()).unwrap();
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
    let mut another = trade(dec!(10));
    another.trading_vehicle.broker_asset_id = Some("2345".into());
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .max_daily_entries = 1;
    assert_eq!(
        admit(&s, &another, std::slice::from_ref(&t), Some("agent"), now())
            .unwrap_err()
            .code,
        "agent_entry_count_exceeded"
    );
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .max_daily_entries = 3;
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .max_daily_turnover = dec!(3999);
    assert_eq!(
        admit(&s, &another, std::slice::from_ref(&t), Some("agent"), now())
            .unwrap_err()
            .code,
        "agent_turnover_exceeded"
    );
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .reentry_cooldown_seconds = 60;
    assert_eq!(
        admit(&s, &trade(dec!(1)), &[t], Some("agent"), now())
            .unwrap_err()
            .code,
        "agent_reentry_cooldown"
    );
    observe_agent_pnl(
        &mut s,
        &BTreeMap::from([("agent".into(), dec!(-50))]),
        now(),
    )
    .unwrap();
    assert_eq!(
        admit(&s, &another, &[], Some("agent"), now())
            .unwrap_err()
            .code,
        "agent_daily_loss_exceeded"
    );
    let saved = serde_json::to_string(&s).unwrap();
    let mut restored: AccountSafetyState = serde_json::from_str(&saved).unwrap();
    observe_agent_pnl(
        &mut restored,
        &BTreeMap::from([("agent".into(), dec!(-100))]),
        now() + Duration::days(1),
    )
    .unwrap();
    assert_eq!(
        restored.portfolio.agent_losses["agent"].day_start_pnl,
        dec!(-50)
    );
    assert!(observe_agent_pnl(
        &mut restored,
        &BTreeMap::from([("agent".into(), dec!(0))]),
        now()
    )
    .is_err());
}
#[test]
fn missing_legacy_policy_agent_budget_and_arithmetic_overflow_fail_closed() {
    let mut s = state();
    let t = trade(dec!(1));
    agent(&mut s, &t);
    s.mandates.get_mut("agent").unwrap().risk_budget = None;
    assert_eq!(
        admit(&s, &t, &[], Some("agent"), now()).unwrap_err().code,
        "agent_risk_budget_missing"
    );
    p(&mut s).minimum_fee = Decimal::MAX;
    assert!(check(&s, &t, &[]).is_err());
    s.policy.as_mut().unwrap().controls = None;
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "portfolio_policy_missing"
    );
}
#[test]
fn loss_is_monotone_in_quantity_and_volatility() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(1u32..100, 1u32..100, 1u32..25),
            |(quantity, extra, range)| {
                let mut s = state();
                p(&mut s).max_trade_heat_pct = dec!(100);
                p(&mut s).max_portfolio_heat_pct = dec!(100);
                p(&mut s).max_stress_loss_pct = dec!(100);
                let mut a = trade(Decimal::from(quantity));
                a.status = Status::Funded;
                a.safety_stop.unit_price = dec!(99.99);
                let first = portfolio_report(&s, &[a.clone()], now()).unwrap();
                a.entry.quantity += Decimal::from(extra);
                let second = portfolio_report(&s, &[a.clone()], now()).unwrap();
                assert!(second.planned_loss >= first.planned_loss);
                let d = s.portfolio.market.get_mut("1234").unwrap();
                let old =
                    market_unit_floor(&controls(), s.snapshot.as_ref().unwrap(), &a, d, now())
                        .unwrap();
                for b in &mut d.daily_bars {
                    b.high += Decimal::from(range);
                    b.low -= Decimal::from(range);
                }
                let new =
                    market_unit_floor(&controls(), s.snapshot.as_ref().unwrap(), &a, d, now())
                        .unwrap();
                assert!(new >= old);
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn actual_turnover_observation_is_idempotent_counts_adverse_exits_and_rolls_over() {
    let mut s = state();
    let mut t = trade(dec!(10));
    agent(&mut s, &t);
    t.entry.filled_quantity = dec!(10);
    t.entry.average_filled_price = Some(dec!(100));
    t.target.filled_quantity = dec!(10);
    t.target.average_filled_price = Some(dec!(350));
    t.status = Status::ClosedTarget;
    observe_turnover(&mut s, &[t.clone()], now()).unwrap();
    observe_turnover(&mut s, &[t.clone()], now()).unwrap();
    assert_eq!(s.portfolio.turnover[&t.id].today, dec!(4500));
    assert_eq!(
        check_agent_capacity(&s, &[t.clone()], "agent", now())
            .unwrap_err()
            .code,
        "agent_turnover_exhausted"
    );
    observe_turnover(&mut s, &[t.clone()], now() + Duration::days(1)).unwrap();
    assert_eq!(s.portfolio.turnover[&t.id].today, dec!(0));
    t.target.average_filled_price = Some(dec!(349));
    assert_eq!(
        observe_turnover(&mut s, &[t], now() + Duration::days(1))
            .unwrap_err()
            .code,
        "agent_turnover_evidence_regressed"
    );
}
#[test]
fn cooldown_starts_again_at_actual_exit_even_when_funding_was_days_ago() {
    let mut s = state();
    let mut prior = trade(dec!(1));
    agent(&mut s, &prior);
    prior.status = Status::ClosedStopLoss;
    prior.updated_at = (now() - Duration::seconds(30)).naive_utc();
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .reentry_cooldown_seconds = 60;
    assert_eq!(
        admit(&s, &trade(dec!(1)), &[prior], Some("agent"), now())
            .unwrap_err()
            .code,
        "agent_reentry_cooldown"
    );
}
#[test]
fn existing_snapshot_and_margin_age_and_orphan_reservations_are_checked() {
    let mut s = state();
    s.snapshot
        .as_mut()
        .unwrap()
        .margin
        .as_mut()
        .unwrap()
        .observed_at = now() - Duration::seconds(121);
    assert_eq!(
        check_existing(&s, &[], now()).unwrap_err().code,
        "margin_evidence_invalid_or_stale"
    );
    s = state();
    s.snapshot.as_mut().unwrap().observed_at = now() - Duration::seconds(121);
    assert_eq!(
        check_existing(&s, &[], now()).unwrap_err().code,
        "broker_snapshot_stale"
    );
    s = state();
    let t = trade(dec!(1));
    let r = check(&s, &t, &[]).unwrap();
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &r, now()).unwrap();
    assert_eq!(
        check_existing(&s, &[], now()).unwrap_err().code,
        "risk_reservation_unreconciled"
    );
}

#[test]
fn a_protective_order_cannot_be_credited_to_two_trades_or_legs() {
    let mut s = state();
    let mut one = trade(dec!(10));
    one.status = Status::Filled;
    one.entry.filled_quantity = dec!(10);
    let mut two = trade(dec!(10));
    two.status = Status::Filled;
    two.entry.filled_quantity = dec!(10);
    two.safety_stop.broker_order_id = Some(one.safety_stop.id.to_string());
    s.snapshot
        .as_mut()
        .unwrap()
        .positions
        .push(position(dec!(20)));
    s.snapshot.as_mut().unwrap().working_orders.push(order(
        &one,
        &one.safety_stop,
        dec!(10),
        "stop",
        "sell",
    ));
    assert_eq!(
        account_safety::verify_reconciled(s.snapshot.as_ref().unwrap(), &[one.clone(), two])
            .unwrap_err()
            .code,
        "broker_order_identity_ambiguous"
    );
    one.target.broker_order_id = Some(one.safety_stop.id.to_string());
    assert_eq!(
        portfolio_report(&s, &[one], now()).unwrap_err().code,
        "broker_order_identity_ambiguous"
    );
}
#[test]
fn nonzero_risk_that_underflows_decimal_precision_cannot_authorize_an_entry() {
    let mut s = state();
    let tiny = Decimal::from_str_exact("0.0000000000000000000000000001").unwrap();
    let mut t = trade(tiny);
    t.entry.unit_price = tiny;
    t.safety_stop.unit_price = Decimal::ZERO;
    let d = s.portfolio.market.get_mut("1234").unwrap();
    d.bid = tiny;
    d.ask = tiny;
    for b in &mut d.daily_bars {
        b.open = tiny;
        b.close = tiny;
        b.high = tiny;
        b.low = tiny;
    }
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "risk_arithmetic_precision_loss"
    );
}

#[test]
fn historical_gains_do_not_grant_an_agent_extra_daily_loss_capacity() {
    let mut s = state();
    let t = trade(dec!(1));
    agent(&mut s, &t);
    s.portfolio.agent_losses.clear();
    observe_agent_pnl(
        &mut s,
        &BTreeMap::from([("agent".into(), dec!(1000))]),
        now(),
    )
    .unwrap();
    assert_eq!(s.portfolio.agent_losses["agent"].day_start_pnl, dec!(1000));
    observe_agent_pnl(
        &mut s,
        &BTreeMap::from([("agent".into(), dec!(950))]),
        now(),
    )
    .unwrap();
    assert_eq!(
        check_agent_capacity(&s, &[], "agent", now())
            .unwrap_err()
            .code,
        "agent_daily_loss_exceeded"
    );
    s.portfolio.agent_losses.clear();
    observe_agent_pnl(
        &mut s,
        &BTreeMap::from([("agent".into(), dec!(-50))]),
        now(),
    )
    .unwrap();
    assert_eq!(s.portfolio.agent_losses["agent"].day_start_pnl, dec!(0));
    assert_eq!(
        check_agent_capacity(&s, &[], "agent", now())
            .unwrap_err()
            .code,
        "agent_daily_loss_exceeded"
    );
}

#[test]
fn missing_identity_market_baseline_and_throttle_evidence_block_new_risk() {
    let mut s = state();
    let mut t = trade(dec!(1));
    t.trading_vehicle.broker_asset_id = None;
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "instrument_unresolved"
    );
    t.trading_vehicle.broker_asset_id = Some("1234".into());
    s.portfolio.market.clear();
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "market_evidence_missing"
    );

    let mut s = state();
    s.baseline = None;
    assert_eq!(
        drawdown_pct(&s).unwrap_err().code,
        "equity_baseline_missing"
    );
    assert_eq!(
        portfolio_report(&s, &[], now()).unwrap_err().code,
        "equity_baseline_missing"
    );

    let mut s = state();
    s.portfolio.throttle = None;
    assert_eq!(
        check(&s, &t, &[]).unwrap_err().code,
        "drawdown_observation_missing"
    );
}

#[test]
fn agent_readiness_and_admission_require_independent_loss_evidence() {
    let mut s = state();
    let t = trade(dec!(1));
    assert_eq!(
        check_agent_capacity(&s, &[], "agent", now())
            .unwrap_err()
            .code,
        "agent_risk_budget_missing"
    );
    assert_eq!(
        admit(&s, &t, &[], Some("agent"), now()).unwrap_err().code,
        "agent_mandate_missing"
    );
    agent(&mut s, &t);
    s.portfolio.agent_losses.clear();
    assert_eq!(
        check_agent_capacity(&s, &[], "agent", now())
            .unwrap_err()
            .code,
        "agent_loss_evidence_missing"
    );
    assert_eq!(
        admit(&s, &t, &[], Some("agent"), now()).unwrap_err().code,
        "agent_loss_evidence_missing"
    );
}

#[test]
fn incomplete_execution_evidence_cannot_create_zero_turnover_credit() {
    for price in [None, Some(dec!(0)), Some(dec!(-1))] {
        let mut s = state();
        let mut t = trade(dec!(1));
        agent(&mut s, &t);
        t.entry.filled_quantity = dec!(1);
        t.entry.average_filled_price = price;
        assert_eq!(
            observe_turnover(&mut s, &[t], now()).unwrap_err().code,
            "agent_fill_evidence_missing"
        );
        assert!(s.portfolio.turnover.is_empty());
    }
}

#[test]
fn corrupt_equity_baselines_and_unrepresentable_costs_fail_closed() {
    let mut s = state();
    s.baseline.as_mut().unwrap().high_water = dec!(0);
    let before = s.portfolio.throttle.clone();
    assert_eq!(
        observe_throttle(&mut s, now()).unwrap_err().code,
        "risk_arithmetic_overflow"
    );
    assert_eq!(
        serde_json::to_value(&s.portfolio.throttle).unwrap(),
        serde_json::to_value(before).unwrap()
    );

    s.baseline.as_mut().unwrap().high_water = Decimal::MAX;
    s.snapshot.as_mut().unwrap().equity = Decimal::MIN;
    assert_eq!(
        drawdown_pct(&s).unwrap_err().code,
        "risk_arithmetic_overflow"
    );

    let mut s = state();
    p(&mut s).fee_per_unit = Decimal::MAX;
    assert_eq!(
        check(&s, &trade(dec!(2)), &[]).unwrap_err().code,
        "risk_arithmetic_overflow"
    );

    let mut s = state();
    p(&mut s).drawdown_steps.clear();
    assert_eq!(
        observe_throttle(&mut s, now()).unwrap_err().code,
        "invalid_drawdown_steps"
    );
}

#[test]
fn pending_reservations_charge_turnover_even_without_an_entry_charge_record() {
    let mut s = state();
    let mut t = trade(dec!(10));
    agent(&mut s, &t);
    let risk = admit(&s, &t, &[], Some("agent"), now()).unwrap();
    reserve(&mut s, &t, "agent", Uuid::new_v4(), &risk, now()).unwrap();
    t.status = Status::Funded;
    s.portfolio.entry_charges.clear();
    s.mandates
        .get_mut("agent")
        .unwrap()
        .risk_budget
        .as_mut()
        .unwrap()
        .max_daily_turnover = dec!(2000);
    assert_eq!(
        check_agent_capacity(&s, &[t], "agent", now())
            .unwrap_err()
            .code,
        "agent_turnover_exhausted"
    );
}
