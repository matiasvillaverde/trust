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

    /// Trade ID - if the transaction is related to a trade, this field contains the trade ID.
    pub trade_id: Option<Uuid>,
}

/// TransactionCategory enum - represents the type of the transaction
#[derive(PartialEq, Debug)]
pub enum TransactionCategory {
    /// Deposit - money deposited into the account
    Deposit,

    /// Withdrawal - money withdrawn from the account
    Withdrawal,

    /// Output - money transferred out of the account to a trade
    Output,

    /// Input - money transferred into the account from a trade
    Input,

    /// InputTax - money transferred into the account from a trade.
    /// This is a special case of Input to not use the money that should be paid to the tax authorities.
    InputTax,
}

// Implementations

impl std::fmt::Display for TransactionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            TransactionCategory::Deposit => write!(f, "Deposit"),
            TransactionCategory::Withdrawal => write!(f, "Withdrawal"),
            TransactionCategory::Input => write!(f, "Input"),
            TransactionCategory::Output => write!(f, "Output"),
            TransactionCategory::InputTax => write!(f, "InputTax"),
        }
    }
}
#[derive(PartialEq, Debug)]
pub struct TransactionCategoryParseError;

impl std::str::FromStr for TransactionCategory {
    type Err = TransactionCategoryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Deposit" => Ok(TransactionCategory::Deposit),
            "Withdrawal" => Ok(TransactionCategory::Withdrawal),
            "Input" => Ok(TransactionCategory::Input),
            "Output" => Ok(TransactionCategory::Output),
            "InputTax" => Ok(TransactionCategory::InputTax),
            _ => Err(TransactionCategoryParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_transaction_category_from_string() {
        let result = TransactionCategory::from_str("Deposit")
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Deposit);
        let result = TransactionCategory::from_str("Withdrawal")
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Withdrawal);
        let result = TransactionCategory::from_str("Input")
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Input);
        let result = TransactionCategory::from_str("Output")
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::Output);
        let result = TransactionCategory::from_str("InputTax")
            .expect("Failed to parse TransactionCategory from string");
        assert_eq!(result, TransactionCategory::InputTax);
    }

    #[test]
    fn test_transaction_category_from_invalid_string() {
        TransactionCategory::from_str("Invalid")
            .expect_err("Failed to parse TransactionCategory from string"); // Invalid
    }
}
