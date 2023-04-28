use crate::currency::Currency;
use crate::price::Price;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Account entity
#[derive(PartialEq, Debug)]
pub struct Account {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub name: String,
    pub description: String,
}

/// AccountOverview entity (read-only)
/// This entity is used to display the account overview
/// This entity is read-only
/// This entity is a cached calculation of all the transactions that an account have.
/// It is not used to create or update an account
/// Each account has one AccountOverview per currency
#[derive(PartialEq, Debug)]
pub struct AccountOverview {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub account_id: Uuid,

    /// Total balance of the account
    pub total_balance: Price,

    /// Total amount of money currently used in open trades
    pub total_in_trade: Price,

    /// Total amount of money available for trading
    pub total_available: Price,

    /// Total amount of money that it must be paid out to the tax authorities
    pub total_taxable: Price,

    /// The currency of the account
    pub currency: Currency,
}

// Implementations

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.description)
    }
}
