use model::{Status, Trade};
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

pub fn can_cancel(trade: &Trade) -> TradeValidationResult {
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

#[derive(Debug, PartialEq)]

pub enum TradeValidationErrorCode {
    TradeNotFunded,
    TradeNotFilled,
}

#[derive(Debug)]
pub struct TradeValidationError {
    pub code: TradeValidationErrorCode,
    pub message: String,
}

impl std::fmt::Display for TradeValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "TradeValidationError: {}", self.message)
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
        let result = can_cancel(&trade);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cancel_not_funded() {
        let trade = Trade {
            status: Status::Canceled,
            ..Default::default()
        };
        let result = can_cancel(&trade);
        assert!(result.is_err());
    }
}
