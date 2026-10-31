//! Compatibility at the storage boundary only; the domain always uses points.
use model::percentage::{validate_account_percentages, PercentagePoints};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::{error::Error, str::FromStr};

pub(crate) const POINTS: &str = "percentage_points";

pub(crate) fn decode(
    taxes: &str,
    earnings: &str,
    units: &str,
) -> Result<(Decimal, Decimal), Box<dyn Error>> {
    let taxes = Decimal::from_str(taxes).map_err(|_| "Invalid taxes_percentage")?;
    let earnings = Decimal::from_str(earnings).map_err(|_| "Invalid earnings_percentage")?;
    let taxes = match units {
        POINTS => {
            validate_account_percentages(taxes, earnings)?;
            taxes
        }
        "legacy" => {
            // Old reports interpreted [0, 1] as fractions. Never apply this to new accounts.
            let points = if taxes <= Decimal::ONE {
                taxes
                    .checked_mul(dec!(100))
                    .ok_or("Legacy tax percentage overflow")?
            } else {
                taxes
            };
            PercentagePoints::new(points)?;
            PercentagePoints::new(earnings)?;
            points
        }
        _ => return Err("Unknown account tax percentage units".into()),
    };
    Ok((taxes, earnings))
}
