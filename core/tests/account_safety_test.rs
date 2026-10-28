#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use chrono::{Duration, Utc};
extern crate core as trust_core;
use model::{account_safety::*, Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::BTreeSet;
use trust_core::account_safety::*;
use uuid::Uuid;

fn scope() -> BrokerAccountScope {
    BrokerAccountScope {
        broker: "ibkr".into(),
        environment: "paper".into(),
        account_id: "DU_TEST".into(),
    }
}
fn policy() -> AccountSafetyPolicy {
    AccountSafetyPolicy {
        currency: "USD".into(),
        max_gross_notional: dec!(5000),
        max_order_notional: dec!(2000),
        max_daily_loss: dec!(100),
        max_drawdown: dec!(200),
        max_snapshot_age_seconds: 120,
        controls: None,
    }
}
fn snapshot() -> BrokerAccountSnapshot {
    let now = Utc::now();
    BrokerAccountSnapshot {
        scope: scope(),
        currency: "USD".into(),
        observed_at: now,
        received_at: now,
        complete: true,
        consistent: true,
        equity: dec!(10000),
        cash: dec!(10000),
        available_funds: dec!(10000),
        realized_pnl: Decimal::ZERO,
        unrealized_pnl: Decimal::ZERO,
        positions: vec![],
        working_orders: vec![],
        margin: None,
    }
}
fn state() -> AccountSafetyState {
    let mut state = AccountSafetyState {
        policy: Some(policy()),
        ..Default::default()
    };
    observe(&mut state, snapshot(), &scope(), Utc::now()).unwrap();
    state
}
fn trade() -> Trade {
    let mut trade = Trade {
        status: Status::New,
        ..Default::default()
    };
    trade.entry.quantity = dec!(10);
    trade.entry.unit_price = dec!(100);
    trade.safety_stop.unit_price = dec!(90);
    trade
}
fn check(state: &AccountSafetyState, trade: &Trade, trades: &[Trade]) -> Result<(), SafetyError> {
    admit(state, &scope(), trade, trades, None, Utc::now())
}

#[test]
fn every_external_position_order_and_shared_account_reservation_consumes_capacity() {
    let mut state = state();
    let proposed = trade();
    let mut other = trade();
    other.id = Uuid::new_v4();
    other.account_id = Uuid::new_v4();
    other.status = Status::Funded;
    state
        .snapshot
        .as_mut()
        .unwrap()
        .positions
        .push(BrokerPosition {
            instrument_id: "external".into(),
            symbol: "MANUAL".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            quantity: dec!(-10),
            market_value: dec!(-2000),
        });
    state
        .snapshot
        .as_mut()
        .unwrap()
        .working_orders
        .push(BrokerWorkingOrder {
            order_id: "external-order".into(),
            client_order_id: None,
            instrument_id: "other".into(),
            symbol: "OTHER".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            side: "buy".into(),
            order_type: "limit".into(),
            stop_price: None,
            active: true,
            remaining_quantity: dec!(10),
            notional: Some(dec!(1000)),
        });
    assert!(check(&state, &proposed, std::slice::from_ref(&other)).is_ok());
    state.policy.as_mut().unwrap().max_gross_notional = dec!(4999.999999);
    assert_eq!(
        check(&state, &proposed, &[other]).unwrap_err().code,
        "account_exposure_exceeded"
    );
}

#[test]
fn submission_credits_only_its_own_existing_reservation() {
    let state = state();
    let mut proposed = trade();
    proposed.status = Status::Funded;
    let mut other = trade();
    other.id = Uuid::new_v4();
    other.status = Status::Funded;
    let mut limited = state.clone();
    limited.policy.as_mut().unwrap().max_gross_notional = dec!(2000);
    assert!(check(&limited, &proposed, &[proposed.clone(), other.clone()]).is_ok());
    limited.policy.as_mut().unwrap().max_gross_notional = dec!(1999);
    limited.policy.as_mut().unwrap().max_order_notional = dec!(1000);
    assert_eq!(
        check(&limited, &proposed, &[proposed.clone(), other])
            .unwrap_err()
            .code,
        "account_exposure_exceeded"
    );
}

#[test]
fn stale_future_incomplete_foreign_currency_and_unbounded_evidence_never_admit() {
    let state = state();
    let proposed = trade();
    let mut cases = Vec::new();
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().observed_at = Utc::now() - Duration::seconds(121);
    cases.push(s);
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().observed_at = Utc::now() + Duration::seconds(1);
    cases.push(s);
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().complete = false;
    cases.push(s);
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().consistent = false;
    cases.push(s);
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().scope.account_id = "WRONG".into();
    cases.push(s);
    let mut s = state.clone();
    s.snapshot.as_mut().unwrap().currency = "EUR".into();
    cases.push(s);
    let mut s = state.clone();
    s.snapshot
        .as_mut()
        .unwrap()
        .working_orders
        .push(BrokerWorkingOrder {
            order_id: "market".into(),
            client_order_id: None,
            instrument_id: "x".into(),
            symbol: "X".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            side: "buy".into(),
            order_type: "limit".into(),
            stop_price: None,
            active: true,
            remaining_quantity: dec!(1),
            notional: None,
        });
    cases.push(s);
    for case in cases {
        assert!(check(&case, &proposed, &[]).is_err());
    }
}

#[test]
fn unrealized_daily_loss_latches_and_later_profit_does_not_resume() {
    let mut state = state();
    let mut observation = snapshot();
    observation.equity = dec!(9900);
    observation.unrealized_pnl = dec!(-100);
    assert_eq!(
        observe(&mut state, observation, &scope(), Utc::now())
            .unwrap_err()
            .code,
        "daily_equity_loss_exceeded"
    );
    assert!(state.halt.is_some());
    observe(&mut state, snapshot(), &scope(), Utc::now()).unwrap();
    assert_eq!(
        check(&state, &trade(), &[]).unwrap_err().code,
        "account_halted"
    );
}

#[test]
fn drawdown_uses_persisted_high_water_and_cash_flows_do_not_erase_losses() {
    let mut state = state();
    state.policy.as_mut().unwrap().max_daily_loss = dec!(1000);
    let mut peak = snapshot();
    peak.equity = dec!(10500);
    peak.unrealized_pnl = dec!(500);
    observe(&mut state, peak, &scope(), Utc::now()).unwrap();
    let mut lower = snapshot();
    lower.equity = dec!(10300);
    lower.unrealized_pnl = dec!(300);
    assert_eq!(
        observe(&mut state, lower.clone(), &scope(), Utc::now())
            .unwrap_err()
            .code,
        "equity_drawdown_exceeded"
    );
    let encoded = serde_json::to_string(&state).unwrap();
    let mut restored: AccountSafetyState = serde_json::from_str(&encoded).unwrap();
    let mut deposit = lower;
    deposit.equity = dec!(11300);
    assert_eq!(
        observe(&mut restored, deposit.clone(), &scope(), Utc::now())
            .unwrap_err()
            .code,
        "equity_cash_flow_unreconciled"
    );
    record_cash_flow(&mut restored, "bank-receipt-1", dec!(1000)).unwrap();
    assert_eq!(
        observe(&mut restored, deposit, &scope(), Utc::now())
            .unwrap_err()
            .code,
        "equity_drawdown_exceeded"
    );
    assert!(record_cash_flow(&mut restored, "bank-receipt-1", dec!(1000)).is_err());
}

#[test]
fn instrument_expiry_direction_count_and_other_agents_are_distinct_mandate_constraints() {
    let mut state = state();
    let proposed = trade();
    let owner = "agent-a";
    let mandate = AgentMandate {
        instrument_ids: BTreeSet::from([proposed.trading_vehicle.id]),
        max_order_notional: dec!(1000),
        max_total_notional: dec!(1500),
        max_open_trades: 1,
        allow_short: false,
        expires_at: Utc::now() + Duration::hours(1),
        risk_budget: None,
    };
    state.mandates.insert(owner.into(), mandate);
    state.trade_owners.insert(proposed.id, owner.into());
    assert!(admit(&state, &scope(), &proposed, &[], Some(owner), Utc::now()).is_ok());
    let mut other = proposed.clone();
    other.id = Uuid::new_v4();
    other.status = Status::Funded;
    state.trade_owners.insert(other.id, owner.into());
    assert_eq!(
        admit(
            &state,
            &scope(),
            &proposed,
            std::slice::from_ref(&other),
            Some(owner),
            Utc::now()
        )
        .unwrap_err()
        .code,
        "agent_exposure_limit_exceeded"
    );
    state.trade_owners.insert(other.id, "agent-b".into());
    assert!(admit(
        &state,
        &scope(),
        &proposed,
        &[other],
        Some(owner),
        Utc::now()
    )
    .is_ok());
    state.mandates.get_mut(owner).unwrap().expires_at = Utc::now() - Duration::seconds(1);
    assert_eq!(
        admit(&state, &scope(), &proposed, &[], Some(owner), Utc::now())
            .unwrap_err()
            .code,
        "agent_mandate_expired"
    );
    state.mandates.get_mut(owner).unwrap().expires_at = Utc::now() + Duration::hours(1);
    state
        .mandates
        .get_mut(owner)
        .unwrap()
        .instrument_ids
        .clear();
    assert_eq!(
        check_mandate(&state, owner, &proposed, &[], Utc::now())
            .unwrap_err()
            .code,
        "instrument_not_permitted"
    );
    state
        .mandates
        .get_mut(owner)
        .unwrap()
        .instrument_ids
        .insert(proposed.trading_vehicle.id);
    let mut short = proposed;
    short.category = model::TradeCategory::Short;
    assert_eq!(
        check_mandate(&state, owner, &short, &[], Utc::now())
            .unwrap_err()
            .code,
        "short_not_permitted"
    );
}

#[test]
fn increasing_unrelated_reservations_never_increases_admissible_capacity() {
    use proptest::test_runner::{TestCaseError, TestRunner};
    let mut runner = TestRunner::default();
    runner
        .run(&(0u32..20, 1000i64..20000), |(count, limit)| {
            let mut state = state();
            state.policy.as_mut().unwrap().max_gross_notional = Decimal::from(limit);
            state.policy.as_mut().unwrap().max_order_notional = dec!(1000);
            state.snapshot.as_mut().unwrap().available_funds = dec!(100000);
            let proposed = trade();
            let mut reservations: Vec<_> = (0..count)
                .map(|_| {
                    let mut t = trade();
                    t.status = Status::Funded;
                    t.id = Uuid::new_v4();
                    t
                })
                .collect();
            let before = check(&state, &proposed, &reservations).is_ok();
            let mut extra = trade();
            extra.status = Status::Funded;
            extra.id = Uuid::new_v4();
            reservations.push(extra);
            let after = check(&state, &proposed, &reservations).is_ok();
            if !before && after {
                return Err(TestCaseError::fail("Extra reservations increased capacity"));
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn subsecond_future_duplicate_and_derivative_observations_fail_closed() {
    let now = Utc::now();
    let mut observation = snapshot();
    observation.received_at = now + Duration::milliseconds(100);
    observation.observed_at = observation.received_at;
    assert!(validate_snapshot(&observation, &scope(), &policy(), now).is_err());
    let position = BrokerPosition {
        instrument_id: "external".into(),
        symbol: "TEST".into(),
        currency: "USD".into(),
        asset_class: "STK".into(),
        quantity: dec!(1),
        market_value: dec!(100),
    };
    let mut observation = snapshot();
    observation.positions = vec![position.clone(), position.clone()];
    assert_eq!(
        validate_snapshot(&observation, &scope(), &policy(), Utc::now())
            .unwrap_err()
            .code,
        "broker_positions_invalid"
    );
    observation.positions = vec![position];
    observation.positions[0].asset_class = "OPT".into();
    assert_eq!(
        validate_snapshot(&observation, &scope(), &policy(), Utc::now())
            .unwrap_err()
            .code,
        "unsupported_broker_risk_instrument"
    );
}

#[test]
fn arithmetic_overflow_never_becomes_zero_risk_or_capacity() {
    let state = state();
    let mut proposed = trade();
    proposed.entry.quantity = Decimal::MAX;
    proposed.entry.unit_price = dec!(2);
    assert_eq!(
        check(&state, &proposed, &[]).unwrap_err().code,
        "risk_arithmetic_overflow"
    );
    let mut state = state.clone();
    state.baseline.as_mut().unwrap().high_water = Decimal::MAX;
    assert_eq!(
        record_cash_flow(&mut state, "overflow", dec!(1))
            .unwrap_err()
            .code,
        "risk_arithmetic_overflow"
    );
    assert!(state.cash_flow_references.is_empty());
}

#[test]
fn mandate_capacity_marks_existing_exposure_and_respects_remaining_slots() {
    let mut state = state();
    let proposed = trade();
    let mut existing = trade();
    existing.id = Uuid::new_v4();
    existing.status = Status::Funded;
    existing.trading_vehicle.broker_asset_id = Some("contract".into());
    state.trade_owners.insert(existing.id, "agent".into());
    state.mandates.insert(
        "agent".into(),
        AgentMandate {
            instrument_ids: [proposed.trading_vehicle.id].into_iter().collect(),
            max_order_notional: dec!(2000),
            max_total_notional: dec!(2500),
            max_open_trades: 2,
            allow_short: false,
            expires_at: Utc::now() + Duration::hours(1),
            risk_budget: None,
        },
    );
    assert!(check_mandate(
        &state,
        "agent",
        &proposed,
        std::slice::from_ref(&existing),
        Utc::now()
    )
    .is_ok());
    state
        .snapshot
        .as_mut()
        .unwrap()
        .positions
        .push(BrokerPosition {
            instrument_id: "contract".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            quantity: dec!(10),
            market_value: dec!(2000),
        });
    assert_eq!(
        check_mandate(
            &state,
            "agent",
            &proposed,
            std::slice::from_ref(&existing),
            Utc::now()
        )
        .unwrap_err()
        .code,
        "agent_exposure_limit_exceeded"
    );
    let capacity =
        remaining_capacity(&state, std::slice::from_ref(&existing), Some("agent")).unwrap();
    assert_eq!(capacity.max_new_notional, dec!(500));
    assert_eq!(capacity.agent_open_trades_remaining, Some(1));
    state.mandates.get_mut("agent").unwrap().max_open_trades = 1;
    assert_eq!(
        remaining_capacity(&state, &[existing], Some("agent"))
            .unwrap()
            .max_new_notional,
        Decimal::ZERO
    );
}

#[test]
fn utc_rollover_does_not_forgive_unobserved_overnight_loss() {
    let now = Utc::now();
    let mut state = state();
    state.baseline.as_mut().unwrap().day = (now - Duration::days(1)).date_naive();
    let mut observation = snapshot();
    observation.equity = dec!(9900);
    observation.unrealized_pnl = dec!(-100);
    assert_eq!(
        observe(
            &mut state,
            observation,
            &scope(),
            now + Duration::milliseconds(1)
        )
        .unwrap_err()
        .code,
        "daily_equity_loss_exceeded"
    );
    assert_eq!(state.baseline.as_ref().unwrap().day_start, dec!(10000));
}

#[test]
fn verified_recovery_allows_bounded_manual_exposure_but_requires_trust_entry_and_protection() {
    let mut snapshot = snapshot();
    snapshot.positions.push(BrokerPosition {
        instrument_id: "manual".into(),
        symbol: "MANUAL".into(),
        currency: "USD".into(),
        asset_class: "STK".into(),
        quantity: dec!(5),
        market_value: dec!(500),
    });
    assert!(verify_reconciled(&snapshot, &[]).is_ok());
    let mut trade = trade();
    trade.trading_vehicle.broker_asset_id = Some("1234".into());
    trade.status = Status::Submitted;
    assert_eq!(
        verify_reconciled(&snapshot, std::slice::from_ref(&trade))
            .unwrap_err()
            .code,
        "broker_entry_unreconciled"
    );
    trade.status = Status::Filled;
    trade.entry.filled_quantity = trade.entry.quantity;
    assert_eq!(
        verify_reconciled(&snapshot, &[trade]).unwrap_err().code,
        "broker_protection_unreconciled"
    );
}

#[test]
fn inactive_retyped_or_weakened_broker_stops_block_admission_for_both_sides() {
    for short in [false, true] {
        let mut trade = trade();
        trade.status = Status::Filled;
        trade.category = if short {
            model::TradeCategory::Short
        } else {
            model::TradeCategory::Long
        };
        trade.trading_vehicle.broker_asset_id = Some("contract".into());
        trade.entry.filled_quantity = trade.entry.quantity;
        trade.safety_stop.unit_price = dec!(100);
        trade.safety_stop.broker_order_id = Some("stop".into());
        let mut snapshot = snapshot();
        snapshot.positions.push(BrokerPosition {
            instrument_id: "contract".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            quantity: if short { dec!(-10) } else { dec!(10) },
            market_value: if short { dec!(-1000) } else { dec!(1000) },
        });
        snapshot.working_orders.push(BrokerWorkingOrder {
            order_id: "stop".into(),
            client_order_id: None,
            instrument_id: "contract".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            asset_class: "STK".into(),
            side: if short { "buy".into() } else { "sell".into() },
            order_type: "stop".into(),
            stop_price: Some(dec!(100)),
            active: true,
            remaining_quantity: dec!(10),
            notional: None,
        });
        assert!(verify_reconciled(&snapshot, std::slice::from_ref(&trade)).is_ok());
        for fault in ["inactive", "limit", "missing_price", "weaker"] {
            let mut invalid = snapshot.clone();
            let stop = invalid.working_orders.first_mut().unwrap();
            match fault {
                "inactive" => stop.active = false,
                "limit" => stop.order_type = "limit".into(),
                "missing_price" => stop.stop_price = None,
                _ => stop.stop_price = Some(if short { dec!(101) } else { dec!(99) }),
            }
            assert_eq!(
                verify_reconciled(&invalid, std::slice::from_ref(&trade))
                    .unwrap_err()
                    .code,
                "broker_protection_unreconciled"
            );
        }
    }
}

#[test]
fn owner_pnl_counter_reconciliation_preserves_losses_and_cannot_reset_high_water() {
    let mut state = state();
    state.baseline.as_mut().unwrap().last_pnl = dec!(1000);
    let mut observation = snapshot();
    observation.equity = dec!(9900);
    assert_eq!(
        observe(&mut state, observation, &scope(), Utc::now())
            .unwrap_err()
            .code,
        "equity_cash_flow_unreconciled"
    );
    assert_eq!(
        reconcile_equity(
            &mut state,
            &scope(),
            "independent-statement",
            dec!(0),
            Utc::now()
        )
        .unwrap_err()
        .code,
        "daily_equity_loss_exceeded"
    );
    assert_eq!(state.baseline.as_ref().unwrap().high_water, dec!(10000));
    assert_eq!(state.baseline.as_ref().unwrap().day_start, dec!(10000));
    assert!(state.halt.is_some());
    assert_eq!(
        reconcile_equity(
            &mut state,
            &scope(),
            "independent-statement",
            dec!(0),
            Utc::now()
        )
        .unwrap_err()
        .code,
        "invalid_cash_flow_evidence"
    );
}
