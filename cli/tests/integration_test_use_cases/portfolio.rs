//! Portfolio expectations come from proposed prices/fees, never from the returned ledger.
use super::financial_scenarios::{run, Exit, Scenario};
use super::*;

pub(super) fn assert_cash_curve(trust: &mut TrustFacade, account: &Account, expected: &[Decimal]) {
    let curve =
        RealizedDrawdownCalculator::calculate_equity_curve(&account_transactions(trust, account))
            .unwrap();
    assert_eq!(
        curve.points.iter().map(|p| p.balance).collect::<Vec<_>>(),
        expected,
        "available-cash path, including capital reservations and actual fees"
    );
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();
    let current = *expected.last().unwrap();
    let peak = *expected.iter().max().unwrap();
    // Enumerate every earlier-peak/later-trough pair. This does not reuse the calculator's loop
    // or feed its own ledger back as its oracle. Equal maximum declines use the earliest trough.
    let (trough_index, drawdown, peak_at_trough) = expected
        .iter()
        .enumerate()
        .map(|(i, cash)| {
            let prior_peak = *expected[..=i].iter().max().unwrap();
            (i, prior_peak - cash, prior_peak)
        })
        .max_by_key(|(i, amount, _)| (*amount, std::cmp::Reverse(*i)))
        .unwrap();
    assert_eq!(metrics.current_equity, current);
    assert_eq!(metrics.peak_equity, peak);
    assert_eq!(metrics.current_drawdown, peak - current);
    assert_eq!(
        metrics.current_drawdown_percentage,
        (peak - current) * dec!(100) / peak
    );
    assert_eq!(metrics.max_drawdown, drawdown);
    assert_eq!(
        metrics.max_drawdown_percentage,
        drawdown * dec!(100) / peak_at_trough
    );
    assert_eq!(
        metrics.max_drawdown_date,
        Some(curve.points[trough_index].timestamp)
    );
}

fn portfolio(scenarios: &[Scenario]) {
    let broker = TestBroker::success();
    let mut trust = new_trust_with_broker(broker.clone());
    let account = create_account(&mut trust, "portfolio", model::Environment::Paper);
    deposit(&mut trust, &account, dec!(3000000), Currency::USD);
    add_risk_rules(&mut trust, &account, 20.0, 2.0);
    let vehicle = super::financial_scenarios::fractional_vehicle(&mut trust, "BOOK");
    let mut cash = dec!(3000000);
    let mut expected_cash = vec![cash];
    let mut profits = Vec::new();
    let mut multiples = Vec::new();
    let mut targets = 0;
    for &scenario in scenarios {
        let outcome = run(&mut trust, &broker, &account, &vehicle, scenario, cash);
        cash += outcome.net;
        expected_cash.extend(outcome.cash_path);
        profits.push(outcome.net);
        // This legacy metric uses planned bracket prices; realized net profit is checked independently.
        multiples.push(outcome.planned_r);
        targets += usize::from(outcome.trade.status == Status::ClosedTarget);
    }
    let closed = trust.search_closed_trades(Some(account.id)).unwrap();
    assert_eq!(closed.len(), scenarios.len());
    assert_eq!(
        trust
            .search_trades(account.id, Status::ClosedTarget)
            .unwrap()
            .len(),
        targets
    );
    assert_eq!(
        trust
            .search_trades(account.id, Status::ClosedStopLoss)
            .unwrap()
            .len(),
        scenarios.len() - targets
    );
    assert_eq!(
        closed
            .iter()
            .map(|t| t.balance.total_performance)
            .sum::<Decimal>(),
        profits.iter().sum()
    );
    let wins: Vec<_> = profits.iter().copied().filter(|p| *p > dec!(0)).collect();
    let losses: Vec<_> = profits.iter().copied().filter(|p| *p < dec!(0)).collect();
    let average = |values: &[Decimal]| {
        if values.is_empty() {
            dec!(0)
        } else {
            values.iter().sum::<Decimal>() / Decimal::from(values.len())
        }
    };
    let stats = PerformanceCalculator::calculate_performance_stats(&closed);
    assert_eq!(stats.total_trades, scenarios.len());
    assert_eq!(stats.winning_trades, wins.len());
    assert_eq!(stats.losing_trades, losses.len());
    assert_eq!(
        stats.win_rate,
        Decimal::from(wins.len()) / Decimal::from(scenarios.len()) * dec!(100)
    );
    assert_eq!(stats.average_win, average(&wins));
    assert_eq!(stats.average_loss, average(&losses));
    assert_eq!(stats.best_trade, profits.iter().max().copied());
    assert_eq!(stats.worst_trade, profits.iter().min().copied());
    assert_eq!(stats.average_r_multiple, average(&multiples));
    assert_eq!(usd_balance(&mut trust, &account).total_available, cash);
    assert!(trust
        .calculate_open_positions(Some(account.id))
        .unwrap()
        .is_empty());
    assert_cash_curve(&mut trust, &account, &expected_cash);
}

fn trade(side: TradeCategory, exit: Exit, quantity: Decimal, fees: bool) -> Scenario {
    Scenario {
        side,
        exit,
        quantity,
        fees,
        better_entry: false,
    }
}

#[test]
fn successive_long_and_short_winners_increase_available_cash() {
    portfolio(&[
        trade(TradeCategory::Long, Exit::Target, dec!(10), true),
        trade(TradeCategory::Short, Exit::ImprovedTarget, dec!(2.5), true),
        trade(TradeCategory::Long, Exit::Target, dec!(5), false),
    ]);
}
#[test]
fn a_loss_streak_accumulates_losses_and_execution_costs() {
    portfolio(&[
        trade(TradeCategory::Long, Exit::Stop, dec!(10), true),
        trade(TradeCategory::Short, Exit::SlippedStop, dec!(2.5), true),
        trade(TradeCategory::Long, Exit::SlippedStop, dec!(5), false),
    ]);
}
#[test]
fn recovery_after_losses_reaches_a_new_cash_peak() {
    portfolio(&[
        trade(TradeCategory::Short, Exit::Stop, dec!(10), true),
        trade(TradeCategory::Long, Exit::Stop, dec!(2.5), true),
        trade(TradeCategory::Short, Exit::Target, dec!(10), true),
        trade(TradeCategory::Long, Exit::Target, dec!(10), true),
    ]);
}
#[test]
fn alternating_wins_and_losses_reconcile_fractional_positions() {
    portfolio(&[
        trade(TradeCategory::Long, Exit::Target, dec!(2.5), false),
        trade(TradeCategory::Short, Exit::Stop, dec!(2.5), true),
        trade(TradeCategory::Long, Exit::SlippedStop, dec!(2.5), true),
        trade(TradeCategory::Short, Exit::ImprovedTarget, dec!(2.5), false),
    ]);
}
#[test]
fn fees_can_turn_a_target_exit_into_a_net_loss() {
    portfolio(&[
        trade(TradeCategory::Long, Exit::Target, dec!(0.025), true),
        trade(TradeCategory::Short, Exit::Target, dec!(0.025), true),
        trade(TradeCategory::Long, Exit::Target, dec!(0.025), false),
    ]);
}
#[test]
fn improved_entries_change_proceeds_without_duplicating_refunds() {
    portfolio(&[
        Scenario {
            better_entry: true,
            ..trade(TradeCategory::Long, Exit::Stop, dec!(10), true)
        },
        Scenario {
            better_entry: true,
            ..trade(TradeCategory::Short, Exit::Target, dec!(2.5), true)
        },
    ]);
}
