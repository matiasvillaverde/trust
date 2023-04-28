use crate::currency::Currency;
use chrono::NaiveDateTime;
use rust_decimal::prelude::*;
use std::fmt;
use uuid::Uuid;

/// Price entity
#[derive(PartialEq, Debug)]
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
        write!(
            f,
            "Price: {} {} {}",
            self.id,
            self.currency,
            self.amount.to_string()
        )
    }
}
