//! Asset-aware decimal formatting for CLI display.

use model::TradingVehicleCategory;
use rust_decimal::{Decimal, RoundingStrategy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DisplayPrecision {
    price_dp: u32,
    quantity_dp: u32,
    trim_price: bool,
}

impl DisplayPrecision {
    pub(crate) fn for_category(category: TradingVehicleCategory) -> Self {
        match category {
            TradingVehicleCategory::Crypto => Self {
                price_dp: 8,
                quantity_dp: 8,
                trim_price: true,
            },
            TradingVehicleCategory::Bond => Self {
                price_dp: 3,
                quantity_dp: 3,
                trim_price: false,
            },
            TradingVehicleCategory::Stock | TradingVehicleCategory::Etf => Self {
                price_dp: 2,
                quantity_dp: 6,
                trim_price: false,
            },
            TradingVehicleCategory::Fiat => Self {
                price_dp: 2,
                quantity_dp: 2,
                trim_price: false,
            },
            _ => Self {
                price_dp: 2,
                quantity_dp: 6,
                trim_price: false,
            },
        }
    }

    pub(crate) fn format_price(self, value: Decimal) -> String {
        if self.trim_price {
            trim_decimal(value, self.price_dp)
        } else {
            fixed_decimal(value, self.price_dp)
        }
    }

    pub(crate) fn format_quantity(self, value: Decimal) -> String {
        trim_decimal(value, self.quantity_dp)
    }
}

pub(crate) fn format_price(category: TradingVehicleCategory, value: Decimal) -> String {
    DisplayPrecision::for_category(category).format_price(value)
}

pub(crate) fn format_quantity(category: TradingVehicleCategory, value: Decimal) -> String {
    DisplayPrecision::for_category(category).format_quantity(value)
}

fn fixed_decimal(value: Decimal, decimal_places: u32) -> String {
    let rounded =
        value.round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointAwayFromZero);
    let precision = usize::try_from(decimal_places).unwrap_or(8);
    format!("{rounded:.precision$}")
}

fn trim_decimal(value: Decimal, decimal_places: u32) -> String {
    value
        .round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointAwayFromZero)
        .normalize()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{format_price, format_quantity};
    use model::TradingVehicleCategory;
    use rust_decimal_macros::dec;

    #[test]
    fn stock_prices_are_fixed_to_cents_and_quantities_allow_fractional_shares() {
        assert_eq!(
            format_price(TradingVehicleCategory::Stock, dec!(182.5)),
            "182.50"
        );
        assert_eq!(
            format_quantity(TradingVehicleCategory::Stock, dec!(1.2500004)),
            "1.25"
        );
    }

    #[test]
    fn crypto_quantities_keep_eight_decimal_places_when_needed() {
        assert_eq!(
            format_quantity(TradingVehicleCategory::Crypto, dec!(0.000123456)),
            "0.00012346"
        );
        assert_eq!(
            format_price(TradingVehicleCategory::Crypto, dec!(68123.450000001)),
            "68123.45"
        );
    }

    #[test]
    fn bond_prices_are_quoted_to_thousandths() {
        assert_eq!(
            format_price(TradingVehicleCategory::Bond, dec!(98.75)),
            "98.750"
        );
        assert_eq!(
            format_quantity(TradingVehicleCategory::Bond, dec!(1.2345)),
            "1.235"
        );
    }
}
