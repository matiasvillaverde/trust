use crate::currency::Currency;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use std::fmt::{self, Display, Formatter};
use uuid::Uuid;

/// Account entity
/// It represents a single account that want to be used to trade.
///
/// For example: Binance account, Kraken account, etc.
/// It doesn't need to be a real account. It can be a paper trading account.
#[derive(PartialEq, Debug, Clone)]
pub struct Account {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub name: String,
    pub description: String,
    pub environment: Environment,
    pub taxes_percentage: Decimal,
    pub earnings_percentage: Decimal,
}

/// AccountBalance entity (read-only)
/// This entity is used to display the account balance
/// This entity is a cached calculation of all the transactions that an account have.
/// This entity is read-only
/// It is not used to create or update an account
/// Each account has one AccountBalance per currency
///
/// WARNING: This entity can be out of sync with the actual account.
/// If your feature is important, consider recalculating the account balance.
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct AccountBalance {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub account_id: Uuid,

    /// Total balance of the account
    pub total_balance: Decimal,

    /// Total amount of money currently used in open trades
    pub total_in_trade: Decimal,

    /// Total amount of money available for trading
    pub total_available: Decimal,

    /// Total amount of money that it must be paid out to the tax authorities
    pub taxed: Decimal,

    /// Total amount of money that was earned and can be processed
    pub total_earnings: Decimal,

    /// The currency of the account
    pub currency: Currency,
}

// Implementations

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) {}",
            self.name, self.description, self.environment
        )
    }
}

impl Default for Account {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        Account {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: "".to_string(),
            description: "".to_string(),
            environment: Environment::Paper,
            taxes_percentage: Decimal::default(),
            earnings_percentage: Decimal::default(),
        }
    }
}

impl Default for AccountBalance {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        AccountBalance {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: Uuid::new_v4(),
            total_balance: Decimal::default(),
            total_in_trade: Decimal::default(),
            total_available: Decimal::default(),
            taxed: Decimal::default(),
            total_earnings: Decimal::default(),
            currency: Currency::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Environment {
    Paper,
    Live,
}

impl Environment {
    pub fn all() -> Vec<Environment> {
        vec![Environment::Paper, Environment::Live]
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            Environment::Paper => write!(f, "paper"),
            Environment::Live => write!(f, "live"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EnvironmentParseError;
impl std::str::FromStr for Environment {
    type Err = EnvironmentParseError;
    fn from_str(environment: &str) -> Result<Self, Self::Err> {
        match environment {
            "paper" => Ok(Environment::Paper),
            "live" => Ok(Environment::Live),
            _ => Err(EnvironmentParseError),
        }
    }
}
