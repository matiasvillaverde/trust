use crate::{price::Price, Currency};
use chrono::NaiveDateTime;
use chrono::Utc;
use uuid::Uuid;

/// Transaction entity - represents a single transaction
#[derive(PartialEq, Debug, Clone)]
pub struct Transaction {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The category of the transaction - deposit, withdrawal, input, output, etc.
    pub category: TransactionCategory,

    /// The currency of the transaction
    pub currency: Currency,

    /// The amount of the transaction
    pub price: Price,

    /// The account ID - the account that the transaction is related to
    pub account_id: Uuid,
}

/// TransactionCategory enum - represents the type of the transaction
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum TransactionCategory {
    /// Deposit - money deposited into the account
    Deposit,

    /// Withdrawal - money withdrawn from the account
    Withdrawal,

    /// Money transferred out of the account to a trade.
    /// The Uuid is the trade ID.
    FundTrade(Uuid),

    /// Money transferred into the account from a trade
    /// The Uuid is the trade ID.
    PaymentFromTrade(Uuid),

    /// Money transferred from a trade into the market.
    /// The Uuid is the trade ID.
    OpenTrade(Uuid),

    /// Exit - money transferred from the market into a trade at a profit.
    /// The Uuid is the trade ID.
    CloseTarget(Uuid),

    /// ExitStopLoss - money transferred from the market into a trade at a loss.
    /// The Uuid is the trade ID.
    CloseSafetyStop(Uuid),

    /// Money transferred from the market into a trade at a loss lower than the safety stop.
    /// This is a special case when the safety stop is triggered below the target due to slippage.
    /// The Uuid is the trade ID.
    CloseSafetyStopSlippage(Uuid),

    /// Money transferred from a trade to the broker as a fee to open the trade.
    /// The Uuid is the trade ID.
    FeeOpen(Uuid),

    /// Money transferred from a trade to the broker as a fee to close the trade.
    FeeClose(Uuid),

    /// Money transferred into the account from a trade.
    /// This is a special case of Input to not use the money that should be paid to the tax authorities.
    /// /// The Uuid is the trade ID that incurred into tax liability.
    PaymentTax(Uuid),

    /// Money transferred out of the account to pay taxes.
    /// This is a special case of Withdrawal to use the money that should be paid to the tax authorities.
    WithdrawalTax,

    /// Money transferred out of a trade to pay earnings.
    /// The Uuid is the trade ID.
    PaymentEarnings(Uuid),

    /// Money transferred out an account to enjoy earnings.
    WithdrawalEarnings,
}

impl TransactionCategory {
    pub fn trade_id(&self) -> Option<Uuid> {
        match self {
            TransactionCategory::Deposit => None,
            TransactionCategory::Withdrawal => None,
            TransactionCategory::PaymentFromTrade(id) => Some(*id),
            TransactionCategory::FundTrade(id) => Some(*id),
            TransactionCategory::OpenTrade(id) => Some(*id),
            TransactionCategory::CloseTarget(id) => Some(*id),
            TransactionCategory::CloseSafetyStop(id) => Some(*id),
            TransactionCategory::CloseSafetyStopSlippage(id) => Some(*id),
            TransactionCategory::FeeOpen(id) => Some(*id),
            TransactionCategory::FeeClose(id) => Some(*id),
            TransactionCategory::PaymentEarnings(id) => Some(*id),
            TransactionCategory::WithdrawalEarnings => None,
            TransactionCategory::PaymentTax(id) => Some(*id),
            TransactionCategory::WithdrawalTax => None,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            TransactionCategory::Deposit => "deposit",
            TransactionCategory::Withdrawal => "withdrawal",
            TransactionCategory::PaymentFromTrade(_) => "payment_from_trade",
            TransactionCategory::FundTrade(_) => "fund_trade",
            TransactionCategory::OpenTrade(_) => "open_trade",
            TransactionCategory::CloseTarget(_) => "close_target",
            TransactionCategory::CloseSafetyStop(_) => "close_safety_stop",
            TransactionCategory::CloseSafetyStopSlippage(_) => "close_safety_stop_slippage",
            TransactionCategory::FeeOpen(_) => "fee_open",
            TransactionCategory::FeeClose(_) => "fee_close",
            TransactionCategory::PaymentEarnings(_) => "payment_earnings",
            TransactionCategory::WithdrawalEarnings => "withdrawal_earnings",
            TransactionCategory::PaymentTax(_) => "payment_tax",
            TransactionCategory::WithdrawalTax => "withdrawal_tax",
        }
    }
}

// Implementations

impl std::fmt::Display for TransactionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            TransactionCategory::Deposit => write!(f, "deposit"),
            TransactionCategory::Withdrawal => write!(f, "withdrawal"),
            TransactionCategory::PaymentFromTrade(_) => write!(f, "payment_from_trade"),
            TransactionCategory::FundTrade(_) => write!(f, "fund_trade"),
            TransactionCategory::OpenTrade(_) => write!(f, "open_trade"),
            TransactionCategory::CloseTarget(_) => write!(f, "close_target"),
            TransactionCategory::CloseSafetyStop(_) => write!(f, "close_safety_stop"),
            TransactionCategory::CloseSafetyStopSlippage(_) => {
                write!(f, "close_safety_stop_slippage")
            }
            TransactionCategory::FeeOpen(_) => write!(f, "fee_open"),
            TransactionCategory::FeeClose(_) => write!(f, "fee_close"),
            TransactionCategory::PaymentEarnings(_) => write!(f, "payment_earnings"),
            TransactionCategory::WithdrawalEarnings => write!(f, "withdrawal_earnings"),
            TransactionCategory::PaymentTax(_) => write!(f, "payment_tax"),
            TransactionCategory::WithdrawalTax => write!(f, "withdrawal_tax"),
        }
    }
}

impl Transaction {
    pub fn new(
        account_id: Uuid,
        category: TransactionCategory,
        currency: &Currency,
        price: Price,
    ) -> Transaction {
        let now = Utc::now().naive_utc();
        Transaction {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id,
            category,
            currency: *currency,
            price,
        }
    }
}
#[derive(PartialEq, Debug)]
pub struct TransactionCategoryParseError;

impl TransactionCategory {
    pub fn parse(s: &str, trade_id: Option<Uuid>) -> Result<Self, TransactionCategoryParseError> {
        match s {
            "deposit" => Ok(TransactionCategory::Deposit),
            "withdrawal" => Ok(TransactionCategory::Withdrawal),
            "output_tax" => Ok(TransactionCategory::WithdrawalTax),
            "input" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::PaymentFromTrade(trade_id))
                } else {
                    Err(TransactionCategoryParseError)
                }
            }
            "output" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::FundTrade(trade_id))
                } else {
                    Err(TransactionCategoryParseError)
                }
            }
            "input_tax" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::PaymentTax(trade_id))
                } else {
                    Err(TransactionCategoryParseError)
                }
            }
            _ => Err(TransactionCategoryParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_category_from_string() {
        let result = TransactionCategory::parse("deposit", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Deposit);
        let result = TransactionCategory::parse("withdrawal", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Withdrawal);
        let result = TransactionCategory::parse("output_tax", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::WithdrawalTax);
        let id = Uuid::new_v4();
        let result = TransactionCategory::parse("input", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::PaymentFromTrade(id));
        let result = TransactionCategory::parse("output", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::FundTrade(id));
        let result = TransactionCategory::parse("input_tax", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::PaymentTax(id));
    }

    #[test]
    fn test_transaction_category_from_invalid_string() {
        TransactionCategory::parse("Invalid", None)
            .expect_err("Failed to parse TransactionCategory from string"); // Invalid
        TransactionCategory::parse("Input", None)
            .expect_err("Parsed a transaction input without a trade id");
        TransactionCategory::parse("Output", None)
            .expect_err("Parsed a transaction output without a trade id");
        TransactionCategory::parse("InputTax", None)
            .expect_err("Parsed a transaction InputTax without a trade id");
    }
}
