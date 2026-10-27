use model::{OrderCategory, Status, Trade, TradeCategory};
use rust_decimal::Decimal;
use std::error::Error;

type TradeValidationResult = Result<(), Box<TradeValidationError>>;

pub fn can_submit(trade: &Trade) -> TradeValidationResult {
    match trade.status {
        Status::Funded => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFunded,
            message: format!(
                "Trade with id {} is not funded, cannot submit rule",
                trade.id
            ),
        })),
    }
}

pub fn can_fund(trade: &Trade) -> TradeValidationResult {
    match trade.status {
        Status::New => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotNew,
            message: format!("Trade with id {} is not new, cannot be funded", trade.id),
        })),
    }
}

pub fn can_close(trade: &Trade) -> TradeValidationResult {
    if trade.target.category == OrderCategory::Market {
        return Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeExitAlreadyRequested,
            message: format!(
                "Trade with id {} already has a market exit request",
                trade.id
            ),
        }));
    }
    match trade.status {
        Status::Filled => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFilled,
            message: format!("Trade with id {} is not filled, cannot be closed", trade.id),
        })),
    }
}

pub fn can_cancel_funded(trade: &Trade) -> TradeValidationResult {
    match trade.status {
        Status::Funded => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFunded,
            message: format!(
                "Trade with id {} is not funded, cannot be cancelled",
                trade.id
            ),
        })),
    }
}

pub fn can_cancel_submitted(trade: &Trade) -> TradeValidationResult {
    match trade.status {
        Status::Submitted => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotSubmitted,
            message: format!(
                "Trade with id {} is not submitted, cannot be cancelled",
                trade.id
            ),
        })),
    }
}

pub fn can_modify_stop(trade: &Trade, new_price_stop: Decimal) -> TradeValidationResult {
    if trade.status != Status::Filled {
        return Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFilled,
            message: format!(
                "Trade with id {} is not filled, cannot be modified",
                trade.id
            ),
        }));
    }

    if trade.safety_stop.unit_price <= Decimal::ZERO || new_price_stop <= Decimal::ZERO {
        return Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::InvalidStopPrice,
            message: "Current and proposed stop prices must be positive; reconcile invalid protection before modifying it".into(),
        }));
    }

    if trade.category == TradeCategory::Long && trade.safety_stop.unit_price > new_price_stop
        || trade.category == TradeCategory::Short && trade.safety_stop.unit_price < new_price_stop
    {
        return Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::StopPriceNotValid,
            message: format!(
                "Stops can not be modified because you are risking more money. Do not give more room to stops loss. Required protection bound: {}, new stop: {}",
                trade.safety_stop.unit_price, new_price_stop
            ),
        }));
    }

    Ok(())
}

pub fn can_modify_target(trade: &Trade) -> TradeValidationResult {
    match trade.status {
        Status::Filled => Ok(()),
        _ => Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFilled,
            message: format!(
                "Trade with id {} is not filled, cannot be modified",
                trade.id
            ),
        })),
    }
}

#[derive(Debug, PartialEq)]
/// Stable reasons why a lifecycle or protective-order change is rejected.
pub enum TradeValidationErrorCode {
    /// Funding requires a new trade.
    TradeNotNew,
    /// Submission or local cancellation requires a funded trade.
    TradeNotFunded,
    /// Broker cancellation requires a submitted trade.
    TradeNotSubmitted,
    /// Closing or amending protection requires a filled trade.
    TradeNotFilled,
    /// A market exit is already pending.
    TradeExitAlreadyRequested,
    /// The proposed stop would weaken the latest persisted protection.
    StopPriceNotValid,
    /// Current or proposed protection has a nonpositive price.
    InvalidStopPrice,
}

#[derive(Debug)]
/// Trade lifecycle validation failure, safe to present to authenticated clients.
pub struct TradeValidationError {
    /// Machine-readable reason for rejection.
    pub code: TradeValidationErrorCode,
    /// Domain validation detail without broker credentials.
    pub message: String,
}

impl std::fmt::Display for TradeValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TradeValidationError: {}, code: {:?}",
            self.message, self.code
        )
    }
}

impl Error for TradeValidationError {
    fn description(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    proptest::proptest! {
        #[test]
        fn stop_changes_never_widen_long_or_short_risk(current in 1_i64..1_000_000, proposed in -100_i64..1_000_000, short in proptest::bool::ANY) {
            let trade = Trade {
                status: Status::Filled,
                category: if short {TradeCategory::Short} else {TradeCategory::Long},
                safety_stop: model::Order {unit_price: Decimal::new(current, 4), ..Default::default()},
                ..Default::default()
            };
            let allowed = proposed > 0 && if short {proposed <= current} else {proposed >= current};
            proptest::prop_assert_eq!(can_modify_stop(&trade, Decimal::new(proposed, 4)).is_ok(), allowed);
        }
    }

    #[test]
    fn nonpositive_current_stop_requires_reconciliation() {
        for category in [TradeCategory::Long, TradeCategory::Short] {
            for current in [Decimal::ZERO, dec!(-1)] {
                let trade = Trade {
                    status: Status::Filled,
                    category,
                    safety_stop: model::Order {
                        unit_price: current,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                assert_eq!(
                    can_modify_stop(&trade, dec!(1)).unwrap_err().code,
                    TradeValidationErrorCode::InvalidStopPrice
                );
            }
        }
    }

    #[test]
    fn test_validate_submit_funded() {
        let trade = Trade {
            status: Status::Funded,
            ..Default::default()
        };
        assert!(can_submit(&trade).is_ok());
    }

    #[test]
    fn test_validate_submit_not_funded() {
        let trade = Trade {
            status: Status::New,
            ..Default::default()
        };
        assert!(can_submit(&trade).is_err());
    }

    #[test]
    fn test_validate_close() {
        let mut trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };
        trade.target.category = OrderCategory::Limit;
        assert!(can_close(&trade).is_ok());
    }

    #[test]
    fn test_validate_close_rejects_duplicate_market_exit_request() {
        let trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };

        let error = can_close(&trade).expect_err("market exit request must be idempotent");

        assert_eq!(
            error.code,
            TradeValidationErrorCode::TradeExitAlreadyRequested
        );
    }

    #[test]
    fn test_validate_close_not_funded() {
        let trade = Trade {
            status: Status::ClosedTarget,
            ..Default::default()
        };
        assert!(can_close(&trade).is_err());
    }

    #[test]
    fn test_validate_fund() {
        let trade = Trade {
            status: Status::New,
            ..Default::default()
        };
        let result = can_fund(&trade);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_fund_not_new() {
        let trade = Trade {
            status: Status::Funded,
            ..Default::default()
        };
        let result = can_fund(&trade);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cancel_funded() {
        let trade = Trade {
            status: Status::Funded,
            ..Default::default()
        };
        let result = can_cancel_funded(&trade);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cancel_not_funded() {
        let trade = Trade {
            status: Status::Canceled,
            ..Default::default()
        };
        let result = can_cancel_funded(&trade);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cancel_submitted() {
        let trade = Trade {
            status: Status::Submitted,
            ..Default::default()
        };
        let result = can_cancel_submitted(&trade);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cancel_not_submitted() {
        let trade = Trade {
            status: Status::Canceled,
            ..Default::default()
        };
        let result = can_cancel_submitted(&trade);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_modify_stop() {
        let trade = Trade {
            status: Status::Filled,
            safety_stop: model::Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_stop_not_filled() {
        let trade = Trade {
            status: Status::Canceled,
            ..Default::default()
        };
        let error = can_modify_stop(&trade, dec!(10)).unwrap_err();
        assert_eq!(error.code, TradeValidationErrorCode::TradeNotFilled);
    }

    #[test]
    fn test_validate_modify_stop_not_filled_prioritizes_status_over_price() {
        let long_trade = Trade {
            status: Status::Submitted,
            category: TradeCategory::Long,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let long_error = can_modify_stop(&long_trade, dec!(9)).unwrap_err();
        assert_eq!(long_error.code, TradeValidationErrorCode::TradeNotFilled);

        let short_trade = Trade {
            status: Status::Submitted,
            category: TradeCategory::Short,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let short_error = can_modify_stop(&short_trade, dec!(11)).unwrap_err();
        assert_eq!(short_error.code, TradeValidationErrorCode::TradeNotFilled);
    }

    #[test]
    fn test_validate_modify_stop_risking_more_money_long() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Long,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(9));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_modify_stop_risking_more_money_short() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Short,
            safety_stop: model::Order {
                unit_price: dec!(11),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(12));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_modify_stop_risking_same_money() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Short,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_stop_risking_same_money_long() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Long,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_stop_risking_less_money_long() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Long,
            safety_stop: model::Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(11));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_stop_risking_less_money_short() {
        let trade = Trade {
            status: Status::Filled,
            category: TradeCategory::Short,
            safety_stop: model::Order {
                unit_price: dec!(11),
                ..Default::default()
            },
            ..Default::default()
        };
        let result = can_modify_stop(&trade, dec!(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_target() {
        let trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };
        let result = can_modify_target(&trade);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_modify_target_not_filled() {
        let trade = Trade {
            status: Status::Canceled,
            ..Default::default()
        };
        let result = can_modify_target(&trade);
        assert!(result.is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn trade_validation_error_display_and_description_are_stable() {
        let error = TradeValidationError {
            code: TradeValidationErrorCode::TradeNotFunded,
            message: "trade is not funded".to_string(),
        };

        assert_eq!(
            error.to_string(),
            "TradeValidationError: trade is not funded, code: TradeNotFunded"
        );
        assert_eq!(
            std::error::Error::description(&error),
            "trade is not funded"
        );
    }
}
