use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Event emitted when a trade is closed.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeClosed {
    /// Closed trade identifier.
    pub trade_id: Uuid,
    /// Account that owns the trade.
    pub account_id: Uuid,
    /// Final realized PnL of the trade.
    pub final_pnl: Decimal,
    /// Realized P&L divided by remaining bracket risk at close.
    /// Zero when the stop locks in break-even/profit (original risk is not stored).
    pub r_multiple: Decimal,
    /// Why the trade was closed.
    pub close_reason: CloseReason,
    /// Close timestamp.
    pub closed_at: NaiveDateTime,
}

/// Reason why a trade was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Trade closed at target.
    Target,
    /// Trade closed at stop.
    StopLoss,
    /// Trade closed at stop with slippage.
    StopLossSlippage,
    /// Trade closed manually.
    Manual,
}

impl TradeClosed {
    /// Build a close event using the direction of the persisted bracket.
    /// Arithmetic failures propagate so the surrounding close transaction rolls back.
    pub fn from_trade(
        trade: &model::Trade,
        close_reason: CloseReason,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let distance = match trade.category {
            model::TradeCategory::Long => trade
                .entry
                .unit_price
                .checked_sub(trade.safety_stop.unit_price),
            model::TradeCategory::Short => trade
                .safety_stop
                .unit_price
                .checked_sub(trade.entry.unit_price),
        }
        .ok_or("Close-event stop distance overflow")?;
        let risk = distance
            .max(Decimal::ZERO)
            .checked_mul(trade.entry.quantity)
            .ok_or("Close-event risk amount overflow")?;
        let r_multiple = if risk > Decimal::ZERO {
            trade
                .balance
                .total_performance
                .checked_div(risk)
                .ok_or("Close-event R-multiple overflow")?
        } else {
            Decimal::ZERO
        };
        Ok(Self {
            trade_id: trade.id,
            account_id: trade.account_id,
            final_pnl: trade.balance.total_performance,
            r_multiple,
            close_reason,
            closed_at: trade.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{Trade, TradeCategory};
    use rust_decimal_macros::dec;

    #[test]
    fn long_and_short_brackets_have_the_same_reward_in_risk_units() {
        for (category, stop) in [
            (TradeCategory::Long, dec!(90)),
            (TradeCategory::Short, dec!(110)),
        ] {
            for (profit, expected) in [
                (dec!(50), dec!(2)),
                (dec!(-25), dec!(-1)),
                (dec!(0), dec!(0)),
            ] {
                let mut trade = Trade {
                    category,
                    ..Trade::default()
                };
                trade.entry.unit_price = dec!(100);
                trade.entry.quantity = dec!(2.5);
                trade.safety_stop.unit_price = stop;
                trade.balance.total_performance = profit;
                let event = TradeClosed::from_trade(&trade, CloseReason::Target).unwrap();
                assert_eq!(event.r_multiple, expected);
                assert_eq!(event.final_pnl, profit);
                assert_eq!(event.trade_id, trade.id);
                assert_eq!(event.account_id, trade.account_id);
                assert_eq!(event.closed_at, trade.updated_at);
            }
        }
    }

    #[test]
    fn protected_and_break_even_stops_do_not_invent_remaining_risk() {
        for (category, stop) in [
            (TradeCategory::Long, dec!(100)),
            (TradeCategory::Long, dec!(110)),
            (TradeCategory::Short, dec!(90)),
        ] {
            let mut trade = Trade {
                category,
                ..Trade::default()
            };
            trade.entry.unit_price = dec!(100);
            trade.entry.quantity = dec!(2);
            trade.safety_stop.unit_price = stop;
            trade.balance.total_performance = dec!(20);
            assert_eq!(
                TradeClosed::from_trade(&trade, CloseReason::StopLoss)
                    .unwrap()
                    .r_multiple,
                Decimal::ZERO
            );
        }
    }

    #[test]
    fn arithmetic_failure_cannot_silently_become_a_zero_r_event() {
        let mut trade = Trade::default();
        trade.entry.unit_price = Decimal::MAX;
        trade.entry.quantity = dec!(2);
        trade.safety_stop.unit_price = Decimal::ZERO;
        assert!(TradeClosed::from_trade(&trade, CloseReason::Target).is_err());
    }
}
