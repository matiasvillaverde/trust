use model::{Trade, TradeCategory};
use rust_decimal::Decimal;

/// Calculates the maximum capital required to fund a trade.
///
/// For long trades: Uses entry price × quantity
/// For short trades: Uses stop price × quantity (maximum capital needed)
///
/// ## Why Short Trades Use Stop Price
///
/// In a short trade, we sell first (receive money) and buy back later (pay money).
/// The maximum capital we need is the amount to buy back at the stop price.
///
/// Example scenario:
/// - Entry: SELL 100 shares at $10 (we receive $1,000)
/// - Stop: BUY 100 shares at $15 (we must pay $1,500 to close)
/// - Maximum capital needed: $1,500 (the full stop price)
///
/// Even though we receive $1,000 from the sale, we still need the full $1,500
/// available to buy back the shares. The $1,000 received doesn't reduce our
/// capital requirement - it's profit/loss that gets settled after the trade.
///
/// If entry executes at $11 (better price):
/// - We receive: $1,100
/// - We still must pay: $1,500 to buy back
/// - Net loss: $400
/// - But we need the full $1,500 available upfront!
///
/// This is different from long trades where the entry price represents the
/// maximum capital needed, since we buy first and sell later.
pub struct TradeCapitalRequired;

impl TradeCapitalRequired {
    pub fn calculate(trade: &Trade) -> Result<Decimal, Box<dyn std::error::Error>> {
        match trade.category {
            TradeCategory::Long => trade
                .entry
                .unit_price
                .checked_mul(Decimal::from(trade.entry.quantity))
                .ok_or_else(|| {
                    format!(
                        "Arithmetic overflow in multiplication: {} * {}",
                        trade.entry.unit_price, trade.entry.quantity
                    )
                    .into()
                }),
            TradeCategory::Short => {
                // For short trades, we need to ensure we have enough capital
                // to buy back at the stop price (worst case scenario)
                trade
                    .safety_stop
                    .unit_price
                    .checked_mul(Decimal::from(trade.safety_stop.quantity))
                    .ok_or_else(|| {
                        format!(
                            "Arithmetic overflow in multiplication: {} * {}",
                            trade.safety_stop.unit_price, trade.safety_stop.quantity
                        )
                        .into()
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Order;
    use rust_decimal_macros::dec;

    #[test]
    fn test_calculate_required_capital_long_trade() {
        // Given: Long trade with entry=$10, quantity=5
        let trade = Trade {
            category: TradeCategory::Long,
            entry: Order {
                unit_price: dec!(10),
                quantity: 5,
                ..Default::default()
            },
            ..Default::default()
        };

        // When: Calculating required capital
        let required = TradeCapitalRequired::calculate(&trade).unwrap();

        // Then: Should return $50 (entry * quantity)
        assert_eq!(required, dec!(50));
    }

    #[test]
    fn test_calculate_required_capital_short_trade() {
        // Given: Short trade with entry=$10, stop=$15, quantity=5
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(10),
                quantity: 5,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(15),
                quantity: 5,
                ..Default::default()
            },
            ..Default::default()
        };

        // When: Calculating required capital
        let required = TradeCapitalRequired::calculate(&trade).unwrap();

        // Then: Should return $75 (stop * quantity)
        assert_eq!(required, dec!(75));
    }

    #[test]
    fn test_calculate_required_capital_short_trade_with_different_quantities() {
        // Given: Short trade where stop quantity might differ
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(20),
                quantity: 10,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(25),
                quantity: 10, // Same quantity for safety
                ..Default::default()
            },
            ..Default::default()
        };

        // When: Calculating required capital
        let required = TradeCapitalRequired::calculate(&trade).unwrap();

        // Then: Should return $250 (stop price * stop quantity)
        assert_eq!(required, dec!(250));
    }
}
