use model::{Currency, DatabaseFactory, RuleName};
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;
use std::str::FromStr;
use uuid::Uuid;

use crate::calculators_account::AccountCapitalAvailable;
use crate::calculators_trade::RiskCalculator;

pub struct QuantityCalculator;

const MAX_QUANTITY_DECIMAL_PLACES: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct LevelAdjustedQuantity {
    pub base_quantity: Decimal,
    pub level_multiplier: Decimal,
    pub final_quantity: Decimal,
}

impl QuantityCalculator {
    pub fn maximum_quantity(
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let total_available = AccountCapitalAvailable::calculate(
            account_id,
            currency,
            database.transaction_read().as_mut(),
        )?;

        // Get rules by priority
        let mut rules = database.rule_read().read_all_rules(account_id)?;
        rules.sort_by_key(|rule| rule.priority);

        let mut risk_per_month = dec!(100.0); // Default to 100% of the available capital

        // match rules by name
        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(risk) => {
                    risk_per_month =
                        RiskCalculator::calculate_max_percentage_to_risk_current_month(
                            risk, account_id, currency, database,
                        )?;
                }
                RuleName::RiskPerTrade(risk) => {
                    let risk_decimal = Decimal::from_f32_retain(risk)
                        .ok_or_else(|| format!("Failed to convert risk {risk} to Decimal"))?;
                    if risk_per_month < risk_decimal {
                        return Ok(Decimal::ZERO); // No capital to risk this month, so quantity is 0. AKA: No trade.
                    } else {
                        let risk_per_trade = QuantityCalculator::max_quantity_per_trade(
                            total_available,
                            entry_price,
                            stop_price,
                            risk,
                        );
                        return Ok(risk_per_trade);
                    }
                }
            }
        }

        // If there are no rules, return the maximum quantity based on available funds
        if entry_price <= dec!(0.0) {
            return Err(format!(
                "Invalid entry price {entry_price} for quantity calculation (must be greater than 0)"
            )
            .into());
        }
        if total_available <= dec!(0.0) {
            return Ok(Decimal::ZERO);
        }
        let max_quantity = total_available.checked_div(entry_price).ok_or_else(|| {
            format!("Division by zero or overflow: {total_available} / {entry_price}")
        })?;
        let max_quantity = if max_quantity < Decimal::ZERO {
            Decimal::ZERO
        } else {
            max_quantity
        };
        Ok(Self::conservative_quantity(max_quantity))
    }

    pub fn maximum_quantity_with_level(
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<LevelAdjustedQuantity, Box<dyn std::error::Error>> {
        let base_quantity =
            Self::maximum_quantity(account_id, entry_price, stop_price, currency, database)?;
        let level = database.level_read().level_for_account(account_id)?;
        let final_quantity =
            Self::apply_multiplier_to_quantity(base_quantity, level.risk_multiplier);

        Ok(LevelAdjustedQuantity {
            base_quantity,
            level_multiplier: level.risk_multiplier,
            final_quantity,
        })
    }

    fn apply_multiplier_to_quantity(quantity: Decimal, multiplier: Decimal) -> Decimal {
        let adjusted = match quantity.checked_mul(multiplier) {
            Some(value) => value,
            None => return Decimal::ZERO,
        };
        Self::conservative_quantity(adjusted)
    }

    fn conservative_quantity(quantity: Decimal) -> Decimal {
        quantity
            .max(Decimal::ZERO)
            .round_dp_with_strategy(MAX_QUANTITY_DECIMAL_PLACES, RoundingStrategy::ToZero)
    }

    fn max_quantity_per_trade(
        available: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        risk: f32,
    ) -> Decimal {
        if available <= dec!(0.0) {
            return Decimal::ZERO;
        }

        let Some(raw_price_diff) = entry_price.checked_sub(stop_price) else {
            return Decimal::ZERO; // Entry price must be greater than stop price
        };
        let price_diff = if raw_price_diff < dec!(0) {
            raw_price_diff
                .checked_mul(dec!(-1))
                .unwrap_or(Decimal::ZERO)
        } else {
            raw_price_diff
        };

        if price_diff <= dec!(0.0) || !risk.is_finite() || risk <= 0.0 {
            return Decimal::ZERO;
        }

        let Some(max_quantity) = available.checked_div(entry_price) else {
            return Decimal::ZERO; // Division overflow
        };

        let Some(max_risk) = max_quantity.checked_mul(price_diff) else {
            return Decimal::ZERO; // Multiplication overflow
        };

        let Ok(risk_decimal) = Decimal::from_str(&risk.to_string()) else {
            return Decimal::ZERO; // Failed to convert risk to Decimal
        };

        let Some(risk_percent) = risk_decimal.checked_div(dec!(100.0)) else {
            return Decimal::ZERO; // Division overflow
        };

        let Some(risk_capital) = available.checked_mul(risk_percent) else {
            return Decimal::ZERO; // Multiplication overflow
        };

        if risk_capital >= max_risk {
            // The risk capital is greater than the max risk, so return the max quantity
            Self::conservative_quantity(max_quantity)
        } else {
            // The risk capital is less than the max risk, so return the max quantity based on the risk capital
            let Some(risk_per_trade) = risk_capital.checked_div(price_diff) else {
                return Decimal::ZERO; // Division overflow
            };
            Self::conservative_quantity(risk_per_trade)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db_sqlite::SqliteDatabase;
    use model::{Account, AccountType, Environment, RuleLevel};

    fn create_test_account(database: &mut SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create_with_hierarchy(
                name,
                name,
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Primary,
                None,
            )
            .expect("account should be created")
    }

    fn create_test_transaction(
        database: &mut SqliteDatabase,
        account: &Account,
        amount: Decimal,
        category: model::TransactionCategory,
    ) {
        database
            .transaction_write()
            .create_transaction(account, amount, &Currency::USD, category)
            .expect("transaction should be created");
    }

    fn create_test_deposit(database: &mut SqliteDatabase, account: &Account, amount: Decimal) {
        create_test_transaction(
            database,
            account,
            amount,
            model::TransactionCategory::Deposit,
        );
    }

    fn create_test_rule(
        database: &mut SqliteDatabase,
        account: &Account,
        name: RuleName,
        priority: u32,
    ) {
        database
            .rule_write()
            .create_rule(
                account,
                &name,
                "quantity test rule",
                priority,
                &RuleLevel::Error,
            )
            .expect("rule should be created");
    }

    #[test]
    fn test_max_quantity_per_trade_default() {
        // Test case 1: The trade risk is within the available funds
        let available = dec!(10_000);
        let entry_price = dec!(50);
        let stop_price = dec!(45);
        let risk = 2.0; // 2% risk

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            dec!(40)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_low_risk() {
        // Test case 2: The trade risk is greater than the available funds
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 0.1;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            dec!(1)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_high_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 90.0;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            dec!(100)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_max_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 100.0;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            dec!(100)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_less_than_maximum_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 9.99;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            dec!(99.9)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_rounds_down_to_fundable_decimal_quantity() {
        let quantity = QuantityCalculator::max_quantity_per_trade(
            dec!(169_940),
            dec!(169),
            dec!(151.7113),
            1.0,
        );

        assert_eq!(quantity, dec!(98.29541839));
        assert!(
            quantity
                .checked_mul(dec!(17.2887))
                .expect("risk should fit")
                <= dec!(1699.40)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_rejects_non_positive_inputs() {
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(0), dec!(100), dec!(90), 2.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(100), dec!(100), 2.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(100), dec!(90), 0.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(100), dec!(90), -1.0),
            dec!(0)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_uses_absolute_stop_distance() {
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(90), dec!(100), 2.0),
            dec!(20)
        );
    }

    #[test]
    fn test_max_quantity_per_trade_returns_zero_for_decimal_overflow_paths() {
        let capped_by_risk =
            QuantityCalculator::max_quantity_per_trade(Decimal::MAX, dec!(1), dec!(0), 2.0);
        assert_eq!(
            capped_by_risk,
            Decimal::MAX
                .checked_mul(dec!(0.02))
                .expect("risk cap should fit")
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(0), dec!(-1), 2.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(
                dec!(10_000),
                Decimal::MAX,
                Decimal::MIN,
                2.0,
            ),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(Decimal::MAX, dec!(1), dec!(-1), 2.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(Decimal::MAX, dec!(1), dec!(0), 200.0),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(dec!(10_000), dec!(100), dec!(90), f32::NAN),
            dec!(0)
        );
    }

    #[test]
    fn test_apply_multiplier_to_quantity_preserves_fractional_results() {
        assert_eq!(
            QuantityCalculator::apply_multiplier_to_quantity(dec!(101), dec!(0.5)),
            dec!(50.5)
        );
        assert_eq!(
            QuantityCalculator::apply_multiplier_to_quantity(dec!(101), dec!(1.5)),
            dec!(151.5)
        );
    }

    #[test]
    fn test_apply_multiplier_to_quantity_saturates_invalid_results_to_zero() {
        assert_eq!(
            QuantityCalculator::apply_multiplier_to_quantity(Decimal::MAX, Decimal::MAX),
            dec!(0)
        );
        assert_eq!(
            QuantityCalculator::apply_multiplier_to_quantity(dec!(100), dec!(-1.5)),
            dec!(0)
        );
    }

    #[test]
    fn test_maximum_quantity_without_rules_uses_available_capital() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-no-rules");
        create_test_deposit(&mut database, &account, dec!(1_000));

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(250),
            dec!(200),
            &Currency::USD,
            &mut database,
        )
        .expect("quantity should calculate");

        assert_eq!(quantity, dec!(4));
    }

    #[test]
    fn test_maximum_quantity_without_rules_rejects_invalid_entry_price() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-invalid-entry");

        let error = QuantityCalculator::maximum_quantity(
            account.id,
            Decimal::ZERO,
            dec!(200),
            &Currency::USD,
            &mut database,
        )
        .expect_err("invalid entry should fail");

        assert_eq!(
            error.to_string(),
            "Invalid entry price 0 for quantity calculation (must be greater than 0)"
        );
    }

    #[test]
    fn test_maximum_quantity_without_rules_returns_zero_when_no_capital_is_available() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-no-capital");

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(250),
            dec!(200),
            &Currency::USD,
            &mut database,
        )
        .expect("quantity should calculate");

        assert_eq!(quantity, dec!(0));
    }

    #[test]
    fn test_maximum_quantity_without_rules_reports_division_overflow() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-division-overflow");
        create_test_deposit(&mut database, &account, Decimal::MAX);

        let error = QuantityCalculator::maximum_quantity(
            account.id,
            Decimal::new(1, 28),
            Decimal::ZERO,
            &Currency::USD,
            &mut database,
        )
        .expect_err("overflowing division should fail");

        assert!(error.to_string().contains("Division by zero or overflow"));
    }

    #[test]
    fn test_maximum_quantity_without_rules_preserves_fractional_quantity() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-fractional");
        create_test_deposit(&mut database, &account, dec!(100));

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(40000),
            Decimal::ZERO,
            &Currency::USD,
            &mut database,
        )
        .expect("fractional quantity should calculate");

        assert_eq!(quantity, dec!(0.0025));
    }

    #[test]
    fn test_maximum_quantity_propagates_available_capital_errors() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-negative-capital");
        create_test_transaction(
            &mut database,
            &account,
            dec!(1),
            model::TransactionCategory::Withdrawal,
        );

        let error = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(250),
            dec!(200),
            &Currency::USD,
            &mut database,
        )
        .expect_err("negative available capital should fail");

        assert_eq!(
            error.to_string(),
            "capital_available: total available is negative: -1"
        );
    }

    #[test]
    fn test_maximum_quantity_with_risk_per_trade_rule_limits_size() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-risk-rule");
        create_test_deposit(&mut database, &account, dec!(10_000));
        create_test_rule(&mut database, &account, RuleName::RiskPerTrade(2.0), 1);

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(100),
            dec!(90),
            &Currency::USD,
            &mut database,
        )
        .expect("quantity should calculate");

        assert_eq!(quantity, dec!(20));
    }

    #[test]
    fn test_maximum_quantity_applies_risk_per_month_before_trade_risk() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-month-risk-rule");
        create_test_deposit(&mut database, &account, dec!(10_000));
        create_test_rule(&mut database, &account, RuleName::RiskPerMonth(1.0), 1);
        create_test_rule(&mut database, &account, RuleName::RiskPerTrade(2.0), 2);

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(100),
            dec!(90),
            &Currency::USD,
            &mut database,
        )
        .expect("quantity should calculate");

        assert_eq!(quantity, dec!(0));
    }

    #[test]
    fn test_maximum_quantity_returns_zero_when_trade_risk_exceeds_monthly_allowance() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-monthly-allowance");
        create_test_deposit(&mut database, &account, dec!(10_000));
        create_test_rule(&mut database, &account, RuleName::RiskPerTrade(101.0), 1);

        let quantity = QuantityCalculator::maximum_quantity(
            account.id,
            dec!(100),
            dec!(90),
            &Currency::USD,
            &mut database,
        )
        .expect("quantity should calculate");

        assert_eq!(quantity, dec!(0));
    }

    #[test]
    fn test_maximum_quantity_with_level_applies_persisted_multiplier() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "quantity-level");
        create_test_deposit(&mut database, &account, dec!(1_000));

        let mut level = database
            .level_write()
            .create_default_level(&account)
            .expect("level should be created");
        level.current_level = 4;
        level.risk_multiplier = dec!(1.50);
        database
            .level_write()
            .update_level(&level)
            .expect("level should be updated");

        let quantity = QuantityCalculator::maximum_quantity_with_level(
            account.id,
            dec!(250),
            dec!(200),
            &Currency::USD,
            &mut database,
        )
        .expect("level-adjusted quantity should calculate");

        assert_eq!(
            quantity,
            LevelAdjustedQuantity {
                base_quantity: 4.into(),
                level_multiplier: dec!(1.5),
                final_quantity: 6.into(),
            }
        );
    }
}
