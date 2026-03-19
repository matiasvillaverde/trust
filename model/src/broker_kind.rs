use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Supported broker integrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrokerKind {
    /// Alpaca broker integration.
    Alpaca,
    /// Interactive Brokers integration.
    Ibkr,
}

impl BrokerKind {
    /// Returns all supported broker kinds.
    pub fn all() -> Vec<Self> {
        vec![Self::Ibkr, Self::Alpaca]
    }

    /// Stable lowercase identifier used in persistence and CLI parsing.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Alpaca => "alpaca",
            Self::Ibkr => "ibkr",
        }
    }
}

impl Display for BrokerKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing an unsupported broker kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerKindParseError;

impl FromStr for BrokerKind {
    type Err = BrokerKindParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "alpaca" => Ok(Self::Alpaca),
            "ibkr" => Ok(Self::Ibkr),
            _ => Err(BrokerKindParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BrokerKind, BrokerKindParseError};
    use std::str::FromStr;

    #[test]
    fn broker_kind_roundtrips() {
        assert_eq!(BrokerKind::from_str("alpaca").unwrap(), BrokerKind::Alpaca);
        assert_eq!(BrokerKind::from_str("IBKR").unwrap(), BrokerKind::Ibkr);
        assert_eq!(BrokerKind::Alpaca.to_string(), "alpaca");
        assert_eq!(BrokerKind::Ibkr.to_string(), "ibkr");
    }

    #[test]
    fn broker_kind_rejects_unknown_values() {
        assert_eq!(
            BrokerKind::from_str("unknown").unwrap_err(),
            BrokerKindParseError
        );
    }
}
