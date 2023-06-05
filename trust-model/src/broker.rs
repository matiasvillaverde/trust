use crate::Trade;
use chrono::NaiveDateTime;
use std::error::Error;
use uuid::Uuid;
use async_trait::async_trait;

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

#[async_trait]
pub trait Broker {
    async fn submit_order(trade: &Trade) -> Result<BrokerLog, Box<dyn Error>>;
}
