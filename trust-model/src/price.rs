use crate::currency::Currency;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::prelude::*;
use std::fmt;
use uuid::Uuid;

/// Price entity
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Price {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub currency: Currency,
    pub amount: Decimal,
}

// Implementations
impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Price: {} {} {}", self.id, self.currency, self.amount)
    }
}

impl Price {
    pub fn new(currency: &Currency, amount: Decimal) -> Price {
        let now = Utc::now().naive_utc();
        Price {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: currency.clone(),
            amount,
        }
    }
}

impl Default for Price {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        Price {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD,
            amount: Decimal::new(0, 0),
        }
    }
}
