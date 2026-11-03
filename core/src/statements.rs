//! Statement comparison never books money, changes trades or authorizes trading.
use crate::TrustFacade;
use chrono::{DateTime, Utc};
use model::{account_safety::BrokerAccountScope, statement::*};
use rust_decimal::Decimal;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};
use uuid::Uuid;

type Fallible<T> = Result<T, Box<dyn Error>>;
type Amounts = BTreeMap<String, Decimal>;
type Positions = BTreeMap<(String, String), Decimal>;

impl TrustFacade {
    /// Compare all Trust profiles sharing the broker account using one DB snapshot.
    /// Unsupported storage/corrupt evidence must be recorded as inconclusive by the caller.
    pub fn reconcile_broker_statement(
        &mut self,
        scope: &BrokerAccountScope,
        statement: &BrokerStatement,
    ) -> Fallible<StatementReconciliation> {
        if scope.broker != "ibkr" || scope.account_id != statement.broker_account_id {
            return Err("Statement broker account mismatch".into());
        }
        let accounts: BTreeSet<_> = self
            .search_all_accounts()?
            .iter()
            .filter(|a| BrokerAccountScope::from_account(a).ok().as_ref() == Some(scope))
            .map(|a| a.id)
            .collect();
        let ledger = self
            .factory
            .statement_read()
            .ok_or("Statement evidence unavailable")?
            .read_statement_ledger(&accounts)?;
        reconcile(statement, &ledger)
    }
}

fn add<K: Ord>(map: &mut BTreeMap<K, Decimal>, key: K, amount: Decimal) -> Fallible<()> {
    let previous = map.entry(key).or_insert(Decimal::ZERO);
    *previous = previous
        .checked_add(amount)
        .ok_or("Statement arithmetic overflow")?;
    Ok(())
}
fn negative(amount: Decimal) -> Fallible<Decimal> {
    Ok(Decimal::ZERO
        .checked_sub(amount)
        .ok_or("Statement arithmetic overflow")?)
}
fn difference(a: Decimal, b: Decimal) -> Fallible<Decimal> {
    Ok(a.checked_sub(b)
        .ok_or("Statement arithmetic overflow")?
        .abs())
}
fn within(time: DateTime<Utc>, statement: &BrokerStatement) -> bool {
    time >= statement.start && time < statement.end_exclusive
}
fn issue(
    report: &mut StatementReconciliation,
    code: &str,
    reference: Option<String>,
    currency: Option<String>,
    broker: Option<Decimal>,
    trust: Option<Decimal>,
) {
    report.issue_count = report.issue_count.saturating_add(1);
    if report.issues.len() < 200 {
        report.issues.push(StatementIssue {
            code: code.into(),
            reference,
            currency,
            broker_amount: broker,
            trust_amount: trust,
        });
    }
}

fn validate_ledger(ledger: &StatementLedger) -> Fallible<BTreeMap<Uuid, String>> {
    validate_trade_fills(ledger)?;
    let mut executions = BTreeSet::new();
    let mut trades = BTreeMap::new();
    let mut trade_accounts = BTreeMap::new();
    for row in &ledger.executions {
        let e = &row.execution;
        if !executions.insert(&e.execution_id)
            || e.execution_id.is_empty()
            || e.quantity <= Decimal::ZERO
            || e.price <= Decimal::ZERO
            || !matches!(e.currency.as_str(), "USD" | "EUR")
        {
            return Err("Invalid or duplicate ledger execution evidence".into());
        }
        if trades
            .insert(row.trade_id, e.currency.clone())
            .is_some_and(|c| c != e.currency)
            || trade_accounts
                .insert(row.trade_id, row.account_id)
                .is_some_and(|a| a != row.account_id)
        {
            return Err("Inconsistent execution trade identity".into());
        }
    }
    let mut activity_ids = BTreeSet::new();
    let mut actual_fees = BTreeMap::new();
    for fee in &ledger.fees {
        let trade = fee.trade_id.ok_or("Unallocated fee activity")?;
        if fee.amount < Decimal::ZERO
            || !activity_ids.insert(&fee.activity_id)
            || trade_accounts.get(&trade) != Some(&fee.account_id)
        {
            return Err("Invalid fee activity identity".into());
        }
        add(&mut actual_fees, trade, fee.amount)?;
    }
    let mut booked_fees = BTreeMap::new();
    for movement in &ledger.movements {
        if movement.amount < Decimal::ZERO || !matches!(movement.currency.as_str(), "USD" | "EUR") {
            return Err("Invalid cash receipt".into());
        }
        if matches!(movement.category.as_str(), "fee_open" | "fee_close") {
            let trade = movement.trade_id.ok_or("Unlinked booked fee")?;
            if trade_accounts.get(&trade) != Some(&movement.account_id)
                || trades.get(&trade) != Some(&movement.currency)
            {
                return Err("Invalid booked fee attribution".into());
            }
            add(&mut booked_fees, trade, movement.amount)?;
        }
    }
    actual_fees.retain(|_, amount| !amount.is_zero());
    booked_fees.retain(|_, amount| !amount.is_zero());
    if actual_fees != booked_fees {
        return Err("Booked fees do not match allocated broker fee activities".into());
    }
    Ok(trades)
}

fn validate_trade_fills(ledger: &StatementLedger) -> Fallible<()> {
    let mut trades = BTreeMap::new();
    for t in &ledger.trades {
        if t.entry_filled < Decimal::ZERO
            || t.exit_filled < Decimal::ZERO
            || t.exit_filled > t.entry_filled
            || trades.insert(t.trade_id, t).is_some()
        {
            return Err("Invalid recorded trade quantities".into());
        }
    }
    let mut entries = BTreeMap::new();
    let mut exits = BTreeMap::new();
    for e in &ledger.executions {
        let t = trades
            .get(&e.trade_id)
            .filter(|t| t.account_id == e.account_id)
            .ok_or("Execution without matching trade")?;
        if e.execution.buy == t.entry_buy {
            add(&mut entries, t.trade_id, e.execution.quantity)?;
        } else {
            add(&mut exits, t.trade_id, e.execution.quantity)?;
        }
    }
    for t in trades.values() {
        if entries.get(&t.trade_id).copied().unwrap_or_default() != t.entry_filled
            || exits.get(&t.trade_id).copied().unwrap_or_default() != t.exit_filled
        {
            return Err("Recorded trade quantities do not match executions".into());
        }
    }
    Ok(())
}

fn balances_at(
    ledger: &StatementLedger,
    currencies: &BTreeMap<Uuid, String>,
    cutoff: DateTime<Utc>,
) -> Fallible<(Amounts, Positions)> {
    let mut cash = Amounts::new();
    let mut positions = Positions::new();
    for movement in ledger.movements.iter().filter(|m| m.occurred_at < cutoff) {
        let amount = match movement.category.as_str() {
            "deposit" => movement.amount,
            "withdrawal" | "withdrawal_earnings" | "withdrawal_tax" => negative(movement.amount)?,
            "fee_open" | "fee_close" => continue, // Use verified fee occurrence dates, including late bookings.
            _ => return Err("Unsupported cash movement".into()),
        };
        add(&mut cash, movement.currency.clone(), amount)?;
    }
    for row in ledger
        .executions
        .iter()
        .filter(|r| r.execution.occurred_at < cutoff)
    {
        let e = &row.execution;
        let notional = e
            .price
            .checked_mul(e.quantity)
            .ok_or("Execution amount overflow")?;
        add(
            &mut cash,
            e.currency.clone(),
            if e.buy { negative(notional)? } else { notional },
        )?;
        add(
            &mut positions,
            (e.instrument_id.clone(), e.currency.clone()),
            if e.buy {
                e.quantity
            } else {
                negative(e.quantity)?
            },
        )?;
    }
    for fee in ledger.fees.iter().filter(|f| f.occurred_at < cutoff) {
        let currency = fee
            .trade_id
            .and_then(|t| currencies.get(&t))
            .ok_or("Unlinked fee currency")?;
        add(&mut cash, currency.clone(), negative(fee.amount)?)?;
    }
    positions.retain(|_, quantity| !quantity.is_zero());
    Ok((cash, positions))
}

fn executions(
    statement: &BrokerStatement,
    ledger: &StatementLedger,
    report: &mut StatementReconciliation,
) -> Fallible<()> {
    let actual: BTreeMap<_, _> = ledger
        .executions
        .iter()
        .filter(|r| within(r.execution.occurred_at, statement))
        .map(|r| (r.execution.execution_id.as_str(), r))
        .collect();
    let mut seen = BTreeSet::new();
    let mut expected_fees = BTreeMap::new();
    for e in &statement.executions {
        if !seen.insert(e.execution_id.as_str())
            || !within(e.occurred_at, statement)
            || e.quantity <= Decimal::ZERO
            || e.price <= Decimal::ZERO
            || e.commission < Decimal::ZERO
        {
            return Err("Invalid normalized statement execution".into());
        }
        let Some(row) = actual.get(e.execution_id.as_str()) else {
            issue(
                report,
                "execution_missing_in_trust",
                Some(e.execution_id.clone()),
                Some(e.currency.clone()),
                Some(e.quantity),
                None,
            );
            continue;
        };
        let a = &row.execution;
        if a.instrument_id != e.instrument_id
            || a.currency != e.currency
            || a.buy != e.buy
            || a.quantity != e.quantity
            || a.price != e.price
            || a.occurred_at.timestamp() != e.occurred_at.timestamp()
        {
            issue(
                report,
                "execution_mismatch",
                Some(e.execution_id.clone()),
                Some(e.currency.clone()),
                Some(e.quantity),
                Some(a.quantity),
            );
        }
        add(&mut expected_fees, row.trade_id, e.commission)?;
    }
    for (id, row) in actual.iter().filter(|(id, _)| !seen.contains(*id)) {
        issue(
            report,
            "execution_missing_in_statement",
            Some((*id).into()),
            Some(row.execution.currency.clone()),
            None,
            Some(row.execution.quantity),
        );
    }
    fees(statement, ledger, report, expected_fees)
}

fn fees(
    statement: &BrokerStatement,
    ledger: &StatementLedger,
    report: &mut StatementReconciliation,
    expected_fees: BTreeMap<Uuid, Decimal>,
) -> Fallible<()> {
    let mut actual_fees = BTreeMap::new();
    for fee in ledger
        .fees
        .iter()
        .filter(|f| within(f.occurred_at, statement))
    {
        add(
            &mut actual_fees,
            fee.trade_id.ok_or("Unallocated fee")?,
            fee.amount,
        )?;
    }
    for trade in expected_fees
        .keys()
        .chain(actual_fees.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let expected = expected_fees.get(&trade).copied().unwrap_or(Decimal::ZERO);
        let actual = actual_fees.get(&trade).copied().unwrap_or(Decimal::ZERO);
        if difference(expected, actual)? > report.cash_tolerance {
            issue(
                report,
                "commission_mismatch",
                Some(trade.to_string()),
                None,
                Some(expected),
                Some(actual),
            );
        }
    }
    Ok(())
}

fn net_flows(ledger: &StatementLedger, statement: &BrokerStatement) -> Fallible<Amounts> {
    let mut flows = Amounts::new();
    for movement in ledger
        .movements
        .iter()
        .filter(|m| within(m.occurred_at, statement))
    {
        let amount = match movement.category.as_str() {
            "deposit" => movement.amount,
            "withdrawal" | "withdrawal_earnings" | "withdrawal_tax" => negative(movement.amount)?,
            "fee_open" | "fee_close" => continue,
            _ => return Err("Unsupported cash movement".into()),
        };
        add(&mut flows, movement.currency.clone(), amount)?;
    }
    Ok(flows)
}

fn cash(
    statement: &BrokerStatement,
    ledger: &StatementLedger,
    currencies: &BTreeMap<Uuid, String>,
    report: &mut StatementReconciliation,
) -> Fallible<()> {
    let (opening, _) = balances_at(ledger, currencies, statement.start)?;
    let (closing, positions) = balances_at(ledger, currencies, statement.end_exclusive)?;
    let flows = net_flows(ledger, statement)?;
    for currency in statement
        .cash
        .keys()
        .chain(opening.keys())
        .chain(closing.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let Some(broker) = statement.cash.get(&currency) else {
            if !opening
                .get(&currency)
                .copied()
                .unwrap_or_default()
                .is_zero()
                || !closing
                    .get(&currency)
                    .copied()
                    .unwrap_or_default()
                    .is_zero()
            {
                issue(
                    report,
                    "cash_currency_missing",
                    None,
                    Some(currency),
                    None,
                    None,
                );
            }
            continue;
        };
        record_cash(
            report,
            CashComparison {
                currency: currency.clone(),
                broker_opening: broker.opening,
                trust_opening: opening.get(&currency).copied().unwrap_or_default(),
                broker_closing: broker.closing,
                trust_closing: closing.get(&currency).copied().unwrap_or_default(),
                broker_net_deposits: broker.net_deposits,
                trust_net_deposits: flows.get(&currency).copied().unwrap_or_default(),
            },
        )?;
    }
    compare_positions(statement, positions, report)
}

fn record_cash(report: &mut StatementReconciliation, comparison: CashComparison) -> Fallible<()> {
    for (code, expected, actual) in [
        (
            "net_cash_flow_mismatch",
            comparison.broker_net_deposits,
            comparison.trust_net_deposits,
        ),
        (
            "opening_cash_mismatch",
            comparison.broker_opening,
            comparison.trust_opening,
        ),
        (
            "closing_cash_mismatch",
            comparison.broker_closing,
            comparison.trust_closing,
        ),
    ] {
        if difference(expected, actual)? > report.cash_tolerance {
            issue(
                report,
                code,
                None,
                Some(comparison.currency.clone()),
                Some(expected),
                Some(actual),
            );
        }
    }

    report.cash.push(comparison);
    Ok(())
}

fn compare_positions(
    statement: &BrokerStatement,
    positions: Positions,
    report: &mut StatementReconciliation,
) -> Fallible<()> {
    let expected: Positions = statement
        .positions
        .iter()
        .map(|p| ((p.instrument_id.clone(), p.currency.clone()), p.quantity))
        .collect();
    if expected.len() != statement.positions.len() {
        return Err("Duplicate statement position".into());
    }
    for key in expected
        .keys()
        .chain(positions.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let broker = expected.get(&key).copied().unwrap_or_default();
        let trust = positions.get(&key).copied().unwrap_or_default();
        if broker != trust {
            issue(
                report,
                "position_mismatch",
                Some(key.0),
                Some(key.1),
                Some(broker),
                Some(trust),
            );
        }
    }
    Ok(())
}

/// Pure deterministic comparison. Exact quantities/prices; one cent cash/fee tolerance.
pub fn reconcile(
    statement: &BrokerStatement,
    ledger: &StatementLedger,
) -> Fallible<StatementReconciliation> {
    if statement.start >= statement.end_exclusive
        || statement.cash.is_empty()
        || statement.end_exclusive > statement.generated_at
        || statement.generated_at > Utc::now()
        || statement
            .cash
            .keys()
            .any(|c| !matches!(c.as_str(), "USD" | "EUR"))
    {
        return Err("Invalid statement period or cash coverage".into());
    }
    let currencies = validate_ledger(ledger)?;
    let mut report = StatementReconciliation {
        status: ReconciliationStatus::Matched,
        checked_at: Utc::now(),
        execution_count: statement.executions.len(),
        position_count: statement.positions.len(),
        issue_count: 0,
        issues: Vec::new(),
        cash: Vec::new(),
        cash_tolerance: Decimal::new(1, 2),
        permits_trading: false,
    };
    for blocker in &statement.blockers {
        issue(&mut report, blocker, None, None, None, None);
    }
    executions(statement, ledger, &mut report)?;
    cash(statement, ledger, &currencies, &mut report)?;
    if !statement.blockers.is_empty() {
        report.status = ReconciliationStatus::Inconclusive;
    } else if report.issue_count > 0 {
        report.status = ReconciliationStatus::Discrepancies;
    }
    Ok(report)
}
