use super::*;

fn leaves<'a, 'input>(parent: Node<'a, 'input>, name: &str) -> Result<Vec<Node<'a, 'input>>> {
    let rows: Vec<_> = parent.children().filter(Node::is_element).collect();
    if rows.len() > 2_000
        || rows
            .iter()
            .any(|n| !n.has_tag_name(name) || n.children().any(|c| c.is_element()))
    {
        return Err(error());
    }
    Ok(rows)
}
fn currency(node: Node<'_, '_>) -> Result<String> {
    let value = attr(node, "currency")?;
    if !matches!(value, "USD" | "EUR") {
        return Err(StatementParseError("unsupported_statement_currency"));
    }
    Ok(value.into())
}
fn stock(node: Node<'_, '_>, output: &mut BrokerStatement) -> Result<()> {
    if attr(node, "assetCategory")? != "STK" || decimal(node, "multiplier")? != Decimal::ONE {
        output.blockers.insert("unsupported_statement_asset".into());
    }
    Ok(())
}
fn in_period(time: DateTime<Utc>, output: &BrokerStatement) -> Result<()> {
    if time < output.start || time >= output.end_exclusive {
        return Err(StatementParseError("statement_row_outside_period"));
    }
    Ok(())
}
fn total(mut values: impl Iterator<Item = Decimal>) -> Result<Decimal> {
    values.try_fold(Decimal::ZERO, |sum, v| sum.checked_add(v).ok_or_else(error))
}
fn neg(value: Decimal) -> Result<Decimal> {
    Decimal::ZERO.checked_sub(value).ok_or_else(error)
}
fn differs(a: Decimal, b: Decimal) -> Result<bool> {
    Ok(a.checked_sub(b).ok_or_else(error)?.abs() > Decimal::new(1, 2))
}

fn cash(output: &mut BrokerStatement, parent: Node<'_, '_>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for row in leaves(parent, "CashReportCurrency")? {
        let code = attr(row, "currency")?;
        if !seen.insert(code) {
            return Err(StatementParseError("duplicate_statement_currency"));
        }
        if code == "BASE_SUMMARY" {
            continue;
        }
        let code = currency(row)?;
        if date(attr(row, "fromDate")?)? != output.from_date
            || date(attr(row, "toDate")?)? != output.to_date
        {
            return Err(error());
        }
        let commissions = decimal(row, "commissions")?;
        if commissions > Decimal::ZERO {
            output
                .blockers
                .insert("unsupported_commission_rebate".into());
        }
        for attribute in row.attributes() {
            let name = attribute.name();
            if matches!(
                name,
                "accountId"
                    | "acctAlias"
                    | "model"
                    | "currency"
                    | "fromDate"
                    | "toDate"
                    | "levelOfDetail"
            ) || name.ends_with("MTD")
                || name.ends_with("YTD")
            {
                continue;
            }
            let base = name.strip_suffix("Sec").unwrap_or(name);
            if matches!(
                base,
                "startingCash"
                    | "endingCash"
                    | "endingSettledCash"
                    | "commissions"
                    | "depositWithdrawals"
                    | "deposits"
                    | "withdrawals"
                    | "netTradesSales"
                    | "netTradesPurchases"
            ) {
                continue;
            }
            if !attribute.value().is_empty() && !decimal(row, name)?.is_zero() {
                output.blockers.insert("unsupported_cash_component".into());
            }
        }
        output.cash.insert(
            code,
            StatementCash {
                opening: decimal(row, "startingCash")?,
                closing: decimal(row, "endingCash")?,
                commissions: neg(commissions)?,
                net_deposits: decimal(row, "depositWithdrawals")?,
            },
        );
    }
    if output.cash.is_empty() {
        return Err(StatementParseError("native_currency_cash_required"));
    }
    Ok(())
}

fn executions(output: &mut BrokerStatement, parent: Node<'_, '_>, tz: Tz) -> Result<()> {
    let mut ids = BTreeSet::new();
    for row in leaves(parent, "Trade")? {
        if attr(row, "levelOfDetail")? != "EXECUTION" {
            return Err(StatementParseError("execution_detail_required"));
        }
        stock(row, output)?;
        if attr(row, "transactionType")? != "ExchTrade"
            || row
                .attribute("origTradeID")
                .is_some_and(|s| !s.is_empty() && s != "0")
        {
            output
                .blockers
                .insert("unsupported_trade_correction_or_type".into());
        }
        let execution_id = attr(row, "ibExecID")?.to_owned();
        if !ids.insert(execution_id.clone()) {
            return Err(StatementParseError("duplicate_statement_execution"));
        }
        let currency = currency(row)?;
        if attr(row, "ibCommissionCurrency")? != currency {
            output.blockers.insert("cross_currency_commission".into());
        }
        let signed = decimal(row, "quantity")?;
        let buy = match attr(row, "buySell")? {
            "BUY" if signed > Decimal::ZERO => true,
            "SELL" if signed < Decimal::ZERO => false,
            _ => return Err(error()),
        };
        let price = decimal(row, "tradePrice")?;
        if price <= Decimal::ZERO {
            return Err(error());
        }
        let commission = decimal(row, "ibCommission")?;
        if commission > Decimal::ZERO || !decimal(row, "taxes")?.is_zero() {
            output
                .blockers
                .insert("unsupported_trade_tax_or_rebate".into());
        }
        let occurred_at = time(attr(row, "dateTime")?, tz)?;
        in_period(occurred_at, output)?;
        let proceeds = neg(signed.checked_mul(price).ok_or_else(error)?)?;
        if differs(proceeds, decimal(row, "proceeds")?)? {
            output
                .blockers
                .insert("inconsistent_execution_proceeds".into());
        }
        output.executions.push(StatementExecution {
            execution_id,
            instrument_id: attr(row, "conid")?.into(),
            symbol: attr(row, "symbol")?.into(),
            currency,
            buy,
            quantity: signed.abs(),
            price,
            occurred_at,
            commission: neg(commission)?,
        });
    }
    Ok(())
}

fn positions(output: &mut BrokerStatement, parent: Node<'_, '_>) -> Result<()> {
    let mut ids = BTreeSet::new();
    for row in leaves(parent, "OpenPosition")? {
        if attr(row, "levelOfDetail")? != "SUMMARY" {
            return Err(StatementParseError("position_summary_required"));
        }
        if date(attr(row, "reportDate")?)? != output.to_date {
            return Err(error());
        }
        stock(row, output)?;
        let currency = currency(row)?;
        let instrument_id = attr(row, "conid")?.to_owned();
        if !ids.insert((instrument_id.clone(), currency.clone())) {
            return Err(StatementParseError("duplicate_statement_position"));
        }
        let quantity = decimal(row, "position")?;
        if !quantity.is_zero() {
            output.positions.push(StatementPosition {
                instrument_id,
                currency,
                quantity,
            });
        }
    }
    Ok(())
}

fn cash_flows(output: &mut BrokerStatement, parent: Node<'_, '_>, tz: Tz) -> Result<()> {
    let mut ids = BTreeSet::new();
    for row in leaves(parent, "CashTransaction")? {
        let reference = attr(row, "transactionID")?.to_owned();
        if !ids.insert(reference.clone()) {
            return Err(StatementParseError("duplicate_statement_cash_flow"));
        }
        let currency = currency(row)?;
        let occurred_at = time(attr(row, "dateTime")?, tz)?;
        in_period(occurred_at, output)?;
        if attr(row, "type")? != "Deposits/Withdrawals" {
            output
                .blockers
                .insert("unsupported_cash_transaction".into());
        }
        output.cash_flows.push(StatementCashFlow {
            reference,
            currency,
            amount: decimal(row, "amount")?,
            occurred_at,
        });
    }
    Ok(())
}

fn verify_summary(output: &mut BrokerStatement) -> Result<()> {
    for currency in output
        .executions
        .iter()
        .map(|e| &e.currency)
        .chain(output.positions.iter().map(|p| &p.currency))
        .chain(output.cash_flows.iter().map(|c| &c.currency))
    {
        if !output.cash.contains_key(currency) {
            return Err(StatementParseError("native_currency_cash_required"));
        }
    }
    for (currency, cash) in &output.cash {
        let commissions = total(
            output
                .executions
                .iter()
                .filter(|e| &e.currency == currency)
                .map(|e| e.commission),
        )?;
        let flows = total(
            output
                .cash_flows
                .iter()
                .filter(|e| &e.currency == currency)
                .map(|e| e.amount),
        )?;
        let mut closing = cash
            .opening
            .checked_add(flows)
            .and_then(|v| v.checked_sub(commissions))
            .ok_or_else(error)?;
        for execution in output.executions.iter().filter(|e| &e.currency == currency) {
            let amount = execution
                .quantity
                .checked_mul(execution.price)
                .ok_or_else(error)?;
            closing = closing
                .checked_add(if execution.buy { neg(amount)? } else { amount })
                .ok_or_else(error)?;
        }
        if differs(commissions, cash.commissions)?
            || differs(flows, cash.net_deposits)?
            || differs(closing, cash.closing)?
        {
            output
                .blockers
                .insert("statement_cash_does_not_balance".into());
        }
    }
    Ok(())
}

pub(super) fn parse(output: &mut BrokerStatement, node: Node<'_, '_>, tz: Tz) -> Result<()> {
    cash(output, section(node, "CashReport")?)?;
    executions(output, section(node, "Trades")?, tz)?;
    positions(output, section(node, "OpenPositions")?)?;
    cash_flows(output, section(node, "CashTransactions")?, tz)?;
    verify_summary(output)
}
