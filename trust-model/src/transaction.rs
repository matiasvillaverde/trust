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

impl TransactionCategory {
    pub fn from_str(s: &str) -> TransactionCategory {
        match s {
            "Deposit" => TransactionCategory::Deposit,
            "Withdrawal" => TransactionCategory::Withdrawal,
            "Input" => TransactionCategory::Input,
            "Output" => TransactionCategory::Output,
            "InputTax" => TransactionCategory::InputTax,
            _ => panic!("Unknown TransactionCategory: {}", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_string() {
        let result = TransactionCategory::from_str("Deposit");
        assert_eq!(result, TransactionCategory::Deposit);
        let result = TransactionCategory::from_str("Withdrawal");
        assert_eq!(result, TransactionCategory::Withdrawal);
        let result = TransactionCategory::from_str("Input");
        assert_eq!(result, TransactionCategory::Input);
        let result = TransactionCategory::from_str("Output");
        assert_eq!(result, TransactionCategory::Output);
        let result = TransactionCategory::from_str("InputTax");
        assert_eq!(result, TransactionCategory::InputTax);
    }
}
