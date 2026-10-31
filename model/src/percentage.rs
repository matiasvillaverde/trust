//! Explicit percentage-point units: `1` is 1%, `100` is 100%.
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// A validated percentage expressed in points, never a fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PercentagePoints(Decimal);

/// Invalid or unrepresentable percentage.
#[derive(Debug)]
pub struct InvalidPercentage;

impl std::fmt::Display for InvalidPercentage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Percentage points must be between 0 and 100 inclusive; combined account reserves cannot exceed 100")
    }
}
impl std::error::Error for InvalidPercentage {}

impl PercentagePoints {
    /// Validate an external percentage-point value.
    pub fn new(points: Decimal) -> Result<Self, InvalidPercentage> {
        if !(Decimal::ZERO..=dec!(100)).contains(&points) {
            return Err(InvalidPercentage);
        }
        Ok(Self(points))
    }

    /// Convert percentage points to a fraction for multiplication.
    pub fn fraction(self) -> Result<Decimal, InvalidPercentage> {
        self.0.checked_div(dec!(100)).ok_or(InvalidPercentage)
    }

    /// Return the value in percentage points.
    pub fn value(self) -> Decimal {
        self.0
    }
}

/// Account reserves share one earnings base and cannot allocate more than 100%.
pub fn validate_account_percentages(
    taxes: Decimal,
    earnings: Decimal,
) -> Result<(), InvalidPercentage> {
    PercentagePoints::new(taxes)?;
    PercentagePoints::new(earnings)?;
    PercentagePoints::new(taxes.checked_add(earnings).ok_or(InvalidPercentage)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_never_depend_on_magnitude() {
        for (points, fraction) in [
            (dec!(0), dec!(0)),
            (dec!(0.25), dec!(0.0025)),
            (dec!(1), dec!(0.01)),
            (dec!(2), dec!(0.02)),
            (dec!(100), dec!(1)),
        ] {
            assert_eq!(
                PercentagePoints::new(points).unwrap().fraction().unwrap(),
                fraction
            );
        }
    }

    #[test]
    fn allocations_reject_negative_excessive_and_overflowing_values() {
        for (taxes, earnings) in [
            (dec!(-1), dec!(0)),
            (dec!(0), dec!(-1)),
            (dec!(101), dec!(0)),
            (dec!(80), dec!(80)),
            (Decimal::MAX, Decimal::MAX),
        ] {
            assert!(validate_account_percentages(taxes, earnings).is_err());
        }
        assert!(validate_account_percentages(dec!(25), dec!(75)).is_ok());
    }
}
