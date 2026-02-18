use crate::currency::Currency;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use uuid::Uuid;

/// Account type for specialized account purposes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccountType {
    /// Main trading account
    Primary,
    /// Personal earnings allocation  
    Earnings,
    /// Tax liability reserve
    TaxReserve,
    /// Additional trading capital
    Reinvestment,
}

/// Error type for account hierarchy validation failures
#[derive(Debug, Clone)]
pub enum AccountHierarchyError {
    /// Self-referencing account
    SelfReference,
    /// Circular dependency detected
    CircularDependency,
    /// Referenced parent account not found
    ParentNotFound,
    /// Parent account type cannot have children
    InvalidParentType,
}

/// Account entity
/// It represents a single account that want to be used to trade.
///
/// For example: Binance account, Kraken account, etc.
/// It doesn't need to be a real account. It can be a paper trading account.
#[derive(PartialEq, Debug, Clone)]
pub struct Account {
    /// Unique identifier for the account
    pub id: Uuid,

    /// When the account was created
    pub created_at: NaiveDateTime,
    /// When the account was last updated
    pub updated_at: NaiveDateTime,
    /// When the account was deleted, if applicable
    pub deleted_at: Option<NaiveDateTime>,

    /// Human-readable name for the account
    pub name: String,
    /// Description of the account's purpose
    pub description: String,
    /// Trading environment (paper or live)
    pub environment: Environment,
    /// Tax percentage to withhold from earnings
    pub taxes_percentage: Decimal,
    /// Percentage of earnings to set aside
    pub earnings_percentage: Decimal,
    /// Account type for specialization
    pub account_type: AccountType,
    /// Optional parent account for hierarchy
    pub parent_account_id: Option<Uuid>,
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
    /// Unique identifier for the account balance
    pub id: Uuid,

    /// When the balance record was created
    pub created_at: NaiveDateTime,
    /// When the balance record was last updated
    pub updated_at: NaiveDateTime,
    /// When the balance record was deleted, if applicable
    pub deleted_at: Option<NaiveDateTime>,

    /// ID of the account this balance belongs to
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
            account_type: AccountType::Primary,
            parent_account_id: None,
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

impl Account {
    /// Validates account hierarchy relationships
    pub fn validate_hierarchy(
        &self,
        all_accounts: &[&Account],
    ) -> Result<(), AccountHierarchyError> {
        // Check for self-reference
        if self.parent_account_id == Some(self.id) {
            return Err(AccountHierarchyError::SelfReference);
        }

        // If no parent, validation passes
        let Some(parent_id) = self.parent_account_id else {
            return Ok(());
        };

        // Find parent account
        let parent = all_accounts
            .iter()
            .find(|a| a.id == parent_id)
            .ok_or(AccountHierarchyError::ParentNotFound)?;

        // Check parent can have children
        if !parent.account_type.can_have_children() {
            return Err(AccountHierarchyError::InvalidParentType);
        }

        // Check for circular dependency
        let mut visited = HashSet::new();
        let mut current_id = Some(parent_id);

        while let Some(id) = current_id {
            if !visited.insert(id) {
                return Err(AccountHierarchyError::CircularDependency);
            }

            if id == self.id {
                return Err(AccountHierarchyError::CircularDependency);
            }

            current_id = all_accounts
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.parent_account_id);
        }

        Ok(())
    }
}

impl AccountType {
    /// Returns true if this account type can have child accounts
    pub fn can_have_children(&self) -> bool {
        matches!(self, AccountType::Primary)
    }

    /// Returns true if this account type requires a parent account
    pub fn requires_parent(&self) -> bool {
        !matches!(self, AccountType::Primary)
    }
}

impl Display for AccountType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            AccountType::Primary => write!(f, "primary"),
            AccountType::Earnings => write!(f, "earnings"),
            AccountType::TaxReserve => write!(f, "tax_reserve"),
            AccountType::Reinvestment => write!(f, "reinvestment"),
        }
    }
}

/// Trading environment type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Environment {
    /// Paper trading environment for testing
    Paper,
    /// Live trading environment with real money
    Live,
}

impl Environment {
    /// Returns all possible environment values
    pub fn all() -> Vec<Environment> {
        vec![Environment::Paper, Environment::Live]
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Environment::Paper => write!(f, "paper"),
            Environment::Live => write!(f, "live"),
        }
    }
}

/// Error when parsing environment from string fails
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_type_primary() {
        let account_type = AccountType::Primary;
        assert_eq!(account_type.to_string(), "primary");
        assert!(account_type.can_have_children());
        assert!(!account_type.requires_parent());
    }

    #[test]
    fn test_account_type_earnings() {
        let account_type = AccountType::Earnings;
        assert_eq!(account_type.to_string(), "earnings");
        assert!(!account_type.can_have_children());
        assert!(account_type.requires_parent());
    }

    #[test]
    fn test_account_type_tax_reserve() {
        let account_type = AccountType::TaxReserve;
        assert_eq!(account_type.to_string(), "tax_reserve");
        assert!(!account_type.can_have_children());
        assert!(account_type.requires_parent());
    }

    #[test]
    fn test_account_type_reinvestment() {
        let account_type = AccountType::Reinvestment;
        assert_eq!(account_type.to_string(), "reinvestment");
        assert!(!account_type.can_have_children());
        assert!(account_type.requires_parent());
    }

    #[test]
    fn test_account_hierarchy_validation_valid() {
        let parent = Account {
            id: Uuid::new_v4(),
            account_type: AccountType::Primary,
            parent_account_id: None,
            ..Account::default()
        };

        let child = Account {
            id: Uuid::new_v4(),
            account_type: AccountType::Earnings,
            parent_account_id: Some(parent.id),
            ..Account::default()
        };

        assert!(child.validate_hierarchy(&[&parent]).is_ok());
    }

    #[test]
    fn test_account_hierarchy_validation_circular_dependency() {
        let account1_id = Uuid::new_v4();
        let account2_id = Uuid::new_v4();

        let account1 = Account {
            id: account1_id,
            account_type: AccountType::Primary,
            parent_account_id: Some(account2_id),
            ..Account::default()
        };

        let account2 = Account {
            id: account2_id,
            account_type: AccountType::Primary,
            parent_account_id: Some(account1_id),
            ..Account::default()
        };

        assert!(account1.validate_hierarchy(&[&account2]).is_err());
    }

    #[test]
    fn test_account_hierarchy_validation_self_reference() {
        let account_id = Uuid::new_v4();
        let account = Account {
            id: account_id,
            account_type: AccountType::Primary,
            parent_account_id: Some(account_id),
            ..Account::default()
        };

        assert!(account.validate_hierarchy(&[]).is_err());
    }

    #[test]
    fn test_account_hierarchy_validation_missing_parent() {
        let missing_parent_id = Uuid::new_v4();
        let child = Account {
            id: Uuid::new_v4(),
            account_type: AccountType::Earnings,
            parent_account_id: Some(missing_parent_id),
            ..Account::default()
        };

        assert!(child.validate_hierarchy(&[]).is_err());
    }

    #[test]
    fn test_account_hierarchy_validation_invalid_parent_type() {
        let parent = Account {
            id: Uuid::new_v4(),
            account_type: AccountType::Earnings, // Child account type cannot be parent
            parent_account_id: None,
            ..Account::default()
        };

        let child = Account {
            id: Uuid::new_v4(),
            account_type: AccountType::TaxReserve,
            parent_account_id: Some(parent.id),
            ..Account::default()
        };

        assert!(child.validate_hierarchy(&[&parent]).is_err());
    }
}
