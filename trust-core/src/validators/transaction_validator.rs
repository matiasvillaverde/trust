use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_model::{
    Currency, ReadAccountOverviewDB, ReadTradeDB, Status, Trade, TransactionCategory,
};
use uuid::Uuid;
pub struct TransactionValidator;
type TransactionValidationResult = Result<(), Box<TransactionValidationError>>;

impl TransactionValidator {
    pub fn validate(
        category: TransactionCategory,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
        database: &mut dyn ReadAccountOverviewDB,
        database_trade: &mut dyn ReadTradeDB,
    ) -> TransactionValidationResult {
        match category {
            TransactionCategory::Deposit => {
                validate_deposit(amount, currency, account_id, database)
            }
            TransactionCategory::Withdrawal => {
                validate_withdraw(amount, currency, account_id, database)
            }
            TransactionCategory::FundTrade(trade_id) => validate_trade(
                amount,
                currency,
                account_id,
                trade_id,
                database,
                database_trade,
            ),
            _default => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::NotAuthorized,
                message: "Manually creating transaction is not allowed".to_string(),
            })),
        }
    }

    pub fn validate_fill(trade: &Trade, total: Decimal) -> TransactionValidationResult {
        match trade.status {
            Status::Submitted | Status::Funded => (),
            _ => {
                return Err(Box::new(TransactionValidationError {
                    code: TransactionValidationErrorCode::WrongTradeStatus,
                    message: "Trade status is wrong".to_string(),
                }))
            }
        }

        if total <= dec!(0) {
            return Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::FillingMustBePositive,
                message: "Filling must be positive".to_string(),
            }));
        }

        if total > trade.overview.funding {
            return Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::NotEnoughFunds,
                message: "Trade doesn't have enough funding".to_string(),
            }));
        }
        Ok(())
    }
}

fn validate_deposit(
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
    database: &mut dyn ReadAccountOverviewDB,
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
    database: &mut dyn ReadAccountOverviewDB,
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
                if overview.total_available >= amount {
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
    database: &mut dyn ReadAccountOverviewDB,
    database_trade: &mut dyn ReadTradeDB,
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
                if overview.total_available >= amount {

                    let trade = database_trade.read_trade(trade_id);

                    // Validate that state in the trade is not null (it is approved)
                    match trade {
                        Ok(trade) => {
                            if trade.status == Status::Funded {
                                Ok(())
                            } else {
                                Err(Box::new(TransactionValidationError {
                                    code: TransactionValidationErrorCode::NotAuthorized,
                                    message: "Trade is not funded".to_string(),
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
    NotEnoughFunds,
    WrongTradeStatus,
    FillingMustBePositive,
}

#[derive(Debug, PartialEq)]
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
#[cfg(test)]
mod tests {
    use trust_model::TradeOverview;

    use super::*;

    #[test]
    fn test_validate_fill_with_enough_funds() {
        let trade = Trade {
            overview: TradeOverview {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Funded,
            ..Default::default()
        };
        let total = dec!(500);
        assert!(TransactionValidator::validate_fill(&trade, total).is_ok());
    }

    #[test]
    fn test_validate_fill_with_enough_funds_status_submitted() {
        let trade = Trade {
            overview: TradeOverview {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let total = dec!(459.3);
        assert!(TransactionValidator::validate_fill(&trade, total).is_ok());
    }

    #[test]
    fn test_validate_fill_with_not_enough_funds() {
        let trade = Trade {
            overview: TradeOverview {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Funded,
            ..Default::default()
        };
        let total = dec!(1500);
        let expected_err = TransactionValidationError {
            code: TransactionValidationErrorCode::NotEnoughFunds,
            message: "Trade doesn't have enough funding".to_string(),
        };
        assert_eq!(
            TransactionValidator::validate_fill(&trade, total),
            Err(Box::new(expected_err))
        );
    }

    #[test]
    fn test_validate_fill_with_zero_total() {
        let trade = Trade {
            overview: TradeOverview {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Funded,
            ..Default::default()
        };
        let total = dec!(0);
        assert_eq!(
            TransactionValidator::validate_fill(&trade, total),
            Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::FillingMustBePositive,
                message: "Filling must be positive".to_string(),
            }))
        );
    }

    #[test]
    fn test_validate_fill_with_unfunded_trade() {
        let trade = Trade {
            overview: TradeOverview {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Filled,
            ..Default::default()
        };
        let total = dec!(500);
        assert_eq!(
            TransactionValidator::validate_fill(&trade, total),
            Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::WrongTradeStatus,
                message: "Trade status is wrong".to_string(),
            }))
        );
    }
}
