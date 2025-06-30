use model::{Trade, TradeCategory};
use rust_decimal::Decimal;

/// Calculates the maximum capital required to fund a trade.
/// 
/// For long trades: Uses entry price × quantity
/// For short trades: Uses stop price × quantity (worst-case scenario)
/// 
/// This ensures short trades have enough capital even if the entry
/// executes at a better price than expected.
/// 
/// ## Why Short Trades Use Stop Price
/// 
/// When shorting, a "better" execution price means selling at a higher price.
/// However, this creates a funding issue:
/// 
/// Example scenario:
/// - Entry: SELL 100 shares at $10 (expect to receive $1,000)
/// - Stop: BUY 100 shares at $15 (expect to pay $1,500)
/// - Expected funding needed: $1,500 - $1,000 = $500
/// 
/// If entry executes at $11 (better price):
/// - Actually receive: $1,100
/// - Still need to pay: $1,500 at stop
/// - Actual funding needed: $1,500 - $1,100 = $400
/// 
/// The system must fund based on the worst case (stop price) to ensure
/// sufficient capital is always available.
pub struct TradeCapitalRequired;

impl TradeCapitalRequired {
    pub fn calculate(trade: &Trade) -> Decimal {
        match trade.category {
            TradeCategory::Long => {
                trade.entry.unit_price * Decimal::from(trade.entry.quantity)
            }
            TradeCategory::Short => {
                // For short trades, we need to ensure we have enough capital
                // to buy back at the stop price (worst case scenario)
                trade.safety_stop.unit_price * Decimal::from(trade.safety_stop.quantity)
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
        let required = TradeCapitalRequired::calculate(&trade);

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
        let required = TradeCapitalRequired::calculate(&trade);

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
        let required = TradeCapitalRequired::calculate(&trade);

        // Then: Should return $250 (stop price * stop quantity)
        assert_eq!(required, dec!(250));
    }
}