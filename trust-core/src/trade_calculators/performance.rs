use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{ReadTransactionDB, Trade, TransactionCategory};

pub struct TradePerformance;

impl TradePerformance {
    pub fn calculate(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut performance = dec!(0);

        for tx in database.all_trade_transactions(trade.id)? {
            match tx.category {
                TransactionCategory::OpenTrade(_)
                | TransactionCategory::FeeClose(_)
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::PaymentTax(_) => performance -= tx.price.amount,

                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => performance += tx.price.amount,
                _ => {} // We don't want to count the transactions paid out of the trade or fund the trade.
            }
        }

        Ok(performance)
    }
}
