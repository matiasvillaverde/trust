use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, Trade, TransactionCategory};
use uuid::Uuid;

pub struct TransactionsCalculator;

impl TransactionsCalculator {
    pub fn capital_available(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_account_transactions_excluding_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::Withdrawal => {
                    -transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) | TransactionCategory::Deposit => {
                    transaction.price.amount
                }
                default => panic!(
                    "capital_available: Unexpected transaction category: {}",
                    default
                ),
            })
            .sum();

        Ok(total_available)
    }

    pub fn total_balance(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_balance = dec!(0.0);
        // Get all transactions
        for tx in database.all_transactions(account_id, currency)? {
            match tx.category {
                TransactionCategory::Withdrawal
                | TransactionCategory::WithdrawalTax
                | TransactionCategory::WithdrawalEarnings => {
                    total_balance -= tx.price.amount;
                }
                TransactionCategory::Deposit => {
                    total_balance += tx.price.amount;
                }
                TransactionCategory::OpenTrade(_) => {
                    total_balance -= tx.price.amount; // The money is in the market it counts at negative.
                }
                TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    total_balance += tx.price.amount; // We add the money that we get by exit the market.
                }
                _ => { // We don't want to count the transactions for taxes, earnings and funding trades.
                }
            }
        }

        Ok(total_balance)
    }

    pub fn capital_in_trades(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_account_transactions_excluding_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) => transaction.price.amount,
                TransactionCategory::PaymentFromTrade(_) => -transaction.price.amount,
                _ => dec!(0),
            })
            .sum();

        Ok(total_available)
    }

    pub fn capital_taxable(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions = database.read_all_account_transactions_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::PaymentTax(_) => transaction.price.amount,
                TransactionCategory::WithdrawalTax => -transaction.price.amount,
                default => panic!(
                    "capital_taxable: Unexpected transaction category: {}",
                    default
                ),
            })
            .sum();

        Ok(total_available)
    }

    pub fn capital_in_trades_not_at_risk(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database.all_open_trades_for_currency(account_id, currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_risk;
        }
        Ok(total_capital_not_at_risk)
    }

    pub fn capital_at_beginning_of_month(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate all the transactions at the beginning of the month
        let mut total_beginning_of_month = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, currency)?
        {
            match transaction.category {
                TransactionCategory::FundTrade(_) => {
                    total_beginning_of_month -= transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Deposit => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Withdrawal => {
                    total_beginning_of_month -= transaction.price.amount
                }
                default => panic!(
                    "capital_at_beginning_of_month: Unexpected transaction category: {}",
                    default
                ),
            }
        }
        Ok(total_beginning_of_month)
    }

    // Trade transactions

    pub fn capital_out_of_market(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have put into the trade
                    total_trade += tx.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
                    // This is money that we have extracted from the trade
                    total_trade -= tx.price.amount
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade -= tx.price.amount
                }
                TransactionCategory::CloseTarget(_) => {
                    // This is money that we have used to exit the market.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseSafetyStop(_) => {
                    // This is money that we have used to exit the market at a loss.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseSafetyStopSlippage(_) => {
                    // This is money that we have used to exit the market at a loss - slippage.
                    total_trade += tx.price.amount
                }
                default => panic!(
                    "capital_out_of_market: Unexpected transaction category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn capital_in_market(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::PaymentFromTrade(_) => {
                    // Nothing
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    total_trade = Decimal::from(0) // We have exited the market, so we have no money in the market.
                }
                default => panic!(
                    "capital_in_market: Unexpected transaction category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn funding(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_funding_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                default => panic!("funding: Unexpected transaction category: {}", default),
            }
        }

        Ok(total_trade)
    }

    pub fn taxes(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_taxes_transactions(trade)? {
            match tx.category {
                TransactionCategory::PaymentTax(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                default => panic!("taxes: Unexpected transaction category: {}", default),
            }
        }

        Ok(total_trade)
    }
}
