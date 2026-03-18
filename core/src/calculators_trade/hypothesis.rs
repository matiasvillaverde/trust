use model::{Currency, DatabaseFactory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::calculators_account::AccountCapitalAvailable;

/// Calculated trade hypothesis values for a single proposed position.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeHypothesis {
    /// Available account capital in the selected currency.
    pub available_capital: Decimal,
    /// Proposed position quantity.
    pub quantity: i64,
    /// Required capital to fund the position.
    ///
    /// Long setups use `entry_price * quantity`.
    /// Short setups use `stop_price * quantity`, matching the real funding path.
    pub capital_required: Decimal,
    /// Capital required as a percentage of available capital.
    pub capital_required_pct_of_available: Option<Decimal>,
    /// Per-share price risk for the inferred trade side.
    pub risk_per_share: Decimal,
    /// Per-share reward for the inferred trade side.
    pub reward_per_share: Decimal,
    /// Maximum loss for the proposed quantity.
    pub max_loss: Decimal,
    /// Maximum loss as a percentage of available capital.
    pub max_loss_pct_of_available: Option<Decimal>,
    /// Maximum gain for the proposed quantity.
    pub max_gain: Decimal,
    /// Maximum gain as a percentage of available capital.
    pub max_gain_pct_of_available: Option<Decimal>,
    /// Reward-to-risk ratio. `None` when the setup has zero risk.
    pub risk_reward_ratio: Option<Decimal>,
}

/// Calculator for pre-trade hypothesis analysis.
pub struct TradeHypothesisCalculator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HypothesisSide {
    Long,
    Short,
}

impl TradeHypothesisCalculator {
    /// Calculates maximum loss, gain, and account impact for a hypothetical trade.
    pub fn calculate(
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        quantity: i64,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<TradeHypothesis, Box<dyn std::error::Error>> {
        Self::ensure_positive_price("entry_price", entry_price)?;
        Self::ensure_positive_price("stop_price", stop_price)?;
        Self::ensure_positive_price("target_price", target_price)?;

        if quantity <= 0 {
            return Err(format!("quantity must be greater than 0, got {quantity}").into());
        }

        Self::ensure_account_exists(account_id, database)?;

        let available_capital = AccountCapitalAvailable::calculate(
            account_id,
            currency,
            database.transaction_read().as_mut(),
        )?;
        Self::calculate_with_available_capital(
            available_capital,
            entry_price,
            stop_price,
            target_price,
            quantity,
        )
    }

    fn calculate_with_available_capital(
        available_capital: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        quantity: i64,
    ) -> Result<TradeHypothesis, Box<dyn std::error::Error>> {
        Self::ensure_positive_price("entry_price", entry_price)?;
        Self::ensure_positive_price("stop_price", stop_price)?;
        Self::ensure_positive_price("target_price", target_price)?;

        if quantity <= 0 {
            return Err(format!("quantity must be greater than 0, got {quantity}").into());
        }

        let quantity_decimal = Decimal::from(quantity);
        let side = Self::infer_side(entry_price, stop_price, target_price)?;

        let risk_per_share = match side {
            HypothesisSide::Long => {
                Self::checked_difference(entry_price, stop_price, "risk per share")?
            }
            HypothesisSide::Short => {
                Self::checked_difference(stop_price, entry_price, "risk per share")?
            }
        };
        let reward_per_share = match side {
            HypothesisSide::Long => {
                Self::checked_difference(target_price, entry_price, "reward per share")?
            }
            HypothesisSide::Short => {
                Self::checked_difference(entry_price, target_price, "reward per share")?
            }
        };
        let funding_price = Self::funding_price(entry_price, stop_price, side);

        let capital_required =
            Self::checked_multiply(funding_price, quantity_decimal, "capital required")?;
        let max_loss = Self::checked_multiply(risk_per_share, quantity_decimal, "maximum loss")?;
        let max_gain = Self::checked_multiply(reward_per_share, quantity_decimal, "maximum gain")?;

        let capital_required_pct_of_available =
            Self::percentage_of_available(capital_required, available_capital)?;
        let max_loss_pct_of_available = Self::percentage_of_available(max_loss, available_capital)?;
        let max_gain_pct_of_available = Self::percentage_of_available(max_gain, available_capital)?;
        let risk_reward_ratio = if risk_per_share > Decimal::ZERO {
            Some(
                reward_per_share
                    .checked_div(risk_per_share)
                    .ok_or("Division by zero or overflow while calculating risk/reward ratio")?,
            )
        } else {
            None
        };

        Ok(TradeHypothesis {
            available_capital,
            quantity,
            capital_required,
            capital_required_pct_of_available,
            risk_per_share,
            reward_per_share,
            max_loss,
            max_loss_pct_of_available,
            max_gain,
            max_gain_pct_of_available,
            risk_reward_ratio,
        })
    }

    fn ensure_positive_price(
        label: &str,
        price: Decimal,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if price <= Decimal::ZERO {
            return Err(format!("{label} must be greater than 0, got {price}").into());
        }
        Ok(())
    }

    fn ensure_account_exists(
        account_id: Uuid,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match database.account_read().id(account_id) {
            Ok(_) => Ok(()),
            Err(error) => {
                let message = error.to_string().to_ascii_lowercase();
                if message.contains("record not found") || message.contains("not found") {
                    Err(format!("Account not found: {account_id}").into())
                } else {
                    Err(error)
                }
            }
        }
    }

    fn checked_difference(
        left: Decimal,
        right: Decimal,
        label: &str,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        left.checked_sub(right)
            .ok_or_else(|| format!("Arithmetic overflow while calculating {label}").into())
    }

    fn checked_multiply(
        left: Decimal,
        right: Decimal,
        label: &str,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        left.checked_mul(right)
            .ok_or_else(|| format!("Arithmetic overflow while calculating {label}").into())
    }

    fn infer_side(
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
    ) -> Result<HypothesisSide, Box<dyn std::error::Error>> {
        use std::cmp::Ordering::{Equal, Greater, Less};

        match (stop_price.cmp(&entry_price), target_price.cmp(&entry_price)) {
            (Less, Greater) | (Less, Equal) | (Equal, Greater) => Ok(HypothesisSide::Long),
            (Greater, Less) | (Greater, Equal) | (Equal, Less) => Ok(HypothesisSide::Short),
            (Equal, Equal) => Err(format!(
                "trade hypothesis requires stop_price or target_price to differ from entry_price {entry_price}"
            )
            .into()),
            (Less, Less) | (Greater, Greater) => Err(format!(
                "trade hypothesis requires stop and target to imply a single trade side relative to entry_price {entry_price}; got stop_price {stop_price} and target_price {target_price}"
            )
            .into()),
        }
    }

    fn funding_price(entry_price: Decimal, stop_price: Decimal, side: HypothesisSide) -> Decimal {
        match side {
            HypothesisSide::Long => entry_price,
            HypothesisSide::Short => stop_price,
        }
    }

    fn percentage_of_available(
        amount: Decimal,
        available_capital: Decimal,
    ) -> Result<Option<Decimal>, Box<dyn std::error::Error>> {
        if available_capital <= Decimal::ZERO {
            return Ok(None);
        }

        let scaled = amount
            .checked_mul(dec!(100))
            .ok_or("Arithmetic overflow while calculating account impact percentage")?;
        let percentage = scaled
            .checked_div(available_capital)
            .ok_or("Division by zero or overflow while calculating account impact percentage")?;
        Ok(Some(percentage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db_sqlite::SqliteDatabase;

    #[test]
    fn test_calculate_trade_hypothesis_for_long_setup() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(10_000),
            dec!(40),
            dec!(38),
            dec!(48),
            100,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.available_capital, dec!(10_000));
        assert_eq!(result.quantity, 100);
        assert_eq!(result.capital_required, dec!(4_000));
        assert_eq!(result.capital_required_pct_of_available, Some(dec!(40)));
        assert_eq!(result.risk_per_share, dec!(2));
        assert_eq!(result.reward_per_share, dec!(8));
        assert_eq!(result.max_loss, dec!(200));
        assert_eq!(result.max_loss_pct_of_available, Some(dec!(2)));
        assert_eq!(result.max_gain, dec!(800));
        assert_eq!(result.max_gain_pct_of_available, Some(dec!(8)));
        assert_eq!(result.risk_reward_ratio, Some(dec!(4)));
    }

    #[test]
    fn test_calculate_trade_hypothesis_for_short_setup() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(5_000),
            dec!(50),
            dec!(55),
            dec!(40),
            10,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.capital_required, dec!(550));
        assert_eq!(result.capital_required_pct_of_available, Some(dec!(11)));
        assert_eq!(result.risk_per_share, dec!(5));
        assert_eq!(result.reward_per_share, dec!(10));
        assert_eq!(result.max_loss, dec!(50));
        assert_eq!(result.max_loss_pct_of_available, Some(dec!(1)));
        assert_eq!(result.max_gain, dec!(100));
        assert_eq!(result.max_gain_pct_of_available, Some(dec!(2)));
        assert_eq!(result.risk_reward_ratio, Some(dec!(2)));
    }

    #[test]
    fn test_funding_price_uses_stop_for_short_and_entry_for_long() {
        assert_eq!(
            TradeHypothesisCalculator::funding_price(dec!(40), dec!(38), HypothesisSide::Long),
            dec!(40)
        );
        assert_eq!(
            TradeHypothesisCalculator::funding_price(dec!(40), dec!(45), HypothesisSide::Short),
            dec!(45)
        );
    }

    #[test]
    fn test_infer_side_accepts_long_and_short_shapes_with_zero_reward_or_risk() {
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(38), dec!(48))
                .expect("long shape should infer"),
            HypothesisSide::Long
        );
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(40), dec!(48))
                .expect("zero-risk long shape should infer"),
            HypothesisSide::Long
        );
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(38), dec!(40))
                .expect("zero-reward long shape should infer"),
            HypothesisSide::Long
        );
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(45), dec!(35))
                .expect("short shape should infer"),
            HypothesisSide::Short
        );
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(45), dec!(40))
                .expect("zero-reward short shape should infer"),
            HypothesisSide::Short
        );
        assert_eq!(
            TradeHypothesisCalculator::infer_side(dec!(40), dec!(40), dec!(35))
                .expect("zero-risk short shape should infer"),
            HypothesisSide::Short
        );
    }

    #[test]
    fn test_infer_side_rejects_conflicting_or_flat_shapes() {
        let same_side_above = TradeHypothesisCalculator::infer_side(dec!(50), dec!(55), dec!(60))
            .expect_err("same-side-above prices should fail");
        assert_eq!(
            same_side_above.to_string(),
            "trade hypothesis requires stop and target to imply a single trade side relative to entry_price 50; got stop_price 55 and target_price 60"
        );

        let same_side_below = TradeHypothesisCalculator::infer_side(dec!(50), dec!(45), dec!(40))
            .expect_err("same-side-below prices should fail");
        assert_eq!(
            same_side_below.to_string(),
            "trade hypothesis requires stop and target to imply a single trade side relative to entry_price 50; got stop_price 45 and target_price 40"
        );

        let flat = TradeHypothesisCalculator::infer_side(dec!(50), dec!(50), dec!(50))
            .expect_err("flat prices should fail");
        assert_eq!(
            flat.to_string(),
            "trade hypothesis requires stop_price or target_price to differ from entry_price 50"
        );
    }

    #[test]
    fn test_calculate_trade_hypothesis_preserves_decimal_precision() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(50_000),
            dec!(40.17),
            dec!(39.62),
            dec!(42.92),
            17,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.capital_required, dec!(682.89));
        assert_eq!(
            result.capital_required_pct_of_available,
            Some(dec!(1.36578))
        );
        assert_eq!(result.risk_per_share, dec!(0.55));
        assert_eq!(result.reward_per_share, dec!(2.75));
        assert_eq!(result.max_loss, dec!(9.35));
        assert_eq!(result.max_loss_pct_of_available, Some(dec!(0.0187)));
        assert_eq!(result.max_gain, dec!(46.75));
        assert_eq!(result.max_gain_pct_of_available, Some(dec!(0.0935)));
        assert_eq!(result.risk_reward_ratio, Some(dec!(5)));
    }

    #[test]
    fn test_calculate_trade_hypothesis_with_zero_available_capital_returns_no_percentages() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            Decimal::ZERO,
            dec!(100),
            dec!(95),
            dec!(120),
            2,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.available_capital, Decimal::ZERO);
        assert_eq!(result.capital_required_pct_of_available, None);
        assert_eq!(result.max_loss_pct_of_available, None);
        assert_eq!(result.max_gain_pct_of_available, None);
    }

    #[test]
    fn test_calculate_trade_hypothesis_with_negative_available_capital_returns_no_percentages() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(-250),
            dec!(100),
            dec!(95),
            dec!(120),
            2,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.available_capital, dec!(-250));
        assert_eq!(result.capital_required_pct_of_available, None);
        assert_eq!(result.max_loss_pct_of_available, None);
        assert_eq!(result.max_gain_pct_of_available, None);
    }

    #[test]
    fn test_calculate_trade_hypothesis_allows_zero_risk_but_omits_ratio() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(25),
            dec!(25),
            dec!(30),
            4,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.risk_per_share, Decimal::ZERO);
        assert_eq!(result.max_loss, Decimal::ZERO);
        assert_eq!(result.risk_reward_ratio, None);
    }

    #[test]
    fn test_calculate_trade_hypothesis_allows_zero_reward_for_long_side() {
        let result = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(25),
            dec!(24),
            dec!(25),
            4,
        )
        .expect("hypothesis should calculate");

        assert_eq!(result.risk_per_share, dec!(1));
        assert_eq!(result.reward_per_share, Decimal::ZERO);
        assert_eq!(result.max_gain, Decimal::ZERO);
        assert_eq!(result.risk_reward_ratio, Some(Decimal::ZERO));
    }

    #[test]
    fn test_calculate_trade_hypothesis_rejects_non_positive_quantity() {
        let error = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(40),
            dec!(38),
            dec!(48),
            0,
        )
        .expect_err("zero quantity should fail");

        assert_eq!(error.to_string(), "quantity must be greater than 0, got 0");
    }

    #[test]
    fn test_calculate_trade_hypothesis_rejects_non_positive_prices() {
        let entry_error = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            Decimal::ZERO,
            dec!(38),
            dec!(48),
            1,
        )
        .expect_err("zero entry should fail");
        assert_eq!(
            entry_error.to_string(),
            "entry_price must be greater than 0, got 0"
        );

        let stop_error = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(40),
            dec!(-1),
            dec!(48),
            1,
        )
        .expect_err("negative stop should fail");
        assert_eq!(
            stop_error.to_string(),
            "stop_price must be greater than 0, got -1"
        );

        let target_error = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(40),
            dec!(38),
            Decimal::ZERO,
            1,
        )
        .expect_err("zero target should fail");
        assert_eq!(
            target_error.to_string(),
            "target_price must be greater than 0, got 0"
        );
    }

    #[test]
    fn test_calculate_trade_hypothesis_rejects_conflicting_direction_inputs() {
        let error = TradeHypothesisCalculator::calculate_with_available_capital(
            dec!(1_000),
            dec!(50),
            dec!(55),
            dec!(60),
            1,
        )
        .expect_err("conflicting direction should fail");

        assert_eq!(
            error.to_string(),
            "trade hypothesis requires stop and target to imply a single trade side relative to entry_price 50; got stop_price 55 and target_price 60"
        );
    }

    #[test]
    fn test_calculate_trade_hypothesis_propagates_capital_required_overflow() {
        let stop_price = Decimal::MAX
            .checked_sub(dec!(1))
            .expect("Decimal::MAX - 1 should be representable");
        let error = TradeHypothesisCalculator::calculate_with_available_capital(
            Decimal::ZERO,
            Decimal::MAX,
            stop_price,
            Decimal::MAX,
            2,
        )
        .expect_err("overflow should fail");

        assert_eq!(
            error.to_string(),
            "Arithmetic overflow while calculating capital required"
        );
    }

    #[test]
    fn test_checked_difference_propagates_overflow() {
        let error = TradeHypothesisCalculator::checked_difference(
            Decimal::MIN,
            Decimal::MAX,
            "risk per share",
        )
        .expect_err("overflow should fail");

        assert_eq!(
            error.to_string(),
            "Arithmetic overflow while calculating risk per share"
        );
    }

    #[test]
    fn test_checked_multiply_propagates_overflow() {
        let error =
            TradeHypothesisCalculator::checked_multiply(Decimal::MAX, dec!(2), "maximum loss")
                .expect_err("overflow should fail");

        assert_eq!(
            error.to_string(),
            "Arithmetic overflow while calculating maximum loss"
        );
    }

    #[test]
    fn test_percentage_of_available_propagates_overflow() {
        let error = TradeHypothesisCalculator::percentage_of_available(Decimal::MAX, dec!(1))
            .expect_err("overflow should fail");

        assert_eq!(
            error.to_string(),
            "Arithmetic overflow while calculating account impact percentage"
        );
    }

    #[test]
    fn test_calculate_trade_hypothesis_rejects_unknown_account() {
        let mut database = SqliteDatabase::new_in_memory();
        let account_id = Uuid::new_v4();

        let error = TradeHypothesisCalculator::calculate(
            account_id,
            dec!(40),
            dec!(38),
            dec!(48),
            1,
            &Currency::USD,
            &mut database,
        )
        .expect_err("unknown account should fail");

        assert_eq!(
            error.to_string(),
            format!("Account not found: {account_id}")
        );
    }
}
