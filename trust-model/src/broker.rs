use crate::{Account, Order, Status, Trade};
use chrono::NaiveDateTime;
use std::error::Error;
use uuid::Uuid;

pub struct BrokerLog {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    pub trade_id: Uuid,
    pub log: String,
}

impl Default for BrokerLog {
    fn default() -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4(),
            log: String::new(),
        }
    }
}

pub struct OrderIds {
    pub stop: Uuid,
    pub entry: Uuid,
    pub target: Uuid,
}

pub trait Broker {
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>>;
    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn Error>>;
}
