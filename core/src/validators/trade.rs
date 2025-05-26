use model::{Status, Trade, TradeCategory};
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

pub fn can_close(trade: &Trade) -> TradeValidationResult {
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
            code: TradeValidationErrorCode::TradeNotFunded,
            message: format!(
                "Trade with id {} is not funded, cannot be cancelled",
                trade.id
            ),
        })),
    }
}

pub fn can_modify_stop(trade: &Trade, new_price_stop: Decimal) -> TradeValidationResult {
    if trade.category == TradeCategory::Long && trade.safety_stop.unit_price > new_price_stop
        || trade.category == TradeCategory::Short && trade.safety_stop.unit_price < new_price_stop
    {
        return Err(Box::new(TradeValidationError {
            code: TradeValidationErrorCode::StopPriceNotValid,
            message: format!(
                "Stops can not be modified because you are risking more money. Do not give more room to stops loss. Current stop: {}, new stop: {}",
                trade.safety_stop.unit_price, new_price_stop
            ),
        }));
    }

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
pub enum TradeValidationErrorCode {
    TradeNotFunded,
    TradeNotFilled,
    StopPriceNotValid,
}

#[derive(Debug)]
pub struct TradeValidationError {
    pub code: TradeValidationErrorCode,
    pub message: String,
}

impl std::fmt::Display for TradeValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TradeValidationError: {}, code: {:?}", self.message, self.code)
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
        let trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };
        assert!(can_close(&trade).is_ok());
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
        let result = can_modify_stop(&trade, dec!(10));
        assert!(result.is_err());
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
}
