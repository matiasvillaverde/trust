use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{Currency, Database, TransactionCategory};
use uuid::Uuid;
pub struct TransactionValidator;
type TransactionValidationResult = Result<(), Box<TransactionValidationError>>;

impl TransactionValidator {
    pub fn validate(
        category: TransactionCategory,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
        database: &mut dyn Database,
    ) -> TransactionValidationResult {
        match category {
            TransactionCategory::Deposit => {
                validate_deposit(amount, currency, account_id, database)
            }
            TransactionCategory::Withdrawal => {
                validate_withdraw(amount, currency, account_id, database)
            }
            TransactionCategory::Input(_)
            | TransactionCategory::Output(_)
            | TransactionCategory::InputTax(_)
            | TransactionCategory::OutputTax => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::NotImplemented,
                message: "Manually creating transaction is not allowed".to_string(),
            })),
        }
    }
}

fn validate_deposit(
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
    database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfDepositMustBePositive,
            message: "Amount of deposit must be positive".to_string(),
        }))
    } else {
        match database.read_account_overview_currency(account_id, currency) {
            Ok(_) => Ok(()),
            Err(_) => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::OverviewNotFound,
                message: "Overview not found. It can be that the user never created a deposit on this currency".to_string(),
            })),
        }
    }
}

fn validate_withdraw(
    amount: Decimal,
    _currency: &Currency,
    _account_id: Uuid,
    _database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfWithdrawalMustBeNegative,
            message: "Amount of withdrawal must be negative".to_string(),
        }))
    } else {
        // TODO: validate that the amount is valid
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum TransactionValidationErrorCode {
    NotImplemented,
    NotAuthorized,
    AmountOfWithdrawalMustBeNegative,
    AmountOfDepositMustBePositive,
    WithdrawalAmountIsGreaterThanAvailableAmount,
    OverviewNotFound,
    OverviewForWithdrawNotFound,
}

#[derive(Debug)]
pub struct TransactionValidationError {
    pub code: TransactionValidationErrorCode,
    pub message: String,
}

impl std::fmt::Display for TransactionValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TransactionValidationError: {}", self.message)
    }
}

impl Error for TransactionValidationError {
    fn description(&self) -> &str {
        &self.message
    }
}
