use model::{AccountOverview, Currency, AccountOverviewRead, Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;
type TransactionValidationResult = Result<(), Box<TransactionValidationError>>;

pub fn can_transfer_fill(trade: &Trade, total: Decimal) -> TransactionValidationResult {
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

pub fn can_transfer_fee(account: &AccountOverview, fee: Decimal) -> TransactionValidationResult {
    if fee <= dec!(0) {
        return Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::FeeMustBePositive,
            message: "Fee must be positive".to_string(),
        }));
    }

    if fee > account.total_available {
        return Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::NotEnoughFunds,
            message: "Account doesn't have enough funds".to_string(),
        }));
    }
    Ok(())
}

pub fn can_transfer_close(total: Decimal) -> TransactionValidationResult {
    if total <= dec!(0) {
        return Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::ClosingMustBePositive,
            message: "Closing must be positive".to_string(),
        }));
    }
    Ok(())
}

pub fn can_transfer_deposit(
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
    database: &mut dyn AccountOverviewRead,
) -> TransactionValidationResult {
    if amount.is_sign_negative() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfDepositMustBePositive,
            message: "Amount of deposit must be positive".to_string(),
        }))
    } else {
        match database.for_currency(account_id, currency) {
            Ok(_) => Ok(()),
            Err(_) => Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::OverviewNotFound,
                message: "Overview not found. It can be that the user never created a deposit on this currency".to_string(),
            })),
        }
    }
}

pub fn can_transfer_withdraw(
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
    database: &mut dyn AccountOverviewRead,
) -> TransactionValidationResult {
    if amount.is_sign_negative() | amount.is_zero() {
        Err(Box::new(TransactionValidationError {
            code: TransactionValidationErrorCode::AmountOfWithdrawalMustBePositive,
            message: "Amount of withdrawal must be positive".to_string(),
        }))
    } else {
        let overview = database.for_currency(account_id, currency);
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

#[derive(Debug, PartialEq)]
pub enum TransactionValidationErrorCode {
    AmountOfWithdrawalMustBePositive,
    AmountOfDepositMustBePositive,
    WithdrawalAmountIsGreaterThanAvailableAmount,
    OverviewNotFound,
    OverviewForWithdrawNotFound,
    NotEnoughFunds,
    WrongTradeStatus,
    FillingMustBePositive,
    FeeMustBePositive,
    ClosingMustBePositive,
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
    use model::TradeOverview;

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
        assert!(can_transfer_fill(&trade, total).is_ok());
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
        assert!(can_transfer_fill(&trade, total).is_ok());
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
            can_transfer_fill(&trade, total),
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
            can_transfer_fill(&trade, total),
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
            can_transfer_fill(&trade, total),
            Err(Box::new(TransactionValidationError {
                code: TransactionValidationErrorCode::WrongTradeStatus,
                message: "Trade status is wrong".to_string(),
            }))
        );
    }

    #[test]
    fn test_validate_fee_positive() {
        let account = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let fee = dec!(10);
        assert_eq!(can_transfer_fee(&account, fee), Ok(()));
    }

    #[test]
    fn test_validate_fee_zero() {
        let account = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let fee = dec!(0);
        assert!(can_transfer_fee(&account, fee).is_err());
    }

    #[test]
    fn test_validate_fee_negative() {
        let account = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let fee = dec!(-10);
        assert!(can_transfer_fee(&account, fee).is_err());
    }

    #[test]
    fn test_validate_fee_not_enough_funds() {
        let account = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let fee = dec!(200);
        assert!(can_transfer_fee(&account, fee).is_err());
    }

    #[test]
    fn test_validate_close_success() {
        let result = can_transfer_close(dec!(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_close_failure() {
        let result = can_transfer_close(dec!(-10));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(
            err.code,
            TransactionValidationErrorCode::ClosingMustBePositive
        );
        assert_eq!(err.message, "Closing must be positive");
    }
}
