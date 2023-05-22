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
            TransactionCategory::Output(trade_id) => {
                validate_trade(amount, currency, account_id, trade_id, database)
            }
            TransactionCategory::Input(_)
            | TransactionCategory::InputTax(_)
            | TransactionCategory::OutputTax => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::NotAuthorized,
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
    currency: &Currency,
    account_id: Uuid,
    database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() | amount.is_zero() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfWithdrawalMustBePositive,
            message: "Amount of withdrawal must be positive".to_string(),
        }))
    } else {
        let overview = database.read_account_overview_currency(account_id, currency);
        match overview {
            Ok(overview) => {
                if overview.total_available.amount >= amount {
                    Ok(())
                } else {
                    Err(Box::new(TransactionValidationError {
                        code: TransactionValidationErrorCode::WithdrawalAmountIsGreaterThanAvailableAmount,
                        message: "Withdrawal amount is greater than available amount".to_string(),
                    }))
                }
            },
            Err(_) => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::OverviewForWithdrawNotFound,
                message: "Overview not found. It can be that the user never created a deposit on this currency".to_string(),
            })),
        }
    }
}

fn validate_trade(
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
    trade_id: Uuid,
    database: &mut dyn Database,
) -> TransactionValidationResult {
    if amount.is_sign_negative() | amount.is_zero() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfWithdrawalMustBePositive,
            message: "Amount of withdrawal must be positive".to_string(),
        }))
    } else {
        let overview = database.read_account_overview_currency(account_id, currency);
        match overview {
            Ok(overview) => {
                if overview.total_available.amount >= amount {

                    let trade = database.read_trade(trade_id);

                    // Validate that the approved_at in the trade is not null (it is approved)
                    match trade {
                        Ok(trade) => {
                            if trade.approved_at.is_some() {
                                Ok(())
                            } else {
                                Err(Box::new(TransactionValidationError {
                                    code: TransactionValidationErrorCode::NotAuthorized,
                                    message: "Trade is not approved".to_string(),
                                }))
                            }
                        },
                        Err(_) => Err(Box::new(TransactionValidationError {
                            code: TransactionValidationErrorCode::TradeNotFound,
                            message: "Trade not found".to_string(),
                        })),
                    }
                } else {
                    Err(Box::new(TransactionValidationError {
                        code: TransactionValidationErrorCode::TradeAmountIsGreaterThanAvailableAmount,
                        message: "Trade amount is greater than available amount".to_string(),
                    }))
                }
            },
            Err(_) => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::OverviewForTradeNotFound,
                message: "Overview not found. It can be that the user never created a deposit on this currency".to_string(),
            })),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TransactionValidationErrorCode {
    NotAuthorized,
    AmountOfWithdrawalMustBePositive,
    AmountOfDepositMustBePositive,
    WithdrawalAmountIsGreaterThanAvailableAmount,
    TradeAmountIsGreaterThanAvailableAmount,
    OverviewNotFound,
    OverviewForWithdrawNotFound,
    OverviewForTradeNotFound,
    TradeNotFound,
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
