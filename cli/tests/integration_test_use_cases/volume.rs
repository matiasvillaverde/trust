//! Large analytical inputs and persisted lifecycle workloads are deliberately separate.
use super::*;

pub(super) fn analytical_history() {
    let now = Utc::now().naive_utc();
    let trades: Vec<_> = (0..36865)
        .map(|index| {
            let (pnl, quantity, status) = match index % 4 {
                0 => (dec!(8), dec!(1), Status::ClosedTarget),
                1 => (dec!(-5), dec!(1), Status::ClosedStopLoss),
                2 => (dec!(16), dec!(2), Status::ClosedTarget),
                _ => (dec!(-10), dec!(2), Status::ClosedStopLoss),
            };
            let date = now - Duration::days(index / 101) - Duration::hours(12);
            let mut trade = Trade {
                status,
                created_at: date,
                updated_at: date,
                sector: Some(
                    if index % 2 == 0 {
                        "technology"
                    } else {
                        "energy"
                    }
                    .into(),
                ),
                ..Trade::default()
            };
            trade.entry.unit_price = dec!(100);
            trade.entry.quantity = quantity;
            trade.target.unit_price = dec!(108);
            trade.target.quantity = quantity;
            trade.safety_stop.unit_price = dec!(95);
            trade.safety_stop.quantity = quantity;
            trade.balance.total_performance = pnl;
            trade
        })
        .collect();
    // 9,216 complete four-trade blocks, plus one $8 winner.
    let stats = PerformanceCalculator::calculate_performance_stats(&trades);
    assert_eq!(stats.total_trades, 36865);
    assert_eq!(stats.winning_trades, 18433);
    assert_eq!(stats.losing_trades, 18432);
    assert_eq!(stats.best_trade, Some(dec!(16)));
    assert_eq!(stats.worst_trade, Some(dec!(-10)));
    assert_eq!(stats.average_win, dec!(221192) / dec!(18433));
    assert_eq!(stats.average_loss, dec!(-7.5));
    assert_eq!(stats.win_rate, dec!(18433) / dec!(36865) * dec!(100));
    assert_eq!(stats.average_r_multiple, dec!(11060.8) / dec!(36865));
    let recent = PerformanceCalculator::filter_trades_by_days(&trades, 30);
    assert_eq!(recent.len(), 3030, "30 full days with 101 records each");
    let sectors = ConcentrationCalculator::analyze_by_metadata(&trades, MetadataField::Sector);
    assert_eq!(sectors.groups.len(), 2);
    assert_eq!(sectors.total_risk, dec!(0));
    assert!(sectors.concentration_warnings.is_empty());
}

#[tracing::instrument(
    level = "trace",
    target = "trust::perf",
    name = "lifecycle.workload",
    skip_all
)]
pub(super) fn sync_lifecycles(count: usize) {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "sync-volume", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(5000000), Currency::USD);
    add_risk_rules(&mut trust, &account, 20.0, 2.0);
    let vehicle = create_vehicle(&mut trust, "LOAD");
    let mut expected_cash = dec!(5000000);
    let mut wins = 0;
    for index in 0..count {
        let side = if index % 2 == 0 {
            TradeCategory::Long
        } else {
            TradeCategory::Short
        };
        let direction = if side == TradeCategory::Long {
            dec!(1)
        } else {
            dec!(-1)
        };
        let stop = dec!(100) - direction * dec!(5);
        let target = dec!(100) + direction * dec!(8);
        let trade =
            tracing::trace_span!(target: "trust::perf", "lifecycle.create").in_scope(|| {
                create_trade(
                    &mut trust,
                    &account,
                    &vehicle,
                    side,
                    1,
                    dec!(100),
                    stop,
                    target,
                    None,
                    None,
                    None,
                    None,
                )
            });
        let funded = tracing::trace_span!(target: "trust::perf", "lifecycle.fund")
            .in_scope(|| fund_trade_direct(&mut trust, &account, &trade));
        let submitted = tracing::trace_span!(target: "trust::perf", "lifecycle.submit")
            .in_scope(|| submit_trade_direct(&mut trust, &account, &funded));
        let (status, net, orders) = if index % 4 < 2 {
            wins += 1;
            (
                Status::ClosedTarget,
                dec!(8),
                vec![
                    order_entry_filled(&submitted, dec!(100)),
                    order_target_filled(&submitted, target),
                    order_stop_canceled(&submitted),
                ],
            )
        } else {
            (
                Status::ClosedStopLoss,
                dec!(-5),
                vec![
                    order_entry_filled(&submitted, dec!(100)),
                    order_stop_filled(&submitted, stop),
                    order_target_canceled(&submitted),
                ],
            )
        };
        broker.enqueue_sync(Ok((status, orders)));
        tracing::trace_span!(target: "trust::perf", "lifecycle.sync")
            .in_scope(|| trust.sync_trade(&submitted, &account).unwrap());
        expected_cash += net;
        assert_eq!(
            trust
                .read_trade(trade.id)
                .unwrap()
                .balance
                .total_performance,
            net
        );
        assert_eq!(
            usd_balance(&mut trust, &account).total_available,
            expected_cash
        );
    }
    assert_eq!(
        broker.state.lock().unwrap().sync_calls,
        count,
        "every workload trade traverses broker synchronization"
    );
    assert_eq!(
        trust
            .search_trades(account.id, Status::ClosedTarget)
            .unwrap()
            .len(),
        wins
    );
    assert_eq!(
        trust
            .search_trades(account.id, Status::ClosedStopLoss)
            .unwrap()
            .len(),
        count - wins
    );
    assert_eq!(
        account_transactions(&mut trust, &account).len(),
        1 + count * 2
    );
    let balance = usd_balance(&mut trust, &account);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.total_balance, expected_cash);
    assert!(trust
        .calculate_open_positions(Some(account.id))
        .unwrap()
        .is_empty());
}

#[test]
fn analytics_aggregate_36865_closed_records_with_exact_statistics() {
    analytical_history();
}
#[test]
fn repeated_persisted_long_and_short_lifecycles_conserve_cash() {
    sync_lifecycles(32);
}
