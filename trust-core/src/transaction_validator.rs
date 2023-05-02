use rust_decimal::Decimal;
use trust_model::{Account, Currency, Database, Price, TransactionCategory};
use uuid::Uuid;
pub struct TransactionValidator;
type TransactionValidationResult = Result<(), TransactionValidationError>;

impl TransactionValidator {
    pub fn validate(
        category: TransactionCategory,
        amount: Decimal,
        currency: Currency,
        account_id: Uuid,
        database: &mut dyn Database,
    ) -> TransactionValidationResult {
        match category {
            TransactionCategory::Deposit => {
                return validate_deposit(amount, currency, account_id, database);
            }
            TransactionCategory::Withdrawal => {
                return validate_withdraw(amount, currency, account_id, database);
            }
            TransactionCategory::Input(_)
            | TransactionCategory::Output(_)
            | TransactionCategory::InputTax(_)
            | TransactionCategory::OutputTax => {
                return Err(TransactionValidationError {
                    code: TransactionValidationErrorCode::NotImplemented,
                    message: "Manually creating transaction is not allowed".to_string(),
                });
            }
        }
    }
}

fn validate_deposit(
    amount: Decimal,
    currency: Currency,
    account_id: Uuid,
    database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() {
        return Err(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfDepositMustBePositive,
            message: "Amount of deposit must be positive".to_string(),
        });
    } else {
        Ok(())
    }
}

fn validate_withdraw(
    amount: Decimal,
    currency: Currency,
    account_id: Uuid,
    database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() {
        return Err(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfWithdrawalMustBeNegative,
            message: "Amount of withdrawal must be negative".to_string(),
        });
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
    AccountCalculationNotFound,
    AccountCalculationForWithdrawNotFound,
}

pub struct TransactionValidationError {
    pub code: TransactionValidationErrorCode,
    pub message: String,
}
