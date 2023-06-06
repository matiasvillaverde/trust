use crate::Trade;
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

pub trait Broker {
    fn submit_trade(&self, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>>;
}
