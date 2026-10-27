//! Cumulative broker fills are authoritative; book only cash-flow differences.
use model::{
    DatabaseFactory, Order, OrderStatus, Status, Trade, TradeCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use std::error::Error;

fn filled_quantity(order: &Order) -> Decimal {
    if order.status == OrderStatus::Filled && order.filled_quantity.is_zero() {
        order.quantity
    } else {
        order.filled_quantity
    }
}

fn cumulative_cash(order: &Order) -> Result<Decimal, Box<dyn Error>> {
    let qty = filled_quantity(order);
    if qty < Decimal::ZERO || qty > order.quantity {
        return Err("Broker cumulative fill quantity is outside order bounds".into());
    }
    if qty.is_zero() {
        return Ok(Decimal::ZERO);
    }
    let price = order
        .average_filled_price
        .ok_or("Broker fills require an average price")?;
    if price <= Decimal::ZERO {
        return Err("Broker average fill price must be positive".into());
    }
    qty.checked_mul(price)
        .ok_or_else(|| "Cumulative fill cash overflow".into())
}

// Keep the financial operation and its validation in one reviewable sequence.
#[allow(clippy::too_many_lines)]
pub(crate) fn reconcile(
    database: &mut dyn DatabaseFactory,
    snapshot: &Trade,
    reported: Status,
) -> Result<Trade, Box<dyn Error>> {
    let mut trade = snapshot.clone();
    for order in [&mut trade.entry, &mut trade.target, &mut trade.safety_stop] {
        order.filled_quantity = filled_quantity(order);
    }
    let exited = trade
        .target
        .filled_quantity
        .checked_add(trade.safety_stop.filled_quantity)
        .ok_or("Cumulative exit quantity overflow")?;
    let remaining = trade
        .entry
        .filled_quantity
        .checked_sub(exited)
        .ok_or("Remaining quantity overflow")?;
    if remaining < Decimal::ZERO {
        return Err("Broker exit fills exceed entry fills".into());
    }
    let terminal_entry = matches!(
        reported,
        Status::Canceled | Status::Expired | Status::Rejected
    );
    let status = if remaining > Decimal::ZERO && (terminal_entry || exited > Decimal::ZERO) {
        Status::PartiallyFilled
    } else {
        reported
    };
    if matches!(status, Status::ClosedTarget | Status::ClosedStopLoss) && remaining != Decimal::ZERO
    {
        return Err("Closed broker trade still has open exposure".into());
    }
    if trade.category == TradeCategory::Long
        && cumulative_cash(&trade.entry)? > trade.balance.funding
    {
        return Err(
            "Reported entry cost exceeds reserved funding; reconcile the broker discrepancy".into(),
        );
    }
    let transactions = database
        .transaction_read()
        .all_trade_transactions(trade.id)?;
    // The enclosing sync savepoint reloads the trade and serializes all ledger/projection
    // writes. Apply only the new cash legs, just like deposit/fund/manual execution.
    let previous_exposure = if snapshot.status == Status::Funded {
        snapshot.balance.funding
    } else {
        snapshot.balance.capital_in_market
    };
    let mut updates = Vec::new();
    let adverse_stop =
        trade
            .safety_stop
            .average_filled_price
            .is_some_and(|price| match trade.category {
                TradeCategory::Long => price < trade.safety_stop.unit_price,
                TradeCategory::Short => price > trade.safety_stop.unit_price,
            });
    let stop_category = if adverse_stop {
        TransactionCategory::CloseSafetyStopSlippage(trade.id)
    } else {
        TransactionCategory::CloseSafetyStop(trade.id)
    };
    for (order, category) in [
        (&trade.entry, TransactionCategory::OpenTrade(trade.id)),
        (&trade.target, TransactionCategory::CloseTarget(trade.id)),
        (&trade.safety_stop, stop_category),
    ] {
        let cash = cumulative_cash(order)?;
        let signed = if trade.category == TradeCategory::Short {
            Decimal::ZERO
                .checked_sub(cash)
                .ok_or("Short cash overflow")?
        } else {
            cash
        };
        let booked = transactions
            .iter()
            .filter(|tx| {
                tx.category == category
                    || (matches!(
                        category,
                        TransactionCategory::CloseSafetyStop(_)
                            | TransactionCategory::CloseSafetyStopSlippage(_)
                    ) && matches!(
                        tx.category,
                        TransactionCategory::CloseSafetyStop(_)
                            | TransactionCategory::CloseSafetyStopSlippage(_)
                    ))
            })
            .try_fold(Decimal::ZERO, |total, tx| {
                total.checked_add(tx.amount).ok_or("Booked cash overflow")
            })?;
        let delta = signed.checked_sub(booked).ok_or("Fill delta overflow")?;
        if !delta.is_zero() {
            database
                .transaction_write()
                .create_transaction_by_account_id(
                    trade.account_id,
                    delta,
                    &trade.currency,
                    category,
                )?;
            updates.push((category, delta));
        }
        database.order_write().update(order)?;
    }
    trade = database.trade_write().update_trade_status(status, &trade)?;
    trade.balance = super::balance::calculate_trade(database, &trade)?;
    // Long inventory needs no additional collateral after entry completion. Shorts
    // retain their collateral and proceeds until the entire position is closed.
    let release_cash = (remaining.is_zero() && (terminal_entry || exited > Decimal::ZERO))
        || (trade.category == TradeCategory::Long
            && (trade.entry.status == OrderStatus::Filled || terminal_entry));
    if release_cash && !trade.balance.capital_out_market.is_zero() {
        let payment = trade.balance.capital_out_market;
        database
            .transaction_write()
            .create_transaction_by_account_id(
                trade.account_id,
                trade.balance.capital_out_market,
                &trade.currency,
                TransactionCategory::PaymentFromTrade(trade.id),
            )?;
        updates.push((TransactionCategory::PaymentFromTrade(trade.id), payment));
        trade.balance = super::balance::calculate_trade(database, &trade)?;
    }
    let next_exposure = if trade.status == Status::Funded {
        trade.balance.funding
    } else {
        trade.balance.capital_in_market
    };
    let exposure_delta = next_exposure
        .checked_sub(previous_exposure)
        .ok_or("Account exposure delta overflow")?;
    super::balance::apply_account_projection_batch_by_id(
        database,
        trade.account_id,
        &trade.currency,
        &updates,
        exposure_delta,
    )?;
    Ok(trade)
}
