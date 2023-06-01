use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, DatabaseFactory, ReadTransactionDB, Trade, TransactionCategory};
use uuid::Uuid;

pub struct TradeTransactionsCalculator;

impl TradeTransactionsCalculator {
    pub fn capital_in_trades_not_at_risk(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database
            .read_trade_db()
            .all_open_trades_for_currency(account_id, currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_risk;
        }
        Ok(total_capital_not_at_risk)
    }
}

#[cfg(test)]
mod tests {}
