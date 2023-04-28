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

impl Currency {
    pub fn from_str(currency: &str) -> Currency {
        match currency {
            "USD" => Currency::USD,
            "EUR" => Currency::EUR,
            "BTC" => Currency::BTC,
            _ => panic!("Unknown Currency: {}", currency),
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

    #[test]
    fn test_currency_from_string() {
        let result = Currency::from_str("USD");
        assert_eq!(result, Currency::USD);
        let result = Currency::from_str("EUR");
        assert_eq!(result, Currency::EUR);
        let result = Currency::from_str("BTC");
        assert_eq!(result, Currency::BTC);
    }
}
