use std::fmt;

/// Currency entity
#[derive(PartialEq, Debug, Hash, Eq)]
#[non_exhaustive] // This enum may be extended in the future
pub enum Currency {
    USD,
    EUR,
    BTC,
}

// Implementations

#[derive(PartialEq, Debug)]
pub struct CurrencyError;

impl std::str::FromStr for Currency {
    type Err = CurrencyError;

    fn from_str(currency: &str) -> Result<Self, Self::Err> {
        match currency {
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "BTC" => Ok(Currency::BTC),
            _ => Err(CurrencyError),
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Currency::USD => write!(f, "USD"),
            Currency::EUR => write!(f, "EUR"),
            Currency::BTC => write!(f, "BTC"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_currency_from_string() {
        let result = Currency::from_str("USD").expect("Failed to parse Currency from string");
        assert_eq!(result, Currency::USD);
        let result = Currency::from_str("EUR").expect("Failed to parse Currency from string");
        assert_eq!(result, Currency::EUR);
        let result = Currency::from_str("BTC").expect("Failed to parse Currency from string");
        assert_eq!(result, Currency::BTC);
    }
}
