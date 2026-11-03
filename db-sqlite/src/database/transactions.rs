use super::{lock_connection_or_exit, market_exposure, SqliteDatabase};
use crate::workers::WorkerTransaction;
use model::{
    Currency, DatabaseFactory, ReadTransactionDB, Status, Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

impl ReadTransactionDB for SqliteDatabase {
    fn verified_cash_balance(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Decimal, Box<dyn Error>> {
        crate::workers::worker_transaction::cash_checkpoint::calculate(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn history_page(
        &mut self,
        account_id: Uuid,
        filter: &model::history::TransactionHistoryFilter,
        page: model::history::PageRequest,
    ) -> Result<model::history::HistoryPage<Transaction>, Box<dyn Error>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Transaction history database lock unavailable")?;
        crate::history::transactions(&mut connection, account_id, filter, page)
    }

    fn trade_market_exposure(&mut self, trade_id: Uuid) -> Result<Option<Decimal>, Box<dyn Error>> {
        let trade = self.trade_read().read_trade(trade_id)?;
        market_exposure(&trade)
    }

    fn account_market_exposure(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Option<Decimal>, Box<dyn Error>> {
        let mut total = Decimal::ZERO;
        for status in [
            Status::Funded,
            Status::Submitted,
            Status::PartiallyFilled,
            Status::Filled,
            Status::Canceled,
            Status::Expired,
            Status::Rejected,
        ] {
            for trade in self
                .trade_read()
                .read_trades_with_status(account_id, status)?
            {
                if trade.currency != *currency {
                    continue;
                }
                let transactions = self.transaction_read().all_trade_transactions(trade.id)?;
                let amount = if status == Status::Funded {
                    transactions
                        .iter()
                        .filter(|tx| matches!(tx.category, TransactionCategory::FundTrade(_)))
                        .try_fold(Decimal::ZERO, |total, tx| {
                            total.checked_add(tx.amount).ok_or("Funding overflow")
                        })?
                } else if let Some(exposure) = market_exposure(&trade)? {
                    exposure
                } else {
                    transactions
                        .iter()
                        .try_fold(Decimal::ZERO, |total, tx| match tx.category {
                            TransactionCategory::OpenTrade(_) => total
                                .checked_add(tx.amount.abs())
                                .ok_or("Exposure overflow"),
                            TransactionCategory::CloseTarget(_)
                            | TransactionCategory::CloseSafetyStop(_)
                            | TransactionCategory::CloseSafetyStopSlippage(_) => Ok(Decimal::ZERO),
                            _ => Ok(total),
                        })?
                };
                total = total
                    .checked_add(amount)
                    .ok_or("Account exposure overflow")?;
            }
        }
        Ok(Some(total))
    }

    fn account_cash_movements_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<(TransactionCategory, Decimal)>, Box<dyn Error>> {
        WorkerTransaction::read_cash_movements(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_excluding_taxes(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::all_account_transactions_in_trade(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_account_transactions_taxes(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
        )
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
            TransactionCategory::FundTrade(trade_id),
        )
    }

    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
            TransactionCategory::PaymentTax(trade_id),
        )
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_transaction_excluding_current_month_and_taxes(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn monthly_reference_inputs(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_monthly_reference_inputs(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
            cutoff,
        )
    }

    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_transactions(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }
}
