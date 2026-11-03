//! Admission, fills and exits use SQLite/facade; expected cash comes from the scenario prices.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Exit {
    Target,
    ImprovedTarget,
    Stop,
    SlippedStop,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Scenario {
    pub side: TradeCategory,
    pub quantity: Decimal,
    pub better_entry: bool,
    pub exit: Exit,
    pub fees: bool,
}

pub(super) struct Outcome {
    pub trade: Trade,
    pub net: Decimal,
    pub planned_r: Decimal,
    pub cash_path: Vec<Decimal>,
}

fn fee(trade: &Trade, order: &Order, role: &str, amount: Decimal) -> model::FeeActivity {
    model::FeeActivity {
        broker: "alpaca".into(),
        broker_activity_id: format!("{}:{role}", trade.id),
        account_id: trade.account_id,
        broker_order_id: order.broker_order_id.clone(),
        symbol: Some(trade.trading_vehicle.symbol.clone()),
        activity_type: "FEE".into(),
        amount,
        occurred_at: Utc::now().naive_utc(),
        raw_json: None,
    }
}

pub(super) fn run(
    trust: &mut TrustFacade,
    broker: &TestBroker,
    account: &Account,
    vehicle: &TradingVehicle,
    scenario: Scenario,
    initial_cash: Decimal,
) -> Outcome {
    let Scenario {
        side,
        quantity,
        better_entry,
        exit,
        fees,
    } = scenario;
    let direction = if side == TradeCategory::Long {
        dec!(1)
    } else {
        dec!(-1)
    };
    let entry = dec!(100.25);
    let stop = entry - direction * dec!(5.125);
    let target = entry + direction * dec!(10.25);
    let fill = entry - direction * if better_entry { dec!(0.25) } else { dec!(0) };
    let close_price = match exit {
        Exit::Target => target,
        Exit::ImprovedTarget => target + direction * dec!(0.125),
        Exit::Stop => stop,
        Exit::SlippedStop => stop - direction * dec!(0.25),
    };
    let status = match exit {
        Exit::Target | Exit::ImprovedTarget => Status::ClosedTarget,
        _ => Status::ClosedStopLoss,
    };
    let funding = quantity
        * if side == TradeCategory::Long {
            entry
        } else {
            stop
        };
    let refund = if side == TradeCategory::Long {
        quantity * (entry - fill)
    } else {
        dec!(0)
    };
    let gross = direction * (close_price - fill) * quantity;
    let (entry_fee, exit_fee) = if fees {
        (dec!(0.37), dec!(0.19))
    } else {
        (dec!(0), dec!(0))
    };
    let net = gross - entry_fee - exit_fee;
    let mut cash_path = vec![initial_cash - funding];
    let mut receipts = vec![(TransactionCategory::FundTrade(Uuid::nil()), funding)];
    let mut cash = initial_cash - funding;
    broker.state.lock().unwrap().fees.clear();
    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle.clone(),
                quantity,
                currency: Currency::USD,
                category: side,
                thesis: Some("cash contract".into()),
                sector: Some("technology".into()),
                asset_class: Some("equity".into()),
                context: None,
            },
            stop,
            entry,
            target,
        )
        .unwrap();
    receipts[0].0 = TransactionCategory::FundTrade(trade.id);
    let funded = fund_trade_direct(trust, account, &trade);
    assert_eq!(funded.balance.funding, funding);
    assert_eq!(usd_balance(trust, account).total_available, cash);
    let submitted = submit_trade_direct(trust, account, &funded);
    if fees {
        broker.state.lock().unwrap().fees =
            vec![fee(&submitted, &submitted.entry, "entry", entry_fee)];
    }
    broker.enqueue_sync(Ok((
        Status::Filled,
        vec![
            order_entry_filled(&submitted, fill),
            order_target_accepted(&submitted),
            order_stop_held(&submitted),
        ],
    )));
    trust.sync_trade(&submitted, account).unwrap();
    let filled = trust.read_trade(trade.id).unwrap();
    assert_eq!(filled.status, Status::Filled);
    assert_eq!(filled.entry.average_filled_price, Some(fill));
    assert_eq!(filled.entry.filled_quantity, quantity);
    if refund > dec!(0) {
        cash += refund;
        cash_path.push(cash);
        receipts.push((TransactionCategory::PaymentFromTrade(trade.id), refund));
    }
    if fees {
        cash -= entry_fee;
        cash_path.push(cash);
        receipts.push((TransactionCategory::FeeOpen(trade.id), entry_fee));
    }
    assert_eq!(usd_balance(trust, account).total_available, cash);
    let closing_order = if status == Status::ClosedTarget {
        &filled.target
    } else {
        &filled.safety_stop
    };
    if fees {
        broker
            .state
            .lock()
            .unwrap()
            .fees
            .push(fee(&filled, closing_order, "exit", exit_fee));
    }
    let orders = if status == Status::ClosedTarget {
        vec![
            order_entry_filled(&filled, fill),
            order_target_filled(&filled, close_price),
            order_stop_canceled(&filled),
        ]
    } else {
        vec![
            order_entry_filled(&filled, fill),
            order_stop_filled(&filled, close_price),
            order_target_canceled(&filled),
        ]
    };
    broker.enqueue_sync(Ok((status, orders.clone())));
    trust.sync_trade(&filled, account).unwrap();
    // Returned principal plus directional gross P&L, calculated without the ledger/calculators.
    let returned = funding - refund + gross;
    cash += returned;
    cash_path.push(cash);
    receipts.push((TransactionCategory::PaymentFromTrade(trade.id), returned));
    if fees {
        cash -= exit_fee;
        cash_path.push(cash);
        receipts.push((TransactionCategory::FeeClose(trade.id), exit_fee));
    }
    let closed = trust.read_trade(trade.id).unwrap();
    assert_eq!(closed.status, status);
    assert_eq!(closed.balance.total_performance, net, "{scenario:?}");
    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert_eq!(closed.balance.capital_out_market, dec!(0));
    assert_eq!(closed.entry.status, OrderStatus::Filled);
    assert_eq!(
        closed.target.status,
        if status == Status::ClosedTarget {
            OrderStatus::Filled
        } else {
            OrderStatus::Canceled
        }
    );
    assert_eq!(
        closed.safety_stop.status,
        if status == Status::ClosedStopLoss {
            OrderStatus::Filled
        } else {
            OrderStatus::Canceled
        }
    );
    let balance = usd_balance(trust, account);
    assert_eq!(balance.total_available, initial_cash + net);
    assert_eq!(balance.total_balance, initial_cash + net);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(cash, initial_cash + net);
    let txs = account_transactions_for_trade(trust, account, trade.id);
    let mut actual: Vec<_> = txs.iter().map(|tx| (tx.category, tx.amount)).collect();
    let sort = |values: &mut Vec<(TransactionCategory, Decimal)>| {
        values.sort_by_key(|(category, amount)| (category.to_string(), *amount))
    };
    sort(&mut actual);
    sort(&mut receipts);
    assert_eq!(actual, receipts, "all cash receipts and exact amounts");
    // Repeated fills/fees after terminal reconciliation cannot duplicate or mutate money.
    broker.enqueue_sync(Ok((status, orders)));
    trust.sync_trade(&closed, account).unwrap();
    assert_eq!(
        account_transactions_for_trade(trust, account, trade.id),
        txs
    );
    assert_eq!(usd_balance(trust, account), balance);
    let replayed = trust.read_trade(trade.id).unwrap();
    // Poll/audit bookkeeping may refresh updated_at, but no economic field may change.
    let mut expected = closed.clone();
    expected.updated_at = replayed.updated_at;
    expected.balance.updated_at = replayed.balance.updated_at;
    expected.entry.updated_at = replayed.entry.updated_at;
    expected.safety_stop.updated_at = replayed.safety_stop.updated_at;
    expected.target.updated_at = replayed.target.updated_at;
    assert_eq!(replayed, expected);
    Outcome {
        trade: closed,
        net,
        planned_r: if status == Status::ClosedTarget {
            dec!(2)
        } else {
            dec!(-1)
        },
        cash_path,
    }
}

fn check(scenario: Scenario) {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "lifecycle-cash", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(500000), Currency::USD);
    add_risk_rules(&mut trust, &account, 20.0, 2.0);
    let vehicle = fractional_vehicle(&mut trust, "CASH");
    let outcome = run(
        &mut trust,
        &broker,
        &account,
        &vehicle,
        scenario,
        dec!(500000),
    );
    let mut cash = vec![dec!(500000)];
    cash.extend(outcome.cash_path);
    super::portfolio::assert_cash_curve(&mut trust, &account, &cash);
}

macro_rules! exit_contracts {
    ($module:ident, $side:ident, $quantity:expr, $better:expr, $fees:expr) => {
        mod $module {
            use super::*;
            fn scenario(exit: Exit) -> Scenario {
                Scenario {
                    side: TradeCategory::$side,
                    quantity: $quantity,
                    better_entry: $better,
                    exit,
                    fees: $fees,
                }
            }
            #[test]
            fn target_fill() {
                check(scenario(Exit::Target));
            }
            #[test]
            fn improved_target_fill() {
                check(scenario(Exit::ImprovedTarget));
            }
            #[test]
            fn stop_fill() {
                check(scenario(Exit::Stop));
            }
            #[test]
            fn gap_beyond_stop() {
                check(scenario(Exit::SlippedStop));
            }
        }
    };
}
exit_contracts!(
    long_whole_at_entry_without_fees,
    Long,
    dec!(10),
    false,
    false
);
exit_contracts!(long_whole_at_entry_with_fees, Long, dec!(10), false, true);
exit_contracts!(
    long_whole_improved_entry_without_fees,
    Long,
    dec!(10),
    true,
    false
);
exit_contracts!(
    long_whole_improved_entry_with_fees,
    Long,
    dec!(10),
    true,
    true
);
exit_contracts!(
    long_fractional_at_entry_without_fees,
    Long,
    dec!(2.5),
    false,
    false
);
exit_contracts!(
    long_fractional_at_entry_with_fees,
    Long,
    dec!(2.5),
    false,
    true
);
exit_contracts!(
    long_fractional_improved_entry_without_fees,
    Long,
    dec!(2.5),
    true,
    false
);
exit_contracts!(
    long_fractional_improved_entry_with_fees,
    Long,
    dec!(2.5),
    true,
    true
);
exit_contracts!(
    short_whole_at_entry_without_fees,
    Short,
    dec!(10),
    false,
    false
);
exit_contracts!(short_whole_at_entry_with_fees, Short, dec!(10), false, true);
exit_contracts!(
    short_whole_improved_entry_without_fees,
    Short,
    dec!(10),
    true,
    false
);
exit_contracts!(
    short_whole_improved_entry_with_fees,
    Short,
    dec!(10),
    true,
    true
);
exit_contracts!(
    short_fractional_at_entry_without_fees,
    Short,
    dec!(2.5),
    false,
    false
);
exit_contracts!(
    short_fractional_at_entry_with_fees,
    Short,
    dec!(2.5),
    false,
    true
);
exit_contracts!(
    short_fractional_improved_entry_without_fees,
    Short,
    dec!(2.5),
    true,
    false
);
exit_contracts!(
    short_fractional_improved_entry_with_fees,
    Short,
    dec!(2.5),
    true,
    true
);

pub(super) fn fractional_vehicle(trust: &mut TrustFacade, symbol: &str) -> TradingVehicle {
    trust
        .upsert_trading_vehicle(model::database::TradingVehicleUpsert {
            symbol: symbol.into(),
            isin: None,
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".into(),
            broker_asset_id: Some(symbol.into()),
            exchange: Some("NASDAQ".into()),
            broker_asset_class: Some("us_equity".into()),
            broker_asset_status: Some("active".into()),
            tradable: Some(true),
            marginable: Some(true),
            shortable: Some(true),
            easy_to_borrow: Some(true),
            fractionable: Some(true),
            fixed_income: None,
        })
        .unwrap()
}
