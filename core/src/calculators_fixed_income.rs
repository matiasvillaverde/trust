//! Fixed-income analytics using Decimal-only arithmetic.
//!
//! These calculations intentionally use transparent approximations rather than
//! iterative floating-point pricing models. They are suitable for CLI previews,
//! risk review, and position comparison; execution-grade bond pricing should
//! still come from broker or market-data sources.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// Input for a plain-vanilla bond position analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct BondAnalyticsInput {
    /// Bond face/par value per unit.
    pub face_value: Decimal,
    /// Current clean market price per unit.
    pub market_price: Decimal,
    /// Annual coupon rate as a percentage of face value, e.g. `5` for 5%.
    pub annual_coupon_rate_pct: Decimal,
    /// Number of bond units held or proposed.
    pub quantity: i64,
    /// Years remaining until maturity. Zero is allowed, but YTM will be unavailable.
    pub years_to_maturity: Decimal,
    /// Optional accrued-interest schedule inputs.
    pub accrued_interest: Option<BondAccruedInterestInput>,
}

/// Inputs needed to calculate accrued interest for a bond settlement.
#[derive(Debug, Clone, PartialEq)]
pub struct BondAccruedInterestInput {
    /// Settlement date for the position analysis.
    pub settlement_date: NaiveDate,
    /// Previous coupon payment date.
    pub last_coupon_date: NaiveDate,
    /// Next coupon payment date.
    pub next_coupon_date: NaiveDate,
    /// Number of coupon payments per year.
    pub coupon_frequency_per_year: u16,
    /// Day-count basis for accrued interest.
    pub day_count_basis: DayCountBasis,
}

/// Supported day-count bases for accrued-interest estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayCountBasis {
    /// Actual days elapsed over actual days in the coupon period.
    ActualActual,
    /// Actual days elapsed over a 360-day year.
    Actual360,
    /// Actual days elapsed over a 365-day year.
    Actual365,
}

impl Display for DayCountBasis {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActualActual => f.write_str("actual-actual"),
            Self::Actual360 => f.write_str("actual-360"),
            Self::Actual365 => f.write_str("actual-365"),
        }
    }
}

/// Error returned when parsing a day-count basis fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayCountBasisParseError;

impl FromStr for DayCountBasis {
    type Err = DayCountBasisParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "actual-actual" | "actual/actual" | "act-act" | "act/act" => Ok(Self::ActualActual),
            "actual-360" | "actual/360" | "act-360" | "act/360" => Ok(Self::Actual360),
            "actual-365" | "actual/365" | "act-365" | "act/365" => Ok(Self::Actual365),
            _ => Err(DayCountBasisParseError),
        }
    }
}

/// Decimal-only bond position analytics.
#[derive(Debug, Clone, PartialEq)]
pub struct BondAnalytics {
    /// Total face value across the position.
    pub position_face_value: Decimal,
    /// Total market value across the position.
    pub position_market_value: Decimal,
    /// Annual coupon payment per unit.
    pub annual_coupon_per_unit: Decimal,
    /// Annual coupon income across the position.
    pub annual_coupon_income: Decimal,
    /// Current yield as a percentage of market price.
    pub current_yield_pct: Decimal,
    /// Approximate simple yield to maturity percentage.
    pub approximate_yield_to_maturity_pct: Option<Decimal>,
    /// Difference between market price and face value per unit.
    pub price_premium_discount: Decimal,
    /// Premium/discount as a percentage of face value.
    pub price_premium_discount_pct: Decimal,
    /// Accrued interest per unit since the last coupon date.
    pub accrued_interest_per_unit: Decimal,
    /// Accrued interest across the position.
    pub accrued_interest_total: Decimal,
    /// Clean price plus accrued interest.
    pub dirty_price: Decimal,
    /// Dirty position value across the position.
    pub position_dirty_value: Decimal,
}

/// Errors returned by fixed-income analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedIncomeError {
    /// Face/par value must be greater than zero.
    InvalidFaceValue,
    /// Market price must be greater than zero.
    InvalidMarketPrice,
    /// Coupon rate must be zero or greater.
    InvalidCouponRate,
    /// Quantity must be greater than zero.
    InvalidQuantity,
    /// Years to maturity must be zero or greater.
    InvalidYearsToMaturity,
    /// Coupon frequency must be greater than zero when accrued interest is requested.
    InvalidCouponFrequency,
    /// Coupon schedule dates must satisfy last <= settlement <= next and last < next.
    InvalidCouponSchedule,
    /// A checked Decimal operation overflowed.
    ArithmeticOverflow,
}

impl Display for FixedIncomeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFaceValue => f.write_str("face value must be greater than zero"),
            Self::InvalidMarketPrice => f.write_str("market price must be greater than zero"),
            Self::InvalidCouponRate => f.write_str("coupon rate must be zero or greater"),
            Self::InvalidQuantity => f.write_str("quantity must be greater than zero"),
            Self::InvalidYearsToMaturity => {
                f.write_str("years to maturity must be zero or greater")
            }
            Self::InvalidCouponFrequency => f.write_str("coupon frequency must be greater than zero"),
            Self::InvalidCouponSchedule => f.write_str(
                "coupon schedule must satisfy last_coupon_date <= settlement_date <= next_coupon_date and last_coupon_date < next_coupon_date",
            ),
            Self::ArithmeticOverflow => f.write_str("arithmetic overflow in bond analytics"),
        }
    }
}

impl Error for FixedIncomeError {}

/// Calculator for fixed-income position analytics.
#[derive(Debug)]
pub struct FixedIncomeCalculator;

impl FixedIncomeCalculator {
    /// Analyze a plain-vanilla bond position.
    pub fn analyze_bond(input: BondAnalyticsInput) -> Result<BondAnalytics, FixedIncomeError> {
        validate_input(&input)?;

        let quantity = Decimal::from(input.quantity);
        let coupon_rate = pct_to_ratio(input.annual_coupon_rate_pct)?;
        let annual_coupon_per_unit = checked_mul(input.face_value, coupon_rate)?;
        let annual_coupon_income = checked_mul(annual_coupon_per_unit, quantity)?;
        let position_face_value = checked_mul(input.face_value, quantity)?;
        let position_market_value = checked_mul(input.market_price, quantity)?;
        let current_yield_pct =
            ratio_to_pct(checked_div(annual_coupon_per_unit, input.market_price)?)?;
        let accrued_interest_per_unit = accrued_interest_per_unit(&input, annual_coupon_per_unit)?;
        let accrued_interest_total = checked_mul(accrued_interest_per_unit, quantity)?;
        let dirty_price = checked_add(input.market_price, accrued_interest_per_unit)?;
        let position_dirty_value = checked_mul(dirty_price, quantity)?;
        let price_premium_discount = checked_sub(input.market_price, input.face_value)?;
        let price_premium_discount_pct =
            ratio_to_pct(checked_div(price_premium_discount, input.face_value)?)?;
        let approximate_yield_to_maturity_pct =
            approximate_ytm_pct(&input, annual_coupon_per_unit)?;

        Ok(BondAnalytics {
            position_face_value,
            position_market_value,
            annual_coupon_per_unit,
            annual_coupon_income,
            current_yield_pct,
            approximate_yield_to_maturity_pct,
            price_premium_discount,
            price_premium_discount_pct,
            accrued_interest_per_unit,
            accrued_interest_total,
            dirty_price,
            position_dirty_value,
        })
    }
}

fn validate_input(input: &BondAnalyticsInput) -> Result<(), FixedIncomeError> {
    if input.face_value <= Decimal::ZERO {
        return Err(FixedIncomeError::InvalidFaceValue);
    }
    if input.market_price <= Decimal::ZERO {
        return Err(FixedIncomeError::InvalidMarketPrice);
    }
    if input.annual_coupon_rate_pct < Decimal::ZERO {
        return Err(FixedIncomeError::InvalidCouponRate);
    }
    if input.quantity <= 0 {
        return Err(FixedIncomeError::InvalidQuantity);
    }
    if input.years_to_maturity < Decimal::ZERO {
        return Err(FixedIncomeError::InvalidYearsToMaturity);
    }
    if let Some(accrual) = &input.accrued_interest {
        validate_accrued_interest_input(accrual)?;
    }
    Ok(())
}

fn validate_accrued_interest_input(
    input: &BondAccruedInterestInput,
) -> Result<(), FixedIncomeError> {
    if input.coupon_frequency_per_year == 0 {
        return Err(FixedIncomeError::InvalidCouponFrequency);
    }
    if input.last_coupon_date >= input.next_coupon_date
        || input.settlement_date < input.last_coupon_date
        || input.settlement_date > input.next_coupon_date
    {
        return Err(FixedIncomeError::InvalidCouponSchedule);
    }
    Ok(())
}

fn accrued_interest_per_unit(
    input: &BondAnalyticsInput,
    annual_coupon_per_unit: Decimal,
) -> Result<Decimal, FixedIncomeError> {
    let Some(accrual) = &input.accrued_interest else {
        return Ok(Decimal::ZERO);
    };
    let elapsed_days = days_between(accrual.last_coupon_date, accrual.settlement_date)?;

    match accrual.day_count_basis {
        DayCountBasis::ActualActual => {
            let coupon_frequency = Decimal::from(accrual.coupon_frequency_per_year);
            let coupon_per_period = checked_div(annual_coupon_per_unit, coupon_frequency)?;
            let period_days = days_between(accrual.last_coupon_date, accrual.next_coupon_date)?;
            checked_mul(coupon_per_period, checked_div(elapsed_days, period_days)?)
        }
        DayCountBasis::Actual360 => checked_mul(
            annual_coupon_per_unit,
            checked_div(elapsed_days, dec!(360))?,
        ),
        DayCountBasis::Actual365 => checked_mul(
            annual_coupon_per_unit,
            checked_div(elapsed_days, dec!(365))?,
        ),
    }
}

fn days_between(start: NaiveDate, end: NaiveDate) -> Result<Decimal, FixedIncomeError> {
    let days = end.signed_duration_since(start).num_days();
    if days < 0 {
        return Err(FixedIncomeError::InvalidCouponSchedule);
    }
    Ok(Decimal::from(days))
}

fn approximate_ytm_pct(
    input: &BondAnalyticsInput,
    annual_coupon_per_unit: Decimal,
) -> Result<Option<Decimal>, FixedIncomeError> {
    if input.years_to_maturity == Decimal::ZERO {
        return Ok(None);
    }

    let redemption_gain = checked_sub(input.face_value, input.market_price)?;
    let annualized_gain = checked_div(redemption_gain, input.years_to_maturity)?;
    let numerator = checked_add(annual_coupon_per_unit, annualized_gain)?;
    let average_capital_base =
        checked_div(checked_add(input.face_value, input.market_price)?, dec!(2))?;
    let ytm_ratio = checked_div(numerator, average_capital_base)?;
    let ytm_pct = ratio_to_pct(ytm_ratio)?;

    Ok(Some(ytm_pct))
}

fn pct_to_ratio(value: Decimal) -> Result<Decimal, FixedIncomeError> {
    checked_div(value, dec!(100))
}

fn ratio_to_pct(value: Decimal) -> Result<Decimal, FixedIncomeError> {
    checked_mul(value, dec!(100))
}

fn checked_add(left: Decimal, right: Decimal) -> Result<Decimal, FixedIncomeError> {
    left.checked_add(right)
        .ok_or(FixedIncomeError::ArithmeticOverflow)
}

fn checked_sub(left: Decimal, right: Decimal) -> Result<Decimal, FixedIncomeError> {
    left.checked_sub(right)
        .ok_or(FixedIncomeError::ArithmeticOverflow)
}

fn checked_mul(left: Decimal, right: Decimal) -> Result<Decimal, FixedIncomeError> {
    left.checked_mul(right)
        .ok_or(FixedIncomeError::ArithmeticOverflow)
}

fn checked_div(left: Decimal, right: Decimal) -> Result<Decimal, FixedIncomeError> {
    left.checked_div(right)
        .ok_or(FixedIncomeError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_bond_returns_coupon_income_current_yield_and_ytm() {
        let analytics = FixedIncomeCalculator::analyze_bond(BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1000),
            annual_coupon_rate_pct: dec!(5),
            quantity: 10.into(),
            years_to_maturity: dec!(5),
            accrued_interest: None,
        })
        .expect("valid par bond");

        assert_eq!(analytics.position_face_value, dec!(10000));
        assert_eq!(analytics.position_market_value, dec!(10000));
        assert_eq!(analytics.annual_coupon_per_unit, dec!(50));
        assert_eq!(analytics.annual_coupon_income, dec!(500));
        assert_eq!(analytics.current_yield_pct, dec!(5));
        assert_eq!(analytics.approximate_yield_to_maturity_pct, Some(dec!(5)));
        assert_eq!(analytics.price_premium_discount, dec!(0));
        assert_eq!(analytics.price_premium_discount_pct, dec!(0));
        assert_eq!(analytics.accrued_interest_per_unit, Decimal::ZERO);
        assert_eq!(analytics.dirty_price, dec!(1000));
        assert_eq!(analytics.position_dirty_value, dec!(10000));
    }

    #[test]
    fn discount_bond_includes_annualized_pull_to_par_in_ytm() {
        let analytics = FixedIncomeCalculator::analyze_bond(BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(950),
            annual_coupon_rate_pct: dec!(4),
            quantity: 3.into(),
            years_to_maturity: dec!(5),
            accrued_interest: None,
        })
        .expect("valid discount bond");

        assert_eq!(analytics.position_market_value, dec!(2850));
        assert_eq!(analytics.annual_coupon_income, dec!(120));
        assert_eq!(analytics.current_yield_pct.round_dp(4), dec!(4.2105));
        assert_eq!(
            analytics
                .approximate_yield_to_maturity_pct
                .map(|value| value.round_dp(4)),
            Some(dec!(5.1282))
        );
        assert_eq!(analytics.price_premium_discount, dec!(-50));
        assert_eq!(analytics.price_premium_discount_pct, dec!(-5));
    }

    #[test]
    fn matured_bond_has_no_ytm_but_still_reports_coupon_income() {
        let analytics = FixedIncomeCalculator::analyze_bond(BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1001),
            annual_coupon_rate_pct: dec!(3.5),
            quantity: 2.into(),
            years_to_maturity: Decimal::ZERO,
            accrued_interest: None,
        })
        .expect("valid matured bond preview");

        assert_eq!(analytics.annual_coupon_income, dec!(70.0));
        assert_eq!(analytics.approximate_yield_to_maturity_pct, None);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let valid = BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1000),
            annual_coupon_rate_pct: dec!(5),
            quantity: 1.into(),
            years_to_maturity: dec!(1),
            accrued_interest: None,
        };

        let mut invalid = valid.clone();
        invalid.face_value = Decimal::ZERO;
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidFaceValue)
        );

        let mut invalid = valid.clone();
        invalid.market_price = Decimal::ZERO;
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidMarketPrice)
        );

        let mut invalid = valid.clone();
        invalid.annual_coupon_rate_pct = dec!(-1);
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidCouponRate)
        );

        let mut invalid = valid.clone();
        invalid.quantity = 0.into();
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidQuantity)
        );

        let mut invalid = valid;
        invalid.years_to_maturity = dec!(-0.1);
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidYearsToMaturity)
        );
    }

    #[test]
    fn accrued_interest_adds_dirty_price_and_position_value() {
        let analytics = FixedIncomeCalculator::analyze_bond(BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1000),
            annual_coupon_rate_pct: dec!(6),
            quantity: 10.into(),
            years_to_maturity: dec!(5),
            accrued_interest: Some(BondAccruedInterestInput {
                settlement_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                last_coupon_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                next_coupon_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                coupon_frequency_per_year: 2,
                day_count_basis: DayCountBasis::Actual360,
            }),
        })
        .expect("valid accrued interest inputs");

        assert_eq!(analytics.accrued_interest_per_unit, dec!(15.0));
        assert_eq!(analytics.accrued_interest_total, dec!(150.0));
        assert_eq!(analytics.dirty_price, dec!(1015.0));
        assert_eq!(analytics.position_dirty_value, dec!(10150.0));
    }

    #[test]
    fn day_count_basis_display_and_aliases_are_stable() {
        assert_eq!(DayCountBasis::ActualActual.to_string(), "actual-actual");
        assert_eq!(DayCountBasis::Actual360.to_string(), "actual-360");
        assert_eq!(DayCountBasis::Actual365.to_string(), "actual-365");

        let aliases = [
            ("actual-actual", DayCountBasis::ActualActual),
            ("actual/actual", DayCountBasis::ActualActual),
            (" act-act ", DayCountBasis::ActualActual),
            ("act/act", DayCountBasis::ActualActual),
            ("actual-360", DayCountBasis::Actual360),
            ("actual/360", DayCountBasis::Actual360),
            ("ACT-360", DayCountBasis::Actual360),
            ("act/360", DayCountBasis::Actual360),
            ("actual-365", DayCountBasis::Actual365),
            ("actual/365", DayCountBasis::Actual365),
            ("act-365", DayCountBasis::Actual365),
            ("act/365", DayCountBasis::Actual365),
        ];

        for (raw, basis) in aliases {
            assert_eq!(DayCountBasis::from_str(raw), Ok(basis));
        }

        assert_eq!(
            DayCountBasis::from_str("30/360"),
            Err(DayCountBasisParseError)
        );
    }

    #[test]
    fn days_between_rejects_reversed_ranges() {
        assert_eq!(
            days_between(
                NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            ),
            Err(FixedIncomeError::InvalidCouponSchedule)
        );
    }

    #[test]
    fn approximate_ytm_helper_returns_percent_for_premium_bond() {
        let input = BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1050),
            annual_coupon_rate_pct: dec!(5),
            quantity: 1.into(),
            years_to_maturity: dec!(5),
            accrued_interest: None,
        };

        let ytm = approximate_ytm_pct(&input, dec!(50))
            .expect("ytm should calculate")
            .expect("non-matured bond should have ytm");

        assert_eq!(ytm.round_dp(4), dec!(3.9024));
    }

    #[test]
    fn accrued_interest_supports_actual_actual_and_actual_365() {
        let base = BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(990),
            annual_coupon_rate_pct: dec!(6),
            quantity: 2.into(),
            years_to_maturity: dec!(4),
            accrued_interest: Some(BondAccruedInterestInput {
                settlement_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                last_coupon_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                next_coupon_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                coupon_frequency_per_year: 2,
                day_count_basis: DayCountBasis::ActualActual,
            }),
        };

        let actual_actual =
            FixedIncomeCalculator::analyze_bond(base.clone()).expect("valid actual/actual accrual");
        assert_eq!(
            actual_actual.accrued_interest_per_unit.round_dp(4),
            dec!(14.9171)
        );

        let mut actual_365 = base;
        if let Some(accrual) = actual_365.accrued_interest.as_mut() {
            accrual.day_count_basis = DayCountBasis::Actual365;
        }
        let actual_365 =
            FixedIncomeCalculator::analyze_bond(actual_365).expect("valid actual/365 accrual");
        assert_eq!(
            actual_365.accrued_interest_per_unit.round_dp(4),
            dec!(14.7945)
        );
    }

    #[test]
    fn accrued_interest_rejects_invalid_schedule_and_frequency() {
        let valid = BondAnalyticsInput {
            face_value: dec!(1000),
            market_price: dec!(1000),
            annual_coupon_rate_pct: dec!(6),
            quantity: 1.into(),
            years_to_maturity: dec!(5),
            accrued_interest: Some(BondAccruedInterestInput {
                settlement_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                last_coupon_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                next_coupon_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                coupon_frequency_per_year: 2,
                day_count_basis: DayCountBasis::ActualActual,
            }),
        };

        let mut invalid = valid.clone();
        if let Some(accrual) = invalid.accrued_interest.as_mut() {
            accrual.coupon_frequency_per_year = 0;
        }
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidCouponFrequency)
        );

        let mut invalid = valid.clone();
        if let Some(accrual) = invalid.accrued_interest.as_mut() {
            accrual.settlement_date = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();
        }
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidCouponSchedule)
        );

        let mut invalid = valid;
        if let Some(accrual) = invalid.accrued_interest.as_mut() {
            accrual.last_coupon_date = accrual.next_coupon_date;
        }
        assert_eq!(
            FixedIncomeCalculator::analyze_bond(invalid),
            Err(FixedIncomeError::InvalidCouponSchedule)
        );
    }

    #[test]
    fn fixed_income_errors_have_operator_facing_messages() {
        let cases = [
            (FixedIncomeError::InvalidFaceValue, "face value"),
            (FixedIncomeError::InvalidMarketPrice, "market price"),
            (FixedIncomeError::InvalidCouponRate, "coupon rate"),
            (FixedIncomeError::InvalidQuantity, "quantity"),
            (
                FixedIncomeError::InvalidYearsToMaturity,
                "years to maturity",
            ),
            (FixedIncomeError::InvalidCouponFrequency, "coupon frequency"),
            (FixedIncomeError::InvalidCouponSchedule, "coupon schedule"),
            (FixedIncomeError::ArithmeticOverflow, "arithmetic overflow"),
        ];

        for (error, expected) in cases {
            assert!(
                error.to_string().contains(expected),
                "{error:?} should mention {expected}"
            );
        }
    }
}
