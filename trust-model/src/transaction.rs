use crate::price::Price;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Transaction entity - represents a single transaction
#[derive(PartialEq, Debug)]
pub struct Transaction {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The category of the transaction - deposit, withdrawal, input, output, etc.
    pub category: TransactionCategory,

    /// The amount of the transaction
    pub price: Price,

    /// The account ID - the account that the transaction is related to
    pub account_id: Uuid,
}

/// TransactionCategory enum - represents the type of the transaction
#[derive(PartialEq, Debug)]
pub enum TransactionCategory {
    /// Deposit - money deposited into the account
    Deposit,

    /// Withdrawal - money withdrawn from the account
    Withdrawal,

    /// Output - money transferred out of the account to a trade.
    /// The Uuid is the trade ID.
    Output(Uuid),

    /// Input - money transferred into the account from a trade
    /// The Uuid is the trade ID.
    Input(Uuid),

    /// InputTax - money transferred into the account from a trade.
    /// This is a special case of Input to not use the money that should be paid to the tax authorities.
    /// /// The Uuid is the trade ID that incurred into tax liability.
    InputTax(Uuid),

    /// OutputTax - money transferred out of the account to pay taxes.
    /// This is a special case of Output to use the money that should be paid to the tax authorities.
    OutputTax,
}

impl TransactionCategory {
    fn trade_id(&self) -> Option<Uuid> {
        match self {
            TransactionCategory::Deposit => None,
            TransactionCategory::Withdrawal => None,
            TransactionCategory::Input(id) => Some(*id),
            TransactionCategory::Output(id) => Some(*id),
            TransactionCategory::InputTax(id) => Some(*id),
            TransactionCategory::OutputTax => None,
        }
    }
}

// Implementations

impl std::fmt::Display for TransactionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            TransactionCategory::Deposit => write!(f, "Deposit"),
            TransactionCategory::Withdrawal => write!(f, "Withdrawal"),
            TransactionCategory::Input(_) => write!(f, "Input"),
            TransactionCategory::Output(_) => write!(f, "Output"),
            TransactionCategory::InputTax(_) => write!(f, "InputTax"),
            TransactionCategory::OutputTax => write!(f, "OutputTax"),
        }
    }
}
#[derive(PartialEq, Debug)]
pub struct TransactionCategoryParseError;

impl TransactionCategory {
    pub fn parse(s: &str, trade_id: Option<Uuid>) -> Result<Self, TransactionCategoryParseError> {
        match s {
            "Deposit" => Ok(TransactionCategory::Deposit),
            "Withdrawal" => Ok(TransactionCategory::Withdrawal),
            "OutputTax" => Ok(TransactionCategory::OutputTax),
            "Input" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::Input(trade_id))
                } else {
                    Err(TransactionCategoryParseError)
                }
            }
            "Output" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::Output(trade_id))
                } else {
                    Err(TransactionCategoryParseError)
                }
            }
            "InputTax" => {
                if let Some(trade_id) = trade_id {
                    Ok(TransactionCategory::InputTax(trade_id))
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
        let result = TransactionCategory::parse("Deposit", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Deposit);
        let result = TransactionCategory::parse("Withdrawal", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Withdrawal);
        let result = TransactionCategory::parse("OutputTax", None)
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::OutputTax);
        let id = Uuid::new_v4();
        let result = TransactionCategory::parse("Input", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Input(id));
        let result = TransactionCategory::parse("Output", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Output(id));
        let result = TransactionCategory::parse("InputTax", Some(id))
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::InputTax(id));
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
