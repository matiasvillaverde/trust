//! Exact, ordered cash-ledger arithmetic shared by replay and verified appends.
use super::TransactionCategory;
use rust_decimal::Decimal;

/// A cash ledger cannot silently overflow or include non-cash receipts.
#[derive(Debug)]
pub enum CashLedgerError {
    /// A debit exceeds the decimal representation.
    Subtraction {
        /// Running balance before the receipt.
        balance: Decimal,
        /// Receipt amount.
        amount: Decimal,
    },
    /// A credit exceeds the decimal representation.
    Addition {
        /// Running balance before the receipt.
        balance: Decimal,
        /// Receipt amount.
        amount: Decimal,
    },
    /// Tax and market receipts do not affect available account cash.
    Category(TransactionCategory),
}

impl std::error::Error for CashLedgerError {}

impl std::fmt::Display for CashLedgerError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subtraction { balance, amount } => write!(output, "Arithmetic overflow in subtraction: {balance} - {amount}"),
            Self::Addition { balance, amount } => write!(output, "Arithmetic overflow in addition: {balance} + {amount}"),
            Self::Category(category) => write!(output, "capital_available: does not know how to calculate transaction with category: {category}"),
        }
    }
}

impl TransactionCategory {
    /// Apply one receipt in ledger order; negative intermediate cash is permitted.
    /// Callers still enforce their final balance and risk policy after replay.
    pub fn apply_to_available_cash(
        self,
        balance: Decimal,
        amount: Decimal,
    ) -> Result<Decimal, CashLedgerError> {
        match self {
            Self::FundTrade(_)
            | Self::Withdrawal
            | Self::WithdrawalEarnings
            | Self::FeeOpen(_)
            | Self::FeeClose(_) => balance
                .checked_sub(amount)
                .ok_or(CashLedgerError::Subtraction { balance, amount }),
            Self::PaymentFromTrade(_) | Self::PaymentEarnings(_) | Self::Deposit => balance
                .checked_add(amount)
                .ok_or(CashLedgerError::Addition { balance, amount }),
            category => Err(CashLedgerError::Category(category)),
        }
    }
}
