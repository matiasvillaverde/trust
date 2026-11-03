use crate::{Account, Currency, Transaction, TransactionCategory};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

/// Trait for reading transaction data from the database
pub trait ReadTransactionDB {
    /// Replay exact available cash in ledger order. An implementation may reuse a
    /// verified prefix or atomically fold new receipts into it only while all
    /// historical writes invalidate that prefix, including rollback, replacement
    /// and writes from another connection. Saved values never authorize a trade;
    /// callers still evaluate current policy and reservations.
    fn verified_cash_balance(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Decimal, Box<dyn Error>> {
        self.account_cash_movements_excluding_taxes(account_id, currency)?
            .into_iter()
            .try_fold(Decimal::ZERO, |total, (category, amount)| {
                category
                    .apply_to_available_cash(total, amount)
                    .map_err(Into::into)
            })
    }

    /// Page cash movements (excluding taxes), in ascending (created_at, id) order.
    fn history_page(
        &mut self,
        _account_id: Uuid,
        _filter: &crate::history::TransactionHistoryFilter,
        _page: crate::history::PageRequest,
    ) -> Result<crate::history::HistoryPage<Transaction>, Box<dyn Error>> {
        Err("Bounded transaction history is not supported by this database".into())
    }

    /// Live cost-basis exposure when cumulative fill quantities are available.
    fn trade_market_exposure(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Option<Decimal>, Box<dyn Error>> {
        Ok(None)
    }
    /// Account exposure including partially filled and partially canceled positions.
    fn account_market_exposure(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Option<Decimal>, Box<dyn Error>> {
        Ok(None)
    }

    /// Retrieves all account transactions excluding tax transactions
    /// in the same order, without report-only transaction metadata.
    fn account_cash_movements_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<(TransactionCategory, Decimal)>, Box<dyn Error>> {
        Ok(self
            .all_account_transactions_excluding_taxes(account_id, currency)?
            .into_iter()
            .map(|tx| (tx.category, tx.amount))
            .collect())
    }

    /// Retrieves all account transactions excluding tax transactions
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all account transactions that are funding submitted trades
    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax-related transactions for an account
    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions associated with a specific trade
    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all funding transactions for a specific trade
    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax transactions for a specific trade
    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions excluding current month and tax transactions
    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Inputs that can adjust the monthly reference: earlier funding/refund rows,
    /// current capital flows, and current closing receipts for earlier reservations.
    /// Readers may return a superset; core owns the accounting and carried-state rules.
    /// The default preserves compatibility with readers that only expose full history.
    fn monthly_reference_inputs(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        self.all_transactions(account_id, currency)
    }

    /// Retrieves all transactions for an account in a specific currency
    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;
}

/// Trait for writing transaction data to the database
pub trait WriteTransactionDB {
    /// Creates a new transaction for an account id with the specified parameters.
    fn create_transaction_by_account_id(
        &mut self,
        account_id: Uuid,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>>;

    /// Creates a new transaction with the specified parameters
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        self.create_transaction_by_account_id(account.id, amount, currency, category)
    }

    /// Creates a paired transfer (withdrawal + deposit).
    /// Implementations should provide atomicity when possible.
    fn create_transfer_pair(
        &mut self,
        from_account: &Account,
        to_account: &Account,
        amount: Decimal,
        currency: &Currency,
        withdrawal_category: TransactionCategory,
        deposit_category: TransactionCategory,
    ) -> Result<(Transaction, Transaction), Box<dyn Error>> {
        let withdrawal_amount = Decimal::ZERO
            .checked_sub(amount)
            .ok_or("Invalid withdrawal amount")?;
        let withdrawal_tx = self.create_transaction(
            from_account,
            withdrawal_amount,
            currency,
            withdrawal_category,
        )?;
        let deposit_tx = self.create_transaction(to_account, amount, currency, deposit_category)?;
        Ok((withdrawal_tx, deposit_tx))
    }
}
